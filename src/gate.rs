//! Gate wire values only. DSP remains owned by GigPies.
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub bypass: bool,
    pub threshold_mdb: i32,
    pub attack_us: u32,
    pub hold_ms: u32,
    pub release_ms: u32,
    pub range_mdb: u32,
    pub hysteresis_mdb: u32,
    /// Zero disables detector filtering; otherwise a detector-only bandpass.
    pub detector_hz: u32,
    pub detector_q_milli: u32,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            bypass: true,
            threshold_mdb: -36000,
            attack_us: 100,
            hold_ms: 100,
            release_ms: 300,
            range_mdb: 24000,
            hysteresis_mdb: 6000,
            detector_hz: 0,
            detector_q_milli: 700,
        }
    }
}
impl Config {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
    pub fn validate(self) -> Result<(), String> {
        if !(-90000..=0).contains(&self.threshold_mdb)
            || !(100..=200000).contains(&self.attack_us)
            || self.hold_ms > 2000
            || !(1..=5000).contains(&self.release_ms)
            || self.range_mdb > 90000
            || self.hysteresis_mdb > 24000
            || (self.detector_hz != 0 && !(20..=16000).contains(&self.detector_hz))
            || !(100..=10000).contains(&self.detector_q_milli)
        {
            return Err("gate range".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum State {
    Bypassed,
    Opening,
    Open,
    Holding,
    Closing,
    Closed,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    pub state: State,
    pub attenuation_mdb: u32,
}

/// Explicit, reviewed import from GigPies gate-prepare; never learns at runtime.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreparedDocument {
    #[serde(default)]
    pub causal_evaluation: Option<CausalEvaluation>,
    pub version: u8,
    pub input: String,
    pub calibration_id: String,
    pub evaluation_id: String,
    pub proposal: Proposal,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Proposal {
    pub config: Config,
    pub confidence: f64,
    pub measurements: Option<Measurements>,
    pub reason: Reason,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Measurements {
    pub quiet_hit_db: f64,
    pub bleed_p95_db: f64,
    pub decay_p90_ms: f64,
    pub evaluation_hits: usize,
    pub evaluation_missed_hits: usize,
    pub evaluation_bleed_windows: usize,
    pub evaluation_false_openings: usize,
}
#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    Prepared,
    InsufficientEvidence,
    InvalidEvidence,
    Overlap,
    EvaluationOverlap,
    CausalEvaluationFailed,
}
impl PreparedDocument {
    pub fn decode(bytes: &[u8], input: &str) -> Result<Self, String> {
        if bytes.len() > 16 * 1024 {
            return Err("gate proposal capacity".into());
        }
        let p: Self = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        p.proposal.config.validate()?;
        if p.version != 1
            || p.input != input
            || p.calibration_id.is_empty()
            || p.evaluation_id.is_empty()
            || p.calibration_id == p.evaluation_id
            || p.calibration_id.len() > 128
            || p.evaluation_id.len() > 128
            || !p.proposal.confidence.is_finite()
            || !(0. ..=1.).contains(&p.proposal.confidence)
            || (p.proposal.reason != Reason::Prepared && p.proposal.confidence != 0.)
            || (p.proposal.reason == Reason::Prepared) == p.proposal.config.bypass
            || (p.proposal.reason == Reason::Prepared && p.proposal.measurements.is_none())
        {
            return Err("gate proposal identity/evidence".into());
        }
        if let Some(m) = &p.proposal.measurements
            && (!m.quiet_hit_db.is_finite()
                || !m.bleed_p95_db.is_finite()
                || !m.decay_p90_ms.is_finite()
                || !(0. ..=5000.).contains(&m.decay_p90_ms)
                || !(8..=4096).contains(&m.evaluation_hits)
                || !(8..=4096).contains(&m.evaluation_bleed_windows)
                || m.evaluation_missed_hits > m.evaluation_hits
                || m.evaluation_false_openings > m.evaluation_bleed_windows
                || (p.proposal.reason == Reason::Prepared
                    && (m.evaluation_missed_hits != 0 || m.evaluation_false_openings != 0)))
        {
            return Err("gate proposal measurements".into());
        }
        if p.proposal.reason == Reason::CausalEvaluationFailed && p.causal_evaluation.is_none() {
            return Err("gate causal evidence missing".into());
        }
        if let Some(e) = &p.causal_evaluation
            && (!(8..=32).contains(&e.hits)
                || !(8..=32).contains(&e.bleed_passages)
                || e.missed_hits > e.hits
                || e.false_opening_passages > e.bleed_passages
                || !e.worst_hit_energy_change_db.is_finite()
                || e.worst_hit_energy_change_db > 0.
                || p.proposal.measurements.as_ref().is_none_or(|m| {
                    m.evaluation_hits != e.hits || m.evaluation_bleed_windows != e.bleed_passages
                })
                || (!p.proposal.config.bypass
                    && (e.missed_hits != 0
                        || e.false_opening_passages != 0
                        || e.worst_hit_energy_change_db < -1.)))
        {
            return Err("gate causal evaluation".into());
        }
        Ok(p)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Manual,
    Tom,
    Automatic,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CausalEvaluation {
    pub hits: usize,
    pub missed_hits: usize,
    pub bleed_passages: usize,
    pub false_opening_passages: usize,
    pub worst_hit_energy_change_db: f64,
}
