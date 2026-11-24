# Status

[Using the desk](USING.md) · [Engineering](ENGINEERING.md) · [GigPies status](https://github.com/gigpies/gigpies/blob/main/docs/STATUS.md)

**Software-accepted against the real GigPies engine with synthetic audio. Physical
controllers, displays and listening are not yet qualified.** Shared integration
tasks and their open gates live in the
[GigPies integration plan](../../gigpies/docs/development/MODULE_IMPLEMENTATION_PLAN.md).

| Area | Works in software | Still open |
|---|---|---|
| Channel | Four-band EQ, compressor, default-bypassed gate (processing v5) with Manual/Tom/Automatic import and recorded-soundcheck preparation; fader, pan, mute, Hold; reviewed apply with settled readback | Listening on real drums (GP-GATE) |
| Selection | Three-row keybed: 24 inputs per bank plus Master; draft/review guards preserved | Solo has no provider contract |
| Meters and spectrum | Raw/pre-fader input, Main pre-PA and every monitor bus; stale, invalid and silent shown distinctly; separate GR age; fullscreen EQ/RTA | Remaining frontend test coverage (GP-METER P3) |
| Monitor sends | Per-input sends with raw / pre-fader / post-fader taps, reviewed edits | — |
| Master | Master EQ (muted review), Master gain strip | Pan/mute unavailable by contract |
| Effects | Four Brain FX auxes: pre/post-fader sends, Main/monitor returns, palettes, bypass, panic | Listening, sustained load (GP-FX) |
| PA setup | Measurement review, imported calibrated sweeps, muted PA configuration transactions; feedback-notch output selection and applied-frequency readback | Synchronized physical workflow (GP-PA-MEASURE); feedback rehearsal (GP-PA-FEEDBACK) |
| Recording | Reviewed Start/Stop with FOH lease, readiness/progress/final outcome; no reconnect replay | Physical capture (GP-H1) |
| Brain listening | PFL/AFL, talkback with reviewed grants | Physical duplex device (GP-H1) |
| Audio setups | Sixteen slots, reviewed save/restore per granted scope under an output hold | Physical recall |
| Controllers | Semantic pad menus, typed actions; learned SHR-DAW MiniLab mapping imported (`--controller-import-daw`), read-only MIDI acquisition (`--controller-profile`) | Pads bank 2/key channel check, LEDs, disconnect recovery on hardware (GP-H2) |
| Rendering | Ratatui terminal panel (shared instrument panel 0.3.3); optional native window; CPU offscreen checks | Page-by-page physical redesign, SSH path (GP-PANEL) |

Nothing here opens audio, MIDI output or DMX on its own; the controller reader is
read-only and only runs when a profile is supplied.

Dated validation records, counts, hashes and failures up to 2026-11-24 are kept in
[the evidence archive](archive/evidence-to-2026-11-24.md).
