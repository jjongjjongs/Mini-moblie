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
| KTF WIPI-C table slots | 112 | `WieError::Unimplemented` - kills the title |
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

That leaves 112 aborting slots, of which 35 name a non-OEM API we do not
implement anywhere: the shared-buffer and program-control halves of the kernel
(`MC_knlCreateSharedBuf`, `MC_knlExecute`, `MC_knlLoad`, ...), five database
queries, seven graphics calls (`MC_grpDrawPolygon`, `MC_grpDrawUnicodeString`,
`MC_grpEncodeImage`, ...), the five-call input-method family (`MC_imHandleInput`
and friends - LGT has its own), and the three LED calls. The rest are OEM
extensions (`OEMC_knl*`, `OEMC_grp*`, `MC_mdaUnk*`).

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
