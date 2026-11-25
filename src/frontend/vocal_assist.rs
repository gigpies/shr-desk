//! GP-AUTO main-vocal soundcheck: record GP-METER windows of the selected input's
//! processed tap, let GigPies derive the kept level, then use the ordinary reviewed
//! processing write. Desk performs no analysis; the result never applies itself.
use super::*;
use crate::gate_job::{Job, Kind};
use crate::vocal::{Document, MAX_WINDOWS, Reason, Window};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    /// Start recording, or stop and prepare when already recording.
    Soundcheck,
    /// Put the measured settings into a draft and open the protected review.
    Use,
    /// One key: draft the confirmed target with the assist disabled and open review.
    Revert,
    /// Discard the recording or result; nothing is applied.
    Cancel,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Phase {
    #[default]
    Idle,
    Recording,
    Preparing,
    Ready,
}

type Basis = (u64, String, String, String);

#[derive(Default)]
pub struct Soundcheck {
    pub phase: Phase,
    basis: Option<Basis>,
    pub windows: Vec<Window>,
    last_sequence: Option<u64>,
    job: Option<Job>,
    pub document: Option<Document>,
    pub report: Vec<String>,
}
impl Soundcheck {
    fn reset(&mut self) {
        let report = std::mem::take(&mut self.report);
        *self = Self {
            report,
            ..Self::default()
        };
    }
    /// Context loss: drop the recording and any result; nothing was applied.
    pub(super) fn fence(&mut self) {
        if self.phase != Phase::Idle {
            self.reset();
            self.report =
                vec!["Vocal soundcheck lost its context; arm again. Nothing applied.".into()];
        }
    }
    pub fn status(&self) -> String {
        match self.phase {
            Phase::Idle => "V soundcheck".into(),
            Phase::Recording => format!(
                "RECORDING {} windows / {:.1} s · V stop",
                self.windows.len(),
                self.windows.len() as f64 * 0.04
            ),
            Phase::Preparing => "GigPies measuring…".into(),
            Phase::Ready => self.document.as_ref().map_or("no result".into(), |d| {
                if d.proposal.reason == Reason::Prepared {
                    format!(
                        "MEASURED keep {:.1} dBFS · C use · Esc discard",
                        d.proposal.config.reference_mdb as f64 / 1000.
                    )
                } else {
                    format!("REFUSED: {}", d.proposal.reason.operator_text())
                }
            }),
        }
    }
}

/// Absolute GigPies executable: GIGPIES_BIN, the gate preparation path, or PATH.
fn executable(gate: &str) -> Result<PathBuf, String> {
    let candidates = std::env::var_os("GIGPIES_BIN")
        .map(PathBuf::from)
        .into_iter()
        .chain((!gate.is_empty()).then(|| PathBuf::from(gate)))
        .chain(
            std::env::var_os("PATH")
                .map(|p| {
                    std::env::split_paths(&p)
                        .map(|d| d.join("gigpies"))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default(),
        );
    for path in candidates {
        if path.is_absolute() && path.is_file() {
            return Ok(path);
        }
    }
    Err("GigPies executable not found: set GIGPIES_BIN or install gigpies on PATH".into())
}

impl Frontend {
    fn vocal_basis(&self) -> Result<Basis, String> {
        if self.scope != "foh" {
            return Err("vocal assist is a FOH channel setting".into());
        }
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
            self.selected_input()
                .ok_or("select an input channel")?
                .to_owned(),
        ))
    }
    fn vocal_target(&self, input: &str) -> Result<crate::processing::Config, String> {
        self.state
            .as_ref()
            .and_then(|s| s.processing.as_ref())
            .and_then(|s| s.channels.iter().find(|c| c.input == input))
            .map(|c| c.target.clone())
            .ok_or_else(|| "input processing unavailable".into())
    }
    fn vocal_no_draft(&self) -> Result<(), String> {
        if self.state.as_ref().is_some_and(|s| s.review.is_some())
            || self.processing_draft.is_some()
            || self.exact_draft.is_some()
            || self.structural_draft.is_some()
            || self.send_draft.is_some()
            || self.device_draft.is_some()
            || self.mode_picker
            || self.gate_preparation.guarded
        {
            return Err("finish or cancel the current draft/review first".into());
        }
        Ok(())
    }
    /// Called every frontend tick. Records new meter windows; collects the result.
    pub(super) fn vocal_poll(&mut self) {
        let s = &self.vocal_soundcheck;
        if s.phase == Phase::Idle || s.phase == Phase::Ready {
            if s.phase == Phase::Ready && s.basis.as_ref() != self.vocal_basis().ok().as_ref() {
                self.vocal_soundcheck.fence();
            }
            return;
        }
        if s.basis.as_ref() != self.vocal_basis().ok().as_ref() {
            self.vocal_soundcheck.fence();
            return;
        }
        if s.phase == Phase::Recording {
            let now = std::time::Instant::now();
            let Some(snapshot) = self.meters.snapshot.as_ref() else {
                // Identity loss clears the meter cache: this soundcheck is no longer one source.
                self.vocal_soundcheck.fence();
                return;
            };
            let input = s.basis.as_ref().map(|b| b.3.clone()).unwrap_or_default();
            let sequence = snapshot.sequence.0;
            if s.last_sequence.is_some_and(|l| sequence <= l) {
                return;
            }
            let tap = snapshot
                .taps
                .iter()
                .find(|t| t.id == format!("{input}:processed"));
            // Both levels or neither: a partial tap is recorded as unavailable.
            let levels = tap
                .filter(|t| snapshot.valid && t.valid)
                .and_then(|t| t.rms_millidbfs.zip(t.peak_millidbfs));
            let window = Window {
                sequence: sequence.to_string(),
                rms_mdbfs: levels.map(|l| l.0),
                sample_peak_mdbfs: levels.map(|l| l.1),
                silent: tap.is_some_and(|t| t.silent),
                clip_count: tap.map_or(0, |t| t.clip_count.0.min(u32::MAX as u64) as u32),
                age_ms: self
                    .meters
                    .age(now)
                    .unwrap_or(u64::MAX)
                    .min(u32::MAX as u64) as u32,
            };
            let s = &mut self.vocal_soundcheck;
            s.last_sequence = Some(sequence);
            s.windows.push(window);
            if s.windows.len() >= MAX_WINDOWS
                && let Err(e) = self.vocal_command(Command::Soundcheck)
            {
                self.vocal_soundcheck.reset();
                self.vocal_soundcheck.report = vec![format!(
                    "Soundcheck reached its length limit but could not be measured: {e}"
                )];
            }
            return;
        }
        let Some(result) = s.job.as_ref().and_then(Job::poll) else {
            return;
        };
        let s = &mut self.vocal_soundcheck;
        s.job = None;
        let (_, show, epoch, input) = s.basis.clone().unwrap_or_default();
        match result.and_then(|r| Document::decode_for(&r.bytes, &show, &epoch, &input)) {
            Ok(document) => {
                s.report = document.report();
                s.document = Some(document);
                s.phase = Phase::Ready;
            }
            Err(e) => {
                s.reset();
                s.report = vec![format!("Vocal soundcheck failed: {e}. Nothing applied.")];
            }
        }
    }
    pub(super) fn vocal_command(&mut self, command: Command) -> Result<(), String> {
        match (command, self.vocal_soundcheck.phase) {
            (Command::Soundcheck, Phase::Idle | Phase::Ready) => {
                self.vocal_no_draft()?;
                let basis = self.vocal_basis()?;
                let target = self.vocal_target(&basis.3)?;
                if !target.vocal_assist.bypass {
                    return Err(
                        "disable vocal assist (Z) before soundcheck: the reference is measured without reduction"
                            .into(),
                    );
                }
                if !self.meters.fresh(std::time::Instant::now()) {
                    return Err("fresh GP-METER data required (F11 meters)".into());
                }
                let s = &mut self.vocal_soundcheck;
                s.reset();
                s.basis = Some(basis);
                s.phase = Phase::Recording;
                s.report = vec![
                    "Recording: the singer sings the whole loud song; V stops at its end".into(),
                ];
                Ok(())
            }
            (Command::Soundcheck, Phase::Recording) => {
                let s = &self.vocal_soundcheck;
                let (_, show, epoch, input) = s.basis.clone().ok_or("no recording")?;
                let request = serde_json::json!({"version":1,"show_id":show,"epoch":epoch,
                    "input":input,"tap":format!("{input}:processed"),"windows":s.windows});
                let bytes = serde_json::to_vec(&request).map_err(|e| e.to_string())?;
                let job = Job::start_kind(
                    Kind::Vocal,
                    executable(&self.gate_preparation.executable)?,
                    bytes,
                )?;
                let s = &mut self.vocal_soundcheck;
                s.job = Some(job);
                s.phase = Phase::Preparing;
                s.report = vec![format!(
                    "GigPies is measuring {} recorded windows",
                    s.windows.len()
                )];
                Ok(())
            }
            (Command::Soundcheck, Phase::Preparing) => {
                Err("measurement running; Esc cancels".into())
            }
            (Command::Use, Phase::Ready) => {
                self.vocal_no_draft()?;
                let basis = self.vocal_basis()?;
                let s = &self.vocal_soundcheck;
                if s.basis.as_ref() != Some(&basis) {
                    return Err("channel or provider context changed; soundcheck again".into());
                }
                let document = s.document.as_ref().ok_or("no measured result")?;
                if document.proposal.reason != Reason::Prepared {
                    return Err(format!(
                        "refused: {}",
                        document.proposal.reason.operator_text()
                    ));
                }
                let config = document.proposal.config;
                self.action(Action::ProcessingEdit)?;
                let draft = self.processing_draft.as_mut().ok_or("draft unavailable")?;
                draft.config.vocal_assist = config;
                self.action(Action::ProcessingApply)?;
                self.vocal_soundcheck.reset();
                self.vocal_soundcheck.report =
                    vec!["Measured vocal level in review; Enter confirms, Esc cancels".into()];
                Ok(())
            }
            (Command::Use, _) => Err("stop a soundcheck and wait for its result first".into()),
            (Command::Revert, _) => {
                self.vocal_no_draft()?;
                let basis = self.vocal_basis()?;
                let target = self.vocal_target(&basis.3)?;
                if target.vocal_assist.bypass {
                    return Err("vocal assist is already off".into());
                }
                self.action(Action::ProcessingEdit)?;
                let draft = self.processing_draft.as_mut().ok_or("draft unavailable")?;
                draft.config.vocal_assist = target.vocal_assist.reverted();
                self.action(Action::ProcessingApply)?;
                self.message = "Revert vocal assist in review; Enter confirms".into();
                Ok(())
            }
            (Command::Cancel, _) => {
                let s = &mut self.vocal_soundcheck;
                let had = s.phase != Phase::Idle;
                s.reset();
                s.report = vec![if had {
                    "Vocal soundcheck discarded; nothing applied".into()
                } else {
                    String::new()
                }];
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metering::{Counter, Snapshot, Tap};

    fn meter(f: &mut Frontend, sequence: u64, rms: Option<i32>, clip: u64) {
        let input = f.selected_input().unwrap().to_owned();
        let tap = |id: String| Tap {
            id,
            valid: rms.is_some(),
            reason: rms.is_none().then(|| "unavailable".into()),
            peak_millidbfs: rms.map(|r| (r + 9000).min(0)),
            rms_millidbfs: rms,
            silent: false,
            below_floor: false,
            over_range: false,
            clip_count: Counter(clip),
            invalid_count: Counter(0),
        };
        f.meters.snapshot = Some(Snapshot {
            contract: "GP-METER".into(),
            version: 1,
            kind: "meter_snapshot".into(),
            show_id: "s".into(),
            module: "audio".into(),
            source_epoch: Counter(1),
            query_id: Counter(sequence),
            capability_generation: Counter(1),
            map_generation: Counter(1),
            topology: "t".into(),
            sample_rate: 48000,
            inputs: 1,
            monitors: 0,
            sequence: Counter(sequence),
            first_frame: Some(Counter(sequence * 960)),
            end_frame: Some(Counter(sequence * 960 + 960)),
            acquisition_age_ms: Some(Counter(10)),
            publication_loss: Counter(0),
            valid: true,
            reason: None,
            taps: vec![
                tap(format!("{input}:raw")),
                tap(format!("{input}:processed")),
            ],
        });
    }

    #[test]
    fn recording_needs_disabled_assist_fresh_meters_and_fences_on_context_loss() {
        let (mut f, rx) = super::super::processing_tests::brain_surface();
        f.brain_page = false;
        // No fresh meter cache yet.
        assert!(f.vocal_command(Command::Soundcheck).is_err());
        f.vocal_soundcheck.basis = Some(f.vocal_basis().unwrap());
        f.vocal_soundcheck.phase = Phase::Recording;
        for sequence in 1..=5 {
            meter(&mut f, sequence, Some(-20000), 0);
            f.vocal_poll();
            // Repeated windows are not recorded twice.
            f.vocal_poll();
        }
        meter(&mut f, 6, None, 0);
        f.vocal_poll();
        assert_eq!(f.vocal_soundcheck.windows.len(), 6);
        assert_eq!(f.vocal_soundcheck.windows[5].rms_mdbfs, None);
        assert!(
            f.vocal_soundcheck.windows[..5]
                .iter()
                .all(|w| w.rms_mdbfs == Some(-20000))
        );
        // Provider revision change (any reviewed write) keeps the basis; epoch loss fences.
        f.state.as_mut().unwrap().processing.as_mut().unwrap().epoch = "999".into();
        f.vocal_poll();
        assert_eq!(f.vocal_soundcheck.phase, Phase::Idle);
        assert!(f.vocal_soundcheck.windows.is_empty());
        assert!(f.vocal_command(Command::Use).is_err());
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn meter_identity_loss_discards_recording() {
        let (mut f, _rx) = super::super::processing_tests::brain_surface();
        f.brain_page = false;
        f.vocal_soundcheck.basis = Some(f.vocal_basis().unwrap());
        f.vocal_soundcheck.phase = Phase::Recording;
        meter(&mut f, 1, Some(-20000), 0);
        f.vocal_poll();
        f.meters.clear("IDENTITY / UNAVAILABLE");
        f.vocal_poll();
        assert_eq!(f.vocal_soundcheck.phase, Phase::Idle);
    }

    #[test]
    fn refused_result_cannot_be_used_and_revert_requires_enabled_assist() {
        let (mut f, rx) = super::super::processing_tests::brain_surface();
        f.brain_page = false;
        let basis = f.vocal_basis().unwrap();
        f.vocal_soundcheck.basis = Some(basis.clone());
        f.vocal_soundcheck.phase = Phase::Ready;
        f.vocal_soundcheck.document = Some(Document {
            version: 1,
            show_id: basis.1.clone(),
            epoch: basis.2.clone(),
            input: basis.3.clone(),
            tap: format!("{}:processed", basis.3),
            rule: crate::vocal::RULE.into(),
            proposal: crate::vocal::Proposal {
                config: Default::default(),
                reason: Reason::Clipped,
                measurements: None,
            },
        });
        assert!(
            f.vocal_command(Command::Use)
                .unwrap_err()
                .contains("clipped")
        );
        assert!(f.processing_draft.is_none());
        assert!(
            f.vocal_command(Command::Revert)
                .unwrap_err()
                .contains("already off")
        );
        f.vocal_command(Command::Cancel).unwrap();
        assert_eq!(f.vocal_soundcheck.phase, Phase::Idle);
        assert!(rx.try_recv().is_err());
    }

    fn producer_document(input: &str) -> Document {
        // GigPies producer corpus, retargeted to this surface's identity only.
        let mut d: Document = serde_json::from_slice(include_bytes!(
            "../../tests/fixtures/vocal-assist/v1/soundcheck-document.json"
        ))
        .unwrap();
        d.input = input.into();
        d.tap = format!("{input}:processed");
        d
    }

    #[test]
    fn measured_result_and_revert_each_reach_the_protected_review_only() {
        let (mut f, rx) = super::super::processing_tests::brain_surface();
        f.brain_page = false;
        let basis = f.vocal_basis().unwrap();
        let mut document = producer_document(&basis.3);
        document.show_id = basis.1.clone();
        document.epoch = basis.2.clone();
        let measured = document.proposal.config;
        f.vocal_soundcheck.basis = Some(basis.clone());
        f.vocal_soundcheck.phase = Phase::Ready;
        f.vocal_soundcheck.document = Some(document);
        f.vocal_command(Command::Use).unwrap();
        let request = rx.try_recv().unwrap();
        assert!(matches!(&request.operation,
            Operation::ReviewProcessing { input, config }
                if *input == basis.3 && config.vocal_assist == measured));
        assert_eq!(f.vocal_soundcheck.phase, Phase::Idle);
        assert!(f.processing_draft.is_none());
        // Nothing is confirmed by Use itself: only a review was requested.
        assert!(rx.try_recv().is_err());

        // Pretend the provider confirmed it; one key drafts the disabled assist.
        f.state
            .as_mut()
            .unwrap()
            .processing
            .as_mut()
            .unwrap()
            .channels
            .iter_mut()
            .find(|c| c.input == basis.3)
            .unwrap()
            .target
            .vocal_assist = measured;
        f.vocal_command(Command::Revert).unwrap();
        let request = rx.try_recv().unwrap();
        assert!(matches!(&request.operation,
            Operation::ReviewProcessing { config, .. }
                if config.vocal_assist == measured.reverted()));
        // Soundcheck cannot be armed while the assist is enabled.
        assert!(f.vocal_command(Command::Soundcheck).is_err());
    }
}
