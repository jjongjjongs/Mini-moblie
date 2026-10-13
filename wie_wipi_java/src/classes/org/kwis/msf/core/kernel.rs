use alloc::{borrow::ToOwned, string::String as RustString, vec, vec::Vec};

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
/// Two executables are installed here: the running program itself, and - for a
/// title whose networking goes through the carrier relay - that relay
/// middleware (`com.vdigm.billcom.relay`, id `01039AD6`). 오즈-천공의 기사단's
/// KTF build lists it as a required library and its 새로하기 loads it here
/// before dialing `socket://wipiwicgsfg.magicn.com:17096`, which
/// `local_network::relay` answers in process. `load` reports success for it and
/// failure for everything else; `getExecNames` lists whichever the filters
/// select, so a title that enumerates first and loads what it finds sees the
/// two answers stay consistent.
// class org.kwis.msf.core.Kernel
pub struct Kernel;

/// The carrier relay middleware a KTF title loads for its networking.
const RELAY_DEPENDENCY: &str = "01039AD6";

/// The relay middleware's version, as its required-library entry names it.
const RELAY_DEPENDENCY_VERSION: &str = "01.00.08";

/// The program id `load` hands back for the relay. A title keeps it only to
/// tell success from the negative failure; the value is otherwise opaque, and
/// this is the one a working WIPI player returns.
const RELAY_PROGRAM_ID: i32 = 2;

/// Whether the running title is one whose relay dependency this serves.
///
/// 오즈-천공의 기사단's KTF build (aid `0103CD8A`) is the one such title. A
/// title that does not go through the relay never asks this loader for
/// `01039AD6`, so listing it only for the titles that do keeps every other
/// title's enumeration exactly as it was.
fn serves_relay(aid: &str) -> bool {
    aid.eq_ignore_ascii_case("0103CD8A")
}

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

        // The installed executables, each with the name, version and vendor a
        // filter is matched against. A null filter selects every entry; a
        // non-null one keeps only those whose field equals it.
        //
        // The program's version and vendor are not known here, so a filter on
        // either selects nothing rather than guessing a match - which is what
        // left the program listed under a null-filter enumeration before. The
        // relay's are known, so it answers a filter on them.
        let mut selected = Vec::new();

        let program_matches = name.as_deref().is_none_or(|filter| filter == program) && version.is_none() && vendor.is_none();
        if program_matches {
            selected.push(program.clone());
        }

        if serves_relay(&program) {
            let relay_matches = name.as_deref().is_none_or(|filter| filter == RELAY_DEPENDENCY)
                && version.as_deref().is_none_or(|filter| filter == RELAY_DEPENDENCY_VERSION)
                && vendor.as_deref().is_none_or(|filter| filter.is_empty());
            if relay_matches {
                selected.push(RELAY_DEPENDENCY.to_owned());
            }
        }

        let mut names = jvm.instantiate_array("Ljava/lang/String;", selected.len()).await?;
        for (index, id) in selected.iter().enumerate() {
            let value = JavaLangString::from_rust_string(jvm, id).await?;
            jvm.store_array(&mut names, index, [value]).await?;
        }

        Ok(names.into())
    }

    async fn load(
        jvm: &Jvm,
        context: &mut WieJvmContext,
        name: ClassInstanceRef<String>,
        arguments: ClassInstanceRef<Array<String>>,
    ) -> JvmResult<i32> {
        let name = Self::optional_string(jvm, name).await?;
        let has_arguments = !arguments.is_null();
        let program = context.system().aid().to_owned();

        // The carrier relay is the one sub-module a title loads this way, and
        // only for a title that actually goes through it. Answered, the title
        // dials `socket://wipiwicgsfg.magicn.com:17096` next, which
        // `local_network::relay` serves. Everything else fails with -1, the
        // value the caller reads for a module that could not be started.
        let is_relay = name.as_deref() == Some(RELAY_DEPENDENCY) && serves_relay(&program);
        let result = if is_relay { RELAY_PROGRAM_ID } else { -1 };

        tracing::info!("org.kwis.msf.core.Kernel::load({name:?}, arguments: {has_arguments}) -> {result}");

        Ok(result)
    }

    async fn optional_string(jvm: &Jvm, value: ClassInstanceRef<String>) -> JvmResult<Option<RustString>> {
        if value.is_null() {
            Ok(None)
        } else {
            Ok(Some(JavaLangString::to_rust_string(jvm, &value).await?))
        }
    }
}

#[cfg(test)]
mod test {
    use alloc::{boxed::Box, string::String as RustString, vec::Vec};

    use java_runtime::classes::java::lang::String;
    use jvm::{Array, ClassInstanceRef, Jvm, Result as JvmResult, runtime::JavaLangString};
    use test_utils::{run_jvm_test_with_aid, run_jvm_test_with_files};
    use wie_util::Result;

    use super::{RELAY_DEPENDENCY, RELAY_PROGRAM_ID, serves_relay};
    use crate::get_protos;

    /// The KTF build of 오즈-천공의 기사단 is the title the relay is served for.
    #[test]
    fn only_the_relay_title_is_recognised() {
        assert!(serves_relay("0103CD8A"));
        assert!(serves_relay("0103cd8a"), "the id is matched without case");
        assert!(!serves_relay("00026DBF"), "the LGT build has no relay");
        assert!(!serves_relay(""));
    }

    async fn exec_names(jvm: &Jvm, name: Option<&str>, version: Option<&str>, vendor: Option<&str>) -> JvmResult<Vec<RustString>> {
        async fn filter(jvm: &Jvm, value: Option<&str>) -> JvmResult<ClassInstanceRef<String>> {
            Ok(match value {
                Some(text) => JavaLangString::from_rust_string(jvm, text).await?.into(),
                None => None.into(),
            })
        }

        let names: ClassInstanceRef<Array<String>> = jvm
            .invoke_static(
                "org/kwis/msf/core/Kernel",
                "getExecNames",
                "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)[Ljava/lang/String;",
                (filter(jvm, name).await?, filter(jvm, version).await?, filter(jvm, vendor).await?),
            )
            .await?;

        let length = jvm.array_length(&names).await?;
        let elements: Vec<ClassInstanceRef<String>> = jvm.load_array(&names, 0, length).await?;
        let mut result = Vec::new();
        for element in elements {
            result.push(JavaLangString::to_rust_string(jvm, &element).await?);
        }
        Ok(result)
    }

    async fn load(jvm: &Jvm, name: &str) -> JvmResult<i32> {
        let name: ClassInstanceRef<String> = JavaLangString::from_rust_string(jvm, name).await?.into();
        let arguments: ClassInstanceRef<Array<String>> = jvm.instantiate_array("Ljava/lang/String;", 0).await?.into();
        jvm.invoke_static(
            "org/kwis/msf/core/Kernel",
            "load",
            "(Ljava/lang/String;[Ljava/lang/String;)I",
            (name, arguments),
        )
        .await
    }

    /// The relay title enumerates and loads the relay, and the program alone
    /// falls out of a null-filter enumeration alongside it.
    #[test]
    fn the_relay_title_finds_and_loads_the_relay() -> Result<()> {
        run_jvm_test_with_aid("0103CD8A", Box::new([get_protos().into()]), |jvm| async move {
            let all = exec_names(&jvm, None, None, None).await?;
            assert!(all.iter().any(|id| id == "0103CD8A"), "the program is listed");
            assert!(all.iter().any(|id| id == RELAY_DEPENDENCY), "the relay is listed");

            // A filter on the relay's own name, version and vendor selects it.
            let relay = exec_names(&jvm, Some(RELAY_DEPENDENCY), Some("01.00.08"), Some("")).await?;
            assert_eq!(relay, [RELAY_DEPENDENCY]);

            assert_eq!(load(&jvm, RELAY_DEPENDENCY).await?, RELAY_PROGRAM_ID, "the relay loads");
            assert_eq!(load(&jvm, "DEADBEEF").await?, -1, "nothing else loads");

            Ok(())
        })
    }

    /// A title that does not go through the relay sees neither: its enumeration
    /// is the program alone and the relay does not load.
    #[test]
    fn another_title_is_left_as_it_was() -> Result<()> {
        run_jvm_test_with_files(Box::new([get_protos().into()]), &[], |jvm| async move {
            // The default test title has the empty aid, which is not the relay's.
            let all = exec_names(&jvm, None, None, None).await?;
            assert!(!all.iter().any(|id| id == RELAY_DEPENDENCY), "no relay is offered");

            assert_eq!(load(&jvm, RELAY_DEPENDENCY).await?, -1, "the relay does not load");

            Ok(())
        })
    }
}
