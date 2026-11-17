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
        // 오즈-천공의 기사단 is the title that needs 0 rather than the usual
        // answer, in both its builds. Its character-creation path calls this,
        // and on any answer but 0 ("just established" or "failed") it takes a
        // branch that ends on "서버와의 접속이 끊어졌습니다" and returns to the
        // menu - the two outcomes are only a few instructions apart in its own
        // code. On 0 it goes one call further and opens its server.
        //
        // The LGT build (aid 00026DBF) then reaches URL.find on a dead game
        // server, whose SchemeNotFoundException it catches to carry on offline
        // (see `url.rs`'s refusal). The KTF build (aid 0103CD8A) instead reaches
        // URL.find on its carrier relay, answered in process (see
        // `local_network::relay`), and its 새로하기 creates the character
        // through that. Both need the connection claimed here first, and
        // answering 0 claims a connection this platform does not have, so it is
        // given only to these titles; every other title keeps the "just
        // established" answer it was written around.
        let aid = context.system().aid();
        if aid.eq_ignore_ascii_case("00026DBF") || aid.eq_ignore_ascii_case("0103CD8A") {
            return Ok(0);
        }

        Ok(1)
    }

    async fn disconnect(_: &Jvm, _: &mut WieJvmContext) -> JvmResult<()> {
        Ok(())
    }
}
