mod array_class_definition;
mod array_class_instance;
mod class_definition;
mod class_file;
mod class_instance;
mod classes;
mod field;
mod jvm_implementation;
mod method;
mod name;
mod value;
mod vtable;

use alloc::{boxed::Box, format, sync::Arc};
use core::mem::{offset_of, size_of};
use jvm_implementation::KtfJvmImplementation;

use bytemuck::{Pod, Zeroable};

use java_runtime::classes::java::util::{Enumeration, jar::JarEntry};
use jvm::{ClassDefinition, ClassInstance, ClassInstanceRef, Jvm, runtime::JavaLangString};

use wie_backend::System;
use wie_core_arm::{Allocator, ArmCore};
use wie_jvm_support::JvmSupport;
use wie_util::{Result, WieError, read_generic, write_generic};

use wipi_types::ktf::java::{JavaClass as RawJavaClass, JavaClassDescriptor as RawJavaClassDescriptor};

use self::{
    array_class_instance::JavaArrayClassInstance,
    classes::{
        com::ktf::kfc::{GForm, GMenubarForm, GMsgBox, GTextField, GTextListener},
        net::wie::{ClassLoaderContext, KtfClassLoader},
        wec::DMInfo,
    },
    name::JavaFullName,
};
use super::interface::register_java_interface_svc_handler;

pub use self::{
    array_class_definition::JavaArrayClassDefinition,
    class_definition::JavaClassDefinition,
    class_instance::JavaClassInstance,
    method::{JavaMethod, JavaMethodResult},
    vtable::JavaVtable,
};

pub type KtfJvmWord = u32;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct KtfJvmExceptionContext {
    unk: [u32; 8],
    current_java_exception_handler: u32,
    /// What a native compiled into the title's own module leaves its answer in.
    ///
    /// This structure is ours - `InitParam1` hands the module its address at
    /// `fn_init` - and the AOT C runtime linked into the module keeps its
    /// return slot at the end of it: a type tag, then the value. A native
    /// returning `int` writes tag 2 and the number; a `void` one writes
    /// nothing. See `call_native`, which is the only thing that reads them.
    native_return_type: u32,
    native_return_value: u32,
}

/// `native_return_type` and `native_return_value` from the start of the
/// exception context, for the reads `call_native` does without the struct.
/// Where the head of the handler chain sits in that structure, which is what
/// has to be private to each thread; see `register_thread_local_word`.
const EXCEPTION_HANDLER_HEAD_OFFSET: u32 = 0x20;

pub(crate) const NATIVE_RETURN_TYPE_OFFSET: u32 = 0x24;
pub(crate) const NATIVE_RETURN_VALUE_OFFSET: u32 = 0x28;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct KtfJvmSupportContext {
    /// The array of class records the JVM context is, which is what a vtable
    /// index indexes. See [`KtfJvmSupport::get_vtable_index`].
    ptr_class_records: u32,
    ptr_jvm_exception_context: u32,
    /// The class each record in `ptr_class_records` stands for, one per
    /// record. See [`KtfJvmSupport::class_by_vtable_word`].
    ptr_vtable_classes: u32,
}

const SUPPORT_CONTEXT_BASE: u32 = 0x7fff0000;

/// How many class records the JVM context carries.
const VTABLE_COUNT: usize = 128;

/// How far apart two vtable indices are, which is a class record in words.
///
/// The index an object carries is a word offset from the JVM context, and what
/// it points at there is a whole class record - so one class takes as many
/// indices as a record has words, and the indices a class can be given are the
/// multiples of this. `[C`, `java/lang/String` and `java/lang/Class` at 0, 5
/// and 10 are the first three records of that array, which is what says a
/// record is five words long.
const VTABLE_STRIDE: usize = size_of::<RawJavaClass>() / size_of::<u32>();

/// Which record a vtable index names, if it names one at all.
///
/// An index that is not a whole number of records from the head of the array
/// names no class: it falls inside one, which is what a word this runtime
/// never wrote looks like.
fn record_of_index(index: u32) -> Option<usize> {
    let record = index as usize / VTABLE_STRIDE;

    ((index as usize).is_multiple_of(VTABLE_STRIDE) && record < VTABLE_COUNT).then_some(record)
}

/// Where a class record keeps its vtable, which is the one word of it this
/// runtime used to fill in.
const VTABLE_OFFSET: u32 = offset_of!(RawJavaClass, ptr_vtable) as u32;

/// The vtable slots a compiled image expects its own classes at. See
/// [`KtfJvmSupport::reserve_vtable_index`].
const BUILT_IN_VTABLES: [(&str, usize); 3] = [("[C", 0), ("java/lang/String", 5), ("java/lang/Class", 10)];

/// What holds a built-in class's slot until the class itself can be resolved,
/// which is only once the JVM it lives in is up. No class's vtable is here, so
/// nothing matches it and nothing takes the slot for itself.
const VTABLE_HELD: u32 = 0xffff_ffff;

pub struct KtfJvmSupport;

impl KtfJvmSupport {
    /// `binary_name` is the jar's `client.bin*`, when the caller already knows
    /// it - see `crate::adf::client_bin_name`. Without one the jar is walked
    /// through the JVM to find it, which is what this used to do always and
    /// what cost a title with a large jar seconds of its first tick.
    pub async fn init(
        core: &mut ArmCore,
        system: &mut System,
        jar_name: Option<&str>,
        binary_name: Option<&str>,
    ) -> Result<(Jvm, Box<dyn ClassInstance>)> {
        // The context is an array of class records, which is what the index an
        // object carries reaches into. See [`KtfJvmSupport::get_vtable_index`].
        let ptr_jvm_context = Allocator::alloc(core, (VTABLE_COUNT * size_of::<RawJavaClass>()) as u32)?;
        for record in 0..VTABLE_COUNT {
            write_generic(
                core,
                ptr_jvm_context + (record * size_of::<RawJavaClass>()) as u32,
                RawJavaClass::zeroed(),
            )?;
        }

        let jvm_exception_context = KtfJvmExceptionContext {
            unk: [0; 8],
            current_java_exception_handler: 0,
            native_return_type: 0,
            native_return_value: 0,
        };
        let ptr_jvm_exception_context = Allocator::alloc(core, size_of::<KtfJvmExceptionContext>() as u32)?;
        write_generic(core, ptr_jvm_exception_context, jvm_exception_context)?;

        // The head of the handler chain is thread state, not process state.
        // The AOT runtime links a record per `try` through this one word, whose
        // address the module is handed at `fn_init`, so left shared every
        // thread's records went onto one chain: a throw on one thread found a
        // catch belonging to a frame on another thread's stack, and the unwind
        // - rightly - refused to resume a frame that was not on the stack it
        // was unwinding. 지크 loses its sound thread to that the moment two
        // threads are running, and with it the game.
        core.register_thread_local_word(ptr_jvm_exception_context + EXCEPTION_HANDLER_HEAD_OFFSET)?;

        // One per class record, which is what indexes this.
        let ptr_vtable_classes = Allocator::alloc(core, (VTABLE_COUNT * size_of::<u32>()) as u32)?;
        for record in 0..VTABLE_COUNT {
            write_generic(core, ptr_vtable_classes + (record * size_of::<u32>()) as u32, 0u32)?;
        }

        let context_data = KtfJvmSupportContext {
            ptr_class_records: ptr_jvm_context,
            ptr_jvm_exception_context,
            ptr_vtable_classes,
        };
        write_generic(core, SUPPORT_CONTEXT_BASE, context_data)?;

        // Before the JVM starts: the first classes it loads would otherwise
        // take these, and an object already carrying an index cannot be told
        // that its class has moved.
        for (_, index) in BUILT_IN_VTABLES {
            write_generic(core, Self::class_record(&context_data, index as u32) + VTABLE_OFFSET, VTABLE_HELD)?;
        }

        // KTF's own vendor classes go alongside the shared WIPI-Java and MIDP
        // ones: an LGT or SKT title loads the shared two and not these.
        let ktf_protos: Box<[_]> = Box::new([
            DMInfo::as_proto(),
            GForm::as_proto(),
            GMenubarForm::as_proto(),
            GMsgBox::as_proto(),
            GTextField::as_proto(),
            GTextListener::as_proto(),
        ]);
        let protos = [wie_wipi_java::get_protos().into(), wie_midp::get_protos().into(), ktf_protos];
        let jvm_implementation = KtfJvmImplementation::new(core);
        let jvm = JvmSupport::new_jvm(system, jar_name, Box::new(protos), &[], jvm_implementation.clone()).await?;
        register_java_interface_svc_handler(core, &jvm)?;

        for (name, index) in BUILT_IN_VTABLES {
            Self::reserve_vtable_index(core, &jvm, name, index).await?;
        }

        let system_class_loader: Box<dyn ClassInstance> = jvm
            .invoke_static("java/lang/ClassLoader", "getSystemClassLoader", "()Ljava/lang/ClassLoader;", [])
            .await
            .unwrap();

        // used in tests
        if jar_name.is_none() {
            return Ok((jvm, system_class_loader));
        }

        // find client.bin
        let binary_name = match binary_name {
            Some(name) => JavaLangString::from_rust_string(&jvm, name).await.unwrap(),
            None => Self::find_client_bin(&jvm, jar_name.unwrap()).await?,
        };

        let class_loader_class = JavaClassDefinition::new(
            core,
            &jvm,
            KtfClassLoader::as_proto(),
            Box::new(ClassLoaderContext {
                core: core.clone(),
                system: system.clone(),
            }) as Box<_>,
            jvm_implementation.java_functions(),
        )
        .await?;

        jvm.register_class(Box::new(class_loader_class), None).await.unwrap();

        // The loader's constructor runs the title's own WIPI init, so a class the
        // title wants and we do not have surfaces here. Carrying it out as a
        // WieError names the class; unwrapping made it a panic with nothing but
        // "에뮬레이터 내부 오류" to show for it.
        let class_loader = match jvm
            .new_class(
                "net/wie/KtfClassLoader",
                "(Ljava/lang/ClassLoader;Ljava/lang/String;II)V",
                (system_class_loader, binary_name, ptr_jvm_context as i32, ptr_jvm_exception_context as i32),
            )
            .await
        {
            Ok(x) => x,
            Err(x) => return Err(JvmSupport::to_wie_err(&jvm, x).await),
        };

        Ok((jvm, class_loader))
    }

    /// The jar's `client.bin*`, found by walking it through the JVM.
    ///
    /// The slow way, kept for a caller that has no name to give - every step
    /// builds a JarEntry, a ZipEntry and a String in guest memory. See
    /// `crate::adf::client_bin_name` for the fast one.
    async fn find_client_bin(jvm: &Jvm, jar_name: &str) -> Result<Box<dyn ClassInstance>> {
        let jar_name_java = JavaLangString::from_rust_string(jvm, jar_name).await.unwrap();
        let jar_file = jvm
            .new_class("java/util/jar/JarFile", "(Ljava/lang/String;)V", (jar_name_java,))
            .await
            .unwrap();
        let entries: ClassInstanceRef<Enumeration> = jvm.invoke_virtual(&jar_file, "entries", "()Ljava/util/Enumeration;", []).await.unwrap();

        loop {
            let has_more_elements: bool = jvm.invoke_virtual(&entries, "hasMoreElements", "()Z", []).await.unwrap();
            if !has_more_elements {
                return Err(WieError::FatalError("client.bin not found".into()));
            }

            let entry: ClassInstanceRef<JarEntry> = jvm.invoke_virtual(&entries, "nextElement", "()Ljava/lang/Object;", []).await.unwrap();
            let name = jvm.invoke_virtual(&entry, "getName", "()Ljava/lang/String;", []).await.unwrap();
            let name_rust = JavaLangString::to_rust_string(jvm, &name).await.unwrap();

            if name_rust.starts_with("client.bin") {
                return Ok(name);
            }
        }
    }

    pub fn class_definition_raw(definition: &dyn ClassDefinition) -> Result<u32> {
        Ok(if let Some(x) = definition.as_any().downcast_ref::<JavaClassDefinition>() {
            x.ptr_raw
        } else {
            let class = definition.as_any().downcast_ref::<JavaArrayClassDefinition>().unwrap();

            class.class.ptr_raw
        })
    }

    pub fn class_from_raw(core: &ArmCore, ptr_class: u32) -> JavaClassDefinition {
        JavaClassDefinition::from_raw(ptr_class, core)
    }

    pub fn class_instance_from_raw(core: &ArmCore, ptr_instance: u32) -> JavaClassInstance {
        JavaClassInstance::from_raw(ptr_instance, core)
    }

    pub fn read_name(core: &ArmCore, ptr_name: u32) -> Result<Arc<JavaFullName>> {
        JavaFullName::from_ptr(core, ptr_name)
    }

    #[allow(clippy::borrowed_box)]
    pub fn class_instance_raw(instance: &Box<dyn ClassInstance>) -> u32 {
        if let Some(x) = instance.as_any().downcast_ref::<JavaClassInstance>() {
            x.ptr_raw
        } else {
            let instance = instance.as_any().downcast_ref::<JavaArrayClassInstance>().unwrap();

            instance.class_instance.ptr_raw
        }
    }

    /// Where a native compiled into the title's own module leaves its answer.
    ///
    /// See `KtfJvmExceptionContext::native_return_type`.
    pub fn native_return_slot(core: &ArmCore) -> Result<u32> {
        let context_data: KtfJvmSupportContext = read_generic(core, SUPPORT_CONTEXT_BASE)?;

        Ok(context_data.ptr_jvm_exception_context)
    }

    /// Where the record a vtable index names sits.
    ///
    /// The index is a word offset from the JVM context, which is the head of
    /// the array of records; see [`KtfJvmSupport::get_vtable_index`].
    fn class_record(context_data: &KtfJvmSupportContext, index: u32) -> u32 {
        context_data.ptr_class_records + index * size_of::<u32>() as u32
    }

    /// Makes `record` stand for `class` to the module.
    ///
    /// The module reads a class out of the JVM context whole - its vtable to
    /// dispatch through, its descriptor to ask what package it is in and which
    /// class it extends - so the record has to be a class record and not just
    /// somewhere to keep a vtable pointer. What it cannot be is the class
    /// itself: that record is wherever it already is, in the module's own
    /// image or on the heap, and the index only reaches inside the context.
    ///
    /// So the record is given a descriptor of its own, a copy of the class's
    /// with the class itself as its parent. A module walking up from the
    /// record - 시네마타이쿤's `com/mc/b.paint` does, to settle whether
    /// `lib/h` may touch a protected field of `game/i` - meets the real class
    /// on its first step and the real hierarchy after it. Left as it was, a
    /// vtable pointer with whatever the neighbouring records held around it,
    /// that walk read `[descriptor]` out of another class's vtable count and
    /// took the emulator down.
    fn write_class_record(core: &mut ArmCore, record: u32, class: &JavaClassDefinition) -> Result<()> {
        let raw: RawJavaClass = read_generic(core, class.ptr_raw)?;
        let mut descriptor: RawJavaClassDescriptor = read_generic(core, raw.ptr_descriptor)?;
        descriptor.ptr_parent_class = class.ptr_raw;

        let ptr_descriptor = Allocator::alloc(core, size_of::<RawJavaClassDescriptor>() as u32)?;
        write_generic(core, ptr_descriptor, descriptor)?;

        write_generic(core, record, RawJavaClass { ptr_descriptor, ..raw })
    }

    /// The index the module is to carry for `class`, in an object's first
    /// field shifted up by five.
    ///
    /// It is a word offset from the JVM context, and what it names there is a
    /// whole class record - so the indices are the multiples of
    /// [`VTABLE_STRIDE`], one per record.
    pub fn get_vtable_index(core: &mut ArmCore, class: &JavaClassDefinition) -> Result<u32> {
        // TODO remove context
        let context_data: KtfJvmSupportContext = read_generic(core, SUPPORT_CONTEXT_BASE)?;

        // Which class a record stands for, not which vtable it holds: an array
        // class has no vtable of its own, so asked by vtable every one of them
        // matched the first record still empty - and took an index whose
        // record nothing had written. A module that reads the class out of
        // such a record finds a descriptor of zero and goes on to read the
        // element type at 0x14 of it, which is where `FtFinal.startApp` and
        // `ED4.startApp` die the moment either makes an array.
        let mut free = None;
        for record in 0..VTABLE_COUNT {
            let index = (record * VTABLE_STRIDE) as u32;
            let registered: u32 = read_generic(core, context_data.ptr_vtable_classes + (record * size_of::<u32>()) as u32)?;

            if registered == class.ptr_raw {
                return Ok(index);
            }

            if registered == 0 && free.is_none() {
                // Not one a built-in class is being kept, which is held until
                // the JVM is up enough to resolve it. See
                // [`KtfJvmSupport::reserve_vtable_index`].
                let held: u32 = read_generic(core, Self::class_record(&context_data, index) + VTABLE_OFFSET)?;
                if held == 0 {
                    free = Some(index);
                }
            }
        }

        let Some(index) = free else {
            return Err(WieError::FatalError(format!("no room for a {} vtable", class.name()?)));
        };
        Self::write_class_record(core, Self::class_record(&context_data, index), class)?;
        write_generic(
            core,
            context_data.ptr_vtable_classes + (index as usize / VTABLE_STRIDE * size_of::<u32>()) as u32,
            class.ptr_raw,
        )?;

        Ok(index)
    }

    /// Keeps `name`'s vtable at `index`, where a module expects to find it.
    ///
    /// An object a title's own image carries - a string constant, the char
    /// array behind it, a class record - already holds the index of its
    /// class's vtable, written when the title was compiled. Those are the KTF
    /// VM's own: `[C` at 0, `java/lang/String` at 5, `java/lang/Class` at 10,
    /// which is what 텐가이's 215 string constants, their 215 char arrays and
    /// its 22 class records say. So this runtime puts them there too, rather
    /// than rewriting every object in the image.
    ///
    /// Done before anything is instantiated, while the slots are still empty;
    /// a slot already taken by another class is left alone and said so.
    pub async fn reserve_vtable_index(core: &mut ArmCore, jvm: &Jvm, name: &str, index: usize) -> Result<()> {
        let class = match jvm.resolve_class(name).await {
            Ok(x) => x,
            Err(e) => return Err(JvmSupport::to_wie_err(jvm, e).await),
        };

        let ptr_class = Self::class_definition_raw(&*class.definition)?;
        let ptr_vtable = JavaClassDefinition::from_raw(ptr_class, core).ptr_vtable()?;

        let context_data: KtfJvmSupportContext = read_generic(core, SUPPORT_CONTEXT_BASE)?;
        let record = Self::class_record(&context_data, index as u32);

        let current: u32 = read_generic(core, record + VTABLE_OFFSET)?;
        if current != 0 && current != VTABLE_HELD && current != ptr_vtable {
            tracing::warn!("vtable {index} is taken, so {name} is not where a compiled image looks for it");

            return Ok(());
        }

        Self::write_class_record(core, record, &JavaClassDefinition::from_raw(ptr_class, core))?;

        // An array class has no vtable of its own here - nothing dispatches
        // through one - so the record keeps its holder, which is what stops
        // another class taking it.
        if ptr_vtable == 0 {
            write_generic(core, record + VTABLE_OFFSET, VTABLE_HELD)?;
        }

        write_generic(
            core,
            context_data.ptr_vtable_classes + (index / VTABLE_STRIDE * size_of::<u32>()) as u32,
            ptr_class,
        )?;

        Ok(())
    }

    /// The class an object's first field names.
    ///
    /// Every KTF object carries its vtable's index there, shifted up by five -
    /// see `JavaClassInstance::instantiate` - and that is all an object in a
    /// relocated module's own image carries: those have no room for the class
    /// pointer this runtime's own objects keep beside their fields, so the
    /// index is the only way back to the class. The index names a record of
    /// the JVM context, and the class it stands for is what this answers -
    /// zero for an index no record was written at.
    pub fn class_by_vtable_word(core: &mut ArmCore, word: u32) -> Result<u32> {
        let Some(record) = record_of_index((word >> 5) / size_of::<u32>() as u32) else {
            return Ok(0);
        };

        let context_data: KtfJvmSupportContext = read_generic(core, SUPPORT_CONTEXT_BASE)?;

        read_generic(core, context_data.ptr_vtable_classes + (record * size_of::<u32>()) as u32)
    }

    pub fn current_java_exception_handler(core: &mut ArmCore) -> Result<u32> {
        let context_data: KtfJvmSupportContext = read_generic(core, SUPPORT_CONTEXT_BASE)?;
        let exception_context: KtfJvmExceptionContext = read_generic(core, context_data.ptr_jvm_exception_context)?;

        Ok(exception_context.current_java_exception_handler)
    }

    /// Makes `handler` the innermost handler record.
    ///
    /// A throw caught in the frame that is already innermost leaves the head
    /// where it was, but one caught further out has to pop the records it
    /// unwound past - otherwise the next throw searches a frame that has
    /// already gone.
    pub fn set_current_java_exception_handler(core: &mut ArmCore, handler: u32) -> Result<()> {
        let context_data: KtfJvmSupportContext = read_generic(core, SUPPORT_CONTEXT_BASE)?;
        let mut exception_context: KtfJvmExceptionContext = read_generic(core, context_data.ptr_jvm_exception_context)?;

        exception_context.current_java_exception_handler = handler;

        write_generic(core, context_data.ptr_jvm_exception_context, exception_context)
    }
}

#[cfg(test)]
mod test {
    use alloc::{boxed::Box, sync::Arc, vec, vec::Vec};
    use core::sync::atomic::{AtomicBool, Ordering};

    use core::mem::size_of;

    use jvm::{Jvm, runtime::JavaLangString};
    use wipi_types::ktf::java::{JavaExceptionHandler, JavaMethodDefinition, JavaMethodExceptionTableEntry};

    use wie_backend::{DefaultTaskRunner, System};
    use wie_core_arm::{Allocator, ArmCore};
    use wie_util::{Result, WieError, write_generic};

    use super::{JavaArrayClassInstance, KtfJvmSupport, method::JavaMethod};

    use test_utils::TestPlatform;

    async fn init_jvm(system: &mut System) -> Result<(Jvm, ArmCore)> {
        let mut core = ArmCore::new(false, None)?;
        Allocator::init(&mut core)?;

        let mut context = core.save_context();
        let stack = Allocator::alloc(&mut core, 0x100)?;
        context.sp = stack + 0x100;
        core.restore_context(&context);

        let (jvm, _) = KtfJvmSupport::init(&mut core, system, None, None).await?;

        Ok((jvm, core))
    }

    /// A throw that no `try` in the innermost frame covers belongs to whichever
    /// frame further out does cover it. Each protected call links its record to
    /// the one it is nested inside, and searching only the head means only the
    /// innermost `try` can ever catch.
    #[test]
    fn a_throw_is_caught_by_an_enclosing_frame() -> Result<()> {
        let mut system = System::new(Box::new(TestPlatform::new()), "", "", DefaultTaskRunner);

        let done = Arc::new(AtomicBool::new(false));
        let done_clone = done.clone();
        let mut system_clone = system.clone();

        system.spawn(async move || {
            let (jvm, mut core) = init_jvm(&mut system_clone).await?;

            // One entry, one pointer to it, one method record naming both.
            let mut build_method = |from_pc: u32, to_pc: u32, target: u32| -> Result<u32> {
                let entry = Allocator::alloc(&mut core, size_of::<JavaMethodExceptionTableEntry>() as _)?;
                write_generic(
                    &mut core,
                    entry,
                    JavaMethodExceptionTableEntry {
                        from_pc,
                        to_pc,
                        target,
                        ptr_class: 0, // catch any
                    },
                )?;

                let table = Allocator::alloc(&mut core, 4)?;
                write_generic(&mut core, table, entry)?;

                let method = Allocator::alloc(&mut core, size_of::<JavaMethodDefinition>() as _)?;
                let mut definition: JavaMethodDefinition = bytemuck::Zeroable::zeroed();
                definition.fn_body_native_or_exception_table = table;
                definition.exception_table_count = 1;
                write_generic(&mut core, method, definition)?;

                Ok(method)
            };

            let inner_method = build_method(0x10, 0x20, 0x20)?;
            let outer_method = build_method(0x200, 0x300, 0x2a0)?;

            // The restore function the unwind resumes through, read from
            // `ptr_functions + 4`.
            let functions = Allocator::alloc(&mut core, 8)?;
            write_generic(&mut core, functions, 0u32)?;
            write_generic(&mut core, functions + 4, 0xdead_beefu32)?;

            let mut build_handler = |ptr_method: u32, current_pc: u32, ptr_old_handler: u32, frame_sp: u32| -> Result<u32> {
                let handler = Allocator::alloc(&mut core, size_of::<JavaExceptionHandler>() as _)?;
                let mut record: JavaExceptionHandler = bytemuck::Zeroable::zeroed();
                record.ptr_method = ptr_method;
                record.ptr_old_handler = ptr_old_handler;
                record.current_pc = current_pc;
                record.ptr_functions = functions;
                // r4-lr, so the stack pointer is the tenth.
                record.context[9] = frame_sp;
                write_generic(&mut core, handler, record)?;

                Ok(handler)
            };

            // 0x250 is inside the outer frame's range and nowhere near the
            // inner frame's, so only the enclosing frame can catch.
            let outer = build_handler(outer_method, 0x250, 0, 0xbeef_0000)?;
            let inner = build_handler(inner_method, 0x100, outer, 0xbeef_1000)?;

            KtfJvmSupport::set_current_java_exception_handler(&mut core, inner)?;

            let exception = jvm.new_class("java/lang/Exception", "()V", ()).await.unwrap();
            let exception_raw = KtfJvmSupport::class_instance_raw(&exception);

            // A sentinel where the catch block reads what it caught, so the
            // assertion below is about the unwind writing it rather than about
            // the word happening to be right.
            write_generic(&mut core, outer + 16, 0xbaad_f00du32)?;

            let result = JavaMethod::handle_exception(&mut core, &jvm, exception).await;

            match result {
                Err(WieError::JavaExceptionUnwind {
                    context_base,
                    target,
                    next_pc,
                    frame_sp,
                }) => {
                    assert_eq!(target, 0x2a0, "caught by the enclosing frame");
                    assert_eq!(context_base, outer + 24, "the enclosing frame's saved context");
                    assert_eq!(next_pc, 0xdead_beef);
                    // Which guest call may resume this is decided against the stack
                    // pointer the catching frame saved, so the unwind has to carry it.
                    assert_eq!(frame_sp, 0xbeef_0000, "the catching frame's own stack pointer");
                }
                Err(other) => panic!("expected an unwind into the enclosing frame, got {other:?}"),
                Ok(_) => panic!("expected an unwind into the enclosing frame, got a return"),
            }

            // The frames it unwound past are gone.
            assert_eq!(KtfJvmSupport::current_java_exception_handler(&mut core)?, outer);

            // And the catch block can read what it caught.
            let caught: u32 = wie_util::read_generic(&core, outer + 16)?;
            assert_eq!(caught, exception_raw, "the record carries the exception it caught");

            // The label says the catch block is outside the region it is about to
            // leave, so a throw from inside the block does not match the entry the
            // block belongs to and jump back to its own first instruction.
            let label: u32 = wie_util::read_generic(&core, outer + 12)?;
            assert_eq!(label, 0x2a0, "the record's label is past the range it caught in");

            done_clone.store(true, Ordering::SeqCst);

            Ok(())
        });

        while !done.load(Ordering::SeqCst) {
            system.tick()?;
        }

        Ok(())
    }

    /// The JVM's collector cannot see what the guest holds, so a KTF object it
    /// calls garbage must keep its memory. Freeing on its word is what took
    /// 투스워즈 down mid-resource-load.
    #[test]
    fn a_destroyed_instance_keeps_its_guest_memory() -> Result<()> {
        let mut system = System::new(Box::new(TestPlatform::new()), "", "", DefaultTaskRunner);

        let done = Arc::new(AtomicBool::new(false));
        let done_clone = done.clone();
        let mut system_clone = system.clone();

        system.spawn(async move || {
            let (jvm, core) = init_jvm(&mut system_clone).await?;

            let mut array = jvm.instantiate_array("B", 387).await.unwrap();
            jvm.store_array(&mut array, 0, (0..387).map(|x| x as i8).collect::<Vec<_>>())
                .await
                .unwrap();
            let address = KtfJvmSupport::class_instance_raw(&array);

            jvm.destroy(array).unwrap();

            // Same address, read back from the guest as the ARM code would.
            let survivor = JavaArrayClassInstance::from_raw(address, &core);
            assert_eq!(survivor.array_length().unwrap(), 387);

            done_clone.store(true, Ordering::SeqCst);

            Ok(())
        });

        while !done.load(Ordering::SeqCst) {
            system.tick()?;
        }

        Ok(())
    }

    #[test]
    fn test_jvm_support() -> Result<()> {
        let mut system = System::new(Box::new(TestPlatform::new()), "", "", DefaultTaskRunner);

        let done = Arc::new(AtomicBool::new(false));

        let done_clone = done.clone();
        let mut system_clone = system.clone();
        system.spawn(async move || {
            let (jvm, _core) = init_jvm(&mut system_clone).await?;

            let string1 = JavaLangString::from_rust_string(&jvm, "test1").await.unwrap();
            let string2 = JavaLangString::from_rust_string(&jvm, "test2").await.unwrap();

            let string3 = jvm
                .invoke_virtual(&string1, "concat", "(Ljava/lang/String;)Ljava/lang/String;", [string2.into()])
                .await
                .unwrap();

            assert_eq!(JavaLangString::to_rust_string(&jvm, &string3).await.unwrap(), "test1test2");

            let mut array = jvm.instantiate_array("S", 10).await.unwrap();
            jvm.store_array(&mut array, 0, (0..10i16).collect::<Vec<_>>()).await.unwrap();
            let temp: Vec<i16> = jvm.load_array(&array, 5, 4).await.unwrap();

            assert_eq!(temp, vec![5, 6, 7, 8]);

            done_clone.store(true, Ordering::Relaxed);

            // test 64bit parameter passing
            let date = jvm.new_class("java/util/Date", "(J)V", (0x12345678_abcdef01i64,)).await.unwrap();
            let time: i64 = jvm.invoke_virtual(&date, "getTime", "()J", ()).await.unwrap();

            assert_eq!(time, 0x12345678_abcdef01);

            Ok(())
        });

        loop {
            system.tick()?;
            if done.load(Ordering::Relaxed) {
                break;
            }
        }

        Ok(())
    }

    #[test]
    fn test_long_array_store_load() -> Result<()> {
        let mut system = System::new(Box::new(TestPlatform::new()), "", "", DefaultTaskRunner);

        let done = Arc::new(AtomicBool::new(false));

        let done_clone = done.clone();
        let mut system_clone = system.clone();
        system.spawn(async move || {
            let (jvm, core) = init_jvm(&mut system_clone).await?;

            let values = vec![i64::MIN, -1, 0x12345678_9abcdef0, i64::MAX];

            let mut array = jvm.instantiate_array("J", 4).await.unwrap();
            jvm.store_array(&mut array, 0, values.clone()).await.unwrap();
            let loaded: Vec<i64> = jvm.load_array(&array, 0, 4).await.unwrap();

            assert_eq!(loaded, values);

            // guard against store/load flipping words symmetrically: check raw guest memory layout
            let array_instance = JavaArrayClassInstance::from_raw(KtfJvmSupport::class_instance_raw(&array), &core);
            let mut raw = [0u8; 8];
            array_instance.load_raw(16, &mut raw)?;
            assert_eq!(raw, 0x12345678_9abcdef0u64.to_le_bytes());

            done_clone.store(true, Ordering::Relaxed);

            Ok(())
        });

        loop {
            system.tick()?;
            if done.load(Ordering::Relaxed) {
                break;
            }
        }

        Ok(())
    }

    #[test]
    fn test_double_array_store_load() -> Result<()> {
        let mut system = System::new(Box::new(TestPlatform::new()), "", "", DefaultTaskRunner);

        let done = Arc::new(AtomicBool::new(false));

        let done_clone = done.clone();
        let mut system_clone = system.clone();
        system.spawn(async move || {
            let (jvm, _core) = init_jvm(&mut system_clone).await?;

            let values = vec![f64::MIN_POSITIVE, -1.5, f64::MAX];

            let mut array = jvm.instantiate_array("D", 3).await.unwrap();
            jvm.store_array(&mut array, 0, values.clone()).await.unwrap();
            let loaded: Vec<f64> = jvm.load_array(&array, 0, 3).await.unwrap();

            let to_bits = |x: &Vec<f64>| x.iter().map(|x| x.to_bits()).collect::<Vec<_>>();
            assert_eq!(to_bits(&loaded), to_bits(&values));

            done_clone.store(true, Ordering::Relaxed);

            Ok(())
        });

        loop {
            system.tick()?;
            if done.load(Ordering::Relaxed) {
                break;
            }
        }

        Ok(())
    }

    /// `wec.DMInfo` is the handset's own record, so it is one object for the
    /// life of the handset and the number it reports is the one every other
    /// question about this handset's identity is answered with. A title that
    /// asks twice, or that asks here and through `HandsetProperty`, compares
    /// the answers.
    #[test]
    fn the_handset_record_is_one_object_naming_the_one_subscriber() -> Result<()> {
        let mut system = System::new(Box::new(TestPlatform::new()), "", "", DefaultTaskRunner);

        let done = Arc::new(AtomicBool::new(false));
        let done_clone = done.clone();
        let mut system_clone = system.clone();

        system.spawn(async move || {
            let (jvm, _core) = init_jvm(&mut system_clone).await?;

            let first: Box<dyn jvm::ClassInstance> = jvm.invoke_static("wec/DMInfo", "getDMInfo", "()Lwec/DMInfo;", []).await.unwrap();
            let second: Box<dyn jvm::ClassInstance> = jvm.invoke_static("wec/DMInfo", "getDMInfo", "()Lwec/DMInfo;", []).await.unwrap();
            assert!(first.equals(&*second).unwrap(), "the handset has one record, not one per call");

            let min = jvm.invoke_virtual(&first, "gethandsetMIN", "()Ljava/lang/String;", []).await.unwrap();
            let min = JavaLangString::to_rust_string(&jvm, &min).await.unwrap();

            let name = JavaLangString::from_rust_string(&jvm, "MIN").await.unwrap();
            let through_property = jvm
                .invoke_static(
                    "org/kwis/msp/handset/HandsetProperty",
                    "getSystemProperty",
                    "(Ljava/lang/String;)Ljava/lang/String;",
                    (name,),
                )
                .await
                .unwrap();
            let through_property = JavaLangString::to_rust_string(&jvm, &through_property).await.unwrap();

            assert_eq!(min, through_property, "both identity readers name the same handset");
            assert!(!min.is_empty(), "the handset has a number to report");

            done_clone.store(true, Ordering::Relaxed);

            Ok(())
        });

        loop {
            system.tick()?;
            if done.load(Ordering::Relaxed) {
                break;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod vtable_index_tests {
    use super::{BUILT_IN_VTABLES, VTABLE_COUNT, VTABLE_STRIDE, record_of_index};

    #[test]
    fn a_class_record_is_five_words() {
        assert_eq!(VTABLE_STRIDE, 5);
    }

    #[test]
    fn the_built_in_classes_are_the_first_records() {
        let records = BUILT_IN_VTABLES.map(|(_, index)| record_of_index(index as _));

        assert_eq!(records, [Some(0), Some(1), Some(2)]);
    }

    #[test]
    fn an_index_that_falls_inside_a_record_names_no_class() {
        assert_eq!(record_of_index(4), None);
        assert_eq!(record_of_index(11), None);
    }

    #[test]
    fn an_index_past_the_last_record_names_no_class() {
        let last = (VTABLE_COUNT - 1) * VTABLE_STRIDE;

        assert_eq!(record_of_index(last as _), Some(VTABLE_COUNT - 1));
        assert_eq!(record_of_index((last + VTABLE_STRIDE) as _), None);
    }
}
