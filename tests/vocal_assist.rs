//! GP-AUTO main-vocal assist consumer checks against the GigPies producer corpus
//! `tests/fixtures/vocal-assist/v1` (copied byte-identical; hashes in the README).
use shr_desk::vocal::{Document, Reason, State, Window};

const DIR: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/vocal-assist/v1"
);
fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!("{DIR}/{name}")).unwrap()
}

#[test]
fn v6_readback_with_active_reduction_decodes_and_validates() {
    let reply = shr_desk::processing::decode_reply(&fixture("snapshot.json")).unwrap();
    assert_eq!(reply.version, 6);
    let snapshot = reply.snapshot.unwrap();
    snapshot.validate_version(6).unwrap();
    let c = &snapshot.channels[0];
    let a = c.vocal_assist.as_ref().unwrap();
    assert_eq!(a.state, State::Reducing);
    assert_eq!((a.reduction_mdb, a.deepest_mdb), (6000, 6000));
    assert!(!c.target.vocal_assist.bypass);
    assert!(
        snapshot.channels[1..]
            .iter()
            .all(|c| c.vocal_assist.is_none())
    );
    // A v5 consumer view of the same snapshot is refused, not silently truncated.
    assert!(snapshot.validate_version(5).is_err());
    // An observation without configured assist is incoherent.
    let mut broken = snapshot.clone();
    broken.channels[1].vocal_assist = c.vocal_assist.clone();
    assert!(broken.validate_version(6).is_err());
}

#[test]
fn soundcheck_documents_are_identity_fenced_and_refusals_stay_bypassed() {
    let show = "11111111-1111-4111-8111-111111111111";
    let d =
        Document::decode_for(&fixture("soundcheck-document.json"), show, "3", "input-01").unwrap();
    assert_eq!(d.proposal.reason, Reason::Prepared);
    assert_eq!(d.proposal.config.reference_mdb, -24200);
    assert!(d.report().iter().any(|l| l.contains("Keep -24.2 dBFS")));
    for (show, epoch, input) in [
        (show, "4", "input-01"),
        (show, "3", "input-02"),
        ("22222222-2222-4222-8222-222222222222", "3", "input-01"),
    ] {
        assert!(
            Document::decode_for(&fixture("soundcheck-document.json"), show, epoch, input).is_err()
        );
    }
    let clipped = Document::decode_for(
        &fixture("soundcheck-clipped-document.json"),
        show,
        "3",
        "input-01",
    )
    .unwrap();
    assert_eq!(clipped.proposal.reason, Reason::Clipped);
    assert!(clipped.proposal.config.bypass);
}

#[test]
fn desk_windows_serialize_exactly_as_the_producer_request() {
    let request: serde_json::Value =
        serde_json::from_slice(&fixture("soundcheck-request.json")).unwrap();
    let windows: Vec<Window> = serde_json::from_value(request["windows"].clone()).unwrap();
    assert_eq!(serde_json::to_value(&windows).unwrap(), request["windows"]);
}

/// Real GigPies control process over the producer request:
/// `GIGPIES_BIN=/abs/gigpies cargo test --test vocal_assist -- --ignored`.
#[test]
#[ignore = "needs an installed GigPies executable in GIGPIES_BIN"]
fn installed_gigpies_prepares_the_producer_document() {
    use shr_desk::gate_job::{Job, Kind};
    let bin = std::env::var("GIGPIES_BIN").unwrap();
    let job = Job::start_kind(Kind::Vocal, bin.into(), fixture("soundcheck-request.json")).unwrap();
    let end = std::time::Instant::now() + std::time::Duration::from_secs(30);
    let result = loop {
        if let Some(r) = job.poll() {
            break r.unwrap();
        }
        assert!(std::time::Instant::now() < end);
        std::thread::sleep(std::time::Duration::from_millis(10));
    };
    let got: serde_json::Value = serde_json::from_slice(&result.bytes).unwrap();
    let want: serde_json::Value =
        serde_json::from_slice(&fixture("soundcheck-document.json")).unwrap();
    assert_eq!(got, want);
}
