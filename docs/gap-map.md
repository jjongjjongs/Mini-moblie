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
| KTF WIPI-C table slots | 77 | `WieError::Unimplemented` - kills the title |
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

**Input method** (`MC_imHandleInput`, `MC_imSetCurrentMode`,
`MC_imGetCurrentMode`, `MC_imGetSupportModeCount`, `MC_imGetSupportedModes`) is
now shared in `wie_wipi_c/src/api/im.rs` and wired into both platforms. An
earlier note here said LGT's versions would not port because two of them take
`(a0, a1, a2, a3)` - that was wrong, and reading them settled it: those four
words are LGT's dispatch scaffolding and native reads none of them, so the calls
are nullary, which is a shape, not an unknown. The other three were already
pinned down - `MC_imHandleInput` takes `(key, event, output0, output0_len,
output1, output1_len)` - and LGT native implements the standard calls rather
than anything of its own.

The four modes (`EN/S`, `EN/L`, `N123`, `KO`) are confirmed twice over: the
reference's string table carries exactly `EN/S\0EN/L\0N123\0KO\0` beside its
`setCurrentMode(I)Z`, and the shared UIC text component KTF already runs cycles
modes modulo 4.

**Database** (`MC_dbGetAccessMode`, `MC_dbGetNumberOfRecords`,
`MC_dbGetRecordSize`, `MC_dbSortRecords`, `MC_dbListDataBase`) has `*_lgt`
variants, but they read LGT's own `.idx`-equivalent metadata, which a KTF
database does not have - they would answer -1 for every KTF handle. So these are
`*_ktf` variants written against KTF's own model, which is one record read and
written as a byte stream: the record count is the one `MC_dbListRecords` would
list, the record size is how many bytes that stream holds, and sorting one record
succeeds without a comparator because one record is already sorted.
`MC_dbGetAccessMode` takes either an open handle or a name - both arrive as one
word, and a handle is recognised by the magic this runtime writes at the front of
it, the same trick KTF's slot 6 already needed - so neither reading of the ABI
has to be guessed at.

**Graphics** (`MC_grpDrawUnicodeString`, `MC_grpEncodeImage`) was the real work
of the four groups. The first is `MC_grpDrawString` reading UCS-2 instead of
EUC-KR; both now share one drawing path, and `MC_grpGetUnicodeStringWidth` was
already there to measure with.

`MC_grpEncodeImage`'s contract came out of the reference emulator's
`ktf.ktfWIPICGraphicsEncodeImage`, disassembled: six arguments
`(src, x, y, w, h, out_len)`; `*out_len` cleared before anything else and written
again only on success; `x`/`y` not negative, `w`/`h` positive, and `x + w` /
`y + h` inside the framebuffer; `image/bmp` as the encoding; empty or past 32 MiB
refused; and on success a freshly allocated guest buffer whose address is the
return value. The BMP is 24-bit bottom-up BGR by the same rules as
`org.kwis.msp.lcdui.Graphics.encodeImage`, which was derived from the same native
encoder - so a title that saves a screenshot through either door gets the same
file.

What is left in KTF's table is OEM extensions (`OEMC_knl*`, `OEMC_grp*`) and
slots whose names nobody has recovered - `MC_knlReserved2..13`, `MC_dbUnk13..15`,
`MC_mdaUnk*`. Every standard call in it that has a name is answered; the rest
cannot be written until a title reaches one and says what it wanted.

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

## Measured against another implementation's source

`wfeature` (MIT) is a Go runtime covering the same three platforms, and unlike
the binaries above it ships its source, its tests, and ~8,000 lines of written
findings. Everything below is a comparison that was run, not a reading.

**Its KTF LWC is mostly stubs** - `Component.getWidth`/`getHeight` answer zero,
`ContainerComponent.layout`/`validate` do nothing - because it routes text entry
to the host's own keyboard instead of drawing a widget. Ours is the fuller one.
It is the better reference for the platform *outside* the toolkit.

| compared | result |
|---|---|
| KTF class surface | no gap. Its `apiscan` reads the client image's name pool; ported and run over our five archives, each names 43-65 platform classes and the only one unpublished here is `PluginJlet`, which every archive names and none extends. |
| WIPI Java surface | no gap. All 36 reference classes are published and every reference method resolves. `Clip.setBuffer` was moved to `BaseClip` and made return `Z` independently on both sides. |
| C runtime hooks | no gap. memcpy/memset/strcpy/strlen are recognised in the image and replaced with native stubs on both sides; ours is `wie_core_arm/src/binary_patches`, applied over the whole KTF image. |
| a container answering zero children | not ours. Their adds and reads kept two types in one field; ours reads and writes `children`/`childCount` one way. |
| a card forwarding a key to its own text field | not ours. `TextComponent.keyNotify` already runs the key through the input method. |
| `Display` capability answers | **gap, fixed** - `isColor` and `numColors` were placeholders answering `false` and `0`. |
| multi-tap commit delay | **gap, fixed** - we had none, so the same letter twice could not be typed. |
| a frame loop asking for no frame period | **measured, not applicable** - see below. |

### A floor on the wait after a frame, and why it is not here

A title can put its whole frame loop on a guest thread - repaint,
serviceRepaints, sleep, round again - and ask for a sleep of nothing. Answered
literally, the loop redraws as fast as the executor allows and every redraw but
the last is thrown away. Their fix marks the thread that published a frame and
raises only that thread's next wait to a frame period; one of their archives was
publishing 4,807 frames for each one collected, and the fix took sixty rounds
from 2m 6.6s to 1.2s.

The mechanism is real and we have no equivalent. It was written, measured and
removed again, because on this corpus it changes nothing. Instrumenting
`System::sleep` over Bigi 미궁 - our slowest title at ~10ms a tick, and the only
one that drives its own frame loop - counted 232 frames published and 10,183
sleeps in 2,000 ticks:

| wait | follows a frame | count |
|---|---|---|
| 1ms | no | 8,908 |
| 16ms | no | 822 |
| 80ms | **yes** | 182 |
| 16ms | **yes** | 50 |

Every wait that follows a frame already asks for 16ms or more, so the floor
never fires; the waits that dominate the run do not follow a frame, and their
rule deliberately leaves those alone - a loader sleeping between chunks has
drawn nothing, and flooring it would cost it a frame per chunk. Adding the floor
would have put a lock on a path taken ten thousand times per two thousand ticks
to change nothing.

**What the same measurement did turn up** is that Bigi 미궁 asks for `sleep(1)`
8,908 times in 2,000 ticks - four and a half times a tick, from a task that
never draws. That turned out to be ours rather than the title's: 8,893 of them
came from the clip-completion watcher, which read a flag every millisecond that
the audio layer's own watcher only writes every fifty. Reading it at a frame
instead takes the same window from 8,893 polls to 195.

### A line pitch given in bytes, and two halves that read it differently

`Graphics.setRGBPixels(x, y, w, h, int[] pixels, offset, bpl)` names its last
argument as the bytes one line of the picture needs - "한 줄의 이미지가 저장되기
위해서 필요한 바이트 수". MIDP's `drawRGB` names its equivalent in array
elements. Four bytes to a pixel, so the two differ by a factor of four, and this
half handed the byte count straight to the element one. `MC_grpSetRGBPixels`
beside it has always read the argument as bytes, so our own two halves disagreed
about the unit - which is the kind of contradiction that settles which one is
wrong without needing a handset.

Under it, `drawRGB` ignored its scanlength entirely and read `width * height`
elements as one run, so the mismatch never showed as the range error it should
have: a caller whose rows sit further apart than they are wide simply had its
padding drawn as pixels. Both are fixed, each with a test that fails on the old
code - the element read throws `ArrayIndexOutOfBoundsException` on an
eight-element array, and the ignored scanlength draws black where blue belongs.

Two other findings from the same round are **not ours**. A per-dimension surface
cap of 2048 - which refuses a 2464x32 sprite strip while allowing 2048x2048, a
picture forty times the size - does not exist here; the bound here is the byte
budget, which is the one that means anything. And the guest blitting an image it
has already destroyed, where the handle decodes as whatever the arena reissued
the span to, has no symptom on this corpus; noted rather than chased, and the
reference's answer if one turns up is to draw nothing for an address
`MC_knlFree` handed back and keep failing for a handle nothing ever issued.

### The standard library, and why four titles is not the size of a defect

A hand-played sweep of their corpus reported four titles stopping on a class
library member the link could not find: `Boolean`'s constructor,
`StringBuffer.append(char[],int,int)`, `String.replace(char,char)` and
`ByteArrayInputStream`'s protected `buf`. Their point generalises past the four -
the standard library is what every title links against, so a member missing from
it is missing for all of them, and which four stopped is which four happened to
be played far enough.

**All four are present and declared here.** Three of theirs had working bodies
and no declaration naming them - reachable by native dispatch and invisible to
compiled code - and eleven were in that state. That state cannot arise in this
runtime: `JavaMethodProto::new` takes the name, the descriptor and the body
together, so a body without a declaration is not expressible. The field is
declared too, and `Boolean.TRUE`, `Boolean.FALSE` and `new Boolean(true)` all
resolve here, so the class-registration re-entrancy their boxed flag needed - a
static initializer instantiating the class being built - is not ours either.

**A whole-corpus boot sweep says no title dies on a member.** All 148 local
uploads were run at 400 ticks, KTF first and LGT for what KTF would not load: 78
boot and draw a frame, none reports an unresolved method or field, and none stops
or fails to load. Of the rest, 61 are not game archives at all - logs, source
bundles, Android platform-tools - and one is real and neither platform's:
액션퍼즐패밀리 by 컴투스, a MIDP-1.0 MIDlet with no WIPI class in it, which the
two WIPI probes correctly refuse.

Two caveats on that number, both theirs and both worth keeping. **A boot sweep
presses nothing**, so it cannot reach a member a title only names a dozen key
presses in - which is exactly where their four were found. And the MIDlet above
is a gap of a different kind: `wie_j2me` exists and has no headless probe, so
that path has no corpus evidence behind it at all.

### A throw that only the innermost frame could catch

Their thirteenth round is about what happens to an exception nothing catches,
and answering it for this runtime turned up something larger than the question.

KTF's AOT scheme gives every protected call a handler record on the guest stack,
linked to the record of the frame it is nested inside through `ptr_old_handler`.
**Nothing here read that field.** `handle_exception` took the head of the chain,
searched that one method's table, and ended the run if no entry matched - so a
throw that no `try` in the innermost frame covered could never reach the `try` in
the frame that called it. Every nested try in the corpus was one frame deep by
accident of what has been played.

The search now walks outward, and the record that catches becomes the innermost
one, because the frames it unwound past are gone and the next throw must not
search them. A cycle, an unaligned record and a chain past 256 deep are each
named rather than followed. Nothing changes for a throw the head already catches
- the loop finds it on its first pass, exactly as before - so the only runs this
can move are ones that used to die.

Their per-match writes are not ours to make: they write the record's label, the
caught object and the chain head themselves, where this runtime hands
`context_base` and `target` to the guest's own restore function out of
`ptr_functions`, which does the rest. The head is the one of the three that has
to be written from outside, and only because an outer match pops records that a
head-only search never had to.

**Two of their conclusions we already match.** The range test is half-open here
too, and their disassembly is what says that is right rather than an off-by-one:
every entry's target is the first label past its own range, so a throw carrying
`to` is a throw from after the try, and an inclusive bound would run a catch for
an exception raised outside it. And a failed search now reports the whole chain
it looked at - each record's method, the label it carried, and every entry's
range and target - because "no handler" means either the title has no catch for
this or it has one this platform did not match, and only the chain tells those
apart.

**What is deliberately not ported is the absorption.** They let an uncaught
exception end the callback rather than the session, on the grounds that a host
callback is less than a thread and the title's own `try` is on its own thread.
The argument is sound and their guards are careful - only a guest exception, and
counted rather than swallowed. It is held here because every defect this document
records was found by a title dying loudly: the line-pitch bug two sections up
surfaces as `ArrayIndexOutOfBoundsException`, which absorption would have turned
into a picture that quietly did not draw. It goes in when a local title needs it,
not before.

### A freed block this runtime leaves alone, and a handle it used to trust

Their eleventh round is five titles that died reading an address nothing had
computed, and the two worth the most were reading the arena's own
use-after-free fill. A title had freed a structure, kept a pointer into it, read
the pointer back out of the freed block and followed it - which works on a
handset and in a release build, because both leave a freed block's contents
alone, and failed only under the debug fill. One of the two runs a two-and-a-half
thousand tick session with the detector recording instead of filling, and stops
after 160 ticks with the fill.

**Not ours.** `ListAllocator::free` reads the canary and writes the header;
`BucketAllocator::free` clears a bit in a bitmap; `MC_knlFree` calls one of those
and nothing else. A freed payload here keeps its bytes, so a title that reads one
back gets what the handset gave it. The same lesson is already written into
`bucket.rs` from the other end: a `debug_assert!` on a double free used to kill a
debug build where release and the handset both carried on, and 창세기전3 is the
title that showed it. The cost of never filling is that a use-after-free cannot
be *detected* here either - their recording detector is a diagnostic we do not
have, and building one is not a thing to do before a title asks for it.

**One of the other three is ours, and it is on the LGT side.** A Clet's drawing
is not bounded by the surface it was given: one of their titles clears several
thousand bytes past the end of the LCD, and what it landed in was the copy of a
resource name `MC_knlGetResourceID` had handed back as an id, sixty bytes past
the end of the screen. The name read back empty, the `MC_knlGetResource` that
followed looked for a resource called `""`, the title took a size out of a buffer
it had not filled, asked for `0x1c1c1c1c` bytes, got the null that refuses, and
stored through it - **reported as a write to address zero inside a timer
callback, seven hundred instructions and one platform call after the cause.**

`get_resource_id` here allocated a copy of the name and returned the pointer;
`get_resource` read the name back from that pointer. Same shape, same exposure.
The id still has to be a pointer to the name - that is what the other WIPI
platform hands out and what a title stores as the resource's identity - so what
changed is that the platform keeps its own record of what each id was issued for
and reads the name from there. An id this run did not issue still reads from
guest memory, and says so.

**That fixes a consequence and not the cause.** Their other answer is to put
pixels in a region of their own, so an overrun lands in space nothing else is
keeping; here the framebuffer and everything else still come out of one arena, so
an overrunning title can still destroy something. What it can no longer destroy
silently is a resource name.

### A record list that counted nothing at all

Their twelfth round is a unit ambiguity in `MC_dbListRecords`: the specification
calls its third argument the size of the buffer over an `M_Int32 *`, which reads
as bytes or as a count of ids, and there is no answer safe under both - a caller
who meant bytes passes four times what a caller who meant entries does, so
serving the count reading for a byte-meaning caller writes past the array as soon
as the database holds more than a quarter of that number. They had read it as
bytes, so a title's array of twelve got three ids and nine untouched words, and
the title went on to index entry three.

**Two of our three list calls already read it as a count**, and that is what
settles it here without needing the specification to be clearer:
`list_records_lgt`, whose bounds come from native's own `CMP`/`BLT`, and
`list_record_info` on the same KTF path, which stops at capacity. The reference's
title agrees from the other side - it reserves `0x30` bytes for twelve ids, hands
the call `12`, and reads entry three.

**The third read it as nothing at all.** KTF's `list_record` ignored the argument
and wrote every id the database held, however small the buffer was - the platform
itself overrunning a guest array. It now refuses a buffer that cannot hold them
all with `M_E_SHORTBUF` and writes nothing, which is the pattern this file already
documents elsewhere, and answers a null buffer or a count of nothing with the
parameter error rather than an empty list. Switching from no check to a count
check can only refuse a caller that would have been overrun, since the count
reading is the more permissive of the two.

**What the round is really about is the distance between a wrong call and the
fault.** Their chain: a list call short by nine entries, a record id read out of
an unwritten stack frame, a select that refused and wrote nothing, a structure
field made of that same frame, an index 78 into a table of fourteen, an empty
resource name, a lookup that answered not-found, and a null the caller
dereferenced - with only the last link in the fault report. An error a title does
not check is not a failure at the call; it is a value invented some distance
later in a register nothing traces back.

**One thing this turned up that is not answered.** `get_record_ids` returns ids in
the repository's own order - the test above saw `[1, 3, 2]` - and a title that
indexes the list by position cares. Nothing here establishes what native's order
was, so the test sorts before comparing and the behaviour is left alone.

### Two ceilings we do not have, and three calls we already serve

**Their fifteenth round is two limits this runtime does not impose.** They cap
the guest workers a round will grant and the steps a Host service call may spend,
and two titles reached both by playing normally: one starts a thread per sound
effect and had 253 started, 189 retired, 64 alive and the sixty-fifth stack
refused, because a round that retired a worker still spent its grant on it; the
other busy-waits inside a timer callback by polling `MC_knlCurrentTime` in a
counted delay loop, and 5.2 seconds of that is 828 million steps against an
allowance of 500 million.

Neither can happen here, for different reasons in each case. There is no step
allowance at all, so nothing charges a guest for waiting. And a thread's stack is
`ThreadState`'s to hold: it takes 1MB from a reuse pool and gives it back in
`Drop`, so a thread that finished returns its stack without anything having to
notice that it finished. The pool's cap bounds how many stacks are kept for
reuse, not how many a title may have.

Their other half does not reach us either. A Host that steps ticks holds its
session clock still for the length of a service call, so a guest busy-waiting on
the clock never sees its wait end - which is why their free renewal is
conditional on the clock having moved. Time here is read from the platform on
each call rather than from a clock the Host holds, so the wait ends in a probe
(a millisecond per read) and on a real frontend (the wall) alike.

**What we lack is the diagnostic, not the ceiling.** The executor yields only at
an SVC that awaits (`core.rs`), so a guest loop that never reaches one freezes
this runtime with nothing said - the same freeze a handset has, since a callback
there has no scheduler to yield to either, but without the three-second failure
their allowance turns it into. Worth having if a local title ever presents one;
not worth inventing a number for before then.

**Their sixteenth round is three WIPI-C calls they did not serve, and we serve
all three.** `MC_knlGetCurProgramID` answers a stable non-zero id - theirs is
derived from the archive's identifier so two archives never collide, ours is the
constant 1, which is the same thing for a runtime that runs one archive at a
time. `MC_fsRename` is served, and a leading separator is already trimmed to one
key by `normalize_guest_path`, so `/lo.dsk` and `lo.dsk` are one file here rather
than the two that lost their title's save.

And `MC_grpPostEvent` has the reader their version was missing. Theirs queued the
message a title sends itself into a queue nothing drained - the guest's own
`getNextEvent` loop reads its own queue, and everything the Host originates goes
straight to the card stack. Here the push lands as `Event::Notify`, the event
queue turns it into `NotifyEvent`, and `Display.handleNotifyEvent` dispatches it,
so a middleware routine compiled to
`MC_grpPostEvent(MC_knlGetCurProgramID(), ...)` gets its own message back.
