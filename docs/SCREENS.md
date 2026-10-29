# Screens and operating workflows

Design contract. Current tasks/progress live in [the owning plan](GIGPIES_IMPLEMENTATION.md);
shared integration tasks live in GigPies. Simulator drawings are illustrative.
The provider frontend renders actual supported controls and keeps unavailable
fields explicit. Generate simulator drafts only with the documented gallery command;
a draft is not evidence that its intended workflow is implemented.

## Full-HD frame

The reviewed first draft uses 1920×1080 with 12×24 Terminus glyphs:

- 48 px persistent header: product/page, FOH or monitor scope, simulation/live,
  automation mode, connection freshness, recorder state and selected channel.
- 888 px body: Mix uses twelve 96 px strip slots, a 192 px main area and a 552 px
  detail area, including gutters. Detail pages keep selection stable.
- 144 px footer: two rotary rows/assignment context, eight pad actions, latest
  result/alert. This intentionally expands yesterday's 96 px footer to make the
  controller visible; it leaves 37 rather than 39 body rows.

The draft shows both actual rows of eight with labels and values; native input
must add pickup arrows and confirmed gesture context. No
hidden reassignment during a held gesture. Twelve strips are a bank size, never
an engine channel limit. The 36-channel fixture exercises three octaves only.

```text
SHR DESK | MIX / FOH | AUTO | LIVE/STALE | REC | SELECTED CH
12 input strips                   | LR/masters | selected channel
identity / ownership              | bus scope  | applied vs proposed
meters / GR / pan                 | monitor   | processing / sends
fader / mute / PFL                | alerts    | reason / bounds
K01..K08 assignments and values / K09..K16 assignments and values
P1..P8 labels + active state / bank + octave / pending/rejected result
```

Charcoal, pale text, cyan selection, amber holds/pending, red faults/mute and
restrained source colours. Selection outlines, HOLD/MUTED labels and stale marks
carry meaning without colour. Avoid decorative motion, dial pictures and repeated
miniature graphs on every strip. Numeric units and target identity stay visible.

## Screen map and provider boundaries

This table describes intended operator workflows and supported provider seams,
not implementation task state. Unsupported controls stay visibly unavailable.
Physical operation follows separate acceptance. Add work to its owning task card.

| Page | Intended operator workflow | Provider boundary |
|---|---|---|
| Mix | Select/bank, fader/pan/mute, holds, levels and PFL/AFL | Real dynamic mixer controls and Brain audition exist; actual level telemetry is GP-METER |
| Channel | Source identity, four-band EQ/compression, sends and precise feedback | Real prepared processing/control/readback; GR is detector feedback, not a signal-level meter; trim/HPF/gate need advertised support |
| Sends / monitors | Explicit destination, sends, bus/listen context and stable return to FOH | Configurable monitor scopes exist; established sends are raw post-mute, not arbitrary pre/post tap switching |
| Routing | Physical/logical identities, detached draft, review and confirmed apply | Actual dynamic topology, output patch and PA configuration controls; no arbitrary patchbay claim |
| Automation | Mode, scope, proposals/reasons/bounds, human hold and deliberate release | Authority/proposals/holds exist; live musical proposal generation is GP-AUTO |
| Analysis | Measured levels first; later spectrum/spectrogram/stereo analysis | Schematic simulator plots are not measurements; GP-METER excludes broader FFT/analysis work |
| FX | Send/return context, rack/effect details, bypass/mute and readback | Actual fixed-delay health; writable prepared rack depends on owner FX-02/03 and shared GP-FX |
| Recorder | Readiness, start/stop, accepted/written frames, gaps and finalized outcome | Actual host/raw REC lifecycle and health; Recorder (J) offers reviewed Start/Stop; durability stays unknown |
| PA | Protection/fault summary, reviewed configuration and output routing | Configurable owner v2 is integrated; setup-mic/phase/alignment remain PA-owned work |
| Brain audio | Duplex mapping/review/arm, PFL/AFL/listen and held talkback | Actual GigPies Brain contracts; Desk opens no PCM; physical qualification remains separate |
| Show / recovery | Freshness, scopes, connections and reviewed scene recall with safes | Authenticated/recovery foundations exist; complete audio scene workflow is GP-SHOW |
| Doctor / lights | Evidence and proposals; lighting stays independent | Lightdesk/Lux own real lighting behavior; no second lighting authority in Desk |
| Controller setup | Identity, profiles/encoding, pickup and exclusive LEDs | Decoder/role/software primitives exist; actual learned devices/LED operation need qualification |

Source/preamp gain is not channel trim; one physical source may feed several
channels. Phantom power is an explicit source operation and never a fast Mix knob.
EQ drawing must reflect the actual engine response, not an invented curve. The
first Channel draft labels its curve schematic for that reason.

## Core workflows

**Manual mix:** choose MANUAL with a visible scoped preview; preserve existing
levels. Press a key to select a channel. Adjust the bank or selected fader; use
Channel for EQ/dynamics. Select monitor destination before changing sends. Exit
to FOH without changing audio. PA protection remains active.

**Override Auto:** key selects lead vocal; move its level; display pending then
applied gain and HUMAN HOLD. New automix proposals remain visible without moving
the held fader. Open Release, inspect target/delta/ramp, confirm that channel and
parameter. Reject the commit if context/revision changed. No automatic timeout.

**Prepare a monitor:** open Sends; choose the performer/bus; persistent header
changes to that scope. Bank rotaries now control sends, not FOH gains. Every strip
shows bus name and tap. PFL auditions the proper bus only when monitor routing is
supported. Back always restores the previous FOH selection and assignments.

**Recover a link:** retain last applied values as stale, disable edits, show last
received time and epoch. Do not replace stale meters with reassuring zeroes.
On reconnect obtain capabilities, snapshot and write grant; drop the old pending
queue. Renderer's restart never loads a show. An ACK lost during a disconnect may
mean the engine already applied a change: the fresh snapshot settles that state.

**Recall a scene:** browse without recall, preview changed targets and exclusions,
preserve recorder/protection/phantom safes by policy, then commit one engine
transaction. Scene undo also requires validation against current state.

## Useful later, unnecessary now

Useful P1: DCA/mute-group spill, custom user banks, A/B setting comparisons with
clear scope, virtual soundcheck from authorized recordings, local performer access,
short health history, larger text mode, operator-specific layouts and scene safes.

Defer P2: theatre actor/understudy tracking, per-send Nodal-style processing,
immersive panning, broadcast mix-minus/automated mic ownership, hundreds of channels,
large plugin markets, motorized surface replication, arbitrary macros and scripting,
DAW timeline/clip launch, synthesizer editing and a commercial-console skin library.
They add engine and training cost without being necessary for the small-band goal.

Do not duplicate every reference-console feature. The distinguishing screen is
Automation: humans must see what the automixer wants, what it changed, what they
own, and how to take over. A compact manual desk and understandable overrides take
priority over dense racks of processors.

[Historical screen assessment](archive/tracking-before-2026-10-09/SCREENS.md) preserves the original missing-dependency table.
