# PA measurement review

GigPies owns bounded capture on a shared source timeline; SHR PA owns measurement
and alignment proposals. Desk presents results and requests transactions. It never
opens a microphone, copies an analysis algorithm or constructs a correction.
The first integration is software-only and requires an explicitly configured
provider. Old providers remain usable without the explicit measurement probe.

Native Y opens/probes the measurement page. The page reports admitted reference
input IDs, reserved microphone capture slots, common source/map/graph identity,
progress and result IDs. Measurement output indices and capture slots are zero
based and are separate from logical input IDs. A reserved microphone cannot join
the program or monitor mix through this workflow.

C opens capture entry: capture ID, logical reference input, reserved mic slot,
PA output index, position ID, sample count, maximum arrival search in samples,
and low/high analysis frequency in Hz. Valid captures contain32768..65536 samples
at48kHz; arrival search is1..2048 samples and frequency bounds20..20000Hz.
Enter prepares the exact review; every material page must be presented before
another Enter confirms. X explicitly requests capture cancellation. R opens a
result ID; every spectral bin and uncertainty/refusal remain scrollable.

P opens proposal entry: proposal ID followed by anchor/target capture ID pairs,
up to eight positions. Capture IDs designate already completed owner results;
they do not select a live microphone or start playback. The owner may return
proposed, no_change or refused. No-change/refusal never creates a PA transaction.

A selects a proposal for the existing muted whole-configuration review. Desk
checks its exact current source/map/graph/configuration/program-bus basis and
verifies that the supplied candidate changes only the owner-declared delay/polarity
fields or explicitly proposed output PEQ slots. Outputs must already be muted/quiesced. The full configuration
is reviewed and applied with GP14; rearm remains an independent explicit action.
A successful capture, proposal or ACK is not an acoustic alignment claim.

Focus/resize/role loss and changed authority invalidate reviews and queued intent.
Detached text and useful result descriptions remain available, but require fresh
observations and a new review. Reconnect is read-only and never replays capture
or applies an old proposal. Synthetic/null acceptance is distinct from future
separately authorized microphone, listening and hardware-output sessions.

The same bounded editor is available without graphics:

```text
shr-desk --measurement-local ENDPOINT SHOW EPOCH WRITER pa_configuration --script FILE
measurement probe
measurement capture ID REFERENCE MIC_SLOT OUTPUT POSITION SAMPLES MAX_LAG LOW_HZ HIGH_HZ
confirm
measurement result ID
measurement propose PROPOSAL ANCHOR TARGET [ANCHOR TARGET ...]
confirm
measurement apply PROPOSAL
confirm
```

Grant and input-release remain explicit script commands. `cancel` discards an
unsent review; `measurement cancel ID` prepares cancellation of an active job and
requires confirmation. `Y`/`measurement probe` explicitly refreshes progress and
results. Repeated requests do not restart captures. The page's stored descriptions
are observations, not a continuously renewed permission to write.

## Distance, crossover overlap and EQ review

In the detached F9 PA editor, delay fields accept `1m`, `34.3cm`, `1ms` or bare
milliseconds. Only the owner's `delay_ms` is sent. Display includes requested and
sample-rounded milliseconds, samples, metres and centimetres at 343 m/s. This is
added acoustic path equivalent, not a measured distance. Input/output meanings
and 100/10 ms limits remain unchanged. Owner validation remains authoritative.

The P proposal editor also accepts `overlap ID ANCHOR TARGET [ANCHOR TARGET ...]`
and `eq ID flat|bass_shaped|damped_high_frequency CAPTURE [CAPTURE ...]`.
These explicitly send GP20 request version 2. Original P entries keep version 1.
Old providers may refuse v2; no automatic fallback or application occurs.

Overlap reviews show every predicted combined-response bin separately from the
measured transfer, selected analysis band and proposed sample/distance/polarity
changes. EQ reviews show the explicit target, each proposed bell, predicted RMS
error and required extra headroom. They use the same A → muted full-configuration
review → explicit confirm path. Rearm remains separate; no-change/refusal applies
nothing. Current capture/configuration identity, cancellation and freshness checks
still apply. Algorithms and numerical definitions belong to
[SHR PA offline setup](../../shr-pa/docs/OFFLINE_SETUP.md).

Calibration and repeated-sweep extraction are currently offline owner operations.
Their provenance-bearing documents can be inspected off-device; GP20 does not
accept arbitrary imported files as authenticated capture evidence. The PA owner
can produce a detached EQ configuration for explicit F9 import/review; such an
import is a manual configuration transaction, not a GP20 measurement acceptance.

[Software validation record](archive/pa-setup-2026-10-29.md).
