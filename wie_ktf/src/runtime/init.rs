use alloc::{format, string::String};
use core::mem::size_of;
use jvm::Jvm;

use wie_backend::System;
use wie_core_arm::{Allocator, ArmCore, EmulatedFunction, ResultWriter, SvcId};
use wie_util::{Result, WieError, read_generic, read_null_terminated_string_bytes, write_generic};

use wipi_types::ktf::{
    ExeInterface, ExeInterfaceFunctions, InitParam0, InitParam1, InitParam3, InitParam4, WipiExe,
    java::{JavaClass, JavaClassDescriptor},
};

use crate::{
    adf::parse_bss_size,
    emulator::IMAGE_BASE,
    runtime::{
        SVC_CATEGORY_INIT, SVC_CATEGORY_MODULE, SVC_CATEGORY_MODULE_CLASS,
        java::interface::{get_wipi_jb_interface, java_array_new, java_check_type, java_class_load, java_new, java_throw},
        svc_ids::InitSvcId,
        wipi_c::{interface::get_wipic_knl_interface, register_wipic_svc_handler},
    },
};

pub fn register_init_svc_handler(core: &mut ArmCore, jvm: &Jvm) -> Result<()> {
    core.register_svc_handler(SVC_CATEGORY_MODULE, handle_module_svc, &())?;
    core.register_svc_handler(SVC_CATEGORY_MODULE_CLASS, handle_module_class_svc, &())?;
    core.register_svc_handler(SVC_CATEGORY_INIT, handle_init_svc, jvm)
}

/// How many slots `MNInterface` is handed out with.
///
/// A guess, and deliberately a loud one: nothing has called through it yet, so
/// nothing says how many there are. Every slot answers with a warning naming
/// itself and its arguments, so the first title that uses one says what it is
/// in a single run.
const MODULE_INTERFACE_SLOTS: u32 = 64;

/// A slot of `MNInterface`, which writes down what it was asked and answers
/// nothing.
async fn handle_module_svc(core: &mut ArmCore, _: &mut (), id: SvcId) -> Result<()> {
    let (_, lr) = core.read_pc_lr()?;
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

/// The first parent in a module's class table that is not a class, if any.
///
/// A parent is a `JavaClass` address once the module has resolved its
/// imports. Anything below where the image was loaded is not one.
fn first_unresolved_parent(core: &mut ArmCore, descriptor: u32) -> Result<Option<u32>> {
    let buckets: u32 = read_generic(core, descriptor)?;
    let bucket_count: u32 = read_generic(core, descriptor + 2 * size_of::<u32>() as u32)?;

    for bucket in 0..bucket_count {
        let ptr_class: u32 = read_generic(core, buckets + bucket * size_of::<u32>() as u32)?;
        if ptr_class == 0 {
            continue;
        }

        let class: JavaClass = read_generic(core, ptr_class)?;
        let class_descriptor: JavaClassDescriptor = read_generic(core, class.ptr_descriptor)?;

        // `java/lang/Object` has no parent, and nothing is wrong with that.
        if class_descriptor.ptr_parent_class != 0 && class_descriptor.ptr_parent_class < IMAGE_BASE {
            return Ok(Some(class_descriptor.ptr_parent_class));
        }
    }

    Ok(None)
}

/// Walks a module descriptor's class table for `name`.
fn module_class_by_name(core: &mut ArmCore, descriptor: u32, name: &str) -> Result<u32> {
    let buckets: u32 = read_generic(core, descriptor)?;
    let bucket_count: u32 = read_generic(core, descriptor + 2 * size_of::<u32>() as u32)?;

    for bucket in 0..bucket_count {
        let ptr_class: u32 = read_generic(core, buckets + bucket * size_of::<u32>() as u32)?;
        if ptr_class == 0 {
            continue;
        }

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

        // A class's parent is a pointer once the module has resolved its
        // imports, and something far too small to be one before. 텐가이's
        // `Tengai` names `0x75` and its `[LEnemy;` names `0x4b`, which are
        // neither addresses nor indices into any table here - so the step that
        // turns them into classes has not been made, and registering one of
        // these would read a parent out of low memory.
        //
        // The ordinary module resolves its imports inside `fn_init`, calling
        // back through `java_class_load`. A relocated one has no `fn_init` to
        // call; wfeature has a step of its own for it, beside the entry -
        // "execute KTF interface init" - and that is what is missing.
        if let Some(unresolved) = first_unresolved_parent(core, descriptor)? {
            return Err(WieError::FatalError(format!(
                "{filename} is a relocated module: it loads, relocates, starts and answers with its {classes} classes, but their imports are \
                 unresolved - {unresolved:#x} where a parent class should be - so the interface init a relocated module takes instead of \
                 `fn_init` is still missing. Module descriptor at {descriptor:#x}. See wie_ktf::module."
            )));
        }

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
