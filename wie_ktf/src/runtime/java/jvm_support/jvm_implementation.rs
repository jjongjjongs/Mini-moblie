use alloc::{boxed::Box, collections::BTreeMap, format, sync::Arc};
use core::{
    ops::{Deref, DerefMut},
    pin::Pin,
};

use java_class_proto::JavaClassProto;
use jvm::{ClassDefinition, Jvm, Result as JvmResult};
use spin::Mutex;

use wie_core_arm::ArmCore;
use wie_jvm_support::JvmImplementation;

use crate::runtime::java::{JavaSvcFunctions, register_java_svc_handler};

use super::{
    JavaArrayClassDefinition, JavaClassDefinition, class_file,
    classes::net::wie::{KtfClassLoader, module_class},
};

#[derive(Clone)]
pub struct KtfJvmImplementation {
    core: ArmCore,
    java_functions: JavaSvcFunctions,
}

impl KtfJvmImplementation {
    pub fn new(core: &mut ArmCore) -> Self {
        let java_functions = Arc::new(Mutex::new(BTreeMap::new()));
        register_java_svc_handler(core, &java_functions).unwrap();

        Self {
            core: core.clone(),
            java_functions,
        }
    }

    pub fn java_functions(&self) -> JavaSvcFunctions {
        self.java_functions.clone()
    }
}

impl JvmImplementation for KtfJvmImplementation {
    fn define_class_rust<'a, C, Context>(
        &'a self,
        jvm: &'a Jvm,
        proto: JavaClassProto<C>,
        context: Context,
    ) -> Pin<Box<dyn Future<Output = JvmResult<Box<dyn ClassDefinition>>> + Send + 'a>>
    where
        C: ?Sized + 'static + Send,
        Context: Sync + Send + DerefMut + Deref<Target = C> + Clone + 'static,
    {
        Box::pin(async move {
            Ok(Box::new(
                JavaClassDefinition::new(&mut self.core.clone(), jvm, proto, context, self.java_functions.clone())
                    .await
                    .unwrap(),
            ) as _)
        })
    }

    /// A class the ordinary class path found as a `.class` file.
    ///
    /// KTF runs no bytecode. A title's classes are compiled into its own module
    /// and the runtime asks the module for them one at a time - which is what
    /// the reference does too, and why nothing here can define a class from the
    /// bytes it was built from.
    ///
    /// Most archives carry only the module, so this was never reached. 바이러스
    /// keeps `Clet` and its card in the jar beside a client.bin that holds both,
    /// and the class path - which the loader asks before its own `findClass` -
    /// finds those first. So the bytes are read for the one thing they can
    /// still answer, the name of the class being asked for, and the module is
    /// asked for that class.
    async fn define_class_java(&self, jvm: &Jvm, data: &[u8]) -> JvmResult<Box<dyn ClassDefinition>> {
        let Some(name) = class_file::class_name(data) else {
            return Err(jvm.exception("java/lang/ClassFormatError", "not a class file").await);
        };

        let Some(fn_get_class) = KtfClassLoader::module_entry_point(jvm).await else {
            return Err(jvm
                .exception("java/lang/ClassNotFoundException", &format!("{name}: no module to ask"))
                .await);
        };

        match module_class(&mut self.core.clone(), fn_get_class, &name).await? {
            Some(class) => Ok(Box::new(class) as _),
            None => Err(jvm.exception("java/lang/ClassNotFoundException", &name).await),
        }
    }

    async fn define_array_class(&self, jvm: &Jvm, element_type_name: &str) -> JvmResult<Box<dyn ClassDefinition>> {
        let class_name = format!("[{element_type_name}");
        let class = JavaArrayClassDefinition::new(&mut self.core.clone(), jvm, &class_name).await.unwrap();

        Ok(Box::new(class) as Box<_>)
    }
}
