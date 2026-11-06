use super::*;
use crate::fx_aux as w;
impl Operator {
    pub(crate) fn refresh_fx_aux(&mut self, explicit: bool) -> Result<(), String> {
        let routing = explicit
            || self
                .session
                .fx_aux
                .snapshot
                .as_ref()
                .is_none_or(|s| s.get("return_routing_version").is_some());
        self.refresh_fx_aux_mode(
            explicit,
            routing,
            self.session
                .fx_aux
                .snapshot
                .as_ref()
                .is_some_and(|s| s.get("restore_version").is_some()),
        )
    }
    pub(crate) fn legacy_fx_aux(&mut self) -> Result<(), String> {
        self.refresh_fx_aux_mode(true, false, false)
    }
    fn refresh_fx_aux_mode(
        &mut self,
        explicit: bool,
        routing: bool,
        saved: bool,
    ) -> Result<(), String> {
        self.check_guard()?;
        self.session.fx_aux.receipt = None;
        let deadline = Instant::now() + Duration::from_millis(250);
        let mut budget = 64;
        while let Some(bytes) = self.transport.receive_available_until(deadline)? {
            if budget == 0 || Instant::now() >= deadline {
                return Err("FX aux backlog bound".into());
            }
            budget -= 1;
            if let Some(d) =
                self.processing_document(crate::provider::StrictDocument::parse(&bytes)?)?
            {
                let r = audio::decode_reply_document(d)?;
                if r.context == self.session.snapshot_request().context {
                    // A valid stale observation is consumed without renewing freshness;
                    // it is never a mutation reply merely because ingest returned false.
                    self.telemetry(&r)?;
                } else {
                    self.session
                        .accept(r, self.now())
                        .map_err(|e| format!("FX read/drain authority: {e}"))?;
                }
            }
        }
        let raw_request = self.session.snapshot_request();
        let mut request = raw_request.clone();
        request.kind = "fx_aux_snapshot".into();
        if routing {
            request.body = json!({"return_routing":true});
        }
        if saved {
            request.body = json!({"return_routing":true,"saved_intent":true});
        }
        let sent = self.now();
        self.transport
            .send_frame_until(&raw_request.encode()?, deadline)?;
        self.transport
            .send_frame_until(&request.encode()?, deadline)?;
        let mut raw = false;
        let mut aux = None;
        while budget > 0 && Instant::now() < deadline {
            self.check_guard()?;
            let Some(d) = self.transport.receive_document_until(deadline)? else {
                continue;
            };
            budget -= 1;
            if d.value()["contract"] == w::CONTRACT && d.value()["state"] == "snapshot" {
                aux = Some(w::snapshot(d.value().clone())?);
            } else if let Some(d) = self.processing_document(d)? {
                let r = audio::decode_reply_document(d)?;
                if r.context == raw_request.context {
                    if let Some(snapshot) = r.snapshot {
                        raw = self.session.ingest_snapshot(snapshot, sent)?;
                    }
                } else {
                    self.session
                        .accept(r, self.now())
                        .map_err(|e| format!("FX read/drain authority: {e}"))?;
                }
            }
            if raw && let Some(v) = aux.take() {
                let raw = self
                    .session
                    .snapshot
                    .as_ref()
                    .ok_or("FX raw read missing")?;
                if v["show_id"] != raw.authority.show_id
                    || v["epoch"] != raw.authority.epoch
                    || v["revision"] != raw.authority.revision
                {
                    return Err("FX paired context changed".into());
                }
                if Instant::now() >= deadline {
                    return Err("FX aux probe deadline".into());
                }
                if let (Some(monitors), Some(topology)) = (v["monitors"].as_array(), &raw.topology)
                    && monitors.len() != topology.monitors
                {
                    return Err("FX monitor topology mismatch".into());
                }
                let settled = v["available"] == true && v["pending"].is_null();
                self.session.fx_aux.snapshot = Some(v);
                self.session.fx_aux.receipt = Some(sent);
                if explicit && settled {
                    self.session.fx_aux.unknown = false;
                    if self.session.pending.as_ref().is_some_and(|p| {
                        p.request.kind == "fx_aux_configure"
                            && p.state == audio::PendingState::Uncertain
                    }) {
                        self.session.pending = None;
                        self.session.last_result="FX aux fresh state reconciled; prior outcome remains UNKNOWN; new review required".into();
                    }
                }
                return Ok(());
            }
        }
        Err("FX aux probe deadline".into())
    }
    pub(crate) fn saved_fx_aux(&mut self) -> Result<(), String> {
        self.refresh_fx_aux_mode(true, true, true)
    }
    pub(crate) fn save_fx_aux(
        &mut self,
        directory: &std::path::Path,
        slot: usize,
    ) -> Result<(), String> {
        if self.session.fx_aux.unknown || self.session.pending.is_some() {
            return Err("Reconcile pending/UNKNOWN FX before saving".into());
        }
        self.refresh_fx_aux(false)?;
        let saved = crate::fx_saved::capture(
            self.session
                .fx_aux
                .snapshot
                .as_ref()
                .ok_or("FX unavailable")?,
        )?;
        crate::fx_saved::save(directory, slot, &saved)?;
        self.session.fx_aux.saved = Some(saved);
        self.session.fx_aux.loaded_slot = Some(slot);
        self.session.last_result =
            format!("Saved authoritative four-aux intent in slot {}", slot + 1);
        Ok(())
    }
    pub(crate) fn load_fx_aux(
        &mut self,
        directory: &std::path::Path,
        slot: usize,
    ) -> Result<(), String> {
        let saved = crate::fx_saved::load(directory, slot)?;
        self.session.fx_aux.saved = Some(saved);
        self.session.fx_aux.loaded_slot = Some(slot);
        self.session.last_result = format!(
            "Loaded saved intent slot {}; engine unchanged. Preview before restore.",
            slot + 1
        );
        Ok(())
    }
    pub(crate) fn review_saved_fx_aux(&mut self) -> Result<(), String> {
        self.refresh_fx_aux(false)?;
        let saved = self
            .session
            .fx_aux
            .saved
            .as_ref()
            .ok_or("Load a saved FX intent first")?;
        let s = self
            .session
            .fx_aux
            .snapshot
            .as_ref()
            .ok_or("FX unavailable")?;
        match crate::fx_saved::mutation(saved, s) {
            Ok(body) => self.stage_fx_aux(body),
            Err(reason) => {
                let preview = json!({"saved":saved,"current_inputs":s["inputs"],"current_monitors":s["monitors"],"current_show":s["show_id"],"current_library":s["library_sha256"],"reason":reason});
                self.stage("fx_saved_preview", preview)
            }
        }
    }
    pub(crate) fn stage_fx_aux(&mut self, body: Value) -> Result<(), String> {
        self.refresh_fx_aux(false)?;
        self.session.fx_aux.validate(&body, self.now())?;
        self.stage("fx_aux_configure", body)
    }
    pub(crate) fn mutate_fx_aux(&mut self, body: Value) -> Result<(), String> {
        let result = self.mutate_fx_aux_once(body);
        if result.is_err()
            && self
                .session
                .pending
                .as_ref()
                .is_some_and(|p| p.request.kind == "fx_aux_configure")
        {
            self.session.fx_aux.unknown = true;
            self.session.fx_aux.receipt = None;
            self.session.pending.as_mut().unwrap().state = audio::PendingState::Uncertain;
            self.session.last_result =
                "FX aux outcome UNKNOWN; no retry; explicit probe required".into();
        }
        result
    }
    fn mutate_fx_aux_once(&mut self, body: Value) -> Result<(), String> {
        self.check_guard()?;
        self.refresh_fx_aux(false)?;
        self.session.fx_aux.validate(&body, self.now())?;
        let edits = crate::fx_saved::edits(&body)?;
        let basis = self.session.fx_aux.snapshot.clone().ok_or("FX basis")?;
        let request = self
            .session
            .begin("fx_aux_configure", body.clone(), self.now())?;
        let deadline = Instant::now() + Duration::from_millis(1900);
        self.transport
            .send_frame_until(&request.encode()?, deadline)?;
        let mut accepted_frame = None;
        for _ in 0..128 {
            self.check_guard()?;
            if Instant::now() >= deadline {
                break;
            }
            let Some(d) = self.transport.receive_document_until(deadline)? else {
                continue;
            };
            if d.value()["contract"] == w::CONTRACT {
                let v = d.value();
                let c = &request.context;
                if v["show_id"] != c.show_id
                    || v["epoch"] != c.epoch
                    || v["writer"] != json!(c.writer)
                    || v["lease"] != json!(c.lease)
                    || v["request_id"] != json!(c.request_id)
                {
                    continue;
                }
                crate::provider::keys(
                    v,
                    &[
                        "contract",
                        "version",
                        "show_id",
                        "module",
                        "epoch",
                        "writer",
                        "lease",
                        "request_id",
                        "state",
                        "reason",
                        "effective_frame",
                    ],
                )?;
                if v["version"] != 1 || v["module"] != "audio" {
                    return Err("FX aux reply version/module".into());
                }
                match v["state"].as_str() {
                    Some("preparing" | "prepared") => (),
                    Some("rejected") => {
                        self.session.pending = None;
                        return Err(format!("FX aux refused: {}", v["reason"]));
                    }
                    Some("accepted") => {
                        if !v["reason"].is_null() {
                            return Err("FX acceptance reason".into());
                        }
                        accepted_frame = Some(crate::provider::counter(
                            v["effective_frame"].as_str().ok_or("FX boundary missing")?,
                        )?);
                        break;
                    }
                    _ => return Err("FX aux unexpected mutation state".into()),
                }
            } else if let Some(d) = self.processing_document(d)? {
                let r = audio::decode_reply_document(d)?;
                if r.context == self.session.snapshot_request().context {
                    self.telemetry(&r)?;
                } else {
                    self.session
                        .accept(r, self.now())
                        .map_err(|e| format!("FX mutation authority: {e}"))?;
                }
            }
        }
        let accepted_frame = accepted_frame.ok_or("FX aux acceptance deadline")?;
        // Receipt is only publication; wait for owner fade and send-ramp settlement.
        for _ in 0..64 {
            if Instant::now() >= deadline {
                break;
            }
            self.refresh_fx_aux(false)?;
            let s = self
                .session
                .fx_aux
                .snapshot
                .as_ref()
                .ok_or("FX owner lost")?;
            if s["available"] != true
                || s["library_sha256"] != basis["library_sha256"]
                || s["map_revision"] != basis["map_revision"]
                || s["owner_binding"] != basis["owner_binding"]
                || s["show_id"] != basis["show_id"]
                || s["epoch"] != basis["epoch"]
                || s["monitors"] != basis["monitors"]
                || s["return_routing_version"] != basis["return_routing_version"]
                || s["inputs"] != basis["inputs"]
                || (body.get("restore").is_some() && s["restore_ready"] != true)
            {
                return Err("FX aux owner replaced".into());
            }
            let mut all_settled = true;
            for body in &edits {
                let bus = body["bus"].as_u64().unwrap() as usize;
                let generation =
                    crate::provider::counter(body["expected_generation"].as_str().unwrap())?
                        .checked_add(1)
                        .ok_or("FX generation exhausted")?
                        .to_string();
                let b = &s["buses"][bus];
                if b["generation"] != generation {
                    return Err("FX aux generation changed".into());
                }
                let actual = w::configuration(
                    b["configuration_json"]
                        .as_str()
                        .ok_or("FX config missing")?,
                )?;
                let status = crate::master_eq::parse_owner_bounded(
                    b["owner_json"].as_str().ok_or("FX status missing")?,
                    4096,
                )?;
                if body["panic"] == true {
                    let before = w::configuration(
                        basis["buses"][bus]["configuration_json"]
                            .as_str()
                            .ok_or("FX prior target")?,
                    )?;
                    let prior_status = crate::master_eq::parse_owner_bounded(
                        basis["buses"][bus]["owner_json"]
                            .as_str()
                            .ok_or("FX prior status")?,
                        4096,
                    )?;
                    if actual["returns"] != before["returns"]
                        || actual["sends"] != before["sends"]
                        || actual["settings"] != status["current"]
                        || (actual["settings"] != prior_status["current"]
                            && actual["settings"] != before["settings"])
                    {
                        return Err("FX panic active-state mismatch".into());
                    }
                } else if actual
                    != w::configuration(
                        body["configuration_json"]
                            .as_str()
                            .ok_or("FX target missing")?,
                    )?
                {
                    return Err("FX target mismatch".into());
                }
                if status["fault"] != 0 {
                    return Err("FX owner fault".into());
                }
                let frame =
                    crate::provider::counter(s["frame"].as_str().ok_or("FX applied frame")?)?;
                let settled = b["applied_generation"] == generation
                    && status["transition"] == 0
                    && status["current"] == actual["settings"]
                    && status["target"] == actual["settings"]
                    && matches!(status["request_state"].as_u64(), Some(0 | 3 | 4 | 5))
                    && s["pending"].is_null()
                    && frame >= accepted_frame
                    && crate::provider::counter(s["revision"].as_str().ok_or("FX revision")?)?
                        > crate::provider::counter(
                            basis["revision"].as_str().ok_or("FX basis revision")?,
                        )?;
                all_settled &= settled;
            }
            if all_settled {
                if body.get("restore").is_some() && s["restore_ready"] != true {
                    return Err("FX restore output quiescence changed".into());
                }
                self.session.pending = None;
                self.session.last_result = if body.get("restore").is_some() {
                    "Four saved FX auxes applied and settled; outputs remain quiesced; fresh correlated readback"
                        .into()
                } else {
                    format!(
                        "FX {} applied and settled; fresh owner readback",
                        body["bus"].as_u64().unwrap() + 1
                    )
                };
                return Ok(());
            }
        }
        Err("FX aux settlement deadline".into())
    }
}
