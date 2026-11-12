//! Read-only GigPies analysis process; Desk owns no signal analysis.
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceFile {
    pub path: PathBuf,
    dev: u64,
    ino: u64,
    size: u64,
    mtime: i64,
    mtime_nsec: i64,
    ctime: i64,
    ctime_nsec: i64,
}
impl EvidenceFile {
    pub fn unchanged(&self) -> bool {
        use std::os::unix::fs::MetadataExt;
        self.path.metadata().is_ok_and(|m| {
            m.is_file()
                && m.dev() == self.dev
                && m.ino() == self.ino
                && m.len() == self.size
                && m.mtime() == self.mtime
                && m.mtime_nsec() == self.mtime_nsec
                && m.ctime() == self.ctime
                && m.ctime_nsec() == self.ctime_nsec
        })
    }
}
#[derive(Debug)]
pub struct ResultDocument {
    pub bytes: Vec<u8>,
    pub files: Vec<EvidenceFile>,
}
pub struct Job {
    cancel: Arc<AtomicBool>,
    pub progress: Arc<AtomicUsize>,
    result: mpsc::Receiver<Result<ResultDocument, String>>,
}
static ACTIVE: AtomicBool = AtomicBool::new(false);
impl Job {
    pub fn start(executable: PathBuf, request: Vec<u8>) -> Result<Self, String> {
        if !executable.is_absolute() || request.len() > 512 * 1024 {
            return Err("absolute GigPies executable and bounded request required".into());
        }
        if ACTIVE.swap(true, Ordering::AcqRel) {
            return Err("previous preparation is stopping; retry after completion".into());
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let progress = Arc::new(AtomicUsize::new(0));
        let (tx, result) = mpsc::sync_channel(1);
        let c = cancel.clone();
        let p = progress.clone();
        std::thread::Builder::new()
            .name("gate-analysis".into())
            .spawn(move || {
                struct Release;
                impl Drop for Release {
                    fn drop(&mut self) {
                        ACTIVE.store(false, Ordering::Release);
                    }
                }
                let _release = Release;
                let _ = tx.send(run(executable, request, &c, &p));
            })
            .map_err(|e| {
                ACTIVE.store(false, Ordering::Release);
                e.to_string()
            })?;
        Ok(Self {
            cancel,
            progress,
            result,
        })
    }
    pub fn poll(&self) -> Option<Result<ResultDocument, String>> {
        match self.result.try_recv() {
            Ok(r) => Some(r),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(_) => Some(Err("analysis worker lost".into())),
        }
    }
}
impl Drop for Job {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
    }
}
fn run(
    executable: PathBuf,
    request: Vec<u8>,
    cancel: &AtomicBool,
    progress: &AtomicUsize,
) -> Result<ResultDocument, String> {
    use std::{
        io::{Read, Write},
        os::fd::AsRawFd,
        process::{Command, Stdio},
    };
    struct Child(std::process::Child);
    impl Drop for Child {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    use std::os::unix::process::CommandExt;
    let parent = std::process::id();
    let mut command = Command::new(executable);
    // The helper cannot outlive Desk even on abrupt exit. Only async-signal-safe calls.
    unsafe {
        command.pre_exec(move || {
            if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) != 0
                || libc::getppid() as u32 != parent
            {
                return Err(std::io::Error::other("analysis parent disappeared"));
            }
            Ok(())
        });
    }
    let mut child = Child(
        command
            .arg("gate-prepare-stream")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("GigPies analysis launch: {e}"))?,
    );
    let input = child.0.stdin.take().ok_or("analysis stdin")?;
    let mut out = child.0.stdout.take().ok_or("analysis stdout")?;
    for fd in [input.as_raw_fd(), out.as_raw_fd()] {
        // Only our owned pipes; nonblocking prevents provider failure from wedging cancellation.
        if unsafe { libc::fcntl(fd, libc::F_SETFL, libc::O_NONBLOCK) } < 0 {
            return Err("analysis pipe setup".into());
        }
    }
    let end = Instant::now() + Duration::from_secs(120);
    let mut sent = 0;
    let mut input = Some(input);
    let mut pending = Vec::new();
    let mut count = 0;
    let mut final_result = None;
    let mut exited: Option<std::process::ExitStatus> = None;
    loop {
        if cancel.load(Ordering::Acquire) {
            return Err("preparation cancelled; nothing applied".into());
        }
        if Instant::now() >= end {
            return Err("preparation exceeded 120 second bound; nothing applied".into());
        }
        if let Some(writer) = &mut input {
            match writer.write(&request[sent..]) {
                Ok(n) => sent += n,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(e) => return Err(format!("analysis input: {e}")),
            }
            if sent == request.len() {
                input = None;
            }
        }
        let mut block = [0; 8192];
        loop {
            match out.read(&mut block) {
                Ok(0) => break,
                Ok(n) => {
                    count += n;
                    if count > 256 * 1024 {
                        return Err("analysis response capacity".into());
                    }
                    pending.extend_from_slice(&block[..n]);
                    while let Some(at) = pending.iter().position(|b| *b == b'\n') {
                        let line: Vec<_> = pending.drain(..=at).collect();
                        let v: serde_json::Value = serde_json::from_slice(&line)
                            .map_err(|e| format!("analysis protocol: {e}"))?;
                        if let Some(e) = v["error"].as_str() {
                            return Err(e.to_owned());
                        }
                        if let Some(n) = v["progress"].as_u64() {
                            progress.store(n.min(128) as usize, Ordering::Release);
                        } else {
                            if final_result.is_some() {
                                return Err("duplicate analysis completion".into());
                            }
                            let files: Vec<EvidenceFile> =
                                serde_json::from_value(v["files"].clone())
                                    .map_err(|e| e.to_string())?;
                            if !(32..=128).contains(&files.len())
                                || !files.iter().all(EvidenceFile::unchanged)
                            {
                                return Err("evidence changed or missing".into());
                            }
                            final_result = Some(ResultDocument {
                                bytes: serde_json::to_vec(&v["document"])
                                    .map_err(|e| e.to_string())?,
                                files,
                            });
                        }
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) => return Err(format!("analysis output: {e}")),
            }
        }
        if let Some(status) = exited {
            if !status.success() {
                return Err(format!(
                    "GigPies analysis failed ({status}); no proposal applied"
                ));
            }
            if !pending.is_empty() {
                return Err("incomplete analysis response".into());
            }
            return final_result.ok_or("GigPies exited without completed analysis".into());
        }
        exited = child.0.try_wait().map_err(|e| e.to_string())?;
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn replacement_even_at_same_length_invalidates_evidence() {
        use std::os::unix::fs::MetadataExt;
        let path = std::env::temp_dir().join(format!("gate-stamp-{}", std::process::id()));
        std::fs::write(&path, "first").unwrap();
        let m = path.metadata().unwrap();
        let evidence = EvidenceFile {
            path: path.clone(),
            dev: m.dev(),
            ino: m.ino(),
            size: m.len(),
            mtime: m.mtime(),
            mtime_nsec: m.mtime_nsec(),
            ctime: m.ctime(),
            ctime_nsec: m.ctime_nsec(),
        };
        assert!(evidence.unchanged());
        std::fs::write(&path, "other").unwrap();
        assert!(!evidence.unchanged());
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn failed_provider_cancel_and_next_job_recover_without_proposal() {
        let end = Instant::now() + Duration::from_secs(3);
        let job = Job::start("/bin/false".into(), b"{}".to_vec()).unwrap();
        loop {
            if let Some(result) = job.poll() {
                assert!(result.is_err());
                break;
            }
            assert!(Instant::now() < end);
            std::thread::sleep(Duration::from_millis(5));
        }
        drop(job);
        while ACTIVE.load(Ordering::Acquire) {
            assert!(Instant::now() < end);
            std::thread::yield_now();
        }
        let job = Job::start("/bin/cat".into(), vec![b' '; 512 * 1024]).unwrap();
        drop(job);
        while ACTIVE.load(Ordering::Acquire) {
            assert!(Instant::now() < end);
            std::thread::sleep(Duration::from_millis(5));
        }
        let job = Job::start("/nonexistent-gigpies".into(), b"{}".to_vec()).unwrap();
        loop {
            if let Some(result) = job.poll() {
                assert!(result.unwrap_err().contains("launch"));
                break;
            }
            assert!(Instant::now() < end);
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}
