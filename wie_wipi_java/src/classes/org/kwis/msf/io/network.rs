use alloc::vec;

use java_class_proto::JavaMethodProto;
use java_constants::MethodAccessFlags;
use jvm::{ClassInstanceRef, Jvm, Result as JvmResult};

use wie_jvm_support::{WieJavaClassProto, WieJvmContext};

// class org.kwis.msf.io.Network
pub struct Network;

impl Network {
    pub fn as_proto() -> WieJavaClassProto {
        WieJavaClassProto {
            name: "org/kwis/msf/io/Network",
            parent_class: Some("java/lang/Object"),
            interfaces: vec![],
            methods: vec![
                JavaMethodProto::new("<init>", "()V", Self::init, Default::default()),
                JavaMethodProto::new("connect", "()I", Self::connect, MethodAccessFlags::NATIVE | MethodAccessFlags::STATIC),
                JavaMethodProto::new(
                    "disconnect",
                    "()V",
                    Self::disconnect,
                    MethodAccessFlags::NATIVE | MethodAccessFlags::STATIC,
                ),
            ],
            fields: vec![],
            access_flags: Default::default(),
        }
    }

    async fn init(_: &Jvm, _: &mut WieJvmContext, this: ClassInstanceRef<Self>) -> JvmResult<()> {
        tracing::debug!("org.kwis.msf.io.Network::<init>({this:?})");

        Ok(())
    }

    async fn connect(_: &Jvm, context: &mut WieJvmContext) -> JvmResult<i32> {
        // The specification's three answers: 0 when access is already available,
        // 1 when it was just established, -1 when it failed.
        //
        // 오즈-천공의 기사단 (aid 00026DBF) is the one title that needs 0 rather
        // than the usual answer. Its character-creation path calls this, and on
        // any answer but 0 ("just established" or "failed") it takes a branch
        // that ends on "서버와의 접속이 끊어졌습니다" and returns to the menu -
        // the two outcomes are only a few instructions apart in its own code. On
        // 0 it goes one call further to URL.find, whose SchemeNotFoundException it
        // catches and carries on offline into character creation (see
        // `url.rs`'s refusal). Answering 0 claims a connection this platform does
        // not have, so it is given only to this title, and every other title
        // keeps the "just established" answer it was written around.
        if context.system().aid().eq_ignore_ascii_case("00026DBF") {
            return Ok(0);
        }

        Ok(1)
    }

    async fn disconnect(_: &Jvm, _: &mut WieJvmContext) -> JvmResult<()> {
        Ok(())
    }
}
