//! GP-AUTO main-vocal assist wire values and the GigPies soundcheck document.
//! The detector, reduction and soundcheck rule are owned by GigPies; Desk only
//! records GP-METER windows, presents readback and drives reviewed processing writes.
use serde::{Deserialize, Serialize};

pub const MAX_CUT_MDB: u32 = 12000;
pub const MAX_WINDOWS: usize = 3000;
pub const RULE: &str = "gp-auto-main-vocal-2026-11-21";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    #[default]
    Manual,
    Soundcheck,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub bypass: bool,
    pub reference_mdb: i32,
    pub headroom_mdb: u32,
    pub max_cut_mdb: u32,
    pub attack_us: u32,
    pub release_ms: u32,
    pub origin: Origin,
    pub evidence_windows: u32,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            bypass: true,
            reference_mdb: -18000,
            headroom_mdb: 3000,
            max_cut_mdb: 6000,
            attack_us: 5000,
            release_ms: 400,
            origin: Origin::Manual,
            evidence_windows: 0,
        }
    }
}
impl Config {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
    pub fn validate(self) -> Result<(), String> {
        let stepped = |v: i64, lo: i64, hi: i64, step: i64| (lo..=hi).contains(&v) && v % step == 0;
        if !stepped(self.reference_mdb.into(), -60000, 0, 100)
            || !stepped(self.headroom_mdb.into(), 0, 12000, 100)
            || !stepped(self.max_cut_mdb.into(), 100, MAX_CUT_MDB.into(), 100)
            || !stepped(self.attack_us.into(), 100, 200000, 100)
            || !(10..=5000).contains(&self.release_ms)
            || i64::from(self.reference_mdb) + i64::from(self.headroom_mdb) > 0
            || (self.origin == Origin::Manual) != (self.evidence_windows == 0)
            || self.evidence_windows > MAX_WINDOWS as u32
        {
            return Err("vocal assist range".into());
        }
        Ok(())
    }
    pub fn threshold_mdb(self) -> i32 {
        self.reference_mdb + self.headroom_mdb as i32
    }
    /// Operator-entered reference is never labelled as measured.
    pub fn set_manual_reference(&mut self, reference_mdb: i32) {
        self.reference_mdb = reference_mdb;
        self.origin = Origin::Manual;
        self.evidence_windows = 0;
    }
    /// One-key revert: disable, keeping settings so re-enabling is explicit.
    pub fn reverted(self) -> Self {
        Self {
            bypass: true,
            ..self
        }
    }
    pub fn provenance(self) -> String {
        match self.origin {
            Origin::Manual => "manual reference".into(),
            Origin::Soundcheck => format!("soundcheck, {} active windows", self.evidence_windows),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Bypassed,
    Idle,
    Reducing,
    Recovering,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    pub state: State,
    pub reduction_mdb: u32,
    pub deepest_mdb: u32,
    pub onsets: String,
}
impl Observation {
    pub fn validate(&self) -> Result<(), String> {
        crate::provider::counter(&self.onsets)?;
        if self.reduction_mdb > MAX_CUT_MDB
            || self.deepest_mdb > MAX_CUT_MDB
            || self.reduction_mdb > self.deepest_mdb
            || (matches!(self.state, State::Bypassed | State::Idle) && self.reduction_mdb != 0)
        {
            return Err("vocal assist observation".into());
        }
        Ok(())
    }
    pub fn summary(&self) -> String {
        format!(
            "{} / cut {:.1} dB / deepest {:.1} dB / {} events",
            match self.state {
                State::Bypassed => "OFF",
                State::Idle => "HOLDING LEVEL",
                State::Reducing => "REDUCING PEAK",
                State::Recovering => "RECOVERING",
            },
            self.reduction_mdb as f64 / 1000.,
            self.deepest_mdb as f64 / 1000.,
            self.onsets
        )
    }
}

/// One received GP-METER window of the selected input's processed tap.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Window {
    pub sequence: String,
    pub rms_mdbfs: Option<i32>,
    pub sample_peak_mdbfs: Option<i32>,
    pub silent: bool,
    pub clip_count: u32,
    pub age_ms: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    Prepared,
    TooShort,
    UnreliableMeter,
    Clipped,
    TooQuiet,
    InvalidEvidence,
}
impl Reason {
    pub fn operator_text(self) -> &'static str {
        match self {
            Reason::Prepared => "measured",
            Reason::TooShort => "too little singing; sing longer and stop again",
            Reason::UnreliableMeter => "meter was stale or unavailable; check the connection",
            Reason::Clipped => "the input clipped; lower the input gain and soundcheck again",
            Reason::TooQuiet => "too quiet for a vocal; raise the input gain",
            Reason::InvalidEvidence => "invalid soundcheck evidence",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Measurements {
    pub windows: u32,
    pub stale: u32,
    pub unavailable: u32,
    pub silent: u32,
    pub active: u32,
    pub first_sequence: crate::metering::Counter,
    pub last_sequence: crate::metering::Counter,
    pub rms_p10_mdbfs: i32,
    pub rms_p50_mdbfs: i32,
    pub rms_p90_mdbfs: i32,
    pub rms_p99_mdbfs: i32,
    pub peak_max_mdbfs: i32,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Proposal {
    pub config: Config,
    pub reason: Reason,
    pub measurements: Option<Measurements>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Document {
    pub version: u32,
    pub show_id: String,
    pub epoch: String,
    pub input: String,
    pub tap: String,
    pub rule: String,
    pub proposal: Proposal,
}
impl Document {
    /// Strict import fenced to the soundcheck that produced it.
    pub fn decode_for(
        bytes: &[u8],
        show_id: &str,
        epoch: &str,
        input: &str,
    ) -> Result<Self, String> {
        let d: Self = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        if d.version != 1
            || d.rule != RULE
            || d.show_id != show_id
            || d.epoch != epoch
            || d.input != input
            || d.tap != format!("{input}:processed")
        {
            return Err("vocal soundcheck document identity".into());
        }
        d.proposal.config.validate()?;
        let prepared = d.proposal.reason == Reason::Prepared;
        if prepared != (!d.proposal.config.bypass && d.proposal.config.origin == Origin::Soundcheck)
            || (!prepared && !d.proposal.config.is_default())
            || (prepared && d.proposal.measurements.is_none())
        {
            return Err("vocal soundcheck proposal coherence".into());
        }
        Ok(d)
    }
    pub fn report(&self) -> Vec<String> {
        let p = &self.proposal;
        let mut lines = vec![format!(
            "{} ({})",
            p.reason.operator_text(),
            serde_json::to_value(p.reason)
                .ok()
                .and_then(|v| v.as_str().map(str::to_owned))
                .unwrap_or_default()
        )];
        if let Some(m) = p.measurements {
            lines.push(format!(
                "{} windows: {} singing, {} silent, {} stale, {} unavailable",
                m.windows, m.active, m.silent, m.stale, m.unavailable
            ));
            lines.push(format!(
                "RMS p10 {:.1} / p50 {:.1} / p90 {:.1} / p99 {:.1} dBFS, peak max {:.1} dBFS",
                m.rms_p10_mdbfs as f64 / 1000.,
                m.rms_p50_mdbfs as f64 / 1000.,
                m.rms_p90_mdbfs as f64 / 1000.,
                m.rms_p99_mdbfs as f64 / 1000.,
                m.peak_max_mdbfs as f64 / 1000.
            ));
        }
        if p.reason == Reason::Prepared {
            let c = p.config;
            lines.push(format!(
                "Keep {:.1} dBFS; reduce above {:.1} dBFS by at most {:.1} dB",
                c.reference_mdb as f64 / 1000.,
                c.threshold_mdb() as f64 / 1000.,
                c.max_cut_mdb as f64 / 1000.
            ));
        }
        lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn manual_reference_drops_soundcheck_provenance() {
        let mut c = Config {
            bypass: false,
            reference_mdb: -20000,
            origin: Origin::Soundcheck,
            evidence_windows: 200,
            ..Config::default()
        };
        c.validate().unwrap();
        c.set_manual_reference(-19000);
        assert_eq!((c.origin, c.evidence_windows), (Origin::Manual, 0));
        c.validate().unwrap();
        assert!(c.reverted().bypass);
        assert_eq!(c.reverted().reference_mdb, -19000);
    }
}
