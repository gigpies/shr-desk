use super::*;
fn surface(monitors: Option<usize>) -> (Frontend, Receiver<Request>) {
    let (mut f, rx) = super::processing_tests::brain_surface();
    f.brain_page = false;
    f.fx_aux_ui.open = true;
    f.scope = "fx_configuration".into();
    let mut s: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/gp-fx-aux/v1/owner.json")).unwrap();
    // Deliberate dimension variants of the producer corpus; no hardware claim.
    if let Some(n) = monitors {
        s["return_routing_version"] = json!(1);
        s["monitors"] = json!((1..=n).map(|i| format!("monitor-{i}")).collect::<Vec<_>>());
        for bus in s["buses"].as_array_mut().unwrap() {
            let mut c =
                crate::fx_aux::configuration(bus["configuration_json"].as_str().unwrap()).unwrap();
            c["returns"] = json!({"main":1.,"monitors":vec![0.; n]});
            bus["configuration_json"] = json!(c.to_string());
        }
    }
    let u = f.state.as_mut().unwrap();
    u.fx_aux.snapshot = Some(crate::fx_aux::snapshot(s).unwrap());
    u.fx_aux.receipt = Some(0);
    u.fx_aux_age_ms = Some(0);
    (f, rx)
}
fn text(f: &Frontend) -> String {
    f.scene()
        .primitives
        .iter()
        .filter_map(|p| {
            if let Primitive::Text { value, .. } = p {
                Some(value.as_str())
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}
#[test]
fn every_aux_and_dynamic_return_is_reachable_through_production_keys_and_rendering() {
    for count in [0, 1, 3, 5, 17, 64] {
        let (mut f, rx) = surface(Some(count));
        for bus in 0..4 {
            f.fx_aux_ui.field = 9;
            let original = f.state.as_ref().unwrap().fx_aux.snapshot.clone().unwrap();
            for field in 9..10 + count {
                f.state.as_mut().unwrap().received = Instant::now();
                let scene = f.scene();
                assert!(scene.in_bounds(), "{count}/{bus}/{field}");
                assert!(text(&f).contains(if field == 9 {
                    "Main-only return"
                } else {
                    "monitor-"
                }));
                f.key(if field == 9 { "-" } else { "+" }).unwrap();
                let request = rx.try_recv().unwrap();
                let Operation::FxAux(crate::fx_aux::Operation::Review(body)) = &request.operation
                else {
                    panic!("wrong action")
                };
                assert_eq!(body["bus"], bus);
                let c = crate::fx_aux::configuration(body["configuration_json"].as_str().unwrap())
                    .unwrap();
                let mut expected = crate::fx_aux::configuration(
                    original["buses"][bus]["configuration_json"]
                        .as_str()
                        .unwrap(),
                )
                .unwrap();
                if field == 9 {
                    expected["returns"]["main"] = json!(0.95);
                } else {
                    expected["returns"]["monitors"][field - 10] = json!(0.05);
                }
                assert_eq!(c, expected, "untouched fields changed");
                drop(request);
                f.key("Down").unwrap();
            }
            assert_eq!(f.fx_aux_ui.field, 0);
            f.key("]").unwrap();
        }
    }
}
#[test]
fn independent_routing_survives_effect_send_tempo_bypass_and_legacy_is_explicit() {
    let (mut f, rx) = surface(Some(3));
    for (field, key) in [
        (0, "+"),
        (5, "-"),
        (6, "+"),
        (7, "+"),
        (8, "+"),
        (0, "D"),
        (0, "Space"),
    ] {
        f.fx_aux_ui.field = field;
        f.state.as_mut().unwrap().received = Instant::now();
        f.key(key).unwrap();
        let r = rx.try_recv().unwrap();
        let Operation::FxAux(crate::fx_aux::Operation::Review(body)) = &r.operation else {
            panic!()
        };
        let c = crate::fx_aux::configuration(body["configuration_json"].as_str().unwrap()).unwrap();
        assert_eq!(c["returns"], json!({"main":1.,"monitors":[0.,0.,0.]}));
    }
    let (mut f, rx) = surface(None);
    assert!(text(&f).contains("UNAVAILABLE"));
    f.fx_aux_ui.field = 9;
    assert!(f.key("-").is_err());
    assert!(rx.try_recv().is_err());
    f.key("L").unwrap();
    assert!(matches!(
        rx.try_recv().unwrap().operation,
        Operation::FxAux(crate::fx_aux::Operation::LegacyProbe)
    ));
    f.key("W").unwrap();
    assert!(matches!(
        rx.try_recv().unwrap().operation,
        Operation::FxAux(crate::fx_aux::Operation::Probe)
    ));
}
#[test]
fn stale_unknown_refused_pending_and_disarmed_are_visible_and_do_not_rearm() {
    let (mut f, rx) = surface(Some(5));
    f.state.as_mut().unwrap().fx_aux.unknown = true;
    assert!(text(&f).contains("UNKNOWN"));
    assert!(f.key("+").is_err());
    f.state.as_mut().unwrap().fx_aux.unknown = false;
    f.state.as_mut().unwrap().fx_aux_age_ms = Some(1001);
    assert!(text(&f).contains("stale"));
    assert!(f.key("+").is_err());
    let u = f.state.as_mut().unwrap();
    u.fx_aux_age_ms = Some(0);
    u.last_operation = Some("FX aux refused: stale lease".into());
    u.fx_aux.snapshot.as_mut().unwrap()["pending"] = json!(0);
    u.snapshot.as_mut().unwrap().clock = Some(crate::topology::Clock {
        domain: "single-interface".into(),
        sample_rate: 48000,
        epoch: 1,
        next_frame: 0,
        state: crate::topology::ClockState::Disarmed,
        adat_lock: crate::topology::LockEvidence::Unknown,
        mapping_verified: false,
        fault: None,
    });
    assert!(text(&f).contains("DISARMED"));
    assert!(text(&f).contains("refused"));
    assert!(text(&f).contains("PENDING"));
    assert!(rx.try_recv().is_err());
}

#[test]
fn saved_controls_use_semantic_menu_keyboard_and_rendered_slot_selection() {
    let (mut f, rx) = surface(Some(3));
    f.fx_aux_ui.directory = Some(std::path::PathBuf::from("/explicit/private/shows"));
    let items = f.controller_items();
    for action in [
        crate::fx_aux::Action::Save,
        crate::fx_aux::Action::Load,
        crate::fx_aux::Action::ReviewSaved,
        crate::fx_aux::Action::SavedProbe,
    ] {
        assert!(
            items
                .iter()
                .any(|i| i.action == Action::FxAux(action.clone()))
        );
    }
    f.key(".").unwrap();
    assert_eq!(f.fx_aux_ui.saved_slot, 1);
    assert!(rx.try_recv().is_err());
    assert!(text(&f).contains("slot 2/16"));
    f.key("S").unwrap();
    assert!(matches!(
        rx.try_recv().unwrap().operation,
        Operation::FxAux(crate::fx_aux::Operation::Save(_, 1))
    ));
    f.key("O").unwrap();
    assert!(matches!(
        rx.try_recv().unwrap().operation,
        Operation::FxAux(crate::fx_aux::Operation::Load(_, 1))
    ));
    assert!(f.key("V").unwrap_err().contains("Load the selected"));
    assert!(rx.try_recv().is_err());
    f.state.as_mut().unwrap().fx_aux.loaded_slot = Some(1);
    f.state.as_mut().unwrap().fx_aux.saved = Some(serde_json::json!({}));
    f.key("V").unwrap();
    assert!(matches!(
        rx.try_recv().unwrap().operation,
        Operation::FxAux(crate::fx_aux::Operation::ReviewSaved)
    ));
    f.key(".").unwrap();
    assert!(text(&f).contains("from slot 2"));
    assert!(f.key("V").unwrap_err().contains("Load the selected"));
    assert!(rx.try_recv().is_err());
    assert!(f.scene().in_bounds());
}
