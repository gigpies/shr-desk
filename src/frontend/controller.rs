//! Complete console affordances shared by keyboard fallbacks and controller menus.
use super::*;
use crate::controller_surface::{Command as C, Item};
impl Frontend {
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
            (&mut self.processing_entry, 16)
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
        add("Previous input bank", Action::Bank(-1));
        add("Next input bank", Action::Bank(1));
        add("Accept entered value", Action::Console(C::EntryAccept));
        add("Review draft", Action::Console(C::ReviewDraft));
        add("Cancel", Action::Cancel);
        add("Channel processing", Action::ProcessingEdit);
        add("Previous parameter", Action::ProcessingField(-1));
        add("Next parameter", Action::ProcessingField(1));
        add("Decrease parameter", Action::ProcessingAdjust(-1));
        add("Increase parameter", Action::ProcessingAdjust(1));
        add("Exact channel level", Action::ExactEdit(Parameter::Fader));
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
            ("Effects", F::Open),
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
