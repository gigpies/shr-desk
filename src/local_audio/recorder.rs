use super::*;
impl Operator {
    pub(crate) fn refresh_record(&mut self) -> Result<(), String> {
        self.recorder.received = None;
        let result = self.read_record();
        if result.is_err() {
            self.record_query_failed = true;
            // GP05 has no query nonce. Retire the attachment after an uncertain
            // query so a late reply cannot refresh a subsequent observation.
            self.session.disconnect();
        }
        result
    }
    fn read_record(&mut self) -> Result<(), String> {
        if self.record_query_failed {
            return Err("REC readback attachment retired; reconnect required".into());
        }
        let sent = Instant::now();
        let deadline = sent + Duration::from_millis(250);
        let mut q = self.session.snapshot_request();
        q.kind = "module_status".into();
        let mut v: Value = serde_json::from_slice(&q.encode()?).map_err(|e| e.to_string())?;
        v["contract"] = "GP05-modules".into();
        self.transport
            .send_frame_until(&serde_json::to_vec(&v).unwrap(), deadline)?;
        for _ in 0..64 {
            self.check_guard()?;
            let Some(d) = self.transport.receive_document_until(deadline)? else {
                break;
            };
            if Instant::now() >= deadline {
                break;
            }
            if d.value()["contract"] == "GP05-modules" && d.value().get("available").is_some() {
                let o = crate::recorder::Observation::decode(
                    d.value(),
                    &q.context.show_id,
                    &q.context.epoch,
                )?;
                if Instant::now() >= deadline {
                    break;
                }
                self.recorder.observation = Some(o);
                self.recorder.received = Some(sent);
                return Ok(());
            }
            if let Some(d) = self.processing_document(d)? {
                let r = audio::decode_reply_document(d)?;
                if !self.telemetry(&r)? {
                    self.session.accept(r, self.now())?;
                }
            }
        }
        Err("REC readback unavailable; reconnect read-only, never replay".into())
    }
    pub(crate) fn stage_record(&mut self, start: bool, body: Value) -> Result<(), String> {
        if self.scope != "foh" {
            return Err("REC requires FOH attachment and explicit grant".into());
        }
        self.refresh_record()?;
        self.recorder
            .observation
            .as_ref()
            .ok_or("REC observation")?
            .admit(start, &body)?;
        self.stage(if start { "record_start" } else { "record_stop" }, body)
    }
    pub(super) fn record_reply(&mut self, v: &Value) -> Result<(), String> {
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
                "take_id",
                "operation_id",
                "state",
                "reason",
            ],
        )?;
        let r = self
            .record_requests
            .iter()
            .find(|r| {
                let c = &r.context;
                v["version"] == r.version
                    && v["show_id"] == c.show_id
                    && v["module"] == c.module
                    && v["epoch"] == c.epoch
                    && v["writer"] == serde_json::json!(c.writer)
                    && v["lease"] == serde_json::json!(c.lease)
                    && v["request_id"] == serde_json::json!(c.request_id)
                    && v["take_id"] == r.body["take_id"]
                    && v["operation_id"] == r.body["operation_id"]
            })
            .ok_or("REC reply correlation")?;
        if !matches!(
            v["state"].as_str(),
            Some("accepted_pending" | "rejected" | "completed" | "cancelled")
        ) || !(v["reason"].is_null() || v["reason"].as_str().is_some_and(|s| s.len() <= 1024))
        {
            return Err("REC reply state/reason".into());
        }
        if self
            .session
            .pending
            .as_ref()
            .is_some_and(|p| p.request.context == r.context && p.request.kind == r.kind)
        {
            self.session.pending = None;
            self.session.last_result = format!(
                "REC {}: admission/lifecycle reply only; inspect take readback",
                v["state"].as_str().unwrap()
            );
        }
        Ok(())
    }
    pub(super) fn mutate_record(&mut self, kind: &str, body: Value) -> Result<(), String> {
        let result = self.mutate_record_once(kind, body);
        if result.is_err()
            && let Some(pending) = self.session.pending.as_mut()
            && crate::recorder::mutation(&pending.request.kind)
        {
            // Send/receive/guard failures may occur after host admission. Old
            // take health must not remain fresh while this outcome is unknown.
            pending.state = audio::PendingState::Uncertain;
            self.recorder.received = None;
        }
        result
    }
    fn mutate_record_once(&mut self, kind: &str, body: Value) -> Result<(), String> {
        let revision = self
            .session
            .snapshot
            .as_ref()
            .ok_or("REC raw snapshot")?
            .authority
            .revision
            .clone();
        self.refresh_record()?;
        self.recorder
            .observation
            .as_ref()
            .ok_or("REC observation")?
            .admit(kind == "record_start", &body)?;
        if self
            .session
            .snapshot
            .as_ref()
            .is_none_or(|s| s.authority.revision != revision)
        {
            return Err("REC review revision changed".into());
        }
        self.check_guard()?;
        let r = self.session.begin(kind, body, self.now())?;
        if self.record_requests.len() == 64 {
            self.record_requests.pop_front();
        }
        self.record_requests.push_back(r.clone());
        let deadline = Instant::now() + Duration::from_millis(250);
        self.transport.send_frame_until(&r.encode()?, deadline)?;
        for _ in 0..64 {
            self.check_guard()?;
            let Some(d) = self.transport.receive_document_until(deadline)? else {
                break;
            };
            if Instant::now() >= deadline {
                break;
            }
            if d.value()["contract"] == "GP05-modules" {
                let current = d.value()["request_id"] == serde_json::json!(r.context.request_id);
                let refused = d.value()["state"] == "rejected";
                let reason = d.value()["reason"].clone();
                self.record_reply(d.value())?;
                if current {
                    if refused {
                        return Err(format!("REC refused: {reason}"));
                    }
                    // Admission is sufficient to release the command slot. The
                    // take remains preparing/finalizing until polled owner evidence.
                    self.refresh_record()?;
                    return Ok(());
                }
            } else if let Some(d) = self.processing_document(d)? {
                let reply = audio::decode_reply_document(d)?;
                if !self.telemetry(&reply)? {
                    self.session.accept(reply, self.now())?;
                }
            }
        }
        self.recorder.received = None;
        Err("REC admission UNKNOWN; no retry; reconnect and inspect take".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Silent;
    impl AuthorityConnection for Silent {
        fn send_frame_until(&mut self, _: &[u8], _: Instant) -> Result<(), String> {
            Ok(())
        }
        fn receive_until(&mut self, _: Instant) -> Result<Option<Vec<u8>>, String> {
            Ok(None)
        }
        fn receive_available(&mut self) -> Result<Option<Vec<u8>>, String> {
            Ok(None)
        }
    }
    fn operator() -> Operator {
        Operator::from_connection(
            Box::new(Silent),
            "11111111-1111-4111-8111-111111111111",
            9,
            "rec-test",
            "foh",
        )
        .unwrap()
    }
    #[test]
    fn record_reply_requires_every_sent_identity_and_does_not_claim_outcome() {
        let mut o = operator();
        let r: audio::Request = audio::Request {
            version: 1,
            context: audio::Context {
                show_id: "11111111-1111-4111-8111-111111111111".into(),
                module: "audio".into(),
                epoch: "9".into(),
                writer: Some("rec-test".into()),
                lease: Some("1".into()),
                request_id: Some("2".into()),
                expected_revision: Some("0".into()),
            },
            kind: "record_start".into(),
            body: json!({"take_id":"take-test","operation_id":"17"}),
        };
        let bytes = r.encode().unwrap();
        let wire: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(wire["contract"], "GP05-modules");
        o.record_requests.push_back(r.clone());
        let mut reply = wire;
        reply.as_object_mut().unwrap().remove("expected_revision");
        reply.as_object_mut().unwrap().remove("kind");
        let body = reply.as_object_mut().unwrap().remove("body").unwrap();
        reply["take_id"] = body["take_id"].clone();
        reply["operation_id"] = body["operation_id"].clone();
        reply["state"] = "accepted_pending".into();
        reply["reason"] = Value::Null;
        o.record_reply(&reply).unwrap();
        assert!(o.recorder.observation.is_none());
        for key in [
            "show_id",
            "epoch",
            "writer",
            "lease",
            "request_id",
            "take_id",
            "operation_id",
        ] {
            let mut bad = reply.clone();
            bad[key] = "wrong".into();
            assert!(o.record_reply(&bad).is_err(), "{key}");
        }
        o.session.pending = Some(audio::record_test_pending(r));
        assert!(o.session.retry(100).is_none());
        assert!(o.session.retry_wake(100).is_none());
        o.record_reply(&reply).unwrap();
        assert!(o.session.pending.is_none());
        assert!(o.session.last_result.contains("readback"));
        assert!(o.recorder.observation.is_none());
        let mut newer = o.record_requests.front().unwrap().clone();
        newer.context.request_id = Some("3".into());
        o.record_requests.push_back(newer.clone());
        o.session.pending = Some(audio::record_test_pending(newer));
        reply["state"] = "completed".into();
        o.record_reply(&reply).unwrap();
        assert_eq!(
            o.session
                .pending
                .as_ref()
                .unwrap()
                .request
                .context
                .request_id
                .as_deref(),
            Some("3")
        );
        o.recorder.received = Some(Instant::now());
        assert!(
            o.mutate_record(
                "record_stop",
                json!({"take_id":"take-test","operation_id":"17"})
            )
            .is_err()
        );
        assert!(!o.recorder.fresh());
        assert_eq!(
            o.session.pending.as_ref().unwrap().state,
            audio::PendingState::Uncertain
        );
    }
    #[test]
    fn missing_readback_is_unknown_and_never_reuses_old_freshness() {
        let mut o = operator();
        o.recorder.received = Some(Instant::now());
        assert!(o.refresh_record().is_err());
        assert!(!o.recorder.fresh());
        assert!(o.record_query_failed);
        assert!(o.refresh_record().unwrap_err().contains("retired"));
    }
}
