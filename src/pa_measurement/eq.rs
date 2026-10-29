//! EQ review admission only. All fitting and candidate construction belong to PA.
use serde_json::Value;
pub fn validate(v: &Value) -> Result<(), String> {
    crate::provider::keys(
        v,
        &[
            "contract",
            "version",
            "status",
            "reason",
            "basis_capture",
            "basis_configuration",
            "basis_configuration_revision",
            "measurement_ids",
            "output_index",
            "target",
            "calibration",
            "bands",
            "before_rms_db",
            "predicted_rms_db",
            "required_headroom_db",
            "verification_required",
        ],
    )?;
    if !v["calibration"].is_null() {
        let c = &v["calibration"];
        crate::provider::keys(c, &["provenance", "points"])?;
        if !c["provenance"].as_str().is_some_and(super::identifier) {
            return Err("EQ calibration provenance".into());
        }
        let points = c["points"].as_array().ok_or("EQ calibration points")?;
        if !(2..=256).contains(&points.len()) {
            return Err("EQ calibration capacity".into());
        }
        let mut previous = 0.;
        for p in points {
            let p = p.as_array().ok_or("EQ calibration pair")?;
            if p.len() != 2 {
                return Err("EQ calibration pair".into());
            }
            let hz = p[0].as_f64().ok_or("EQ calibration Hz")?;
            let db = p[1].as_f64().ok_or("EQ calibration dB")?;
            if hz <= previous || !(10.0..=24000.0).contains(&hz) || !(-20.0..=20.0).contains(&db) {
                return Err("EQ calibration bounds".into());
            }
            previous = hz;
        }
    }
    let capture: super::Capture =
        serde_json::from_value(v["basis_capture"].clone()).map_err(|e| e.to_string())?;
    capture.validate()?;
    if v["contract"] != "C-PA-EQ-PROPOSAL"
        || v["version"] != 1
        || v["verification_required"] != true
        || v["basis_configuration_revision"] != capture.configuration_revision
        || v["output_index"].as_u64() != Some(capture.output_index as u64)
        || !matches!(
            v["target"].as_str(),
            Some("flat" | "bass_shaped" | "damped_high_frequency")
        )
        || !matches!(
            v["status"].as_str(),
            Some("proposed" | "no_change" | "refused")
        )
    {
        return Err("EQ proposal identity".into());
    }
    let ids = v["measurement_ids"].as_array().ok_or("EQ measurements")?;
    if ids.is_empty()
        || ids.len() > 4
        || ids
            .iter()
            .any(|id| !id.as_str().is_some_and(super::identifier))
    {
        return Err("EQ measurement bounds".into());
    }
    for key in ["before_rms_db", "predicted_rms_db", "required_headroom_db"] {
        if !v[key].as_f64().is_some_and(|v| v.is_finite() && v >= 0.) {
            return Err("EQ metric".into());
        }
    }
    if !v["reason"].as_str().is_some_and(|s| s.len() <= 256) {
        return Err("EQ reason".into());
    }
    let bands = v["bands"].as_array().ok_or("EQ bands")?;
    if bands.len() > 8
        || (v["status"] != "proposed" && !bands.is_empty())
        || (v["status"] == "proposed" && bands.is_empty())
    {
        return Err("EQ proposal state".into());
    }
    let mut boost = 0.;
    let mut cut = 0.;
    for b in bands {
        crate::provider::keys(b, &["kind", "slope", "hz", "q", "db"])?;
        let hz = b["hz"].as_f64().ok_or("EQ frequency")?;
        let q = b["q"].as_f64().ok_or("EQ Q")?;
        let db = b["db"].as_f64().ok_or("EQ gain")?;
        if b["kind"] != "bell"
            || b["slope"].as_f64() != Some(1.)
            || !(50.0..=12000.0).contains(&hz)
            || !(0.5..=2.0).contains(&q)
            || !(-6.0..=3.0).contains(&db)
        {
            return Err("EQ bounds".into());
        }
        boost += db.max(0.);
        cut += db.min(0.);
    }
    if boost > 3. || cut < -12. || v["required_headroom_db"].as_f64() != Some(boost) {
        return Err("EQ headroom/bounds".into());
    }
    Ok(())
}
pub fn candidate(v: &Value, base: &Value, actual: &Value) -> Result<(), String> {
    validate(v)?;
    if &v["basis_configuration"] != base || v["status"] != "proposed" {
        return Err("EQ stale/unchanged candidate".into());
    }
    let mut restored = actual.clone();
    let index = v["output_index"].as_u64().ok_or("EQ output")? as usize;
    let old = base["outputs"]
        .as_array()
        .and_then(|a| a.get(index))
        .ok_or("EQ output")?;
    let new = restored["outputs"]
        .as_array_mut()
        .and_then(|a| a.get_mut(index))
        .ok_or("EQ output")?;
    if old["source"].is_null() || old["processing"]["eq_enabled"] != true {
        return Err("EQ bank unavailable".into());
    }
    let slots = old["processing"]["eq"].as_array().ok_or("EQ slots")?;
    let after = new["processing"]["eq"].as_array().ok_or("EQ slots")?;
    if slots.len() != 8 || after.len() != 8 {
        return Err("EQ slot count".into());
    }
    let mut bands = v["bands"].as_array().unwrap().iter();
    for (a, b) in slots.iter().zip(after) {
        let expected = if a["db"].as_f64() == Some(0.) {
            bands.next().unwrap_or(a)
        } else {
            a
        };
        if b != expected {
            return Err("EQ candidate mismatch".into());
        }
    }
    if bands.next().is_some() {
        return Err("EQ capacity".into());
    }
    new["processing"]["eq"] = old["processing"]["eq"].clone();
    if &restored != base {
        return Err("EQ changed protected configuration".into());
    }
    Ok(())
}
pub fn lines(v: &Value) -> Result<Vec<String>, String> {
    validate(v)?;
    let mut lines = vec![
        format!("Offline EQ {}: {}", v["status"], v["reason"]),
        format!(
            "Target {} / output {} / measurements {}",
            v["target"], v["output_index"], v["measurement_ids"]
        ),
        format!(
            "Predicted RMS error {} -> {} dB; required extra headroom {} dB",
            v["before_rms_db"], v["predicted_rms_db"], v["required_headroom_db"]
        ),
    ];
    lines.push(
        match v["target"].as_str().unwrap() {
            "flat" => "Target: 0 dB relative to reference",
            "bass_shaped" => "Target: +3 dB below 100 Hz, log taper to 0 dB at 200 Hz",
            _ => "Target: -1.5 dB/octave above 2 kHz, limited to -6 dB",
        }
        .into(),
    );
    lines.push(if v["calibration"].is_null() {
        "Microphone correction: none; absolute SPL uncalibrated".into()
    } else {
        format!(
            "Magnitude correction provenance {}; absolute SPL uncalibrated",
            v["calibration"]["provenance"]
        )
    });
    for b in v["bands"].as_array().unwrap() {
        lines.push(format!(
            "Bell {} Hz / {} dB / Q {}",
            b["hz"], b["db"], b["q"]
        ));
    }
    lines.push("Prediction only. Muted review/commit; separate rearm and fresh measurement. Acoustics unverified.".into());
    Ok(lines)
}
