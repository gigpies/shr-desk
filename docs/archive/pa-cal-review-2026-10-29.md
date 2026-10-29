# DS-CAL-REVIEW — calibrated offline Desk review

State: **ACCEPTED (software only)**, 2026-10-29. Single-threaded canonical-checkout work.

Explicit offline sweep import and owner result review in Desk; preserve provenance and route candidates through muted GP14 review.

Plan: add explicit catalog-ID proposal entry, distinguish imported results and present complete calibration/origin evidence in muted review. Reuse existing cancellation/freshness handling. Actual-owner application is tracked only in the shared integration card. No automatic apply/rearm. No devices (including null PCM), services, hardware or load tests.

Acceptance:
- [x] Bounded evidence and calibration provenance, common origin and configuration identity preserved.
- [x] Imported offline evidence visibly distinct from authenticated captures.
- [x] Detached-entry, provenance presentation, cancellation and current-basis admission checks pass.
- [x] Required normal production and compatibility suites pass without device access.
- [x] Separate physical procedure documented; disposable outputs cleaned.

Evidence: baseline PA f6be5ae, Desk 511a811, GigPies bddb503; all three checkouts clean. PA published CI fails sibling-relative documentation links (fixed locally); Desk and GigPies published CI pass.

Next action: separately authorized physical qualification; commit/push only on user request. No pending software blocker in this scope.

Shared integration owner: [GP-CAL-REVIEW](https://github.com/gigpies/gigpies/blob/main/docs/archive/pa-cal-review-2026-10-29.md).

## Consumer validation

P accepts `offline REVIEW_ID EVIDENCE_ID` through explicit GP20 v2. Imported
results preserve their distinct kind; complete calibration points, provenance,
original timing metadata and candidate configuration appear in muted apply review.
The same workflow is available through `measurement propose offline ...`.
No owner analysis, device access or automatic application is implemented in Desk.

Rust 1.97.1, CARGO_INCREMENTAL=0, CARGO_BUILD_JOBS=1, canonical target:
`cargo test --locked --all-targets`: 351 passed / 25 opt-in ignored.
The same command with `--features native`: 354 passed / 25 ignored.
Native all-target Clippy with `-D warnings`, nine Python policy tests, formatting
and complete candidate-index publication checks passed. Frozen producer corpora
remain in the default suite. The ignored release actual-provider drivers were
explicitly run; their cross-module evidence belongs to the shared record.

The new ignored driver initially sent `Escape`, while the configured key adapter
expects `Esc`. Only that driver spelling changed after the production suites;
production code stayed unchanged. Both release interoperability drivers then
passed on the final tree. No display, controller, audio or hardware session ran.
