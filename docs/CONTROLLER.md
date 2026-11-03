# MiniLab mkII controller flows

## Production semantic menu

`src/controller_surface.rs` owns the labelled menu, bounded character wheel and
complete-message pad binding. The eight logical pads are Previous, Next, Choose,
Cancel, Character−, Character+, Add and Delete. Previous/Next also opens the menu;
no encoder click, extra pad bank or computer key is required for this path.
Physical profiles must supply the exact endpoint, MIDI channel and eight distinct
note numbers; there is no guessed default. Note-off and velocity-zero note-on
complete the gesture. Repeats cannot choose twice; lost input requires release.
No MIDI port, preset or LED output is opened by this implementation.

Menu actions call the existing provider-backed semantic workflow, not simulated
computer keys. Numeric and text editors use existing parsing, capacity and
review gates. The actual review must be displayed separately; the menu overlay
cannot acknowledge hidden review content. Controller input retains its acquisition
generation, and stale role/context/input cannot be replayed against a new target.

The terminal footer exposes the same logical controls for software operation.
The physical input worker/profile installation and target-device validation remain
[GP-H2](https://github.com/gigpies/gigpies/blob/main/docs/development/MODULE_IMPLEMENTATION_PLAN.md#gp-h2--physical-dual-console-operation).
The older simulator mappings below are separate from this production menu path.


2026-10-04. The related SHR LUX/DAW documentation identifies an **Arturia MiniLab
mkII**, USB `1c75:0289`. No device was opened in this task. This is an evidence-based
starting profile, not a fresh inventory or proof of the active memory's messages.

The [Arturia manual](https://downloads.arturia.net/products/minilab-mkII/manual/minilab-mkii_Manual_1_1_EN.pdf)
describes 25 keys, sixteen rotary encoders, eight pads with two banks, and octave
transposition. SHR FX's interface records physical clicks on rotaries 1 and 9.
The encoders are endless controls, not motorized faders. Learn their actual
absolute/relative encoding; never infer it from appearance or CC number.

## Profile/input/output implementation

`src/controller_flow.rs` and `Frontend::inject_controller_flow` accept complete
messages from an explicitly named endpoint with acquisition generation. No MIDI
port is opened. Profiles declare sixteen relative encodings/CCs, optional alternate
CCs only for rotaries 1/9, two distinct pad-note banks, key channel/anchor, and
optional exact captured Shift-down/up messages. A profile must be installed with
`Input::reset(current_generation)`. Unknown hardware bindings remain unqualified.
General held Shift applies to all rotaries only when those messages are supplied;
an alternate rotary CC implies Shift for that turn only and dispatches once.
No mkII Shift SysEx or MiniLab 3 mapping is invented. Shift+pad/key stays reserved
for hardware memory/channel behavior. Context/input loss clears modifiers and
requires pad release; delayed generations cannot act on a new selection.

The normal menu's first bank uses its eight labelled semantic gestures; the second
bank uses Mix, Channel, Sends, Controls, Grant, Release Hold, Cancel, Reconnect.
Pad actions complete on release, preserving the action captured at press. The
selected Channel's on-screen pad preview is still separate from that menu binding.

`Feedback` is a bounded non-audio poll worker over an injected `Output` transport:
explicit endpoint and `arturia-minilab-mkii-pad-v1`, one latest 16-colour state,
changed-pad coalescing, at most one 16-message batch per 50 ms. Frontend feedback
uses current menu labels and confirmed grant state, never input echoes. Errors
leave input independent and retry changed output on a later batch. Reconnect
invalidates the sent cache. No preset/backlight writes or real messages are sent.
Physical press-white, bank/Shift/memory indication and exit restoration remain
unqualified; the transient last colours may remain until hardware replaces them.

### EQ Shift, gain guards and Master speed

Columns 1–4 are frequency above gain; held Shift changes upper to Q and lower to
fine gain. Fine gain is 0.1 dB per decoded step, without acceleration. Column 8 is
pan above level for the explicitly selected target. Channel upward gain sensitivity
uses a smoothstep factor from 1 below −12 dB to 0.2 at +6 dB, separately clamped
at +12 dB. Normal steps are 0.5 dB; downward steps keep full sensitivity. Each of
at most eight decoded steps evaluates the guard again. These are configurable
software policy values; fader level is not measured audio headroom.

The separate Master policy starts its guard at −18 dB, reaches minimum sensitivity
at 0 dB and caps at +6 dB. The pure `MasterSpeed` model requires the profile's
explicit return-to-center capability: 8% dead zone, quadratic displacement,
maximum ±0.5 dB/s before the upward guard, 250 ms stale-input/application timeout,
elapsed-time integration through center transitions, and immediate stop on permission/generation
loss. Center stops and preserves gain. No finger-release event is assumed.
**Master speed-to-provider integration is pending** because no dedicated Master
gain contract exists. The production adapter refuses this intent; model tests
are not a claim of a working live Master strip. Physical feel remains pending.
The modulation strip/CC1 is unassigned. SHR DAW's separate pitch/CC1 synth
forwarding is unchanged.

### Gate and Compressor modes

Click the labelled Compressor/Gate mode selector in Channel, or choose
Compressor rotaries / Gate rotaries in the controller menu. Gate uses column
5 threshold/attack, 6 hold/release, 7 range/hysteresis. Its menu includes bypass
and detector Hz/Q; zero Hz disables detector filtering. Compressor retains
5 threshold/attack, 6 ratio/release, 7 knee/makeup. All edits use existing draft,
review, permission and generation checks. Gate attenuation/state is separate
from compressor GR. Dynamic sessions require processing v5 for gate writes.
The Gate menu offers **Manual**, **Tom** and **Automatic Import**. Manual enables
the gate with detector filtering disabled. Tom requires a measured nonzero
frequency set in Detector first; it makes no instrument-specific frequency guess.
Automatic Import opens an absolute-path entry for a GigPies `gate-prepare` JSON
proposal (16 KiB maximum). It checks the stable input ID, distinct calibration and
evaluation IDs, config bounds and confidence/reason coherence. The imported
settings remain a local draft; confidence/reason appears in the status message,
and Review/Apply still requires a separate confirmation. Rejected evidence imports
bypass. Settings are frozen after preparation. GigPies also supplies cued-PCM
preparation with calibration-only detector selection and independent causal
evaluation; Desk imports its identified result rather than classifying audio.

Reuse evidence: SHR DAW `src/midi.rs` tests preserve pitch bend/CC1 musical
forwarding and exercise configured held modifiers/alternate encoder CCs;
`docs/CONTROLLER_INTERFACE.md` explicitly distinguishes those input mechanisms.
Its `docs/CONTROLLER_LED_FEEDBACK.md` documents the palette/packet and unresolved
hardware-owned indications. These sources establish reusable software/protocol
evidence, not captured messages for the Desk's actual device/preset.

## Selected Channel: eight physical columns

The operator's October 12 direction defines the selected Channel layout. The screen follows the MiniLab mkII's two rows of eight encoders and one
row of eight pads. These are intended physical positions, not verified CC/note
bindings. The [Arturia manual](https://downloads.arturia.com/products/minilab-mkII/manual/MiniLabmkII_Manual_1_0_7_EN.pdf)
describes the pad 1–8 / 9–16 bank switch; it does not establish our active preset's
messages or LED feedback protocol.

| Column / pad | Upper encoder | Lower encoder |
|---|---|---|
| 1 | 1: band 1 frequency | 9: band 1 gain |
| 2 | 2: band 2 frequency | 10: band 2 gain |
| 3 | 3: band 3 frequency | 11: band 3 gain |
| 4 | 4: band 4 frequency | 12: band 4 gain |
| 5 | 5: threshold | 13: attack |
| 6 | 6: ratio | 14: release |
| 7 | 7: knee | 15: makeup |
| 8 | 8: pan | 16: channel level |

The eight primary pads are Band 1, Band 2, Band 3, Band 4, Compressor,
Review/Apply, Hold and Output. Band menus expose frequency, gain, Q, individual
and global EQ bypass, review, cancellation and Back. The compressor menu exposes
its six parameters, bypass and Back. Output exposes pan, level, mute, Hold,
release, sends, cancellation and Back. Mute is secondary to the level pot.

The 9–16 layer carries Mix, Channel, Sends, Controls (including Health), Writer Grant, Release Hold,
Cancel and Reconnect. Menu meanings are local to the selected screen. Clicking
`PADS 1–8` / `PADS 9–16` changes this preview layer; clicking a pad uses the same
existing semantic field or action path. It does not synthesize a MIDI message.
Green indicates fresh owner-enabled processing, red fresh owner mute, amber Hold
or an edit requiring review, and blue local selection/navigation. These meanings
are labelled on screen; green is not evidence that a physical output is playing.

Still missing: captured encoder/modifier/pad messages for the actual preset,
physical transport installation, absolute pickup in the new relative profile,
and physical end-to-end qualification. Semantic relative dispatch and testable
LED output are implemented above. No port is
opened and no controller memory is programmed by the terminal preview. Exact
entry, grants, full review, freshness and confirmation fences remain unchanged.

### Bank-switch evidence (software investigation, 2026-10-12)

Arturia MiniLab mkII manual 1.0.7 §§2.1.4, 2.1.7 and 3.4 describes the
button selecting two sets of subsequent pad functions and the illuminated second
bank. It does **not** specify a separate MIDI message emitted by the bank button.
Therefore neither “distinct message” nor “only subsequent messages” is verified
for this device/preset. Desk `src/midi.rs` currently decodes one fixture bank of
eight note numbers; it has no bank-switch decoder. Its two-bank SysEx encoder is
output construction, not evidence of input behavior. No MIDI port was opened.
The eventual profile must capture the button alone and both banks' pad messages;
until then the on-screen layer selector is a software menu control only. Learned
CC/note assignments, encoder modes and LED feedback remain outside the operating
screen and unqualified.

## Channel selection and the three-row keybed

The production `controller_flow` profile maps 24 consecutive notes from its
explicit anchor to the current 24-slot display bank; note 25 selects **Master**.
It never selects input 25 implicitly. PageUp/PageDown pages input banks; Master
stays anchored. Octave-transposed messages outside the learned 25-note range are
ignored. Changing hardware MIDI channel through Shift+key is hardware-owned;
Desk does not reinterpret it as a selection or add a second transposition.

Slots resolve through provider stable input IDs. Unavailable slots refuse
selection; sparse/reordered IDs must use the current displayed mapping. Selected
keys are green with a `>` fallback. Independent R/S/M positions show simultaneous
state; unknown/stale is `?`, preparing/finalizing is `P`/`F`, recording fault is
`!`. Recording means membership in the provider's active take mapping, not arm
or individual-track durability. Solo has no supported provider contract and
remains `?`. Master is explicit and currently read-only: the provider lacks a
dedicated Master gain/pan/mute target or Master recording contract. Its status
indicators remain unavailable (`?`). There is no silent input substitution.

The terminal reserves exactly three keybed rows above the existing footer,
requires 160×49 cells, and preserves the original character-grid silhouette.
Full selected ID and bank range are alongside it. Selection is guarded during
drafts/reviews and never toggles R/S/M. Detail/meter pages keep their 12-input
banks; 24 is only an overview bank. The old simulator decoder remains a separate
fixture path, not the production mapping or a hardware-qualified profile.

## Sixteen physical rotations

Physical rows are positions 1–8 and 9–16. The main Mix bank spans positions 1–12;
it wraps part-way across the lower row. The footer must draw the actual 2×8 shape
and channel IDs so this compromise is visible. Compare it on hardware with a
future selected-channel-first profile before deciding ergonomics permanently.

| Context | K01–K08 | K09–K12 | K13 | K14 | K15 | K16 |
|---|---|---|---|---|---|---|
| Mix | Channels 1–8 level in visible bank | Channels 9–12 level | Selected channel level | Selected pan | Select bus/scope, confirm before switching sends | Main level, only with explicit capability/grant |
| Sends | Send levels 1–8 to named bus | Sends 9–12 | Selected send | Send pan if supported | Destination browse/confirm | Current bus master |
| FX | Owner-provided named parameters | Owner-provided named parameters | Return level | Wet balance if meaningful | Slot browse | Focused value |
| Analysis | Cursor/range/zoom controls, no sound edits | Time/frequency controls | Tap browse | Freeze display | Panel browse | Focused value |

Unsupported positions show `--` and emit no commands. The initial simulator
implements Mix absolute-value controls K01–K14 only; K15/K16 refuse changes.
Channel uses the eight-column table above. Other context maps remain proposals
dependent on descriptors, not hard-coded DSP assumptions. Never map phantom power, routing, PA protection or recorder stop
to an unguarded rotary.

Learned clicks on K01/K09 can provide Open/Confirm and Back/Cancel, following SHR
FX's established convention. Every operation must remain available through pads
and the ordinary keyboard if those click messages are unavailable. Holding a pad
may provide fine mode only after release/timeout behavior has been verified.

## Eight pads, visible actions and feedback

The earlier simulator base-page proposal below remains separate from the
selected Channel menus above. Every screen must show what all eight pads will
do in its current context.

| Pad | Base action | State shown |
|---|---|---|
| 1 | Mix / return to FOH view | Cyan when selected; blue when available |
| 2 | Selected Channel | Cyan when selected |
| 3 | Analysis | Cyan when selected |
| 4 | Selected channel mute toggle | Red only when confirmed muted; yellow pending |
| 5 | Hold selected automation parameter | Amber/yellow held; pending distinctly labelled on screen |
| 6 | Open Release preview; confirm separately | Target/delta/ramp/parameter shown before application |
| 7 | Open Auto/Assist/Manual picker | Current mode labelled; selecting it alone changes nothing |
| 8 | Status/menu; access Sends, Routing, FX, REC, PA, Show and setup | Alert colour supplements the current action label |

In a menu, pads choose labelled items and provide Back/Next; in a confirmation
panel, they show Confirm/Cancel. No global mode cycling or mix recall from an
unlabelled tap. Channel selection while a dialog is open cancels or explicitly
retargets it; never apply a preview to a newly selected channel by accident.

The simulator originally proposed mirroring both banks. Selected Channel now
uses 9–16 for the less-used actions listed above. These are eight physical pads
with two logical layers, not extra physical pads or a different user memory. No dependency on a specific User2 preset is established.
Mapping both logical input banks and transient device gestures requires capture
and acceptance. The simulator currently models one fixture input bank; it can
encode feedback for both banks. P6/P7 in the simulator show guidance; the explicit
`release`/`mode` CLI commands confirm them.

SHR LUX and SHR DAW document the mkII transient pad message:

```text
F0 00 20 6B 7F 42 02 00 10 PP CC F7
PP = 70..7F; CC = 00 off, 01 red, 04 green, 05 yellow,
                 10 blue, 11 purple, 14 cyan, 7F white
```

This is a discrete palette, not arbitrary RGB. Source: the device-specific
[firsthand protocol notes](https://github.com/mhugo/sysex#arturia-minilab-mk-ii),
cross-checked against `../shr-lux/docs/notes/0018-analysis-and-pad-preview.md` and
`../shr-daw/docs/CONTROLLER_LED_FEEDBACK.md`. The Arturia manual does not specify
this packet. The implementation constructs these bytes and sends them only through the explicit
injected transport; no operating-system MIDI output driver is installed.

Desk must be the sole owner of the physical controller while mixing. SHR LUX
must not run a pad light show simultaneously; show state can inform Desk's
feedback through an adapter. The implemented `FeedbackWorker` owns the injected `Output` transport on a separate thread, with a latest-state mailbox,
changed-pad coalescing and at most 20 refresh batches/s.
LED failure does not block input. Feedback follows confirmed state, never raw
input echoes. No hardware memory, mapping, firmware or backlight writes are
required. Transient pad colour restoration needs a documented device policy.

## Pickup, scaling and keyboard fallback

Absolute CC controls use pickup: after bank/page/selection/reconnect or external
value change, ignore motion until it approaches or crosses the applied value.
Show an arrow and target. Relative modes are explicit per profile; the library
supports two's complement, binary offset and signed-bit decoding with bounded
eight-step deltas. The simulator uses absolute values only and rearms
conservatively after each accepted value change. The production relative profile uses the tested `controller_flow` gesture model
above, with no acceleration and 0.1 dB fine gain. Absolute pickup integration and
physical fader feel remain pending.
The current linear dB mapping is a simulation convention, not an accepted taper.

Planned keyboard map: F1 Mix, F2 Channel, F3 Sends, F4 Routing, F5 Automation,
F6 Analysis, F7 FX, F8 Recorder, F9 PA, F10 Show; arrows/Tab navigate,
Enter opens/confirms, Esc returns/cancels, +/- edit, M mutes, H holds,
R opens Release, A opens mode picker. Text entry owns keys while focused;
the letter shortcuts must not fire when naming a channel. PFL uses an explicit
momentary action with release-on-focus-loss. The existing line simulator is not
yet this keyboard workflow or the proposed recovery TUI.

The Frontend's explicit `attach_controller_feedback` boundary installs that
worker; its ordinary pump publishes the current action projection and repaints
after generation changes. Detachment/drop never waits for endpoint I/O. Transport
implementations must bound send time for prompt thread shutdown. A blocked-output
regression proves that pending colours coalesce and publishing/drop remain usable.
`controller_feedback_failed` exposes output failure independently of input.
There is still no OS MIDI endpoint acquisition or qualified hardware binding.

The pure pitch-speed model now integrates the prior velocity up to each event,
so returning to centre preserves motion accrued between pump calls. It uses
actual elapsed time without the former 100 ms truncation; stale input, stale
application (>250 ms), generation change and lost permission discard pending
motion. The 8% dead zone and ±0.5 dB/s maximum are unchanged. **This model remains
unconnected to a provider Master target.**
