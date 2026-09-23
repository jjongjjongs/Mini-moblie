use alloc::{string::String as RustString, vec};

use java_class_proto::JavaMethodProto;
use java_constants::MethodAccessFlags;
use java_runtime::classes::java::{
    io::{DataInputStream, DataOutputStream, InputStream, OutputStream},
    lang::String,
};
use jvm::{ClassInstanceRef, JavaError, Jvm, Result, runtime::JavaLangString};

use wie_jvm_support::{WieJavaClassProto, WieJvmContext};

use super::Connection;

// class javax.microedition.io.Connector
//
// The Generic Connection Framework's factory, which refuses every name with the
// `ConnectionNotFoundException` a handset with no coverage answered. Nothing
// here reaches a network, and the servers these titles were written against
// are long gone.
//
// Refusing is what lets a title recover. 코인마스터's ranking menu opens its
// socket from a thread that catches `Exception`; with the class missing that
// thread died on `NoClassDefFoundError`, an `Error` the catch does not see, and
// the menu waited for ever on a connect that had already ended. Handing back
// a connection that never delivers bytes strands a title the same way. The
// refusal reaches the title's own catch, which is the path its author wrote
// for a failed connect. (wfeature answers the same, `connector.go`.)
pub struct Connector;

impl Connector {
    pub fn as_proto() -> WieJavaClassProto {
        WieJavaClassProto {
            name: "javax/microedition/io/Connector",
            parent_class: Some("java/lang/Object"),
            interfaces: vec![],
            methods: vec![
                JavaMethodProto::new(
                    "open",
                    "(Ljava/lang/String;)Ljavax/microedition/io/Connection;",
                    Self::open,
                    MethodAccessFlags::STATIC,
                ),
                JavaMethodProto::new(
                    "open",
                    "(Ljava/lang/String;I)Ljavax/microedition/io/Connection;",
                    Self::open_with_mode,
                    MethodAccessFlags::STATIC,
                ),
                JavaMethodProto::new(
                    "open",
                    "(Ljava/lang/String;IZ)Ljavax/microedition/io/Connection;",
                    Self::open_with_mode_and_timeouts,
                    MethodAccessFlags::STATIC,
                ),
                JavaMethodProto::new(
                    "openInputStream",
                    "(Ljava/lang/String;)Ljava/io/InputStream;",
                    Self::open_input_stream,
                    MethodAccessFlags::STATIC,
                ),
                JavaMethodProto::new(
                    "openDataInputStream",
                    "(Ljava/lang/String;)Ljava/io/DataInputStream;",
                    Self::open_data_input_stream,
                    MethodAccessFlags::STATIC,
                ),
                JavaMethodProto::new(
                    "openOutputStream",
                    "(Ljava/lang/String;)Ljava/io/OutputStream;",
                    Self::open_output_stream,
                    MethodAccessFlags::STATIC,
                ),
                JavaMethodProto::new(
                    "openDataOutputStream",
                    "(Ljava/lang/String;)Ljava/io/DataOutputStream;",
                    Self::open_data_output_stream,
                    MethodAccessFlags::STATIC,
                ),
            ],
            fields: vec![],
            access_flags: Default::default(),
        }
    }

    async fn open(jvm: &Jvm, _: &mut WieJvmContext, name: ClassInstanceRef<String>) -> Result<ClassInstanceRef<Connection>> {
        tracing::debug!("javax.microedition.io.Connector::open({name:?})");

        Err(refuse(jvm, &name).await)
    }

    async fn open_with_mode(jvm: &Jvm, _: &mut WieJvmContext, name: ClassInstanceRef<String>, mode: i32) -> Result<ClassInstanceRef<Connection>> {
        tracing::debug!("javax.microedition.io.Connector::open({name:?}, {mode})");

        Err(refuse(jvm, &name).await)
    }

    async fn open_with_mode_and_timeouts(
        jvm: &Jvm,
        _: &mut WieJvmContext,
        name: ClassInstanceRef<String>,
        mode: i32,
        timeouts: bool,
    ) -> Result<ClassInstanceRef<Connection>> {
        tracing::debug!("javax.microedition.io.Connector::open({name:?}, {mode}, {timeouts})");

        Err(refuse(jvm, &name).await)
    }

    async fn open_input_stream(jvm: &Jvm, _: &mut WieJvmContext, name: ClassInstanceRef<String>) -> Result<ClassInstanceRef<InputStream>> {
        tracing::debug!("javax.microedition.io.Connector::openInputStream({name:?})");

        Err(refuse(jvm, &name).await)
    }

    async fn open_data_input_stream(jvm: &Jvm, _: &mut WieJvmContext, name: ClassInstanceRef<String>) -> Result<ClassInstanceRef<DataInputStream>> {
        tracing::debug!("javax.microedition.io.Connector::openDataInputStream({name:?})");

        Err(refuse(jvm, &name).await)
    }

    async fn open_output_stream(jvm: &Jvm, _: &mut WieJvmContext, name: ClassInstanceRef<String>) -> Result<ClassInstanceRef<OutputStream>> {
        tracing::debug!("javax.microedition.io.Connector::openOutputStream({name:?})");

        Err(refuse(jvm, &name).await)
    }

    async fn open_data_output_stream(jvm: &Jvm, _: &mut WieJvmContext, name: ClassInstanceRef<String>) -> Result<ClassInstanceRef<DataOutputStream>> {
        tracing::debug!("javax.microedition.io.Connector::openDataOutputStream({name:?})");

        Err(refuse(jvm, &name).await)
    }
}

/// The refusal every `Connector` entry answers with, naming what was asked for.
async fn refuse(jvm: &Jvm, name: &ClassInstanceRef<String>) -> JavaError {
    let name = if name.is_null() {
        RustString::new()
    } else {
        match JavaLangString::to_rust_string(jvm, name).await {
            Ok(name) => name,
            Err(error) => return error,
        }
    };
    tracing::info!("Refusing connection to {name:?}: no network");

    jvm.exception("javax/microedition/io/ConnectionNotFoundException", &name).await
}
