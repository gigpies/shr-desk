use super::*;
use crate::{
    local_audio::Operator,
    setup_saved::{Action as A, Saved},
};
use std::{
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};
const SHOW: &str = "11111111-1111-4111-8111-111111111111";
struct Host {
    path: PathBuf,
    child: Option<Child>,
}
impl Host {
    fn start(path: &Path, epoch: u64) -> Self {
        std::fs::write(path.join("test-epoch"), epoch.to_string()).unwrap();
        let child = Command::new(std::env::var_os("GP_SETUP_PROVIDER_EXE").unwrap())
            .args([
                "--ignored",
                "--exact",
                "serve_audio_setup",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("GP_SETUP_DIRECTORY", path)
            .env("GP_SETUP_EPOCH", epoch.to_string())
            .stdout(Stdio::null())
            .spawn()
            .unwrap();
        let mut host = Self {
            path: path.into(),
            child: Some(child),
        };
        let deadline = Instant::now() + Duration::from_secs(10);
        while !path.join("audio.sock").exists() {
            assert!(Instant::now() < deadline);
            assert!(host.child.as_mut().unwrap().try_wait().unwrap().is_none());
            thread::sleep(Duration::from_millis(5));
        }
        host
    }
    fn stop(&mut self) {
        std::fs::write(self.path.join("stop"), "").unwrap();
        assert!(self.child.take().unwrap().wait().unwrap().success());
        let r: Value =
            serde_json::from_slice(&std::fs::read(self.path.join("result.json")).unwrap()).unwrap();
        assert!(r["silent_samples"].as_u64().unwrap() > 0);
        eprintln!("setup witness silent samples: {}", r["silent_samples"]);
        std::fs::remove_file(self.path.join("stop")).unwrap();
        std::fs::remove_file(self.path.join("result.json")).unwrap();
    }
}
impl Drop for Host {
    fn drop(&mut self) {
        if let Some(c) = &mut self.child {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}
fn op(path: &Path, scope: &str, writer: &str) -> Operator {
    let mut o = Operator::connect_version(
        &path.join("audio.sock"),
        SHOW,
        std::fs::read_to_string(path.join("test-epoch"))
            .unwrap()
            .parse()
            .unwrap(),
        writer,
        scope,
        2,
    )
    .unwrap();
    o.refresh().unwrap();
    o.mutate(
        "grant",
        json!({"scope":crate::scopes::value(scope).unwrap()}),
    )
    .unwrap();
    o.session.input_released();
    o
}
fn wait(f: &mut Frontend, label: &str, p: impl Fn(&Frontend) -> bool) {
    let end = Instant::now() + Duration::from_secs(10);
    loop {
        f.pump();
        if p(f) {
            return;
        }
        assert!(
            Instant::now() < end,
            "{label}: {:?} / {}",
            f.state.as_ref().map(|s| (&s.status, &s.last_operation)),
            f.message
        );
        thread::sleep(Duration::from_millis(5));
    }
}
fn tap(f: &mut Frontend, key: &str) {
    if key == "G" {
        wait(f, "fresh authority before grant", Frontend::fresh);
    }
    for pressed in [true, false] {
        f.enqueue(Event::Key {
            key: key.into(),
            pressed,
        })
        .unwrap();
        f.pump();
    }
}
fn submit_confirm(f: &mut Frontend) {
    wait(f, "review", |f| {
        f.state.as_ref().is_some_and(|s| s.review.is_some())
    });
    f.synchronize_review();
    for i in 0..f.review_pages() {
        if i > 0 {
            f.action(Action::Console(
                crate::controller_surface::Command::ReviewPage(1),
            ))
            .unwrap();
        }
        assert!(f.scene().in_bounds());
        f.mark_presented();
    }
    tap(f, "Enter");
}
fn confirm(f: &mut Frontend) {
    submit_confirm(f);
    wait(f, "completion", |f| {
        f.state.as_ref().is_some_and(|s| {
            s.review.is_none()
                && s.last_operation
                    .as_ref()
                    .is_some_and(|r| r.contains("applied") || r.starts_with("SETUP scope"))
        })
    });
}
fn attach(path: &Path, scenes: &Path, scope: &str) -> Frontend {
    let mut f = Frontend::new(Config {
        wire_version: 2,
        remote: None,
        endpoint: path.join("audio.sock"),
        show: SHOW.into(),
        epoch: std::fs::read_to_string(path.join("test-epoch"))
            .unwrap()
            .parse()
            .unwrap(),
        writer: format!("setup-ui-{scope}"),
        scope: scope.into(),
    });
    f.setup_directory = Some(scenes.into());
    wait(&mut f, "attach", Frontend::fresh);
    f
}
#[test]
#[ignore = "GP_SETUP_PROVIDER_EXE and actual PA/FX/REC library manifest environment; synthetic provider only"]
fn actual_setup_save_restart_review_restore_frontend() {
    episode(0);
}
#[test]
#[ignore = "actual libraries and bounded synthetic provider; interruption/reconnect acceptance"]
fn actual_setup_interruption_reconnect_frontend() {
    episode(1);
}
#[test]
#[ignore = "actual libraries and bounded synthetic provider; stale review acceptance"]
fn actual_setup_stale_review_refuses_before_mutation() {
    episode(2);
}
#[test]
#[ignore = "actual libraries and bounded synthetic provider; replaced-file review acceptance"]
fn actual_setup_replaced_file_refuses_before_mutation() {
    episode(3);
}
#[test]
#[ignore = "actual GigPies analysis/provider and PA/FX/REC libraries; recorded-gate save/recall software episode"]
fn actual_setup_prepared_gate_save_and_recall() {
    episode(4);
}

fn prepare_recorded_gate(path: &Path, scenes: &Path) {
    use super::gate_preparation::Command as G;
    let mut groups = Vec::new();
    for group in 0usize..4 {
        let dir = path.join(format!("gate-cues-{group}"));
        std::fs::create_dir(&dir).unwrap();
        for index in 0..8 {
            let mut pcm = Vec::new();
            for n in 0..24000 {
                let t = n as f64 / 48000.;
                let hit = if group.is_multiple_of(2) {
                    (0.12 + (index + group) as f64 * 0.001)
                        * (-t / 0.04).exp()
                        * (std::f64::consts::TAU * 200. * t).sin()
                } else {
                    0.
                };
                let bleed = (0.001 + (group * 8 + index) as f64 * 0.00001)
                    * (std::f64::consts::TAU * 4000. * t).sin();
                pcm.extend_from_slice(
                    &(((hit + bleed) * 8388608.).round() as i32).to_le_bytes()[..3],
                );
            }
            let mut wav = Vec::new();
            wav.extend_from_slice(b"RIFF");
            wav.extend_from_slice(&(36 + pcm.len() as u32).to_le_bytes());
            wav.extend_from_slice(b"WAVEfmt ");
            wav.extend_from_slice(&16u32.to_le_bytes());
            wav.extend_from_slice(&1u16.to_le_bytes());
            wav.extend_from_slice(&1u16.to_le_bytes());
            wav.extend_from_slice(&48000u32.to_le_bytes());
            wav.extend_from_slice(&144000u32.to_le_bytes());
            wav.extend_from_slice(&3u16.to_le_bytes());
            wav.extend_from_slice(&24u16.to_le_bytes());
            wav.extend_from_slice(b"data");
            wav.extend_from_slice(&(pcm.len() as u32).to_le_bytes());
            wav.extend(pcm);
            std::fs::write(dir.join(format!("{index}.wav")), wav).unwrap();
        }
        groups.push(dir);
    }
    let mut f = attach(path, scenes, "foh");
    f.enable_processing().unwrap();
    wait(&mut f, "gate processing", Frontend::processing_ready);
    tap(&mut f, "G");
    wait(&mut f, "gate grant", |f| {
        f.state.as_ref().is_some_and(|u| u.writer_granted())
    });
    f.action(Action::Page(Page::Channel)).unwrap();
    wait(
        &mut f,
        "fresh Channel for gate preparation",
        Frontend::processing_ready,
    );
    f.gate_command(G::Open).unwrap();
    for (index, value) in [
        std::env::var("GP_GATE_PREPARE").unwrap(),
        "calibration".into(),
        "held-out".into(),
        groups[0].display().to_string(),
        groups[1].display().to_string(),
        groups[2].display().to_string(),
        groups[3].display().to_string(),
    ]
    .into_iter()
    .enumerate()
    {
        if index > 0 {
            f.gate_command(G::Field(1)).unwrap();
        }
        f.gate_command(G::Text(value)).unwrap();
        f.gate_command(G::Add).unwrap();
    }
    f.gate_command(G::Start).unwrap();
    wait(&mut f, "gate prepared", |f| {
        f.gate_preparation
            .report
            .iter()
            .any(|s| s.starts_with("Prepared |"))
    });
    f.gate_command(G::Use).unwrap();
    tap(&mut f, "F4");
    wait(&mut f, "gate review for file guard", |f| {
        f.state.as_ref().is_some_and(|u| u.review.is_some())
    });
    let id = f.state.as_ref().unwrap().review.as_ref().unwrap().0;
    let revision = f
        .state
        .as_ref()
        .unwrap()
        .processing
        .as_ref()
        .unwrap()
        .revision
        .clone();
    let changed = groups[0].join("0.wav");
    let original = std::fs::read(&changed).unwrap();
    std::fs::write(&changed, b"changed after frontend confirmation admission").unwrap();
    // Independently test the worker guard, after UI-side confirmation validation.
    f.provider
        .send(Some(revision.clone()), Operation::Confirm(id))
        .unwrap();
    wait(&mut f, "worker refused changed gate evidence", |f| {
        f.state
            .as_ref()
            .and_then(|u| u.last_operation.as_deref())
            .is_some_and(|s| s.contains("gate evidence changed before submission"))
    });
    assert_eq!(
        f.state
            .as_ref()
            .unwrap()
            .processing
            .as_ref()
            .unwrap()
            .revision,
        revision
    );
    f.action(Action::Cancel).unwrap();
    std::fs::write(&changed, original).unwrap();
    wait(
        &mut f,
        "fresh gate after refusal",
        Frontend::processing_ready,
    );
    f.gate_command(G::Open).unwrap();
    f.gate_command(G::Start).unwrap();
    wait(&mut f, "reprepared gate", |f| {
        f.gate_preparation
            .report
            .iter()
            .any(|s| s.starts_with("Prepared |"))
    });
    f.gate_command(G::Use).unwrap();
    tap(&mut f, "F4");
    confirm(&mut f);
    wait(&mut f, "gate correlated and settled", |f| {
        f.message.starts_with("Gate applied at frame")
    });
    assert!(
        !f.state
            .as_ref()
            .unwrap()
            .processing
            .as_ref()
            .unwrap()
            .channels[0]
            .current
            .gate
            .bypass
    );
    f.action(Action::Console(
        crate::controller_surface::Command::ReleaseWriter,
    ))
    .unwrap();
    wait(&mut f, "gate release", |f| {
        f.state.as_ref().is_some_and(|s| !s.writer_granted())
    });
}
fn episode(mode: u8) {
    use std::os::unix::fs::PermissionsExt;
    let root = std::env::temp_dir().join(format!("desk-complete-setup-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    struct Cleanup(PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(root.clone());
    let path = root.join("provider");
    let scenes = root.join("scenes");
    for d in [&path, &scenes] {
        std::fs::create_dir(d).unwrap();
        std::fs::set_permissions(d, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let mut host = Host::start(&path, 9);
    if mode == 4 {
        prepare_recorded_gate(&path, &scenes);
    }
    let mut o = op(&path, "foh", "configure-foh");
    let mut desired = o.capture_setup().unwrap();
    for e in &mut desired.composed.engine.parameters {
        match e.target.parameter.as_str() {
            "fader" => e.value = json!(-8000),
            "pan" => e.value = json!(20),
            "mute" => e.value = json!(true),
            "master_gain" => e.value = json!(-3000),
            "send" => e.value = json!(-12000),
            _ => unreachable!(),
        }
    }
    desired.composed.engine.processing[0].eq_bypass = false;
    desired.composed.engine.processing[0].band2_gain_mdb = 3000;
    desired.composed.engine.processing[0].gate.bypass = false;
    desired.composed.engine.processing[0].compressor_bypass = false;
    desired.composed.engine.processing[0].threshold_mdb = -18000;
    desired.composed.engine.processing[0].ratio_milli = 3000;
    desired.composed.engine.send_taps[0][0] = crate::sends::Tap::ProcessedPreFader;
    desired.composed.engine.topology.outputs[0].source = None;
    desired.composed.brain.source = crate::brain::Source::Pfl { input: 0 };
    desired.composed.brain.monitor_gain_cdb = -2400;
    desired.composed.brain.monitor_mute = false;
    desired.composed.brain.monitor_dim = true;
    desired.composed.brain.talkback_gain_cdb = -1800;
    desired.composed.brain.talkback_mute = false;
    desired.composed.brain.talkback_monitors = vec![1];
    let mut pa: Value =
        serde_json::from_str(desired.composed.pa_configuration_json.as_ref().unwrap()).unwrap();
    pa["inputs"][0]["gain_db"] = json!(-3.0);
    pa["inputs"][0]["eq"][0]["db"] = json!(-2.5);
    pa["inputs"][1]["eq"][0]["db"] = json!(-2.5);
    desired.composed.pa_configuration_json = Some(pa.to_string());
    let fx = desired.fx.as_mut().unwrap();
    for i in 0..4 {
        let mut c =
            crate::fx_aux::configuration(fx["configurations"][i].as_str().unwrap()).unwrap();
        c["sends"][0] = json!(0.05 * (i + 1) as f64);
        fx["configurations"][i] = json!(c.to_string());
    }
    o.mutate("release", json!({})).unwrap();
    drop(o);
    // Configure through the same existing live commands used by Desk controls.
    for (index, scope) in [
        "foh",
        "monitor1",
        "monitor2",
        "pa_configuration",
        "output_routes",
        "local_operator_monitor",
        "talkback_destinations",
        "fx_configuration",
    ]
    .iter()
    .enumerate()
    {
        let mut o = op(&path, scope, &format!("configure-{index}"));
        let current = o.capture_setup().unwrap();
        for step in desired
            .steps(&current)
            .unwrap()
            .into_iter()
            .filter(|s| s.scope == *scope)
        {
            o.capture_setup().unwrap();
            let body = if step.kind == "fx_aux_configure" {
                crate::fx_saved::mutation(
                    desired.fx.as_ref().unwrap(),
                    o.session.fx_aux.snapshot.as_ref().unwrap(),
                )
                .unwrap()
            } else {
                step.body
            };
            o.mutate(&step.kind, body).unwrap();
            let deadline = Instant::now() + Duration::from_secs(1);
            while o.capture_setup().is_err() {
                assert!(Instant::now() < deadline, "configuration did not settle");
            }
        }
        let _ = o.mutate("release", json!({}));
    }
    let mut f = attach(&path, &scenes, "foh");
    f.action(Action::Setup(A::Save)).unwrap();
    wait(&mut f, "save", |f| {
        f.state.as_ref().is_some_and(|s| {
            s.last_operation
                .as_ref()
                .is_some_and(|r| r.contains("saved durably"))
        })
    });
    let saved = Saved::load(&scenes, 0).unwrap();
    if mode == 4 {
        let value = serde_json::to_value(&saved).unwrap();
        let text = value.to_string();
        for forbidden in [
            "calibration",
            "held-out",
            "gate-cues",
            "grant",
            "job",
            "execution",
        ] {
            assert!(
                !text.contains(forbidden),
                "saved execution state: {forbidden}"
            );
        }
        assert!(!saved.composed.engine.processing[0].gate.bypass);
    }
    assert!(
        desired.steps(&saved).unwrap().is_empty(),
        "{}",
        desired.review(&saved)
    );
    drop(f);
    host.stop();
    host = Host::start(&path, 10);
    let mut f = attach(&path, &scenes, "pa_configuration");
    f.action(Action::Setup(A::Review)).unwrap();
    wait(&mut f, "restart review", |f| {
        f.state.as_ref().is_some_and(|s| s.review.is_some())
    });
    assert!(
        f.state
            .as_ref()
            .unwrap()
            .review
            .as_ref()
            .unwrap()
            .1
            .contains("CURRENT -> SAVED")
    );
    f.action(Action::Cancel).unwrap();
    wait(&mut f, "cancel", |f| {
        f.state.as_ref().is_some_and(|s| s.review.is_none())
    });
    tap(&mut f, "G");
    wait(&mut f, "grant", |f| {
        f.state.as_ref().is_some_and(|s| s.writer_granted())
    });
    f.action(Action::Setup(A::Hold)).unwrap();
    confirm(&mut f);
    if mode == 1 {
        f.action(Action::SwitchScope("foh".into())).unwrap();
        wait(&mut f, "failure scope", |f| f.fresh() && f.scope() == "foh");
        tap(&mut f, "G");
        wait(&mut f, "failure grant", |f| {
            f.state.as_ref().is_some_and(|s| s.writer_granted())
        });
        f.action(Action::Setup(A::Restore)).unwrap();
        let receipt = scenes.join("audio-recall-01.json");
        let stop = path.join("stop");
        let observer = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                if std::fs::read(&receipt)
                    .ok()
                    .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
                    .is_some_and(|v| v["scopes"]["foh"]["state"] == "UNKNOWN")
                {
                    std::fs::write(stop, "").unwrap();
                    return;
                }
                assert!(Instant::now() < deadline, "no durable before-send receipt");
                thread::sleep(Duration::from_millis(1));
            }
        });
        submit_confirm(&mut f);
        wait(&mut f, "interrupted recall", |f| {
            f.state.as_ref().is_some_and(|s| {
                s.last_operation
                    .as_ref()
                    .is_some_and(|r| r.contains("PARTIAL/UNKNOWN"))
            })
        });
        observer.join().unwrap();
        let journal = crate::setup_saved::Journal::read(&scenes, 0, &saved).unwrap();
        assert!(matches!(
            journal.scopes["foh"]["state"].as_str(),
            Some("UNKNOWN" | "applied-readback-unavailable")
        ));
        drop(f);
        host.stop();
        host = Host::start(&path, 11);
        f = attach(&path, &scenes, "pa_configuration");
        f.action(Action::Setup(A::Verify)).unwrap();
        wait(&mut f, "recovery review", |f| {
            f.state.as_ref().is_some_and(|s| s.review.is_some())
        });
        assert!(
            !f.state
                .as_ref()
                .unwrap()
                .review
                .as_ref()
                .unwrap()
                .1
                .contains("scope-complete")
        );
        f.action(Action::Cancel).unwrap();
        wait(&mut f, "recovery cancel", |f| {
            f.state.as_ref().is_some_and(|s| s.review.is_none())
        });
        tap(&mut f, "G");
        wait(&mut f, "recovery grant", |f| {
            f.state.as_ref().is_some_and(|s| s.writer_granted())
        });
        f.action(Action::Setup(A::Hold)).unwrap();
        confirm(&mut f);
    }
    if mode == 2 || mode == 3 {
        f.action(Action::SwitchScope("foh".into())).unwrap();
        wait(&mut f, "stale scope", |f| f.fresh() && f.scope() == "foh");
        tap(&mut f, "G");
        wait(&mut f, "stale grant", |f| {
            f.state.as_ref().is_some_and(|s| s.writer_granted())
        });
        f.action(Action::Setup(A::Restore)).unwrap();
        wait(&mut f, "stale review", |f| {
            f.state.as_ref().is_some_and(|s| s.review.is_some())
        });
        if mode == 2 {
            let mut concurrent = op(&path, "monitor1", "concurrent-settings");
            concurrent.mutate("set",json!({"targets":[{"target":{"parameter":"send","input":saved.composed.engine.topology.inputs[0].id,"monitor":"monitor-1"},"value":-6000}]})).unwrap();
            concurrent.mutate("release", json!({})).unwrap();
            drop(concurrent);
        } else {
            let mut changed = saved.clone();
            changed.composed.brain.monitor_gain_cdb -= 100;
            changed.save(&scenes, 0).unwrap();
        }
        submit_confirm(&mut f);
        wait(&mut f, "stale refusal", |f| {
            f.state.as_ref().is_some_and(|s| {
                s.last_operation
                    .as_ref()
                    .is_some_and(|r| r.contains("stale") || r.contains("changed"))
            })
        });
        assert!(
            crate::setup_saved::Journal::read(&scenes, 0, &saved)
                .unwrap()
                .scopes
                .is_empty()
        );
        saved.save(&scenes, 0).unwrap();
        f.action(Action::Cancel).unwrap();
        wait(&mut f, "stale cancelled", |f| {
            f.state.as_ref().is_some_and(|s| s.review.is_none())
        });
    }
    for scope in [
        "foh",
        "monitor1",
        "monitor2",
        "pa_configuration",
        "output_routes",
        "local_operator_monitor",
        "talkback_destinations",
        "fx_configuration",
    ] {
        f.action(Action::SwitchScope(scope.into())).unwrap();
        wait(&mut f, "scope attach", |f| f.fresh() && f.scope() == scope);
        tap(&mut f, "G");
        wait(&mut f, "grant", |f| {
            f.state.as_ref().is_some_and(|s| s.writer_granted())
        });
        f.action(Action::Setup(A::Restore)).unwrap();
        confirm(&mut f);
    }
    f.action(Action::Setup(A::Verify)).unwrap();
    wait(&mut f, "verify", |f| {
        f.state.as_ref().is_some_and(|s| s.review.is_some())
    });
    let text = &f.state.as_ref().unwrap().review.as_ref().unwrap().1;
    assert!(text.contains("0 differences requiring commands"), "{text}");
    assert!(text.contains("scope-complete"));
    drop(f);
    let mut o = op(&path, "pa_configuration", "final-read");
    let actual = o.capture_setup().unwrap();
    assert!(saved.steps(&actual).unwrap().is_empty());
    assert!(o.session.structural.as_ref().unwrap().outputs_quiesced);
    assert_eq!(
        o.session.structural.as_ref().unwrap().setup_hold,
        Some(true)
    );
    assert!(!o.session.brain.as_ref().unwrap().monitor_armed);
    drop(o);
    host.stop();
}
