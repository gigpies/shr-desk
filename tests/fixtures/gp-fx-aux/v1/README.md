# Actual GP-FX-aux producer fixture

`owner.json` is the four-input synthetic LocalAudio result after a reviewed
source-boundary publication, send ramp and actual native-f64 delay settlement.
It includes all ten actual provider defaults/descriptors and per-aux owner status.
Four fixture inputs are not a product capacity limit; the actual Frontend episode
uses sixteen and monitor count remains independently configured.

Regenerate from GigPies with the parent-held shared build lock, Rust1.97.1,
locked Cargo.lock and incremental compilation off:

```sh
GP_FX_LIBRARY=/absolute/libshr_fx.so \
GP_FX_AUX_FIXTURE=/absolute/shr-desk/tests/fixtures/gp-fx-aux/v1/owner.json \
cargo +1.97.1 test --locked --features hardware-host --lib actual_aux_authority -- --ignored
```

The library SHA256 is carried by the fixture itself. This is offline software
contract evidence, not physical port, listening or aggregate cost acceptance.

`brain-owner.json` comes from the actual authenticated loopback QUIC episode:
four Stagebox sends, four Brain-owned effects (delay, plate, tape echo, bright
exciter), and returned wet sample comparisons at Main. It includes the opaque
media-session binding. Regenerate from GigPies with the same lock/toolchain:

```sh
GP_FX_LIBRARY=/absolute/libshr_fx.so \
GP_FX_AUX_REMOTE_FIXTURE=/absolute/shr-desk/tests/fixtures/gp-fx-aux/v1/brain-owner.json \
cargo +1.97.1 test --release --locked --features hardware-host --lib actual_four_aux_tls -- --ignored --test-threads=1
```

Use optimized code for this opt-in real transport episode; all existing control
and lease deadlines remain unchanged. The normal decoder tests use these frozen
producer artifacts without starting a network or loading an external library.

`return-owner.json` and `return-brain-owner.json` are opt-in version-1 return
snapshots produced by the actual Desk Frontend keyboard/review/confirmation path.
Every aux reaches Main 0.95, monitor-1 0.05 and monitor-2 0.10, with matching
host applied generations and independently checked owner state. The former uses
the 16-input headless local provider; the latter uses the four-input authenticated
loopback Stagebox/Brain provider. Both use the approved GP-H1 SHR FX library.
No fixture is evidence of physical output or a product capacity ceiling.

Build Desk's `fx_aux_actual` integration executable with `cargo +1.97.1 test
--locked -j1 --test fx_aux_actual --no-run`. Under the same shared lock and
single-job environment, regenerate the local fixture from Desk:

```sh
GP_FX_PROVIDER_EXE=/absolute/gigpies/target/release/gigpies-headless \
GP_FX_LIBRARY=/absolute/libshr_fx.so \
GP_FX_DESK_FIXTURE=/absolute/shr-desk/tests/fixtures/gp-fx-aux/v1/return-owner.json \
cargo +1.97.1 test --locked -j1 --test fx_aux_actual \
  actual_four_aux_frontend_review_and_readback -- --ignored --test-threads=1
```

For independent sample acceptance, set `GP_FX_DESK_TEST_EXE` to the exact Desk
executable reported by `--no-run`, and run from GigPies:

```sh
GP_FX_LIBRARY=/absolute/libshr_fx.so \
GP_FX_DESK_TEST_EXE=/absolute/shr-desk/target/debug/deps/fx_aux_actual-HASH \
GP_FX_DESK_FIXTURE=/absolute/shr-desk/tests/fixtures/gp-fx-aux/v1/return-brain-owner.json \
cargo +1.97.1 test --release --locked -j1 --features hardware-host --lib -- \
  --ignored --test-threads=1 --nocapture \
  actual_frontend_returns_local_samples actual_four_aux_tls_brain_controls_and_samples
```

The local sample harness does not write the remote fixture. Both harnesses own
and clean their private sockets/credentials/children and compare independent
Main/monitor samples after the Frontend reaches applied readback. Normal tests
decode both frozen snapshots without starting providers. Current acceptance,
limitations and remaining work belong to GigPies' GP-FX card.
