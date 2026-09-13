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

### A void call that was writing a register

Their third round is three archives whose guest-memory faults were one defect,
in a function the specification says returns nothing. `MC_knlFree` is declared
`void`, so what it leaves in `r0` is not part of its contract - and that is
exactly why putting a value there is a decision. A middleware shared by three of
their archives ends an initialization step by freeing the buffer it loaded a font
through, with the free in tail position and nothing after it; the caller tests
`r0` for zero and reads zero as failure. Writing zero turned "the font loaded"
into "the font failed", the initializer returned early, and the title's first
`paint` faulted on a word that early return never filled - **six calls and one
early return from the cause.**

Ours answered the pointer it had just freed. That is non-zero, so it escapes that
particular reading, but it is still overwriting a register a void call has no
business touching, and a caller whose own value was live in `r0` across a tail
free would have got the pointer instead. It writes nothing now. The machinery was
already there: `ResultConverter<()>` produces no result words and
`write_return_value` leaves `r0` alone when there are none, so the fix is the
signature. The test asserts the call produces no result word at all, for a real
block and for null.

The neighbouring calls were swept and left alone: `close`, `delete` and `set`
shapes in this API return `M_Int32` status codes that titles do check, so only the
one the specification calls `void` changed.

**Two more of that round are already ours.** `MC_knlGetResource` refuses a handle
with the sign bit set - a title that does not check what `MC_knlGetResourceID`
answered hands the error code back as the handle, and reading `-12` as an address
faults the guest instead of failing the call its own error path is waiting for.
Ours has refused that since before this comparison. The bug they found behind
theirs was Go's `&` and `<<` sharing a precedence level, so `handle&1<<31` parsed
as `(handle&1)<<31` and the guard had never rejected anything; Rust gives `&`
lower precedence than `<<`, so the same expression means what it reads as here.

And their note on what they did **not** do is worth keeping: three of their titles
stop on a reference the JVM has no object for, and binding a synthetic `Object` so
the call proceeds moved all three walls rather than removing them. Fabricating an
object for an arbitrary word is the same wrong-answer-silently shape this document
keeps refusing.

### A catch block that was never handed what it caught

This is the correction to what the exception-chain section above assumed. That
change said the record's label, the caught object and the chain head were the
guest restore function's business here, and the head was the one exception. **The
caught object was not.** It cannot be: the restore function is handed
`context_base` and `target`, and nothing ever tells it what was thrown.

A handler record carries the exception at offset 16, and the record is built on
the guest stack by the try block's prologue - so that word is whatever the frame
underneath had there until the unwind fills it in. Nothing on this side reads it,
which is why nothing on this side noticed, and a catch block reads it every time.
Our field for it was named `unk3`.

The values a title gets are the plausible kind, which is why one cause looked like
several. The reference found it behind five titles at once: a
`catch (e) { close(); throw e; }` rethrowing a Thumb code address, a
`System.err.println(e)` and a `StringBuffer.append(e)` handing that word to a
runtime method as an object reference, and `e.printStackTrace()` and
`e.getMessage()` dispatching through it as an object header and faulting on a wild
address inside the title's own helper - twice on the same instruction, which is
what first suggested one cause rather than five.

The test writes a sentinel at that offset before the throw and asserts the record
carries the exception afterwards; on the old code it reads the sentinel back.

**Two other findings from that round are not ours.** `ByteArrayInputStream` and
`InputStream` both declare `mark`, `reset` and `markSupported` here, so a title
that reads a header, resets, and hands the same bytes to its own decoder gets its
bytes rather than the `IOException` three of their titles caught. And their record
layout has one generation-dependent detail worth knowing: an older module keeps
the label at 16 and the object at 12, the other way round from the current
generation. We read the label at 12 and that has always worked, so the object at
16 is the same generation's answer - a client of the older shape would need both
swapped.

**One structural difference is recorded rather than changed.** Their frame loop
fix needed repaints and `callSerially` Runnables to come off one queue in posting
order. Here a Runnable goes into `callSeriallyEvents` and a repaint drives the
host through `request_redraw`, so the two are not one queue - and their symptom,
two titles running four hundred ticks with no error and no lit pixel, is specific
enough to recognise if a local title ever shows it. Nothing here does, and
rebuilding the scheduling on a symptom we cannot see is how a week goes missing.

### Cards: two rounds of theirs we pass, one difference, and one exposure we have

**Their eighth round found `serviceRepaints` painting a card the display was not
showing.** A title that loads in stages inside `startApp`, calling repaint and
serviceRepaints between stages to move a progress bar and pushing its card only
at the end, had its `paint` entered against a state it had not built, and stopped
in its own compiled null check. Not ours: `Card.serviceRepaints` here goes through
the card's canvas, and the canvas is what the card stack sets on push and clears
on pop and remove - so a card that has never been pushed has no canvas and nothing
is painted. Their fallback-vtable finding is not ours either: `build_vtable` walks
the real class hierarchy from the root down, so a superclass's slot number is
valid on every subclass's table, and `KtfClassLoader::findClass` answers not-found
for a class the guest's own table does not have rather than fabricating a record
that extends Object.

**Their ninth round is `Card.showNotify` never being called.** One of their titles
divided by zero twice a frame because the field it divides by is written by a
method whose first call site is its card's `showNotify`, and their card stack
appended and truncated a slice without telling anyone. Ours calls it in all four
operations - push, pop, remove, removeAll - and a push of a card already in the
stack is a no-op, so nobody is notified twice.

**One difference, recorded rather than changed.** Their shown card is the top of
the pushed stack: `paintTopCard` paints it, `isShown` answers for it, and a change
of top hides one card and shows another. Here every pushed card is shown and
painted, bottom to top, so a dialog pushed over a menu leaves the menu drawn
underneath and still `isShown`. That is internally consistent - canvas, `isShown`
and paint all agree - and the specification line they quote settles only that push
and pop drive `showNotify`, not what a covered card is. Changing it would move
every title with a card stack, so it stays until something says which model a
handset had.

### A paint the guest did not ask for, which we do have

Between those two rounds is the one that reaches us. A title can drive its own
frame loop from Java - `Card.repaint` to ask, `Card.serviceRepaints` to enter
`paint` - and if the Host's own paint keeps running beside it, the title gets two
paints for every frame it asked for. **That is a correctness bug and not just
waste, because a frame loop that lives inside `paint` advances the world once per
entry.** Their scrolling title asked for 583 frames in 1,250 rounds and had `paint`
entered 1,833 times, so its world moved three times per step it took: the same
column of terrain laid down at three offsets, a slope drawn as a sawtooth, and
ground that leaves the bottom of the screen and never comes back. Everything that
looked like the cause - `copyArea`'s argument order, its overlapping snapshot, the
palette transparency, the tile decode, the anchor - was checked and correct.

**We have the same double paint.** Bigi 미궁 enters `paint` 535 times in 4,000
probe ticks: 435 from its own `repaint`/`serviceRepaints` pairs and 100 from the
`Event::Redraw` the probe feeds. And it is not a probe artefact - `wie_cli` turns
winit's `RedrawRequested` into `Event::Redraw` and `wie_android` does the same, so
on a real frontend the Host's paint arrives at the window's refresh rate beside
whatever the title paints for itself.

**Not ported, and the reason is the constant.** Their fix stands the round paint
down for eight rounds after a frame the guest painted and brings it back when the
guest stops - eight because they measured that a title driving its own screen
returns within two or three rounds and at worst seven, while one that has handed
the screen back leaves hundreds or never returns. Counting calls does not separate
those two shapes, and standing down for good on the first call froze three of
their titles, taking flush counts from about 600 in 600 rounds to 3, 4 and 36. So
the number has to be measured against this corpus before it is worth anything
here.

**What to look for.** The signature is a world that advances faster than the title
steps it: repeated terrain at several offsets, a scroll that outruns its own
column, sprites that move in multiples. Our own "Y축 전경 지터 (더블이미지)"
investigation closed against the reference without a shipped change; if that
symptom returns, a Host paint running beside a guest-driven one is the cause to
check first.

### Measuring the stand-down against this corpus, and what the harness cannot say

The section above left the Host-paint stand-down unported because its eight-round
window is a constant measured against another corpus. So it was measured against
this one: `Card.serviceRepaints` and the repaint event were each logged with the
guest clock, over 8,000 probe ticks per archive.

| archive | guest paints | host paints | gap p50 | p99 | max |
|---|---:|---:|---:|---:|---:|
| k1, k3, k5 | 0 | 200 | - | - | - |
| k2 | 2 | 200 | 2 | 2 | 2 |
| k4 (Bigi 미궁) | 259 | 200 | 80 | 87 | 99 |

**Both of their shapes are here, and cleanly apart.** k4 drives its own screen -
259 paints, one every 31 ticks against the probe's host paint every 40 - and k1,
k3 and k5 never call it at all. k2 is the third shape and the important one: it
calls twice, two clock units apart, and then never again, drawing the rest of its
run from a Host paint it does not ask for. **That is exactly the title their naive
version froze.** Standing the Host paint down for good on the first
`serviceRepaints` would take k2 from 200 painted frames to 2. So if this is ever
ported, the expiry is not a refinement, it is the part that keeps a local archive
alive.

**What this harness cannot give is the number.** `CapturePlatform::now()` advances
one millisecond per *call*, so k4's "80" is eighty clock reads between paints
rather than eighty milliseconds of anything, and the probe's own host paint every
40 ticks is a probe constant rather than a refresh rate. The window has to be
long enough that a driving title never loses its Host paint and short enough that
a load screen gets it back, and both halves of that are ratios between a title's
real frame period and a real display's refresh - neither of which exists in a run
where the clock moves because the guest looked at it.

So the measurement settles the shape and not the constant: **the stand-down needs
an expiry, this corpus proves it, and the value needs a run on a real clock.** The
same limit already sits behind the multi-tap commit delay, which the probe also
cannot exercise - making the probe's clock advance with ticks rather than with
reads would unblock both, at the cost of new frame-signature baselines for every
archive.

### The probe's clock now runs on the guest's execution

The measurement above could settle the shape of the Host-paint stand-down and not
its size, because the probe's clock advanced a millisecond per *read*: a title
that polls the clock in a delay loop had time fly, one that never asks had it
stand still, and nothing measured in those milliseconds transferred to a frontend
where the clock is real.

It runs on `EXECUTED_INSTRUCTIONS` now - a counter that was already there, public
and cumulative, so no shipped code changed. Within a tick, time advances only as
the guest executes; at the end of a tick the probe folds that execution into the
base and charges at least a tick's worth, so a moment where every task is asleep
still ends.

**Two attempts got it wrong first, and both are worth writing down.** Taking
whichever of tick-pacing and execution-pacing had got further turns
`Executor::tick`'s bound into "execute until work catches up with the tick count",
which for a title that has been idle is not a bound at all - one archive went from
running 8,000 ticks to not finishing 500. And pacing purely on execution hangs for
a different reason: a task can make progress without executing a guest
instruction, which `yield_now` and a sleep of nothing both do, so the time the
tick is waiting for is time only the guest can buy and nobody is buying it. Clock
reads remain as a floor under the rate, at a thousand to the millisecond rather
than one.

**The rate is 10,000 instructions to the millisecond, not the 100,000 a handset
ARM of the era would give**, and the corpus is what chose it. At 100,000 the
title that drives its own screen reads as 125 frames a second, which no title of
this era was; at 10,000 it reads as 12.5, which is what it looks like. The
difference is that this platform draws in host code, so a frame's real cost is
missing from the guest's instruction count, and the rate is where that has to be
absorbed.

**What it measured.** Over 500 ticks - about four seconds of guest time:

| archive | guest paints | gap p50 | max |
|---|---:|---:|---:|
| k4 (Bigi 미궁) | 50 | 80ms | 81ms |
| k2 | 2 | 22ms | 22ms |
| k1, k3, k5 | 0 | - | - |

k4 drives its own screen at a very steady 12.5 frames a second, and its worst gap
is 81ms - **about five paints of a 60Hz display, so a stand-down of eight covers
it with margin.** That is the reference's own number, arrived at from this corpus
instead of theirs. k2 still says the expiry is mandatory: two paints 22ms apart and
then nothing for the rest of the run.

**New KTF baselines**, since tick counts mean something different now: at 500
ticks k1 draws 13 frames, k2 15, k3 13, k4 63, k5 13, and k1/k3/k5's 13 is exactly
the probe's own every-40-ticks paint. The LGT probe is untouched, so 메이플2007,
디스트로이어 and 엑시온2 keep the signatures this document has been checking
against all along.

### The stand-down, ported with the number this corpus gave

With the probe's clock on the guest's execution the window could be measured, so
the Host paint now stands down for **200ms of guest time** after a frame the guest
painted, and comes back on its own.

**Guest time rather than a count of host paints**, which is what the reference
bounds it by. A count means something different on every frontend: the probe's
host paint arrives every 40 ticks and a 60Hz display's every 16, so eight of them
is a fifth of a second in one place and two and a half seconds in the other. A
duration reads the same in both, and it is the thing the measurement measured -
Bigi 미궁's worst gap between its own paints is 81ms, so 200 is about two and a
half times the longest wait a driving title asks for.

**What it did, at 500 ticks an archive:**

| archive | before | after | |
|---|---:|---:|---|
| k4 (Bigi 미궁) | 63 | **51** | 12 of its 13 host paints gone; its own 50 untouched |
| k2 | 15 | **15** | nothing lost |
| k1, k3, k5 | 13 | 13 | never drive their own screen |

k4 is the point: it was being painted 63 times for the 50 frames it asked for, and
a frame loop inside `paint` advances the world once per entry. k2 is the guard - a
count of eight host paints would have cost it eight frames at the probe's cadence,
and the duration costs it none, because 200ms of guest time expires long before the
next host paint arrives.

The LGT archives are unchanged, signatures included: that platform's titles do not
drive their screen through `Card.serviceRepaints`, so nothing stands down for them.

**No unit test, and the reason is the fixture.** `Canvas` is abstract, so the
guest-paint side cannot be reached without building a concrete subclass and a
display for it, and the suppression side needs the event queue driven by hand. What
the change is really claimed to do is distinguish the two shapes, and the frame
counts above do that in a way a field-write assertion would not.

### The probe now says how much guest time a run covered

The clock change is only useful if a run says what it measured, so the summary
line carries `guest_ms` beside its tick count. Across the local KTF archives at
500 ticks:

| archive | guest_ms | ms a tick |
|---|---:|---:|
| k1 | 4,045 | 8.09 |
| k2 | 4,555 | 9.11 |
| k3 | 4,514 | 9.03 |
| k4 | 4,011 | 8.02 |
| k5 | 4,055 | 8.11 |

Every one of them sits on the eight-millisecond floor with a little more where the
guest executed longer than that, which is the design working. **It also settles the
conversion a script needs**: 900ms of guest time - the input method's commit delay -
is a little over a hundred ticks, so pressing the same key at tick 200 and tick 400
starts a second character while pressing it at 200 and 260 walks the multi-tap ring.

**What that does and does not verify.** The delay's behaviour is pinned by four unit
tests against an explicit clock, and what the probe could not show before was
whether a real run's guest clock moves far enough for the delay to be reachable at
all - on the old clock a millisecond cost a clock read, so a title that never asked
the time never got there. It moves. An end-to-end run that types into a title's own
text field would be the next thing, and it needs a fixture: the one local archive
with a text field is a first-run installer that has to be run twice to reach it, and
reading its text back means reading pixels.

### The third of the three writes, and a heap figure a title could not divide

**The label was ours after all.** The chain-walking section said the record's
label, the caught object and the chain head were the guest restore function's
business and the head was the exception; the caught object turned out not to be,
and neither is the label. Entering a catch block leaves the region that was
protected, and the label is what says which region execution is in - so the record
has to say so before the block runs. Writing the entry's target is the same thing
as saying it, because every entry's target is the first label past its own range.

Left stale, a throw from inside the catch block matches the entry the block belongs
to and jumps back to the block's own first instruction. The reference watched a
title do that four hundred thousand times and report its instruction ceiling.
Theirs also names why this cannot be left to the guest: two of their three titles
write the label themselves at the top of each catch block and the third does not,
and there is nothing wrong with the third - it is the platform's job on the way in.
The test asserts the record's label is the target afterwards; on the old code it is
still the label the throw came from.

**And a heap figure a title could not do arithmetic on.** `MC_knlGetTotalMemory`
and `MC_knlGetFreeMemory` answered 32MiB and 24MiB here. The comment beside them
recorded the lower bound - 1MiB made memory-probing titles read the heap as already
full and refuse to load - and not the upper one: a title works these out in 32-bit
ints, and `free * 100` for a percentage leaves `i32` above about 20.5MiB. The
reference has a title that printed `-28% FREE`, collected, printed `-29% FREE`, and
went round for as long as it was left running. Both of our figures were past that.
They are 16MiB and 12MiB now, which is inside both bounds, and a test multiplies
each by a hundred so the reason travels with the numbers.

**The rest of those two rounds is not ours.** A title that patches a palette into
an encoded PNG and leaves the chunk's old checksum behind is already handled here,
and handled the same careful way - the CRCs are recomputed only after a decode has
already failed, so no picture that decoded before takes a different path. Our own
comment names 액션퍼즐패밀리1 for it and records a worse consequence than theirs: the
null image takes an exception out through the title's key handler, past the line
that releases the handler's lock, and the next key release waits on that lock
forever. `getPixels`/`setPixels` are the device-format pair here too, a byte array
with a byte pitch, which is what makes them different methods from
`getRGBPixels` rather than a spelling of them. And both halves of a 64-bit answer
already come back: a Long or Double return sends two result words, and one-word
returns leave r1 as the callee had it.
