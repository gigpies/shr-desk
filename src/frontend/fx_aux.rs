use super::*;
use crate::fx_aux::{Action as A, Operation as O};
impl Frontend {
    pub(super) fn fx_aux_action(&mut self, a: A) -> Result<(), String> {
        match a {
            A::SavedSlot(d) => {
                self.fx_aux_ui.saved_slot =
                    (self.fx_aux_ui.saved_slot as i32 + i32::from(d)).rem_euclid(16) as usize;
                Ok(())
            }
            A::SavedProbe => self.send(Operation::FxAux(O::SavedProbe)),
            A::Save | A::Load => {
                let directory = self
                    .fx_aux_ui
                    .directory
                    .clone()
                    .or_else(|| {
                        std::env::var_os("SHR_DESK_FX_SHOW_DIRECTORY").map(std::path::PathBuf::from)
                    })
                    .ok_or(
                        "Configure SHR_DESK_FX_SHOW_DIRECTORY to an owned absolute 0700 directory",
                    )?;
                let slot = self.fx_aux_ui.saved_slot;
                self.send(Operation::FxAux(if a == A::Save {
                    O::Save(directory, slot)
                } else {
                    O::Load(directory, slot)
                }))
            }
            A::ReviewSaved => {
                if self.state.as_ref().and_then(|u| u.fx_aux.loaded_slot)
                    != Some(self.fx_aux_ui.saved_slot)
                {
                    return Err("Load the selected FX slot before previewing it".into());
                }
                self.send(Operation::FxAux(O::ReviewSaved))
            }

            A::Open | A::Probe | A::LegacyProbe => {
                self.fx_aux_ui.open = true;
                self.fx_ui.open = false;
                self.record_page = false;
                self.measurement_ui.open = false;
                self.send(Operation::FxAux(if a == A::LegacyProbe {
                    O::LegacyProbe
                } else {
                    O::Probe
                }))
            }
            A::Scope => self.action(Action::SwitchScope("fx_configuration".into())),
            A::Bus(d) => {
                self.fx_aux_ui.bus =
                    (self.fx_aux_ui.bus as i32 + i32::from(d)).rem_euclid(4) as usize;
                Ok(())
            }
            A::Field(d) => {
                let count = self
                    .state
                    .as_ref()
                    .and_then(|u| u.fx_aux.snapshot.as_ref())
                    .and_then(|s| s["monitors"].as_array())
                    .map_or(9, |m| 10 + m.len());
                self.fx_aux_ui.field =
                    (self.fx_aux_ui.field as i32 + i32::from(d)).rem_euclid(count as i32) as usize;
                Ok(())
            }
            A::Adjust(_) | A::Algorithm(_) | A::ChooseAlgorithm(_) | A::Bypass | A::Panic => {
                if !self.fresh()
                    || self.provider.pending_edits.load(Ordering::Acquire) != 0
                    || self.state.as_ref().is_some_and(|u| u.review.is_some())
                {
                    return Err("fresh FX aux state and no pending review required".into());
                }
                let u = self.state.as_ref().ok_or("FX unavailable")?;
                if u.fx_aux.unknown
                    || u.fx_aux_age_ms.is_none_or(|t| {
                        t.saturating_add(u.received.elapsed().as_millis() as u64) > 1000
                    })
                {
                    return Err("FX aux explicit probe required".into());
                }
                let s = u.fx_aux.snapshot.as_ref().ok_or("FX aux unavailable")?;
                let bus = self.fx_aux_ui.bus;
                let mut c = crate::fx_aux::configuration(
                    s["buses"][bus]["configuration_json"]
                        .as_str()
                        .ok_or("FX aux configuration")?,
                )?;
                let palette = crate::master_eq::parse_owner_bounded(
                    s["palette_json"].as_str().ok_or("FX palette")?,
                    32768,
                )?;
                let palette = palette.as_array().ok_or("FX palette shape")?;
                match a {
                    A::Panic => (),
                    A::Bypass => {
                        c["settings"]["bypass"] =
                            json!(1 - c["settings"]["bypass"].as_u64().unwrap());
                    }
                    A::Algorithm(d) => {
                        let index = palette
                            .iter()
                            .position(|p| p["algorithm"] == c["settings"]["algorithm"])
                            .ok_or("FX algorithm unavailable")?;
                        let next =
                            (index as i32 + i32::from(d)).rem_euclid(palette.len() as i32) as usize;
                        c["tempo"] = Value::Null;
                        let gain = c["settings"]["gain"].clone();
                        let bypass = c["settings"]["bypass"].clone();
                        c["settings"] = palette[next]["defaults"].clone();
                        c["settings"]["gain"] = gain;
                        c["settings"]["bypass"] = bypass;
                    }
                    A::ChooseAlgorithm(id) => {
                        let chosen = palette
                            .iter()
                            .find(|p| p["algorithm"] == id)
                            .ok_or("effect unavailable on this provider")?;
                        let gain = c["settings"]["gain"].clone();
                        let bypass = c["settings"]["bypass"].clone();
                        c["settings"] = chosen["defaults"].clone();
                        c["settings"]["gain"] = gain;
                        c["settings"]["bypass"] = bypass;
                        c["tempo"] = Value::Null;
                    }
                    A::Adjust(d) => {
                        if self.fx_aux_ui.field >= 9 {
                            let field = self.fx_aux_ui.field;
                            let r = c.get_mut("returns").ok_or(
                                "Independent return routing unavailable; probe support first",
                            )?;
                            let value = if field == 9 {
                                &mut r["main"]
                            } else {
                                r["monitors"]
                                    .get_mut(field - 10)
                                    .ok_or("Monitor replaced; select a current field")?
                            };
                            *value = json!(
                                (value.as_f64().ok_or("FX return level")? + f64::from(d) * 0.05)
                                    .clamp(0., 1.)
                            );
                        } else if self.fx_aux_ui.field == 0 {
                            let v = c["sends"]
                                .get_mut(self.selected)
                                .ok_or("select an available input first")?;
                            *v = json!(
                                (v.as_f64().ok_or("FX send")? + f64::from(d) * 0.05).clamp(0., 1.)
                            );
                        } else if self.fx_aux_ui.field >= 6 {
                            if !matches!(c["settings"]["algorithm"].as_u64(), Some(1 | 8)) {
                                return Err("tempo divisions apply to digital/tape delay".into());
                            }
                            if c["tempo"].is_null() {
                                c["tempo"] = json!({"bpm":120.,"denominator":4,"modifier":0});
                            }
                            let t = &mut c["tempo"];
                            match self.fx_aux_ui.field {
                                6 => {
                                    t["bpm"] = json!(
                                        (t["bpm"].as_f64().unwrap() + f64::from(d))
                                            .clamp(30., 300.)
                                    )
                                }
                                7 => {
                                    let divisions = [1, 2, 4, 8, 16, 32, 64];
                                    let i = divisions
                                        .iter()
                                        .position(|n| Some(*n) == t["denominator"].as_u64())
                                        .ok_or("delay division")?;
                                    t["denominator"] = json!(
                                        divisions[(i as i32 + i32::from(d)).rem_euclid(7) as usize]
                                    );
                                }
                                _ => {
                                    t["modifier"] = json!(
                                        (t["modifier"].as_i64().unwrap() + i64::from(d))
                                            .rem_euclid(3)
                                    )
                                }
                            }
                            let ms = 240000.
                                / t["bpm"].as_f64().unwrap()
                                / t["denominator"].as_f64().unwrap()
                                * [1., 1.5, 2. / 3.][t["modifier"].as_u64().unwrap() as usize];
                            if !(1.0..=2000.0).contains(&ms) {
                                return Err(
                                    "division exceeds 1–2000 ms; choose another BPM/division"
                                        .into(),
                                );
                            }
                            c["settings"]["time_ms"] = json!(ms);
                        } else {
                            let field = self.fx_aux_ui.field;
                            if field == 1 {
                                c["tempo"] = Value::Null;
                            }
                            let key =
                                ["", "time_ms", "amount", "damping", "rate_hz", "gain"][field];
                            let p = palette
                                .iter()
                                .find(|p| p["algorithm"] == c["settings"]["algorithm"])
                                .ok_or("FX algorithm")?;
                            let p = &p["parameters"][field - 1];
                            if p["available"] != 1 {
                                return Err("parameter unavailable for this effect".into());
                            }
                            let min = p["minimum"].as_f64().ok_or("FX minimum")?;
                            let max = p["maximum"].as_f64().ok_or("FX maximum")?;
                            let step = match p["unit"].as_u64() {
                                Some(1) => 1.,
                                Some(2) if max > 100. => 100.,
                                _ => ((max - min) / 100.).max(0.001),
                            };
                            c["settings"][key] = json!(
                                (c["settings"][key].as_f64().ok_or("FX value")?
                                    + f64::from(d) * step)
                                    .clamp(min, max)
                            );
                        }
                    }
                    _ => unreachable!(),
                }
                let body = crate::fx_aux::mutation(s, bus, (!matches!(a, A::Panic)).then_some(c))?;
                self.send(Operation::FxAux(O::Review(body)))
            }
        }
    }
    pub(super) fn fx_aux_scene(&self) -> Scene {
        let mut scene = Scene::default();
        scene.primitives.push(Primitive::Rect {
            x: 0,
            y: 0,
            w: 1920,
            h: 1080,
            fill: "#10181c",
        });
        let mut lines=vec![format!("FX SENDS & RETURNS / FX {} selected",self.fx_aux_ui.bus+1),
            "Channel send → shared wet processing → independent Main / monitor returns".into(),
            "Menu provides every action. Select a channel in Mix to adjust its FX send.".into(),
            "[ / ] aux | A / D effect | Up / Down field | − / + value | Space bypass | P clear tail".into(),
            "W probe routing | L legacy read | X control FX | G grant | Enter confirm | Esc cancel / Back".into()];
        lines.push(format!("Saved FX slot {}/16 | S save/replace | O load | V preview/restore | , / . browse | K probe saved-intent support", self.fx_aux_ui.saved_slot+1));
        if let Some(u) = &self.state {
            if u.fx_aux.saved.is_some() {
                lines.push(format!(
                    "SAVED INTENT from slot {}; separate preview and confirmation required",
                    u.fx_aux
                        .loaded_slot
                        .map_or_else(|| "unknown".into(), |slot| (slot + 1).to_string())
                ));
            }
            let fresh = u
                .fx_aux_age_ms
                .is_some_and(|t| t.saturating_add(u.received.elapsed().as_millis() as u64) <= 1000)
                && !u.fx_aux.unknown
                && self.fresh();
            lines.push(if u.fx_aux.unknown {
                "UNKNOWN: explicit probe and new review required".into()
            } else if !fresh {
                "Readback stale: refresh to edit".into()
            } else if !u.writer_granted() {
                "Read-only: Control FX, then Grant to edit".into()
            } else {
                "FX control ready".into()
            });
            lines.push(
                match u
                    .snapshot
                    .as_ref()
                    .and_then(|s| s.clock.as_ref())
                    .map(|c| &c.state)
                {
                    Some(crate::topology::ClockState::Running) if self.fresh() => {
                        "Source running (software state; physical output unverified)"
                    }
                    Some(_) if self.fresh() => {
                        "DISARMED / quiesced: routes retained; explicit output rearm is separate"
                    }
                    _ => "Source arm state UNKNOWN / stale",
                }
                .into(),
            );
            if self.provider.pending_edits.load(Ordering::Acquire) != 0 {
                lines.push("PENDING request: no confirmed application yet".into());
            }
            if let Some(result) = &u.last_operation {
                lines.push(result.clone());
            }
            if let Some(s) = &u.fx_aux.snapshot {
                lines.push(
                    if s["pending"].is_null() {
                        "CONFIRMED readback (freshness shown above)"
                    } else {
                        "PENDING: applied targets below are not completion"
                    }
                    .into(),
                );
                if s.get("return_routing_version").is_none() {
                    lines.push("Independent Main / monitor routing UNAVAILABLE in this read; W probes support".into());
                }
                if s["available"] != true {
                    lines.push("Four FX auxes are unavailable on this provider".into());
                }
                if let Some(buses) = s["buses"].as_array() {
                    for (i, b) in buses.iter().enumerate() {
                        if let Ok(c) = crate::fx_aux::configuration(
                            b["configuration_json"].as_str().unwrap_or(""),
                        ) {
                            let owner = serde_json::from_str::<Value>(
                                b["owner_json"].as_str().unwrap_or("{}"),
                            )
                            .unwrap_or(Value::Null);
                            let state = if owner["fault"].as_u64().is_some_and(|n| n != 0) {
                                "FAULT — clear tail to reset"
                            } else if b["generation"] != b["applied_generation"] {
                                "Changing"
                            } else if c["settings"]["bypass"] == 1 {
                                "Bypassed — tails allowed"
                            } else {
                                "Active"
                            };
                            lines.push(format!(
                                "FX {}  {}  | Shared wet gain {:.0}% | {}",
                                i + 1,
                                crate::fx_aux::name(
                                    c["settings"]["algorithm"].as_u64().unwrap_or(0)
                                ),
                                c["settings"]["gain"].as_f64().unwrap_or(0.) * 100.,
                                state
                            ));
                            if i == self.fx_aux_ui.bus {
                                let input = s["inputs"][self.selected]["id"]
                                    .as_str()
                                    .unwrap_or("Select an input");
                                lines.push(format!(
                                    "  {} send: {:.0}% / post-fader, post-mute",
                                    input,
                                    c["sends"][self.selected].as_f64().unwrap_or(0.) * 100.
                                ));
                                let algorithm = c["settings"]["algorithm"].as_u64().unwrap_or(0);
                                let labels = crate::fx_aux::field_labels(algorithm);
                                let field = self.fx_aux_ui.field;
                                let label = if field < 9 {
                                    labels[field].to_owned()
                                } else if field == 9 {
                                    "Main-only return".into()
                                } else {
                                    format!("monitor-{} return", field - 9)
                                };
                                lines.push(format!("  Selected control: {label}"));
                                if let Some(r) = c.get("returns") {
                                    lines.push(format!(
                                        "  Main-only return: {:.0}%",
                                        r["main"].as_f64().unwrap() * 100.
                                    ));
                                    let monitors = r["monitors"].as_array().unwrap();
                                    // Bounded moving window keeps every configured monitor reachable.
                                    let start = field
                                        .saturating_sub(10)
                                        .saturating_sub(2)
                                        .min(monitors.len().saturating_sub(5));
                                    for (i, level) in
                                        monitors.iter().enumerate().skip(start).take(5)
                                    {
                                        lines.push(format!(
                                            "  {} monitor-{} return: {:.0}% ({}/{})",
                                            if field == i + 10 { ">" } else { " " },
                                            i + 1,
                                            level.as_f64().unwrap() * 100.,
                                            i + 1,
                                            monitors.len()
                                        ));
                                    }
                                    if monitors.is_empty() {
                                        lines.push("  No configured monitors".into());
                                    }
                                }
                                lines.extend(
                                    crate::fx_aux::sound_lines(&c)
                                        .into_iter()
                                        .map(|s| format!("  {s}")),
                                );
                            }
                        }
                    }
                }
            }
        } else {
            lines.push("Refresh to read the provider".into());
        }
        lines.push("Main zero leaves monitors audible. Bypass drains all tails; panic clears history, not a persistent mute.".into());
        lines.push(self.message.clone());
        for (i, value) in lines.into_iter().enumerate() {
            scene.primitives.push(Primitive::Text {
                x: 24,
                y: 24 + i as u32 * 30,
                value,
                color: "#e4e8e9",
            });
        }
        scene
    }
}
