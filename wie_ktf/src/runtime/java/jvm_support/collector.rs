//! Giving a KTF title's dead objects' memory back.
//!
//! A KTF object is two blocks of guest memory - an eight-byte header naming
//! its fields and its class, and the fields - and the title's compiled code
//! holds it by the header's address, in registers, on its stack, in the
//! fields of other objects and in memory of its own. None of that is a JVM
//! root, so the JVM's opinion of which objects are dead is no ground to free
//! one on (see `JavaClassInstance::destroy`), and until this nothing freed
//! them at all: a title that builds a few strings a frame filled the heap's
//! eight-byte slots in a few minutes of play and stopped.
//!
//! So this collects them the way the compiled code sees them, conservatively,
//! as the reference emulator's KTF runtime does too:
//!
//! - every word that could be a reference roots what it points into: each
//!   thread's registers and the live part of its stack, every mapped region
//!   outside the heap (the title's image with its data), every live heap block
//!   that is not itself one of these objects (whatever the title and this
//!   runtime allocated for their own use), and everything the JVM still reaches
//!   - its frames, statics, pins and the monitors in use;
//! - a word pointing anywhere inside an object's header or fields counts, since
//!   compiled code walks an array through a pointer to its elements;
//! - each object reached has its fields read the same way, and what nothing
//!   reaches is freed.
//!
//! It runs when the heap runs out - an allocation that fails collects and tries
//! once more - and, before that is reached, after the JVM's own collection once
//! enough has been made since the last one: that comes at the end of a paint,
//! with the event loop between events.

use alloc::{boxed::Box, collections::BTreeSet, vec::Vec};
use core::{
    mem::size_of,
    sync::atomic::{AtomicU32, Ordering},
};

use hashbrown::{HashMap, HashSet};
use spin::Mutex;

use jvm::Jvm;
use wipi_types::ktf::java::JavaClassInstance as RawJavaClassInstance;

use wie_core_arm::{Allocator, ArmCore, HEAP_BASE, HEAP_SIZE};
use wie_util::{ByteRead, ByteWrite, Result, WieError, read_generic};

use super::JavaClassInstance;

/// The header every object has: `{ptr_fields, ptr_class}`.
const HEADER_SIZE: u32 = size_of::<RawJavaClassInstance>() as u32;

/// Objects made since the last collection that make another one due.
const DUE_OBJECTS: u32 = 0x40000;

/// Bytes made since the last collection that make another one due.
const DUE_BYTES: u64 = 0x200_0000;

/// Collect before every this-many objects made, 0 for never - a test's way to
/// have every reference this runtime keeps checked as often as it can be.
pub static STRESS_INTERVAL: AtomicU32 = AtomicU32::new(0);

/// What one collection did.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Collection {
    pub objects: usize,
    pub freed: usize,
    pub freed_bytes: u64,
}

/// The objects this runtime made, kept per core. See the module's notes.
#[derive(Default)]
pub struct KtfHeap {
    state: Mutex<HeapState>,
}

#[derive(Default)]
struct HeapState {
    /// The header address of every object made and not yet freed.
    objects: BTreeSet<u32>,
    /// Set once the JVM is up; nothing is collected before.
    jvm: Option<Jvm>,
    collecting: bool,
    made_objects: u32,
    made_bytes: u64,
    /// Every object made, for [`STRESS_INTERVAL`].
    made_total: u32,
}

impl KtfHeap {
    pub fn of(core: &ArmCore) -> alloc::sync::Arc<Self> {
        core.extension::<Self>()
    }

    /// Starts collecting, with `jvm` as the source of what it still reaches.
    pub fn set_jvm(&self, jvm: Jvm) {
        self.state.lock().jvm = Some(jvm);
    }

    /// Whether the object at `ptr_raw` is one this made and has not freed.
    #[cfg(test)]
    pub fn is_live(&self, ptr_raw: u32) -> bool {
        self.state.lock().objects.contains(&ptr_raw)
    }

    /// Allocates an object's header and its `fields_size` bytes of fields,
    /// collecting and trying once more if the heap is out of room.
    pub fn alloc_object(&self, core: &mut ArmCore, fields_size: u32) -> Result<(u32, u32)> {
        let stress = STRESS_INTERVAL.load(Ordering::Relaxed);
        let made = {
            let mut state = self.state.lock();
            state.made_total = state.made_total.wrapping_add(1);
            state.made_total
        };
        if stress != 0 && made.is_multiple_of(stress) {
            self.collect(core)?;
        }

        let (ptr_raw, ptr_fields) = match Self::alloc_blocks(core, fields_size) {
            Err(WieError::AllocationFailure) => {
                tracing::warn!("KTF heap full: collecting and trying again");
                self.collect(core)?;

                Self::alloc_blocks(core, fields_size)?
            }
            x => x?,
        };

        let mut state = self.state.lock();
        state.objects.insert(ptr_raw);
        state.made_objects += 1;
        state.made_bytes += u64::from(HEADER_SIZE + fields_size);

        Ok((ptr_raw, ptr_fields))
    }

    fn alloc_blocks(core: &mut ArmCore, fields_size: u32) -> Result<(u32, u32)> {
        let ptr_raw = Allocator::alloc(core, HEADER_SIZE)?;
        match Allocator::alloc(core, fields_size) {
            Ok(ptr_fields) => Ok((ptr_raw, ptr_fields)),
            Err(error) => {
                Allocator::free(core, ptr_raw, HEADER_SIZE)?;
                Err(error)
            }
        }
    }

    /// Collects if enough has been made since the last collection.
    pub fn collect_if_due(&self, core: &mut ArmCore) -> Result<()> {
        let due = {
            let state = self.state.lock();
            let live = state.objects.len() as u32;

            state.made_objects >= DUE_OBJECTS.max(live / 2) || state.made_bytes >= DUE_BYTES
        };

        if due {
            self.collect(core)?;
        }

        Ok(())
    }

    /// Frees every object nothing reaches. See the module's notes.
    pub fn collect(&self, core: &mut ArmCore) -> Result<Collection> {
        let (jvm, objects) = {
            let mut state = self.state.lock();
            let Some(jvm) = state.jvm.clone() else {
                return Ok(Collection::default());
            };
            if state.collecting {
                return Ok(Collection::default());
            }
            state.collecting = true;

            (jvm, state.objects.iter().copied().collect::<Vec<_>>())
        };

        let result = Self::collect_objects(core, &jvm, objects);

        let mut state = self.state.lock();
        state.collecting = false;
        let (freed, collection) = result?;
        for ptr_raw in freed {
            state.objects.remove(&ptr_raw);
        }
        state.made_objects = 0;
        state.made_bytes = 0;

        tracing::info!(
            "KTF collection: {} of {} objects freed ({} bytes)",
            collection.freed,
            collection.objects,
            collection.freed_bytes
        );

        Ok(collection)
    }

    fn collect_objects(core: &mut ArmCore, jvm: &Jvm, objects: Vec<u32>) -> Result<(Vec<u32>, Collection)> {
        // What the JVM reaches goes first, before anything is read: it walks
        // the objects through their fields, and a field is guest memory.
        let pinned: HashSet<u32> = jvm.gc_reachable_identities().into_iter().map(|x| x as u32).collect();

        let graph = ObjectGraph::new(core, &objects)?;
        let mut marking = Marking::new(&graph);

        for (index, &ptr_raw) in objects.iter().enumerate() {
            if pinned.contains(&ptr_raw) || graph.fields[index].is_none() {
                marking.mark(index);
            }
        }

        // The threads.
        let stacks = core.gc_stack_roots();
        for &register in &stacks.registers {
            marking.reference(&graph, register);
        }
        for &(low, high) in &stacks.ranges {
            marking.scan_range(core, &graph, low, high)?;
        }

        // The title's image and whatever else is mapped beside the heap.
        for (low, high) in core.gc_data_regions() {
            marking.scan_range(core, &graph, low, high)?;
        }

        // Every heap block that is not an object's - but not the stacks, whose
        // live parts are above and whose rest is stale.
        let stack_blocks: HashSet<u32> = stacks.stack_blocks.iter().copied().collect();
        Allocator::scan_live_blocks(
            core,
            |address| graph.blocks.contains(&address) || stack_blocks.contains(&address),
            |_, bytes| marking.scan(&graph, bytes),
        )?;

        // And on through what those reach.
        while let Some(index) = marking.worklist.pop() {
            if let Some((ptr_fields, size)) = graph.fields[index] {
                let mut bytes = alloc::vec![0; size as usize];
                core.read_bytes(ptr_fields, &mut bytes)?;
                marking.scan(&graph, &bytes);
            }
        }

        let stress = STRESS_INTERVAL.load(Ordering::Relaxed) != 0;
        let mut freed = Vec::new();
        let mut collection = Collection {
            objects: objects.len(),
            ..Default::default()
        };
        for (index, &ptr_raw) in objects.iter().enumerate() {
            if marking.marked[index] {
                continue;
            }
            let Some((ptr_fields, size)) = graph.fields[index] else {
                continue;
            };

            // Out of the JVM's books first: it names the class as it goes,
            // which it reads from the object.
            let _ = jvm.destroy(Box::new(JavaClassInstance::from_raw(ptr_raw, core)));

            if stress {
                // Something still using it reads nonsense rather than whatever
                // gets the memory next, which is easier to tell.
                core.write_bytes(ptr_raw, &[0xee; HEADER_SIZE as usize])?;
                core.write_bytes(ptr_fields, &alloc::vec![0xee; size as usize])?;
            }

            Allocator::free(core, ptr_fields, size)?;
            Allocator::free(core, ptr_raw, HEADER_SIZE)?;

            freed.push(ptr_raw);
            collection.freed += 1;
            collection.freed_bytes += u64::from(HEADER_SIZE + size);
        }

        Ok((freed, collection))
    }
}

/// Where each object's blocks are, and which object an address falls in.
struct ObjectGraph {
    /// Each object's fields block and its size, or `None` where it could not
    /// be read - such an object is kept and not looked into.
    fields: Vec<Option<(u32, u32)>>,
    /// Every block start that belongs to an object, so a heap walk passes it.
    blocks: HashSet<u32>,
    /// Bucket slot start to the object owning it.
    slots: HashMap<u32, usize>,
    /// Fields blocks too big for a bucket: start, end and object, by start.
    large: Vec<(u32, u32, usize)>,
}

impl ObjectGraph {
    fn new(core: &ArmCore, objects: &[u32]) -> Result<Self> {
        let mut graph = Self {
            fields: Vec::with_capacity(objects.len()),
            blocks: HashSet::with_capacity(objects.len() * 2),
            slots: HashMap::with_capacity(objects.len() * 2),
            large: Vec::new(),
        };

        for (index, &ptr_raw) in objects.iter().enumerate() {
            graph.blocks.insert(ptr_raw);
            graph.slots.insert(ptr_raw, index);

            let fields = read_generic::<RawJavaClassInstance, _>(core, ptr_raw)
                .ok()
                .and_then(|raw| Some((raw.ptr_fields, Allocator::allocation_size(core, raw.ptr_fields).ok()?)));
            if let Some((ptr_fields, size)) = fields {
                graph.blocks.insert(ptr_fields);
                match Allocator::bucket_slot_of(ptr_fields) {
                    Some((slot, _)) => {
                        graph.slots.insert(slot, index);
                    }
                    None => graph.large.push((ptr_fields, ptr_fields + size, index)),
                }
            }
            graph.fields.push(fields);
        }
        graph.large.sort_unstable();

        Ok(graph)
    }

    /// The object `address` points into, if any.
    fn object_at(&self, address: u32) -> Option<usize> {
        if !(HEAP_BASE..HEAP_BASE + HEAP_SIZE).contains(&address) {
            return None;
        }

        if let Some((slot, _)) = Allocator::bucket_slot_of(address) {
            return self.slots.get(&slot).copied();
        }

        let after = self.large.partition_point(|&(start, _, _)| start <= address);
        let &(_, end, index) = self.large.get(after.checked_sub(1)?)?;

        (address < end).then_some(index)
    }
}

struct Marking {
    marked: Vec<bool>,
    worklist: Vec<usize>,
}

impl Marking {
    fn new(graph: &ObjectGraph) -> Self {
        Self {
            marked: alloc::vec![false; graph.fields.len()],
            worklist: Vec::new(),
        }
    }

    fn mark(&mut self, index: usize) {
        if !self.marked[index] {
            self.marked[index] = true;
            self.worklist.push(index);
        }
    }

    fn reference(&mut self, graph: &ObjectGraph, word: u32) {
        if let Some(index) = graph.object_at(word) {
            self.mark(index);
        }
    }

    fn scan(&mut self, graph: &ObjectGraph, bytes: &[u8]) {
        for word in bytes.chunks_exact(4) {
            self.reference(graph, u32::from_le_bytes([word[0], word[1], word[2], word[3]]));
        }
    }

    /// Scans `[low, high)` a piece at a time.
    fn scan_range(&mut self, core: &ArmCore, graph: &ObjectGraph, low: u32, high: u32) -> Result<()> {
        const PIECE: u32 = 0x10000;

        let mut buffer = alloc::vec![0; PIECE as usize];
        let mut at = low & !3;
        while at < high {
            let length = PIECE.min(high - at);
            let piece = &mut buffer[..length as usize];
            // A region can have a hole an engine will not read; what is
            // unreadable holds no references.
            if core.read_bytes(at, piece).is_ok() {
                self.scan(graph, piece);
            }
            at += length;
        }

        Ok(())
    }
}
