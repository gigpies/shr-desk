//! GP-METER P3: Desk presentation of every measured state, independent GR ageing,
//! context loss and isolation from held actions and local drafts.
use super::processing_tests::brain_surface;
use super::*;
use crate::metering::{Identity, Snapshot};
use crate::render::Primitive;

const DIM: &str = "#9caebc";
const CYAN: &str = "#66dfd3";
const AMBER: &str = "#f1bd6b";
const RED: &str = "#f47c85";
const EDGE: &str = "#3c4f63";

fn fixture(name: &str) -> Snapshot {
    let path = format!(
        "{}/tests/fixtures/gp-meter/v1/{name}.json",
        env!("CARGO_MANIFEST_DIR")
    );
    Snapshot::decode(&std::fs::read(path).unwrap()).unwrap()
}

fn identity(s: &Snapshot) -> Identity {
    Identity {
        show: s.show_id.clone(),
        epoch: s.source_epoch.0,
        map: s.map_generation.0,
        topology: s.topology.clone(),
        inputs: s.inputs,
        monitors: s.monitors,
        rate: s.sample_rate,
    }
}

/// Meter page over the brain surface (held baseline, live lease, fresh control).
fn page(name: &str) -> (Frontend, Receiver<Request>) {
    let (mut f, rx) = brain_surface();
    f.brain_page = false;
    f.meter_page = true;
    let s = fixture(name);
    let id = identity(&s);
    f.meters
        .accept(s, &id, 1, Duration::ZERO, Instant::now())
        .unwrap();
    // The admitted topology sizes the page even while the cache is unavailable.
    f.meter_identity = Some(id);
    (f, rx)
}

fn texts(f: &Frontend) -> Vec<(String, &'static str)> {
    let scene = f.scene();
    assert!(scene.in_bounds());
    scene
        .primitives
        .into_iter()
        .filter_map(|p| match p {
            Primitive::Text { value, color, .. } => Some((value, color)),
            _ => None,
        })
        .collect()
}

fn line(f: &Frontend, id: &str) -> (String, &'static str) {
    texts(f)
        .into_iter()
        .find(|(v, _)| v.starts_with(&format!("{id} ")))
        .unwrap_or_else(|| panic!("{id} not drawn"))
}

/// Coloured level bar fills (the 6 px rows), excluding the empty track.
fn bars(f: &Frontend) -> Vec<(u32, u32, &'static str)> {
    f.scene()
        .primitives
        .into_iter()
        .filter_map(|p| match p {
            Primitive::Rect {
                x, y, h: 6, fill, ..
            } if fill != EDGE => Some((x, y, fill)),
            _ => None,
        })
        .collect()
}

fn header(f: &Frontend) -> String {
    texts(f)
        .into_iter()
        .find(|(v, _)| v.contains("/ age "))
        .unwrap()
        .0
}

#[test]
fn clip_is_red_on_raw_and_processed_and_silent_taps_draw_no_bar() {
    let (mut f, rx) = page("valid-over-range");
    assert!(header(&f).starts_with("FRESH"));
    for processed in [false, true] {
        f.meter_processed = processed;
        let kind = if processed { "processed" } else { "raw" };
        let (text, color) = line(&f, &format!("input-01:{kind}"));
        assert!(text.ends_with("CLIP"), "{text}");
        assert_eq!(color, RED);
        assert!(bars(&f).contains(&(24, 220, RED)));
    }
    // Main is silent in this window: cyan text, explicit SILENCE, no level bar.
    let (text, color) = line(&f, "main-l");
    assert!(text.contains("SILENCE") && !text.contains("CLIP"), "{text}");
    assert_eq!(color, CYAN);
    assert!(!bars(&f).iter().any(|&(x, y, _)| x == 1008 && y == 220));
    assert!(rx.try_recv().is_err());
}

#[test]
fn silence_below_floor_stale_and_invalid_are_distinct() {
    let (f, _rx) = page("valid-silence");
    let (text, color) = line(&f, "input-01:raw");
    assert!(
        text.contains("SILENCE") && !text.contains("STALE"),
        "{text}"
    );
    assert_eq!(color, CYAN);
    assert!(!bars(&f).iter().any(|&(x, y, _)| x == 24 && y == 220));
    assert!(line(&f, "monitor-1").0.starts_with("monitor-1 "));

    // Old acquisition: every tap dims and says STALE; nothing claims a clip.
    let (f, _rx) = page("valid-stale");
    assert!(!f.meters.fresh(Instant::now()));
    assert!(header(&f).starts_with("STALE / UNAVAILABLE"));
    for id in ["input-01:raw", "main-l", "monitor-3"] {
        let (text, color) = line(&f, id);
        assert!(text.contains("STALE") && !text.contains("CLIP"), "{text}");
        assert_eq!(color, DIM);
    }
    assert!(bars(&f).iter().all(|&(_, _, c)| c == DIM));

    // Invalid tap: amber with its reason, other taps unaffected.
    let (mut f, _rx) = page("valid-invalid");
    f.selected = 47;
    let (text, color) = line(&f, "input-48:raw");
    assert!(text.contains("INVALID / "), "{text}");
    assert_eq!(color, AMBER);
    assert_eq!(line(&f, "input-47:raw").1, CYAN);
    assert!(
        !bars(&f)
            .iter()
            .any(|&(x, y, _)| x == 24 && y == 192 + 11 * 60 + 28)
    );
}

#[test]
fn unavailable_reasons_reach_the_header_and_every_tap() {
    for (name, reason) in [
        ("unavailable-capacity", "capacity"),
        ("unavailable-identity", "identity"),
        ("unavailable-missing_window", "missing_window"),
        ("unavailable-quiesced", "quiesced"),
        ("unavailable-unsupported", "unsupported"),
    ] {
        let (f, rx) = page(name);
        assert!(f.meters.snapshot.is_none());
        let head = header(&f);
        assert!(
            head.starts_with("STALE / UNAVAILABLE") && head.contains(reason),
            "{head}"
        );
        for id in ["input-01:raw", "main-l", "main-r"] {
            let (text, color) = line(&f, id);
            assert_eq!(text, format!("{id}  UNAVAILABLE"));
            assert_eq!(color, DIM);
        }
        assert!(bars(&f).is_empty());
        assert!(rx.try_recv().is_err());
    }
}

#[test]
fn stale_control_dims_fresh_meters_without_dropping_them() {
    let (mut f, _rx) = page("valid-sine");
    assert_eq!(line(&f, "input-01:raw").1, CYAN);
    let u = f.state.as_mut().unwrap();
    u.received = Instant::now() - Duration::from_secs(1);
    assert!(!f.fresh());
    assert!(f.meters.fresh(Instant::now()));
    assert!(header(&f).starts_with("STALE / UNAVAILABLE"));
    let (text, color) = line(&f, "input-01:raw");
    assert!(text.contains("STALE"), "{text}");
    assert_eq!(color, DIM);
    assert!(f.meters.snapshot.is_some());
}

#[test]
fn gain_reduction_is_aged_independently_of_meters() {
    let gr = |f: &Frontend| {
        texts(f)
            .into_iter()
            .find(|(v, _)| v.contains("GR "))
            .unwrap()
            .0
    };
    let (mut f, _rx) = page("valid-sine");
    let processing = f.state.as_mut().unwrap().processing.as_mut().unwrap();
    processing.channels[0].gain_reduction_mdb = Some(4500);
    let channels = processing.channels.len();
    f.selected = 0;
    let text = gr(&f);
    assert!(
        text.starts_with("Selected channel GR 4.500 dB / FRESH / independently aged 0 ms"),
        "{text}"
    );
    // Old processing observation: GR goes stale while measured audio stays fresh.
    f.state.as_mut().unwrap().processing_age_ms = Some(300);
    assert!(gr(&f).contains("4.500 dB / STALE / independently aged 300 ms"));
    assert!(header(&f).starts_with("FRESH"));
    f.state.as_mut().unwrap().processing_age_ms = None;
    assert!(gr(&f).contains("/ STALE / independently aged -- ms"));
    // Channels without processing identity, and missing GR, say UNAVAILABLE.
    f.state.as_mut().unwrap().processing_age_ms = Some(0);
    f.state
        .as_mut()
        .unwrap()
        .processing
        .as_mut()
        .unwrap()
        .channels[0]
        .gain_reduction_mdb = None;
    assert!(gr(&f).starts_with("Selected channel GR UNAVAILABLE / FRESH"));
    f.selected = channels;
    assert!(gr(&f).starts_with("Selected channel GR UNAVAILABLE"));
    f.state = None;
    assert_eq!(gr(&f), "GR UNAVAILABLE");
}

#[test]
fn provider_generation_or_identity_change_clears_meters_and_clip_hold() {
    let (mut f, rx) = page("valid-over-range");
    // Bind the cache to the current (topology-less) context, as a prior pump would.
    f.meter_generation = f.provider.generation();
    f.meter_identity = None;
    let held = f.meters.clone();
    f.pump();
    assert!(
        f.meters.snapshot.is_some(),
        "unchanged context keeps meters"
    );
    assert!(f.meters.clipped("input-01:raw", Instant::now()));
    f.provider.generation.fetch_add(1, Ordering::AcqRel);
    f.pump();
    assert!(f.meters.snapshot.is_none());
    assert!(!f.meters.clipped("input-01:raw", Instant::now()));
    assert_eq!(
        f.meters.message,
        "METER UNAVAILABLE / awaiting current topology"
    );
    assert!(!texts(&f).iter().any(|(v, _)| v.contains("CLIP")));
    assert!(bars(&f).is_empty());

    // A changed topology identity (new map) is also a context loss.
    let (mut f, _rx2) = page("valid-over-range");
    f.meter_generation = f.provider.generation();
    f.meter_identity = Some(Identity {
        map: 99,
        ..identity(held.snapshot.as_ref().unwrap())
    });
    f.pump();
    assert!(f.meters.snapshot.is_none());
    assert!(!f.meters.clipped("input-01:raw", Instant::now()));
    assert!(
        rx.try_recv().is_err(),
        "meter context loss sends nothing to control"
    );
}

#[test]
fn meter_browsing_never_touches_held_actions_drafts_or_control() {
    let (mut f, rx) = page("valid-invalid");
    let before = {
        let u = f.state.as_ref().unwrap();
        (
            u.held_baseline_ready,
            u.held_transport_authenticated,
            u.writer_lease_remaining_ms,
            u.brain.clone().map(|b| format!("{b:?}")),
        )
    };
    for key in ["T", "PageDown", "PageUp", "PageDown", "T", "t"] {
        f.key(key).unwrap();
        assert!(f.meter_page);
        assert!(!f.provider.brain_signal.live(), "{key} must not hold");
        assert!(f.scene().in_bounds());
    }
    assert!(f.meter_processed);
    // Seven monitors fit one bank: paging wraps in place.
    assert_eq!(f.meter_bus_bank, 0);
    // A meter cache loss does not release or alter held state.
    f.meters.clear("context lost");
    let after = {
        let u = f.state.as_ref().unwrap();
        (
            u.held_baseline_ready,
            u.held_transport_authenticated,
            u.writer_lease_remaining_ms,
            u.brain.clone().map(|b| format!("{b:?}")),
        )
    };
    assert_eq!(before, after);
    assert!(rx.try_recv().is_err(), "browsing meters sends nothing");

    // With a processing draft open, the meter page yields and keeps the draft.
    f.key("B").unwrap();
    assert!(!f.meter_page);
    f.action(Action::ProcessingEdit).unwrap();
    assert!(f.processing_draft.is_some());
    f.meter_page = true;
    assert!(
        !texts(&f)
            .iter()
            .any(|(v, _)| v.starts_with("MEASURED AUDIO")),
        "draft stays visible over meters"
    );
    assert!(f.key("B").is_err(), "B is not a meter toggle over a draft");
    assert!(f.processing_draft.is_some());
    assert!(rx.try_recv().is_err());
}
