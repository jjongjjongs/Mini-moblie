# Gap map

What this runtime does not implement yet, and what the reference emulator says
about whether it matters.

Two sources feed it. The first is our own unimplemented surface, which is
certain: a stub is a stub whatever anyone else does. The second is the
reference emulator (WIPI-X 0.1.7), which is evidence about which of those gaps
real titles actually reach.

Keep it honest: a row here is either measured or marked as not.

## What the reference is, and what it is not

The reference ships a build manifest (`assets/WIPI-X-BUILD-INFO.json`) listing
431 source paths and 105 patches with ids like `wipix-lgt-util-ntohs`. That
list is its **bug history**, not a specification. Three things follow:

- It is not a backlog. A patch id says a symptom class existed in their
  implementation, not that the same symptom exists here. We have fixed several
  of them independently - `wipix-lgt-util-ntohs` is `wie_wipi_c/src/api/util.rs`,
  sign extension and all, arrived at from 붉은보석's billing reply.
- Ten of the 105 are their architecture only: their frontend, the ebiten engine
  they embed, their Android audio and input paths.
- The rest are worth reading *when a title misbehaves*, as a list of places to
  look. Working from the list instead of from a symptom is how a week gets
  spent on something the reference turns out not to do at all - see the
  collector note below.

Its platform packages map to ours: `internal/ktf` is KTF, `internal/raptor`
plus `internal/wipi` is LGT, `skvm`/`skvmhost` is SK-VM, `cpu/interpreter` is
the ARM core.

## Settled: KTF Java objects are never collected

Not a gap. The reference builds KTF Java objects in guest memory exactly as
this runtime does and has no collector for them: the only `collectGarbage` in
its binary belongs to its SK-VM, there is no free, destroy or reclaim of a KTF
Java object anywhere in it, and its one KTF root-visitor covers strings for
state snapshots. Its LGT-equivalent *does* collect
(`CollectUnusedJavaStrings`, `DestroyRaptorJava`), which is the same split this
runtime has.

So `JavaClassInstance::destroy` freeing nothing for KTF is the design, and a
KTF title's heap growing is what the reference does too.

## Certain gaps: our own unimplemented surface

| area | count | what a call does today |
|---|---|---|
| KTF WIPI-C table slots | 89 | `WieError::Unimplemented` - kills the title |
| `wie_wipi_c` stubs | 17 | logs and answers benignly |
| WIPI-Java stubs | 53 | logs and answers benignly |
| SK-VM / SKT stubs | 20 | logs and answers benignly |

The KTF row is the serious one, because those abort rather than answer.

### KTF: 65 were already implemented, and are now wired

`wie_wipi_c` implemented these and LGT wired them; KTF's table did not, so a
title calling one died on the call. Wiring was mechanical - each aborting slot
already carried the name of the function it should call, e.g.
`gen_stub(3, "MC_netSocketConnect")` beside `net::socket_connect`.

| module | implemented | KTF wired | KTF wires now |
|---|---|---|---|
| net | 35 | 3 | 30 |
| uic | 43 | 10 | 43 |
| util | 6 | 1 | 6 |
| graphics | 44 | 32 | 37 |

Five more graphics calls turned out to be the same case and are wired too:
`MC_grpGetContext`, `MC_grpGetUnicodeStringWidth`, `MC_grpDecodeNextImage`,
`MC_grpFillPolygon`, `MC_grpDrawPolygon`.

Three of the remaining families have since been written and wired:
`MC_knlCreateSharedBuf` and its four companions (`wie_wipi_c/src/api/shared_buf.rs`),
the three `MC_miscGetLedCount`/`SetLed`/`GetLed` calls, which answer that this
handset has no LEDs, and the ten program-control calls below.

**Kernel program control** (`MC_knlExecute`, `MC_knlMExecute`, `MC_knlLoad`,
`MC_knlMLoad`, `MC_knlProgramStop`, `MC_knlGetExecNames`,
`MC_knlGetProgramInfo`, `MC_knlGetParentProgramID`, `MC_knlGetAppManagerID`,
`MC_knlGetAccessLevel`) starts and stops sibling programs, which this runtime
cannot do: one title is loaded and nothing can install or start another. So they
answer the world as it is - one program, id 1, no parent and no application
manager - and say "no such program" to everything else, without touching the
buffers they are handed, since no title we have pins their shapes down.
`MC_knlProgramStop` on the title's own id is the exception that does something:
it is a request to quit, and is honoured like `MC_knlExit`.

`MC_knlGetAccessLevel` is the one where a wrong answer could have a title refuse
itself work, so it reports what the title's own `__adf__` declares in `SLvl`
(`00142F9C` in 투스워즈, `00142F1C` in 드래곤하트) rather than a level nobody
wrote down. The reference does not implement this family at all - no
`knlExecute`, `knlLoad`, `ProgramStop` or `AccessLevel` symbol appears in its KTF
package - which is the measure of how rarely a title reaches it.

That leaves 12 non-OEM names with no implementation:

- **Input method** (`MC_imHandleInput`, `MC_imSetCurrentMode`,
  `MC_imGetCurrentMode`, `MC_imGetSupportModeCount`, `MC_imGetSupportedModes`).
  LGT has all five, but privately and in a shape that will not port as it
  stands: two of them take `(a0, a1, a2, a3)`, which is LGT scaffolding for
  arguments its native was not read closely enough to name. Lifting that into
  KTF would be asserting an ABI nobody has checked.
- **Database** (`MC_dbGetAccessMode`, `MC_dbGetNumberOfRecords`,
  `MC_dbGetRecordSize`, `MC_dbSortRecords`, `MC_dbListDataBase`). Four exist as
  `*_lgt` variants, but those carry LGT native's own answers - a 123-byte name
  limit, `-9`/`-22` error codes - so wiring them to KTF would assert KTF native
  agrees.
- **Graphics** (`MC_grpDrawUnicodeString`, `MC_grpEncodeImage`) - real work.

The rest are OEM extensions (`OEMC_knl*`, `OEMC_grp*`, `MC_mdaUnk*`).

The reference implements the same APIs - its shared WIPI runtime dispatches
`dispatchUIC`, `dispatchNetwork` and `dispatchUtility` by index - so these are
live APIs, not dead table space.

Caveat worth stating: no KTF title we have reaches these slots, so wiring them
is verified by tests, not by a title. KTF's own ABI decides which numeric slot
is which function; the slot labels in the table are what encode that, and they
are what the wiring must follow.

## Reference patches worth reading when a title misbehaves

Not a to-do list. Each names a symptom class the reference had to handle.

KTF: `wipix-ktf-paint-capacity`, `wipix-ktf-stale-card-repaint`,
`wipix-ktf-handset-key-repeat`, `wipix-ktf-java-input-method` (16 sites),
`wipix-ktf-c-text-component` (11), `wipix-ktf-clip-completion-listener`,
`wipix-ktf-local-star-purchase`, `ktf-wipic-put-count`,
`wipix-ktf-input-effect-work-budget`.

LGT Java ABI: `wipix-lgt-java-application-class-layout`,
`wipix-lgt-java-array-type-abi`, `wipix-lgt-java-platform-virtual-slots`,
`wipix-lgt-fixed-platform-vtable-precedence`,
`wipix-lgt-java-superclass-materialization`,
`wipix-raptor-java-string-char-array-slot`,
`wipix-raptor-java-wide-store-word-order`,
`wipix-raptor-java-zero-layout-class`.

LGT graphics: `wipix-lgt-image-alpha-mask` (13 sites),
`wipix-lgt-platform-graphics-heap` (32), `wipix-wipi-lcd-flush-region` (16),
`wipix-lgt-pixel-operation-order`, `wipix-tempest-blend-pixel-operation`.

Audio: `wipix-smaf-mixed-track-streaming`, `wipix-smaf-setup-ram-pcm`,
`wipix-raptor-java-audio-source-mute`.

CPU: `wipix-cpu-native-branch-exchange`, `wipix-cpu-read-only-leaf-traps`,
`wipix-native-tlb-refill-classification`, `wipix-application-cpsr-memory-cache`.

## Not ours

`wipix-native-audio-probe`, `wipix-audio-underrun-rebuffer`,
`wipix-audio-sync-telemetry`, `wipix-audio-sample-cursor-jitter`,
`wipix-guest-vibration-gate`, `wipix-ownership-persistence-context`,
`wipix-native-playback-speed-audio`, `wipix-android-digital-triggers`,
`wipix-input-execution-isolation`, `wipix-android-audio-read-chunks` - their
frontend, their audio backend, the ebiten engine.
