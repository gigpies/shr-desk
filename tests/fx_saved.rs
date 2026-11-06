use serde_json::{Value, json};
use shr_desk::{fx_aux, fx_saved};
fn snapshot() -> Value {
    let mut s: Value =
        serde_json::from_str(include_str!("fixtures/gp-fx-aux/v1/return-owner.json")).unwrap();
    s["restore_version"] = json!(1);
    s["restore_ready"] = json!(true);
    s
}
#[test]
fn saved_intent_roundtrip_strictness_and_route_identity() {
    let mut s = snapshot();
    s["inputs"][0]["physical_port"] = json!(format!("Stage Left {}", "x".repeat(80)));
    let saved = fx_saved::capture(&s).unwrap();
    let bytes = serde_json::to_vec(&saved).unwrap();
    assert_eq!(fx_saved::decode(&bytes).unwrap(), saved);
    for n in [0, 1, bytes.len() / 2, bytes.len() - 1] {
        assert!(fx_saved::decode(&bytes[..n]).is_err());
    }
    let mut bad = saved.clone();
    bad["armed"] = json!(true);
    assert!(fx_saved::validate(&bad).is_err());
    let mut bad = saved.clone();
    bad["version"] = json!(2);
    assert!(fx_saved::validate(&bad).is_err());
    assert!(fx_saved::decode(br#"{"version":1,"engine":{}}"#).is_err());
    let duplicate = String::from_utf8(bytes).unwrap().replacen(
        "\"version\":1",
        "\"version\":1,\"version\":1",
        1,
    );
    assert!(fx_saved::decode(duplicate.as_bytes()).is_err());
    for port in [String::new(), "x".repeat(129)] {
        let mut bad = saved.clone();
        bad["inputs"][0]["physical_port"] = json!(port);
        assert!(fx_saved::validate(&bad).is_err());
    }
    let mut changed = s.clone();
    changed["inputs"][0]["physical_port"] = json!("replacement");
    assert!(fx_saved::compatible(&saved, &changed).is_err());
    let body = fx_saved::mutation(&saved, &s).unwrap();
    assert_eq!(fx_saved::edits(&body).unwrap().len(), 4);
    let state = fx_aux::State {
        snapshot: Some(s.clone()),
        receipt: Some(100),
        unknown: false,
        saved: None,
        loaded_slot: None,
    };
    assert!(state.validate(&body, 100).is_ok());
    assert!(state.validate(&body, 1101).is_err());
    for bus in 0..4 {
        let mut b = body.clone();
        b["restore"]["expected_generations"][bus] = json!("99999");
        assert!(state.validate(&b, 100).is_err());
    }
    let mut bad = s;
    bad["buses"][3]["applied_generation"] = json!("999999");
    assert!(fx_saved::capture(&bad).is_err());
    let mut bad = snapshot();
    let mut owner: Value =
        serde_json::from_str(bad["buses"][3]["owner_json"].as_str().unwrap()).unwrap();
    owner["request_state"] = json!(99);
    bad["buses"][3]["owner_json"] = json!(owner.to_string());
    assert!(fx_saved::capture(&bad).is_err());
}
#[test]
fn saved_file_replace_and_bounded_corruption() {
    use std::os::unix::fs::PermissionsExt;
    let path = std::env::temp_dir().join(format!("desk-fx-saved-{}", std::process::id()));
    std::fs::create_dir(&path).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(path.clone());
    let saved = fx_saved::capture(&snapshot()).unwrap();
    fx_saved::save(&path, 0, &saved).unwrap();
    assert_eq!(fx_saved::load(&path, 0).unwrap(), saved);
    fx_saved::save(&path, 0, &saved).unwrap();
    assert_eq!(std::fs::read_dir(&path).unwrap().count(), 1);
    std::fs::write(path.join("fx-scene-01.json"), b"{truncated").unwrap();
    assert!(fx_saved::load(&path, 0).is_err());
}
