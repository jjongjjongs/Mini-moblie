use alloc::{borrow::ToOwned, string::String as RustString, vec};

use java_class_proto::JavaMethodProto;
use java_constants::MethodAccessFlags;
use java_runtime::classes::java::lang::String;
use jvm::{Array, ClassInstanceRef, Jvm, Result as JvmResult, runtime::JavaLangString};

use wie_jvm_support::{WieJavaClassProto, WieJvmContext};

/// `org.kwis.msf.core.Kernel` - the WIPI module loader.
///
/// A WIPI-C-over-Java title reaches its own executable and any middleware it
/// depends on through this: `getExecNames` enumerates the installed executables
/// whose name, version and vendor match the filters it is given, and `load`
/// starts one by name and hands back a program id, or a negative value when it
/// cannot.
///
/// The only executable installed here is the running program itself. The
/// middleware some titles load for their networking (the carrier relay a
/// KTF title dials through) is not served yet, so `load` reports failure for
/// everything and `getExecNames` lists the program alone. A title enumerates
/// first and loads what it finds, so the two answers stay consistent.
// class org.kwis.msf.core.Kernel
pub struct Kernel;

impl Kernel {
    pub fn as_proto() -> WieJavaClassProto {
        WieJavaClassProto {
            name: "org/kwis/msf/core/Kernel",
            parent_class: Some("java/lang/Object"),
            interfaces: vec![],
            methods: vec![
                JavaMethodProto::new("<init>", "()V", Self::init, Default::default()),
                JavaMethodProto::new(
                    "getExecNames",
                    "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)[Ljava/lang/String;",
                    Self::get_exec_names,
                    MethodAccessFlags::STATIC,
                ),
                JavaMethodProto::new("load", "(Ljava/lang/String;[Ljava/lang/String;)I", Self::load, MethodAccessFlags::STATIC),
            ],
            fields: vec![],
            access_flags: Default::default(),
        }
    }

    async fn init(_: &Jvm, _: &mut WieJvmContext, this: ClassInstanceRef<Self>) -> JvmResult<()> {
        tracing::debug!("org.kwis.msf.core.Kernel::<init>({this:?})");

        Ok(())
    }

    async fn get_exec_names(
        jvm: &Jvm,
        context: &mut WieJvmContext,
        name: ClassInstanceRef<String>,
        version: ClassInstanceRef<String>,
        vendor: ClassInstanceRef<String>,
    ) -> JvmResult<ClassInstanceRef<Array<String>>> {
        let name = Self::optional_string(jvm, name).await?;
        let version = Self::optional_string(jvm, version).await?;
        let vendor = Self::optional_string(jvm, vendor).await?;

        let program = context.system().aid().to_owned();
        tracing::info!("org.kwis.msf.core.Kernel::getExecNames({name:?}, {version:?}, {vendor:?}), program {program:?}");

        // The program is the one installed executable. A null filter selects it;
        // a name filter matches against the program id. Version and vendor are
        // not known here, so a filter on either selects nothing rather than
        // guessing a match.
        let selected = name.as_deref().is_none_or(|filter| filter == program) && version.is_none() && vendor.is_none();

        let count = if selected { 1 } else { 0 };
        let mut names = jvm.instantiate_array("Ljava/lang/String;", count).await?;
        if selected {
            let id = JavaLangString::from_rust_string(jvm, &program).await?;
            jvm.store_array(&mut names, 0, [id]).await?;
        }

        Ok(names.into())
    }

    async fn load(jvm: &Jvm, _: &mut WieJvmContext, name: ClassInstanceRef<String>, arguments: ClassInstanceRef<Array<String>>) -> JvmResult<i32> {
        let name = Self::optional_string(jvm, name).await?;
        let has_arguments = !arguments.is_null();
        tracing::info!("org.kwis.msf.core.Kernel::load({name:?}, arguments: {has_arguments}) -> -1");

        // No sub-module can be started here: the only middleware a title loads
        // this way is the carrier relay, which is not served. -1 is the failure
        // the caller reads.
        Ok(-1)
    }

    async fn optional_string(jvm: &Jvm, value: ClassInstanceRef<String>) -> JvmResult<Option<RustString>> {
        if value.is_null() {
            Ok(None)
        } else {
            Ok(Some(JavaLangString::to_rust_string(jvm, &value).await?))
        }
    }
}
