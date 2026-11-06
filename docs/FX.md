# Effects sends and returns

## Four FX auxes

Open **Menu → FX auxes**. Select an input in Mix, then set its send to FX1,
FX2, FX3 or FX4. Each aux has its own effect, a Main-only wet return and an independent wet return
to each configured monitor when the provider supports return routing. Sends follow
channel fader, mute and pan; they start at zero. Monitor buses remain separate,
with their existing pre/post send choices and physical outputs.

The current provider offers digital delay, room, small room, chamber, plate,
hall, tape echo, chorus, warm exciter and bright exciter. Reverb damping adjusts
darkness; these are economical algorithms, not hardware-emulation claims.

Use **Control FX auxes**, probe, then **G / Grant** for the existing FX scope.
Choose an effect directly by name from the semantic menu; no intermediate
effects need to be applied. Use previous/next aux and field; decrease/increase
edits the selected field. The same actions work through the configured controller.
Every sounding edit uses the ordinary complete review and separate confirmation.
Cancelling a review leaves the current sound intact. Scope switching reconnects
read-only and never restores a previous grant automatically.

On the FX aux page, **[ / ]** selects an aux, **A / D** chooses an effect,
**Up / Down** chooses a field, and **− / +** changes its value. Fields are the
selected input's send, effect time, amount, damping/tone, rate/tune, shared wet-processing gain,
BPM, note division and rhythmic modifier, followed by Main-only return and
each configured monitor return. Up/Down traverses the complete configured list;
the visible monitor window follows selection. Zero monitors leaves only Main. **W** probes and **X** selects the FX
scope. Send and host-return levels are linear 0..1, adjusted in 5% steps. Delay time and reverb predelay are
milliseconds. Algorithm-specific ranges/defaults come from the provider:
reverb amount is normalized decay and higher damping is darker; exciter amount
is drive, higher tone is brighter, and tune is Hz. Unavailable parameters refuse.
Chorus base time/depth must also remain within the owner's joint bounds.

Digital delay and tape echo offer free time or BPM with 1/1, 1/2, 1/4, 1/8, 1/16,
1/32 and 1/64 notes, straight/dotted/triplet. Editing milliseconds selects free
time. Divisions outside the provider's 2-second capacity refuse visibly.
These are intentional effect times; the configured Brain return-transport delay
comes on top. Total physical path latency is not measured or compensated here.
No external clock or physical tap controller is acquired by this integration.

**Space / FX aux bypass** stops new excitation and lets the tail drain.
**P / Clear FX aux tail** clears that aux at the confirmed source boundary;
new input can excite it again. Brain controls use a 100ms scheduling lead. Panic can interrupt an unsettled change: an unswapped replacement is
cancelled and readback shows the actual active settings. For sustained wet silence at one destination, set its return to zero. Host
return levels ramp over 240 source frames (5 ms at 48 kHz).
Zeroing every input send to an aux, or using bypass, stops new excitation but
leaves existing tails. Changing
algorithm/parameters uses the owner's bounded fade/swap transition and replaces
old history; it is not a tail-preserving preset morph.

**Shared wet-processing gain** belongs to SHR FX and affects every destination.
**Main-only return** changes only Main; setting it to zero leaves enabled monitor
returns audible. Monitor returns start at zero and use the same stereo effect,
folded to mono as `(L + R) / 2`. Opposite-polarity wet channels can cancel.
Bypass and clear-tail act on that one shared owner, not separate monitor effects.
Editing an effect, input send, tempo or another return preserves untouched routes.

**W** opts into independent return readback. A failed read remains an error and
does not establish legacy support. On an older provider, explicitly reconnect
read-only if the connection closed, then choose **L / Read legacy FX**. This
requests the original palette/send contract and leaves independent routing clearly
unavailable. Grant still requires explicit action. W can probe support again;
no mutation is retried to discover capabilities.

The page shows confirmed readback, pending requests, refusals, UNKNOWN outcomes,
staleness, source running/disarmed state and owner faults. A running source is
software state, not proof of physical output. Last-confirmed values remain visible
when stale. The complete review lists Main and every monitor target.
A prepared/accepted edit is not completion: Desk waits for actual settled owner
readback. Unknown outcomes require explicit probe and a new review; no replay.
Four-aux capability requires the configured Brain palette owner (or the explicit
local source-host variant).
It is unavailable on older providers; Desk never substitutes simulator state.
The normal four-aux path sends four stereo buses to Brain and returns their wet
results to independent Main/monitor host routing. The legacy Brain stereo delay remains a separate compatible path.
UI reconnect retains the sounding configuration. Process restart or a new Brain
owner session resets sends to zero; the separate saved FX workflow below can retain intent. Source recovery preserves send and return
intent while disarmed and requires explicit output rearm. A new Brain owner resets
Main to unity and monitor returns to zero. Owner/source replacement invalidates
stale reviews; read fresh state and explicitly review again. Physical aggregate performance and
listening are still unqualified.

## Legacy remote stereo delay

GP21-fx:1 controls one actual Brain owner, ordered `foh-left`, `foh-right`.
GP05 fixed local fallback health keeps its existing read-only meaning. This page
is not a rack or an engine. Desk never implements delay processing.

Use an explicitly authenticated dynamic provider session with `fx_configuration`
permission and scope. **X** explicitly reattaches the existing worker to the FX
scope read-only; a new probe and grant are required. **W** opens and explicitly probes the owner. Attachment
alone sends no GP21 probe and grants no writer. **G** requests the ordinary shared
scope lease; **Q** releases it. Scope switches use the existing session worker.
Old providers and old owner libraries remain unavailable for GP21 writes.

**L/R** selects left/right without changing audio. **E** opens detached exact
numeric entry: `delay_ms feedback damping wet_gain`. Bounds are 1..500 ms,
0..0.85 feedback, 0..0.99 damping coefficient, and 0..1 wet gain. Values must be
finite; fractional values occur only inside the bounded opaque owner JSON. Literal
lowercase text is preserved; command keys normalize before dispatch. **Enter**
requests a complete review, then the ordinary presented-pages/Enter confirmation
applies it. The unselected partner and selected bypass value remain unchanged.

**Space** reviews selected-channel bypass. It settles excitation controls over
20ms and then drains old tails; settlement is not silence. **P** reviews panic
for the selected channel; **B** reviews both. Panic clears history/filter state,
not persistent mute. Every operation shows complete basis, target and partner,
selected mask, ordered binding/lifetime and software lead policy4800 source frames.
**Esc** cancels unsent text/review. A sent mutation is not cancelled or undone by
Esc; producer prepermit authority/session cancellation differs from irrevocable
postpermit unknown. There is no invented operator cancellation wire.

The same AuthorityConnection, Operator, Session, queue, writer/request IDs,
revision and lease own these controls. Reads pair actual-owner and ordinary
snapshots; review and confirmation requery and fence the exact basis. Material
owner state includes session/source/map/capability/instance/library identity,
ordered channels, generation/reset, settled controls and complete target/partner.
Source progress may advance while the reviewed material basis stays identical.
Focus/role/controller/attachment/context loss invalidates intent and review.

Preparing, permitted, applied and settled are separate. Permit advances shared
authorization, not DSP success. A later completion revision may exceed expected+1.
Desk validates original context/ticket/apply frame, exact owner target/generation,
reset and settlement evidence. Panic retains the previous configuration frame;
its reviewed mask is correlated to the producer-validated ticket/apply frame,
reset increment and owner source progress. Public replies do not echo the mask.
Independent samples must verify selected-channel effects.

Unknown/lost-owner/authorization replay outcomes never imply application, even
if historical nested authorization says applied or targets match. No automatic
retry, regrant, replay or reconnect rearm occurs. Further editing is blocked until
an explicit fresh settled owner probe reconciles uncertainty and the operator
starts a new review. Reconciliation reports current actual state; historical
operation outcome remains unknown. Retained evidence is not fresh authorization.

The fixture is pinned to GigPies14504a648ae2494f901b4f25027342ccb1ca614d and
actual SHR FX974ab23753dbe777f5d3401356028b683e4725d0. Normal tests are offline;
the explicitly ignored `fx_actual_frontend` episode requires a separately launched
hash-pinned provider/Brain, explicit readiness, private TLS and independent
streamed wet/PA/raw REC evidence. No physical devices, windows, playback, acoustic
acceptance or hardware clock qualification are implied.

## Saved FX intent

Configure `SHR_DESK_FX_SHOW_DIRECTORY` to an existing owned absolute directory
with mode 0700. This is local file storage configuration, not part of a show.
Open the FX page and use **K / Probe saved FX support** to verify compatibility.
The page offers sixteen slots (`fx-scene-01.json` through `fx-scene-16.json`).
Use **, / .** or the semantic menu to select a slot. The page shows the loaded
slot separately; selecting another slot requires loading it before preview.
**S / Save FX intent** saves
fresh settled configuration for all four auxes, replacing that slot explicitly.
A failed save reports whether replacement completed and durability is uncertain.

**O / Load saved FX intent** reads the selected file without changing the engine
or granting control. **K / Probe saved FX support** is read-only. **V / Preview
and restore saved FX intent** opens the existing paginated review of all current
and saved settings, sends, Main and every monitor return. An incompatible file
opens a complete preview with restore unavailable; no send is remapped and no
legacy defaults are invented. Read every review page;
**Esc / Cancel** leaves the engine unchanged. **Enter / Confirm reviewed change**
is the separate explicit restore action and requires the existing FX grant.

Outputs must already be disarmed or fully muted through the existing reviewed
output controls. Restore preserves that safety state and all human holds. Source
clock recovery, if needed, is separate. Successful restore never rearms outputs;
use the existing separately reviewed **Rearm outputs** action when appropriate.
A prepared request is not confirmation. The page distinguishes saved intent,
review, pending, refused, UNKNOWN and confirmed applied readback. Missing completion
stays UNKNOWN even if values match later; explicitly probe and review again.
Reconnect is read-only and never replays or grants control.

The file saves FX intent only, without tails, runtime authority, safety/arm state,
recording or talkback execution. It neither restores core/PA/light scenes nor
recalls automatically at startup. Exact input identity tuples and logical monitor
IDs must match the same show and approved owner. Physical monitor/device identity
is not represented by the producer's generated logical monitor names.
[Saved contract](CONTROL_CONTRACT.md#saved-four-aux-intent) and
[GP-SHOW](../../gigpies/docs/development/MODULE_IMPLEMENTATION_PLAN.md#gp-show--audio-scenes-and-operator-recovery)
define compatibility, validation and remaining scene work.
