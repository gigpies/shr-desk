//! GP05 recording projection. Other module metadata never grants REC authority.
use crate::{modules::Recording, provider};
use serde_json::Value;
use std::time::{Duration, Instant};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Open,
    Start,
    Stop,
}
#[derive(Clone, Debug)]
pub enum Operation {
    Probe,
    Review { start: bool, body: Value },
}
#[derive(Clone, Debug, Default)]
pub struct View {
    pub observation: Option<Observation>,
    pub received: Option<Instant>,
}
impl View {
    /// Keep recovery identity when transport disappears, never its authority or
    /// freshness. A newly observed no-take replaces this historical evidence.
    pub(crate) fn retain_unknown(&mut self, old: &Self) {
        if self.observation.is_none() {
            self.observation = old.observation.clone();
            self.received = None;
        }
    }
    pub fn fresh(&self) -> bool {
        self.received
            .is_some_and(|t| t.elapsed() <= Duration::from_millis(750))
    }
}
#[derive(Clone, Debug)]
pub struct Observation {
    pub available: bool,
    pub source_fault: u32,
    pub epoch: String,
    pub frame: String,
    pub mapping: Vec<String>,
    pub recording: Option<Recording>,
}
pub fn mutation(kind: &str) -> bool {
    matches!(kind, "record_start" | "record_stop")
}
pub fn validate_body(body: &Value) -> Result<(), String> {
    provider::keys(body, &["take_id", "operation_id"])?;
    if !body["take_id"].as_str().is_some_and(provider::id)
        || provider::counter(body["operation_id"].as_str().ok_or("REC operation type")?)? == 0
    {
        return Err("REC take/operation identity".into());
    }
    Ok(())
}
impl Observation {
    pub fn decode(v: &Value, show: &str, epoch: &str) -> Result<Self, String> {
        let object = v.as_object().ok_or("REC envelope")?;
        let required = [
            "contract",
            "version",
            "show_id",
            "module",
            "epoch",
            "frame",
            "available",
            "physical",
            "protection",
            "readiness",
            "activity",
            "source_fault",
            "configuration",
            "libraries",
            "fx",
            "pa",
            "recording",
        ];
        if required.iter().any(|k| !object.contains_key(*k))
            || object
                .keys()
                .any(|k| !required.contains(&k.as_str()) && k != "pa_v2")
            || v["contract"] != "GP05-modules"
            || !matches!(v["version"].as_u64(), Some(1 | 2))
            || v["show_id"] != show
            || v["epoch"] != epoch
            || v["module"] != "audio"
        {
            return Err("REC status identity/schema".into());
        }
        let frame = v["frame"].as_str().ok_or("REC frame")?.to_string();
        provider::counter(&frame)?;
        let available = v["available"].as_bool().ok_or("REC availability")?;
        let source_fault = u32::try_from(v["source_fault"].as_u64().ok_or("REC source fault")?)
            .map_err(|_| "REC source fault range")?;
        let mut mapping = Vec::new();
        if available {
            let c = &v["configuration"];
            if c["sample_rate"] != 48000
                || c["block_frames"] != 48
                || v["libraries"]["rec"]["sha256"]
                    .as_str()
                    .is_none_or(|s| s.len() != 64 || !s.bytes().all(|b| b.is_ascii_hexdigit()))
            {
                return Err("REC configured provider missing".into());
            }
            mapping = serde_json::from_value(c["raw_inputs"].clone()).map_err(|e| e.to_string())?;
            if mapping.is_empty()
                || mapping.len() > 64
                || mapping
                    .iter()
                    .enumerate()
                    .any(|(i, s)| *s != format!("input-{:02}/raw", i + 1))
            {
                return Err("REC source mapping".into());
            }
        }
        let recording: Option<Recording> =
            serde_json::from_value(v["recording"].clone()).map_err(|e| e.to_string())?;
        if let Some(r) = &recording {
            validate_body(&serde_json::json!({"take_id":r.take_id,"operation_id":r.operation_id}))?;
            provider::counter(&r.source_epoch)?;
            if !available
                || r.mapping != mapping
                || r.durable_frames.is_some()
                || !["preparing", "recording", "finalizing", "finalized"]
                    .contains(&r.state.as_str())
                || !["pending", "complete", "incomplete", "error"].contains(&r.outcome.as_str())
                || (r.state != "finalized" && r.outcome != "pending")
                || (r.state == "finalized" && r.outcome == "pending")
            {
                return Err("REC lifecycle/mapping/durability".into());
            }
            for n in [
                &r.accepted_frames,
                &r.written_frames,
                &r.dropped_frames,
                &r.overflow_blocks,
                &r.gap_frames,
                &r.invalid_blocks,
                &r.clipped_samples,
            ] {
                provider::counter(n)?;
            }
            if provider::counter(&r.written_frames)? > provider::counter(&r.accepted_frames)? {
                return Err("REC written exceeds accepted".into());
            }
            if (r.state == "recording" && r.first_source_frame.is_none())
                || (r.state == "preparing" && r.first_source_frame.is_some())
                || (r.outcome == "complete"
                    && (r.accepted_frames != r.written_frames
                        || r.written_frames == "0"
                        || r.first_source_frame.is_none()
                        || r.writer_fault
                        || r.host_fault != 0
                        || [
                            &r.dropped_frames,
                            &r.overflow_blocks,
                            &r.gap_frames,
                            &r.invalid_blocks,
                            &r.clipped_samples,
                        ]
                        .iter()
                        .any(|s| s.as_str() != "0")))
            {
                return Err("REC readiness/complete integrity contradiction".into());
            }
            if let Some(f) = &r.first_source_frame {
                provider::counter(f)?;
            }
        }
        Ok(Self {
            available,
            source_fault,
            epoch: epoch.into(),
            frame,
            mapping,
            recording,
        })
    }
    pub fn admit(&self, start: bool, body: &Value) -> Result<(), String> {
        validate_body(body)?;
        if !self.available {
            return Err("REC provider unavailable".into());
        }
        if start {
            if self.source_fault != 0
                || self
                    .recording
                    .as_ref()
                    .is_some_and(|r| r.state != "finalized")
            {
                return Err("REC busy or source fault; new start refused".into());
            }
        } else if self.recording.as_ref().is_none_or(|r| {
            r.take_id != body["take_id"]
                || r.operation_id != body["operation_id"]
                || !["preparing", "recording"].contains(&r.state.as_str())
        }) {
            return Err("REC take changed or already finalizing/finalized".into());
        }
        Ok(())
    }
    pub fn lines(&self) -> Vec<String> {
        let mut l = vec![format!(
            "Provider {} / source fault {} / epoch {} / {} raw tracks",
            self.available,
            self.source_fault,
            self.epoch,
            self.mapping.len()
        )];
        if let Some(r) = &self.recording {
            let queued = provider::counter(&r.accepted_frames)
                .unwrap()
                .saturating_sub(provider::counter(&r.written_frames).unwrap());
            l.extend([
                format!(
                    "Take {} / operation {} / {} / outcome {}",
                    r.take_id, r.operation_id, r.state, r.outcome
                ),
                format!(
                    "Accepted {} / queued {} / written {} frames / durable UNKNOWN",
                    r.accepted_frames, queued, r.written_frames
                ),
                format!(
                    "Dropped {} / overflow {} / gaps {} / invalid {} / clipped {}",
                    r.dropped_frames,
                    r.overflow_blocks,
                    r.gap_frames,
                    r.invalid_blocks,
                    r.clipped_samples
                ),
                format!(
                    "Storage/writer fault {} / host fault {} / first source frame {:?}",
                    r.writer_fault, r.host_fault, r.first_source_frame
                ),
            ]);
        } else {
            l.push("No retained take. Start checks storage asynchronously; readiness is not guaranteed.".into());
        }
        l.push("Finalized complete is the writer outcome, not power-loss durability. Interrupted takes require owner recovery.".into());
        l
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const SHOW: &str = "11111111-1111-4111-8111-111111111111";
    fn fixture() -> Value {
        serde_json::from_str(include_str!(
            "../tests/fixtures/gp05/v1/status-finalized.json"
        ))
        .unwrap()
    }
    #[test]
    fn loss_retains_only_stale_recovery_identity_until_actual_readback() {
        let old = View {
            observation: Some(Observation::decode(&fixture(), SHOW, "9").unwrap()),
            received: Some(Instant::now()),
        };
        let mut lost = View::default();
        lost.retain_unknown(&old);
        assert!(lost.observation.as_ref().unwrap().recording.is_some());
        assert!(!lost.fresh());
        let mut observed = old.clone();
        observed.observation.as_mut().unwrap().recording = None;
        observed.retain_unknown(&old);
        assert!(observed.observation.unwrap().recording.is_none());
    }
    #[test]
    fn transport_admits_only_existing_gp05_float_metadata() {
        let v = fixture();
        let bytes = serde_json::to_vec(&v).unwrap();
        let d = provider::StrictDocument::parse(&bytes).unwrap();
        Observation::decode(d.value(), SHOW, "9").unwrap();
        let outer = serde_json::json!({"kind":"reply","session":"session-test","payload":v});
        provider::StrictDocument::parse(&serde_json::to_vec(&outer).unwrap()).unwrap();
        let mut bad = fixture();
        bad["source_fault"] = serde_json::json!(0.5);
        assert!(provider::StrictDocument::parse(&serde_json::to_vec(&bad).unwrap()).is_err());
        bad = fixture();
        bad["contract"] = "C-AUDIO".into();
        assert!(provider::StrictDocument::parse(&serde_json::to_vec(&bad).unwrap()).is_err());
        let duplicate = String::from_utf8(bytes).unwrap().replacen(
            "\"available\":true",
            "\"available\":true,\"available\":false",
            1,
        );
        assert!(provider::StrictDocument::parse(duplicate.as_bytes()).is_err());
    }
    #[test]
    fn recording_projection_preserves_unknown_durability_and_rejects_false_progress() {
        let mut v = fixture();
        let o = Observation::decode(&v, SHOW, "9").unwrap();
        assert!(
            o.lines()
                .iter()
                .any(|s| s.contains("queued 0 / written 1440"))
        );
        let body = serde_json::json!({"take_id":"new-take","operation_id":"99"});
        assert!(o.admit(true, &body).is_ok());
        assert!(o.admit(false, &body).is_err());
        v["recording"]["durable_frames"] = "1440".into();
        assert!(Observation::decode(&v, SHOW, "9").is_err());
        v = fixture();
        v["recording"]["written_frames"] = "1441".into();
        assert!(Observation::decode(&v, SHOW, "9").is_err());
        v = fixture();
        v["recording"]["state"] = "finalizing".into();
        assert!(Observation::decode(&v, SHOW, "9").is_err());
        v["recording"]["outcome"] = "pending".into();
        assert!(
            Observation::decode(&v, SHOW, "9")
                .unwrap()
                .admit(true, &body)
                .is_err()
        );
    }
    #[test]
    fn dynamic_mapping_and_retired_stop_are_fenced() {
        let mut v = fixture();
        v["version"] = 2.into();
        let mapping = serde_json::json!(
            (1..=16)
                .map(|i| format!("input-{i:02}/raw"))
                .collect::<Vec<_>>()
        );
        v["configuration"]["raw_inputs"] = mapping.clone();
        v["recording"]["mapping"] = mapping;
        v["recording"]["state"] = "recording".into();
        v["recording"]["outcome"] = "pending".into();
        let o = Observation::decode(&v, SHOW, "9").unwrap();
        let r = o.recording.as_ref().unwrap();
        let mut body = serde_json::json!({"take_id":r.take_id,"operation_id":r.operation_id});
        o.admit(false, &body).unwrap();
        body["operation_id"] = "999".into();
        assert!(o.admit(false, &body).is_err());
        assert!(Observation::decode(&v, SHOW, "10").is_err());
    }
}
