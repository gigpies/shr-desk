use serde_json::{Value, json};
use shr_desk::fx_aux::{self, State};
fn fixture() -> Value {
    serde_json::from_slice(include_bytes!("fixtures/gp-fx-aux/v1/owner.json")).unwrap()
}
#[test]
fn actual_producer_palette_and_strict_identity_bounds() {
    let v = fx_aux::snapshot(fixture()).unwrap();
    let palette: Value = serde_json::from_str(v["palette_json"].as_str().unwrap()).unwrap();
    assert_eq!(palette.as_array().unwrap().len(), 10);
    for key in ["version", "module", "library_sha256", "map_revision"] {
        let mut bad = v.clone();
        bad[key] = json!("bad");
        assert!(fx_aux::snapshot(bad).is_err(), "{key}");
    }
    let mut bad = v.clone();
    bad["unexpected"] = json!(true);
    assert!(fx_aux::snapshot(bad).is_err());
    let mut bad = v.clone();
    bad["inputs"][1]["id"] = bad["inputs"][0]["id"].clone();
    assert!(fx_aux::snapshot(bad).is_err());
    let mut bad = v.clone();
    let mut p = palette.clone();
    p[0]["parameters"][0]["maximum"] = json!(-1.);
    bad["palette_json"] = json!(p.to_string());
    assert!(fx_aux::snapshot(bad).is_err());
}
#[test]
fn stale_and_unsettled_edits_refuse_but_explicit_panic_remains_reachable() {
    let mut v = fx_aux::snapshot(fixture()).unwrap();
    let config =
        fx_aux::configuration(v["buses"][0]["configuration_json"].as_str().unwrap()).unwrap();
    let body = fx_aux::mutation(&v, 0, Some(config.clone())).unwrap();
    let mut state = State {
        saved: None,
        loaded_slot: None,
        snapshot: Some(v.clone()),
        receipt: Some(100),
        unknown: false,
    };
    assert!(state.validate(&body, 100).is_ok());
    assert!(state.validate(&body, 1101).is_err());
    state.unknown = true;
    assert!(state.validate(&body, 100).is_err());
    state.unknown = false;
    v["buses"][0]["generation"] = json!("2");
    state.snapshot = Some(v.clone());
    assert!(state.validate(&body, 100).is_err());
    assert!(
        state
            .validate(&fx_aux::mutation(&v, 0, Some(config)).unwrap(), 100)
            .is_err()
    );
    assert!(
        state
            .validate(&fx_aux::mutation(&v, 0, None).unwrap(), 100)
            .is_ok()
    );
    v["pending"] = json!(0);
    state.snapshot = Some(v.clone());
    assert!(
        state
            .validate(&fx_aux::mutation(&v, 0, None).unwrap(), 100)
            .is_err()
    );
}

#[test]
fn remote_owner_binding_fences_reconnects_even_when_generations_match() {
    let mut v = fixture();
    v["owner_binding"] = json!("a".repeat(64));
    let v = fx_aux::snapshot(v).unwrap();
    let config =
        fx_aux::configuration(v["buses"][0]["configuration_json"].as_str().unwrap()).unwrap();
    let body = fx_aux::mutation(&v, 0, Some(config)).unwrap();
    let mut state = State {
        saved: None,
        loaded_slot: None,
        snapshot: Some(v.clone()),
        receipt: Some(100),
        unknown: false,
    };
    assert!(state.validate(&body, 100).is_ok());
    let mut changed = v;
    changed["owner_binding"] = json!("b".repeat(64));
    state.snapshot = Some(changed);
    assert!(state.validate(&body, 100).is_err());
    let mut missing = body;
    missing.as_object_mut().unwrap().remove("owner_binding");
    assert!(state.validate(&missing, 100).is_err());
}

#[test]
fn actual_authenticated_brain_fixture_exposes_four_distinct_effects() {
    let v: Value =
        serde_json::from_slice(include_bytes!("fixtures/gp-fx-aux/v1/brain-owner.json")).unwrap();
    let v = fx_aux::snapshot(v).unwrap();
    assert!(v["owner_binding"].is_string());
    for (bus, id) in [1, 6, 8, 10].into_iter().enumerate() {
        let c =
            fx_aux::configuration(v["buses"][bus]["configuration_json"].as_str().unwrap()).unwrap();
        assert_eq!(c["settings"]["algorithm"], id);
        assert_eq!(c["sends"][bus], 1.);
        let body = fx_aux::mutation(&v, bus, Some(c)).unwrap();
        assert_eq!(body["owner_binding"], v["owner_binding"]);
    }
}

#[test]
fn routing_extension_requires_exact_versions_identities_dimensions_and_finite_levels() {
    for count in [0, 1, 3, 5, 17] {
        let mut v = fixture();
        v["return_routing_version"] = json!(1);
        v["monitors"] = json!(
            (1..=count)
                .map(|i| format!("monitor-{i}"))
                .collect::<Vec<_>>()
        );
        for b in v["buses"].as_array_mut().unwrap() {
            let mut c = fx_aux::configuration(b["configuration_json"].as_str().unwrap()).unwrap();
            c["returns"] = json!({"main":1.,"monitors":vec![0.;count]});
            b["configuration_json"] = json!(c.to_string());
        }
        assert!(fx_aux::snapshot(v.clone()).is_ok());
        for key in ["return_routing_version", "monitors"] {
            let mut bad = v.clone();
            bad.as_object_mut().unwrap().remove(key);
            assert!(fx_aux::snapshot(bad).is_err());
        }
        let mut bad = v.clone();
        bad["return_routing_version"] = json!(2);
        assert!(fx_aux::snapshot(bad).is_err());
        if count > 0 {
            let mut bad = v.clone();
            bad["monitors"][0] = json!("monitor-2");
            assert!(fx_aux::snapshot(bad).is_err());
        }
        for returns in [
            json!({"main":-0.1,"monitors":vec![0.;count]}),
            json!({"main":1.01,"monitors":vec![0.;count]}),
            json!({"main":null,"monitors":vec![0.;count]}),
            json!({"main":1.,"monitors":vec![1.01;count.max(1)]}),
            json!({"main":1.,"monitors":vec![Value::Null;count.max(1)]}),
            json!({"main":1.,"monitors":vec![0.;count+1]}),
            json!({"main":"NaN","monitors":vec![0.;count]}),
            json!({"main":1.,"monitors":vec![0.;count],"extra":0}),
        ] {
            let mut bad = v.clone();
            let mut c =
                fx_aux::configuration(bad["buses"][0]["configuration_json"].as_str().unwrap())
                    .unwrap();
            c["returns"] = returns;
            bad["buses"][0]["configuration_json"] = json!(c.to_string());
            assert!(fx_aux::snapshot(bad).is_err());
        }
    }
    assert!(fx_aux::configuration(r#"{"settings":{},"sends":[0],"tempo":null,"returns":{"main":0,"main":1,"monitors":[]}}"#).is_err());
}

#[test]
fn actual_return_producer_fixture_decodes_all_applied_routes() {
    for bytes in [
        include_bytes!("fixtures/gp-fx-aux/v1/return-owner.json").as_slice(),
        include_bytes!("fixtures/gp-fx-aux/v1/return-brain-owner.json").as_slice(),
    ] {
        let v: Value = serde_json::from_slice(bytes).unwrap();
        let v = fx_aux::snapshot(v).unwrap();
        assert_eq!(v["return_routing_version"], 1);
        assert_eq!(v["monitors"], json!(["monitor-1", "monitor-2"]));
        for b in v["buses"].as_array().unwrap() {
            let c = fx_aux::configuration(b["configuration_json"].as_str().unwrap()).unwrap();
            assert_eq!(c["returns"], json!({"main":0.95,"monitors":[0.05,0.1]}));
            assert_eq!(b["generation"], b["applied_generation"]);
        }
    }
}
