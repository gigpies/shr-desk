//! Actual integrated workflow, explicitly supplied trusted software provider only.
use shr_desk::frontend::{Config, Event, Frontend};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};
const SHOW: &str = "11111111-1111-4111-8111-111111111111";
struct Service {
    child: Child,
    dir: std::path::PathBuf,
}
impl Drop for Service {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_dir_all(&self.dir);
    }
}
fn wait(f: &mut Frontend, p: impl Fn(&Frontend) -> bool) {
    let end = Instant::now() + Duration::from_secs(5);
    loop {
        f.pump();
        if p(f) {
            return;
        }
        assert!(Instant::now() < end, "timeout: {} {:?}", f.message, f.state);
        thread::sleep(Duration::from_millis(10));
    }
}
fn tap(f: &mut Frontend, key: &str) {
    wait(f, |f| f.fresh());
    if key == "Enter" {
        let _ = f.scene();
        f.mark_presented();
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
fn rec(f: &Frontend) -> Option<&shr_desk::modules::Recording> {
    f.state
        .as_ref()?
        .recorder
        .observation
        .as_ref()?
        .recording
        .as_ref()
}
#[test]
#[ignore = "explicit SHR_DESK_GP05 and SHR_DESK_GP05_MANIFEST; actual provider, finite synthetic software only"]
fn actual_record_review_readback_reconnect_stop_and_storage_failure() {
    exercise(1);
    exercise(2);
}
fn exercise(version: u8) {
    let dir = std::env::temp_dir().join(format!("desk-rec-{}", std::process::id()));
    fs::create_dir(&dir).unwrap();
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).unwrap();
    let topology = serde_json::json!({
        "identity":"software-16-2-0","mapping_evidence":"synthetic","map_revision":1,
        "sample_rate":48000,"max_block_frames":4096,"capture_channels":16,"playback_channels":4,
        "inputs":(0..16).map(|i|serde_json::json!({"id":format!("input-{:02}",i+1),"capture_slot":i,"physical_port":format!("synthetic-capture-{}",i+1)})).collect::<Vec<_>>(),
        "measurement_slots":[],"monitors":2,"pa_outputs":0,
        "outputs":(0..4).map(|i|serde_json::json!({"id":format!("output-{:02}",i+1),"playback_slot":i,"physical_port":format!("synthetic-playback-{}",i+1),"source":if i<2 {serde_json::json!({"kind":"main","channel":i})}else{serde_json::json!({"kind":"monitor","index":i-2})}})).collect::<Vec<_>>()});
    fs::write(
        dir.join("topology.json"),
        serde_json::to_vec(&topology).unwrap(),
    )
    .unwrap();
    let mut command = Command::new(std::env::var_os("SHR_DESK_GP05").unwrap());
    if version == 2 {
        command.arg("--topology").arg(dir.join("topology.json"));
    }
    let child = command
        .args([
            "--directory",
            dir.to_str().unwrap(),
            "--show",
            SHOW,
            "--epoch",
            "9",
            "--synthetic-source",
            "fouraux",
            "--modules",
        ])
        .arg(std::env::var_os("SHR_DESK_GP05_MANIFEST").unwrap())
        .args(["--ticks", "60000"])
        .stdout(Stdio::null())
        .stderr(Stdio::from(fs::File::create(dir.join("host.log")).unwrap()))
        .spawn()
        .unwrap();
    let mut service = Service {
        child,
        dir: dir.clone(),
    };
    let end = Instant::now() + Duration::from_secs(4);
    while !fs::read_to_string(dir.join("host.log"))
        .unwrap_or_default()
        .contains("GP03-rendered:")
    {
        assert!(service.child.try_wait().unwrap().is_none());
        assert!(Instant::now() < end);
        thread::sleep(Duration::from_millis(10));
    }
    let config = Config {
        wire_version: version,
        remote: None,
        endpoint: dir.join("audio.sock"),
        show: SHOW.into(),
        epoch: 9,
        writer: "rec-desk".into(),
        scope: "foh".into(),
    };
    let mut f = Frontend::new(config.clone());
    wait(&mut f, |f| f.fresh());
    f.sends_page = true; // Switching from sends must not retain its S-key handler.
    tap(&mut f, "J");
    wait(&mut f, |f| {
        f.state.as_ref().is_some_and(|s| s.recorder.fresh())
    });
    tap(&mut f, "N");
    wait(&mut f, |f| {
        f.state.as_ref().is_some_and(|s| s.review.is_some())
    });
    tap(&mut f, "Enter");
    wait(&mut f, |f| {
        f.state.as_ref().is_some_and(|s| {
            s.last_operation
                .as_ref()
                .is_some_and(|s| s.contains("REFUSED"))
        })
    });
    assert!(rec(&f).is_none(), "missing lease must refuse capture");
    tap(&mut f, "G");
    wait(&mut f, |f| {
        f.state
            .as_ref()
            .is_some_and(|s| s.writer_lease_remaining_ms.is_some())
    });
    tap(&mut f, "N");
    wait(&mut f, |f| {
        f.state.as_ref().is_some_and(|s| s.review.is_some())
    });
    assert!(rec(&f).is_none(), "review must not start capture");
    tap(&mut f, "Enter");
    wait(&mut f, |f| {
        rec(f).is_some_and(|r| {
            r.state == "recording" && r.accepted_frames.parse::<u64>().unwrap() > 0
        })
    });
    let first = rec(&f).unwrap().clone();
    drop(f); // Recording belongs to the host, not the UI or its lease.
    thread::sleep(Duration::from_millis(2100));
    let mut config = config;
    config.writer = "rec-desk-reconnected".into();
    let mut f = Frontend::new(config);
    wait(&mut f, |f| f.fresh());
    tap(&mut f, "J");
    wait(&mut f, |f| {
        rec(f).is_some_and(|r| {
            r.take_id == first.take_id
                && r.accepted_frames.parse::<u64>().unwrap()
                    > first.accepted_frames.parse::<u64>().unwrap()
        })
    });
    assert!(
        f.state
            .as_ref()
            .unwrap()
            .writer_lease_remaining_ms
            .is_none()
    );
    tap(&mut f, "G");
    wait(&mut f, |f| {
        f.state
            .as_ref()
            .is_some_and(|s| s.writer_lease_remaining_ms.is_some())
    });
    tap(&mut f, "S");
    wait(&mut f, |f| {
        f.state.as_ref().is_some_and(|s| s.review.is_some())
    });
    tap(&mut f, "Enter");
    wait(&mut f, |f| rec(f).is_some_and(|r| r.state == "finalized"));
    let done = rec(&f).unwrap();
    assert_eq!(done.outcome, "complete");
    assert_eq!(done.accepted_frames, done.written_frames);
    assert!(done.durable_frames.is_none());
    let result: serde_json::Value = serde_json::from_slice(
        &fs::read(dir.join("takes").join(&done.take_id).join("result.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(result["status"], "complete");
    let first_frame = done
        .first_source_frame
        .as_ref()
        .unwrap()
        .parse::<u64>()
        .unwrap();
    let frames = done.written_frames.parse::<usize>().unwrap();
    for ch in 0..if version == 1 { 8 } else { 16 } {
        let bytes = fs::read(
            dir.join("takes")
                .join(&done.take_id)
                .join(format!("channel-{:03}.wav", ch + 1)),
        )
        .unwrap();
        assert_eq!(&bytes[..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WAVE");
        let mut at = 12;
        while &bytes[at..at + 4] != b"data" {
            let n = u32::from_le_bytes(bytes[at + 4..at + 8].try_into().unwrap()) as usize;
            at += 8 + n + (n % 2);
        }
        let n = u32::from_le_bytes(bytes[at + 4..at + 8].try_into().unwrap()) as usize;
        assert_eq!(n, frames * 3);
        for (i, b) in bytes[at + 8..at + 8 + n].chunks_exact(3).enumerate() {
            let actual =
                i32::from_le_bytes([b[0], b[1], b[2], if b[2] & 128 != 0 { 255 } else { 0 }]);
            let frame = first_frame + i as u64;
            let expected = if ch < 8 {
                ((frame % 4096) as i32 - 2048) * (ch + 1) * 32
            } else {
                (((frame as i64 * 17 + ch as i64 * 7919) % 65537) - 32768) as i32
            };
            assert_eq!(actual, expected, "stem {} source frame {}", ch + 1, frame);
        }
    }

    // Actual create-new storage refusal, not a simulated successful ACK.
    tap(&mut f, "N");
    wait(&mut f, |f| {
        f.state.as_ref().is_some_and(|s| s.review.is_some())
    });
    let review = &f.state.as_ref().unwrap().review.as_ref().unwrap().1;
    let body_start = review.find('{').unwrap();
    let body_end = review[body_start..].find('}').unwrap() + body_start + 1;
    let body: serde_json::Value = serde_json::from_str(&review[body_start..body_end]).unwrap();
    fs::create_dir(dir.join("takes").join(body["take_id"].as_str().unwrap())).unwrap();
    tap(&mut f, "Enter");
    wait(&mut f, |f| {
        rec(f).is_some_and(|r| r.take_id == body["take_id"] && r.state == "finalized")
    });
    assert_eq!(rec(&f).unwrap().outcome, "error");
    assert!(rec(&f).unwrap().durable_frames.is_none());
    tap(&mut f, "N");
    wait(&mut f, |f| {
        f.state.as_ref().is_some_and(|s| s.review.is_some())
    });
    tap(&mut f, "Enter");
    wait(&mut f, |f| rec(f).is_some_and(|r| r.state == "recording"));
    let take = rec(&f).unwrap().take_id.clone();
    fs::write(
        dir.join("takes").join(&take).join("channel-001.wav"),
        b"preserve",
    )
    .unwrap();
    tap(&mut f, "S");
    wait(&mut f, |f| {
        f.state.as_ref().is_some_and(|s| s.review.is_some())
    });
    tap(&mut f, "Enter");
    wait(&mut f, |f| {
        rec(f).is_some_and(|r| r.take_id == take && r.state == "finalized")
    });
    assert_eq!(rec(&f).unwrap().outcome, "error");
    assert!(rec(&f).unwrap().writer_fault);
    assert_eq!(
        fs::read(dir.join("takes").join(&take).join("channel-001.wav")).unwrap(),
        b"preserve"
    );
    drop(f);
}
