//! GP-PANEL Mix page: topology-derived banks, measured states, FOH/monitor scopes,
//! draft/review isolation and context loss. Projection fixtures only.
use super::processing_tests::brain_surface;
use super::*;
use crate::channel_panel::{Hit, Target};
use crate::metering::{Identity, Snapshot};
use crate::mix_panel::{Level, MixUi, draw, strip_action};
use shr_instrument_panel::Material;
use shr_instrument_panel::terminal::ratatui::{buffer::Buffer, layout::Rect};

fn meters(name: &str) -> Snapshot {
    let path = format!(
        "{}/tests/fixtures/gp-meter/v1/{name}.json",
        env!("CARGO_MANIFEST_DIR")
    );
    Snapshot::decode(&std::fs::read(path).unwrap()).unwrap()
}

/// Mix over the brain surface; `inputs` is a projection, not a hardware capacity.
fn mix(inputs: usize) -> (Frontend, Receiver<Request>) {
    let (mut f, rx) = brain_surface();
    f.brain_page = false;
    f.page = Page::Mix;
    f.state
        .as_mut()
        .unwrap()
        .snapshot
        .as_mut()
        .unwrap()
        .authority
        .inputs = (1..=inputs).map(|i| format!("input-{i:02}")).collect();
    (f, rx)
}

fn accept(f: &mut Frontend, name: &str) {
    let s = meters(name);
    let id = Identity {
        show: s.show_id.clone(),
        epoch: s.source_epoch.0,
        map: s.map_generation.0,
        topology: s.topology.clone(),
        inputs: s.inputs,
        monitors: s.monitors,
        rate: s.sample_rate,
    };
    f.meters
        .accept(s, &id, 1, Duration::ZERO, Instant::now())
        .unwrap();
}

fn render(f: &Frontend) -> (Vec<String>, Vec<Hit>) {
    let mut b = Buffer::empty(Rect::new(0, 0, 160, 45));
    let hits = draw(f, &mut b, Material::Text, &MixUi::default());
    let rows = (0..45)
        .map(|y| (0..160).map(|x| b[(x, y)].symbol()).collect::<String>())
        .collect();
    (rows, hits)
}

/// Cells from column `x` on (rows hold multi-byte symbols).
fn from(row: &str, x: usize) -> String {
    row.chars().skip(x).collect()
}

fn strips(hits: &[Hit]) -> Vec<String> {
    let mut ids: Vec<String> = hits
        .iter()
        .filter_map(|h| match &h.target {
            Target::Strip { input, .. } => Some(input.clone()),
            _ => None,
        })
        .collect();
    ids.dedup();
    ids
}

#[test]
fn banks_follow_the_topology_with_unused_slots_unavailable() {
    for count in [8usize, 16, 24, 48] {
        let (mut f, rx) = mix(count);
        assert!(f.mix_panel_visible());
        let (rows, hits) = render(&f);
        let shown = count.min(MIX_BANK);
        assert!(
            rows[0].contains(&format!(
                "INPUTS 1–{shown} OF {count} · BANK 1 OF {}",
                count.div_ceil(MIX_BANK)
            )),
            "{}",
            rows[0]
        );
        assert_eq!(strips(&hits).len(), shown);
        // An unused slot draws a dash, never a strip target.
        if count < MIX_BANK {
            let x = 6 + count * 9;
            assert!(
                from(&rows[1], x).trim_start().starts_with('—'),
                "{}",
                rows[1]
            );
        }
        // Paging moves in display banks and keeps the slot within the bank.
        f.selected = 3;
        // Paging fences pending work and drops the snapshot until the worker
        // republishes it; model that republication with the same authority.
        let kept = f.state.clone();
        f.action(Action::Bank(1)).unwrap();
        assert!(!f.mix_panel_visible(), "fenced until fresh authority");
        f.state = kept;
        let expected = if count > MIX_BANK {
            (MIX_BANK + 3).min(count - 1)
        } else {
            3
        };
        assert_eq!(f.selected, expected);
        let (rows, hits) = render(&f);
        let start = expected / MIX_BANK * MIX_BANK;
        assert_eq!(
            strips(&hits).first().map(String::as_str),
            Some(format!("input-{:02}", start + 1).as_str())
        );
        assert!(rows[0].contains(&format!("BANK {} OF", start / MIX_BANK + 1)));
        // Browsing banks never reaches the owner.
        assert!(
            rx.try_iter()
                .all(|r| matches!(r.operation, Operation::Cancel))
        );
    }
}

#[test]
fn measured_states_stay_distinct_and_clip_holds_red() {
    let (mut f, _rx) = mix(48);
    accept(&mut f, "valid-invalid");
    let now = Instant::now();
    assert!(matches!(
        Level::read(&f, "input-01:raw", now),
        Level::Peak {
            mdb: -6021,
            clip: false
        }
    ));
    assert_eq!(Level::read(&f, "input-48:raw", now), Level::Invalid);
    assert_eq!(Level::read(&f, "main-l", now), Level::Silent);
    assert_eq!(Level::read(&f, "monitor-99", now), Level::Unavailable);
    f.selected = 47;
    let (rows, _) = render(&f);
    assert!(rows[0].contains("INPUTS 37–48 OF 48 · BANK 4 OF 4"));
    assert!(
        rows[20].contains("INVALID") && rows[20].contains("-6.0"),
        "{}",
        rows[20]
    );
    assert!(rows[21].contains("SILENT / SILENT"), "{}", rows[21]);

    let (mut f, _rx) = mix(48);
    accept(&mut f, "valid-over-range");
    let (rows, _) = render(&f);
    assert!(rows[20].trim_start().starts_with("dBFS") && rows[20].contains("CLIP"));

    // Control staleness dims fresh meters to STALE instead of dropping them.
    f.state.as_mut().unwrap().received = Instant::now() - Duration::from_secs(1);
    assert_eq!(
        Level::read(&f, "input-01:raw", Instant::now()),
        Level::Stale
    );
    assert!(f.meters.snapshot.is_some());
    let (rows, _) = render(&f);
    assert!(rows[0].contains("STALE · MIX UNKNOWN"));
    assert!(!rows[20].contains("CLIP"));
}

#[test]
fn foh_and_monitor_scopes_choose_fader_or_send_and_pan() {
    let (mut f, _rx) = mix(8);
    let (rows, hits) = render(&f);
    assert!(rows[0].contains("FOH · MANUAL"));
    assert!(rows[38].starts_with("  FDR") && rows[38].contains("-6.0"));
    assert!(rows[23].contains('●'), "FOH pan marker");
    assert!(hits.iter().any(|h| matches!(
        &h.target,
        Target::Strip {
            edit: Some(crate::exact_value::Parameter::Fader),
            ..
        }
    )));
    f.scope = "monitor1".into();
    assert!(f.mix_panel_visible());
    let (rows, hits) = render(&f);
    assert!(rows[0].contains("MONITOR-1 SENDS · MANUAL"), "{}", rows[0]);
    // Fixture sends monitor-1 at 0 dB; pan has no meaning in a monitor scope.
    assert!(rows[38].starts_with("  SEND") && rows[38].contains("+0.0"));
    assert!(!rows[23].contains('●'));
    assert!(
        hits.iter()
            .all(|h| !matches!(&h.target, Target::Strip { edit: Some(_), .. })),
        "monitor sends keep their reviewed Sends path"
    );
    // The controlled monitor bus row is marked, as is the browsed one.
    assert!(from(&rows[25], 116).starts_with(" ▸1"), "{}", rows[25]);
    // Unadvertised or unrelated scopes keep their existing pages.
    f.scope = "pa_configuration".into();
    assert!(!f.mix_panel_visible());
}

#[test]
fn drafts_and_reviews_keep_their_pages_and_strips_only_select() {
    let (mut f, rx) = mix(16);
    f.selected = 0;
    assert_eq!(
        strip_action(&f, "input-01", Some(crate::exact_value::Parameter::Fader)),
        Some(Action::ExactEdit(crate::exact_value::Parameter::Fader))
    );
    assert_eq!(
        strip_action(&f, "input-02", Some(crate::exact_value::Parameter::Fader)),
        Some(Action::Console(
            crate::controller_surface::Command::SelectTarget(
                crate::controller_flow::Target::Input("input-02".into())
            )
        ))
    );
    assert_eq!(strip_action(&f, "input-01", None), None);
    // The detached exact editor keeps its existing page.
    f.action(Action::ExactEdit(crate::exact_value::Parameter::Fader))
        .unwrap();
    assert!(f.exact_draft.is_some());
    assert!(!f.mix_panel_visible());
    f.exact_draft = None;
    // A local processing draft keeps its own presentation; nothing is sent.
    f.page = Page::Channel;
    f.action(Action::ProcessingEdit).unwrap();
    f.page = Page::Mix;
    assert!(f.processing_draft.is_some());
    assert!(!f.mix_panel_visible());
    assert_eq!(strip_action(&f, "input-02", None), None);
    assert!(
        f.action(Action::Console(
            crate::controller_surface::Command::SelectTarget(
                crate::controller_flow::Target::Input("input-02".into())
            )
        ))
        .is_err()
    );
    assert_eq!(f.selected, 0);
    assert!(f.processing_draft.is_some());
    let kept = f.state.clone();
    f.action(Action::Cancel).unwrap();
    f.state = kept; // worker republication after the cancellation fence
    assert!(f.processing_draft.is_none());
    assert!(f.mix_panel_visible());
    // Owner review takes precedence over the plates.
    f.state.as_mut().unwrap().review = Some((1, "review".into()));
    assert!(!f.mix_panel_visible());
    assert!(
        rx.try_iter()
            .all(|r| matches!(r.operation, Operation::Cancel)),
        "no draft or selection reached the owner"
    );
}

#[test]
fn context_loss_clears_values_meters_and_lamps() {
    let (mut f, rx) = mix(16);
    accept(&mut f, "valid-over-range");
    let (rows, _) = render(&f);
    assert!(rows[38].contains("-6.0"));
    // Lost control freshness: no stale fader, pan or lamp is presented as current.
    f.state.as_mut().unwrap().received = Instant::now() - Duration::from_secs(1);
    let (rows, _) = render(&f);
    assert!(!rows[38].contains("-6.0") && rows[38].contains('—'));
    assert!(!rows[23].contains('●'));
    assert!(from(&rows[38], 116).contains("READ-ONLY"));
    // A provider generation change clears the meter cache and clip hold.
    f.meter_generation = f.provider.generation();
    f.meter_identity = None;
    f.provider.generation.fetch_add(1, Ordering::AcqRel);
    f.pump();
    assert!(f.meters.snapshot.is_none());
    assert_eq!(
        Level::read(&f, "input-01:raw", Instant::now()),
        Level::Unavailable
    );
    // No authority snapshot at all: nothing to draw or press.
    f.state = None;
    assert!(!f.mix_panel_visible());
    assert!(render(&f).1.is_empty());
    assert!(
        rx.try_iter()
            .all(|r| matches!(r.operation, Operation::Cancel))
    );
}
