//! Full supported audio intent; no execution state or authority is serializable.
use crate::{provider, topology::Topology};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::Path;
pub const MAX_BYTES: usize = 1024 * 1024;
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Edit {
    pub target: provider::Target,
    pub value: Value,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Engine {
    pub version: u32,
    pub show_id: String,
    pub topology: Topology,
    pub parameters: Vec<Edit>,
    pub processing: Vec<crate::processing::Config>,
    pub send_taps: Vec<Vec<crate::sends::Tap>>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Brain {
    pub version: u32,
    pub source: crate::brain::Source,
    pub monitor_gain_cdb: i32,
    pub monitor_mute: bool,
    pub monitor_dim: bool,
    pub talkback_monitors: Vec<usize>,
    pub talkback_gain_cdb: i32,
    pub talkback_mute: bool,
}
impl Brain {
    pub fn monitor_body(&self) -> Value {
        json!({"source":self.source,"gain_cdb":self.monitor_gain_cdb,"mute":self.monitor_mute,"dim":self.monitor_dim,"armed":false})
    }
    pub fn talkback_body(&self) -> Value {
        json!({"monitors":self.talkback_monitors,"gain_cdb":self.talkback_gain_cdb,"mute":self.talkback_mute})
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Composed {
    pub version: u32,
    pub engine: Engine,
    pub brain: Brain,
    pub pa_configuration_json: Option<String>,
    pub pa_program_buses: Vec<usize>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Saved {
    pub format: String,
    pub version: u32,
    pub composed: Composed,
    pub fx: Option<Value>,
}
#[derive(Clone, Debug)]
pub enum Operation {
    Save(std::path::PathBuf, usize),
    Review(std::path::PathBuf, usize),
    Restore(std::path::PathBuf, usize),
    Verify(std::path::PathBuf, usize),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Slot(i8),
    Save,
    Review,
    Restore,
    Verify,
    Hold,
    ReleaseHold,
}
#[derive(Clone, Debug)]
pub struct Step {
    pub label: String,
    pub scope: String,
    pub kind: String,
    pub body: Value,
}
impl Saved {
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        let text = std::str::from_utf8(bytes).map_err(|e| e.to_string())?;
        let value = crate::master_eq::parse_owner_bounded(text, MAX_BYTES)?;
        let saved: Self = serde_json::from_value(value).map_err(|e| e.to_string())?;
        saved.validate()?;
        Ok(saved)
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.format != "gigpies-audio-setup"
            || self.version != 1
            || self.composed.version != 1
            || self.composed.engine.version != 4
            || self.composed.brain.version != 1
        {
            return Err("Unsupported complete audio setup version".into());
        }
        let e = &self.composed.engine;
        if !provider::uuid(&e.show_id) {
            return Err("Setup show identity".into());
        }
        e.topology.validate()?;
        let n = e.topology.inputs.len();
        let m = e.topology.monitors;
        if e.processing.len() != n
            || e.send_taps.len() != n
            || e.send_taps.iter().any(|r| r.len() != m)
        {
            return Err("Setup processing/send inventory".into());
        }
        for c in &e.processing {
            c.validate()?;
        }
        let mut expected = std::collections::BTreeSet::new();
        expected.insert(provider::Target {
            parameter: "master_gain".into(),
            input: String::new(),
            monitor: None,
        });
        for input in &e.topology.inputs {
            for parameter in ["fader", "pan", "mute"] {
                expected.insert(provider::Target {
                    parameter: parameter.into(),
                    input: input.id.clone(),
                    monitor: None,
                });
            }
            for j in 0..m {
                expected.insert(provider::Target {
                    parameter: "send".into(),
                    input: input.id.clone(),
                    monitor: Some(format!("monitor-{}", j + 1)),
                });
            }
        }
        for edit in &e.parameters {
            if !expected.remove(&edit.target) {
                return Err("Duplicate/unknown setup parameter".into());
            }
            provider::value(&edit.target, &edit.value)?;
        }
        if !expected.is_empty() {
            return Err("Incomplete setup parameter inventory".into());
        }
        for (kind, body) in [
            ("brain_monitor_set", self.composed.brain.monitor_body()),
            ("brain_talkback_set", self.composed.brain.talkback_body()),
        ] {
            crate::brain::validate_body(kind, &body, None, n, m)?;
        }
        if let Some(c) = &self.composed.pa_configuration_json {
            crate::structure::validate_body(
                "pa_set",
                &json!({"configuration_json":c,"program_buses":self.composed.pa_program_buses}),
                None,
            )?;
            if self.composed.pa_program_buses.iter().any(|b| *b >= m + 2) {
                return Err("PA program bus inventory".into());
            }
        } else if !self.composed.pa_program_buses.is_empty() {
            return Err("PA mapping without owner".into());
        }
        if let Some(fx) = &self.fx {
            crate::fx_saved::validate(fx)?;
            if fx["show_id"] != e.show_id
                || fx["inputs"] != json!(e.topology.inputs)
                || fx["monitors"].as_array().unwrap().len() != m
            {
                return Err("Setup FX identity differs from core".into());
            }
        }
        Ok(())
    }
    pub fn load(path: &Path, slot: usize) -> Result<Self, String> {
        Self::decode(&crate::saved_file::load(
            path,
            "audio-setup",
            slot,
            MAX_BYTES,
        )?)
    }
    pub fn save(&self, path: &Path, slot: usize) -> Result<(), String> {
        self.validate()?;
        crate::saved_file::save(path, "audio-setup", slot, &json!(self), MAX_BYTES)
    }
    pub fn compatible(&self, current: &Self) -> Result<(), String> {
        self.validate()?;
        current.validate()?;
        let mut a = self.composed.engine.topology.clone();
        let mut b = current.composed.engine.topology.clone();
        a.map_revision = 1;
        b.map_revision = 1;
        for p in &mut a.outputs {
            p.source = None;
        }
        for p in &mut b.outputs {
            p.source = None;
        }
        if self.composed.engine.show_id != current.composed.engine.show_id || a != b {
            return Err(
                "Incompatible show/input/output identities or capabilities; no automatic remapping"
                    .into(),
            );
        }
        if self.composed.pa_configuration_json.is_some()
            != current.composed.pa_configuration_json.is_some()
        {
            return Err("PA owner availability changed".into());
        }
        match (&self.fx, &current.fx) {
            (Some(a), Some(b)) => {
                for k in ["show_id", "library_sha256", "inputs", "monitors"] {
                    if a[k] != b[k] {
                        return Err(format!("Incompatible FX {k}"));
                    }
                }
            }
            (None, None) => (),
            _ => return Err("FX owner availability changed".into()),
        }
        Ok(())
    }
    pub fn comparison(&self) -> Value {
        let mut value = json!(self);
        // Review labels bind values to stable channel/bus identities rather
        // than exposing a positional index into the serialized parameter list.
        let engine = &self.composed.engine;
        value["composed"]["engine"]["parameters"] = Value::Object(
            engine
                .parameters
                .iter()
                .map(|edit| {
                    (
                        format!(
                            "{} {} {}",
                            edit.target.parameter,
                            edit.target.input,
                            edit.target.monitor.as_deref().unwrap_or("")
                        )
                        .trim()
                        .to_owned(),
                        match edit.target.parameter.as_str() {
                            "fader" | "send" | "master_gain" => edit
                                .value
                                .as_i64()
                                .map(|v| json!(format!("{:.3} dB", v as f64 / 1000.0)))
                                .unwrap_or_else(|| edit.value.clone()),
                            "pan" => edit
                                .value
                                .as_i64()
                                .map(|v| json!(format!("{v}% (negative left, positive right)")))
                                .unwrap_or_else(|| edit.value.clone()),
                            _ => edit.value.clone(),
                        },
                    )
                })
                .collect(),
        );
        value["composed"]["engine"]["processing"] = Value::Object(
            engine
                .topology
                .inputs
                .iter()
                .zip(&engine.processing)
                .map(|(input, config)| (input.id.clone(), json!(config)))
                .collect(),
        );
        value["composed"]["engine"]["send_taps"] = Value::Object(
            engine
                .topology
                .inputs
                .iter()
                .zip(&engine.send_taps)
                .map(|(input, taps)| {
                    (
                        input.id.clone(),
                        Value::Object(
                            taps.iter()
                                .enumerate()
                                .map(|(i, tap)| (format!("monitor-{}", i + 1), json!(tap)))
                                .collect(),
                        ),
                    )
                })
                .collect(),
        );
        if let Some(text) = &self.composed.pa_configuration_json
            && let Ok(parsed) = serde_json::from_str::<Value>(text)
        {
            value["composed"]["pa_configuration_json"] = parsed;
        }
        if let Some(fx) = &self.fx
            && let Some(configs) = fx["configurations"].as_array()
        {
            for (i, text) in configs.iter().enumerate() {
                if let Some(text) = text.as_str()
                    && let Ok(parsed) = crate::fx_aux::configuration(text)
                {
                    value["fx"]["configurations"][i] = parsed;
                }
            }
        }
        value
    }
    pub fn steps(&self, current: &Self) -> Result<Vec<Step>, String> {
        self.compatible(current)?;
        let mut steps = Vec::new();
        let mut add = |label: String, scope: String, kind: &str, body: Value| {
            steps.push(Step {
                label,
                scope,
                kind: kind.into(),
                body,
            })
        };
        let e = &self.composed.engine;
        let c = &current.composed.engine;
        for edit in &e.parameters {
            if !c.parameters.contains(edit) {
                add(
                    format!(
                        "{} {} {}",
                        edit.target.parameter,
                        edit.target.input,
                        edit.target.monitor.as_deref().unwrap_or("")
                    ),
                    edit.target
                        .monitor
                        .as_ref()
                        .map(|s| s.replace("monitor-", "monitor"))
                        .unwrap_or("foh".into()),
                    "set",
                    json!({"targets":[edit]}),
                );
            }
        }
        for (i, (saved, current)) in e.processing.iter().zip(&c.processing).enumerate() {
            if saved != current {
                add(
                    format!("{} processing", e.topology.inputs[i].id),
                    "foh".into(),
                    "processing_set",
                    json!({"input":e.topology.inputs[i].id,"config":saved}),
                );
            }
        }
        for (i, row) in e.send_taps.iter().enumerate() {
            for (j, tap) in row.iter().enumerate() {
                if *tap != c.send_taps[i][j] {
                    add(
                        format!("{} monitor-{} tap", e.topology.inputs[i].id, j + 1),
                        format!("monitor{}", j + 1),
                        "send_tap_set",
                        json!({"input":e.topology.inputs[i].id,"monitor":format!("monitor-{}",j+1),"tap":tap}),
                    );
                }
            }
        }
        let saved_comparison = self.comparison();
        let current_comparison = current.comparison();
        if (saved_comparison["composed"]["pa_configuration_json"]
            != current_comparison["composed"]["pa_configuration_json"]
            || self.composed.pa_program_buses != current.composed.pa_program_buses)
            && let Some(config) = &self.composed.pa_configuration_json
        {
            add(
                "PA complete configuration".into(),
                "pa_configuration".into(),
                "pa_set",
                json!({"configuration_json":config,"program_buses":self.composed.pa_program_buses}),
            );
        }
        if e.topology.outputs != c.topology.outputs {
            add(
                "Output routing".into(),
                "output_routes".into(),
                "output_patch",
                json!({"outputs":e.topology.outputs}),
            );
        }
        if self.composed.brain.monitor_body() != current.composed.brain.monitor_body() {
            add(
                "Brain monitor settings (DISARMED)".into(),
                "local_operator_monitor".into(),
                "brain_monitor_set",
                self.composed.brain.monitor_body(),
            );
        }
        if self.composed.brain.talkback_body() != current.composed.brain.talkback_body() {
            add(
                "Talkback settings (NOT HELD)".into(),
                "talkback_destinations".into(),
                "brain_talkback_set",
                self.composed.brain.talkback_body(),
            );
        }
        // FX body is bound to fresh live generations at confirmation, never saved generations.
        if saved_comparison["fx"] != current_comparison["fx"] {
            add(
                "Four-aux FX bundle".into(),
                "fx_configuration".into(),
                "fx_aux_configure",
                self.fx.clone().ok_or("FX missing")?,
            );
        }
        Ok(steps)
    }
    pub fn review(&self, current: &Self) -> String {
        let compatibility = self.compatible(current).err();
        let mut lines=vec!["COMPLETE AUDIO SETUP — CURRENT -> SAVED".into(), "Restore is per scope, with correlated completion and readback for each step. Partial/UNKNOWN is not rollback. Hold outputs first; rearm is separate.".into()];
        if let Some(e) = compatibility {
            lines.push(format!("INCOMPATIBLE — restore refused: {e}"));
        }
        fn walk(path: &str, a: &Value, b: &Value, lines: &mut Vec<String>) {
            if a == b {
                return;
            }
            match (a, b) {
                (Value::Object(a), Value::Object(b)) => {
                    for k in a
                        .keys()
                        .chain(b.keys())
                        .collect::<std::collections::BTreeSet<_>>()
                    {
                        walk(
                            &format!("{path}/{k}"),
                            a.get(k).unwrap_or(&Value::Null),
                            b.get(k).unwrap_or(&Value::Null),
                            lines,
                        );
                    }
                }
                (Value::Array(a), Value::Array(b)) if a.len() == b.len() => {
                    for (i, (a, b)) in a.iter().zip(b).enumerate() {
                        walk(&format!("{path}/{i}"), a, b, lines);
                    }
                }
                _ => lines.push(format!("{path}: {a} -> {b}")),
            }
        }
        walk("", &current.comparison(), &self.comparison(), &mut lines);
        if let Ok(steps) = self.steps(current) {
            lines.push(format!("{} differences requiring commands", steps.len()));
            for s in steps {
                lines.push(format!("{}: {}", s.scope, s.label));
            }
        }
        lines.join("\n")
    }
}

/// One current receipt per scope, tied to the exact saved payload. This is
/// recovery evidence, never input to a mutation or proof of live state.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Journal {
    pub version: u32,
    pub saved_sha256: String,
    pub scopes: std::collections::BTreeMap<String, Value>,
}
impl Journal {
    pub fn read(path: &Path, slot: usize, saved: &Saved) -> Result<Self, String> {
        use sha2::{Digest, Sha256};
        let hash = format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(saved).map_err(|e| e.to_string())?)
        );
        let empty = Self {
            version: 1,
            saved_sha256: hash.clone(),
            scopes: Default::default(),
        };
        let file = path.join(format!("audio-recall-{:02}.json", slot + 1));
        match std::fs::symlink_metadata(&file) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(empty),
            Err(e) => return Err(e.to_string()),
            _ => (),
        }
        let bytes = crate::saved_file::load(path, "audio-recall", slot, MAX_BYTES)?;
        let value = crate::master_eq::parse_owner_bounded(
            std::str::from_utf8(&bytes).map_err(|e| e.to_string())?,
            MAX_BYTES,
        )?;
        let journal: Self =
            serde_json::from_value(value).map_err(|e| format!("Malformed recall receipt: {e}"))?;
        if journal.version != 1
            || journal.scopes.len() > 4096
            || journal.saved_sha256.len() != 64
            || !journal
                .saved_sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err("Recall receipt version/capacity".into());
        }
        for (scope, r) in &journal.scopes {
            if crate::scopes::value(scope).is_err()
                || !matches!(
                    r["state"].as_str(),
                    Some(
                        "pending"
                            | "UNKNOWN"
                            | "partial"
                            | "applied-readback-unavailable"
                            | "scope-complete"
                            | "scope-complete-with-prior-unknown"
                    )
                )
                || !r["completed"].is_array()
                || !(r["current_step"].is_null() || r["current_step"].is_string())
                || !(r["reason"].is_null() || r["reason"].is_string())
                || !(r["prior_unknown"].is_null()
                    || r["prior_unknown"]
                        .as_array()
                        .is_some_and(|v| v.len() <= 128))
            {
                return Err("Malformed recall scope receipt".into());
            }
        }
        if journal.saved_sha256 != hash {
            return Ok(empty);
        }
        Ok(journal)
    }
    pub fn write(
        &mut self,
        path: &Path,
        slot: usize,
        scope: &str,
        record: &Value,
    ) -> Result<(), String> {
        self.scopes.insert(scope.into(), record.clone());
        crate::saved_file::save(path, "audio-recall", slot, &json!(self), MAX_BYTES)
    }
    pub fn prior_unknown(&self, scope: &str) -> Result<Vec<Value>, String> {
        let Some(previous) = self.scopes.get(scope) else {
            return Ok(Vec::new());
        };
        let mut unknown = previous["prior_unknown"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        if matches!(
            previous["state"].as_str(),
            Some("UNKNOWN" | "applied-readback-unavailable")
        ) {
            let note = json!({"state":previous["state"],"step":previous["current_step"],"epoch":previous["epoch"],"reason":previous["reason"]});
            if !unknown.contains(&note) {
                unknown.push(note);
            }
        }
        if unknown.len() > 128 {
            return Err("Recall unresolved receipt capacity; preserve evidence before starting another attempt".into());
        }
        Ok(unknown)
    }
    pub fn summary(&self) -> String {
        if self.scopes.is_empty() {
            return "No correlated recall receipts for this saved payload; matching values are not evidence of prior execution.".into();
        }
        self.scopes
            .iter()
            .map(|(scope, r)| {
                format!(
                    "Recall receipt {scope}: {} / completed {} / current {} / {} / prior UNKNOWN {}",
                    r["state"],
                    r["completed"].as_array().map_or(0, Vec::len),
                    r["current_step"],
                    r["reason"],
                    r["prior_unknown"]
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Saved {
        let mut engine: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/gp18/v1-corrected/intent-v2.json"
        ))
        .unwrap();
        engine["version"] = json!(4);
        engine["parameters"]
            .as_array_mut()
            .unwrap()
            .push(json!({"target":{"parameter":"master_gain"},"value":0}));
        let saved:Saved=serde_json::from_value(json!({"format":"gigpies-audio-setup","version":1,"composed":{"version":1,"engine":engine,"brain":{"version":1,"source":{"kind":"none"},"monitor_gain_cdb":-1200,"monitor_mute":true,"monitor_dim":false,"talkback_monitors":[],"talkback_gain_cdb":-1200,"talkback_mute":true},"pa_configuration_json":null,"pa_program_buses":[]},"fx":null})).unwrap();
        saved.validate().unwrap();
        saved
    }
    #[test]
    fn complete_inventory_no_runtime_fields_and_exact_identity() {
        let saved = fixture();
        let bytes = serde_json::to_vec(&saved).unwrap();
        assert_eq!(Saved::decode(&bytes).unwrap(), saved);
        for field in [
            "armed",
            "credentials",
            "lease",
            "held_generation",
            "recording",
        ] {
            let mut bad = json!(saved);
            bad.as_object_mut()
                .unwrap()
                .insert(field.into(), json!(true));
            assert!(Saved::decode(&serde_json::to_vec(&bad).unwrap()).is_err());
        }
        let mut current = saved.clone();
        current.composed.engine.topology.inputs[0]
            .physical_port
            .push('x');
        assert!(saved.compatible(&current).is_err());
        let mut current = saved.clone();
        current.composed.engine.topology.outputs[0]
            .physical_port
            .push('x');
        assert!(saved.compatible(&current).is_err());
        let mut current = saved.clone();
        current.composed.engine.topology.map_revision += 5;
        assert!(saved.compatible(&current).is_ok());
        let mut bad = saved.clone();
        bad.composed.engine.parameters.pop();
        assert!(bad.validate().is_err());
        let mut bad = saved.clone();
        bad.composed
            .engine
            .parameters
            .push(bad.composed.engine.parameters[0].clone());
        assert!(bad.validate().is_err());
        assert!(Saved::decode(br#"{"version":1,"version":1}"#).is_err());
    }
    #[test]
    fn diff_routes_scopes_and_never_arms() {
        let saved = fixture();
        let mut current = saved.clone();
        current.composed.engine.parameters[0].value = json!(-10000);
        current.composed.engine.processing[0].compressor_bypass =
            !current.composed.engine.processing[0].compressor_bypass;
        current.composed.engine.send_taps[0][0] = crate::sends::Tap::ProcessedPostFader;
        current.composed.brain.monitor_mute = false;
        current.composed.engine.topology.outputs[0].source = None;
        let steps = saved.steps(&current).unwrap();
        assert!(steps.iter().any(|s| s.scope == "foh"));
        assert!(steps.iter().any(|s| s.scope == "output_routes"));
        assert!(
            steps
                .iter()
                .any(|s| s.kind == "brain_monitor_set" && s.body["armed"] == false)
        );
        assert!(
            steps
                .iter()
                .all(|s| s.kind != "output_rearm" && s.kind != "brain_hold")
        );
        let review = saved.review(&current);
        assert!(review.contains("CURRENT -> SAVED"));
        assert!(review.contains("compressor_bypass"));
        assert!(review.contains("dB"));
        assert!(saved.steps(&saved).unwrap().is_empty());
    }
    #[test]
    fn owner_json_order_is_not_a_recall_difference_but_library_identity_is() {
        let mut saved = fixture();
        let snapshot: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/gp-fx-aux/v1/return-owner.json"
        ))
        .unwrap();
        let mut fx = crate::fx_saved::capture(&snapshot).unwrap();
        let e = &saved.composed.engine;
        fx["show_id"] = json!(e.show_id);
        fx["inputs"] = json!(e.topology.inputs);
        fx["monitors"] = json!(
            (0..e.topology.monitors)
                .map(|i| format!("monitor-{}", i + 1))
                .collect::<Vec<_>>()
        );
        for i in 0..4 {
            let mut config =
                crate::fx_aux::configuration(fx["configurations"][i].as_str().unwrap()).unwrap();
            config["sends"] = json!(vec![0.0; e.topology.inputs.len()]);
            config["returns"]["monitors"] = json!(vec![0.0; e.topology.monitors]);
            fx["configurations"][i] = json!(config.to_string());
        }
        saved.fx = Some(fx);
        saved.validate().unwrap();
        let mut current = saved.clone();
        for i in 0..4 {
            let c = crate::fx_aux::configuration(
                current.fx.as_ref().unwrap()["configurations"][i]
                    .as_str()
                    .unwrap(),
            )
            .unwrap();
            current.fx.as_mut().unwrap()["configurations"][i] =
                json!(serde_json::to_string_pretty(&c).unwrap());
        }
        assert_ne!(saved.fx, current.fx);
        assert!(saved.steps(&current).unwrap().is_empty());
        current.fx.as_mut().unwrap()["library_sha256"] = json!("a".repeat(64));
        assert!(saved.compatible(&current).is_err());
    }

    #[test]
    fn private_durable_restart_and_unknown_receipt() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("desk-setup-storage-{}", std::process::id()));
        std::fs::create_dir(&dir).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        let saved = fixture();
        saved.save(&dir, 0).unwrap();
        assert_eq!(Saved::load(&dir, 0).unwrap(), saved);
        let mut journal = Journal::read(&dir, 0, &saved).unwrap();
        journal.write(&dir,0,"foh",&json!({"state":"UNKNOWN","completed":[],"current_step":"fader","reason":"interrupted"})).unwrap();
        let mut journal = Journal::read(&dir, 0, &saved).unwrap();
        assert!(journal.summary().contains("UNKNOWN"));
        let prior = journal.prior_unknown("foh").unwrap();
        assert_eq!(prior.len(), 1);
        journal.write(&dir,0,"foh",&json!({"state":"scope-complete-with-prior-unknown","prior_unknown":prior,"completed":[],"current_step":null,"reason":null})).unwrap();
        assert!(
            Journal::read(&dir, 0, &saved)
                .unwrap()
                .summary()
                .contains("interrupted")
        );
        assert!(
            saved
                .steps(&Saved::load(&dir, 0).unwrap())
                .unwrap()
                .is_empty()
        );
        assert!(journal.summary().contains("UNKNOWN"));
        let mut malformed_receipt = json!(journal);
        malformed_receipt["saved_sha256"] = json!("broken");
        std::fs::write(
            dir.join("audio-recall-01.json"),
            serde_json::to_vec(&malformed_receipt).unwrap(),
        )
        .unwrap();
        assert!(Journal::read(&dir, 0, &saved).is_err());
        std::fs::write(dir.join("audio-setup-01.json"), "{}").unwrap();
        assert!(Saved::load(&dir, 0).is_err());
        std::fs::remove_file(dir.join("audio-setup-01.json")).unwrap();
        std::os::unix::fs::symlink("audio-recall-01.json", dir.join("audio-setup-01.json"))
            .unwrap();
        assert!(Saved::load(&dir, 0).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
