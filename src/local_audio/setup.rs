use super::*;
use crate::setup_saved::{Brain, Composed, Edit, Engine, Saved};
impl Operator {
    /// Each read is solicited and bounded. The final authority revision must
    /// still equal every component; no mixed-revision save is admitted.
    pub(crate) fn capture_setup(&mut self) -> Result<Saved, String> {
        self.check_guard()?;
        if self.session.pending.is_some() {
            return Err(
                "Pending/UNKNOWN operation: reconnect and review before setup capture".into(),
            );
        }
        self.setup_probe = true;
        self.refresh_structural()?;
        let structure = self
            .session
            .structural
            .clone()
            .ok_or("Structural snapshot absent")?;
        if structure.setup_hold.is_none() {
            return Err("Provider lacks complete setup hold capability".into());
        }
        self.refresh_processing()?;
        let processing = self
            .session
            .processing
            .clone()
            .ok_or("Processing unavailable")?;
        self.refresh_sends()?;
        let sends = self.session.sends.clone().ok_or("Sends unavailable")?;
        self.refresh_brain()?;
        let brain = self.session.brain.clone().ok_or("Brain unavailable")?;
        self.saved_fx_aux()?;
        let fx = self
            .session
            .fx_aux
            .snapshot
            .clone()
            .ok_or("FX capability unavailable")?;
        if structure.pa_configuration_json.is_some() {
            if structure
                .pa_status
                .as_ref()
                .is_none_or(|s| s["fault_latched"] != 0 || s["committed"] != 1)
            {
                return Err("PA owner is not healthy and committed".into());
            }
            self.refresh_live_eq()?;
            if self
                .session
                .live_eq
                .as_ref()
                .is_some_and(|s| s.live_available && !s.settled)
            {
                return Err("PA EQ transition must settle".into());
            }
        }
        self.refresh()?;
        let raw = self.session.snapshot.as_ref().ok_or("Core unavailable")?;
        let rev = &raw.authority.revision;
        if !self.session.fresh(self.now())
            || [
                &structure.revision,
                &processing.revision,
                &sends.revision,
                &brain.revision,
            ]
            .iter()
            .any(|r| *r != rev)
            || fx["revision"] != *rev
            || structure.topology != *raw.topology.as_ref().ok_or("Dynamic topology required")?
        {
            return Err(
                "Setup changed during capture; retry with stable authoritative state".into(),
            );
        }
        if raw.faulted
            || processing.faulted
            || sends.faulted
            || processing
                .channels
                .iter()
                .any(|c| !c.ready || c.current != c.target)
            || sends
                .channels
                .iter()
                .flat_map(|c| &c.sends)
                .any(|s| !s.ready || s.current != s.target)
            || raw
                .coefficients
                .iter()
                .any(|c| c.current_nanogain != c.ramp_target_nanogain)
            || raw
                .master_gain
                .as_ref()
                .is_none_or(|m| m.current_nanogain != m.target_nanogain)
        {
            return Err("Core/processing/send transitions must settle before capture".into());
        }
        let mut topology = structure.topology;
        topology.map_revision = 1;
        let saved = Saved {
            format: "gigpies-audio-setup".into(),
            version: 1,
            composed: Composed {
                version: 1,
                engine: Engine {
                    version: 4,
                    show_id: raw.authority.show_id.clone(),
                    topology,
                    parameters: raw
                        .authority
                        .parameters
                        .iter()
                        .map(|p| Edit {
                            target: p.target.clone(),
                            value: p.target_value.clone(),
                        })
                        .collect(),
                    processing: processing.channels.into_iter().map(|c| c.target).collect(),
                    send_taps: sends
                        .channels
                        .into_iter()
                        .map(|c| c.sends.into_iter().map(|s| s.target).collect())
                        .collect(),
                },
                brain: Brain {
                    version: 1,
                    source: brain.source,
                    monitor_gain_cdb: brain.monitor_gain_cdb,
                    monitor_mute: brain.monitor_mute,
                    monitor_dim: brain.monitor_dim,
                    talkback_monitors: brain.talkback_monitors,
                    talkback_gain_cdb: brain.talkback_gain_cdb,
                    talkback_mute: brain.talkback_mute,
                },
                pa_configuration_json: structure.pa_configuration_json,
                pa_program_buses: structure.pa_program_buses,
            },
            fx: if fx["available"] == true {
                Some(crate::fx_saved::capture(&fx)?)
            } else {
                None
            },
        };
        saved.validate()?;
        Ok(saved)
    }
    pub(crate) fn setup_operation(
        &mut self,
        operation: crate::setup_saved::Operation,
    ) -> Result<(), String> {
        use crate::setup_saved::Operation as O;
        let verifying = matches!(&operation, O::Verify(..));
        match operation {
            O::Save(path, slot) => {
                let saved = self.capture_setup()?;
                saved.save(&path, slot)?;
                self.session.last_result = format!(
                    "Audio setup slot {} saved durably; no runtime arm state",
                    slot + 1
                );
                Ok(())
            }
            O::Review(path, slot) | O::Verify(path, slot) => {
                let saved = Saved::load(&path, slot)?;
                let current = self.capture_setup()?;
                let mut journal = crate::setup_saved::Journal::read(&path, slot, &saved)?;
                if verifying {
                    let remaining = saved.steps(&current)?;
                    let epoch = self
                        .session
                        .snapshot
                        .as_ref()
                        .ok_or("snapshot")?
                        .authority
                        .epoch
                        .clone();
                    let recover: Vec<_> = journal
                        .scopes
                        .iter()
                        .filter(|(scope, r)| {
                            r["state"] == "applied-readback-unavailable"
                                && r["epoch"] == epoch
                                && !remaining.iter().any(|s| s.scope == **scope)
                        })
                        .map(|(scope, r)| (scope.clone(), r.clone()))
                        .collect();
                    for (scope, mut r) in recover {
                        r["state"] = json!("scope-complete");
                        r["reason"] = json!(
                            "Correlated completion retained; exact readback verified explicitly after reconnect"
                        );
                        journal.write(&path, slot, &scope, &r)?;
                    }
                }
                let structure = self
                    .session
                    .structural
                    .as_ref()
                    .ok_or("Setup safety snapshot absent")?;
                let brain = self
                    .session
                    .brain
                    .as_ref()
                    .ok_or("Brain safety snapshot absent")?;
                let safety = format!(
                    "LIVE SAFETY: setup hold={} / outputs quiesced={} / Brain armed={} / talkback held={}. Release and rearm remain separate explicit actions.",
                    structure.setup_hold == Some(true),
                    structure.outputs_quiesced,
                    brain.monitor_armed,
                    brain.held_generation.is_some()
                );
                let report = format!(
                    "Selected audio setup slot {}\n{}\n{}\n{}",
                    slot + 1,
                    saved.review(&current),
                    safety,
                    journal.summary()
                );
                self.setup_draft(
                    "setup_preview",
                    json!({"saved":saved,"current":current,"report":report}),
                )
            }
            O::Restore(path, slot) => {
                let saved = Saved::load(&path, slot)?;
                let current = self.capture_setup()?;
                let steps = saved.steps(&current)?;
                self.require_setup_hold()?;
                let count = steps.iter().filter(|s| s.scope == self.scope).count();
                if count == 0 {
                    return Err(format!(
                        "No differences in scope {}; review other scopes or verify complete setup",
                        self.scope
                    ));
                }
                let report = format!(
                    "Selected audio setup slot {}/16\n{}\nCONFIRM restores {count} changed commands in scope {} only. Other scopes remain pending. Failures retain already-applied work; no rollback. Reconnect requires new review.\n",
                    slot + 1,
                    saved.review(&current),
                    self.scope
                );
                self.setup_draft("setup_restore",json!({"saved":saved,"current":current,"report":report,"directory":path,"slot":slot,"scope":self.scope}))
            }
        }
    }
    fn setup_draft(&mut self, kind: &str, body: Value) -> Result<(), String> {
        if self.draft.is_some() {
            return Err("Cancel existing review first".into());
        }
        self.draft = Some(Draft {
            measurement_basis: None,
            measurement_review: None,
            fx_basis: None,
            monitor_device: None,
            kind: kind.into(),
            body,
            revision: self
                .session
                .snapshot
                .as_ref()
                .ok_or("snapshot")?
                .authority
                .revision
                .clone(),
            generation: self.session.generation(),
        });
        Ok(())
    }
    fn require_setup_hold(&mut self) -> Result<(), String> {
        self.setup_probe = true;
        self.refresh_structural()?;
        let s = self
            .session
            .structural
            .as_ref()
            .ok_or("Setup state unavailable")?;
        if s.setup_hold != Some(true) || !s.outputs_quiesced {
            return Err("Explicit PA scope setup hold and quiesced outputs required".into());
        }
        let hold_revision = s.revision.clone();
        self.refresh_brain()?;
        let b = self
            .session
            .brain
            .as_ref()
            .ok_or("Brain state unavailable")?;
        // Release/rearm advances the common revision. Never let a newer raw or
        // Brain read silently replace the revision that proved the hold.
        if b.revision != hold_revision
            || self
                .session
                .snapshot
                .as_ref()
                .is_none_or(|raw| raw.authority.revision != hold_revision)
        {
            return Err("Setup hold changed during safety read; new review required".into());
        }
        if b.monitor_armed || b.talkback_foh || b.held_generation.is_some() {
            return Err("Brain monitor/talkback must be disarmed and released".into());
        }
        Ok(())
    }
    pub(super) fn confirm_setup(&mut self, d: Draft) -> Result<(), String> {
        if d.kind == "setup_preview" {
            return Err("Read-only setup review; choose Restore current scope for an explicit mutation review".into());
        }
        let saved: Saved =
            serde_json::from_value(d.body["saved"].clone()).map_err(|e| e.to_string())?;
        let reviewed: Saved =
            serde_json::from_value(d.body["current"].clone()).map_err(|e| e.to_string())?;
        let current = self.capture_setup()?;
        if current != reviewed
            || self.session.generation() != d.generation
            || self.session.snapshot.as_ref().unwrap().authority.revision != d.revision
            || d.body["scope"] != self.scope
        {
            return Err("Setup review is stale; new review required".into());
        }
        self.require_setup_hold()?;
        let steps: Vec<_> = saved
            .steps(&current)?
            .into_iter()
            .filter(|s| s.scope == self.scope)
            .collect();
        let path = std::path::PathBuf::from(d.body["directory"].as_str().ok_or("Setup directory")?);
        let slot = d.body["slot"].as_u64().ok_or("Setup slot")? as usize;
        if Saved::load(&path, slot)? != saved {
            return Err("Saved setup changed since review; select and review again".into());
        }
        let mut journal = crate::setup_saved::Journal::read(&path, slot, &saved)?;
        let prior_unknown = journal.prior_unknown(&self.scope)?;
        let mut record = json!({"format":"gigpies-audio-recall-result","version":1,"scope":self.scope,"state":"pending","epoch":self.session.snapshot.as_ref().ok_or("snapshot")?.authority.epoch,"completion":null,"prior_unknown":prior_unknown,"completed":[],"current_step":null,"reason":null});
        let deadline = Instant::now() + Duration::from_secs(30);
        for step in &steps {
            if Instant::now() >= deadline {
                return Err(
                    "PARTIAL: bounded recall duration; review remaining differences".into(),
                );
            }
            self.check_guard()?;
            self.require_setup_hold()?;
            record["state"] = json!("UNKNOWN");
            record["current_step"] = json!(step.label);
            record["completion"] = Value::Null;
            // Durable before send: crash cannot leave a false completed receipt.
            journal.write(&path, slot, &self.scope, &record)?;
            let result = (|| {
                if self.session.renewal_due(self.now()) {
                    self.mutate_inner("renew", json!({}))?;
                }
                let body = if step.kind == "fx_aux_configure" {
                    self.saved_fx_aux()?;
                    crate::fx_saved::mutation(
                        saved.fx.as_ref().ok_or("FX saved")?,
                        self.session.fx_aux.snapshot.as_ref().ok_or("FX snapshot")?,
                    )?
                } else {
                    step.body.clone()
                };
                // Renewal and FX preparation may read newer authority. Pin safety
                // again immediately before admission; subsequent changes conflict.
                self.require_setup_hold()?;
                // The standard mutation path retains exact request correlation and deadlines.
                self.session.input_released();
                self.mutate_inner(&step.kind, body)?;
                if !correlated_applied(&step.kind, &self.session.last_result) {
                    return Err(format!(
                        "No applied completion: {}",
                        self.session.last_result
                    ));
                }
                record["completion"] = json!("correlated-applied");
                journal.write(&path, slot, &self.scope, &record)?;
                let after = self.capture_setup()?;
                if saved
                    .steps(&after)?
                    .iter()
                    .any(|s| s.label == step.label && s.scope == step.scope)
                {
                    return Err(
                        "Correlated completion did not produce exact settled saved readback".into(),
                    );
                }
                self.require_setup_hold()?;
                Ok(())
            })();
            if let Err(e) = result {
                if record["completion"] == "correlated-applied" {
                    record["state"] = json!("applied-readback-unavailable");
                }
                record["reason"] = json!(e);
                let _ = journal.write(&path, slot, &self.scope, &record);
                return Err(format!(
                    "PARTIAL/UNKNOWN at {}: {e}; no rollback; reconnect/read/review before further restore",
                    step.label
                ));
            }
            record["completed"].as_array_mut().unwrap().push(
                json!({"step":step.label,"result":"correlated applied and settled readback"}),
            );
            record["state"] = json!("partial");
            record["current_step"] = Value::Null;
            journal.write(&path, slot, &self.scope, &record)?;
        }
        record["state"] = json!(if prior_unknown.is_empty() {
            "scope-complete"
        } else {
            "scope-complete-with-prior-unknown"
        });
        journal.write(&path, slot, &self.scope, &record)?;
        self.session.last_result = format!(
            "SETUP scope {} complete: {} correlated commands and exact settled readbacks. Outputs held. Verify all scopes and retained prior UNKNOWN receipts; release hold and rearm separately.",
            self.scope,
            steps.len()
        );
        Ok(())
    }
}

// These prefixes are emitted only after the existing codecs validate exact
// request correlation and an applied final. Refusal reason text is never searched.
fn correlated_applied(kind: &str, result: &str) -> bool {
    let prefix = match kind {
        "set" => "set applied revision ",
        "processing_set" => "processing_set applied revision ",
        "send_tap_set" => "sends_set applied revision ",
        "pa_set" | "output_patch" => "structural applied revision ",
        "brain_monitor_set" | "brain_talkback_set" => "Brain applied revision ",
        "fx_aux_configure" => {
            "Four saved FX auxes applied and settled; outputs remain quiesced; fresh correlated readback"
        }
        _ => return false,
    };
    result.starts_with(prefix)
}
#[cfg(test)]
mod tests {
    #[test]
    fn refusal_or_unknown_text_never_counts_as_applied() {
        for kind in [
            "set",
            "processing_set",
            "send_tap_set",
            "pa_set",
            "output_patch",
            "brain_monitor_set",
            "brain_talkback_set",
            "fx_aux_configure",
        ] {
            for status in [
                "not applied",
                "structural REFUSED not_applied; confirmed state unchanged",
                "UNKNOWN applied reply lost",
                "processing backpressure",
                "set rejected revision 3",
            ] {
                assert!(!super::correlated_applied(kind, status));
            }
        }
        assert!(super::correlated_applied("set", "set applied revision 3"));
        assert!(super::correlated_applied(
            "pa_set",
            "structural applied revision 3 at frame 48"
        ));
        assert!(!super::correlated_applied(
            "set",
            "structural applied revision 3"
        ));
    }
}
