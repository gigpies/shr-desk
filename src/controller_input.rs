//! Production controller acquisition: strict learned-profile files plus the
//! shared read-only MIDI reader feeding `Frontend::inject_controller_flow`.
//!
//! Nothing here writes to a device, invents a preset or grants authority. The
//! profile must be learned from the actual controller (see `learn`), the endpoint
//! must equal the role-bound controller identity, and every acquisition loss
//! fences held input until the controller is attached again.
use crate::{
    controller_flow::{Profile, Rotary},
    frontend::Frontend,
    midi::Relative,
};
use serde::Deserialize;
use shr_instrument_panel::midi::{self, Event, Reader};
use std::{path::Path, sync::mpsc::Receiver, time::Instant};

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
enum EncodingFile {
    TwosComplement,
    BinaryOffset,
    SignedBit,
}
impl From<EncodingFile> for Relative {
    fn from(e: EncodingFile) -> Self {
        match e {
            EncodingFile::TwosComplement => Self::TwosComplement,
            EncodingFile::BinaryOffset => Self::BinaryOffset,
            EncodingFile::SignedBit => Self::SignedBit,
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RotaryFile {
    cc: u8,
    encoding: EncodingFile,
    #[serde(default)]
    alternate_cc: Option<u8>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProfileFile {
    contract: String,
    version: u32,
    /// Stable `/dev/snd/by-id` name, which is also the role-bound identity.
    endpoint: String,
    rotary_channel: u8,
    rotaries: Vec<RotaryFile>,
    #[serde(default)]
    shift_down: Option<Vec<u8>>,
    #[serde(default)]
    shift_up: Option<Vec<u8>>,
    key_channel: u8,
    key_anchor: u8,
    pad_channel: u8,
    pads: [[u8; 8]; 2],
    pitch_channel: u8,
    pitch_returns_to_center: bool,
    /// Set only after the physical acceptance in GP-H2 has been performed.
    hardware_qualified: bool,
}
pub const CONTRACT: &str = "SHR-DESK-CONTROLLER";
const MAX_BYTES: usize = 16 * 1024;

/// Strictly decode one learned profile. Unknown fields, missing rotaries,
/// half-specified Shift messages and every existing collision rule are refused.
pub fn parse_profile(bytes: &[u8]) -> Result<Profile, String> {
    if bytes.len() > MAX_BYTES {
        return Err("controller profile capacity".into());
    }
    let f: ProfileFile = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    if f.contract != CONTRACT || f.version != 1 {
        return Err("unsupported controller profile contract".into());
    }
    if f.rotaries.len() != 16 {
        return Err("controller profile requires sixteen learned rotaries".into());
    }
    let shift = match (f.shift_down, f.shift_up) {
        (Some(d), Some(u)) => Some((d, u)),
        (None, None) => None,
        _ => return Err("shift requires both captured messages".into()),
    };
    let mut rotaries = f.rotaries.into_iter().map(|r| Rotary {
        cc: r.cc,
        encoding: Relative::from(r.encoding),
        alternate: r
            .alternate_cc
            .map(|cc| (cc, Relative::from(EncodingFile::TwosComplement))),
    });
    let rotaries: [Rotary; 16] = std::array::from_fn(|_| rotaries.next().unwrap());
    let profile = Profile {
        endpoint: f.endpoint,
        rotary_channel: f.rotary_channel,
        rotaries,
        shift,
        key_channel: f.key_channel,
        anchor: f.key_anchor,
        pad_channel: f.pad_channel,
        pads: f.pads,
        pitch_channel: f.pitch_channel,
        pitch_returns_to_center: f.pitch_returns_to_center,
        hardware_qualified: f.hardware_qualified,
    };
    profile.validate()?;
    Ok(profile)
}
pub fn load_profile(path: &Path) -> Result<Profile, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("controller profile: {e}"))?;
    parse_profile(&bytes)
}

/// Convert SHR-DAW's learned `controller.conf` (v9) into a Desk profile.
///
/// Learned values are used exactly: rotary 1 (`encoder.relative_cc`), its
/// hardware-Shift alternate (`encoder.modified_relative_cc`), rotary 9
/// (`encoder.secondary_press_*` side uses `rotary.9`), rotaries 2–16 and pads
/// 1–8. Arturia Relative 1 (centre 64) is `binary_offset`; `relative_reverse`
/// is refused because Desk has no reversed binary-offset encoding.
/// SHR-DAW learns no second pad bank, key channel or Shift message, so those are
/// returned as explicit, named defaults in the second value, never as learned facts.
pub fn import_daw(
    text: &str,
    endpoint: &str,
    channel: u8,
) -> Result<(serde_json::Value, Vec<&'static str>), String> {
    let mut map = std::collections::HashMap::new();
    let mut lines = text.lines();
    if !lines
        .next()
        .is_some_and(|l| l.trim() == "# SHR-DAW controller profile v9")
    {
        return Err("expected SHR-DAW controller profile v9".into());
    }
    for line in lines {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (k, v) = line.split_once('=').ok_or("malformed SHR-DAW line")?;
        map.insert(k.trim(), v.trim());
    }
    let cc = |k: &str| -> Result<u8, String> {
        map.get(k)
            .and_then(|v| v.parse::<u8>().ok())
            .filter(|v| *v < 128)
            .ok_or_else(|| format!("SHR-DAW profile lacks learned {k}"))
    };
    for k in [
        "encoder.relative_reverse",
        "encoder.modified_relative_reverse",
    ] {
        if map.get(k).is_some_and(|v| *v == "true") {
            return Err(format!("{k}=true has no Desk encoding"));
        }
    }
    let mut rotaries = vec![serde_json::json!({
        "cc": cc("encoder.relative_cc")?,
        "encoding": "binary_offset",
        "alternate_cc": cc("encoder.modified_relative_cc")?,
    })];
    for i in 2..=16 {
        rotaries.push(
            serde_json::json!({"cc": cc(&format!("rotary.{i}"))?, "encoding": "binary_offset"}),
        );
    }
    let mut pad_channel = None;
    let mut bank = [0u8; 8];
    for (i, note) in bank.iter_mut().enumerate() {
        let v = map
            .get(format!("pad.{}", i + 1).as_str())
            .ok_or("SHR-DAW profile lacks learned pads 1-8")?;
        let parts: Vec<_> = v.split('.').collect();
        let [kind, ch, n] = parts[..] else {
            return Err("pad binding format".into());
        };
        let (ch, n): (u8, u8) = (
            ch.parse().map_err(|_| "pad channel")?,
            n.parse().map_err(|_| "pad note")?,
        );
        if kind != "note" || !(1..=16).contains(&ch) || n > 127 {
            return Err("pads must be learned notes".into());
        }
        if pad_channel.replace(ch - 1).is_some_and(|c| c != ch - 1) {
            return Err("pads use more than one channel".into());
        }
        *note = n;
    }
    let pad_channel = pad_channel.unwrap();
    // Defaults for what SHR-DAW does not learn. Bank 2 continues the factory
    // pad layout (pads 9-16 = bank 1 + 8); keys use the rotary channel from C3.
    let bank2: Vec<u8> = bank.iter().map(|n| n.saturating_add(8).min(127)).collect();
    let key_anchor = if bank2
        .iter()
        .chain(bank.iter())
        .any(|n| (48..=72).contains(n))
        && pad_channel == channel
    {
        60
    } else {
        48
    };
    let profile = serde_json::json!({
        "contract": CONTRACT, "version": 1,
        "endpoint": endpoint,
        "rotary_channel": channel,
        "rotaries": rotaries,
        "key_channel": channel, "key_anchor": key_anchor,
        "pad_channel": pad_channel,
        "pads": [bank, bank2],
        "pitch_channel": channel,
        "pitch_returns_to_center": true,
        "hardware_qualified": false,
    });
    parse_profile(&serde_json::to_vec(&profile).map_err(|e| e.to_string())?)?;
    Ok((
        profile,
        vec![
            "pads bank 2 assumed = bank 1 + 8 (not learned by SHR-DAW)",
            "key channel = rotary channel, anchor C3/C4 (not learned by SHR-DAW)",
            "no standalone Shift message: mkII Shift is the learned alternate CC",
        ],
    ))
}

/// Owns the acquisition worker and the decoded input state for one console.
pub struct Acquisition {
    profile: Profile,
    input: Option<crate::controller_flow::Input>,
    events: Receiver<Event>,
    acquired: u64,
    started: Instant,
    pub last_error: Option<String>,
    _reader: Reader,
}
impl Acquisition {
    /// Start read-only acquisition of the profile's endpoint under `/dev/snd`.
    pub fn start(front: &mut Frontend, profile: Profile) -> Result<Self, String> {
        Self::start_at(front, profile, Path::new("/dev/snd"))
    }
    pub fn start_at(front: &mut Frontend, profile: Profile, snd: &Path) -> Result<Self, String> {
        let input = front.attach_controller_profile(profile.clone())?;
        let id = profile.endpoint.clone();
        let snd = snd.to_owned();
        let (reader, events) =
            Reader::spawn(&profile.endpoint, move || midi::resolve_by_id(&snd, &id))?;
        Ok(Self {
            profile,
            input: Some(input),
            events,
            // The reader numbers its first connection 1; the attach above covers it.
            acquired: 1,
            started: Instant::now(),
            last_error: None,
            _reader: reader,
        })
    }
    /// Start from an explicit path (FIFO or device node) instead of `by-id`.
    pub fn start_path(
        front: &mut Frontend,
        profile: Profile,
        path: std::path::PathBuf,
    ) -> Result<Self, String> {
        let input = front.attach_controller_profile(profile.clone())?;
        let (reader, events) = Reader::spawn(&profile.endpoint, move || Ok(path.clone()))?;
        Ok(Self {
            profile,
            input: Some(input),
            events,
            // The reader numbers its first connection 1; the attach above covers it.
            acquired: 1,
            started: Instant::now(),
            last_error: None,
            _reader: reader,
        })
    }
    /// Drain all queued acquisition events into the frontend. Returns the number
    /// of delivered messages. Never blocks.
    pub fn poll(&mut self, front: &mut Frontend) -> usize {
        let mut delivered = 0;
        while let Ok(event) = self.events.try_recv() {
            match event {
                Event::Lost { generation, .. } => {
                    if generation >= self.acquired {
                        front.controller_lost();
                        self.input = None;
                    }
                }
                Event::Message {
                    endpoint,
                    message,
                    generation,
                } => {
                    if generation < self.acquired {
                        continue; // delayed input from an earlier connection
                    }
                    if generation > self.acquired || self.input.is_none() {
                        // New connection: reattach, which fences held state and
                        // requires pad release before anything can act.
                        self.acquired = generation;
                        match front.attach_controller_profile(self.profile.clone()) {
                            Ok(input) => self.input = Some(input),
                            Err(e) => {
                                self.last_error = Some(e);
                                self.input = None;
                                continue;
                            }
                        }
                    }
                    let Some(input) = &mut self.input else {
                        continue;
                    };
                    let now = self.started.elapsed().as_millis() as u64;
                    let provider = front.controller_generation();
                    match front.inject_controller_flow(
                        input,
                        &endpoint,
                        message.bytes(),
                        provider,
                        now,
                    ) {
                        Ok(()) => delivered += 1,
                        Err(e) => self.last_error = Some(e),
                    }
                }
            }
        }
        delivered
    }
}

/// Read-only learn monitor: print each complete message from a by-id endpoint
/// for `seconds`, so the operator can capture the actual controller's notes,
/// CCs and encoder values. Writes nothing to the controller.
pub fn learn(endpoint: &str, seconds: u64, out: &mut impl std::io::Write) -> Result<(), String> {
    let id = endpoint.to_owned();
    let (_reader, events) = Reader::spawn(endpoint, move || {
        midi::resolve_by_id(Path::new("/dev/snd"), &id)
    })?;
    let deadline = Instant::now() + std::time::Duration::from_secs(seconds.clamp(1, 600));
    writeln!(
        out,
        "# read-only capture of {endpoint}; press each pad, turn each encoder both ways"
    )
    .map_err(|e| e.to_string())?;
    while let Some(left) = deadline.checked_duration_since(Instant::now()) {
        match events.recv_timeout(left.min(std::time::Duration::from_millis(100))) {
            Ok(Event::Message {
                message,
                generation,
                ..
            }) => {
                let b = message.bytes();
                let kind = match b[0] & 0xf0 {
                    0x80 => "note-off",
                    0x90 if b[2] == 0 => "note-off",
                    0x90 => "note-on",
                    0xb0 => "cc",
                    0xe0 => "pitch",
                    0xc0 => "program",
                    0xa0 | 0xd0 => "pressure",
                    _ => "other",
                };
                let hex: Vec<String> = b.iter().map(|v| format!("{v:02x}")).collect();
                writeln!(
                    out,
                    "{kind:<9} ch{:<2} {:<9} gen{generation}",
                    (b[0] & 15) + 1,
                    hex.join(" ")
                )
                .map_err(|e| e.to_string())?;
            }
            Ok(Event::Lost { .. }) => {
                writeln!(out, "# controller lost; waiting").map_err(|e| e.to_string())?
            }
            Err(_) => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    pub(crate) fn sample() -> serde_json::Value {
        serde_json::json!({
            "contract": CONTRACT, "version": 1,
            "endpoint": "usb-Arturia_Arturia_MiniLab_mkII-00",
            "rotary_channel": 0,
            "rotaries": (0..16).map(|i| serde_json::json!({"cc": 16 + i, "encoding": "binary_offset"})).collect::<Vec<_>>(),
            "key_channel": 0, "key_anchor": 48, "pad_channel": 9,
            "pads": [[36,37,38,39,40,41,42,43],[44,45,46,47,48,49,50,51]],
            "pitch_channel": 0, "pitch_returns_to_center": true,
            "hardware_qualified": false
        })
    }
    #[test]
    fn imports_learned_shr_daw_minilab_profile() {
        let conf = "# SHR-DAW controller profile v9\ninput=Arturia MiniLab mkII:Arturia MiniLab mkII MIDI 1\nencoder.relative_cc=112\nencoder.relative_reverse=false\nencoder.modified_relative_cc=7\nencoder.modified_relative_reverse=false\nrotary.2=74\nrotary.3=71\nrotary.4=76\nrotary.5=77\nrotary.6=93\nrotary.7=73\nrotary.8=75\nrotary.9=114\nrotary.10=18\nrotary.11=19\nrotary.12=16\nrotary.13=17\nrotary.14=91\nrotary.15=79\nrotary.16=72\npad.1=note.10.36\npad.2=note.10.37\npad.3=note.10.38\npad.4=note.10.39\npad.5=note.10.40\npad.6=note.10.41\npad.7=note.10.42\npad.8=note.10.43\n";
        let (v, assumed) = import_daw(conf, "usb-Arturia_Arturia_MiniLab_mkII-00", 9).unwrap();
        let p = parse_profile(&serde_json::to_vec(&v).unwrap()).unwrap();
        assert_eq!(
            (p.rotaries[0].cc, p.rotaries[0].alternate.unwrap().0),
            (112, 7)
        );
        assert_eq!(
            (p.rotaries[8].cc, p.rotary_channel, p.pad_channel),
            (114, 9, 9)
        );
        assert_eq!(p.pads[0], [36, 37, 38, 39, 40, 41, 42, 43]);
        assert_eq!(
            p.anchor, 60,
            "bank 2 notes 44-51 cannot overlap same-channel keys"
        );
        assert_eq!(assumed.len(), 3);
        assert!(
            import_daw(
                &conf.replace("relative_reverse=false", "relative_reverse=true"),
                "x",
                9
            )
            .is_err()
        );
        assert!(import_daw(&conf.replace("rotary.16=72\n", ""), "x", 9).is_err());
        assert!(import_daw("# SHSynth controller profile v1\n", "x", 9).is_err());
    }
    #[test]
    fn strict_profile_refuses_unknown_partial_and_colliding_bindings() {
        let p = parse_profile(&serde_json::to_vec(&sample()).unwrap()).unwrap();
        assert_eq!(p.rotaries[3].cc, 19);
        assert!(!p.hardware_qualified);
        let mut v = sample();
        v["unexpected"] = true.into();
        assert!(parse_profile(&serde_json::to_vec(&v).unwrap()).is_err());
        let mut v = sample();
        v["rotaries"].as_array_mut().unwrap().pop();
        assert!(parse_profile(&serde_json::to_vec(&v).unwrap()).is_err());
        let mut v = sample();
        v["shift_down"] = serde_json::json!([0xb0, 90, 127]);
        assert!(parse_profile(&serde_json::to_vec(&v).unwrap()).is_err());
        let mut v = sample();
        v["pads"][1][0] = 36.into();
        assert!(parse_profile(&serde_json::to_vec(&v).unwrap()).is_err());
        let mut v = sample();
        v["version"] = 2.into();
        assert!(parse_profile(&serde_json::to_vec(&v).unwrap()).is_err());
        let mut v = sample();
        v["rotaries"][0]["alternate_cc"] = 100.into();
        assert!(parse_profile(&serde_json::to_vec(&v).unwrap()).is_ok());
        v["rotaries"][2]["alternate_cc"] = 101.into();
        assert!(
            parse_profile(&serde_json::to_vec(&v).unwrap()).is_err(),
            "alternate only on rotaries 1/9"
        );
    }
}
