# Using the audio desk

[Open the desk](TERMINAL.md) · [GigPies guide](https://github.com/gigpies/gigpies/blob/main/docs/README.md) · [Engineering](ENGINEERING.md)

These controls require a configured, connected session. Availability depends on
the engine and the control permission granted for the selected page.

## Channel EQ and compression

With processing enabled, **F2** opens Channel in FOH scope. Select a parameter,
use **U/I** to select a field and **J/K** to adjust it, then **F4** opens the review. Read the complete review and press
**Enter** separately to apply. **Esc** cancels an unsubmitted edit.

The four EQ frequency/gain pairs occupy the left side; compressor controls are
in the middle; pan and level are on the right. Blue marks selection and amber
marks a local draft. A draft is not the current engine setting.

Use **D** for exact fader dB entry and **C** for pan. Enter accepts the text;
F4 and a separate Enter complete the review/apply step. Fader range is
−60 to +12 dB; pan runs from −100 (left) to +100 (right).

## Meters and spectrum

Channel shows raw input peak/RMS, compressor gain reduction and processed output.
**OUT is after EQ/compression but before mute, fader and pan.** It is not the
final main-bus meter. Stale observations leave unlit scales.

From Channel, **F11** opens fullscreen EQ/spectrum. **[ / ]** change the analysis
source without changing the EQ target. Click an EQ point to select its band;
use the usual edit/review controls. F11 or the header returns to Channel.
Outside fullscreen, [ / ] adjust pan.

**B** opens the separate meter page; **T** changes raw/pre-fader selection and
**PageUp/PageDown** changes monitor banks. Missing measurements are unavailable,
not generated display activity.

## Monitor sends

**F12** opens sends. **F1** shows a monitor across inputs; **F2** shows one input's
sends. Arrows select inputs, PageUp/PageDown change banks, and **U/I** browse
monitors. **O** attaches to the chosen monitor; request a fresh **G** control grant.

**S** edits send dB. **E**, then **1/2/3**, chooses Raw/Post-mute,
Processed/Pre-fader or Processed/Post-fader. Review with F4, then Enter.
Level and tap changes are separate operations. **F** returns to FOH; **V** selects PA.

## Master EQ

In PA scope, **F11** opens the muted setup editor. Mute and review the outputs
first; after applying settings, rearming is a separate action.

**B** switches PEQ/GEQ, **C** selects linked L/R or one side, **U/I** selects a
parameter and **J/K** changes it. **F3** enters an exact value. F4 reviews.
Supported systems also offer a separate live EQ path: **L** checks availability,
then **E** opens the editor. Live EQ does not rearm outputs.

## Effects

**X** selects FX scope, **W** opens/probes the stereo FX owner, and **G** requests
control. **L/R** selects a side. **E** enters delay time, feedback, damping and
wet gain; Enter opens review, followed by a separate confirmation.

**Space** reviews bypass, allowing tails to drain. **P** reviews panic for the
selected side; **B** reviews panic for both. Panic clears the tail; it is not a
persistent output mute. This is the integrated stereo effect, not the full
standalone SHR FX rack.

## When a control is unavailable

Check connection, selected scope and control grant. **Q** releases your control
permission; it does not mute, release parameter Holds or stop the engine.
After connection loss, read the current state before reviewing another change.
[Detailed controls and recovery](NATIVE_FRONTEND.md).
