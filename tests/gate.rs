//! Synthetic producer bytes from GigPies tests/gate.rs, not a hardware capture.
use shr_desk::{
    gate::State,
    processing::{self, Field},
};
#[test]
fn actual_producer_gate_state_and_independent_gr() {
    let bytes = include_bytes!("fixtures/gate/v5/snapshot.json");
    let reply = processing::decode_reply(bytes).unwrap();
    assert_eq!(reply.version, 5);
    let snapshot = reply.snapshot.unwrap();
    assert_eq!(snapshot.channels.len(), 16);
    let c = &snapshot.channels[0];
    assert_eq!(c.gate.unwrap().state, State::Closed);
    assert_eq!(c.gate.unwrap().attenuation_mdb, 24000);
    assert_eq!(c.gain_reduction_mdb, None);
    let mut config = c.target.clone();
    for (field, text) in [
        (Field::GateThreshold, "-35"),
        (Field::GateAttack, "0.2"),
        (Field::GateHold, "150"),
        (Field::GateRelease, "250"),
        (Field::GateRange, "18"),
        (Field::GateHysteresis, "4"),
    ] {
        config.set_text(field, text).unwrap();
    }
    assert_eq!(config.gate.attack_us, 200);
    assert_eq!(config.gate.range_mdb, 18000);
    assert!(config.set_text(Field::GateBypass, "2").is_err());
    let mut old: serde_json::Value = serde_json::from_slice(bytes).unwrap();
    old["version"] = 4.into();
    assert!(processing::decode_reply(&serde_json::to_vec(&old).unwrap()).is_err());
}

#[test]
fn causal_proposal_requires_consistent_preservation_evidence() {
    use serde_json::json;
    let config = shr_desk::gate::Config {
        bypass: false,
        ..Default::default()
    };
    let mut document = json!({"version":1,"input":"input-01","calibration_id":"train","evaluation_id":"eval",
        "proposal":{"config":config,"confidence":1.0,"reason":"prepared","measurements":{
            "quiet_hit_db":-20.,"bleed_p95_db":-40.,"decay_p90_ms":100.,"evaluation_hits":8,"evaluation_missed_hits":0,"evaluation_bleed_windows":8,"evaluation_false_openings":0}},
        "causal_evaluation":{"hits":8,"missed_hits":0,"bleed_passages":8,"false_opening_passages":0,"worst_hit_energy_change_db":-0.2}});
    let decode = |v: &serde_json::Value| {
        shr_desk::gate::PreparedDocument::decode(&serde_json::to_vec(v).unwrap(), "input-01")
    };
    assert!(decode(&document).is_ok());
    document["causal_evaluation"]["worst_hit_energy_change_db"] = json!(-2.);
    assert!(decode(&document).is_err());
    document["causal_evaluation"]["missed_hits"] = json!(1);
    document["proposal"]["config"]["bypass"] = json!(true);
    document["proposal"]["reason"] = json!("causal_evaluation_failed");
    document["proposal"]["confidence"] = json!(0.);
    assert!(decode(&document).is_ok());
    document["causal_evaluation"]["hits"] = json!(9);
    assert!(decode(&document).is_err());
}
