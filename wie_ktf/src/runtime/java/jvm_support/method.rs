use alloc::{
    boxed::Box,
    string::{String, ToString},
    vec,
    vec::Vec,
};
use core::{
    fmt::{self, Debug, Formatter},
    mem::size_of,
    ops::{Deref, DerefMut},
};
use futures::TryFutureExt;
use wie_jvm_support::JvmSupport;

use java_class_proto::{JavaMethodProto, MethodBody};
use java_constants::MethodAccessFlags;
use jvm::{ClassInstance, JavaError, JavaType, JavaValue, Jvm, Method, Result as JvmResult};
use wipi_types::ktf::java::{
    JavaExceptionHandler as RawJavaExceptionHandler, JavaMethodDefinition as RawJavaMethod,
    JavaMethodExceptionTableEntry as RawJavaMethodExceptionTableEntry,
};

use alloc::{format, sync::Arc};
use core::fmt::Write as _;

/// How far out a throw looks for a catch before the chain is called corrupt
/// rather than deep.
const MAX_EXCEPTION_HANDLERS: usize = 256;

/// Where in a handler record the catch block reads what it caught.
///
/// The record is built on the guest stack by the try block's prologue, which
/// leaves this word as whatever the frame underneath had there; the unwind is
/// what has to fill it in. Nothing on this side reads it, so nothing on this side
/// noticed it was never written - and a catch block reads it every time.
const EXCEPTION_OBJECT_OFFSET: u32 = 16;

/// Where in a handler record the saved registers start, which is what the
/// restore function is handed.
const EXCEPTION_CONTEXT_OFFSET: u32 = 24;

/// Where in a handler record the stack pointer its own frame saved lives - the
/// tenth of the eleven registers from r4 to lr.
///
/// It says which guest call the catch block belongs to. The guest stack is shared
/// by every nested call the host has open, so a handler saved above a call's entry
/// belongs to a caller the host has not returned to yet.
const EXCEPTION_FRAME_STACK_OFFSET: u32 = EXCEPTION_CONTEXT_OFFSET + 9 * 4;

/// Where in a handler record the label lives - which protected region of its
/// method execution is in.
///
/// The guest writes this as it moves between regions, and every exception entry's
/// target is the first label past its own range. Entering a catch block leaves the
/// region that was protected, so the record has to say so before the block runs,
/// and writing the target is the same thing as saying it.
const EXCEPTION_LABEL_OFFSET: u32 = 12;
use wie_core_arm::{
    Allocator, ArmCore, EmulatedFunction, EmulatedFunctionParam, RUN_FUNCTION_LR, RegisteredFunction, RegisteredFunctionHolder, ResultWriter,
};
use wie_util::{ByteWrite, Result, WieError, read_generic, write_generic};

use crate::runtime::java::jvm_support::JavaClassDefinition;
use crate::runtime::{
    SVC_CATEGORY_JAVA,
    init::{module_unwind, module_unwound_arguments},
    java::JavaSvcFunctions,
};

use super::{KtfJvmSupport, class_instance::JavaClassInstance, name::JavaFullName, value::JavaValueExt};

/// Bit set on the SVC id of a method's register-argument entry point, so it
/// does not collide with the parameter-block one registered under the method's
/// own address. Method records live on the emulated heap well below 2 GiB, so
/// the top bit is free.
const REGISTER_ARGS_SVC_FLAG: u32 = 0x8000_0000;

/// A [`JavaMethodProto`] with its body behind an `Arc`, so the two entry points
/// a method can have both run the same Rust implementation.
struct SharedMethodProto<C>
where
    C: ?Sized + Send,
{
    descriptor: String,
    body: Arc<dyn MethodBody<JavaError, C>>,
    access_flags: MethodAccessFlags,
}

impl<C> From<JavaMethodProto<C>> for SharedMethodProto<C>
where
    C: ?Sized + Send,
{
    fn from(proto: JavaMethodProto<C>) -> Self {
        Self {
            descriptor: proto.descriptor,
            body: Arc::from(proto.body),
            access_flags: proto.access_flags,
        }
    }
}

pub struct JavaMethod {
    pub ptr_raw: u32,
    core: ArmCore,
}

impl JavaMethod {
    pub fn from_raw(ptr_raw: u32, core: &ArmCore) -> Self {
        Self { ptr_raw, core: core.clone() }
    }

    pub fn new<C, Context>(
        core: &mut ArmCore,
        jvm: &Jvm,
        ptr_class: u32,
        proto: JavaMethodProto<C>,
        context: Context,
        java_functions: JavaSvcFunctions,
    ) -> Result<Self>
    where
        C: ?Sized + 'static + Send,
        Context: Deref<Target = C> + DerefMut + Clone + 'static + Sync + Send,
    {
        let full_name = JavaFullName {
            tag: 0,
            name: proto.name.clone(),
            descriptor: proto.descriptor.clone(),
        };
        let full_name_bytes = full_name.as_bytes();

        let ptr_name = Allocator::alloc(core, full_name_bytes.len() as u32)?;
        core.write_bytes(ptr_name, &full_name_bytes)?;

        let ptr_raw = Allocator::alloc(core, size_of::<RawJavaMethod>() as u32)?;

        // A title reads these flags back and acts on them, so the record has to
        // say what the method is rather than only what a proto happened to
        // declare. A proto carries a flag when this side needs one - STATIC to
        // decide whether there is a receiver, NATIVE to pick an entry point -
        // and carries nothing otherwise, which as a method record reads as a
        // method that is not even public.
        //
        // Every method a proto stands for is a public one: these are the
        // platform's own classes, and a title only ever reaches their public
        // API. So a proto that does not say otherwise describes a public
        // method, and the record says so.
        //
        // The SDK these titles are built with checks it before every call it
        // makes this way - `flags & (PUBLIC | STATIC | ABSTRACT)` has to come
        // out PUBLIC - and throws `java.lang.Error` when it does not. Four of
        // these five stopped on their download screen there, one instruction
        // after finding `Socket.getOutputStream`, having never called it.
        let access_flags = if proto.access_flags.intersects(MethodAccessFlags::PRIVATE | MethodAccessFlags::PROTECTED) {
            proto.access_flags
        } else {
            proto.access_flags | MethodAccessFlags::PUBLIC
        };
        let proto = SharedMethodProto::from(proto);

        // Every method gets two entry points, because the callers that reach it
        // disagree about where the arguments are. `JavaMethod::run` - the Rust
        // JVM invoking it - writes them into a parameter block and jumps to
        // `fn_body_native`, which is what KTF's own native convention does. A
        // title's AOT code jumps to whichever of the two the handset it was
        // compiled against used, and that is the handset's opinion of which
        // methods are native, not ours.
        //
        // Both directions of that disagreement have cost a title its thread.
        // 지크 calls `org.kwis.msf.io.Network.connect` - native here - the
        // ordinary way, reading `fn_body`; 미니러비's game loop calls
        // `Object.wait(J)` - ordinary Java here - through the native entry
        // point, and landed on the 0 an unflagged method used to leave there,
        // dying as "jump native address is null" on its first pass.
        //
        // So register the body twice whatever the flags say: once reading
        // arguments from the parameter block, once from registers. Both stubs
        // run the same Rust body; only where they pick the arguments up
        // differs, and each caller finds the entry point it expects. Nothing
        // else wants the word `fn_body_native` shares - a proto carries no
        // exception table, so `exception_table_count` below is always 0 and the
        // readers that would take it as a table stop on that.
        let fn_body_native = Self::register_java_method(core, jvm, ptr_raw, &proto, context.clone(), java_functions.clone(), true)?;
        let fn_body = Self::register_java_method(core, jvm, ptr_raw | REGISTER_ARGS_SVC_FLAG, &proto, context, java_functions, false)?;

        write_generic(
            core,
            ptr_raw,
            RawJavaMethod {
                fn_body,
                ptr_class,
                fn_body_native_or_exception_table: fn_body_native,
                ptr_name,
                exception_table_count: 0,
                unk3: 0,
                index_in_vtable: 0, // to be filled later
                access_flags: access_flags.bits(),
                unk6: 0,
            },
        )?;

        tracing::trace!("Wrote method {} at {ptr_raw:#x}", full_name.name);

        Ok(Self::from_raw(ptr_raw, core))
    }

    pub fn write_vtable_index(&mut self, new_index: u16) -> Result<()> {
        let mut raw: RawJavaMethod = read_generic(&self.core, self.ptr_raw)?;

        raw.index_in_vtable = new_index;

        write_generic(&mut self.core, self.ptr_raw, raw)?;

        Ok(())
    }

    pub fn ptr_class(&self) -> u32 {
        let raw: RawJavaMethod = read_generic(&self.core, self.ptr_raw).unwrap();

        raw.ptr_class
    }

    pub fn name(&self) -> Result<Arc<JavaFullName>> {
        let raw: RawJavaMethod = read_generic(&self.core, self.ptr_raw)?;

        JavaFullName::from_ptr(&self.core, raw.ptr_name)
    }

    pub async fn run(&self, args: Box<[JavaValue]>) -> Result<JavaValue> {
        let raw: RawJavaMethod = read_generic(&self.core, self.ptr_raw)?;
        let return_type = JavaType::parse(&self.descriptor()).as_method().1.clone();

        let mut core = self.core.clone();

        let mut raw_args = Vec::with_capacity(args.len());
        for arg in args.iter() {
            if matches!(arg, JavaValue::Double(_) | JavaValue::Long(_)) {
                let (arg, arg_high) = arg.as_raw64();
                raw_args.push(arg);
                raw_args.push(arg_high);
            } else {
                raw_args.push(arg.as_raw());
            }
        }

        struct JavaMethodRunResult {
            result: u32,
            result_high: u32,
        }

        impl wie_core_arm::RunFunctionResult<JavaMethodRunResult> for JavaMethodRunResult {
            fn get(core: &ArmCore) -> Self {
                let result = core.read_param(0).unwrap();
                let result_high = core.read_param(1).unwrap();

                Self { result, result_high }
            }
        }

        let access_flags = MethodAccessFlags::from_bits_truncate(raw.access_flags);

        // Re-enters `run_function` if a Java catch handler matches the current ARM frame —
        // mirrors the trampoline path in `interface.rs::map_jump_result`, but for the
        // outermost frame whose caller is the Rust JVM rather than another ARM trampoline.
        /// Runs a guest call, resuming it at each catch block that belongs to it.
        ///
        /// `entry_sp` is where the call was entered, and a catch block whose
        /// frame was saved above it belongs to a caller that has not returned
        /// yet. It is a parameter rather than something read here because it
        /// has to be the same value for every round: each resume continues
        /// *this* call rather than starting a new one, and `run_function` does
        /// not put the caller's registers back when it unwinds, so reading the
        /// stack pointer again between rounds reads the throwing frame's rather
        /// than the call's. That moved the boundary down each time, and a catch
        /// block that throws again - 지크's does, twice in twenty milliseconds -
        /// then failed the test against its own frame: the record found on the
        /// second throw was the one that had just been resumed, and its frame
        /// sat eight bytes above where the resumed continuation was running.
        async fn run_with_unwind(core: &mut ArmCore, entry_sp: u32, mut pc: u32, mut args: Vec<u32>) -> Result<JavaMethodRunResult> {
            // What the caller had. Resuming a catch block can mean loading that
            // block's own frame into the registers first, and `run_function`
            // hands back the registers it was entered with rather than the ones
            // the caller had - so they go back here after every round.
            let caller = core.save_context();

            loop {
                match core.run_function::<JavaMethodRunResult>(pc, &args).await {
                    Ok(r) => {
                        core.restore_context(&caller);

                        return Ok(r);
                    }
                    Err(WieError::JavaExceptionUnwind {
                        context_base,
                        target,
                        next_pc,
                        frame_sp,
                    }) => {
                        if frame_sp > entry_sp {
                            // Not this call's frame to resume. Leaving lets the next
                            // call out ask the same question of its own entry, and
                            // the outermost guest call owns the whole stack, so the
                            // walk ends. Resuming it here would run a caller's code
                            // inside this call, and when that caller returned the run
                            // would end with the Rust frames that made it still
                            // waiting on a guest stack that no longer exists.
                            tracing::debug!("Exception restore belongs to an outer call: frame_sp={frame_sp:#x} above entry {entry_sp:#x}");

                            return Err(WieError::JavaExceptionUnwind {
                                context_base,
                                target,
                                next_pc,
                                frame_sp,
                            });
                        }

                        tracing::debug!("Resuming via exception restore: pc={next_pc:#x}, context_base={context_base:#x}, target={target:#x}");
                        core.restore_context(&caller);
                        pc = next_pc;
                        args = module_unwound_arguments(core, context_base, target)?;
                    }
                    Err(e) => return Err(e),
                }
            }
        }

        // Read before either branch runs anything, so both describe the same
        // moment: the stack as the JVM's caller left it.
        let entry_sp = core.save_context().sp;

        let result: JavaMethodRunResult = if access_flags.contains(MethodAccessFlags::NATIVE) {
            let arg_container = Allocator::alloc(&mut core, (raw_args.len() as u32) * 4)?;
            for (i, arg) in raw_args.iter().enumerate() {
                write_generic(&mut core, arg_container + (i * 4) as u32, *arg)?;
            }

            // Name it: a native body is a bare stub address, and every one of
            // them looks alike in a capture. When a title dies inside one - 지크
            // does, in an `arraycopy` an `.ani` resource load makes - the name is
            // the difference between reading the log and guessing at it. Native
            // calls are rare enough (dozens a second, against the millions of
            // ordinary ones the trace below counts) to afford reading it.
            tracing::trace!(
                "Calling native method {}: {:#x}",
                self.name().map(|x| x.name.clone()).unwrap_or_default(),
                raw.fn_body_native_or_exception_table
            );
            let result = run_with_unwind(&mut core, entry_sp, raw.fn_body_native_or_exception_table, vec![0, arg_container]).await;

            Allocator::free(&mut core, arg_container, (raw_args.len() as u32) * 4)?;

            result?
        } else {
            let mut params = vec![0];
            params.extend(raw_args);

            tracing::trace!("Calling method: {:#x}", raw.fn_body);
            run_with_unwind(&mut core, entry_sp, raw.fn_body, params).await?
        };

        if matches!(return_type, JavaType::Double | JavaType::Long) {
            Ok(JavaValue::from_raw64(result.result, result.result_high, &return_type))
        } else {
            Ok(JavaValue::from_raw(result.result, &return_type, &core))
        }
    }

    fn exception_table(&self) -> Result<Vec<RawJavaMethodExceptionTableEntry>> {
        let raw: RawJavaMethod = read_generic(&self.core, self.ptr_raw)?;

        let mut result = Vec::with_capacity(raw.exception_table_count as _);

        if raw.exception_table_count == 0 {
            return Ok(result);
        }

        let mut cursor = raw.fn_body_native_or_exception_table;
        for _ in 0..raw.exception_table_count {
            let address = read_generic(&self.core, cursor)?;
            cursor += 4;

            result.push(read_generic(&self.core, address)?);
        }

        Ok(result)
    }

    pub async fn handle_exception(core: &mut ArmCore, jvm: &Jvm, exception: Box<dyn ClassInstance>) -> Result<JavaMethodResult> {
        tracing::warn!("Java exception thrown: {exception:?}");

        let exception_raw = KtfJvmSupport::class_instance_raw(&exception);

        // A relocated module keeps its `try` records on a chain of its own,
        // reached through the `fp` it was handed rather than through the word
        // this runtime hands an ordinary module at `fn_init`. Only one of the
        // two is ever live, and having an `fp` reserved is what says which.
        if core.reserved_fp().is_some() {
            return match module_unwind(core, jvm, &*exception, exception_raw).await? {
                Some(unwind) => Err(unwind),
                None => {
                    tracing::warn!("No try in the module's chain for {exception:?}");

                    Err(JvmSupport::to_wie_err(jvm, JavaError::JavaException(exception)).await)
                }
            };
        }

        let mut handler_address = KtfJvmSupport::current_java_exception_handler(core)?;

        // What the search looked at, kept for the case where it finds nothing.
        // "No handler" ends a run and has two very different causes - the title
        // has no catch for this, or it has one this platform did not match - and
        // only the chain itself tells them apart.
        let mut searched = String::new();
        let mut visited = Vec::new();

        // Each protected call pushes a record and links it to the one it is
        // nested inside, so a throw that no `try` in the innermost frame covers
        // belongs to whichever frame further out does cover it. Walking only the
        // head - which is what this did - means only the innermost `try` in a
        // call chain can ever catch anything.
        while handler_address != 0 {
            if visited.len() >= MAX_EXCEPTION_HANDLERS {
                return Err(WieError::FatalError(format!(
                    "Java exception handler chain exceeds {MAX_EXCEPTION_HANDLERS} records"
                )));
            }
            if handler_address % 4 != 0 {
                return Err(WieError::FatalError(format!(
                    "Java exception handler address {handler_address:#x} is not word-aligned"
                )));
            }
            if visited.contains(&handler_address) {
                return Err(WieError::FatalError(format!(
                    "Java exception handler chain cycles at {handler_address:#x}"
                )));
            }
            visited.push(handler_address);

            let exception_handler: RawJavaExceptionHandler = read_generic(core, handler_address)?;
            let method = JavaMethod::from_raw(exception_handler.ptr_method, core);
            let exception_table = method.exception_table()?;

            let _ = write!(
                searched,
                " [{}] method={:#x} label={:#x} entries={}",
                visited.len() - 1,
                exception_handler.ptr_method,
                exception_handler.current_pc,
                exception_table.len()
            );

            for entry in exception_table {
                let _ = write!(searched, " [{:#x},{:#x})->{:#x}", entry.from_pc, entry.to_pc, entry.target);

                // The range is half-open, and that is not an off-by-one: every
                // entry's target is the first label past its own range, so a
                // throw carrying `to` is a throw from after the try.
                if entry.from_pc > exception_handler.current_pc || exception_handler.current_pc >= entry.to_pc {
                    continue;
                }

                let caught = if entry.ptr_class == 0 {
                    "any".into()
                } else {
                    let class = JavaClassDefinition::from_raw(entry.ptr_class, core);
                    let name = class.name()?;
                    if !jvm.is_instance(&*exception, &name) {
                        continue;
                    }
                    name
                };

                let restore_context: u32 = read_generic(core, exception_handler.ptr_functions + 4)?;
                let contexts_base = handler_address + EXCEPTION_CONTEXT_OFFSET;

                // Name what was caught and what caught it: a resume that lands
                // in the wrong handler and a resume that lands in the right one
                // look identical without this.
                tracing::debug!(
                    "Java exception handler found: {:#x}, method: {:#x}, catches {}, pc {:#x} in [{:#x}, {:#x}), {} records out",
                    entry.target,
                    exception_handler.ptr_method,
                    caught,
                    exception_handler.current_pc,
                    entry.from_pc,
                    entry.to_pc,
                    visited.len() - 1
                );

                // The records this unwound past are gone, so the frame that
                // caught it is the innermost one from here on.
                KtfJvmSupport::set_current_java_exception_handler(core, handler_address)?;

                // And hand the catch block what it caught. Without this it reads
                // whatever the frame underneath left at that offset - the values
                // are the plausible kind, a code address or a small integer, so
                // what the title does with it is what the failure looks like:
                // rethrowing a word that is not an object, printing it, or
                // dispatching through it as an object header and faulting on a
                // wild address in the title's own helper.
                write_generic(core, handler_address + EXCEPTION_OBJECT_OFFSET, exception_raw)?;

                // The block about to run is outside the region that was
                // protected, and the label is what says which region execution is
                // in. Some catch blocks write it themselves at the top and some do
                // not, which is fine - it is this side's job on the way in. Left
                // behind, a throw from inside the catch block matches the entry the
                // block belongs to and jumps back to the block's own first
                // instruction, and does it for as long as the run lasts.
                write_generic(core, handler_address + EXCEPTION_LABEL_OFFSET, entry.target)?;

                return Err(WieError::JavaExceptionUnwind {
                    context_base: contexts_base,
                    target: entry.target,
                    next_pc: restore_context,
                    frame_sp: read_generic(core, handler_address + EXCEPTION_FRAME_STACK_OFFSET)?,
                });
            }

            handler_address = exception_handler.ptr_old_handler;
        }

        tracing::warn!(
            "No Java exception handler for {exception:?}, chain:{}",
            if searched.is_empty() { " none" } else { &searched }
        );

        Err(JvmSupport::to_wie_err(jvm, JavaError::JavaException(exception)).await)
    }

    /// Whether the native entry point at `address` is one this platform
    /// registered whose Java return type occupies two words.
    ///
    /// Only a method we implement can be answered for. A native compiled into
    /// the title's own module returns through that module's convention, and
    /// nothing here knows its descriptor.
    pub fn native_entry_returns_wide(core: &ArmCore, address: u32) -> bool {
        let Some((category, svc_id)) = core.svc_stub_id(address) else {
            return false;
        };
        if category != SVC_CATEGORY_JAVA || svc_id & REGISTER_ARGS_SVC_FLAG != 0 {
            return false;
        }
        let Ok(name) = Self::from_raw(svc_id, core).name() else {
            return false;
        };

        matches!(*JavaType::parse(&name.descriptor).as_method().1, JavaType::Long | JavaType::Double)
    }

    fn register_java_method<C, Context>(
        core: &mut ArmCore,
        jvm: &Jvm,
        svc_id: u32,
        proto: &SharedMethodProto<C>,
        context: Context,
        java_functions: JavaSvcFunctions,
        param_block_args: bool,
    ) -> Result<u32>
    where
        C: ?Sized + 'static + Send,
        Context: Deref<Target = C> + DerefMut + Clone + 'static + Sync + Send,
    {
        let java_type = JavaType::parse(&proto.descriptor);
        let (parameter_types, return_type) = java_type.as_method();

        let mut parameter_types = parameter_types.to_vec();
        if !proto.access_flags.contains(MethodAccessFlags::STATIC) {
            // TODO proper flag handling
            parameter_types.insert(0, JavaType::Class("".into())); // TODO name
        }

        let proxy = JavaMethodProxy {
            jvm: jvm.clone(),
            body: proto.body.clone(),
            param_block_args,
            context,
            parameter_types,
            return_type: return_type.clone(),
        };

        let proxy = RegisteredFunctionHolder::new(proxy, &());
        java_functions
            .lock()
            .insert(svc_id, Arc::new(Box::new(proxy) as Box<dyn RegisteredFunction>));

        core.make_svc_stub(SVC_CATEGORY_JAVA, svc_id)
    }
}

#[async_trait::async_trait]
impl Method for JavaMethod {
    fn name(&self) -> String {
        let name = self.name().unwrap();

        name.name.clone()
    }

    fn descriptor(&self) -> String {
        let name = self.name().unwrap();

        name.descriptor.clone()
    }

    async fn run(&self, jvm: &Jvm, args: Box<[JavaValue]>) -> JvmResult<JavaValue> {
        let jvm_clone = jvm.clone();
        self.run(args)
            .or_else(async move |x| {
                Err(match x {
                    WieError::JavaException(x) => JavaError::JavaException(Box::new(JavaClassInstance::from_raw(x, &self.core))),
                    WieError::JavaExceptionUnwind { .. } => {
                        jvm_clone
                            .exception("net/wie/WieError", "Java exception unwind crossed into JVM caller")
                            .await
                    }
                    _ => jvm_clone.exception("net/wie/WieError", &x.to_string()).await,
                })
            })
            .await
    }

    fn access_flags(&self) -> MethodAccessFlags {
        let raw: RawJavaMethod = read_generic(&self.core, self.ptr_raw).unwrap();

        MethodAccessFlags::from_bits_truncate(raw.access_flags)
    }
}

impl Debug for JavaMethod {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("JavaMethod").field("ptr_raw", &self.ptr_raw).finish()
    }
}

struct JavaMethodProxy<C, Context>
where
    C: ?Sized + Send,
    Context: Deref<Target = C> + DerefMut + Clone,
{
    jvm: Jvm,
    body: Arc<dyn MethodBody<JavaError, C>>,
    /// Where this entry point's caller left the arguments: in a parameter block
    /// whose address is in the first register (KTF's native convention) when
    /// true, in the registers themselves when false.
    param_block_args: bool,
    context: Context,
    parameter_types: Vec<JavaType>,
    return_type: JavaType,
}

#[async_trait::async_trait]
impl<C, Context> EmulatedFunction<(), JavaMethodResult, ()> for JavaMethodProxy<C, Context>
where
    C: ?Sized + Send,
    Context: Deref<Target = C> + DerefMut + Clone + 'static + Sync + Send,
{
    async fn call(&self, core: &mut ArmCore, _: &mut ()) -> Result<JavaMethodResult> {
        let double_long_count = self
            .parameter_types
            .iter()
            .filter(|x| matches!(x, JavaType::Double | JavaType::Long))
            .count();

        let param_count = self.parameter_types.len() + double_long_count;

        let raw_args = if self.param_block_args {
            let param_base = u32::get(core, 1);
            (0..param_count)
                .map(|x| read_generic(core, param_base + (x as u32) * 4))
                .collect::<wie_util::Result<Vec<u32>>>()?
        } else {
            (0..param_count).map(|x| u32::get(core, x + 1)).collect::<Vec<_>>()
        };

        let mut args = Vec::with_capacity(self.parameter_types.len());

        let mut it = raw_args.into_iter();
        for param in self.parameter_types.iter() {
            let arg = it.next().unwrap();

            let value = if matches!(param, JavaType::Double | JavaType::Long) {
                let arg_high = it.next().unwrap();

                JavaValue::from_raw64(arg, arg_high, param)
            } else {
                JavaValue::from_raw(arg, param, core)
            };
            args.push(value);
        }

        let mut context = self.context.clone();
        let (_, lr) = core.read_pc_lr()?;

        let result = self.body.call(&self.jvm, &mut context, args.into_boxed_slice()).await;
        if let Err(JavaError::JavaException(x)) = result {
            // if we executed this from rust code, we should propagate this down
            if lr == RUN_FUNCTION_LR {
                let java_exception = KtfJvmSupport::class_instance_raw(&x);
                return Err(WieError::JavaException(java_exception));
            }
            return JavaMethod::handle_exception(core, &self.jvm, x).await;
        }

        let result = if matches!(self.return_type, JavaType::Double | JavaType::Long) {
            let (result, result_high) = result.unwrap().as_raw64();
            vec![result, result_high]
        } else {
            vec![result.unwrap().as_raw()]
        };

        Ok(JavaMethodResult { result, next_pc: None })
    }
}

pub struct JavaMethodResult {
    result: Vec<u32>,
    next_pc: Option<u32>,
}

impl JavaMethodResult {
    pub fn new(result: Vec<u32>, next_pc: Option<u32>) -> Self {
        Self { result, next_pc }
    }
}

impl ResultWriter<JavaMethodResult> for JavaMethodResult {
    fn write(self, core: &mut ArmCore, next_pc: u32) -> Result<()> {
        core.write_return_value(&self.result)?;

        if let Some(x) = self.next_pc {
            core.set_next_pc(x)?;
        } else {
            core.set_next_pc(next_pc)?;
        }

        Ok(())
    }
}
