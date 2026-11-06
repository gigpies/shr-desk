//! Portable saved intent consumer. Files contain no live authority or arm state.
use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Write},
    path::Path,
};
pub const MAX_BYTES: usize = 256 * 1024;
pub fn decode(bytes: &[u8]) -> Result<Value, String> {
    let text = std::str::from_utf8(bytes).map_err(|e| e.to_string())?;
    let v = crate::master_eq::parse_owner_bounded(text, MAX_BYTES)?;
    validate(&v)?;
    Ok(v)
}
pub fn validate(v: &Value) -> Result<(), String> {
    crate::provider::keys(
        v,
        &[
            "format",
            "version",
            "show_id",
            "library_sha256",
            "inputs",
            "monitors",
            "configurations",
        ],
    )?;
    if v["format"] != "gigpies-fx-intent" || v["version"] != 1 {
        return Err("Unsupported FX intent; legacy core/composed files contain no saved FX".into());
    }
    let inputs = v["inputs"].as_array().ok_or("FX saved inputs")?;
    let monitors = v["monitors"].as_array().ok_or("FX saved monitors")?;
    if inputs.is_empty()
        || inputs.len() > 4096
        || monitors.len() > 4096
        || v["configurations"].as_array().is_none_or(|a| a.len() != 4)
        || v["show_id"].as_str().is_none_or(|s| {
            s.len() != 36
                || !s.bytes().enumerate().all(|(i, b)| {
                    if [8, 13, 18, 23].contains(&i) {
                        b == b'-'
                    } else {
                        b.is_ascii_digit() || (b'a'..=b'f').contains(&b)
                    }
                })
        })
        || v["library_sha256"].as_str().is_none_or(|s| {
            s.len() != 64
                || !s
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        })
    {
        return Err("FX saved identity/capacity".into());
    }
    let mut ids = std::collections::BTreeSet::new();
    let mut slots = std::collections::BTreeSet::new();
    let mut ports = std::collections::BTreeSet::new();
    for p in inputs {
        crate::provider::keys(p, &["id", "capture_slot", "physical_port"])?;
        let id = p["id"].as_str().ok_or("input ID")?;
        let port = p["physical_port"].as_str().ok_or("input port")?;
        let slot = p["capture_slot"].as_u64().ok_or("input slot")?;
        if !crate::provider::id(id)
            || port.is_empty()
            || port.len() > 128
            || slot > u16::MAX as u64
            || !ids.insert(id)
            || !slots.insert(slot)
            || !ports.insert(port)
        {
            return Err("Ambiguous saved input identity".into());
        }
    }
    for (i, id) in monitors.iter().enumerate() {
        if *id != format!("monitor-{}", i + 1) {
            return Err("Saved monitor identity".into());
        }
    }
    for text in v["configurations"].as_array().unwrap() {
        let c = crate::fx_aux::configuration(text.as_str().ok_or("saved FX configuration")?)?;
        crate::provider::keys(&c, &["settings", "sends", "returns", "tempo"])?;
        crate::fx_aux::validate_returns(&c["returns"], Some(monitors.len()))?;
        if c["sends"].as_array().unwrap().len() != inputs.len() {
            return Err("Saved send dimensions".into());
        }
    }
    Ok(())
}
pub fn compatible(saved: &Value, snapshot: &Value) -> Result<(), String> {
    validate(saved)?;
    if snapshot["restore_version"] != 1 {
        return Err("Provider does not advertise reviewed bundle restore".into());
    }
    for k in ["show_id", "library_sha256", "inputs", "monitors"] {
        if saved[k] != snapshot[k] {
            return Err(format!(
                "Saved FX incompatible {k}; automatic remapping refused"
            ));
        }
    }
    Ok(())
}
pub fn capture(snapshot: &Value) -> Result<Value, String> {
    if snapshot["available"] != true || !snapshot["pending"].is_null() {
        return Err("Fresh settled authoritative FX required".into());
    }
    let mut configs = Vec::with_capacity(4);
    for b in snapshot["buses"].as_array().ok_or("FX buses")? {
        let c = crate::fx_aux::configuration(b["configuration_json"].as_str().ok_or("FX config")?)?;
        let o = crate::master_eq::parse_owner_bounded(
            b["owner_json"].as_str().ok_or("FX owner")?,
            4096,
        )?;
        if b["generation"] != b["applied_generation"]
            || o["transition"] != 0
            || o["fault"] != 0
            || !matches!(o["request_state"].as_u64(), Some(0 | 3 | 4 | 5))
            || o["current"] != c["settings"]
            || o["target"] != c["settings"]
        {
            return Err("All four FX owners must be settled before saving".into());
        }
        configs.push(b["configuration_json"].clone());
    }
    let saved = json!({"format":"gigpies-fx-intent","version":1,"show_id":snapshot["show_id"],"library_sha256":snapshot["library_sha256"],"inputs":snapshot["inputs"],"monitors":snapshot["monitors"],"configurations":configs});
    validate(&saved)?;
    Ok(saved)
}
pub fn mutation(saved: &Value, snapshot: &Value) -> Result<Value, String> {
    compatible(saved, snapshot)?;
    let config = crate::fx_aux::configuration(saved["configurations"][0].as_str().unwrap())?;
    let mut body = crate::fx_aux::mutation(snapshot, 0, Some(config))?;
    body["configuration_json"] = saved["configurations"][0].clone();
    body["restore"] = json!({"version":1,"saved":saved,"expected_generations":snapshot["buses"].as_array().unwrap().iter().map(|b|b["generation"].clone()).collect::<Vec<_>>()});
    crate::fx_aux::validate_body(&body)?;
    Ok(body)
}
pub fn edits(body: &Value) -> Result<Vec<Value>, String> {
    let Some(r) = body.get("restore") else {
        return Ok(vec![body.clone()]);
    };
    crate::provider::keys(r, &["version", "saved", "expected_generations"])?;
    validate(&r["saved"])?;
    if r["version"] != 1
        || r["expected_generations"]
            .as_array()
            .is_none_or(|a| a.len() != 4)
        || body["bus"] != 0
        || body["panic"] != false
        || body["library_sha256"] != r["saved"]["library_sha256"]
        || body["configuration_json"] != r["saved"]["configurations"][0]
        || body["expected_generation"] != r["expected_generations"][0]
    {
        return Err("FX restore bundle shape".into());
    }
    (0..4)
        .map(|i| {
            let mut b = body.clone();
            b.as_object_mut().unwrap().remove("restore");
            b["bus"] = json!(i);
            b["configuration_json"] = r["saved"]["configurations"][i].clone();
            b["expected_generation"] = r["expected_generations"][i].clone();
            crate::fx_aux::validate_body(&b)?;
            Ok(b)
        })
        .collect()
}
fn directory(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let m = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !path.is_absolute()
        || !m.is_dir()
        || m.permissions().mode() & 0o777 != 0o700
        || m.uid() != fs::metadata("/proc/self").map_err(|e| e.to_string())?.uid()
    {
        return Err("FX show directory must be an owned absolute 0700 directory".into());
    }
    Ok(())
}
fn name(slot: usize) -> Result<String, String> {
    if slot >= 16 {
        return Err("FX saved slot".into());
    }
    Ok(format!("fx-scene-{:02}.json", slot + 1))
}
pub fn load(path: &Path, slot: usize) -> Result<Value, String> {
    directory(path)?;
    let path = path.join(name(slot)?);
    if !fs::symlink_metadata(&path)
        .map_err(|e| e.to_string())?
        .is_file()
    {
        return Err("FX saved file must be regular".into());
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    decode(&bytes)
}
pub fn save(path: &Path, slot: usize, value: &Value) -> Result<(), String> {
    use std::os::unix::fs::OpenOptionsExt;
    directory(path)?;
    validate(value)?;
    let bytes = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    if bytes.len() > MAX_BYTES {
        return Err("FX saved capacity".into());
    }
    let name = name(slot)?;
    let mut temp = None;
    for n in 0..64 {
        let p = path.join(format!(".{name}.{}.{n}.tmp", std::process::id()));
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&p)
        {
            Ok(f) => {
                temp = Some((p, f));
                break;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
            Err(e) => return Err(e.to_string()),
        }
    }
    let (p, mut f) = temp.ok_or("FX temporary capacity")?;
    let mut replaced = false;
    let result = (|| -> std::io::Result<()> {
        f.write_all(&bytes)?;
        f.flush()?;
        f.sync_all()?;
        fs::rename(&p, path.join(name))?;
        replaced = true;
        fs::File::open(path)?.sync_all()
    })();
    if result.is_err() {
        let _ = fs::remove_file(&p);
    }
    result.map_err(|e| {
        format!("FX save failed; replacement completed={replaced}, durability unconfirmed: {e}")
    })
}
