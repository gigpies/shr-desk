//! Profile-scoped controller semantics. No MIDI device is opened here.
use crate::{midi::Relative, processing::Field};
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    Input(String),
    Master,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Dynamics {
    #[default]
    Compressor,
    Gate,
}
#[derive(Clone, Debug, PartialEq)]
pub enum Intent {
    SelectSlot(usize),
    SelectMaster,
    Processing {
        field: Field,
        steps: i32,
    },
    Pan {
        target: Target,
        steps: i32,
    },
    Level {
        target: Target,
        steps: i32,
    },
    Pad {
        bank: usize,
        index: usize,
        pressed: bool,
    },
    MasterSpeed(f64),
}
#[derive(Clone, Debug)]
pub struct Rotary {
    pub cc: u8,
    pub encoding: Relative,
    pub alternate: Option<(u8, Relative)>,
}
#[derive(Clone, Debug)]
pub struct Profile {
    pub endpoint: String,
    pub rotary_channel: u8,
    pub rotaries: [Rotary; 16],
    /// Exact captured complete messages. None means general held Shift is unavailable.
    pub shift: Option<(Vec<u8>, Vec<u8>)>,
    pub key_channel: u8,
    pub anchor: u8,
    pub pad_channel: u8,
    pub pads: [[u8; 8]; 2],
    pub pitch_channel: u8,
    /// Explicit profile capability; never inferred from pitch bend or finger release.
    pub pitch_returns_to_center: bool,
    pub hardware_qualified: bool,
}
impl Profile {
    pub fn validate(&self) -> Result<(), String> {
        if self.endpoint.is_empty()
            || self.endpoint.len() > 256
            || self.anchor > 103
            || [
                self.rotary_channel,
                self.key_channel,
                self.pad_channel,
                self.pitch_channel,
            ]
            .iter()
            .any(|c| *c > 15)
        {
            return Err("controller profile identity/channel".into());
        }
        let mut ccs = Vec::new();
        for (i, r) in self.rotaries.iter().enumerate() {
            if r.cc > 127 || ccs.contains(&r.cc) {
                return Err("rotary binding collision".into());
            }
            ccs.push(r.cc);
            if let Some((cc, _)) = r.alternate {
                if ![0, 8].contains(&i) || cc > 127 || ccs.contains(&cc) {
                    return Err("alternate binding".into());
                }
                ccs.push(cc);
            }
        }
        let mut notes = Vec::new();
        for note in self.pads.iter().flatten() {
            if *note > 127
                || notes.contains(note)
                || (self.pad_channel == self.key_channel
                    && (self.anchor..=self.anchor + 24).contains(note))
            {
                return Err("pad binding collision".into());
            }
            notes.push(*note);
        }
        if let Some((down, up)) = &self.shift {
            if down == up
                || [down, up]
                    .iter()
                    .any(|m| m.is_empty() || m.len() > 64 || m[0] < 0x80)
            {
                return Err("modifier messages".into());
            }
            for m in [down, up] {
                if m.len() == 3 && m[0] == 0xb0 | self.rotary_channel && ccs.contains(&m[1]) {
                    return Err("modifier rotary collision".into());
                }
            }
        }
        Ok(())
    }
}
pub struct Input {
    pub profile: Profile,
    shift: bool,
    shift_blocked: bool,
    generation: u64,
    last_ms: Option<u64>,
    context: Option<(Target, Dynamics)>,
    held: [[bool; 8]; 2],
    blocked: [[bool; 8]; 2],
}
impl Input {
    pub fn new(profile: Profile) -> Result<Self, String> {
        profile.validate()?;
        Ok(Self {
            profile,
            shift: false,
            shift_blocked: false,
            generation: 0,
            last_ms: None,
            context: None,
            held: [[false; 8]; 2],
            blocked: [[false; 8]; 2],
        })
    }
    pub fn reset(&mut self, generation: u64) {
        self.shift_blocked |= self.shift;
        self.shift = false;
        self.generation = generation;
        self.last_ms = None;
        self.context = None;
        self.held = [[false; 8]; 2];
        self.blocked = [[true; 8]; 2];
    }
    pub fn decode(
        &mut self,
        endpoint: &str,
        bytes: &[u8],
        generation: u64,
        now_ms: u64,
        target: Target,
        mode: Dynamics,
    ) -> Option<Intent> {
        if endpoint != self.profile.endpoint || generation < self.generation {
            return None;
        }
        if generation > self.generation {
            self.reset(generation);
        }
        if self
            .last_ms
            .is_some_and(|last| now_ms < last || now_ms - last > 500)
        {
            self.reset(generation);
        }
        if self
            .context
            .as_ref()
            .is_some_and(|(t, m)| t != &target || *m != mode)
        {
            self.reset(generation);
        }
        self.context = Some((target.clone(), mode));
        self.last_ms = Some(now_ms);
        if let Some((down, up)) = &self.profile.shift {
            if bytes == down {
                self.shift = !self.shift_blocked;
                return None;
            }
            if bytes == up {
                self.shift_blocked = false;
                self.shift = false;
                return None;
            }
        }
        if bytes.len() != 3 || bytes[1] > 127 || bytes[2] > 127 {
            return None;
        }
        let channel = bytes[0] & 15;
        let kind = bytes[0] & 0xf0;
        if kind == 0xe0
            && channel == self.profile.pitch_channel
            && self.profile.pitch_returns_to_center
        {
            return Some(Intent::MasterSpeed(
                (f64::from(u16::from(bytes[1]) | (u16::from(bytes[2]) << 7)) - 8192.) / 8192.,
            ));
        }
        if kind == 0xb0 && channel == self.profile.rotary_channel {
            if self.shift_blocked {
                return None;
            }
            // One match produces one action. Alternate CC carries Shift only for that turn.
            for (index, r) in self.profile.rotaries.iter().enumerate() {
                let binding = if bytes[1] == r.cc {
                    Some((r.encoding, self.shift))
                } else {
                    r.alternate
                        .filter(|(cc, _)| *cc == bytes[1])
                        .map(|(_, e)| (e, true))
                };
                if let Some((encoding, shift)) = binding {
                    return rotary(index, encoding.delta(bytes[2])? as i32, shift, mode, target);
                }
            }
        }
        if matches!(kind, 0x80 | 0x90) && channel == self.profile.pad_channel {
            for bank in 0..2 {
                for index in 0..8 {
                    if bytes[1] == self.profile.pads[bank][index] {
                        let pressed = kind == 0x90 && bytes[2] != 0;
                        if !pressed {
                            self.blocked[bank][index] = false;
                            if std::mem::replace(&mut self.held[bank][index], false) {
                                return Some(Intent::Pad {
                                    bank,
                                    index,
                                    pressed: false,
                                });
                            }
                        } else if !self.shift
                            && !self.shift_blocked
                            && !self.blocked[bank][index]
                            && !self.held[bank][index]
                        {
                            self.held[bank][index] = true;
                            return Some(Intent::Pad {
                                bank,
                                index,
                                pressed: true,
                            });
                        }
                        return None;
                    }
                }
            }
        }
        // Shift+pad and Shift+key belong to the controller's memory/channel functions.
        if kind == 0x90
            && channel == self.profile.key_channel
            && bytes[2] != 0
            && !self.shift
            && !self.shift_blocked
        {
            let slot = bytes[1].checked_sub(self.profile.anchor)? as usize;
            return match slot {
                0..=23 => Some(Intent::SelectSlot(slot)),
                24 => Some(Intent::SelectMaster),
                _ => None,
            };
        }
        None // CC1 is deliberately unassigned.
    }
}
pub fn rotary(
    index: usize,
    steps: i32,
    shift: bool,
    mode: Dynamics,
    target: Target,
) -> Option<Intent> {
    if index >= 16 || steps == 0 {
        return None;
    }
    let steps = steps.clamp(-8, 8);
    let col = index % 8;
    if col == 7 {
        return Some(if index < 8 {
            Intent::Pan { target, steps }
        } else {
            Intent::Level { target, steps }
        });
    }
    if matches!(target, Target::Master) {
        return None;
    } // no silent selected-channel substitution
    let field = if col < 4 {
        let base = 1 + col * 4;
        crate::processing::FIELDS[base
            + if index < 8 {
                if shift { 2 } else { 0 }
            } else {
                1
            }]
    } else {
        match (mode, col, index < 8) {
            (Dynamics::Compressor, 4, true) => Field::Threshold,
            (Dynamics::Compressor, 4, false) => Field::Attack,
            (Dynamics::Compressor, 5, true) => Field::Ratio,
            (Dynamics::Compressor, 5, false) => Field::Release,
            (Dynamics::Compressor, 6, true) => Field::Knee,
            (Dynamics::Compressor, 6, false) => Field::Makeup,
            (Dynamics::Gate, 4, true) => Field::GateThreshold,
            (Dynamics::Gate, 4, false) => Field::GateAttack,
            (Dynamics::Gate, 5, true) => Field::GateHold,
            (Dynamics::Gate, 5, false) => Field::GateRelease,
            (Dynamics::Gate, 6, true) => Field::GateRange,
            (Dynamics::Gate, 6, false) => Field::GateHysteresis,
            _ => return None,
        }
    };
    // EQ gain field is exactly 100 millidB per decoded step, with no acceleration.
    Some(Intent::Processing { field, steps })
}
#[derive(Clone, Copy, Debug)]
pub struct GainPolicy {
    pub guard_db: f64,
    pub upper_db: f64,
    pub maximum_db: f64,
    pub minimum_db: f64,
    pub step_db: f64,
}
impl GainPolicy {
    pub const CHANNEL: Self = Self {
        guard_db: -12.,
        upper_db: 6.,
        maximum_db: 12.,
        minimum_db: -60.,
        step_db: 0.5,
    };
    pub const MASTER: Self = Self {
        guard_db: -18.,
        upper_db: 0.,
        maximum_db: 6.,
        minimum_db: -60.,
        step_db: 0.1,
    };
    pub fn change(self, current: f64, delta: f64) -> f64 {
        if ![
            current,
            delta,
            self.guard_db,
            self.upper_db,
            self.maximum_db,
            self.minimum_db,
            self.step_db,
        ]
        .iter()
        .all(|v| v.is_finite())
            || self.upper_db <= self.guard_db
            || self.minimum_db > self.maximum_db
        {
            return current;
        }
        let t = ((current - self.guard_db) / (self.upper_db - self.guard_db)).clamp(0., 1.);
        let factor = if delta > 0. {
            1. - 0.8 * t * t * (3. - 2. * t)
        } else {
            1.
        };
        (current + delta * factor).clamp(self.minimum_db, self.maximum_db)
    }
    pub fn steps(self, mut current: f64, steps: i32) -> f64 {
        for _ in 0..steps.clamp(-8, 8).unsigned_abs() {
            current = self.change(current, steps.signum() as f64 * self.step_db);
        }
        current
    }
}
#[derive(Default, Debug)]
pub struct MasterSpeed {
    speed: f64,
    pending_db: f64,
    last_apply: Option<u64>,
    last_tick: Option<u64>,
    last_input: Option<u64>,
    generation: u64,
}
impl MasterSpeed {
    pub fn stop(&mut self) {
        self.speed = 0.;
        self.pending_db = 0.;
        self.last_apply = None;
        self.last_tick = None;
        self.last_input = None;
    }
    pub fn input(&mut self, position: f64, now_ms: u64, generation: u64, returns_to_center: bool) {
        if !returns_to_center || !position.is_finite() {
            self.stop();
            return;
        }
        if generation != self.generation
            || self
                .last_input
                .is_some_and(|last| now_ms < last || now_ms - last > 250)
        {
            self.stop();
            self.generation = generation;
        }
        self.integrate(now_ms);
        let d = position.clamp(-1., 1.);
        let magnitude = ((d.abs() - 0.08) / 0.92).max(0.);
        self.speed = d.signum() * magnitude * magnitude * 0.5; // provisional maximum 0.5 dB/s
        self.last_input = Some(now_ms);
        self.last_apply.get_or_insert(now_ms);
        self.last_tick.get_or_insert(now_ms);
        if self.speed == 0. {
            self.last_tick = Some(now_ms);
        }
    }
    pub fn tick(&mut self, current: f64, now_ms: u64, generation: u64, authorized: bool) -> f64 {
        if !authorized
            || self
                .last_apply
                .is_some_and(|last| now_ms < last || now_ms - last > 250)
            || generation != self.generation
            || self
                .last_input
                .is_none_or(|t| now_ms < t || now_ms - t > 250)
        {
            self.stop();
            return current;
        }
        self.integrate(now_ms);
        self.last_apply = Some(now_ms);
        let delta = std::mem::take(&mut self.pending_db);
        GainPolicy::MASTER.change(current, delta)
    }
    fn integrate(&mut self, now_ms: u64) {
        let before = self.last_tick.replace(now_ms).unwrap_or(now_ms);
        if now_ms < before {
            self.stop();
            return;
        }
        self.pending_db += self.speed * (now_ms - before) as f64 / 1000.;
    }
}
/// Explicit output transport, owned by the non-audio caller. No endpoint discovery.
pub trait Output {
    fn send(&mut self, endpoint: &str, message: &[u8]) -> Result<(), String>;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Colour {
    Off = 0,
    Red = 1,
    Green = 4,
    Yellow = 5,
    Blue = 0x10,
    Magenta = 0x11,
    Cyan = 0x14,
    White = 0x7f,
}
pub struct Feedback {
    endpoint: String,
    desired: [Colour; 16],
    sent: [Option<Colour>; 16],
    last_ms: Option<u64>,
    pub failed: bool,
}
impl Feedback {
    pub fn new(endpoint: String, protocol: &str) -> Result<Self, String> {
        if endpoint.is_empty() || protocol != "arturia-minilab-mkii-pad-v1" {
            return Err("explicit mkII output identity required".into());
        }
        Ok(Self {
            endpoint,
            desired: [Colour::Off; 16],
            sent: [None; 16],
            last_ms: None,
            failed: false,
        })
    }
    /// Latest confirmed, labelled action projection; replaces the mailbox in place.
    pub fn update(&mut self, colours: [Colour; 16]) {
        self.desired = colours;
    }
    pub fn reconnect(&mut self) {
        self.sent = [None; 16];
        self.last_ms = None;
        self.failed = false;
    }
    /// At most 16 short messages per 50 ms batch; no input state or locks shared.
    pub fn poll(&mut self, now_ms: u64, transport: &mut impl Output) {
        if self.last_ms.is_some_and(|t| now_ms.saturating_sub(t) < 50) {
            return;
        }
        self.last_ms = Some(now_ms);
        for i in 0..16 {
            if self.sent[i] != Some(self.desired[i]) {
                let packet = [
                    0xf0,
                    0,
                    0x20,
                    0x6b,
                    0x7f,
                    0x42,
                    2,
                    0,
                    0x10,
                    0x70 + i as u8,
                    self.desired[i] as u8,
                    0xf7,
                ];
                if transport.send(&self.endpoint, &packet).is_err() {
                    self.failed = true;
                    return;
                }
                self.sent[i] = Some(self.desired[i]);
            }
        }
        self.failed = false;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn profile() -> Profile {
        Profile {
            endpoint: "fixture".into(),
            rotary_channel: 0,
            rotaries: std::array::from_fn(|i| Rotary {
                cc: 16 + i as u8,
                encoding: Relative::TwosComplement,
                alternate: if i == 0 {
                    Some((70, Relative::TwosComplement))
                } else {
                    None
                },
            }),
            shift: Some((vec![0xb0, 90, 127], vec![0xb0, 90, 0])),
            key_channel: 0,
            anchor: 48,
            pad_channel: 9,
            pads: [
                [36, 37, 38, 39, 40, 41, 42, 43],
                [44, 45, 46, 47, 48, 49, 50, 51],
            ],
            pitch_channel: 0,
            pitch_returns_to_center: true,
            hardware_qualified: false,
        }
    }
    #[test]
    fn held_shift_alternate_and_loss() {
        let mut input = Input::new(profile()).unwrap();
        let t = Target::Input("input-17".into());
        input.decode(
            "fixture",
            &[0xb0, 90, 127],
            0,
            0,
            t.clone(),
            Dynamics::Compressor,
        );
        for i in 0..16 {
            let a = input.decode(
                "fixture",
                &[0xb0, 16 + i, 1],
                0,
                1,
                t.clone(),
                Dynamics::Compressor,
            );
            assert!(a.is_some());
        }
        assert_eq!(
            input.decode(
                "fixture",
                &[0xb0, 70, 1],
                0,
                2,
                t.clone(),
                Dynamics::Compressor
            ),
            Some(Intent::Processing {
                field: Field::Band1Q,
                steps: 1
            })
        );
        assert_eq!(
            input.decode(
                "fixture",
                &[0xb0, 16, 1],
                0,
                600,
                t.clone(),
                Dynamics::Compressor
            ),
            None
        );
        input.decode(
            "fixture",
            &[0xb0, 90, 0],
            0,
            601,
            t.clone(),
            Dynamics::Compressor,
        );
        assert_eq!(
            input.decode(
                "fixture",
                &[0xb0, 16, 1],
                1,
                602,
                t.clone(),
                Dynamics::Compressor
            ),
            Some(Intent::Processing {
                field: Field::Band1Hz,
                steps: 1
            })
        );
        assert_eq!(
            input.decode(
                "fixture",
                &[0xb0, 16, 1],
                0,
                603,
                t.clone(),
                Dynamics::Compressor
            ),
            None
        );
        assert_eq!(
            input.decode("fixture", &[0x90, 72, 100], 1, 604, t, Dynamics::Gate),
            Some(Intent::SelectMaster)
        );
    }
    #[test]
    fn fine_gain_and_guards() {
        assert_eq!(
            rotary(
                8,
                2,
                true,
                Dynamics::Gate,
                Target::Input("sparse-42".into())
            ),
            Some(Intent::Processing {
                field: Field::Band1Gain,
                steps: 2
            })
        );
        let p = GainPolicy::CHANNEL;
        assert!(p.steps(5., 8) - 5. < p.steps(-20., 8) + 20.);
        assert_eq!(p.steps(6., -8), 2.);
        assert_eq!(p.steps(12., 8), 12.);
    }
    #[test]
    fn strip_rate_independence_center_and_stale() {
        fn run(interval: u64) -> f64 {
            let mut s = MasterSpeed::default();
            let mut gain = -30.;
            for ms in (0..=1000).step_by(10) {
                if ms % interval == 0 {
                    s.input(1., ms, 1, true);
                }
                gain = s.tick(gain, ms, 1, true);
            }
            gain
        }
        assert!((run(10) - run(100)).abs() < 1e-9);
        assert!((run(10) + 29.5).abs() < 1e-8);
        let mut s = MasterSpeed::default();
        s.input(1., 0, 1, true);
        assert_eq!(s.tick(-30., 251, 1, true), -30.);
        s.input(1., 300, 1, true);
        s.input(0., 310, 1, true);
        assert!((s.tick(-30., 320, 1, true) + 29.995).abs() < 1e-9);
        s.input(1., 400, 1, true);
        assert_eq!(s.tick(-30., 410, 1, false), -30.);
    }
    #[test]
    fn output_coalescing_palette_and_failure() {
        #[derive(Default)]
        struct Sink(Vec<Vec<u8>>, bool);
        impl Output for Sink {
            fn send(&mut self, e: &str, m: &[u8]) -> Result<(), String> {
                assert_eq!(e, "led-fixture");
                if self.1 {
                    return Err("offline".into());
                }
                self.0.push(m.to_vec());
                Ok(())
            }
        }
        let mut f = Feedback::new("led-fixture".into(), "arturia-minilab-mkii-pad-v1").unwrap();
        let mut s = Sink::default();
        f.poll(0, &mut s);
        assert_eq!(s.0.len(), 16);
        let mut colours = [Colour::Off; 16];
        colours[15] = Colour::Cyan;
        f.update(colours);
        f.poll(49, &mut s);
        assert_eq!(s.0.len(), 16);
        f.poll(50, &mut s);
        assert_eq!(s.0[16][9..11], [0x7f, 0x14]);
        colours[0] = Colour::Red;
        f.update(colours);
        s.1 = true;
        f.poll(100, &mut s);
        assert!(f.failed);
        s.1 = false;
        f.poll(150, &mut s);
        assert!(!f.failed);
    }
}

/// A colour accompanies the currently displayed action label. Unknown confirmation
/// never becomes an enabled/muted indication, and input events are not accepted here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PadState {
    Unavailable,
    Navigation,
    Selected,
    ConfirmedEnabled,
    ConfirmedMuted,
    Review,
}
pub fn pad_colour(label: &str, state: PadState) -> Colour {
    if label.is_empty() {
        return Colour::Off;
    }
    match state {
        PadState::Unavailable => Colour::Off,
        PadState::Navigation => Colour::Blue,
        PadState::Selected => Colour::Cyan,
        PadState::ConfirmedEnabled => Colour::Green,
        PadState::ConfirmedMuted => Colour::Red,
        PadState::Review => Colour::Yellow,
    }
}

/// Non-audio output owner. Publishing never waits for the endpoint; one latest
/// projection replaces pending work. Dropping it never joins a blocked driver.
/// Endpoint implementations must bound their own send timeout for prompt shutdown.
pub struct FeedbackWorker {
    shared: std::sync::Arc<std::sync::Mutex<FeedbackMailbox>>,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    failed: std::sync::Arc<std::sync::atomic::AtomicBool>,
    wake: std::sync::mpsc::SyncSender<()>,
}
#[derive(Default)]
struct FeedbackMailbox {
    colours: Option<[Colour; 16]>,
    repaint: bool,
}
impl FeedbackWorker {
    pub fn spawn(
        endpoint: String,
        protocol: &str,
        mut output: impl Output + Send + 'static,
    ) -> Result<Self, String> {
        use std::sync::{
            Arc, Mutex,
            atomic::{AtomicBool, Ordering},
        };
        let mut feedback = Feedback::new(endpoint, protocol)?;
        let shared = Arc::new(Mutex::new(FeedbackMailbox::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let failed = Arc::new(AtomicBool::new(false));
        let (wake, receiver) = std::sync::mpsc::sync_channel(1);
        let (mailbox, stopping, failure) = (shared.clone(), stop.clone(), failed.clone());
        std::thread::Builder::new()
            .name("controller-feedback".into())
            .spawn(move || {
                let start = std::time::Instant::now();
                while !stopping.load(Ordering::Acquire) {
                    // Copy under the lock, then release it before any transport call.
                    if let Ok(mut pending) = mailbox.lock() {
                        if std::mem::take(&mut pending.repaint) {
                            feedback.reconnect();
                        }
                        if let Some(colours) = pending.colours.take() {
                            feedback.update(colours);
                        }
                    } else {
                        break;
                    }
                    feedback.poll(start.elapsed().as_millis() as u64, &mut output);
                    failure.store(feedback.failed, Ordering::Release);
                    let _ = receiver.recv_timeout(std::time::Duration::from_millis(50));
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            shared,
            stop,
            failed,
            wake,
        })
    }
    /// Call from the control/UI pump with its confirmed-state projection.
    /// False means the mailbox was busy; retain/retry the latest projection.
    pub fn publish(&self, colours: [Colour; 16], repaint: bool) -> bool {
        let Ok(mut pending) = self.shared.try_lock() else {
            return false;
        };
        pending.colours = Some(colours);
        pending.repaint |= repaint;
        drop(pending);
        let _ = self.wake.try_send(());
        true
    }
    pub fn failed(&self) -> bool {
        self.failed.load(std::sync::atomic::Ordering::Acquire)
    }
}
impl Drop for FeedbackWorker {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Release);
        let _ = self.wake.try_send(());
    }
}

#[cfg(test)]
mod worker_tests {
    use super::*;
    #[test]
    fn blocked_output_keeps_latest_mailbox_and_shutdown_nonblocking() {
        struct Blocked(std::sync::mpsc::Sender<()>, std::sync::mpsc::Receiver<()>);
        impl Output for Blocked {
            fn send(&mut self, _: &str, _: &[u8]) -> Result<(), String> {
                let _ = self.0.send(());
                let _ = self.1.recv();
                Err("disconnected".into())
            }
        }
        let (entered, wait) = std::sync::mpsc::channel();
        let (release, blocked) = std::sync::mpsc::channel();
        let worker = FeedbackWorker::spawn(
            "explicit".into(),
            "arturia-minilab-mkii-pad-v1",
            Blocked(entered, blocked),
        )
        .unwrap();
        wait.recv_timeout(std::time::Duration::from_secs(2))
            .unwrap();
        for colour in [Colour::Red, Colour::Green, Colour::Cyan] {
            assert!(worker.publish([colour; 16], false));
        }
        assert_eq!(
            worker.shared.lock().unwrap().colours,
            Some([Colour::Cyan; 16])
        );
        drop(worker);
        release.send(()).unwrap();
    }
}

#[cfg(test)]
mod speed_transition_tests {
    use super::*;
    #[test]
    fn elapsed_motion_survives_center_and_stale_events_do_not_catch_up() {
        let mut speed = MasterSpeed::default();
        speed.input(1., 0, 1, true);
        speed.input(0., 200, 1, true);
        assert!((speed.tick(-30., 200, 1, true) + 29.9).abs() < 1e-9);
        assert_eq!(speed.tick(-29.9, 210, 1, true), -29.9);
        speed.input(1., 1000, 1, true);
        assert_eq!(speed.tick(-30., 1000, 1, true), -30.);
        assert!((speed.tick(-30., 1200, 1, true) + 29.9).abs() < 1e-9);
        speed.input(-1., 1200, 1, true);
        assert!((speed.tick(-30., 1300, 1, true) + 30.05).abs() < 1e-9);
        for now in [1400, 1500, 1600] {
            speed.input(1., now, 1, true);
        }
        assert_eq!(speed.tick(-30., 1700, 1, true), -30.); // no pump catch-up burst
    }
}
