use super::*;
use crate::fx_aux as w;
impl Operator {
    pub(crate) fn refresh_fx_aux(&mut self, explicit: bool) -> Result<(), String> {
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
                if !self.telemetry(&r)? {
                    self.session.accept(r, self.now())?;
                }
            }
        }
        let raw_request = self.session.snapshot_request();
        let mut request = raw_request.clone();
        request.kind = "fx_aux_snapshot".into();
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
                    self.session.accept(r, self.now())?;
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
        let bus = body["bus"].as_u64().unwrap() as usize;
        let generation = crate::provider::counter(body["expected_generation"].as_str().unwrap())?
            .checked_add(1)
            .ok_or("FX generation exhausted")?
            .to_string();
        let basis = self.session.fx_aux.snapshot.clone().ok_or("FX basis")?;
        let request = self
            .session
            .begin("fx_aux_configure", body.clone(), self.now())?;
        let deadline = Instant::now() + Duration::from_millis(1900);
        self.transport
            .send_frame_until(&request.encode()?, deadline)?;
        let mut accepted = false;
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
                match v["state"].as_str() {
                    Some("preparing" | "prepared") => (),
                    Some("rejected") => {
                        self.session.pending = None;
                        return Err(format!("FX aux refused: {}", v["reason"]));
                    }
                    Some("accepted") => {
                        crate::provider::counter(
                            v["effective_frame"].as_str().ok_or("FX boundary missing")?,
                        )?;
                        accepted = true;
                        break;
                    }
                    _ => return Err("FX aux unexpected mutation state".into()),
                }
            } else if let Some(d) = self.processing_document(d)? {
                let r = audio::decode_reply_document(d)?;
                if !self.telemetry(&r)? {
                    self.session.accept(r, self.now())?;
                }
            }
        }
        if !accepted {
            return Err("FX aux acceptance deadline".into());
        }
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
            {
                return Err("FX aux owner replaced".into());
            }
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
                if actual["sends"] != before["sends"]
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
            if b["applied_generation"] == generation && status["transition"] == 0 {
                self.session.pending = None;
                self.session.last_result =
                    format!("FX {} applied and settled; fresh owner readback", bus + 1);
                return Ok(());
            }
        }
        Err("FX aux settlement deadline".into())
    }
}
