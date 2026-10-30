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
