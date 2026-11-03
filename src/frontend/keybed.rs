use super::*;
use shr_instrument_panel::{
    keybed::{Flag, Key},
    terminal::ratatui::{buffer::Buffer, layout::Rect},
};
impl Frontend {
    pub fn draw_keybed(&self, b: &mut Buffer) -> Vec<crate::channel_panel::Hit> {
        if self.controller_surface.open
            || self.state.as_ref().is_some_and(|s| s.review.is_some())
            || b.area.width < 160
            || b.area.height < 49
        {
            return Vec::new();
        }
        let area = Rect::new(0, b.area.height - 4, 100, 3);
        let inputs = self
            .state
            .as_ref()
            .and_then(|s| s.snapshot.as_ref())
            .map(|s| s.authority.inputs.as_slice())
            .unwrap_or(&[]);
        let mut hits = Vec::new();
        let keys = std::array::from_fn(|slot| {
            let master = slot == 24;
            let index = self.keybed_bank * 24 + slot;
            let id = if master { None } else { inputs.get(index) };
            let available = master || id.is_some();
            let selected = if master {
                self.selected_master
            } else {
                !self.selected_master && index == self.selected
            };
            let mut recording = Flag::Unknown;
            let mut mute = Flag::Unknown;
            if let Some(id) = id {
                if self.fresh() {
                    mute = self
                        .state
                        .as_ref()
                        .and_then(|u| u.snapshot.as_ref())
                        .filter(|s| !s.faulted)
                        .and_then(|s| s.coefficients.iter().find(|c| c.input == *id))
                        .filter(|c| c.current_nanogain[3] == c.ramp_target_nanogain[3])
                        .map_or(Flag::Unknown, |c| match c.current_nanogain[3].0 {
                            0 => Flag::On,
                            1_000_000_000 => Flag::Off,
                            _ => Flag::Unknown,
                        });
                }
                if let Some(u) = self
                    .state
                    .as_ref()
                    .filter(|u| self.fresh() && u.recorder.fresh())
                    && let Some(o) = u.recorder.observation.as_ref().filter(|o| o.available)
                {
                    recording = if o.source_fault != 0 {
                        Flag::Fault
                    } else if let Some(r) = o.recording.as_ref() {
                        if r.source_epoch
                            != u.snapshot
                                .as_ref()
                                .and_then(|s| s.clock.as_ref())
                                .map_or_else(|| o.epoch.clone(), |c| c.epoch.to_string())
                        {
                            Flag::Unknown
                        } else if !r.mapping.contains(&format!("{id}/raw")) {
                            Flag::Off
                        } else if r.writer_fault
                            || r.host_fault != 0
                            || matches!(r.outcome.as_str(), "error" | "incomplete")
                            || r.dropped_frames != "0"
                            || r.gap_frames != "0"
                            || r.invalid_blocks != "0"
                        {
                            Flag::Fault
                        } else {
                            match r.state.as_str() {
                                "recording" => Flag::On,
                                "preparing" => Flag::Preparing,
                                "finalizing" => Flag::Finalizing,
                                "finalized" => Flag::Off,
                                _ => Flag::Unknown,
                            }
                        }
                    } else {
                        Flag::Off
                    };
                }
            }
            if available {
                hits.push(crate::channel_panel::Hit {
                    area: Rect::new(slot as u16 * 4, area.y, 4, 3),
                    target: crate::channel_panel::Target::Action(Action::Console(
                        crate::controller_surface::Command::SelectTarget(if master {
                            crate::controller_flow::Target::Master
                        } else {
                            crate::controller_flow::Target::Input(id.unwrap().clone())
                        }),
                    )),
                });
            }
            Key {
                number: index + 1,
                available,
                selected,
                recording,
                solo: Flag::Unknown,
                mute,
            }
        });
        shr_instrument_panel::keybed::draw(area, b, &keys);
        let identity = if self.selected_master {
            "Master"
        } else {
            self.selected_input().unwrap_or("unavailable")
        };
        b.set_stringn(
            101,
            area.y,
            format!(
                "{} | {}–{} / {}",
                identity,
                self.keybed_bank * 24 + 1,
                (self.keybed_bank * 24 + 24).min(inputs.len()),
                inputs.len()
            ),
            b.area.width.saturating_sub(101) as usize,
            shr_instrument_panel::terminal::ratatui::style::Style::default(),
        );
        for (row, label) in [
            (1, "R REC | S unavailable | M mute"),
            (2, "PgUp/PgDn banks | MIDI bindings unqualified"),
        ] {
            b.set_stringn(
                101,
                area.y + row,
                label,
                b.area.width.saturating_sub(101) as usize,
                shr_instrument_panel::terminal::ratatui::style::Style::default(),
            );
        }
        hits
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn identities_master_unavailable_and_stale_flags() {
        let (mut f, _rx) = super::super::processing_tests::brain_surface();
        let ids = f
            .state
            .as_ref()
            .unwrap()
            .snapshot
            .as_ref()
            .unwrap()
            .authority
            .inputs
            .clone();
        let mut b = Buffer::empty(Rect::new(0, 0, 160, 49));
        let hits = f.draw_keybed(&mut b);
        assert_eq!(hits.len(), ids.len().min(24) + 1);
        assert_eq!(b[(97, 45)].symbol(), "M");
        assert_eq!(b[(96, 46)].symbol(), "?"); // no Master recording contract
        assert_eq!(b[(1, 46)].symbol(), "?"); // no solo provider contract
        assert!(hits.iter().any(|h|matches!(&h.target,crate::channel_panel::Target::Action(Action::Console(crate::controller_surface::Command::SelectTarget(crate::controller_flow::Target::Input(id)))) if id==&ids[0])));
        f.action(Action::Console(
            crate::controller_surface::Command::SelectTarget(
                crate::controller_flow::Target::Master,
            ),
        ))
        .unwrap();
        assert!(f.selected_input().is_none());
        assert!(f.target("fader").is_err());
        f.draw_keybed(&mut b);
        assert_eq!(b[(96, 45)].symbol(), ">");
        f.keybed_bank = 100;
        assert_eq!(f.draw_keybed(&mut b).len(), 1);
        assert_eq!(b[(1, 45)].symbol(), "-");
    }
    #[test]
    fn capacity_banks_and_selected_fifteen_keep_provider_ids() {
        for count in [16usize, 24, 32, 48] {
            let (mut f, _rx) = super::super::processing_tests::brain_surface();
            // Projection fixture, not an accepted hardware capacity or provider capture.
            f.state
                .as_mut()
                .unwrap()
                .snapshot
                .as_mut()
                .unwrap()
                .authority
                .inputs = (1..=count).map(|i| format!("input-{i:02}")).collect();
            f.selected = 14;
            let mut b = Buffer::empty(Rect::new(0, 0, 160, 49));
            assert_eq!(f.draw_keybed(&mut b).len(), count.min(24) + 1);
            assert_eq!(b[(56, 45)].symbol(), ">");
            assert_eq!(f.selected_input(), Some("input-15"));
            f.keybed_bank = 1;
            let hits = f.draw_keybed(&mut b);
            assert_eq!(hits.len(), count.saturating_sub(24).min(24) + 1);
            if count > 24 {
                assert!(matches!(&hits[0].target,
                    crate::channel_panel::Target::Action(Action::Console(
                        crate::controller_surface::Command::SelectTarget(
                            crate::controller_flow::Target::Input(id)))) if id == "input-25"));
            }
            assert!(matches!(
                &hits.last().unwrap().target,
                crate::channel_panel::Target::Action(Action::Console(
                    crate::controller_surface::Command::SelectTarget(
                        crate::controller_flow::Target::Master
                    )
                ))
            ));
        }
    }
    #[test]
    fn recording_membership_mute_and_staleness_are_independent() {
        let (mut f, _rx) = super::super::processing_tests::brain_surface();
        let mut v: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/gp05/v1/status-finalized.json"
        ))
        .unwrap();
        v["recording"]["state"] = json!("recording");
        v["recording"]["outcome"] = json!("pending");
        let o =
            crate::recorder::Observation::decode(&v, "11111111-1111-4111-8111-111111111111", "9")
                .unwrap();
        let u = f.state.as_mut().unwrap();
        u.recorder = crate::recorder::View {
            observation: Some(o),
            received: Some(Instant::now()),
        };
        u.received = Instant::now();
        let c = u
            .snapshot
            .as_mut()
            .unwrap()
            .coefficients
            .iter_mut()
            .find(|c| c.input == "input-01")
            .unwrap();
        c.current_nanogain[3] = crate::audio::Nanogain(0);
        c.ramp_target_nanogain[3] = crate::audio::Nanogain(0);
        let mut b = Buffer::empty(Rect::new(0, 0, 160, 49));
        f.draw_keybed(&mut b);
        assert_eq!(b[(0, 46)].symbol(), "R");
        assert_eq!(b[(1, 46)].symbol(), "?");
        assert_eq!(b[(2, 46)].symbol(), "M");
        f.state.as_mut().unwrap().recorder.received = None;
        f.draw_keybed(&mut b);
        assert_eq!(b[(0, 46)].symbol(), "?");
        assert_eq!(b[(2, 46)].symbol(), "M");
    }
}
