//! Operator controls for independently labelled recorded WAV evidence.
use super::*;
use crate::gate_job::{EvidenceFile, Job};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    Open,
    Field(i32),
    Text(String),
    Add,
    Clear,
    Start,
    Cancel,
    Use,
}
#[derive(Default)]
pub struct Preparation {
    pub open: bool,
    pub field: usize,
    pub text: String,
    pub executable: String,
    pub calibration: String,
    pub evaluation: String,
    pub groups: [Vec<PathBuf>; 4],
    job: Option<Job>,
    basis: Option<(u64, String, String, String, String)>,
    document: Option<Vec<u8>>,
    pub(super) files: Vec<EvidenceFile>,
    pub report: Vec<String>,
    pub(super) guarded: bool,
    pub(super) submitted: bool,
    cue: usize,
}
impl Preparation {
    fn invalidate(&mut self) {
        self.job = None;
        self.document = None;
        self.files.clear();
        self.basis = None;
        self.guarded = false;
        self.submitted = false;
    }
    pub(super) fn fence(&mut self) {
        if self.basis.is_some() {
            self.job = None;
            self.document = None;
            // Retain a poisoned guard if already transferred to a draft/review.
            self.basis = None;
            self.report = vec![
                "Context lost: explicitly prepare again; nothing automatically applied".into(),
            ];
        }
    }
}
const LABELS: [&str; 7] = [
    "GigPies executable",
    "Calibration label",
    "Held-out label",
    "Calibration isolated hits",
    "Calibration non-target bleed",
    "Held-out isolated hits",
    "Held-out non-target bleed",
];
impl Frontend {
    fn gate_basis(&self) -> Result<(u64, String, String, String, String), String> {
        if !self.processing_ready() {
            return Err("fresh settled processing required".into());
        }
        let s = self
            .state
            .as_ref()
            .and_then(|s| s.processing.as_ref())
            .ok_or("processing unavailable")?;
        Ok((
            self.provider.generation(),
            s.show_id.clone(),
            s.epoch.clone(),
            s.revision.clone(),
            self.selected_input()
                .ok_or("select an input channel")?
                .to_owned(),
        ))
    }
    pub(super) fn gate_guard(&self) -> Result<(), String> {
        if self.gate_preparation.submitted {
            return Err("gate completion pending; reconcile authoritative readback".into());
        }
        if self.gate_preparation.guarded {
            if self.gate_preparation.basis.as_ref() != Some(&self.gate_basis()?) {
                return Err("gate review context changed; cancel and prepare again".into());
            }
            if !self
                .gate_preparation
                .files
                .iter()
                .all(EvidenceFile::unchanged)
            {
                return Err("gate evidence changed; cancel and prepare again".into());
            }
        }
        Ok(())
    }
    pub(super) fn gate_poll(&mut self) {
        if self.gate_preparation.submitted
            && let Some((_, _, _, revision, input)) = &self.gate_preparation.basis
        {
            let expected = revision
                .parse::<u64>()
                .ok()
                .and_then(|r| r.checked_add(1))
                .map(|r| r.to_string());
            if let Some(u) = &self.state
                && let Some(final_reply) = &u.processing_final
                && expected.as_ref() == Some(&final_reply.revision)
                && final_reply.reason.is_none()
                && final_reply.ticket.is_some()
                && u.last_operation.as_deref().is_some_and(|s| {
                    s.starts_with(&format!(
                        "processing_set applied revision {};",
                        final_reply.revision
                    ))
                })
                && self.processing_ready()
                && let Some(snapshot) = &u.processing
                && snapshot.revision == final_reply.revision
                && let Some(channel) = snapshot.channels.iter().find(|c| &c.input == input)
                && channel.ready
                && channel.current == channel.target
            {
                self.gate_preparation.guarded = false;
                self.gate_preparation.submitted = false;
                self.message = format!(
                    "Gate applied at frame {:?}; correlated revision {}, settled authoritative readback. Save with Audio setup.",
                    final_reply.effective_frame, final_reply.revision
                );
            }
        }

        let Some(result) = self.gate_preparation.job.as_ref().and_then(Job::poll) else {
            return;
        };
        self.gate_preparation.job = None;
        let result = result.and_then(|result| {
            if self.gate_preparation.basis.as_ref() != Some(&self.gate_basis()?) { return Err("channel/provider context changed during preparation".into()); }
            let input = self.selected_input().ok_or("input unavailable")?;
            let p = crate::gate::PreparedDocument::decode(&result.bytes, input)?;
            let mut lines = vec![format!("{:?} | confidence {:.1}% | bypass {}", p.proposal.reason, p.proposal.confidence * 100., p.proposal.config.bypass)];
            if let Some(m) = &p.proposal.measurements {
                lines.push(format!("Quiet hit {:.2} dB / bleed p95 {:.2} dB / separation {:.2} dB / decay {:.0} ms", m.quiet_hit_db, m.bleed_p95_db, m.quiet_hit_db - m.bleed_p95_db, m.decay_p90_ms));
                lines.push(format!("Held-out summary: {}/{} misses, {}/{} false openings", m.evaluation_missed_hits, m.evaluation_hits, m.evaluation_false_openings, m.evaluation_bleed_windows));
            }
            if let Some(e) = &p.causal_evaluation { lines.push(format!("Causal preservation: {}/{} misses, {}/{} false openings; worst energy {:.3} dB", e.missed_hits, e.hits, e.false_opening_passages, e.bleed_passages, e.worst_hit_energy_change_db)); }
            lines.push(format!("Proposed detector {} Hz Q {:.2}; threshold {:.2} dB; attack {} us", p.proposal.config.detector_hz, p.proposal.config.detector_q_milli as f64 / 1000., p.proposal.config.threshold_mdb as f64 / 1000., p.proposal.config.attack_us));
            lines.push(format!("Hold {} ms / release {} ms / range {:.1} dB / hysteresis {:.1} dB", p.proposal.config.hold_ms, p.proposal.config.release_ms, p.proposal.config.range_mdb as f64 / 1000., p.proposal.config.hysteresis_mdb as f64 / 1000.));
            let current = self.state.as_ref().and_then(|s| s.processing.as_ref()).and_then(|s| s.channels.iter().find(|c| c.input == input)).ok_or("input disappeared")?.target.gate;
            lines.push(format!("Current: bypass {} detector {} Hz threshold {:.2} dB hold {} release {} ms", current.bypass, current.detector_hz, current.threshold_mdb as f64 / 1000., current.hold_ms, current.release_ms));
            if p.proposal.config.bypass { lines.push("REFUSED: keep gate bypassed. Inspect labels and evidence; no evaluation-driven retuning.".into()); }
            self.gate_preparation.report = lines;
            self.gate_preparation.files = result.files;
            self.gate_preparation.document = Some(result.bytes);
            Ok(())
        });
        if let Err(e) = result {
            self.gate_preparation.invalidate();
            self.gate_preparation.report = vec![format!("Preparation failed: {e}")];
        }
    }
    pub(super) fn gate_command(&mut self, c: Command) -> Result<(), String> {
        match c {
            Command::Open => {
                if self.page != Page::Channel
                    || self.scope != "foh"
                    || self.brain_page
                    || self.sends_page
                    || self.live_page
                    || self.topology_page.is_some()
                {
                    return Err("open Channel in FOH before gate preparation".into());
                }
                if self.gate_preparation.guarded {
                    return Err("finish or cancel prepared gate review first".into());
                }
                if self.state.as_ref().is_some_and(|s| s.review.is_some())
                    || self.processing_draft.is_some()
                    || self.exact_draft.is_some()
                    || self.structural_draft.is_some()
                    || self.send_draft.is_some()
                    || self.device_draft.is_some()
                    || self.mode_picker
                {
                    return Err("finish or cancel current draft/review before preparation".into());
                }
                self.gate_basis()?;
                self.gate_preparation.open = true;
                self.controller_surface.lost();
            }
            Command::Cancel => {
                let submitted = self.gate_preparation.submitted;
                self.gate_preparation.invalidate();
                self.gate_preparation.report = vec![if submitted {
                    "Submitted operation is not undone; inspect authoritative completion/readback"
                        .into()
                } else {
                    "Preparation cancelled; no settings applied".into()
                }];
            }
            Command::Use => {
                let p = &self.gate_preparation;
                if p.basis.as_ref() != Some(&self.gate_basis()?)
                    || !p.files.iter().all(EvidenceFile::unchanged)
                {
                    return Err("evidence or channel context changed; prepare again".into());
                }
                let bytes = p.document.clone().ok_or("completed preparation required")?;
                self.action(Action::ProcessingEdit)?;
                self.import_gate_proposal(&bytes)?;
                self.gate_preparation.guarded = true;
                self.gate_preparation.open = false;
            }
            Command::Start => {
                self.gate_preparation.invalidate();
                let basis = self.gate_basis()?;
                let p = &mut self.gate_preparation;
                if !p.text.is_empty() {
                    return Err("accept or clear entry before preparing".into());
                }
                if p.calibration.is_empty()
                    || p.evaluation.is_empty()
                    || p.calibration == p.evaluation
                    || p.groups.iter().any(|g| !(8..=32).contains(&g.len()))
                {
                    return Err("distinct labels and 8..32 independent WAV cues in each of four groups required".into());
                }
                let request = json!({"version":2,"input":basis.4,"calibration_id":p.calibration,"evaluation_id":p.evaluation,"hits":p.groups[0],"bleed":p.groups[1],"evaluation_hits":p.groups[2],"evaluation_bleed":p.groups[3]});
                p.job = Some(Job::start(
                    p.executable.clone().into(),
                    serde_json::to_vec(&request).map_err(|e| e.to_string())?,
                )?);
                p.basis = Some(basis);
                p.report = vec![
                    "Preparing read-only evidence; parameters selected from calibration only"
                        .into(),
                ];
            }
            Command::Field(delta) => {
                self.gate_preparation.field =
                    (self.gate_preparation.field as i32 + delta).rem_euclid(7) as usize;
                self.gate_preparation.text.clear();
            }
            Command::Text(s) => {
                if s == "\u{8}" {
                    self.gate_preparation.text.pop();
                } else if s.len() + self.gate_preparation.text.len() <= 1024
                    && !s.chars().any(char::is_control)
                {
                    self.gate_preparation.text.push_str(&s);
                } else {
                    return Err("printable path/label up to 1024 bytes required".into());
                }
            }
            Command::Clear => {
                let p = &mut self.gate_preparation;
                p.invalidate();
                p.text.clear();
                match p.field {
                    0 => p.executable.clear(),
                    1 => p.calibration.clear(),
                    2 => p.evaluation.clear(),
                    n => p.groups[n - 3].clear(),
                }
            }
            Command::Add => {
                let p = &mut self.gate_preparation;
                if p.text.is_empty() {
                    return Err("enter a value first".into());
                }
                p.invalidate();
                match p.field {
                    0 => p.executable = p.text.clone(),
                    1 | 2 => {
                        if p.text.len() > 128 {
                            return Err("label capacity 128".into());
                        }
                        if p.field == 1 {
                            p.calibration = p.text.clone();
                        } else {
                            p.evaluation = p.text.clone();
                        }
                    }
                    n => {
                        let path = PathBuf::from(&p.text);
                        if !path.is_absolute() {
                            return Err("absolute WAV file or cue directory required".into());
                        }
                        let mut paths = Vec::new();
                        if path.is_dir() {
                            for (index, entry) in std::fs::read_dir(&path)
                                .map_err(|e| e.to_string())?
                                .take(129)
                                .enumerate()
                            {
                                if index == 128 {
                                    return Err("cue directory exceeds 128 entries; select individual files or a smaller directory".into());
                                }
                                let path = entry.map_err(|e| e.to_string())?.path();
                                if path
                                    .extension()
                                    .is_some_and(|e| e.eq_ignore_ascii_case("wav"))
                                {
                                    paths.push(path);
                                }
                                if paths.len() > 32 {
                                    return Err("select at most 32 WAV cues per group".into());
                                }
                            }
                        } else {
                            paths.push(path);
                        }
                        paths.sort();
                        if paths.is_empty() || paths.iter().any(|p| !p.is_file()) {
                            return Err("regular local WAV cues required".into());
                        }
                        let mut group = p.groups[n - 3].clone();
                        for path in paths {
                            if !group.contains(&path) {
                                group.push(path);
                            }
                        }
                        if group.len() > 32 {
                            return Err("group capacity 32; previous selection retained".into());
                        }
                        p.groups[n - 3] = group;
                    }
                }
                p.text.clear();
            }
        }
        Ok(())
    }
    pub(super) fn gate_key(&mut self, key: &str) -> Result<(), String> {
        if key == "F10" {
            return self.console_command(crate::controller_surface::Command::Menu);
        }
        let command = match key {
            "Left" => {
                self.gate_preparation.cue = self.gate_preparation.cue.saturating_sub(1);
                return Ok(());
            }
            "Right" => {
                self.gate_preparation.cue = (self.gate_preparation.cue + 1).min(31);
                return Ok(());
            }
            "F9" => {
                let p = &mut self.gate_preparation;
                if p.field >= 3 && p.cue < p.groups[p.field - 3].len() {
                    p.invalidate();
                    p.groups[p.field - 3].remove(p.cue);
                }
                return Ok(());
            }
            "Tab" | "Down" => Command::Field(1),
            "Up" => Command::Field(-1),
            "Enter" => Command::Add,
            "Delete" => Command::Clear,
            "Backspace" => Command::Text("\u{8}".into()),
            "F6" => Command::Start,
            "F7" => Command::Cancel,
            "F8" => Command::Use,
            "Esc" | "Escape" => {
                self.gate_preparation.open = false;
                return self.gate_command(Command::Cancel);
            }
            s if s.chars().count() == 1 => Command::Text(s.into()),
            _ => return Ok(()),
        };
        self.gate_command(command)
    }
    pub(super) fn gate_scene(&self) -> Scene {
        let p = &self.gate_preparation;
        let mut lines = vec![format!("GATE SOUNDCHECK / {} / recorded pre-gate WAV cues", self.selected_input().unwrap_or("unavailable")), "Use isolated target hits with full tails and separately labelled non-target bleed. No mixed-source identification.".into(), "Tab/Up/Down select field; type absolute file/directory or label; Enter adds/replaces; Delete clears selected field.".into()];
        for (i, label) in LABELS.iter().enumerate() {
            let value = match i {
                0 => p.executable.clone(),
                1 => p.calibration.clone(),
                2 => p.evaluation.clone(),
                n => format!(
                    "{} cues: {}",
                    p.groups[n - 3].len(),
                    p.groups[n - 3]
                        .iter()
                        .map(|x| x.file_name().unwrap_or_default().to_string_lossy())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            };
            lines.push(format!(
                "{} {label}: {value}",
                if p.field == i { ">" } else { " " }
            ));
        }
        if p.field >= 3 {
            lines.push(format!(
                "Cue {}: {} | Left/Right inspect, F9 remove",
                p.cue + 1,
                p.groups[p.field - 3]
                    .get(p.cue)
                    .map_or_else(|| "none".into(), |x| x.display().to_string())
            ));
        }
        lines.push(format!("Entry: {}", p.text));
        lines.push("F6 Prepare | F7 Cancel job | F8 Use result in local draft | Esc Close | then F4 Review, Enter Confirm".into());
        if let Some(job) = &p.job {
            lines.push(format!(
                "Analysed {} / {} cues; final held-out causal check follows; 120 second limit",
                job.progress.load(Ordering::Acquire),
                p.groups.iter().map(Vec::len).sum::<usize>()
            ));
        }
        lines.extend(p.report.iter().cloned());
        lines.push(self.message.clone());
        let mut scene = Scene::default();
        scene.primitives.push(Primitive::Rect {
            x: 0,
            y: 0,
            w: 1920,
            h: 1080,
            fill: "#10151d",
        });
        for (i, value) in lines.into_iter().enumerate() {
            scene.primitives.push(Primitive::Text {
                x: 24,
                y: 20 + i as u32 * 36,
                value: value.chars().take(154).collect(),
                color: "#e0e6ee",
            });
        }
        scene
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn evidence_controls_identity_refusal_and_cancel_are_local() {
        let (mut f, rx) = super::super::processing_tests::brain_surface();
        f.brain_page = false;
        f.gate_command(Command::Open).unwrap();
        assert!(f.scene().in_bounds());
        assert!(
            !f.channel_panel_visible(),
            "terminal must render the preparation controls"
        );
        assert!(
            f.controller_items()
                .iter()
                .any(|i| i.label == "Prepare recorded evidence")
        );
        f.gate_command(Command::Field(1)).unwrap();
        f.gate_command(Command::Text("train".into())).unwrap();
        f.gate_command(Command::Add).unwrap();
        f.gate_command(Command::Field(1)).unwrap();
        f.gate_command(Command::Text("train".into())).unwrap();
        f.gate_command(Command::Add).unwrap();
        assert!(f.gate_command(Command::Start).is_err());
        assert!(f.gate_command(Command::Use).is_err());
        f.gate_command(Command::Cancel).unwrap();
        assert!(f.processing_draft.is_none());
        assert!(rx.try_recv().is_err());
    }
    #[test]
    fn prepared_guard_fences_changed_revision_generation_and_reconnect() {
        let (mut f, rx) = super::super::processing_tests::brain_surface();
        f.gate_preparation.basis = Some(f.gate_basis().unwrap());
        f.gate_preparation.guarded = true;
        f.gate_guard().unwrap();
        f.state
            .as_mut()
            .unwrap()
            .processing
            .as_mut()
            .unwrap()
            .revision = "999".into();
        assert!(f.gate_guard().is_err());
        f.gate_preparation.fence();
        assert!(f.gate_guard().is_err());
        assert!(f.action(Action::ProcessingAdjust(1)).is_err());
        assert!(rx.try_recv().is_err());
    }
}
