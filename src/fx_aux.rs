//! Four ordinary FX auxes; the provider owns algorithms and parameter ranges.
use serde_json::{Value, json};
pub const CONTRACT: &str = "GP-FX-aux";
#[derive(Clone, Debug, Default)]
pub struct State {
    pub snapshot: Option<Value>,
    pub receipt: Option<u64>,
    pub unknown: bool,
}
impl State {
    pub fn fresh(&self, now: u64) -> bool {
        !self.unknown && self.receipt.is_some_and(|t| now.saturating_sub(t) <= 1000)
    }
    pub fn validate(&self, body: &Value, now: u64) -> Result<(), String> {
        validate_body(body)?;
        if !self.fresh(now) {
            return Err("FX aux fresh probe required".into());
        }
        let s = self.snapshot.as_ref().ok_or("FX aux unavailable")?;
        let i = body["bus"].as_u64().ok_or("FX aux bus")? as usize;
        let b = &s["buses"][i];
        if s["available"] != true
            || !s["pending"].is_null()
            || s["library_sha256"] != body["library_sha256"]
            || s["map_revision"] != body["map_revision"]
            || s["owner_binding"] != body["owner_binding"]
            || b["generation"] != body["expected_generation"]
            || (body["panic"] != true && b["generation"] != b["applied_generation"])
        {
            return Err("FX aux stale or transitioning".into());
        }
        Ok(())
    }
}
pub fn validate_body(b: &Value) -> Result<(), String> {
    let mut keys = vec![
        "bus",
        "map_revision",
        "library_sha256",
        "expected_generation",
        "configuration_json",
        "panic",
    ];
    if b.get("owner_binding").is_some() {
        keys.push("owner_binding");
        check_binding(&b["owner_binding"])?;
    }
    crate::provider::keys(b, &keys)?;
    if b["bus"].as_u64().is_none_or(|v| v >= 4)
        || b["panic"].as_bool().is_none()
        || b["panic"].as_bool() == Some(b["configuration_json"].is_string())
        || b["library_sha256"].as_str().is_none_or(|s| {
            s.len() != 64
                || !s
                    .bytes()
                    .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        })
    {
        return Err("FX aux operation shape".into());
    }
    for k in ["map_revision", "expected_generation"] {
        crate::provider::counter(b[k].as_str().ok_or("FX counter")?)?;
    }
    if let Some(s) = b["configuration_json"].as_str() {
        configuration(s)?;
    }
    Ok(())
}
pub fn configuration(text: &str) -> Result<Value, String> {
    let v = crate::master_eq::parse_owner_bounded(text, 32768)?;
    crate::provider::keys(&v, &["settings", "sends", "tempo"])?;
    if !v["tempo"].is_null() {
        crate::provider::keys(&v["tempo"], &["bpm", "denominator", "modifier"])?;
        let t = &v["tempo"];
        if t["bpm"]
            .as_f64()
            .is_none_or(|n| !(30.0..=300.0).contains(&n))
            || t["denominator"]
                .as_u64()
                .is_none_or(|n| ![1, 2, 4, 8, 16, 32, 64].contains(&n))
            || t["modifier"].as_u64().is_none_or(|n| n > 2)
        {
            return Err("FX tempo bounds".into());
        }
    }
    let s = &v["settings"];
    crate::provider::keys(
        s,
        &[
            "version",
            "size",
            "algorithm",
            "bypass",
            "time_ms",
            "amount",
            "damping",
            "rate_hz",
            "gain",
            "seed",
            "reserved",
        ],
    )?;
    if s["version"] != 2
        || s["size"] != 64
        || s["reserved"] != 0
        || s["algorithm"].as_u64().is_none_or(|n| n == 0 || n >= 32)
        || s["bypass"].as_u64().is_none_or(|n| n > 1)
        || s["seed"].as_u64().is_none_or(|n| n > u32::MAX as u64)
        || ["time_ms", "amount", "damping", "rate_hz", "gain"]
            .iter()
            .any(|k| s[k].as_f64().is_none_or(|x| !x.is_finite()))
        || v["sends"].as_array().is_none_or(|a| {
            a.is_empty()
                || a.len() > 4096
                || a.iter()
                    .any(|x| x.as_f64().is_none_or(|n| !(0.0..=1.0).contains(&n)))
        })
    {
        return Err("FX aux configuration".into());
    }
    Ok(v)
}
pub fn snapshot(v: Value) -> Result<Value, String> {
    let mut keys = vec![
        "contract",
        "version",
        "show_id",
        "module",
        "epoch",
        "writer",
        "lease",
        "request_id",
        "state",
        "reason",
        "effective_frame",
        "revision",
        "available",
    ];
    if v["available"] == true {
        keys.extend([
            "pending",
            "library_sha256",
            "map_revision",
            "frame",
            "inputs",
            "buses",
            "palette_json",
            "capabilities_json",
        ]);
    }
    if v.get("owner_binding").is_some() {
        keys.push("owner_binding");
        check_binding(&v["owner_binding"])?;
    }
    crate::provider::keys(&v, &keys)?;
    if v["module"] != "audio"
        || ["writer", "lease", "request_id", "reason", "effective_frame"]
            .iter()
            .any(|k| !v[k].is_null())
    {
        return Err("FX aux read context".into());
    }
    if v["contract"] != CONTRACT
        || v["version"] != 1
        || v["state"] != "snapshot"
        || v["available"].as_bool().is_none()
    {
        return Err("FX aux snapshot".into());
    }
    for k in ["epoch", "revision"] {
        crate::provider::counter(v[k].as_str().ok_or("FX aux identity")?)?;
    }
    if v["available"] == true {
        if v["buses"].as_array().is_none_or(|b| b.len() != 4) {
            return Err("FX aux bus count".into());
        }
        crate::provider::counter(v["frame"].as_str().ok_or("FX frame")?)?;
        crate::provider::counter(v["map_revision"].as_str().ok_or("FX map")?)?;
        let inputs = v["inputs"].as_array().ok_or("FX input map")?;
        if v["library_sha256"].as_str().is_none_or(|s| {
            s.len() != 64
                || !s
                    .bytes()
                    .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        }) {
            return Err("FX aux library identity".into());
        }
        if !v["pending"].is_null() && v["pending"].as_u64().is_none_or(|n| n >= 4) {
            return Err("FX pending bus".into());
        }
        let mut ids = std::collections::BTreeSet::new();
        for input in inputs {
            crate::provider::keys(input, &["id", "capture_slot", "physical_port"])?;
            let id = input["id"].as_str().ok_or("FX input identity")?;
            if id.is_empty()
                || !ids.insert(id)
                || input["capture_slot"].as_u64().is_none()
                || input["physical_port"].as_str().is_none()
            {
                return Err("FX input map identity".into());
            }
        }

        for (i, b) in v["buses"].as_array().unwrap().iter().enumerate() {
            crate::provider::keys(
                b,
                &[
                    "id",
                    "generation",
                    "applied_generation",
                    "configuration_json",
                    "owner_json",
                ],
            )?;

            if b["id"] != format!("fx-{}", i + 1) {
                return Err("FX aux identity order".into());
            }
            for k in ["generation", "applied_generation"] {
                crate::provider::counter(b[k].as_str().ok_or("FX generation")?)?;
            }
            let c = configuration(b["configuration_json"].as_str().ok_or("FX configuration")?)?;
            if c["sends"].as_array().unwrap().len() != inputs.len() {
                return Err("FX input shape".into());
            }
            let o = crate::master_eq::parse_owner_bounded(
                b["owner_json"].as_str().ok_or("FX status")?,
                4096,
            )?;
            if o["version"] != 2 || o["size"] != 200 || o["rate"] != 48000 || o["max_block"] != 48 {
                return Err("FX owner ABI".into());
            }
        }
        let p = crate::master_eq::parse_owner_bounded(
            v["palette_json"].as_str().ok_or("FX palette")?,
            32768,
        )?;
        let palette = p.as_array().ok_or("FX palette shape")?;
        if palette.is_empty() || palette.len() > 31 {
            return Err("FX palette shape".into());
        }
        let caps = crate::master_eq::parse_owner_bounded(
            v["capabilities_json"].as_str().ok_or("FX capabilities")?,
            4096,
        )?;
        if caps["version"] != 2
            || caps["size"] != 48
            || caps["sample_bits"] != 64
            || caps["pending_capacity"] != 1
            || caps["retired_capacity"] != 1
        {
            return Err("FX capability ABI".into());
        }
        let mask = caps["algorithm_mask"].as_u64().ok_or("FX algorithm mask")?;
        let mut algorithms = std::collections::BTreeSet::new();
        for p in palette {
            crate::provider::keys(p, &["algorithm", "defaults", "parameters"])?;
            let id = p["algorithm"].as_u64().ok_or("FX algorithm")?;
            if !(1..32).contains(&id)
                || mask & (1 << id) == 0
                || !algorithms.insert(id)
                || p["defaults"]["algorithm"] != id
            {
                return Err("FX palette identity".into());
            }
            configuration(
                &json!({"settings":p["defaults"],"sends":[0.],"tempo":null}).to_string(),
            )?;
            let parameters = p["parameters"].as_array().ok_or("FX parameter array")?;
            if parameters.len() != 6 {
                return Err("FX parameter count".into());
            }
            for (i, d) in parameters.iter().enumerate() {
                crate::provider::keys(
                    d,
                    &[
                        "version",
                        "size",
                        "algorithm",
                        "parameter",
                        "available",
                        "unit",
                        "minimum",
                        "maximum",
                        "default_value",
                    ],
                )?;
                let min = d["minimum"].as_f64().ok_or("FX minimum")?;
                let max = d["maximum"].as_f64().ok_or("FX maximum")?;
                let default = d["default_value"].as_f64().ok_or("FX default")?;
                if d["version"] != 2
                    || d["size"] != 48
                    || d["algorithm"] != id
                    || d["parameter"] != (i + 1) as u64
                    || d["available"].as_u64().is_none_or(|n| n > 1)
                    || d["unit"].as_u64().is_none_or(|n| n > 3)
                    || !min.is_finite()
                    || !max.is_finite()
                    || !default.is_finite()
                    || min > max
                    || default < min
                    || default > max
                {
                    return Err("FX descriptor identity/bounds".into());
                }
            }
        }
    }
    Ok(v)
}
#[derive(Clone, Debug)]
pub enum Operation {
    Probe,
    Review(Value),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Open,
    Probe,
    Scope,
    Bus(i8),
    Algorithm(i8),
    ChooseAlgorithm(u32),
    Field(i8),
    Adjust(i8),
    Bypass,
    Panic,
}
#[derive(Default)]
pub struct Editor {
    pub open: bool,
    pub bus: usize,
    pub field: usize,
}
pub fn mutation(s: &Value, bus: usize, config: Option<Value>) -> Result<Value, String> {
    let mut b = json!({"bus":bus,"map_revision":s["map_revision"],"library_sha256":s["library_sha256"],
        "expected_generation":s["buses"][bus]["generation"],"panic":config.is_none(),
        "configuration_json":config.map(|c|c.to_string())});
    if let Some(binding) = s.get("owner_binding") {
        b["owner_binding"] = binding.clone();
    }
    validate_body(&b)?;
    Ok(b)
}
pub fn name(id: u64) -> &'static str {
    match id {
        1 => "Digital delay",
        2 => "Room",
        3 => "Chorus",
        4 => "Small room",
        5 => "Chamber",
        6 => "Plate",
        7 => "Hall",
        8 => "Tape echo",
        9 => "Warm exciter",
        10 => "Bright exciter",
        _ => "Provider effect",
    }
}

pub fn field_labels(algorithm: u64) -> [&'static str; 9] {
    let mut names = [
        "Channel send",
        "Delay time (ms)",
        "Feedback",
        "Damping / darkness",
        "Rate (Hz)",
        "Return level",
        "BPM",
        "Note division",
        "Straight / dotted / triplet",
    ];
    match algorithm {
        2 | 4..=7 => {
            names[1] = "Predelay (ms)";
            names[2] = "Decay";
            names[4] = "Rate (unavailable)";
        }
        3 => {
            names[1] = "Base delay (ms)";
            names[2] = "Depth (ms)";
            names[3] = "Damping (unavailable)";
        }
        9 | 10 => {
            names[1] = "Time (unavailable)";
            names[2] = "Drive";
            names[3] = "Tone / brightness";
            names[4] = "Tune (Hz)";
        }
        _ => {
            names[4] = "Rate (unavailable)";
        }
    }
    names
}
pub fn sound_lines(c: &Value) -> Vec<String> {
    let s = &c["settings"];
    let algorithm = s["algorithm"].as_u64().unwrap_or(0);
    let labels = field_labels(algorithm);
    let mut lines = Vec::new();
    for (i, key) in ["time_ms", "amount", "damping", "rate_hz"]
        .iter()
        .enumerate()
    {
        if !labels[i + 1].contains("unavailable") {
            lines.push(format!("{}: {}", labels[i + 1], s[key]));
        }
    }
    if matches!(algorithm, 1 | 8) {
        let t = &c["tempo"];
        lines.push(if t.is_null() {
            "Timing: free milliseconds".into()
        } else {
            format!(
                "Timing: {} BPM / 1/{} {}",
                t["bpm"],
                t["denominator"],
                ["straight", "dotted", "triplet"]
                    [t["modifier"].as_u64().unwrap_or(0).min(2) as usize]
            )
        });
    }
    lines
}
pub fn review_lines(body: &Value, snapshot: Option<&Value>) -> String {
    let bus = body["bus"].as_u64().unwrap_or(0) as usize;
    if body["panic"] == true {
        return format!(
            "FX {} — CLEAR TAIL\nClear this effect's history and cancel any unswapped replacement.\nNew input can excite the effect again; this is not a persistent mute.\nConfirm to apply, or Cancel to keep the current sound.",
            bus + 1
        );
    }
    let Ok(c) = configuration(body["configuration_json"].as_str().unwrap_or("")) else {
        return "Invalid FX configuration".into();
    };
    let mut lines = vec![
        format!(
            "FX {} — {}",
            bus + 1,
            name(c["settings"]["algorithm"].as_u64().unwrap_or(0))
        ),
        format!(
            "Return {:.0}% / {}",
            c["settings"]["gain"].as_f64().unwrap_or(0.) * 100.,
            if c["settings"]["bypass"] == 1 {
                "Bypassed: tails allowed"
            } else {
                "Active"
            }
        ),
    ];
    lines.extend(sound_lines(&c));
    lines.push("Channel sends (post-fader and post-mute):".into());
    if let Some(sends) = c["sends"].as_array() {
        for (i, send) in sends.iter().enumerate() {
            let id = snapshot
                .and_then(|s| s["inputs"][i]["id"].as_str())
                .map(str::to_owned)
                .unwrap_or_else(|| format!("Input {}", i + 1));
            lines.push(format!(
                "{}: {:.2}%",
                id,
                send.as_f64().unwrap_or(0.) * 100.
            ));
        }
    }
    lines.push(
        "Effect changes fade out/swap/in; bypass alone preserves the tail. Confirm or Cancel."
            .into(),
    );
    lines.join("\n")
}

fn check_binding(v: &Value) -> Result<(), String> {
    if v.as_str().is_none_or(|s| {
        s.len() != 64
            || !s
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    }) {
        Err("FX aux owner binding".into())
    } else {
        Ok(())
    }
}
