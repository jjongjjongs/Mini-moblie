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
        // 오즈-천공의 기사단 (aid 00026DBF) has no offline branch behind a
        // "connected" result: told it is online it opens its own game server and,
        // when that server (gone for years) does not answer, drops to the menu
        // with "서버와의 접속이 끊어졌습니다". The specification's answer for a
        // handset with no coverage is -1 - the attempt failed - which is what
        // sends such a title down its offline path instead, on to character
        // creation offline. A working WIPI player answers -1 here for every
        // title; this keeps the online result other titles were given and refuses
        // only the one known to need it, so nothing else is disturbed.
        if context.system().aid().eq_ignore_ascii_case("00026DBF") {
            return Ok(-1);
        }

        Ok(1)
    }

    async fn disconnect(_: &Jvm, _: &mut WieJvmContext) -> JvmResult<()> {
        Ok(())
    }
}
