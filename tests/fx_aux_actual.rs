//! Repeatable software-only producer/Frontend episode; no physical endpoint.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use shr_desk::{
    actions::Action,
    frontend::{Config, Event, Frontend},
    fx_aux::Action as A,
};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};
fn wait(f: &mut Frontend, label: &str, ready: impl Fn(&Frontend) -> bool) {
    let end = Instant::now() + Duration::from_secs(5);
    loop {
        f.pump();
        if ready(f) {
            return;
        }
        assert!(
            Instant::now() < end,
            "{label}: {} / {:?}",
            f.message,
            f.state.as_ref().map(|u| (&u.status, &u.last_operation))
        );
        thread::sleep(Duration::from_millis(5));
    }
}
fn tap(f: &mut Frontend, key: &str) {
    for pressed in [true, false] {
        f.enqueue(Event::Key {
            key: key.into(),
            pressed,
        })
        .unwrap();
        f.pump();
    }
}
fn act(f: &mut Frontend, a: A) {
    f.inject_controller(Action::FxAux(a)).unwrap();
    f.pump();
}
fn confirm(f: &mut Frontend) {
    wait(f, "review", |f| {
        f.state.as_ref().is_some_and(|u| u.review.is_some())
    });
    f.synchronize_review();
    for i in 0..f.review_pages() {
        if i > 0 {
            tap(f, "PageDown");
        }
        assert!(f.scene().in_bounds());
        f.mark_presented();
    }
    tap(f, "Enter");
    wait(f, "settled", |f| {
        f.state.as_ref().is_some_and(|u| {
            u.review.is_none()
                && u.last_operation
                    .as_ref()
                    .is_some_and(|s| s.contains("applied and settled"))
        })
    });
}
fn snapshot(f: &Frontend) -> Value {
    f.state.as_ref().unwrap().fx_aux.snapshot.clone().unwrap()
}
#[test]
#[ignore = "GP_FX_PROVIDER_EXE and GP_FX_LIBRARY absolute paths; owned synthetic child only"]
fn actual_four_aux_frontend_review_and_readback() {
    let exe = PathBuf::from(std::env::var("GP_FX_PROVIDER_EXE").unwrap());
    let library = PathBuf::from(std::env::var("GP_FX_LIBRARY").unwrap());
    assert!(exe.is_absolute() && library.is_absolute());
    let hash = format!("{:x}", Sha256::digest(fs::read(&library).unwrap()));
    let dir = std::env::temp_dir().join(format!("desk-fx-aux-test-{}", std::process::id()));
    fs::create_dir(&dir).unwrap();
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).unwrap();
    struct Cleanup {
        child: Option<std::process::Child>,
        dir: PathBuf,
    }
    impl Drop for Cleanup {
        fn drop(&mut self) {
            if let Some(c) = &mut self.child {
                let _ = c.kill();
                let _ = c.wait();
            }
            let _ = fs::remove_dir_all(&self.dir);
        }
    }
    let mut cleanup = Cleanup {
        child: None,
        dir: dir.clone(),
    };
    let topology = json!({"identity":"software-fx-aux","mapping_evidence":"synthetic","map_revision":1,"sample_rate":48000,"max_block_frames":4096,"capture_channels":16,"playback_channels":4,
        "inputs":(0..16).map(|i|json!({"id":format!("input-{:02}",i+1),"capture_slot":i,"physical_port":format!("synthetic-capture-{}",i+1)})).collect::<Vec<_>>(),
        "measurement_slots":[],"monitors":2,"pa_outputs":0,
        "outputs":(0..4).map(|i|json!({"id":format!("output-{:02}",i+1),"playback_slot":i,"physical_port":format!("synthetic-playback-{}",i+1),"source":if i<2{json!({"kind":"main","channel":i})}else{json!({"kind":"monitor","index":i-2})}})).collect::<Vec<_>>()});
    fs::write(dir.join("topology.json"), topology.to_string()).unwrap();
    let show = "11111111-1111-4111-8111-111111111111";
    cleanup.child = Some(
        Command::new(exe)
            .args([
                "--directory",
                dir.to_str().unwrap(),
                "--show",
                show,
                "--epoch",
                "9",
                "--ticks",
                "20000",
                "--topology",
                dir.join("topology.json").to_str().unwrap(),
                "--arm-synthetic",
                "--fx-aux-library",
                library.to_str().unwrap(),
                "--fx-aux-sha256",
                &hash,
            ])
            .stdout(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let end = Instant::now() + Duration::from_secs(5);
    while !dir.join("audio.sock").exists() {
        assert!(Instant::now() < end);
        assert!(
            cleanup
                .child
                .as_mut()
                .unwrap()
                .try_wait()
                .unwrap()
                .is_none()
        );
        thread::sleep(Duration::from_millis(5));
    }
    let mut f = Frontend::new(Config {
        wire_version: 2,
        remote: None,
        endpoint: dir.join("audio.sock"),
        show: show.into(),
        epoch: 9,
        writer: "fx-aux-desk".into(),
        scope: "fx_configuration".into(),
    });
    wait(&mut f, "read-only attach", Frontend::fresh);
    act(&mut f, A::Open);
    wait(&mut f, "actual palette", |f| {
        f.state.as_ref().is_some_and(|u| {
            u.fx_aux
                .snapshot
                .as_ref()
                .is_some_and(|s| s["available"] == true)
        })
    });
    assert!(!f.state.as_ref().unwrap().writer_granted());
    assert!(f.scene().in_bounds());
    tap(&mut f, "G");
    wait(&mut f, "grant", |f| {
        f.state.as_ref().is_some_and(|s| s.writer_granted())
    });
    let old_receipt = f.state.as_ref().unwrap().fx_aux.receipt;
    act(&mut f, A::Probe);
    wait(&mut f, "fresh completed probe", |f| {
        f.fresh()
            && f.state.as_ref().is_some_and(|u| {
                u.fx_aux.receipt != old_receipt && u.fx_aux_age_ms.is_some_and(|t| t < 250)
            })
    });
    let before = snapshot(&f);
    act(&mut f, A::Adjust(1));
    confirm(&mut f);
    let after = snapshot(&f);
    let c =
        shr_desk::fx_aux::configuration(after["buses"][0]["configuration_json"].as_str().unwrap())
            .unwrap();
    assert_eq!(c["sends"][0], 0.05);
    for i in 1..4 {
        assert_eq!(
            before["buses"][i]["configuration_json"],
            after["buses"][i]["configuration_json"]
        );
    }
    act(&mut f, A::ChooseAlgorithm(8));
    confirm(&mut f);
    act(&mut f, A::Field(6));
    act(&mut f, A::Adjust(1));
    confirm(&mut f);
    act(&mut f, A::Field(1));
    act(&mut f, A::Adjust(1));
    confirm(&mut f);
    act(&mut f, A::Field(1));
    act(&mut f, A::Adjust(1));
    confirm(&mut f);
    let timed = shr_desk::fx_aux::configuration(
        snapshot(&f)["buses"][0]["configuration_json"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        timed["tempo"],
        json!({"bpm":121.,"denominator":8,"modifier":1})
    );
    assert!(
        (timed["settings"]["time_ms"].as_f64().unwrap() - 240000. / 121. / 8. * 1.5).abs() < 1e-9
    );
    act(&mut f, A::Bus(3));
    act(&mut f, A::Algorithm(1));
    confirm(&mut f);
    assert_eq!(
        shr_desk::fx_aux::configuration(
            snapshot(&f)["buses"][3]["configuration_json"]
                .as_str()
                .unwrap()
        )
        .unwrap()["settings"]["algorithm"],
        2
    );
    act(&mut f, A::Bypass);
    confirm(&mut f);
    act(&mut f, A::Panic);
    confirm(&mut f);
    let retained = snapshot(&f);
    act(&mut f, A::Algorithm(1));
    wait(&mut f, "cancel review", |f| {
        f.state.as_ref().is_some_and(|s| s.review.is_some())
    });
    tap(&mut f, "Esc");
    wait(&mut f, "cancelled", |f| {
        f.state.as_ref().is_some_and(|s| s.review.is_none())
    });
    assert_eq!(
        snapshot(&f)["buses"][3]["configuration_json"],
        retained["buses"][3]["configuration_json"]
    );
    drop(f);
    let mut reconnect = Frontend::new(Config {
        wire_version: 2,
        remote: None,
        endpoint: dir.join("audio.sock"),
        show: show.into(),
        epoch: 9,
        writer: "fx-aux-reconnect".into(),
        scope: "fx_configuration".into(),
    });
    wait(&mut reconnect, "read-only reconnect", Frontend::fresh);
    assert!(!reconnect.state.as_ref().unwrap().writer_granted());
    act(&mut reconnect, A::Open);
    wait(&mut reconnect, "retained provider", |f| {
        f.state
            .as_ref()
            .is_some_and(|u| u.fx_aux.snapshot.is_some())
    });
    assert_eq!(
        snapshot(&reconnect)["buses"][3]["configuration_json"],
        retained["buses"][3]["configuration_json"]
    );
}
