# Using the audio desk

[Open the desk](TERMINAL.md) · [GigPies guide](https://github.com/gigpies/gigpies/blob/main/docs/README.md) · [Engineering](ENGINEERING.md)

These controls require a configured, connected session. Availability depends on
the engine and the control permission granted for the selected page.

## Controls without computer shortcuts

The live desk is intended for its assigned MIDI controller. Computer-keyboard
shortcuts are optional fallback controls for the same actions.

Outside the Channel panel, the terminal footer provides **Previous, Next, Choose, Cancel, Character−,
Character+, Add and Delete**. Previous/Next opens the labelled action menu;
Choose acts on a full press and release. The menu includes connection/control,
page navigation, value editors and review actions. Select an editor, choose and
add characters, then accept/review the value through the menu. Inspect the actual
review before choosing Confirm; opening the menu does not count as reading it.
The footer is also clickable. On Channel, the secondary pad layer has **Controls**;
its existing band/compressor/output pads stay in place. F10 is the optional
keyboard menu shortcut.

This software path accepts explicitly bound controller messages; it does not
open a MIDI device or install a preset. Physical input acquisition, learned
bindings and device acceptance remain unfinished under
[GP-H2](https://github.com/gigpies/gigpies/blob/main/docs/development/MODULE_IMPLEMENTATION_PLAN.md#gp-h2--physical-dual-console-operation).
The key references below document the optional computer-keyboard fallback.

## Selecting inputs and Master

The terminal requires 160×49 cells. Its three-row strip has 24 input slots and a
separate Master key; click to select, and use PageUp/PageDown for other input
banks. Green plus `>` identifies selection. R/S/M are independent status readouts,
never toggles; `?` means unknown/unavailable. Solo and Master gain controls are
currently unavailable. Finish or cancel a draft/review before selecting elsewhere.

## Channel EQ, compression and Gate

With processing enabled, **F2** opens Channel in FOH scope. Select a parameter,
use **U/I** to select a field and **J/K** to adjust it, then **F4** opens the review. Read the complete review and press
**Enter** separately to apply. **Esc** cancels an unsubmitted edit.

Click the labelled Compressor/Gate selector to change columns 5–7. Gate offers
threshold/attack, hold/release and range/hysteresis; detector Hz/Q and bypass are
in its menu. Manual uses the full-band detector. Tom requires you to set its
measured detector frequency first. Automatic Import accepts the absolute path of
a GigPies `gate-prepare` proposal and shows its confidence/reason. It fills a local
draft; review it before applying. An uncertain proposal stays bypassed. Prepared
settings remain fixed during performance.

### Prepare from recorded soundcheck cues

Select the intended input channel, then press **O** in Channel, or choose **Gate recorded
soundcheck** in the **F10** controller menu. This uses an installed GigPies executable
on the Desk machine for offline analysis; it opens no audio device. Enter its
absolute path in the first field. The normal workflow needs no JSON editing.

Use **Tab/Up/Down** to choose a field, type its value, and **Enter** to accept it.
Give calibration and held-out evaluation distinct labels. In each of the four
cue groups, add absolute WAV paths individually or select a directory containing
only that group's cues. Directory selection adds its WAV files, nonrecursively;
inspect the selected names before preparing. **Left/Right** shows an individual
cue path, **F9** removes it, and **Delete** clears the selected field/group.

Each group needs 8–32 independent mono 48 kHz PCM16/24/32 WAVs: calibration target
hits, calibration non-target bleed, held-out target hits and held-out bleed.
Each hit must include its full decay. Files must end on complete 10 ms windows,
last no longer than 40.96 seconds, and be at most 8 MiB. These labels assert known
pre-gate source identity; an arbitrary mixed recording cannot substitute for an
isolated target cue. Copies of the same decoded audio are rejected, even under
different filenames. Do not select accepted mixes or post-gate recordings.

**F6** starts preparation, with cue progress and a two-minute work limit. **F7**
cancels the read-only job; **Esc** cancels and closes. A stopped job must release
its process before another starts. File changes, missing files, unsupported WAVs
or a failed GigPies process show an error and create no applicable result.

Inspect separation, detector, threshold/timing, confidence, held-out missed hits,
false openings and energy preservation. Refused evidence produces bypass, never
an enabled guess. Evaluation does not retune the calibration-selected settings.
**F8** transfers the result to a frozen local draft. **F4** opens the full
current-versus-proposed review, including evidence; read all pages and press
**Enter** separately to apply. Changed files or stale channel/provider context
require cancellation and fresh preparation. Reconnect never replays a job or
applies a proposal. Cancel the draft to return to manual editing.

Wait for correlated completion and settled authoritative readback. Then use
[Audio setup Save/Restore](#save-and-restore-an-audio-setup) to save the resulting
stable gate settings with the rest of the setup. Cue paths, preparation jobs,
grants and execution state are not saved. Physical acquisition, playback and
listening qualification remain separate.

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

## Recording

In FOH scope, **J** opens Recorder and reads the engine's current take. Use **G**
to request control. **N** reviews a new automatically named take; inspect the raw
track order and confirm with Enter. **S** reviews Stop for the displayed take;
confirm to drain and finalize. The controller menu also exposes Recorder, Record
new take and Stop recording. Esc cancels an unsubmitted review.

Wait for **recording** after Start. **Preparing** is not capture readiness.
After Stop, wait through **finalizing** and inspect the final outcome: complete,
incomplete or error. Accepted frames entered the queues; written frames reached
all stems and the journal. Queued is their difference. Durable stays **UNKNOWN**.
Drops, gaps, clipping and writer/host faults require attention even when frames
were written. Start checks storage asynchronously and can finish with an error.

Files live under the engine's private `takes/<take-id>/`. Desk does not select
paths, recover files or play them. Leaving Desk or losing its lease does not stop
recording. After connection loss, the last observed take remains labelled stale with current
outcome unknown. After a lost reply, reconnect read-only and inspect the take; never
assume failure or repeat Start blindly. A restarted host cannot certify an old
interrupted take; use the recorder owner's evidence-preserving recovery.

## When a control is unavailable

Check connection, selected scope and control grant. **Q** releases your control
permission; it does not mute, release parameter Holds or stop the engine.
After connection loss, read the current state before reviewing another change.
[Detailed controls and recovery](NATIVE_FRONTEND.md).

## Save and restore an audio setup

Set `SHR_DESK_AUDIO_SETUP_DIRECTORY` to an absolute directory owned by your user,
with permissions `0700`. Desk offers sixteen `audio-setup-NN.json` slots. This is
separate from the FX-only slots. Saving replaces the selected slot durably; a
reported storage error means replacement/durability may be uncertain.

Use the controller/touch menu's **Audio setup** actions:

1. Configure channels, Main, monitor sends/taps, routing, four FX auxes and PA
   through the normal controls. Wait for transitions to settle. Select a slot
   with Previous/Next slot, then **Save / replace selected slot**. The status
   identifies the selected slot. Saving needs fresh complete owner readback.
2. After restarting, select that slot and choose **Load and review current /
   saved**. Loading and review do not change audio or acquire control. Read the
   differences and remaining scopes; Cancel closes this read-only review.
3. Select **Control PA**, request control, then **Hold all outputs** and confirm
   its review. This quiesces Stagebox/PA, disarms Brain monitoring and closes
   talkback. The provider refuses output rearm, Brain arm and held talkback
   while this hold is active, including after Desk disconnects.
4. Select each scope listed in the review (FOH, each affected monitor, PA,
   output routes, operator monitor, talkback settings, FX). Request control,
   choose **Restore current scope**, inspect every review page, and confirm.
   Each command needs correlated completion and exact settled readback. A scope
   may stop part way through; completed steps stay applied. No rollback is claimed.
5. Choose **Verify all saved settings**. Zero remaining differences means the
   current authoritative setup matches the saved intent. Recall receipts are
   shown separately: a matching value never converts an UNKNOWN command into a
   successful receipt. The review also reports live output hold, quiescence,
   Brain arm and talkback-held state. Other scopes may still need review and restore.
6. When satisfied, return to PA control and explicitly **Release hold, keep
   disarmed**. Output rearm and Brain monitoring arm remain separate reviewed
   actions. Neither saving, loading, verification nor reconnect activates outputs.

The file includes channel gate/EQ/compression, fader/pan/mute, Master gain, all
supported monitor levels and tap choices, output source assignments, complete PA
configuration (including supported Master EQ), all four FX configurations and
stable Brain monitor/talkback settings. It excludes recording execution, lighting,
automation holds/modes, held talkback, grants, credentials and armed state.

Input and output identities, sample rate and dimensions must match exactly.
Changed port identities are incompatible even at the same channel count. There
is no automatic remapping. Missing or incompatible owners, malformed files,
stale reviews and unsettled processing refuse restoration. A bounded recall can
pause with remaining differences; review again to continue.

After interruption or provider failure, reconnect read-only and review the slot
again. The small `audio-recall-NN.json` file retains each scope's last completion
or UNKNOWN receipt. Do not delete an UNKNOWN receipt to imply success. Fresh
review identifies what is currently matched and what still differs; explicit
restore applies only those differences. Re-establish the setup hold after a
provider restart. This workflow has software acceptance only; physical routing,
listening, timing and live-show qualification are separate.
