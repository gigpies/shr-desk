use super::*;
impl Frontend {
    pub(super) fn record_action(&mut self, a: crate::recorder::Action) -> Result<(), String> {
        use crate::recorder::{Action as A, Operation as O};
        if a == A::Open {
            if self.processing_draft.is_some()
                || self.structural_draft.is_some()
                || self.device_draft.is_some()
                || self.send_draft.is_some()
                || self.fx_ui.text.is_some()
                || self.measurement_ui.editor.is_some()
                || self.state.as_ref().is_some_and(|u| u.review.is_some())
            {
                return Err("Cancel existing editor/review before opening Recorder".into());
            }
            self.talkback_release();
            self.sends_page = false;
            self.brain_page = false;
            self.live_page = false;
            self.topology_page = None;
            self.meter_page = false;
            self.eq_fullscreen = false;
            self.mode_picker = false;
            self.record_page = true;
            self.fx_ui.open = false;
            self.measurement_ui.open = false;
            return self.send(Operation::Record(O::Probe));
        }
        let u = self.state.as_ref().ok_or("REC provider absent")?;
        if !self.fresh()
            || !u.recorder.fresh()
            || u.review.is_some()
            || self.provider.pending_edits.load(Ordering::Acquire) != 0
        {
            return Err("REC fresh readback and no pending review required".into());
        }
        let o = u.recorder.observation.as_ref().ok_or("REC unavailable")?;
        let start = a == A::Start;
        let body = if start {
            let n = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_nanos();
            let id = u64::try_from(n).map_err(|_| "REC identity exhausted")?;
            json!({"take_id":format!("take-{}-{id}",std::process::id()),"operation_id":id.to_string()})
        } else {
            let r = o.recording.as_ref().ok_or("REC no take")?;
            json!({"take_id":r.take_id,"operation_id":r.operation_id})
        };
        o.admit(start, &body)?;
        self.send(Operation::Record(O::Review { start, body }))
    }
    pub(super) fn record_scene(&self) -> Scene {
        let mut scene = Scene::default();
        scene.primitives.push(Primitive::Rect {
            x: 0,
            y: 0,
            w: 1920,
            h: 1080,
            fill: "#10151d",
        });
        let mut lines=vec!["RAW RECORDER / source-clock aligned tracks".into(),"J readback | N new take | S stop | G grant FOH | Q release writer | F5 reconnect read-only".into(),"Start and Stop require review then Enter. Esc cancels unsent intent. Leaving Desk does not stop recording.".into()];
        if let Some(u) = &self.state {
            lines.push(format!(
                "Scope {} / lease {:?} ms / readback {}",
                self.scope,
                u.writer_lease_remaining_ms,
                if u.recorder.fresh() && self.fresh() {
                    "FRESH"
                } else {
                    "STALE / UNKNOWN; controls disabled"
                }
            ));
            if let Some(o) = &u.recorder.observation {
                if !u.recorder.fresh() || !self.fresh() {
                    lines.push(
                        "LAST OBSERVED TAKE ONLY / current capture and outcome UNKNOWN".into(),
                    );
                }
                lines.extend(o.lines());
            }
            if let Some(r) = &u.last_operation {
                lines.push(r.clone());
            }
        }
        lines.push("Storage capacity/time remaining UNKNOWN. Files: provider private takes/<take-id>/. Paths are never selected by Desk. Storage reserve and write/sync failures are enforced by REC.".into());
        lines.push(self.message.clone());
        for (i, line) in lines
            .iter()
            .flat_map(|l| {
                l.chars()
                    .collect::<Vec<_>>()
                    .chunks(150)
                    .map(|c| c.iter().collect::<String>())
                    .collect::<Vec<_>>()
            })
            .take(40)
            .enumerate()
        {
            scene.primitives.push(Primitive::Text {
                x: 24,
                y: 16 + i as u32 * 25,
                value: line,
                color: "#e4e8e9",
            });
        }
        scene
    }
}
