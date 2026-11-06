use super::*;
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};
#[derive(Clone, Copy)]
enum Mode {
    StaleTelemetry,
    Applied,
    Refused,
    Lost,
    LostApplied,
    WrongTarget,
    WrongOwner,
    PendingOwner,
    Replaced,
    Malformed,
    Transport,
}
struct Peer {
    raw: audio::RenderedSnapshot,
    aux: Value,
    queue: VecDeque<Value>,
    mode: Mode,
    sent: Arc<Mutex<Vec<Value>>>,
    changed: bool,
    stale_sent: bool,
}
fn raw_reply(raw: &audio::RenderedSnapshot) -> Value {
    let mut v: Value =
        serde_json::from_str::<Value>(include_str!("../../tests/fixtures/gp14/v1/profile-48.json"))
            .unwrap()["snapshot"]
            .clone();
    let body = serde_json::to_value(raw).unwrap();
    v["snapshot"] = body.clone();
    v["context"]["epoch"] = json!("9");
    v["outcome"]["epoch"] = json!("9");
    v["outcome"]["body"]["revision"] = json!(raw.authority.revision);
    v["outcome"]["body"]["snapshot"] = body["authority"].clone();
    v
}
impl AuthorityConnection for Peer {
    fn send_frame_until(&mut self, bytes: &[u8], _: Instant) -> Result<(), String> {
        let r: Value = serde_json::from_slice(bytes).unwrap();
        self.sent.lock().unwrap().push(r.clone());
        match r["kind"].as_str().unwrap() {
            "snapshot" => {
                self.raw.authority.sequence =
                    (crate::provider::counter(&self.raw.authority.sequence).unwrap() + 1)
                        .to_string();
                let frame = crate::provider::counter(&self.raw.frame).unwrap() + 48;
                self.raw.frame = frame.to_string();
                self.raw.clock.as_mut().unwrap().next_frame = frame;
                self.raw.authority.revision = if self.changed { "1" } else { "0" }.into();
                let v = raw_reply(&self.raw);
                self.queue.push_back(v);
            }
            "fx_aux_snapshot" => {
                if matches!(self.mode, Mode::Transport) {
                    return Err("authentication/transport failed".into());
                }
                let mut v = self.aux.clone();
                v["revision"] = json!(self.raw.authority.revision);
                v["frame"] = json!(self.raw.frame);
                if matches!(self.mode, Mode::Malformed) {
                    v["return_routing_version"] = json!(99);
                }
                if r["body"] == json!({}) {
                    v.as_object_mut().unwrap().remove("monitors");
                    v.as_object_mut().unwrap().remove("return_routing_version");
                    for bus in v["buses"].as_array_mut().unwrap() {
                        let mut c = crate::fx_aux::configuration(
                            bus["configuration_json"].as_str().unwrap(),
                        )
                        .unwrap();
                        c.as_object_mut().unwrap().remove("returns");
                        bus["configuration_json"] = json!(c.to_string());
                    }
                }
                self.queue.push_back(v);
            }
            "fx_aux_configure" => {
                if matches!(self.mode, Mode::Lost) {
                    return Ok(());
                }
                let mut reply = json!({"contract":"GP-FX-aux","version":1,"show_id":r["show_id"],"module":"audio","epoch":r["epoch"],"writer":r["writer"],"lease":r["lease"],"request_id":r["request_id"],"state":"accepted","reason":null,"effective_frame":self.raw.frame});
                if matches!(self.mode, Mode::Refused) {
                    reply["state"] = json!("rejected");
                    reply["reason"] = json!("stale lease");
                    reply["effective_frame"] = Value::Null;
                } else {
                    self.changed = true;
                    for body in crate::fx_saved::edits(&r["body"]).unwrap() {
                        let bus = body["bus"].as_u64().unwrap() as usize;
                        let generation = (crate::provider::counter(
                            body["expected_generation"].as_str().unwrap(),
                        )
                        .unwrap()
                            + 1)
                        .to_string();
                        let b = &mut self.aux["buses"][bus];
                        b["generation"] = json!(generation);
                        b["applied_generation"] = json!(generation);
                        if !matches!(self.mode, Mode::WrongTarget) {
                            b["configuration_json"] = body["configuration_json"].clone();
                        }
                        if matches!(self.mode, Mode::WrongOwner) {
                            let mut status: Value =
                                serde_json::from_str(b["owner_json"].as_str().unwrap()).unwrap();
                            status["current"]["gain"] = json!(0.);
                            b["owner_json"] = json!(status.to_string());
                        }
                    }
                    if matches!(self.mode, Mode::PendingOwner) {
                        let bus = if r["body"].get("restore").is_some() {
                            3
                        } else {
                            r["body"]["bus"].as_u64().unwrap() as usize
                        };
                        let b = &mut self.aux["buses"][bus];
                        let mut status: Value =
                            serde_json::from_str(b["owner_json"].as_str().unwrap()).unwrap();
                        status["request_state"] = json!(1);
                        status["target"]["gain"] = json!(0.);
                        b["owner_json"] = json!(status.to_string());
                    }
                    if matches!(self.mode, Mode::Replaced) {
                        self.aux["owner_binding"] = json!("b".repeat(64));
                    }
                }
                if !matches!(self.mode, Mode::LostApplied) {
                    self.queue.push_back(reply);
                }
            }
            k => panic!("unexpected {k}"),
        }
        Ok(())
    }
    fn receive_available(&mut self) -> Result<Option<Vec<u8>>, String> {
        if matches!(self.mode, Mode::StaleTelemetry) && !self.stale_sent {
            self.stale_sent = true;
            let mut raw = self.raw.clone();
            raw.frame = "0".into();
            raw.clock.as_mut().unwrap().next_frame = 0;
            raw.authority.sequence = "0".into();
            return Ok(Some(serde_json::to_vec(&raw_reply(&raw)).unwrap()));
        }
        Ok(None)
    }
    fn receive_until(&mut self, _: Instant) -> Result<Option<Vec<u8>>, String> {
        self.queue
            .pop_front()
            .map(|v| Some(serde_json::to_vec(&v).unwrap()))
            .ok_or("lost completion".into())
    }
}
fn operator(mode: Mode) -> (Operator, Arc<Mutex<Vec<Value>>>, Value) {
    operator_bundle(mode, false)
}
fn operator_bundle(mode: Mode, bundle: bool) -> (Operator, Arc<Mutex<Vec<Value>>>, Value) {
    let mut session = audio::fx_test_pending();
    session.pending = None;
    let mut raw = session.snapshot.clone().unwrap();
    if bundle {
        raw.clock.as_mut().unwrap().state = crate::topology::ClockState::Disarmed;
    }
    raw.frame = "96".into();
    raw.clock.as_mut().unwrap().next_frame = 96;
    session.ingest_snapshot(raw.clone(), 0).unwrap();
    let count = raw.topology.as_ref().unwrap().monitors;
    let mut aux: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/gp-fx-aux/v1/owner.json")).unwrap();
    aux["revision"] = json!("0");
    aux["owner_binding"] = json!("a".repeat(64));
    aux["return_routing_version"] = json!(1);
    aux["monitors"] = json!(
        (1..=count)
            .map(|i| format!("monitor-{i}"))
            .collect::<Vec<_>>()
    );
    for b in aux["buses"].as_array_mut().unwrap() {
        let mut c =
            crate::fx_aux::configuration(b["configuration_json"].as_str().unwrap()).unwrap();
        c["returns"] = json!({"main":1.,"monitors":vec![0.;count]});
        b["configuration_json"] = json!(c.to_string());
        b["applied_generation"] = b["generation"].clone();
        let mut owner: Value = serde_json::from_str(b["owner_json"].as_str().unwrap()).unwrap();
        owner["current"] = c["settings"].clone();
        owner["transition"] = json!(0);
        b["owner_json"] = json!(owner.to_string());
    }
    let mut target =
        crate::fx_aux::configuration(aux["buses"][0]["configuration_json"].as_str().unwrap())
            .unwrap();
    target["returns"]["main"] = json!(0.);
    let body = crate::fx_aux::mutation(&aux, 0, Some(target)).unwrap();
    let body = if bundle {
        aux["restore_version"] = json!(1);
        aux["restore_ready"] = json!(true);
        let mut saved = crate::fx_saved::capture(&aux).unwrap();
        for config in saved["configurations"].as_array_mut().unwrap() {
            let mut c = crate::fx_aux::configuration(config.as_str().unwrap()).unwrap();
            c["returns"]["main"] = json!(0.25);
            *config = json!(c.to_string());
        }
        crate::fx_saved::mutation(&saved, &aux).unwrap()
    } else {
        body
    };
    let sent = Arc::new(Mutex::new(Vec::new()));
    let mut o = Operator::from_document_connection(
        Box::new(Peer {
            raw,
            aux,
            queue: VecDeque::new(),
            mode,
            sent: sent.clone(),
            changed: false,
            stale_sent: false,
        }),
        &session.snapshot.as_ref().unwrap().authority.show_id,
        9,
        "unused",
        "fx_configuration",
        2,
    )
    .unwrap();
    o.session = session;
    (o, sent, body)
}
#[test]
fn acceptance_requires_exact_applied_routes_owner_and_correlated_completion_without_retry() {
    for mode in [
        Mode::StaleTelemetry,
        Mode::Applied,
        Mode::Refused,
        Mode::Lost,
        Mode::WrongTarget,
        Mode::WrongOwner,
        Mode::PendingOwner,
        Mode::Replaced,
    ] {
        let (mut o, sent, body) = operator(mode);
        let result = o.mutate_fx_aux(body);
        match mode {
            Mode::Applied | Mode::StaleTelemetry => {
                result.unwrap();
                assert!(o.session.last_result.contains("applied and settled"));
            }
            Mode::Refused => {
                assert!(result.unwrap_err().contains("refused"));
                assert!(!o.session.fx_aux.unknown);
            }
            _ => {
                assert!(result.is_err());
                assert!(o.session.fx_aux.unknown);
                assert!(o.session.last_result.contains("UNKNOWN"));
            }
        }
        assert_eq!(
            sent.lock()
                .unwrap()
                .iter()
                .filter(|r| r["kind"] == "fx_aux_configure")
                .count(),
            1
        );
    }
}
#[test]
fn malformed_and_transport_failures_never_become_legacy_support_or_mutation_probes() {
    for mode in [Mode::Malformed, Mode::Transport] {
        let (mut o, sent, _) = operator(mode);
        assert!(o.refresh_fx_aux(true).is_err());
        assert!(o.session.fx_aux.receipt.is_none());
        let sent = sent.lock().unwrap();
        assert_eq!(sent.len(), 2);
        assert_eq!(sent[1]["body"], json!({"return_routing":true}));
    }
    let (mut o, sent, _) = operator(Mode::Applied);
    o.legacy_fx_aux().unwrap();
    assert!(
        o.session
            .fx_aux
            .snapshot
            .as_ref()
            .unwrap()
            .get("monitors")
            .is_none()
    );
    o.refresh_fx_aux(false).unwrap();
    assert!(
        sent.lock()
            .unwrap()
            .iter()
            .filter(|r| r["kind"] == "fx_aux_snapshot")
            .all(|r| r["body"] == json!({}))
    );
}

#[test]
fn replaced_owner_or_source_invalidates_review_and_unknown_needs_explicit_reconciliation() {
    let (mut o, _, body) = operator(Mode::Applied);
    o.stage_fx_aux(body).unwrap();
    assert!(o.review_valid());
    o.session.fx_aux.snapshot.as_mut().unwrap()["owner_binding"] = json!("b".repeat(64));
    assert!(!o.review_valid());
    o.session.context_changed();
    assert!(!o.review_valid());
    let (mut o, sent, body) = operator(Mode::Lost);
    assert!(o.mutate_fx_aux(body.clone()).is_err());
    assert!(o.stage_fx_aux(body.clone()).is_err());
    assert!(o.session.fx_aux.unknown);
    o.refresh_fx_aux(true).unwrap();
    assert!(!o.session.fx_aux.unknown);
    assert!(
        o.session
            .last_result
            .contains("prior outcome remains UNKNOWN")
    );
    assert!(o.session.pending.is_none());
    assert_eq!(
        sent.lock()
            .unwrap()
            .iter()
            .filter(|r| r["kind"] == "fx_aux_configure")
            .count(),
        1
    );
}

#[test]
fn bundle_completion_requires_all_four_and_lost_acceptance_never_replays() {
    for mode in [
        Mode::Applied,
        Mode::Refused,
        Mode::LostApplied,
        Mode::WrongTarget,
        Mode::WrongOwner,
        Mode::PendingOwner,
        Mode::Replaced,
    ] {
        let (mut o, sent, body) = operator_bundle(mode, true);
        let result = o.mutate_fx_aux(body.clone());
        match mode {
            Mode::Applied => {
                result.unwrap();
                assert!(o.session.last_result.contains("Four saved FX"));
            }
            Mode::Refused => {
                assert!(result.is_err());
                assert!(!o.session.fx_aux.unknown);
            }
            _ => {
                assert!(result.is_err());
                assert!(o.session.fx_aux.unknown);
                assert!(o.stage_fx_aux(body.clone()).is_err());
                o.refresh_fx_aux(true).unwrap();
                assert!(!o.session.fx_aux.unknown);
                assert!(
                    o.session
                        .last_result
                        .contains("prior outcome remains UNKNOWN")
                );
                if matches!(mode, Mode::PendingOwner | Mode::WrongOwner) {
                    let fresh = crate::fx_saved::mutation(
                        &body["restore"]["saved"],
                        o.session.fx_aux.snapshot.as_ref().unwrap(),
                    )
                    .unwrap();
                    assert!(o.stage_fx_aux(fresh).is_err());
                }
            }
        }
        assert_eq!(
            sent.lock()
                .unwrap()
                .iter()
                .filter(|r| r["kind"] == "fx_aux_configure")
                .count(),
            1
        );
        assert!(
            !sent
                .lock()
                .unwrap()
                .iter()
                .any(|r| r["kind"] == "grant" || r["kind"] == "rearm")
        );
    }
}

#[test]
fn incompatible_saved_preview_is_complete_and_cannot_mutate_or_grant() {
    let (mut o, sent, body) = operator_bundle(Mode::Applied, true);
    let mut saved = body["restore"]["saved"].clone();
    saved["inputs"][0]["physical_port"] = json!("replacement-port");
    o.session.fx_aux.saved = Some(saved);
    o.review_saved_fx_aux().unwrap();
    let review = o.reviewed().unwrap();
    assert!(review.contains("INCOMPATIBLE"));
    assert!(review.contains("replacement-port"));
    assert!(review.contains("Complete saved intent"));
    assert!(o.confirm().unwrap_err().contains("preview only"));
    assert!(!sent.lock().unwrap().iter().any(|r| matches!(
        r["kind"].as_str(),
        Some("fx_aux_configure" | "grant" | "renew" | "rearm")
    )));
}
