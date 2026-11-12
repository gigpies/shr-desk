//! Shared bounded durable private scene storage; callers validate their schema.
use serde_json::Value;
use std::{
    fs,
    io::{Read, Write},
    path::Path,
};
fn directory(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let m = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !path.is_absolute()
        || !m.is_dir()
        || m.permissions().mode() & 0o777 != 0o700
        || m.uid() != fs::metadata("/proc/self").map_err(|e| e.to_string())?.uid()
    {
        return Err("Scene directory must be an owned absolute 0700 directory".into());
    }
    Ok(())
}
fn name(prefix: &str, slot: usize) -> Result<String, String> {
    if slot >= 16 {
        return Err("Saved slot".into());
    }
    Ok(format!("{prefix}-{:02}.json", slot + 1))
}
pub fn load(path: &Path, prefix: &str, slot: usize, max: usize) -> Result<Vec<u8>, String> {
    directory(path)?;
    let path = path.join(name(prefix, slot)?);
    if !fs::symlink_metadata(&path)
        .map_err(|e| e.to_string())?
        .is_file()
    {
        return Err("Saved file must be regular".into());
    }
    let mut bytes = Vec::new();
    let file = {
        use std::os::unix::fs::OpenOptionsExt;
        fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(path)
    }
    .map_err(|e| e.to_string())?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("Saved descriptor must be regular".into());
    }
    file.take(max as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > max {
        return Err("saved file capacity".into());
    }
    Ok(bytes)
}
pub fn save(
    path: &Path,
    prefix: &str,
    slot: usize,
    value: &Value,
    max: usize,
) -> Result<(), String> {
    use std::os::unix::fs::OpenOptionsExt;
    directory(path)?;
    let bytes = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    if bytes.len() > max {
        return Err("Saved capacity".into());
    }
    let name = name(prefix, slot)?;
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
    let (p, mut f) = temp.ok_or("Scene temporary capacity")?;
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
        format!("Scene save failed; replacement completed={replaced}, durability unconfirmed: {e}")
    })
}
