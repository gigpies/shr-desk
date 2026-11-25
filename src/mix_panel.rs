//! Mix page on the shared instrument plates: a bank of input strips, Main and
//! monitor buses, and the selected target on physical column 8.
//! Strips are readouts and selection targets; provider state and the existing
//! reviewed actions keep authority. Nothing here is a learned MIDI binding.
use crate::channel_panel::{Hit, Target, channel_meter, meter_colour};
use crate::frontend::Frontend;
use shr_instrument_panel::terminal::ratatui::{buffer::Buffer, layout::Rect, style::Color};
use shr_instrument_panel::{Material, rack::*};
use std::time::Instant;

/// Inputs per Mix display bank. A presentation bound shared with the existing
/// Mix/meter banking, never an engine or topology capacity.
pub const BANK: usize = crate::frontend::MIX_BANK;
/// Monitor buses per bus bank, shared with the existing meter page.
pub const BUS_BANK: usize = 12;
const STRIP_X: u16 = 6;
const STRIP_W: u16 = 9;
const BUS_X: u16 = 116;
const SELECTED: Color = Color::Rgb(46, 112, 64);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Menu {
    #[default]
    Mix,
    Scope,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct MixUi {
    pub menu: Menu,
    pub secondary: bool,
}

/// One measured observation, with every non-measurement state kept distinct.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Level {
    Unavailable,
    Stale,
    Invalid,
    Silent,
    BelowFloor,
    Peak { mdb: i32, clip: bool },
}
impl Level {
    pub fn read(f: &Frontend, id: &str, now: Instant) -> Self {
        let Some(tap) = f
            .meters
            .snapshot
            .as_ref()
            .and_then(|s| s.taps.iter().find(|t| t.id == id))
        else {
            return Self::Unavailable;
        };
        if !(f.meters.fresh(now) && f.fresh()) {
            Self::Stale
        } else if !tap.valid {
            Self::Invalid
        } else if tap.silent {
            Self::Silent
        } else if tap.below_floor {
            Self::BelowFloor
        } else {
            tap.peak_millidbfs.map_or(Self::Invalid, |mdb| Self::Peak {
                mdb,
                clip: f.meters.clipped(id, now),
            })
        }
    }
    pub fn label(self) -> String {
        match self {
            Self::Unavailable => "—".into(),
            Self::Stale => "STALE".into(),
            Self::Invalid => "INVALID".into(),
            Self::Silent => "SILENT".into(),
            Self::BelowFloor => "FLOOR".into(),
            Self::Peak { clip: true, .. } => "CLIP".into(),
            Self::Peak { mdb, .. } => format!("{:.1}", f64::from(mdb) / 1000.),
        }
    }
    fn colour(self) -> Color {
        match self {
            Self::Peak { clip: true, .. } => RED,
            Self::Invalid => AMBER,
            Self::Peak { .. } => CYAN,
            _ => DIM,
        }
    }
    /// Lit fraction of the −60..0 dBFS scale; only a valid measurement lights.
    fn amount(self) -> f64 {
        match self {
            Self::Peak { mdb, .. } => ((f64::from(mdb) + 60000.) / 60000.).clamp(0., 1.),
            _ => 0.,
        }
    }
}

/// Vertical 16-segment peak column, 4 dB per segment from 0 dBFS.
fn column(b: &mut Buffer, x: u16, y: u16, width: u16, level: Level) {
    let amount = level.amount();
    let bar = "▰".repeat(usize::from(width));
    for row in 0..16u16 {
        let threshold = f64::from(15 - row) / 15.;
        let lit = amount > 0. && threshold <= amount
            || row == 0 && matches!(level, Level::Peak { clip: true, .. });
        text(
            b,
            x,
            y + row,
            width,
            &bar,
            meter_colour(-i32::from(row) * 4000, lit),
        );
    }
}
/// Horizontal 12-segment bus bar, 5 dB per segment from −60 dBFS.
fn bar(b: &mut Buffer, x: u16, y: u16, level: Level) {
    let amount = level.amount();
    for i in 0..12u16 {
        let db = -60000 + (i32::from(i) + 1) * 5000;
        let lit = amount > 0. && f64::from(i) / 11. <= amount
            || i == 11 && matches!(level, Level::Peak { clip: true, .. });
        text(b, x + i, y, 1, "▰", meter_colour(db, lit));
    }
}
fn centred(b: &mut Buffer, a: Rect, s: &str, c: Color) {
    let n = s.chars().count() as u16;
    text(b, a.x + a.width.saturating_sub(n) / 2, a.y, a.width, s, c);
}
fn fader_row(db: f64) -> u16 {
    // +12 dB at the top row, −60 dB at the bottom row of the 12-row track.
    ((12. - db.clamp(-60., 12.)) / 72. * 10.).round() as u16
}

pub fn draw(f: &Frontend, b: &mut Buffer, m: Material, ui: &MixUi) -> Vec<Hit> {
    let mut hits = Vec::new();
    if b.area.width < 160 || b.area.height < 45 {
        text(b, 0, 0, b.area.width, "Resize to 160 x 45.", TEXT);
        return hits;
    }
    let Some(u) = &f.state else { return hits };
    let Some(raw) = &u.snapshot else { return hits };
    let now = Instant::now();
    let fresh = f.fresh();
    let inputs = &raw.authority.inputs;
    let monitors = &raw.authority.monitors;
    let scope_monitor = f.scope_monitor().ok().flatten();
    let monitor_scope = f.scope() != "foh";
    let start = f.selected / BANK * BANK;
    let banks = inputs.len().div_ceil(BANK).max(1);
    let shown = inputs.len().saturating_sub(start).min(BANK);
    let tap = if f.meter_processed {
        "processed"
    } else {
        "raw"
    };
    let parameter = |input: &str, name: &str, monitor: Option<&str>| {
        raw.authority.parameters.iter().find(|p| {
            p.target.input == input
                && p.target.parameter == name
                && p.target.monitor.as_deref() == monitor
        })
    };
    let mode = raw
        .authority
        .modes
        .iter()
        .find(|(s, _)| s == f.scope())
        .map_or("—".to_string(), |(_, m)| m.to_uppercase());

    fill(b, b.area, Color::Rgb(19, 23, 27));
    panel(
        b,
        Rect::new(1, 0, 114, 44),
        &format!(
            "INPUTS {}–{} OF {} · BANK {} OF {}",
            if shown == 0 { 0 } else { start + 1 },
            start + shown,
            inputs.len(),
            start / BANK + 1,
            banks,
        ),
    );
    panel(b, Rect::new(BUS_X, 0, 24, 44), "BUSES");
    panel(b, Rect::new(141, 0, 18, 44), "SELECTED");
    let status = if !fresh {
        "STALE · MIX UNKNOWN".to_string()
    } else if monitor_scope {
        format!(
            "{} SENDS · {mode}",
            scope_monitor.unwrap_or("MONITOR ?").to_uppercase()
        )
    } else {
        format!("FOH · {mode}")
    };
    text(
        b,
        114 - status.chars().count() as u16,
        0,
        40,
        &status,
        if fresh { CYAN } else { AMBER },
    );
    // Shared left-margin scales for the meter wells and fader tracks.
    for (row, s) in [
        (4, "  0"),
        (5, " -4"),
        (7, "-12"),
        (10, "-24"),
        (13, "-36"),
        (16, "-48"),
        (19, "-60"),
    ] {
        text(
            b,
            2,
            row,
            3,
            s,
            meter_colour(-i32::from(row - 4) * 4000, true),
        );
    }
    text(b, 2, 20, 4, "dBFS", DIM);
    text(
        b,
        2,
        21,
        4,
        if f.meter_processed { "PRE" } else { "RAW" },
        DIM,
    );
    text(b, 2, 23, 4, "PAN", DIM);
    for db in [12., 0., -24., -48., -60.] {
        text(b, 2, 25 + fader_row(db), 4, &format!("{db:>3.0}"), DIM);
    }
    text(b, 2, 37, 4, "dB", DIM);
    text(b, 2, 38, 4, if monitor_scope { "SEND" } else { "FDR" }, DIM);
    text(b, 2, 40, 4, "MUTE", DIM);
    text(b, 2, 42, 4, "HOLD", DIM);

    for slot in 0..BANK {
        let x = STRIP_X + slot as u16 * STRIP_W;
        let index = start + slot;
        let Some(input) = inputs.get(index) else {
            // Unused bank slots are visibly unavailable, never empty controls.
            well(b, Rect::new(x, 3, STRIP_W - 1, 19));
            centred(b, Rect::new(x, 1, STRIP_W - 1, 1), "—", DIM);
            continue;
        };
        let selected = !f.selected_master && index == f.selected;
        if selected {
            fill(b, Rect::new(x, 1, STRIP_W - 1, 1), SELECTED);
        }
        centred(
            b,
            Rect::new(x, 1, STRIP_W - 1, 1),
            &format!("{}{:02}", if selected { "▸" } else { "" }, index + 1),
            TEXT,
        );
        let name: String = input.chars().take(usize::from(STRIP_W - 1)).collect();
        centred(
            b,
            Rect::new(x, 2, STRIP_W - 1, 1),
            &name,
            if selected { TEXT } else { DIM },
        );
        hits.push(Hit {
            area: Rect::new(x, 1, STRIP_W - 1, 22),
            target: Target::Strip {
                input: input.clone(),
                edit: None,
            },
        });
        well(b, Rect::new(x, 3, STRIP_W - 1, 19));
        text(b, x + 5, 3, 2, "GR", Color::Rgb(122, 98, 56));
        let level = Level::read(f, &format!("{input}:{tap}"), now);
        column(b, x + 1, 4, 3, level);
        // Owner gain reduction fills downward, independently fresh from meters.
        let gr = u
            .processing
            .as_ref()
            .filter(|p| f.processing_fresh() && !p.faulted)
            .and_then(|p| p.channels.iter().find(|c| &c.input == input))
            .filter(|c| c.ready && !c.target.compressor_bypass)
            .and_then(|c| c.gain_reduction_mdb);
        let reduction = gr.map_or(0., |v| (v as f64 / 24000.).clamp(0., 1.));
        for row in 0..16u16 {
            let lit = gr.is_some() && reduction > 0. && f64::from(row) / 15. <= reduction;
            text(
                b,
                x + 5,
                4 + row,
                2,
                "▰▰",
                if lit { AMBER } else { Color::Rgb(29, 39, 45) },
            );
        }
        centred(
            b,
            Rect::new(x, 20, STRIP_W - 1, 1),
            &level.label(),
            level.colour(),
        );
        // Pan (FOH only): position marker in a recessed well.
        let pan = (!monitor_scope)
            .then(|| parameter(input, "pan", None))
            .flatten()
            .and_then(|p| p.target_value.as_i64())
            .filter(|_| fresh);
        fill(b, Rect::new(x, 23, STRIP_W - 1, 1), WELL);
        match pan {
            Some(v) => {
                text(b, x, 23, 8, "·······", Color::Rgb(70, 82, 92));
                let at = ((v as f64 + 100.) / 200. * 6.).round() as u16;
                text(b, x + at, 23, 1, "●", if v == 0 { DIM } else { CYAN });
            }
            None => centred(b, Rect::new(x, 23, STRIP_W - 1, 1), "—", DIM),
        }
        // Level: owner target fader (FOH) or the scoped monitor send.
        let gain = parameter(
            input,
            if monitor_scope { "send" } else { "fader" },
            scope_monitor,
        );
        let db = gain
            .and_then(|p| p.target_value.as_i64())
            .filter(|_| fresh)
            .map(|v| v as f64 / 1000.);
        for row in 25..37 {
            text(b, x + 3, row, 2, "▕▏", Color::Rgb(9, 13, 17));
        }
        if let Some(db) = db {
            let y = 25 + fader_row(db);
            if m == Material::Glyph {
                mosaic(b, x + 1, y, 6, 2, "slider");
            } else {
                text(b, x + 1, y, 6, "╞════╡", TEXT);
            }
        }
        if !monitor_scope || gain.is_some() {
            hits.push(Hit {
                area: Rect::new(x, 23, STRIP_W - 1, 16),
                target: Target::Strip {
                    input: input.clone(),
                    edit: (!monitor_scope).then_some(crate::exact_value::Parameter::Fader),
                },
            });
        }
        fill(
            b,
            Rect::new(x, 38, STRIP_W - 1, 1),
            if selected {
                Color::Rgb(32, 69, 81)
            } else {
                WELL
            },
        );
        centred(
            b,
            Rect::new(x, 38, STRIP_W - 1, 1),
            &db.map_or("—".into(), |v| format!("{v:+.1}")),
            if selected { CYAN } else { TEXT },
        );
        let muted =
            parameter(input, "mute", None).is_some_and(|p| p.target_value.as_bool() == Some(true));
        let held = parameter(
            input,
            if monitor_scope { "send" } else { "fader" },
            scope_monitor,
        )
        .is_some_and(|p| p.hold.is_some());
        indicator(b, x + 2, 40, "red", fresh && muted, m);
        indicator(b, x + 2, 42, "yellow", fresh && held, m);
    }

    // Buses: Main pre-PA as two columns, monitors as banked horizontal bars.
    text(b, BUS_X + 1, 1, 22, "MAIN · pre-PA", DIM);
    well(b, Rect::new(BUS_X + 1, 3, 22, 19));
    let left = Level::read(f, "main-l", now);
    let right = Level::read(f, "main-r", now);
    column(b, BUS_X + 3, 4, 4, left);
    column(b, BUS_X + 16, 4, 4, right);
    for (row, s) in [
        (4, "0"),
        (7, "-12"),
        (10, "-24"),
        (13, "-36"),
        (16, "-48"),
        (19, "-60"),
    ] {
        centred(
            b,
            Rect::new(BUS_X + 8, row, 7, 1),
            s,
            meter_colour(-i32::from(row - 4) * 4000, true),
        );
    }
    text(b, BUS_X + 4, 20, 2, "L", DIM);
    text(b, BUS_X + 17, 20, 2, "R", DIM);
    let pair = format!("{} / {}", left.label(), right.label());
    centred(
        b,
        Rect::new(BUS_X + 1, 21, 22, 1),
        &pair,
        if matches!(left, Level::Peak { clip: true, .. })
            || matches!(right, Level::Peak { clip: true, .. })
        {
            RED
        } else {
            left.colour()
        },
    );
    let bus_start = f.meter_bus_bank * BUS_BANK;
    let bus_shown = monitors.len().saturating_sub(bus_start).min(BUS_BANK);
    text(
        b,
        BUS_X + 1,
        23,
        22,
        &format!(
            "MONITORS {}–{} OF {}",
            if bus_shown == 0 { 0 } else { bus_start + 1 },
            bus_start + bus_shown,
            monitors.len()
        ),
        DIM,
    );
    for (row, (i, id)) in monitors
        .iter()
        .enumerate()
        .skip(bus_start)
        .take(BUS_BANK)
        .enumerate()
    {
        let y = 25 + row as u16;
        let browsed = i == f.selected_monitor;
        let controlled = scope_monitor == Some(id.as_str());
        if controlled {
            fill(b, Rect::new(BUS_X + 1, y, 22, 1), Color::Rgb(32, 69, 81));
        }
        text(
            b,
            BUS_X + 1,
            y,
            3,
            &format!("{}{:<2}", if browsed { "▸" } else { " " }, i + 1),
            if controlled { CYAN } else { TEXT },
        );
        let level = Level::read(f, &format!("monitor-{}", i + 1), now);
        bar(b, BUS_X + 4, y, level);
        let label = level.label();
        text(
            b,
            BUS_X + 23 - label.chars().count() as u16,
            y,
            7,
            &label,
            level.colour(),
        );
    }
    indicator(b, BUS_X + 1, 38, "green", fresh && u.writer_granted(), m);
    text(
        b,
        BUS_X + 5,
        38,
        18,
        if fresh && u.writer_granted() {
            "WRITER GRANTED"
        } else {
            "READ-ONLY"
        },
        DIM,
    );
    let message: Vec<char> = f.message.chars().collect();
    for (i, chunk) in message.chunks(22).take(4).enumerate() {
        text(
            b,
            BUS_X + 1,
            39 + i as u16,
            22,
            &chunk.iter().collect::<String>(),
            Color::Rgb(244, 124, 133),
        );
    }

    // Physical column 8: the selected target, identical to the Channel page.
    let selected = (!f.selected_master)
        .then(|| inputs.get(f.selected))
        .flatten();
    text(
        b,
        143,
        1,
        15,
        &selected.map_or_else(
            || {
                if f.selected_master {
                    "MASTER".into()
                } else {
                    "—".into()
                }
            },
            |i| format!("{:02} {i}", f.selected + 1),
        ),
        TEXT,
    );
    let level = selected.map_or(Level::Unavailable, |i| {
        Level::read(f, &format!("{i}:processed"), now)
    });
    channel_meter(
        b,
        143,
        "PEAK",
        match level {
            Level::Peak { mdb, .. } => Some(mdb),
            _ => None,
        },
        false,
        level == Level::Silent,
    );
    text(b, 143, 22, 14, "PRE-FADER", DIM);
    if let Some(input) = selected {
        let pan = parameter(input, "pan", None).and_then(|p| p.target_value.as_i64());
        let gain = parameter(
            input,
            if monitor_scope { "send" } else { "fader" },
            scope_monitor,
        );
        let db = gain
            .and_then(|p| p.target_value.as_i64())
            .map(|v| v as f64 / 1000.);
        let mut pot = |y: u16, label: &str, value: String, n: f64, target: Option<Target>| {
            text(b, 143, y - 1, 16, label, DIM);
            encoder(b, 147, y, n, m);
            fill(b, Rect::new(143, y + 5, 16, 1), WELL);
            text(b, 144, y + 5, 14, &value, TEXT);
            if let Some(target) = target {
                hits.push(Hit {
                    area: Rect::new(143, y, 16, 6),
                    target,
                });
            }
        };
        let exact = |p| Target::Action(crate::actions::Action::ExactEdit(p));
        if monitor_scope {
            pot(24, "08 PAN", "— (monitor)".into(), 0.5, None);
        } else {
            pot(
                24,
                "08 PAN",
                pan.filter(|_| fresh).map_or("—".into(), |v| {
                    if v == 0 {
                        "CENTER".into()
                    } else {
                        format!("{} {}", if v < 0 { "L" } else { "R" }, v.abs())
                    }
                }),
                pan.map_or(0.5, |v| (v as f64 + 100.) / 200.),
                Some(exact(crate::exact_value::Parameter::Pan)),
            );
        }
        pot(
            33,
            if monitor_scope {
                "16 SEND / dB"
            } else {
                "16 LEVEL / dB"
            },
            db.filter(|_| fresh)
                .map_or("—".into(), |v| format!("{v:+.1}")),
            db.map_or(0., |v| (v + 60.) / 72.),
            (!monitor_scope).then(|| exact(crate::exact_value::Parameter::Fader)),
        );
        let muted =
            parameter(input, "mute", None).is_some_and(|p| p.target_value.as_bool() == Some(true));
        let held = gain.is_some_and(|p| p.hold.is_some());
        text(b, 143, 40, 10, if muted { "MUTED" } else { "UNMUTED" }, DIM);
        indicator(b, 154, 40, "red", fresh && muted, m);
        text(b, 143, 42, 10, if held { "HELD" } else { "HOLD OFF" }, DIM);
        indicator(b, 154, 42, "yellow", fresh && held, m);
    }

    // Eight contextual pads aligned with the physical pad columns.
    let selected_muted = selected.is_some_and(|i| {
        parameter(i, "mute", None).is_some_and(|p| p.target_value.as_bool() == Some(true))
    });
    let selected_held = selected.is_some_and(|i| {
        parameter(
            i,
            if monitor_scope { "send" } else { "fader" },
            scope_monitor,
        )
        .is_some_and(|p| p.hold.is_some())
    });
    use crate::actions::Action as A;
    use crate::controller_surface::Command as C;
    let pads: Vec<(String, Target, &str, bool)> = if ui.secondary {
        vec![
            ("MIX".into(), Target::MixMenu(Menu::Mix), "blue", true),
            (
                "CHANNEL".into(),
                Target::Action(A::Page(crate::model::Page::Channel)),
                "blue",
                false,
            ),
            ("SENDS".into(), Target::Action(A::SendsPage), "blue", false),
            (
                "CONTROLS".into(),
                Target::Action(A::Console(C::Menu)),
                "blue",
                false,
            ),
            (
                "GRANT".into(),
                Target::Action(A::Console(C::Grant)),
                "green",
                fresh && u.writer_granted(),
            ),
            (
                "RELEASE".into(),
                Target::Action(A::Release),
                "yellow",
                fresh && selected_held,
            ),
            ("CANCEL".into(), Target::Action(A::Cancel), "yellow", false),
            (
                "RECONNECT".into(),
                Target::Action(A::Console(C::Reconnect)),
                "red",
                !fresh,
            ),
        ]
    } else {
        match ui.menu {
            Menu::Mix => vec![
                ("BANK ◀".into(), Target::Action(A::Bank(-1)), "blue", false),
                ("BANK ▶".into(), Target::Action(A::Bank(1)), "blue", false),
                (
                    "CHANNEL".into(),
                    Target::Action(A::Page(crate::model::Page::Channel)),
                    "blue",
                    false,
                ),
                (
                    if selected_muted { "UNMUTE" } else { "MUTE" }.into(),
                    Target::Action(A::Mute),
                    "red",
                    fresh && selected_muted,
                ),
                (
                    if selected_held { "HELD" } else { "HOLD" }.into(),
                    Target::Action(A::Hold),
                    "yellow",
                    fresh && selected_held,
                ),
                (
                    "RELEASE".into(),
                    Target::Action(A::Release),
                    "yellow",
                    false,
                ),
                ("MODE".into(), Target::Action(A::ModePicker), "blue", false),
                (
                    "SCOPE".into(),
                    Target::MixMenu(Menu::Scope),
                    "blue",
                    monitor_scope,
                ),
            ],
            Menu::Scope => vec![
                (
                    "MON ◀".into(),
                    Target::Action(A::BrowseMonitor(-1)),
                    "blue",
                    false,
                ),
                (
                    "MON ▶".into(),
                    Target::Action(A::BrowseMonitor(1)),
                    "blue",
                    false,
                ),
                (
                    format!("CTRL MON {}", f.selected_monitor + 1),
                    Target::Action(A::SwitchScope(format!("monitor{}", f.selected_monitor + 1))),
                    "blue",
                    monitor_scope,
                ),
                (
                    "CTRL FOH".into(),
                    Target::Action(A::SwitchScope("foh".into())),
                    "blue",
                    !monitor_scope,
                ),
                (
                    if f.meter_processed {
                        "METER PRE"
                    } else {
                        "METER RAW"
                    }
                    .into(),
                    Target::Action(A::Console(C::MeterTap)),
                    "blue",
                    f.meter_processed,
                ),
                (
                    "BUS BANK".into(),
                    Target::Action(A::Console(C::MeterBank(1))),
                    "blue",
                    false,
                ),
                ("SENDS".into(), Target::Action(A::SendsPage), "blue", false),
                ("BACK".into(), Target::MixMenu(Menu::Mix), "blue", false),
            ],
        }
    };
    for (i, (label, target, colour, on)) in pads.into_iter().enumerate() {
        crate::channel_panel::pad(
            b,
            &mut hits,
            (i as u16, i as u16 + if ui.secondary { 9 } else { 1 }),
            &label,
            target,
            (colour, on),
            m,
        );
    }
    hits.push(Hit {
        area: Rect::new(BUS_X + 14, 0, 10, 1),
        target: Target::Layer,
    });
    text(
        b,
        BUS_X + 14,
        0,
        10,
        if ui.secondary {
            "PADS 9–16"
        } else {
            "PADS 1–8"
        },
        CYAN,
    );
    hits
}

/// Strip press: select a different input through the guarded keybed path, or
/// open the existing detached exact editor for the input already selected.
pub fn strip_action(
    f: &Frontend,
    input: &str,
    edit: Option<crate::exact_value::Parameter>,
) -> Option<crate::actions::Action> {
    use crate::actions::Action;
    if !f.mix_panel_visible() {
        return None;
    }
    if !f.selected_master && f.selected_input() == Some(input) {
        edit.map(Action::ExactEdit)
    } else {
        Some(Action::Console(
            crate::controller_surface::Command::SelectTarget(
                crate::controller_flow::Target::Input(input.to_string()),
            ),
        ))
    }
}
