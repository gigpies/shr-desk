//! Complete console affordances shared by keyboard fallbacks and controller menus.
use super::*;
use crate::controller_surface::{Command as C, Item};
impl Frontend {
    /// Explicit injected output endpoint. The transport owns any device handle;
    /// this method never discovers or opens hardware. Input remains independent.
    pub fn attach_controller_feedback(
        &mut self,
        endpoint: String,
        protocol: &str,
        output: impl crate::controller_flow::Output + Send + 'static,
    ) -> Result<(), String> {
        let worker = crate::controller_flow::FeedbackWorker::spawn(endpoint, protocol, output)?;
        self.controller_feedback_worker = Some((u64::MAX, worker));
        Ok(())
    }
    pub fn detach_controller_feedback(&mut self) {
        self.controller_feedback_worker = None;
    }
    pub fn controller_feedback_failed(&self) -> bool {
        self.controller_feedback_worker
            .as_ref()
            .is_some_and(|(_, w)| w.failed())
    }

    pub fn import_gate_proposal(&mut self, bytes: &[u8]) -> Result<(), String> {
        if !self.processing_ready() || self.state.as_ref().is_some_and(|u| u.review.is_some()) {
            return Err("gate import requires fresh processing and no review".into());
        }
        let input = self
            .selected_input()
            .ok_or("gate input unavailable")?
            .to_owned();
        let proposal = crate::gate::PreparedDocument::decode(bytes, &input)?;
        let d = self
            .processing_draft
            .as_mut()
            .ok_or("open processing draft first")?;
        if d.input != input || d.generation != self.provider.generation() {
            return Err("gate draft context changed".into());
        }
        d.config.gate = proposal.proposal.config;
        self.message = format!(
            "Gate {:?}, confidence {:.0}% — frozen local proposal; Review/Apply required",
            proposal.proposal.reason,
            proposal.proposal.confidence * 100.
        );
        self.processing_entry.clear();
        self.gate_import = false;
        Ok(())
    }
    /// Adapter boundary for complete messages from the explicitly assigned endpoint.
    /// The caller retains the acquisition generation; old input is never restamped.
    pub fn inject_surface_midi(
        &mut self,
        binding: &crate::controller_surface::MidiBinding,
        endpoint: &str,
        bytes: &[u8],
        generation: u64,
    ) -> Result<(), String> {
        if self.controller_identity.as_deref() != Some(endpoint) {
            return Ok(());
        }
        if !self.provider.authorization.load(Ordering::Acquire) {
            self.controller_surface.lost();
            return Err("controller endpoint has no live assigned role".into());
        }
        if let Some(gesture) = binding.decode(endpoint, bytes) {
            self.enqueue(Event::Gesture {
                gesture,
                generation,
            })?;
        }
        Ok(())
    }

    pub(super) fn console_command(&mut self, command: C) -> Result<(), String> {
        match command {
            C::Dynamics(mode) => {
                self.dynamics_mode = mode;
                self.controller_surface.lost();
                Ok(())
            }

            C::GateMode(mode) => {
                if self.processing_draft.is_none() {
                    self.action(Action::ProcessingEdit)?;
                }
                if !self.processing_ready()
                    || self.state.as_ref().is_some_and(|u| u.review.is_some())
                {
                    return Err("gate requires fresh processing and no review".into());
                }
                let input = self
                    .selected_input()
                    .ok_or("gate input unavailable")?
                    .to_owned();
                let d = self.processing_draft.as_mut().unwrap();
                if d.input != input || d.generation != self.provider.generation() {
                    return Err("gate draft context changed".into());
                }
                match mode {
                    crate::gate::Mode::Manual => {
                        d.config.gate.detector_hz = 0;
                        d.config.gate.bypass = false;
                    }
                    crate::gate::Mode::Tom => {
                        if d.config.gate.detector_hz == 0 {
                            return Err(
                                "Set measured tom detector Hz/Q in Detector menu first".into()
                            );
                        }
                        d.config.gate.bypass = false;
                    }
                    crate::gate::Mode::Automatic => {
                        self.processing_entry.clear();
                        self.gate_import = true;
                        self.message = "Automatic: enter absolute gate-prepare proposal path; Enter imports into local review draft".into();
                    }
                }
                Ok(())
            }
            C::GateImport => {
                use std::io::Read;
                if !self.gate_import || !self.processing_ready() {
                    return Err("automatic gate import unavailable".into());
                }
                let path = std::path::Path::new(&self.processing_entry);
                if !path.is_absolute() {
                    return Err("absolute gate proposal path required".into());
                }
                if !path.metadata().map_err(|e| e.to_string())?.is_file() {
                    return Err("regular gate proposal file required".into());
                }
                let mut bytes = Vec::new();
                std::fs::File::open(path)
                    .map_err(|e| e.to_string())?
                    .take(16 * 1024 + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|e| e.to_string())?;
                self.import_gate_proposal(&bytes)
            }

            C::SelectTarget(target) => self.select_keybed_target(target),
            C::KeybedBank(delta) => {
                let n = self
                    .state
                    .as_ref()
                    .and_then(|s| s.snapshot.as_ref())
                    .map_or(0, |s| s.authority.inputs.len());
                self.keybed_bank = (self.keybed_bank as i64 + i64::from(delta.clamp(-1, 1)))
                    .rem_euclid(n.div_ceil(24).max(1) as i64)
                    as usize;
                Ok(())
            }
            C::Rotary { field, steps } => {
                let index = crate::processing::FIELDS
                    .iter()
                    .position(|f| *f == field)
                    .ok_or("field unavailable")?;
                self.channel_panel_field(index)?;
                self.action(Action::ProcessingAdjust(steps.clamp(-8, 8)))
            }
            C::GuardedLevel { target, steps } => {
                if target != self.controller_target().ok_or("target unavailable")? {
                    return Err("controller target changed".into());
                }
                let t = self.target("fader")?;
                let current = self
                    .state
                    .as_ref()
                    .and_then(|s| s.snapshot.as_ref())
                    .and_then(|s| {
                        s.authority
                            .parameters
                            .iter()
                            .find(|p| crate::audio::target_value(&p.target) == t)
                    })
                    .and_then(|p| p.target_value.as_i64())
                    .ok_or("gain unavailable")?;
                let policy = if target == crate::controller_flow::Target::Master {
                    crate::controller_flow::GainPolicy::MASTER
                } else {
                    crate::controller_flow::GainPolicy::CHANNEL
                };
                let value = policy.steps(current as f64 / 1000., steps);
                // Inputs retain 0.1 dB steps; Master supports milli-dB motion.
                // Never round an upward sensitivity guard result up.
                let scale = if target == crate::controller_flow::Target::Master {
                    1000.
                } else {
                    10.
                };
                let next = if steps > 0 {
                    (value * scale + 1e-9).floor()
                } else {
                    (value * scale - 1e-9).ceil()
                } as i64
                    * (1000. / scale) as i64;
                if next == current {
                    return Ok(());
                }
                self.adjust("fader", next - current, false)
            }

            C::Menu => {
                self.controller_surface
                    .gesture(crate::controller_surface::Gesture::Menu, &[]);
                Ok(())
            }
            C::Grant => self.send(Operation::Grant),
            C::ReleaseWriter => self.send(Operation::ReleaseWriter),
            C::LegacyReconnect => self.reconnect_legacy(),
            C::Reconnect => {
                self.fence();
                self.attachment_fence = Some(self.provider.generation());
                self.provider.send(None, Operation::Reconnect)
            }
            C::ReviewPage(n) => {
                let pages = self.review_lines().len().div_ceil(32).max(1);
                self.review_page = (self.review_page as i64 + i64::from(n.clamp(-1, 1)))
                    .clamp(0, pages as i64 - 1) as usize;
                Ok(())
            }
            C::FullscreenEq => {
                if !self.channel_panel_visible() {
                    return Err("Channel view required".into());
                }
                self.eq_fullscreen = !self.eq_fullscreen;
                Ok(())
            }
            C::AnalysisSource(n) => {
                if !self.channel_panel_visible()
                    || !self.eq_fullscreen
                    || self.analysis_sources.is_empty()
                {
                    return Err("fullscreen RTA sources unavailable".into());
                }
                let at = self
                    .analysis_sources
                    .iter()
                    .position(|s| Some(&s.id) == self.analysis_source.as_ref())
                    .unwrap_or(0);
                let next = (at as i64 + i64::from(n.clamp(-1, 1)))
                    .rem_euclid(self.analysis_sources.len() as i64)
                    as usize;
                self.analysis_source = Some(self.analysis_sources[next].id.clone());
                if let Some(w) = &self.spectrum {
                    w.stop();
                }
                Ok(())
            }
            C::Meters => {
                self.meter_page = !self.meter_page;
                Ok(())
            }
            C::MeterTap => {
                self.meter_processed = !self.meter_processed;
                Ok(())
            }
            C::MeterBank(n) => {
                let banks = self
                    .meter_identity
                    .as_ref()
                    .map_or(0, |i| i.monitors)
                    .div_ceil(12)
                    .max(1);
                self.meter_bus_bank = (self.meter_bus_bank as i64 + i64::from(n.clamp(-1, 1)))
                    .rem_euclid(banks as i64) as usize;
                Ok(())
            }
            C::SendsView(channel) => {
                self.send_cancel();
                self.sends_channel = channel;
                Ok(())
            }
            C::BrainBus(n) => {
                let count = self
                    .state
                    .as_ref()
                    .and_then(|s| s.snapshot.as_ref())
                    .map_or(0, |s| s.authority.monitors.len());
                self.brain_bus = (self.brain_bus as i64 + i64::from(n.clamp(-1, 1)))
                    .clamp(0, count.saturating_sub(1) as i64)
                    as usize;
                Ok(())
            }
            C::EntryAppend(text) => self.controller_text(Some(&text)),
            C::EntryBackspace => self.controller_text(None),
            C::EntryAccept => {
                if self.gate_import {
                    return self.console_command(C::GateImport);
                }

                let action = if let Some(d) = &self.exact_draft {
                    Action::ExactText(d.text.clone())
                } else if let Some(s) = &self.send_entry {
                    Action::SendLevelText(s.clone())
                } else if let Some(s) = &self.device_entry {
                    Action::DeviceText(s.clone())
                } else if self.fx_ui.text.is_some() {
                    Action::Fx(crate::fx::Action::Submit)
                } else if self.measurement_ui.editor.is_some() {
                    Action::Measurement(crate::pa_measurement::editor::Action::Submit)
                } else if self.structural_draft.is_some() {
                    if let Some(path) = self.processing_entry.strip_prefix('@') {
                        Action::StructureImport(crate::structure::read_import(
                            std::path::Path::new(path),
                        )?)
                    } else {
                        Action::StructureText(self.processing_entry.clone())
                    }
                } else if self.processing_draft.is_some() {
                    Action::ProcessingText(self.processing_entry.clone())
                } else {
                    return Err("no detached value editor".into());
                };
                self.action(action)?;
                self.send_entry = None;
                self.device_entry = None;
                Ok(())
            }
            C::ReviewDraft => {
                let a = if self.fx_ui.text.is_some() {
                    Action::Fx(crate::fx::Action::Submit)
                } else if self.measurement_ui.editor.is_some() {
                    Action::Measurement(crate::pa_measurement::editor::Action::Submit)
                } else if self.exact_draft.is_some() {
                    Action::ExactApply
                } else if self.send_draft.is_some() {
                    Action::SendApply
                } else if self.device_draft.is_some() {
                    Action::DeviceApply
                } else if self.structural_draft.is_some() {
                    Action::StructureApply
                } else {
                    Action::ProcessingApply
                };
                self.action(a)
            }
        }
    }
    fn controller_text(&mut self, text: Option<&str>) -> Result<(), String> {
        if text.is_some_and(|s| s.len() > 128 || !s.bytes().all(|c| (32..=126).contains(&c))) {
            return Err("printable entry capacity128".into());
        }
        if self.state.as_ref().is_some_and(|s| s.review.is_some()) {
            return Err("cancel review before editing".into());
        }
        if self.fx_ui.text.is_some() {
            return self.action(Action::Fx(crate::fx::Action::Text(
                text.unwrap_or("\u{8}").into(),
            )));
        }
        if self.measurement_ui.editor.is_some() {
            return self.action(Action::Measurement(
                crate::pa_measurement::editor::Action::Text(text.unwrap_or("\u{8}").into()),
            ));
        }
        let (entry, bound) = if let Some(d) = &mut self.exact_draft {
            if d.review_requested {
                return Err("cancel exact review before editing".into());
            }
            d.value = None;
            (&mut d.text, 16)
        } else if self.send_draft.is_some() {
            (self.send_entry.get_or_insert_with(String::new), 16)
        } else if self.device_draft.is_some() {
            (self.device_entry.get_or_insert_with(String::new), 16000)
        } else if self.structural_draft.is_some() {
            (&mut self.processing_entry, 48 * 1024)
        } else if self.processing_draft.is_some() {
            (
                &mut self.processing_entry,
                if self.gate_import { 1024 } else { 16 },
            )
        } else {
            return Err("open a value editor first".into());
        };
        if let Some(s) = text {
            if entry.len() + s.len() > bound {
                return Err("value entry capacity".into());
            }
            entry.push_str(s);
        } else {
            entry.pop();
        }
        Ok(())
    }
    pub fn controller_scene(&self) -> Scene {
        let mut scene = Scene::default();
        scene.primitives.push(Primitive::Rect {
            x: 0,
            y: 0,
            w: 1920,
            h: 1080,
            fill: "#10151d",
        });
        let items = self.controller_items();
        let selected = self
            .controller_surface
            .selected
            .min(items.len().saturating_sub(1));
        let start = selected / 24 * 24;
        let mut lines =
            vec!["CONTROLS / choose an action; provider availability still applies".to_owned()];
        for (i, item) in items.iter().enumerate().skip(start).take(24) {
            lines.push(format!(
                "{} {}",
                if i == selected { ">" } else { " " },
                item.label
            ));
        }
        lines.push(format!(
            "Entry character: {:?}",
            self.controller_surface.character()
        ));
        let entry = self
            .exact_draft
            .as_ref()
            .map(|d| d.text.as_str())
            .or(self.send_entry.as_deref())
            .or(self.device_entry.as_deref())
            .or(self.fx_ui.text.as_deref())
            .unwrap_or(if self.measurement_ui.editor.is_some() {
                &self.measurement_ui.text
            } else {
                &self.processing_entry
            });
        lines.push(format!(
            "Value: {}",
            entry
                .chars()
                .rev()
                .take(140)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<String>()
        ));
        lines.push(
            "Pads: Previous / Next / Choose / Cancel / Character- / Character+ / Add / Delete"
                .into(),
        );
        lines.push(self.message.clone());
        for (i, value) in lines.into_iter().enumerate() {
            scene.primitives.push(Primitive::Text {
                x: 24,
                y: 24 + i as u32 * 32,
                value: value.chars().take(156).collect(),
                color: "#e4e8e9",
            });
        }
        scene
    }
    /// Visible labels bind to typed actions. Availability is checked by the existing handler.
    pub fn controller_items(&self) -> Vec<Item> {
        use crate::{
            exact_value::Parameter, fx::Action as F, model::Mode,
            pa_measurement::editor::Action as M,
        };
        let mut items = Vec::new();
        let mut add = |label, action| items.push(Item::new(label, action));
        if self.state.as_ref().is_some_and(|s| s.review.is_some()) {
            add("Previous review page", Action::Console(C::ReviewPage(-1)));
            add("Next review page", Action::Console(C::ReviewPage(1)));
            add("Confirm reviewed change", Action::Confirm);
            add("Cancel", Action::Cancel);
            return items;
        }
        add("Request control", Action::Console(C::Grant));
        add("Release control", Action::Console(C::ReleaseWriter));
        add("Reconnect read-only", Action::Console(C::Reconnect));
        add("Mix", Action::Page(Page::Mix));
        add("Channel", Action::Page(Page::Channel));
        add("Health / analysis", Action::Page(Page::Analysis));
        add("Previous input", Action::Move(-1));
        add("Next input", Action::Move(1));
        add(
            "Select Master",
            Action::Console(C::SelectTarget(crate::controller_flow::Target::Master)),
        );
        add("Previous input bank", Action::Bank(-1));
        add("Next input bank", Action::Bank(1));
        add("Accept entered value", Action::Console(C::EntryAccept));
        add("Review draft", Action::Console(C::ReviewDraft));
        add("Cancel", Action::Cancel);
        add("Channel processing", Action::ProcessingEdit);
        add(
            "Compressor rotaries",
            Action::Console(C::Dynamics(crate::controller_flow::Dynamics::Compressor)),
        );
        add(
            "Gate rotaries",
            Action::Console(C::Dynamics(crate::controller_flow::Dynamics::Gate)),
        );
        add(
            "Gate manual",
            Action::Console(C::GateMode(crate::gate::Mode::Manual)),
        );
        add(
            "Gate tom",
            Action::Console(C::GateMode(crate::gate::Mode::Tom)),
        );
        add(
            "Gate automatic import",
            Action::Console(C::GateMode(crate::gate::Mode::Automatic)),
        );

        add("Previous parameter", Action::ProcessingField(-1));
        add("Next parameter", Action::ProcessingField(1));
        add("Decrease parameter", Action::ProcessingAdjust(-1));
        add("Increase parameter", Action::ProcessingAdjust(1));
        add(
            if self.selected_master {
                "Exact Master gain"
            } else {
                "Exact channel level"
            },
            Action::ExactEdit(Parameter::Fader),
        );
        add("Exact pan", Action::ExactEdit(Parameter::Pan));
        add("Mute", Action::Mute);
        add("Hold", Action::Hold);
        add("Release Hold", Action::Release);
        add("Operating mode", Action::ModePicker);
        add("Manual", Action::ChooseMode(Mode::Manual));
        add("Assist", Action::ChooseMode(Mode::Assist));
        add("Auto", Action::ChooseMode(Mode::Auto));
        add("Fullscreen EQ", Action::Console(C::FullscreenEq));
        add(
            "Previous spectrum source",
            Action::Console(C::AnalysisSource(-1)),
        );
        add(
            "Next spectrum source",
            Action::Console(C::AnalysisSource(1)),
        );
        add("Meters", Action::Console(C::Meters));
        add("Raw / processed meters", Action::Console(C::MeterTap));
        add("Previous meter bank", Action::Console(C::MeterBank(-1)));
        add("Next meter bank", Action::Console(C::MeterBank(1)));
        add("Sends", Action::SendsPage);
        add("Sends overview", Action::Console(C::SendsView(false)));
        add("Selected input sends", Action::Console(C::SendsView(true)));
        add("Previous monitor", Action::BrowseMonitor(-1));
        add("Next monitor", Action::BrowseMonitor(1));
        add(
            "Control selected monitor",
            Action::SwitchScope(format!("monitor{}", self.selected_monitor + 1)),
        );
        add("Control FOH", Action::SwitchScope("foh".into()));
        add("Control PA", Action::SwitchScope("pa_configuration".into()));
        add("Send level", Action::SendLevelEdit);
        add("Send tap", Action::SendTapEdit);
        add(
            "Raw / post mute",
            Action::SendTap(crate::sends::Tap::RawPostMute),
        );
        add(
            "Processed / pre fader",
            Action::SendTap(crate::sends::Tap::ProcessedPreFader),
        );
        add(
            "Processed / post fader",
            Action::SendTap(crate::sends::Tap::ProcessedPostFader),
        );
        for (label, a) in [
            ("FX auxes", crate::fx_aux::Action::Open),
            ("Probe saved FX support", crate::fx_aux::Action::SavedProbe),
            (
                "Save FX intent to selected slot",
                crate::fx_aux::Action::Save,
            ),
            ("Load selected saved FX intent", crate::fx_aux::Action::Load),
            (
                "Preview and restore saved FX intent",
                crate::fx_aux::Action::ReviewSaved,
            ),
            (
                "Previous saved FX slot",
                crate::fx_aux::Action::SavedSlot(-1),
            ),
            ("Next saved FX slot", crate::fx_aux::Action::SavedSlot(1)),
            ("Probe FX auxes", crate::fx_aux::Action::Probe),
            (
                "Read legacy FX (routing unavailable)",
                crate::fx_aux::Action::LegacyProbe,
            ),
            ("Control FX auxes", crate::fx_aux::Action::Scope),
            ("Previous FX aux", crate::fx_aux::Action::Bus(-1)),
            ("Next FX aux", crate::fx_aux::Action::Bus(1)),
            ("Previous FX effect", crate::fx_aux::Action::Algorithm(-1)),
            ("Next FX effect", crate::fx_aux::Action::Algorithm(1)),
            ("Previous FX field", crate::fx_aux::Action::Field(-1)),
            ("Next FX field", crate::fx_aux::Action::Field(1)),
            ("Decrease FX field", crate::fx_aux::Action::Adjust(-1)),
            ("Increase FX field", crate::fx_aux::Action::Adjust(1)),
            ("FX aux bypass", crate::fx_aux::Action::Bypass),
            ("Clear FX aux tail", crate::fx_aux::Action::Panic),
        ] {
            add(label, Action::FxAux(a));
        }
        if let Some(text) = self
            .state
            .as_ref()
            .and_then(|u| u.fx_aux.snapshot.as_ref())
            .and_then(|s| s["palette_json"].as_str())
            && let Ok(palette) = serde_json::from_str::<serde_json::Value>(text)
            && let Some(effects) = palette.as_array()
        {
            for effect in effects {
                if let Some(id) = effect["algorithm"].as_u64().filter(|id| *id < 32) {
                    add(
                        crate::fx_aux::name(id),
                        Action::FxAux(crate::fx_aux::Action::ChooseAlgorithm(id as u32)),
                    );
                }
            }
        }
        add("Master EQ", Action::MasterEqEdit);
        add("Live EQ", Action::LiveEqEdit);
        add("EQ channel", Action::MasterEqChannel);
        add("EQ section", Action::MasterEqSection);
        add("Routing", Action::Topology);
        add("Routing editor", Action::StructureEdit);
        add("Previous routing field", Action::StructureField(-1));
        add("Next routing field", Action::StructureField(1));
        add("Decrease routing field", Action::StructureAdjust(-1));
        add("Increase routing field", Action::StructureAdjust(1));
        add("Mute outputs", Action::OutputMute);
        add("Rearm outputs", Action::OutputRearm);
        for (label, a) in [
            ("Legacy stereo delay", F::Open),
            ("Control effects", F::Scope),
            ("Left effect", F::Select(0)),
            ("Right effect", F::Select(1)),
            ("Edit effect", F::Edit),
            ("Effect bypass", F::Bypass),
            ("Clear left tail", F::Panic(1)),
            ("Clear right tail", F::Panic(2)),
            ("Clear both tails", F::Panic(3)),
        ] {
            add(label, Action::Fx(a));
        }
        for (label, a) in [
            ("Recorder", crate::recorder::Action::Open),
            ("Record new take", crate::recorder::Action::Start),
            ("Stop recording", crate::recorder::Action::Stop),
        ] {
            add(label, Action::Record(a));
        }
        for (label, a) in [
            ("Measurement", M::Open),
            ("Capture measurement", M::Capture),
            ("Cancel capture", M::CancelCapture),
            ("Measurement result", M::Result),
            ("Alignment proposal", M::Propose),
            ("Review muted alignment", M::Apply),
            ("Previous measurement page", M::Scroll(-1)),
            ("Next measurement page", M::Scroll(1)),
        ] {
            add(label, Action::Measurement(a));
        }
        if self.brain_enabled {
            add("Operator listening", Action::BrainPage);
            add("Local device setup", Action::DeviceEdit);
            add(
                "Listen to main",
                Action::BrainSource(crate::brain::Source::Main),
            );
            add(
                "Stop listening",
                Action::BrainSource(crate::brain::Source::None),
            );
            add(
                "PFL selected input",
                Action::BrainSource(crate::brain::Source::Pfl {
                    input: self.selected,
                }),
            );
            add(
                "AFL selected input",
                Action::BrainSource(crate::brain::Source::Afl {
                    input: self.selected,
                }),
            );
            add("Previous listening bus", Action::Console(C::BrainBus(-1)));
            add("Next listening bus", Action::Console(C::BrainBus(1)));
            add(
                "Listen to selected bus",
                Action::BrainSource(crate::brain::Source::Monitor {
                    index: self.brain_bus,
                }),
            );
            add("Arm listening", Action::BrainArm);
            add("Dim listening", Action::BrainDim);
            add("Mute listening", Action::BrainMute);
            if let Ok(b) = self.brain_confirmed() {
                add(
                    "Listening quieter",
                    Action::BrainGain((b.monitor_gain_cdb - 100).max(-9000)),
                );
                add(
                    "Listening louder",
                    Action::BrainGain((b.monitor_gain_cdb + 100).min(0)),
                );
                let mut monitors = b.talkback_monitors.clone();
                if let Some(i) = monitors.iter().position(|n| *n == self.brain_bus) {
                    monitors.remove(i);
                } else {
                    monitors.push(self.brain_bus);
                    monitors.sort_unstable();
                }
                add(
                    "Toggle talkback destination",
                    Action::TalkbackConfigure {
                        monitors,
                        gain_cdb: b.talkback_gain_cdb,
                        mute: b.talkback_mute,
                    },
                );
                add(
                    "Talkback mute",
                    Action::TalkbackConfigure {
                        monitors: b.talkback_monitors.clone(),
                        gain_cdb: b.talkback_gain_cdb,
                        mute: !b.talkback_mute,
                    },
                );
                add(
                    "Talkback quieter",
                    Action::TalkbackConfigure {
                        monitors: b.talkback_monitors.clone(),
                        gain_cdb: (b.talkback_gain_cdb - 100).max(-9000),
                        mute: b.talkback_mute,
                    },
                );
                add(
                    "Talkback louder",
                    Action::TalkbackConfigure {
                        monitors: b.talkback_monitors.clone(),
                        gain_cdb: (b.talkback_gain_cdb + 100).min(0),
                        mute: b.talkback_mute,
                    },
                );
                add("Talkback to FOH", Action::TalkbackFoh(!b.talkback_foh));
            }
        }
        items
    }
}

impl Frontend {
    pub fn controller_target(&self) -> Option<crate::controller_flow::Target> {
        if self.selected_master {
            Some(crate::controller_flow::Target::Master)
        } else {
            self.selected_input()
                .map(|id| crate::controller_flow::Target::Input(id.into()))
        }
    }
    fn select_keybed_target(
        &mut self,
        target: crate::controller_flow::Target,
    ) -> Result<(), String> {
        if self.processing_draft.is_some()
            || self.structural_draft.is_some()
            || self.device_draft.is_some()
            || self.send_draft.is_some()
            || self.exact_draft.is_some()
            || self.fx_ui.text.is_some()
            || self.measurement_ui.editor.is_some()
            || self.state.as_ref().is_some_and(|s| s.review.is_some())
        {
            return Err("Finish or cancel the current draft/review before selecting a key".into());
        }
        let index = match &target {
            crate::controller_flow::Target::Master => None,
            crate::controller_flow::Target::Input(id) => Some(
                self.state
                    .as_ref()
                    .and_then(|s| s.snapshot.as_ref())
                    .and_then(|s| s.authority.inputs.iter().position(|i| i == id))
                    .ok_or("input unavailable")?,
            ),
        };
        self.fence();
        self.selected_master = index.is_none();
        if let Some(i) = index {
            self.selected = i;
            self.keybed_bank = i / 24;
        }
        self.message = format!(
            "Selected {:?}; bank {}–{}",
            target,
            self.keybed_bank * 24 + 1,
            self.keybed_bank * 24 + 24
        );
        Ok(())
    }
    /// Attach an explicitly supplied software input profile. This opens no device
    /// and grants no provider authority; role-bound endpoints must match.
    pub fn attach_controller_profile(
        &mut self,
        profile: crate::controller_flow::Profile,
    ) -> Result<crate::controller_flow::Input, String> {
        profile.validate()?;
        if self.role_required
            && self.controller_identity.as_deref() != Some(profile.endpoint.as_str())
        {
            return Err("controller differs from role binding".into());
        }
        let input = crate::controller_flow::Input::new(profile.clone())?;
        self.fence();
        self.controller_identity = Some(profile.endpoint);
        Ok(input)
    }
    pub fn controller_lost(&mut self) {
        self.fence();
        self.controller_identity = None;
    }
    /// Complete-message software transport integration. Caller owns input lifetime and generation.
    pub fn inject_controller_flow(
        &mut self,
        input: &mut crate::controller_flow::Input,
        endpoint: &str,
        bytes: &[u8],
        generation: u64,
        now_ms: u64,
    ) -> Result<(), String> {
        use crate::controller_flow::Intent as I;
        if self.controller_identity.as_deref() != Some(endpoint)
            || generation != self.provider.generation()
            || !self.focused
            || !self.device_ready
            || !self.provider.authorization.load(Ordering::Acquire)
        {
            self.stop_master_motion();
            input.reset(self.provider.generation());
            return Err("controller context unavailable".into());
        }
        let target = self
            .controller_target()
            .ok_or("selected target unavailable")?;
        let Some(intent) = input.decode(
            endpoint,
            bytes,
            generation,
            now_ms,
            target.clone(),
            self.dynamics_mode,
        ) else {
            return Ok(());
        };
        let action = match intent {
            I::Processing { field, steps } => Action::Console(C::Rotary { field, steps }),
            I::Level { target, steps } => Action::Console(C::GuardedLevel { target, steps }),
            I::Pan { target: t, steps }
                if t == target && t != crate::controller_flow::Target::Master =>
            {
                Action::AdjustPan(steps as i16)
            }
            I::SelectMaster => {
                Action::Console(C::SelectTarget(crate::controller_flow::Target::Master))
            }
            I::SelectSlot(slot) => {
                let id = self
                    .state
                    .as_ref()
                    .and_then(|s| s.snapshot.as_ref())
                    .and_then(|s| s.authority.inputs.get(self.keybed_bank * 24 + slot))
                    .ok_or("unavailable key slot")?;
                Action::Console(C::SelectTarget(crate::controller_flow::Target::Input(
                    id.clone(),
                )))
            }
            I::Pad {
                bank: 0,
                index,
                pressed,
            } => {
                return self.enqueue(Event::Gesture {
                    gesture: crate::controller_surface::Gesture::Pad { index, pressed },
                    generation,
                });
            }
            I::Pad {
                bank: 1,
                pressed: true,
                ..
            } => return Ok(()),
            I::Pad {
                bank: 1,
                index,
                pressed: false,
            } => match index {
                0 => Action::Page(Page::Mix),
                1 => Action::Page(Page::Channel),
                2 => Action::SendsPage,
                3 => Action::Console(C::Menu),
                4 => Action::Console(C::Grant),
                5 => Action::Release,
                6 => Action::Cancel,
                7 => Action::Console(C::Reconnect),
                _ => return Ok(()),
            },
            I::Pad { .. } => return Ok(()),
            I::MasterSpeed(position) => {
                if position.abs() <= 0.08 {
                    self.stop_master_motion();
                    self.master_unknown = false;
                    return Ok(());
                }
                if self
                    .master_input
                    .is_some_and(|t| t.elapsed() > Duration::from_millis(250))
                {
                    let pending = self.master_pending.is_some();
                    self.stop_master_motion();
                    self.master_unknown |= pending;
                }
                if self.master_unknown {
                    return Err("Master outcome UNKNOWN; center and fresh readback required".into());
                }
                if !self.master_motion_ready() {
                    self.stop_master_motion();
                    return Err("Master motion requires fresh explicit FOH authority".into());
                }
                let now = std::time::Instant::now();
                if self.master_input.is_none() {
                    self.master_revision = self
                        .state
                        .as_ref()
                        .and_then(|u| u.snapshot.as_ref())
                        .map(|s| s.authority.revision.clone());
                }
                self.master_input = Some(now);
                self.master_speed.input(
                    position,
                    self.master_clock.elapsed().as_millis() as u64,
                    generation,
                    input.profile.pitch_returns_to_center,
                );
                return Ok(());
            }
            I::Pan { .. } => {
                return Err("Master provider gain/pan contract unavailable".into());
            }
        };
        self.enqueue(Event::Controller { action, generation })
    }
}
impl Frontend {
    /// Latest-state mailbox source for the normal menu's eight physical pads.
    pub fn controller_feedback(&self) -> [crate::controller_flow::Colour; 16] {
        use crate::controller_flow::{Colour, PadState, pad_colour};
        let mut colours = [Colour::Off; 16];
        let labels = [
            "Previous",
            "Next",
            "Choose",
            "Cancel",
            "Character−",
            "Character+",
            "Add",
            "Delete",
        ];
        let active = self.focused
            && self.device_ready
            && self.provider.authorization.load(Ordering::Acquire);
        for (i, label) in labels.iter().enumerate() {
            colours[i] = pad_colour(
                label,
                if active {
                    PadState::Navigation
                } else {
                    PadState::Unavailable
                },
            );
        }
        let labels = [
            "Mix",
            "Channel",
            "Sends",
            "Controls",
            "Writer grant",
            "Release Hold",
            "Cancel",
            "Reconnect",
        ];
        for (i, label) in labels.iter().enumerate() {
            colours[8 + i] = pad_colour(
                label,
                if active {
                    PadState::Navigation
                } else {
                    PadState::Unavailable
                },
            );
        }
        if self.controller_surface.open && active {
            colours[2] = Colour::Cyan;
        }
        if active && self.fresh() && self.state.as_ref().is_some_and(|u| u.writer_granted()) {
            colours[12] = Colour::Green;
        }
        if active && self.state.as_ref().is_some_and(|u| u.review.is_some()) {
            colours[14] = Colour::Yellow;
        }
        colours
    }
}

impl Frontend {
    pub(super) fn stop_master_motion(&mut self) {
        self.master_speed.stop();
        self.master_fraction = 0.;
        self.master_input = None;
        self.master_pending = None;
        self.master_revision = None;
        self.master_signal.fetch_add(1, Ordering::AcqRel);
    }
    fn master_motion_ready(&self) -> bool {
        self.focused
            && self.device_ready
            && self.controller_identity.is_some()
            && self.provider.authorization.load(Ordering::Acquire)
            && self.scope == "foh"
            && self.fresh()
            && self.exact_draft.is_none()
            && self.processing_draft.is_none()
            && self.structural_draft.is_none()
            && !self.controller_surface.open
            && self.state.as_ref().is_some_and(|u| {
                u.writer_granted()
                    && u.review.is_none()
                    && !u.last_operation.as_deref().is_some_and(|s| {
                        s.contains("UNCERTAIN")
                            || s.contains("UNKNOWN")
                            || s.contains("READBACK UNAVAILABLE")
                            || (self.master_pending.is_none() && s.starts_with("PENDING"))
                    })
                    && u.snapshot.as_ref().is_some_and(|s| s.master_gain.is_some())
            })
    }
    pub(super) fn pump_master_motion(&mut self) {
        let Some(input) = self.master_input else {
            return;
        };
        if !self.master_motion_ready() || input.elapsed() > Duration::from_millis(250) {
            let unresolved = self.master_pending.is_some();
            self.stop_master_motion();
            if unresolved {
                self.master_unknown = true;
                self.message = "UNKNOWN Master motion completion; stopped".into();
            }
            return;
        }
        if let Some((revision, sent)) = &self.master_pending {
            if sent.elapsed() > Duration::from_millis(250) {
                self.stop_master_motion();
                self.master_unknown = true;
                self.message = "UNKNOWN Master motion completion; stopped".into();
                return;
            }
            if self.provider.pending_edits.load(Ordering::Acquire) != 0 {
                return;
            }
            let u = self.state.as_ref().unwrap();
            if u.last_operation
                .as_deref()
                .is_some_and(|s| s.starts_with("PENDING"))
            {
                return;
            }
            if u.last_operation.as_deref()
                != Some(format!("set applied revision {revision}").as_str())
                || u.snapshot
                    .as_ref()
                    .is_none_or(|s| &s.authority.revision != revision)
            {
                self.stop_master_motion();
                self.master_unknown = true;
                self.message = "Master motion stopped: uncorrelated or concurrent result".into();
                return;
            }
            self.master_revision = Some(revision.clone());
            self.master_pending = None;
        }
        if self.provider.pending_edits.load(Ordering::Acquire) != 0 {
            self.stop_master_motion();
            return;
        }
        let s = self.state.as_ref().unwrap().snapshot.as_ref().unwrap();
        if self.master_revision.as_ref() != Some(&s.authority.revision) {
            self.stop_master_motion();
            self.message = "Master motion stopped: concurrent revision".into();
            return;
        }
        let gain = s.master_gain.as_ref().unwrap();
        if gain.current_nanogain != gain.target_nanogain {
            return;
        }
        let Some(current) = s
            .authority
            .parameters
            .iter()
            .find(|p| p.target.parameter == "master_gain")
            .and_then(|p| p.target_value.as_i64())
        else {
            self.stop_master_motion();
            return;
        };
        let next = self.master_speed.tick(
            current as f64 / 1000.,
            self.master_clock.elapsed().as_millis() as u64,
            self.provider.generation(),
            true,
        );
        if !self.master_speed.is_active() {
            self.stop_master_motion();
            return;
        }
        let next = next + self.master_fraction;
        let value = if next >= current as f64 / 1000. {
            (next * 1000.).floor()
        } else {
            (next * 1000.).ceil()
        } as i64;
        self.master_fraction = next - value as f64 / 1000.;
        if value == current {
            return;
        }
        let Some(revision) = s
            .authority
            .revision
            .parse::<u64>()
            .ok()
            .and_then(|r| r.checked_add(1))
        else {
            self.stop_master_motion();
            return;
        };
        let guard = super::MotionGuard {
            signal: self.master_signal.clone(),
            token: self.master_signal.load(Ordering::Acquire),
            deadline: input + Duration::from_millis(250),
        };
        match self.send(Operation::MasterMotion { value, guard }) {
            Ok(()) => self.master_pending = Some((revision.to_string(), std::time::Instant::now())),
            Err(e) => {
                self.stop_master_motion();
                self.message = e;
            }
        }
    }
}
