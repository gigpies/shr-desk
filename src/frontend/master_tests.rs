use super::*;
use crate::controller_surface::Command as C;
fn surface() -> (Frontend, Receiver<Request>) {
    let (mut f, rx) = super::processing_tests::brain_surface();
    f.brain_page = false;
    f.brain_enabled = false;
    f.scope = "foh".into();
    f.selected_master = true;
    f.controller_identity = Some("software".into());
    let s = f.state.as_mut().unwrap().snapshot.as_mut().unwrap();
    s.master_gain = Some(crate::audio::MasterGain {
        version: 1,
        target: "main".into(),
        current_nanogain: 1_000_000_000,
        target_nanogain: 1_000_000_000,
    });
    s.authority.parameters.push(crate::provider::Parameter {
        target: crate::provider::Target {
            parameter: "master_gain".into(),
            input: String::new(),
            monitor: None,
        },
        actual: None,
        target_value: json!(0),
        proposal: None,
        hold: None,
        owner: None,
    });
    f.master_revision = Some(
        f.state
            .as_ref()
            .unwrap()
            .snapshot
            .as_ref()
            .unwrap()
            .authority
            .revision
            .clone(),
    );
    (f, rx)
}
#[test]
fn strict_capability_and_legacy_refusal() {
    let (f, _) = surface();
    let s = f.state.unwrap().snapshot.unwrap();
    let mut v = serde_json::to_value(&s).unwrap();
    crate::audio::decode_snapshot(&serde_json::to_vec(&v).unwrap()).unwrap();
    for malformed in [
        json!(null),
        json!({"version":2,"target":"main","current_nanogain":1000000000,"target_nanogain":1000000000}),
    ] {
        v["master_gain"] = malformed;
        assert!(crate::audio::decode_snapshot(&serde_json::to_vec(&v).unwrap()).is_err());
    }
    let mut legacy_null = serde_json::to_value(&s).unwrap();
    legacy_null["authority"]["parameters"]
        .as_array_mut()
        .unwrap()
        .retain(|p| p["target"]["parameter"] != "master_gain");
    legacy_null["master_gain"] = json!(null);
    assert!(crate::audio::decode_snapshot(&serde_json::to_vec(&legacy_null).unwrap()).is_err());
    let (mut f, _) = surface();
    f.state
        .as_mut()
        .unwrap()
        .snapshot
        .as_mut()
        .unwrap()
        .master_gain = None;
    assert!(
        f.action(Action::ExactEdit(crate::exact_value::Parameter::Fader))
            .is_err()
    );
    assert!(f.action(Action::Mute).is_err());
    assert!(f.action(Action::AdjustPan(1)).is_err());
}
#[test]
fn menu_exact_draft_review_render_and_stale_revision() {
    let (mut f, rx) = surface();
    let action = f
        .controller_items()
        .into_iter()
        .find(|i| i.label == "Exact Master gain")
        .unwrap()
        .action;
    f.inject_controller(action).unwrap();
    f.pump();
    assert!(f.exact_draft.is_some());
    assert!(f.action(Action::ExactText("6.1".into())).is_err());
    f.action(Action::ExactText("-12.3".into())).unwrap();
    assert!(f.scene().in_bounds());
    f.action(Action::ExactApply).unwrap();
    assert!(
        matches!(rx.try_iter().find(|r| !matches!(r.operation, Operation::InputReleased)).unwrap().operation, Operation::ReviewSet { target, value } if target == json!({"parameter":"master_gain"}) && value == json!(-12300))
    );
    f.state
        .as_mut()
        .unwrap()
        .snapshot
        .as_mut()
        .unwrap()
        .authority
        .revision = "999".into();
    assert!(f.action(Action::ExactApply).is_err());
    assert!(rx.try_recv().is_err());
}
#[test]
fn motion_is_single_pending_and_center_or_loss_cancels_queued_token() {
    let (mut f, rx) = surface();
    f.master_clock = Instant::now() - Duration::from_millis(100);
    f.master_input = Some(Instant::now());
    f.master_speed.input(-1., 0, 1, true);
    f.pump();
    let request = rx.try_recv().unwrap();
    let Operation::MasterMotion { value, guard } = &request.operation else {
        panic!("motion expected");
    };
    assert!((-55..=-45).contains(value));
    assert!(guard.valid());
    f.pump();
    assert!(rx.try_recv().is_err());
    f.stop_master_motion();
    assert!(!guard.valid());
    assert!(f.master_pending.is_none());
    drop(request);
    f.pump();
    assert!(rx.try_recv().is_err());
    f.console_command(C::Reconnect).unwrap();
    assert!(rx.try_iter().all(|r| !matches!(
        r.operation,
        Operation::MasterMotion { .. } | Operation::Grant | Operation::Set { .. }
    )));
}
#[test]
fn motion_stale_input_permission_and_context_stop() {
    for cause in 0..4 {
        let (mut f, rx) = surface();
        f.master_clock = Instant::now() - Duration::from_millis(100);
        f.master_input = Some(Instant::now());
        f.master_speed.input(1., 0, 1, true);
        match cause {
            0 => f.master_input = Some(Instant::now() - Duration::from_millis(251)),
            1 => f.provider.authorization.store(false, Ordering::Release),
            2 => f.focused = false,
            _ => {
                f.master_clock = Instant::now() - Duration::from_millis(300);
                f.master_fraction = -0.0006;
            }
        }
        f.pump();
        assert!(rx.try_recv().is_err());
        assert!(f.master_input.is_none());
        assert_eq!(f.master_fraction, 0.);
    }
}

#[test]
fn master_relative_upward_guard_preserves_millidecibel_movement() {
    let (mut f, rx) = surface();
    f.inject_controller(Action::Console(C::GuardedLevel {
        target: crate::controller_flow::Target::Master,
        steps: 1,
    }))
    .unwrap();
    f.pump();
    assert!(rx.try_iter().any(|r| matches!(r.operation, Operation::Set { target, value } if target == json!({"parameter":"master_gain"}) && value == json!(20))));
}

#[test]
fn equal_values_without_correlated_completion_and_concurrent_revisions_stop_motion() {
    for lost in [true, false] {
        let (mut f, rx) = surface();
        f.master_input = Some(Instant::now());
        f.master_speed.input(-1., 0, f.provider.generation(), true);
        if lost {
            let s = f.state.as_ref().unwrap().snapshot.as_ref().unwrap();
            // Even fresh equal values and the expected revision are not a completion.
            f.master_pending = Some((s.authority.revision.clone(), Instant::now()));
            f.state.as_mut().unwrap().last_operation = None;
        } else {
            f.state
                .as_mut()
                .unwrap()
                .snapshot
                .as_mut()
                .unwrap()
                .authority
                .revision = "999".into();
        }
        f.pump();
        assert!(rx.try_recv().is_err());
        assert!(f.master_input.is_none());
        assert_eq!(f.master_unknown, lost);
    }
}

#[test]
fn stopped_input_with_lost_completion_and_unresolved_pending_never_restart() {
    for expired in [true, false] {
        let (mut f, rx) = surface();
        f.master_clock = Instant::now() - Duration::from_millis(100);
        f.master_input = Some(Instant::now());
        f.master_speed.input(-1., 0, f.provider.generation(), true);
        if expired {
            f.master_input = Some(Instant::now() - Duration::from_millis(251));
            f.master_pending = Some(("2".into(), Instant::now() - Duration::from_millis(251)));
        } else {
            f.state.as_mut().unwrap().last_operation = Some("PENDING parameter edit".into());
        }
        f.pump();
        assert!(rx.try_recv().is_err());
        assert!(f.master_input.is_none());
        assert_eq!(f.master_unknown, expired);
    }
}
