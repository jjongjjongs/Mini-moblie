use alloc::{collections::BTreeSet, format, string::String, vec::Vec};
use core::mem::{offset_of, size_of};
use jvm::Jvm;

use wie_backend::System;
use wie_core_arm::{Allocator, ArmCore, EmulatedFunction, ResultWriter, SvcId};
use wie_jvm_support::JvmSupport;
use wie_util::{Result, WieError, read_generic, read_null_terminated_string_bytes, read_null_terminated_table, write_generic};

use java_constants::{FieldAccessFlags, MethodAccessFlags};
use jvm::JavaType;
use wipi_types::ktf::{
    ExeInterface, ExeInterfaceFunctions, InitParam0, InitParam1, InitParam3, InitParam4, WipiExe,
    java::{JavaClass, JavaClassDescriptor, JavaFieldDefinition, JavaMethodDefinition},
};

use crate::{
    adf::parse_bss_size,
    emulator::IMAGE_BASE,
    runtime::{
        SVC_CATEGORY_INIT, SVC_CATEGORY_MODULE, SVC_CATEGORY_MODULE_CLASS, SVC_CATEGORY_MODULE_JUMP,
        java::{
            interface::{
                get_java_method, get_wipi_jb_interface, java_array_new, java_check_type, java_class_load, java_new, java_throw, map_jump_result,
            },
            jvm_support::{JavaMethodResult, JavaVtable, KtfJvmSupport},
        },
        svc_ids::InitSvcId,
        wipi_c::{interface::get_wipic_knl_interface, register_wipic_svc_handler},
    },
};

pub fn register_init_svc_handler(core: &mut ArmCore, jvm: &Jvm) -> Result<()> {
    core.register_svc_handler(SVC_CATEGORY_MODULE, handle_module_svc, jvm)?;
    core.register_svc_handler(SVC_CATEGORY_MODULE_CLASS, handle_module_class_svc, &())?;
    core.register_svc_handler(SVC_CATEGORY_MODULE_JUMP, handle_module_jump_svc, &())?;
    core.register_svc_handler(SVC_CATEGORY_INIT, handle_init_svc, jvm)
}

/// How many slots `MNInterface` is handed out with.
///
/// A guess, and deliberately a loud one: the slots below are the ones a title
/// has asked for, and every other slot answers with a warning naming itself
/// and its arguments, so the next one says what it is in a single run.
const MODULE_INTERFACE_SLOTS: u32 = 64;

/// `MNInterface`'s throw, at `+0x20`, which takes the name of the class to
/// throw and a word the module leaves zero.
const MODULE_JAVA_THROW: u32 = 0x20 / size_of::<u32>() as u32;

/// `MNInterface`'s instantiate, at `+0x38`, which takes the class to make one
/// of and answers the instance, or zero.
const MODULE_JAVA_NEW: u32 = 0x38 / size_of::<u32>() as u32;

/// `MNInterface`'s array make, at `+0x3c`, which takes the array's own class
/// and a length - `java_array_new`, the same call the ordinary module makes,
/// which reads the element out of the class's name.
const MODULE_ARRAY_NEW: u32 = 0x3c / size_of::<u32>() as u32;

/// `MNInterface`'s array class, at `+0x6c`, which takes what an array is to be
/// an array of and answers the class [`MODULE_ARRAY_NEW`] takes. A module asks
/// for this one only where the element is a class: an array of primitives
/// names its element by its letter, and goes straight to the make.
const MODULE_ARRAY_CLASS: u32 = 0x6c / size_of::<u32>() as u32;

/// `MNInterface`'s other array make, at `+0x74`, which takes the same two.
const MODULE_ARRAY_NEW_OF_CLASS: u32 = 0x74 / size_of::<u32>() as u32;

/// `MNInterface`'s primitive array make, at `+0x70`, which takes a type and a
/// length. The type is where that type's letter sits in `InitParam3` - the
/// table of them the ordinary module is handed at `fn_init` - so 텐가이 asks
/// for an array of `0x28`, which is where `int` is.
const MODULE_PRIMITIVE_ARRAY_NEW: u32 = 0x70 / size_of::<u32>() as u32;

/// The letter `java_array_new` takes for the type `InitParam3` keeps at
/// `offset`.
fn module_primitive_element(offset: u32) -> Result<u32> {
    let letter = match offset as usize {
        x if x == offset_of!(InitParam3, boolean) => b'Z',
        x if x == offset_of!(InitParam3, char) => b'C',
        x if x == offset_of!(InitParam3, float) => b'F',
        x if x == offset_of!(InitParam3, double) => b'D',
        x if x == offset_of!(InitParam3, byte) => b'B',
        x if x == offset_of!(InitParam3, short) => b'S',
        x if x == offset_of!(InitParam3, int) => b'I',
        x if x == offset_of!(InitParam3, long) => b'J',
        _ => return Err(WieError::FatalError(format!("a module asked for an array of whatever is at {offset:#x}"))),
    };

    Ok(letter as u32)
}

/// The class of an array of `ptr_class`.
async fn module_array_class(core: &mut ArmCore, jvm: &Jvm, ptr_class: u32) -> Result<u32> {
    let element = KtfJvmSupport::class_from_raw(core, ptr_class).name()?;

    // `[I` is an array of `I`; `[Ljava/lang/String;` is an array of that class.
    let name = if element.starts_with('[') || element.len() == 1 {
        format!("[{element}")
    } else {
        format!("[L{element};")
    };

    match jvm.resolve_class(&name).await {
        Ok(x) => KtfJvmSupport::class_definition_raw(&*x.definition),
        Err(e) => Err(JvmSupport::to_wie_err(jvm, e).await),
    }
}

/// `MNInterface`'s class of an object, at `+0x44`.
///
/// Read the way the module reads it - out of the object's first field, which
/// carries its vtable's index - because that is all some of these objects
/// carry. A string constant lives in the module's own image, one word for the
/// pointer to its fields and the fields behind it, with no room for the class
/// pointer this runtime's own objects keep.
const MODULE_OBJECT_CLASS: u32 = 0x44 / size_of::<u32>() as u32;

/// The class of `ptr_instance`, however its object was made.
///
/// `JavaClassInstance` answers this for both kinds - the word beside the
/// fields where an object of this runtime's own making has one, and the
/// vtable index in the first field for an object that a module's image
/// carries.
fn module_object_class(core: &mut ArmCore, ptr_instance: u32) -> Result<u32> {
    Ok(KtfJvmSupport::class_instance_from_raw(core, ptr_instance).class()?.ptr_raw)
}

/// `MNInterface`'s type test, at `+0x48`, which takes a class and an object.
///
/// The module asks this before it stores into an array of objects, and throws
/// `ArrayStoreException` on a no. Walking the object's own class up its
/// parents answers it: a KTF class record carries its parent and nothing
/// about its interfaces - `JavaClassDefinition::interface_names` has none
/// either - so an interface is the one thing this cannot see.
const MODULE_IS_INSTANCE: u32 = 0x48 / size_of::<u32>() as u32;

/// Whether `ptr_instance` is one of `ptr_class`.
fn module_is_instance(core: &mut ArmCore, ptr_class: u32, ptr_instance: u32) -> Result<u32> {
    if ptr_instance == 0 {
        return Ok(0);
    }

    // Where the class comes out of an array's record, an array this runtime
    // made of anything but objects leaves it zero - there is no class to name.
    // Nothing is known against the store, so it is allowed.
    if ptr_class == 0 {
        return Ok(1);
    }

    let mut current = module_object_class(core, ptr_instance)?;
    while current != 0 {
        if current == ptr_class {
            return Ok(1);
        }

        let class: JavaClass = read_generic(core, current)?;
        let descriptor: JavaClassDescriptor = read_generic(core, class.ptr_descriptor)?;
        current = descriptor.ptr_parent_class;
    }

    let instance_class = module_object_class(core, ptr_instance)?;
    let instance_name = KtfJvmSupport::class_from_raw(core, instance_class).name()?;
    let class_name = KtfJvmSupport::class_from_raw(core, ptr_class).name()?;
    tracing::debug!("{instance_name} ({instance_class:#x}) is not a {class_name} ({ptr_class:#x})");

    Ok(0)
}

/// `MNInterface`'s class initializer, at `+0x60`.
///
/// The module asks for this before it touches a class's statics, and only
/// where the class's own record does not already carry
/// [`CLASS_INITIALIZED`] - which is the flag this runtime writes into every
/// class it makes itself, so only the module's own classes ever reach here.
const MODULE_ENSURE_INITIALIZED: u32 = 0x60 / size_of::<u32>() as u32;

/// The bit a `JavaClass` carries once its initializer has run.
const CLASS_INITIALIZED: u16 = 8;

/// Runs a module class's initializer, if the JVM has not run it already.
///
/// The JVM's own initialization is what runs it: this is a class it knows,
/// registered through `fn_get_class`, and asking it again while it is running
/// - which is where the module asks from, inside `<clinit>` itself - answers
/// without running anything a second time.
async fn module_ensure_initialized(core: &mut ArmCore, jvm: &mut Jvm, ptr_class: u32) -> Result<u32> {
    let class = KtfJvmSupport::class_from_raw(core, ptr_class);
    let name = class.name()?;

    tracing::debug!("module class initializer for {name}");

    let resolved = match jvm.resolve_class(&name).await {
        Ok(x) => x,
        Err(e) => return Err(JvmSupport::to_wie_err(jvm, e).await),
    };

    if let Err(e) = jvm.ensure_initialized(&resolved).await {
        return Err(JvmSupport::to_wie_err(jvm, e).await);
    }

    let mut raw: JavaClass = read_generic(core, ptr_class)?;
    raw.unk_flag |= CLASS_INITIALIZED;
    write_generic(core, ptr_class, raw)?;

    Ok(0)
}

/// `MNInterface`'s method lookup, at `+0x64`, which takes a class and a full
/// name - descriptor and name in one string - and answers the method.
const MODULE_GET_METHOD: u32 = 0x64 / size_of::<u32>() as u32;

/// `MNInterface`'s class load, at `+0x40`.
///
/// The module's own resolver reads a class reference cell, and where the cell
/// holds an index rather than an address - the same `(index << 1) | 1` a
/// class's parent carries, see [`import_index`] - it takes the name the
/// constant pool keeps at that index and asks for it here, with a word of its
/// own stack to put the class in. Which is `java_class_load`, the call the
/// ordinary module makes for the same thing.
const MODULE_CLASS_LOAD: u32 = 0x40 / size_of::<u32>() as u32;

/// A slot of `MNInterface`.
async fn handle_module_svc(core: &mut ArmCore, jvm: &mut Jvm, id: SvcId) -> Result<()> {
    let (_, lr) = core.read_pc_lr()?;
    tracing::trace!(
        "MNInterface-{} ({:#x}, {:#x}) from lr={lr:#x}",
        id.0,
        core.read_param(0)?,
        core.read_param(1)?
    );

    match id.0 {
        MODULE_JAVA_THROW => EmulatedFunction::call(&java_throw, core, jvm).await?.write(core, lr),
        MODULE_JAVA_NEW => EmulatedFunction::call(&java_new, core, jvm).await?.write(core, lr),
        MODULE_CLASS_LOAD => EmulatedFunction::call(&java_class_load, core, jvm).await?.write(core, lr),
        MODULE_GET_METHOD => EmulatedFunction::call(&get_java_method, core, &mut ()).await?.write(core, lr),
        MODULE_ENSURE_INITIALIZED => module_ensure_initialized(core, jvm, core.read_param(0)?).await?.write(core, lr),
        MODULE_ARRAY_NEW => EmulatedFunction::call(&java_array_new, core, jvm).await?.write(core, lr),
        MODULE_ARRAY_CLASS => module_array_class(core, jvm, core.read_param(0)?).await?.write(core, lr),
        MODULE_OBJECT_CLASS => module_object_class(core, core.read_param(0)?)?.write(core, lr),
        MODULE_IS_INSTANCE => {
            let (ptr_class, ptr_instance) = (core.read_param(0)?, core.read_param(1)?);

            module_is_instance(core, ptr_class, ptr_instance)?.write(core, lr)
        }
        MODULE_ARRAY_NEW_OF_CLASS => EmulatedFunction::call(&java_array_new, core, jvm).await?.write(core, lr),
        MODULE_PRIMITIVE_ARRAY_NEW => {
            let element = module_primitive_element(core.read_param(0)?)?;
            let count = core.read_param(1)?;

            java_array_new(core, jvm, element, count).await?.write(core, lr)
        }
        _ => {
            let args = [core.read_param(0)?, core.read_param(1)?, core.read_param(2)?, core.read_param(3)?];

            tracing::warn!(
                "stub MNInterface-{} ({:#x}, {:#x}, {:#x}, {:#x}) from lr={lr:#x}",
                id.0,
                args[0],
                args[1],
                args[2],
                args[3]
            );

            0u32.write(core, lr)
        }
    }
}

/// What a relocated module keeps in `fp`, and what it keeps in there.
///
/// The module every other archive carries reaches the runtime through the
/// tables it was handed at `fn_init`. This one reaches it through `fp`, which
/// it never sets: its compiled code opens a `try` by writing a record of its
/// own stack into the word at `+0x2c` and chaining the old head behind it, and
/// steps onto the stack at `+0x34` whenever it calls into the runtime, putting
/// the stack it left in `+0x24`. Nothing hands that over, so it is made here.
///
/// One of these, not one per thread, which is only right while one thread runs
/// module code: the handler chain and the runtime stack are a thread's own.
/// 텐가이 has not got far enough to start a second.
const VM_CONTEXT_SIZE: u32 = 0x40;

/// The stack a module steps onto to call the runtime, whose top goes in
/// `+0x34`.
const VM_CONTEXT_STACK_SIZE: u32 = 0x10000;

/// The word the module takes its runtime stack from. The two it writes -
/// `+0x24` for the stack it stepped off, `+0x2c` for the head of its handler
/// chain - it writes itself, and both start zero like the rest.
const VM_CONTEXT_STACK: u32 = 0x34;

/// The word the module reads the JVM context from, which is where it finds a
/// vtable.
///
/// A virtual call reads the receiver's first field - the vtable index, shifted
/// up by five, which is how every KTF object here already carries it - adds it
/// to this word and reads `+0xc` of that, which is `InitParam2`'s table of
/// vtables. The same arithmetic `get_java_method` does for the ordinary
/// module when it is handed one of these instead of a class.
const VM_CONTEXT_JVM: u32 = 0x38;

fn module_vm_context(core: &mut ArmCore, ptr_jvm_context: u32) -> Result<u32> {
    let context = Allocator::alloc(core, VM_CONTEXT_SIZE)?;
    for word in (0..VM_CONTEXT_SIZE).step_by(size_of::<u32>()) {
        write_generic(core, context + word, 0u32)?;
    }

    let stack = Allocator::alloc(core, VM_CONTEXT_STACK_SIZE)?;
    write_generic(core, context + VM_CONTEXT_STACK, stack + VM_CONTEXT_STACK_SIZE)?;
    write_generic(core, context + VM_CONTEXT_JVM, ptr_jvm_context)?;

    Ok(context)
}

/// The jump table's first two entries, which are the module's method calls:
/// the ordinary one, and the one it takes when the method's own record says
/// the method is native.
const MODULE_INVOKE: u32 = 0;
const MODULE_INVOKE_NATIVE: u32 = 1;

/// The jump table's fifth entry, which a compiled method calls from inside a
/// loop with whatever its registers happen to hold and whose answer it never
/// reads - a place for the runtime to do something, and there is nothing this
/// runtime needs to do there.
const MODULE_POLL: u32 = 4;

/// How many words the module leaves on the stack when it jumps.
///
/// Its call sites put the first three arguments in `r1`, `r2` and `r3` and the
/// rest on the stack, the way its own compiled methods read them - see
/// 텐가이's `TengaiCard.keyNotify(II)Z`, which takes its receiver from `r1`.
/// The stub that reaches this runtime pushes `r2` and `r3` on the way, because
/// resolving the method reference it was given costs those two registers. It
/// never puts them back: whatever serves the jump takes them off the stack
/// itself, which is why the module's own return path adds eight to the stack
/// pointer.
const MODULE_JUMP_PUSHED: u32 = 2;

/// A method call a relocated module made.
///
/// `r0` is the method record, resolved by the stub that jumped here, and the
/// arguments are as [`MODULE_JUMP_PUSHED`] describes. What answers is the
/// method's own entry point, which every method already has two of - one
/// reading its arguments from registers, one from a block - so this picks the
/// one the module's own flag test picked and hands the arguments over the way
/// that entry point reads them.
async fn module_invoke(core: &mut ArmCore, native: bool) -> Result<JavaMethodResult> {
    let ptr_method = core.read_param(0)?;
    let raw: JavaMethodDefinition = read_generic(core, ptr_method)?;

    let mut registers = core.save_context();
    let entry_sp = registers.sp;

    if tracing::enabled!(tracing::Level::TRACE) {
        let name = KtfJvmSupport::read_name(core, raw.ptr_name)?;

        tracing::trace!("module invoke {name} on {:#x}", core.read_param(1)?);
    }

    // The two the stub pushed, and then whatever the call site left above them.
    let argument = |core: &mut ArmCore, index: usize| -> Result<u32> {
        match index {
            0 => core.read_param(1),
            _ => read_generic(core, entry_sp + (index as u32 - 1) * size_of::<u32>() as u32),
        }
    };

    registers.sp = entry_sp + MODULE_JUMP_PUSHED * size_of::<u32>() as u32;
    core.restore_context(&registers);

    let result = if native {
        let name = KtfJvmSupport::read_name(core, raw.ptr_name)?;
        let words = method_argument_words(&name.descriptor, MethodAccessFlags::from_bits_truncate(raw.access_flags));

        let arguments = Allocator::alloc(core, words * size_of::<u32>() as u32)?;
        for index in 0..words {
            let word = argument(core, index as usize)?;
            write_generic(core, arguments + index * size_of::<u32>() as u32, word)?;
        }

        let result = core.run_function::<u32>(raw.fn_body_native_or_exception_table, &[0, arguments]).await;

        Allocator::free(core, arguments, words * size_of::<u32>() as u32)?;

        result
    } else {
        // Only the three the entry point takes in registers: the rest are on
        // the stack already, where it reads them, and passing them again would
        // put a second copy above them.
        let arguments = [0, argument(core, 0)?, argument(core, 1)?, argument(core, 2)?];

        core.run_function::<u32>(raw.fn_body, &arguments).await
    };

    map_jump_result(entry_sp, result)
}

/// How many words a call of this descriptor puts in registers and on the
/// stack, the receiver included unless the method is static.
fn method_argument_words(descriptor: &str, access_flags: MethodAccessFlags) -> u32 {
    let parsed = JavaType::parse(descriptor);
    let (parameters, _) = parsed.as_method();

    let receiver = if access_flags.contains(MethodAccessFlags::STATIC) { 0 } else { 1 };

    parameters.iter().fold(receiver, |words, parameter| {
        words + if matches!(parameter, JavaType::Long | JavaType::Double) { 2 } else { 1 }
    })
}

/// An entry of the table a relocated module tail-jumps through.
///
/// See [`crate::module::RelocatedModule::jump_table`]. What each one is, from
/// where the module jumps to it:
///
/// | slot | jumped to after                                  |
/// |------|--------------------------------------------------|
/// | 0    | a method reference resolved, receiver in `r1`     |
/// | 1    | the same, for a method whose flags took the other branch |
/// | 2    | a class reference resolved                        |
/// | 3    | a class reference resolved, the other of the pair  |
/// | 4    | a frame popped                                    |
/// | 5    | an array index found out of bounds                |
async fn handle_module_jump_svc(core: &mut ArmCore, _: &mut (), id: SvcId) -> Result<()> {
    let (_, lr) = core.read_pc_lr()?;

    match id.0 {
        MODULE_INVOKE => return module_invoke(core, false).await?.write(core, lr),
        MODULE_INVOKE_NATIVE => return module_invoke(core, true).await?.write(core, lr),
        MODULE_POLL => return 0u32.write(core, lr),
        _ => (),
    }

    let args = [core.read_param(0)?, core.read_param(1)?, core.read_param(2)?, core.read_param(3)?];

    // Nothing answers the rest yet, and answering nothing is not an answer: the
    // module jumped, so whatever is here is the rest of its call. Say which one
    // it was and stop, rather than return a zero it will branch through.
    Err(WieError::FatalError(format!(
        "module jump {} ({:#x}, {:#x}, {:#x}, {:#x}) from lr={lr:#x} is not served yet. See wie_ktf::module.",
        id.0, args[0], args[1], args[2], args[3]
    )))
}

/// A relocated module's `fn_get_class`, which this runtime writes for it.
///
/// The ordinary module hands out a function of its own that answers a
/// `JavaClass` for a name. A relocated one has no such function - it has a
/// class table, in its module descriptor - so the table is read here and
/// handed back through a stub that looks like the function the rest of this
/// runtime already asks. The descriptor's address rides in the stub's own SVC
/// id, so there is no state to keep beside it.
///
/// The table is 텐가이's `JavaClass` records, the same ones
/// `KtfJvmSupport::class_from_raw` reads: each bucket holds one, whose first
/// word is its own address plus four - the mark this runtime already tests for
/// in `get_java_method`. Its 22 classes land in 22 of its 32 buckets, so a
/// bucket holding more than one has not been seen; the scan takes whatever a
/// bucket holds and compares the name.
async fn handle_module_class_svc(core: &mut ArmCore, _: &mut (), id: SvcId) -> Result<()> {
    let (_, lr) = core.read_pc_lr()?;
    let ptr_name = core.read_param(0)?;

    let name_bytes = read_null_terminated_string_bytes(core, ptr_name)?;
    let name = encoding_rs::EUC_KR.decode(&name_bytes).0;

    let class = module_class_by_name(core, id.0, &name)?;
    tracing::debug!("module class {name} -> {class:#x}");

    class.write(core, lr)
}

/// Whether a word where a class should be is an import instead.
///
/// A resolved one is a `JavaClass` address, and those are word aligned. An
/// unresolved one is the index the module's constant pool names it at,
/// shifted up and marked with the bit an address never has: 텐가이's
/// `java/lang/Object` is `pool[0x25]`, written `0x4b`.
fn import_index(value: u32) -> Option<u32> {
    (value & 1 != 0).then_some(value >> 1)
}

/// Every class a module descriptor's table holds.
fn module_classes(core: &mut ArmCore, descriptor: u32) -> Result<Vec<u32>> {
    let buckets: u32 = read_generic(core, descriptor)?;
    let bucket_count: u32 = read_generic(core, descriptor + 2 * size_of::<u32>() as u32)?;

    let mut classes = Vec::new();
    for bucket in 0..bucket_count {
        let ptr_class: u32 = read_generic(core, buckets + bucket * size_of::<u32>() as u32)?;
        if ptr_class != 0 {
            classes.push(ptr_class);
        }
    }

    Ok(classes)
}

/// Puts each of a module's classes behind the fields of its parent.
///
/// A class's field records name offsets into its own block only - 텐가이's
/// `Tengai` keeps its three at 0, 4 and 8 and calls that twelve bytes - and
/// where the parent's fields go is not the module's to decide: the parent is
/// this runtime's class, whose own layout it cannot know. The ordinary module
/// settles that inside `fn_init`, reading the parent's size out of the record
/// this runtime handed it and moving its own fields behind it. A relocated one
/// has no `fn_init`, and it reads the offsets back out of these records
/// whenever it touches a field, so moving them here is the same answer.
///
/// A parent in this module is moved first, so what it says about its size is
/// the whole of it by the time a child asks.
fn rebase_module_class_fields(core: &mut ArmCore, descriptor: u32, ptr_class: u32, done: &mut BTreeSet<u32>) -> Result<u16> {
    let class: JavaClass = read_generic(core, ptr_class)?;
    let mut class_descriptor: JavaClassDescriptor = read_generic(core, class.ptr_descriptor)?;

    if done.contains(&ptr_class) {
        return Ok(class_descriptor.fields_size);
    }
    done.insert(ptr_class);

    let parent_size = if class_descriptor.ptr_parent_class != 0 {
        rebase_module_class_fields(core, descriptor, class_descriptor.ptr_parent_class, done)?
    } else {
        0
    };

    // A class of this runtime's own is already laid out behind its parent, and
    // its records are shared with every other title. Only a module's are moved.
    if !module_classes(core, descriptor)?.contains(&ptr_class) {
        return Ok(class_descriptor.fields_size);
    }

    let name = KtfJvmSupport::class_from_raw(core, ptr_class).name()?;

    // An array class keeps its element type where another keeps its fields.
    if !name.starts_with('[') && class_descriptor.ptr_fields_or_element_type != 0 {
        for ptr_field in read_null_terminated_table(core, class_descriptor.ptr_fields_or_element_type)? {
            let mut field: JavaFieldDefinition = read_generic(core, ptr_field)?;

            // A static keeps its value in that word, not an offset. What it
            // keeps there before the class's initializer has run is the
            // compiler's own: 텐가이's `TengaiClient.SERVER_IP` holds
            // `0x579fa20`, which is no address in this image and no object.
            // A static of a reference type starts as null, and the
            // initializer - which the module asks for before it reads one -
            // puts the real thing there.
            if FieldAccessFlags::from_bits_truncate(field.access_flags as u16).contains(FieldAccessFlags::STATIC) {
                let descriptor = KtfJvmSupport::read_name(core, field.ptr_name)?.descriptor.clone();

                if descriptor.starts_with('L') || descriptor.starts_with('[') {
                    field.offset_or_value = 0;
                    write_generic(core, ptr_field, field)?;
                }

                continue;
            }

            if parent_size != 0 {
                field.offset_or_value += parent_size as u32;
                write_generic(core, ptr_field, field)?;
            }
        }
    }

    class_descriptor.fields_size += parent_size;
    write_generic(core, class.ptr_descriptor, class_descriptor)?;

    tracing::debug!(
        "{name} keeps its fields behind {parent_size:#x}, so it is {:#x}",
        class_descriptor.fields_size
    );

    Ok(class_descriptor.fields_size)
}

/// Gives each of a module's classes the vtable its own records do not carry.
///
/// The ordinary module builds these inside `fn_init` and hands them over with
/// the class; a relocated one ships `0` where the vtable goes, and its
/// compiled code dispatches through `InitParam2`'s table of them - the
/// receiver's own first field says which one. So they are built here, the same
/// way this runtime builds one for a class of its own, which also writes each
/// method the index its callers read back out of the method record.
fn build_module_vtables(core: &mut ArmCore, descriptor: u32) -> Result<()> {
    for ptr_class in module_classes(core, descriptor)? {
        let class = KtfJvmSupport::class_from_raw(core, ptr_class);

        // An array class has no methods of its own, and the word another class
        // keeps its method table in holds its element type.
        if class.name()?.starts_with('[') {
            continue;
        }

        let vtable = JavaVtable::new(core, &class)?;

        write_generic(core, ptr_class + 3 * size_of::<u32>() as u32, vtable.ptr_raw)?;
        write_generic(core, ptr_class + 4 * size_of::<u32>() as u32, vtable.len()? as u16)?;
    }

    Ok(())
}

/// Turns a relocated module's imports into the classes they name.
///
/// This is the step the ordinary module makes for itself: its `fn_init` walks
/// its own imports and calls back through `java_class_load` for each one,
/// which resolves the name and writes the class's address where the module
/// kept the name. A relocated module has no `fn_init` - wfeature makes the
/// step for it, beside the entry - so it is made here, out of the same two
/// things: the constant pool, and `Jvm::resolve_class`.
///
/// What carries an import is a class's parent, and an array class's element
/// type - which sits in the word another class keeps its fields in. Both hold
/// a class whose image is not this one: `java/lang/Object`,
/// `org/kwis/msp/lcdui/Jlet`, `java/lang/Thread`. A class of this module's own
/// is already an address, written when the image was relocated.
async fn resolve_module_imports(core: &mut ArmCore, jvm: &Jvm, descriptor: u32, pool: u32) -> Result<()> {
    for ptr_class in module_classes(core, descriptor)? {
        let class: JavaClass = read_generic(core, ptr_class)?;
        let mut class_descriptor: JavaClassDescriptor = read_generic(core, class.ptr_descriptor)?;
        let name_bytes = read_null_terminated_string_bytes(core, class_descriptor.ptr_name)?;
        let name = encoding_rs::EUC_KR.decode(&name_bytes).0.into_owned();

        let mut resolved = false;

        // `java/lang/Object` has no parent, and nothing is waiting for it.
        if let Some(index) = import_index(class_descriptor.ptr_parent_class) {
            class_descriptor.ptr_parent_class = module_import(core, jvm, pool, index, &name, "parent").await?;
            resolved = true;
        }

        if name.starts_with('[')
            && let Some(index) = import_index(class_descriptor.ptr_fields_or_element_type)
        {
            class_descriptor.ptr_fields_or_element_type = module_import(core, jvm, pool, index, &name, "element type").await?;
            resolved = true;
        }

        if resolved {
            write_generic(core, class.ptr_descriptor, class_descriptor)?;
        }
    }

    Ok(())
}

/// The class a module's constant pool names at `index`, as an address the
/// module can hold.
async fn module_import(core: &mut ArmCore, jvm: &Jvm, pool: u32, index: u32, class: &str, what: &str) -> Result<u32> {
    let ptr_name: u32 = read_generic(core, pool + index * size_of::<u32>() as u32)?;
    let name_bytes = read_null_terminated_string_bytes(core, ptr_name)?;
    let name = encoding_rs::EUC_KR.decode(&name_bytes).0.into_owned();

    let resolved = match jvm.resolve_class(&name).await {
        Ok(x) => KtfJvmSupport::class_definition_raw(&*x.definition)?,
        Err(e) => {
            let reason = JvmSupport::to_wie_err(jvm, e).await;

            return Err(WieError::FatalError(format!(
                "{class} imports {name} as its {what}, which did not load: {reason}"
            )));
        }
    };

    tracing::trace!("{class}'s {what} is pool[{index:#x}] {name} at {resolved:#x}");

    Ok(resolved)
}

/// Walks a module descriptor's class table for `name`.
fn module_class_by_name(core: &mut ArmCore, descriptor: u32, name: &str) -> Result<u32> {
    for ptr_class in module_classes(core, descriptor)? {
        let class: JavaClass = read_generic(core, ptr_class)?;
        let descriptor: JavaClassDescriptor = read_generic(core, class.ptr_descriptor)?;
        let class_name = read_null_terminated_string_bytes(core, descriptor.ptr_name)?;

        if encoding_rs::EUC_KR.decode(&class_name).0 == name {
            return Ok(ptr_class);
        }
    }

    Ok(0)
}

/// The interface a relocated module asks for by name.
///
/// 텐가이's entry asks for this one and nothing else, stores what it is given
/// in a global of its own and answers zero - success - only if it was given
/// something. So the address has to be real even before what is behind it is
/// known. See `crate::module`.
fn get_module_interface(core: &mut ArmCore) -> Result<u32> {
    let address = Allocator::alloc(core, MODULE_INTERFACE_SLOTS * size_of::<u32>() as u32)?;

    for slot in 0..MODULE_INTERFACE_SLOTS {
        let stub = core.make_svc_stub(SVC_CATEGORY_MODULE, slot)?;
        write_generic(core, address + slot * size_of::<u32>() as u32, stub)?;
    }

    Ok(address)
}

async fn handle_init_svc(core: &mut ArmCore, jvm: &mut Jvm, id: SvcId) -> Result<()> {
    let (_, lr) = core.read_pc_lr()?;

    match InitSvcId::try_from(id)? {
        InitSvcId::GetInterface => get_interface(core, core.read_param(0)?).await?.write(core, lr),
        InitSvcId::JavaThrow => EmulatedFunction::call(&java_throw, core, jvm).await?.write(core, lr),
        InitSvcId::JavaCheckType => EmulatedFunction::call(&java_check_type, core, jvm).await?.write(core, lr),
        InitSvcId::JavaNew => EmulatedFunction::call(&java_new, core, jvm).await?.write(core, lr),
        InitSvcId::JavaArrayNew => EmulatedFunction::call(&java_array_new, core, jvm).await?.write(core, lr),
        InitSvcId::JavaClassLoad => EmulatedFunction::call(&java_class_load, core, jvm).await?.write(core, lr),
        InitSvcId::Alloc => EmulatedFunction::call(&alloc, core, &mut ()).await?.write(core, lr),
    }
}

pub async fn load_native(
    core: &mut ArmCore,
    system: &mut System,
    jvm: &Jvm,
    filename: &str,
    data: &[u8],
    ptr_jvm_context: u32,
    ptr_jvm_exception_context: u32,
) -> Result<ExeInterfaceFunctions> {
    let bss_size = parse_bss_size(filename)?;

    // Which of the two kinds of module this is. See `crate::module`.
    let relocated = crate::module::RelocatedModule::parse(data);

    core.load(data, IMAGE_BASE, data.len() + bss_size as usize)?;

    // Patterns target instruction encodings, which the guest self-rebase at
    // IMAGE_BASE+1 doesn't rewrite — so installing here is sound and skips a
    // re-scan after relocation. Hash-matched entries take priority over
    // hash-less generic ones; only one entry is installed because each install
    // claims fresh SVC categories from a fixed base and they would collide.
    //
    // The scan range covers the whole loaded image because KTF binaries don't
    // expose a code/metadata boundary at this point. Safety relies on the
    // patterns being long enough (and `{exit_b}` strict enough) that a
    // metadata-region collision is implausible; tighten patterns rather than
    // narrow the range if that ever becomes false.
    wie_core_arm::install_binary_patches(core, data, &[(IMAGE_BASE, data.len() as u32)])?;

    register_wipic_svc_handler(core, system, jvm)?;
    register_init_svc_handler(core, jvm)?;

    tracing::debug!("Loaded at {IMAGE_BASE:#x}, size {:#x}, bss {bss_size:#x}", data.len());

    let ptr_param_0 = Allocator::alloc(core, size_of::<InitParam0>() as u32)?;
    write_generic(core, ptr_param_0, InitParam0 { unk: 0 })?;

    let ptr_param_1 = Allocator::alloc(core, size_of::<InitParam1>() as u32)?;
    write_generic(core, ptr_param_1, InitParam1 { ptr_jvm_exception_context })?;

    let param_3 = InitParam3 {
        unk1: 0,
        unk2: 0,
        unk3: 0,
        unk4: 0,
        boolean: b'Z' as u32,
        char: b'C' as u32,
        float: b'F' as u32,
        double: b'D' as u32,
        byte: b'B' as u32,
        short: b'S' as u32,
        int: b'I' as u32,
        long: b'J' as u32,
    };

    let ptr_param_3 = Allocator::alloc(core, size_of::<InitParam3>() as u32)?;
    write_generic(core, ptr_param_3, param_3)?;

    let param_4 = InitParam4 {
        fn_get_interface: core.make_svc_stub(SVC_CATEGORY_INIT, InitSvcId::GetInterface)?,
        fn_java_throw: core.make_svc_stub(SVC_CATEGORY_INIT, InitSvcId::JavaThrow)?,
        unk1: 0,
        unk2: 0,
        fn_java_check_type: core.make_svc_stub(SVC_CATEGORY_INIT, InitSvcId::JavaCheckType)?,
        fn_java_new: core.make_svc_stub(SVC_CATEGORY_INIT, InitSvcId::JavaNew)?,
        fn_java_array_new: core.make_svc_stub(SVC_CATEGORY_INIT, InitSvcId::JavaArrayNew)?,
        unk6: 0,
        fn_java_class_load: core.make_svc_stub(SVC_CATEGORY_INIT, InitSvcId::JavaClassLoad)?,
        unk7: 0,
        unk8: 0,
        fn_alloc: core.make_svc_stub(SVC_CATEGORY_INIT, InitSvcId::Alloc)?,
    };

    let ptr_param_4 = Allocator::alloc(core, size_of::<InitParam4>() as u32)?;
    write_generic(core, ptr_param_4, param_4)?;

    // The ordinary module opens with a stub of its own that rebases the image
    // and answers with a `WipiExe`, and takes the bss size. A relocated one is
    // rebased here instead and names its entry in its header; what it takes is
    // a pointer to the host's own functions, the first of which it calls to
    // ask for an interface by name.
    let (entry, argument) = match &relocated {
        Some(module) => {
            module.relocate(core, data, IMAGE_BASE)?;
            module.rebase_module_fields(core, data, IMAGE_BASE)?;

            let jumps = module.base(IMAGE_BASE) + module.jump_table(data)?;
            for slot in 0..crate::module::RelocatedModule::JUMP_TABLE_ENTRIES {
                let stub = core.make_svc_stub(SVC_CATEGORY_MODULE_JUMP, slot)?;
                write_generic(core, jumps + slot * size_of::<u32>() as u32, stub)?;
            }

            // Both ways the module reaches its context: `fp` for the code that
            // was handed it, and the word at `+0x20` for the code that reads it
            // out of the image - a method's epilogue puts the handler chain
            // back through that one.
            let vm_context = module_vm_context(core, ptr_jvm_context)?;
            core.reserve_fp(vm_context);
            write_generic(
                core,
                module.base(IMAGE_BASE) + crate::module::RelocatedModule::VM_CONTEXT_OFFSET as u32,
                vm_context,
            )?;

            tracing::debug!("{filename}: jump table at {jumps:#x}, VM context at {vm_context:#x}");

            let entry = module.entry(data, IMAGE_BASE)?;
            tracing::debug!(
                "Relocated {filename} at {:#x}: {} relocations, bss {:#x}, entry {entry:#x}",
                module.base(IMAGE_BASE),
                module.relocations,
                module.bss_size
            );

            (entry, ptr_param_4)
        }
        None => (IMAGE_BASE + 1, bss_size),
    };

    let entry_result = core.run_function::<u32>(entry, &[argument]).await?;

    // The two kinds answer differently. The ordinary module hands back its
    // `WipiExe`, whose `ExeInterface` carries the functions below. A relocated
    // one answers zero - "the interface I asked for was there" - and keeps
    // what it built in its module descriptor, so the one function the rest of
    // this runtime asks of a module, `fn_get_class`, is written for it here.
    if let Some(module) = &relocated {
        if entry_result != 0 {
            return Err(WieError::FatalError(format!("{filename} refused to start: {entry_result:#x}")));
        }

        let descriptor = module.base(IMAGE_BASE) + module.descriptor(data)?;
        let classes: u32 = read_generic(core, descriptor + size_of::<u32>() as u32)?;
        tracing::debug!("{filename} started: module descriptor at {descriptor:#x}, {classes} classes");

        // A class's parent is an address once the module's imports are
        // resolved, and the index its constant pool names it at before that.
        // The ordinary module resolves them inside `fn_init`, calling back
        // through `java_class_load`. A relocated one has no `fn_init` to
        // call, so the same step is made here.
        let pool = module.base(IMAGE_BASE) + module.constant_pool(data)?;
        resolve_module_imports(core, jvm, descriptor, pool).await?;

        let mut rebased = BTreeSet::new();
        for ptr_class in module_classes(core, descriptor)? {
            rebase_module_class_fields(core, descriptor, ptr_class, &mut rebased)?;
        }

        build_module_vtables(core, descriptor)?;

        return Ok(ExeInterfaceFunctions {
            unk1: 0,
            unk2: 0,
            fn_init: 0,
            fn_get_default_dll: 0,
            fn_get_class: core.make_svc_stub(SVC_CATEGORY_MODULE_CLASS, descriptor)?,
            fn_unk2: 0,
            fn_unk3: 0,
        });
    }

    let wipi_exe = entry_result;
    tracing::debug!("Got wipi_exe {wipi_exe:#x}");

    let wipi_exe: WipiExe = read_generic(core, wipi_exe)?;
    let exe_interface: ExeInterface = read_generic(core, wipi_exe.ptr_exe_interface)?;
    let exe_interface_functions: ExeInterfaceFunctions = read_generic(core, exe_interface.ptr_functions)?;

    tracing::debug!("Call init at {:#x}", exe_interface_functions.fn_init);
    let result = core
        .run_function::<u32>(
            exe_interface_functions.fn_init,
            &[ptr_param_0, ptr_param_1, ptr_jvm_context, ptr_param_3, ptr_param_4],
        )
        .await?;

    if result != 0 {
        return Err(WieError::FatalError(format!("Init failed with code {result:#x}")));
    }

    // call init
    let result = core.run_function::<u32>(wipi_exe.fn_init, &[]).await?;
    if result != 0 {
        return Err(WieError::FatalError(format!("wipi init failed with code {result:#x}")));
    }

    Ok(exe_interface_functions)
}

async fn get_interface(core: &mut ArmCore, ptr_name: u32) -> Result<u32> {
    tracing::trace!("get_interface({ptr_name:#x})");

    let name = String::from_utf8(read_null_terminated_string_bytes(core, ptr_name)?).unwrap();

    match name.as_str() {
        "WIPIC_knlInterface" => get_wipic_knl_interface(core),
        "WIPI_JBInterface" => get_wipi_jb_interface(core),
        "MNInterface" => get_module_interface(core),
        _ => {
            tracing::warn!("Unknown {name}");

            Ok(0)
        }
    }
}

async fn alloc(core: &mut ArmCore, _: &mut (), a0: u32) -> Result<u32> {
    tracing::trace!("alloc({a0})");

    Allocator::alloc(core, a0)
}
