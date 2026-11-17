//! A `StreamConnection` over a connection the emulator answers for itself.
//!
//! [`Connector`](super::super::super::javax::microedition::io::Connector)
//! refuses to dial out, since the servers these titles were written against are
//! gone. But a title whose own server is answered in process by a
//! [`LocalEndpoint`](wie_backend::LocalConnection) needs a real `Connection`
//! handed back, with streams that carry its bytes to and from that endpoint.
//!
//! These three classes are that connection. They are wie-internal - a title
//! never names them - so they live under `net/wie` and hold the local
//! descriptor the way `org.kwis.msf.io.Socket` does on the WIPI side.

use alloc::vec;

use java_class_proto::{JavaFieldProto, JavaMethodProto};
use java_runtime::classes::java::io::{DataInputStream, DataOutputStream, InputStream, OutputStream};
use jvm::{Array, ClassInstanceRef, Jvm, Result as JvmResult};

use wie_jvm_support::{WieJavaClassProto, WieJvmContext};

// class net.wie.LocalStreamConnection
pub struct LocalStreamConnection;

impl LocalStreamConnection {
    pub fn as_proto() -> WieJavaClassProto {
        WieJavaClassProto {
            name: "net/wie/LocalStreamConnection",
            parent_class: Some("java/lang/Object"),
            interfaces: vec!["javax/microedition/io/StreamConnection"],
            methods: vec![
                JavaMethodProto::new("<init>", "(I)V", Self::init, Default::default()),
                JavaMethodProto::new("openInputStream", "()Ljava/io/InputStream;", Self::open_input_stream, Default::default()),
                JavaMethodProto::new(
                    "openDataInputStream",
                    "()Ljava/io/DataInputStream;",
                    Self::open_data_input_stream,
                    Default::default(),
                ),
                JavaMethodProto::new(
                    "openOutputStream",
                    "()Ljava/io/OutputStream;",
                    Self::open_output_stream,
                    Default::default(),
                ),
                JavaMethodProto::new(
                    "openDataOutputStream",
                    "()Ljava/io/DataOutputStream;",
                    Self::open_data_output_stream,
                    Default::default(),
                ),
                JavaMethodProto::new("close", "()V", Self::close, Default::default()),
            ],
            fields: vec![JavaFieldProto::new("fd", "I", Default::default())],
            access_flags: Default::default(),
        }
    }

    async fn init(jvm: &Jvm, _: &mut WieJvmContext, mut this: ClassInstanceRef<Self>, fd: i32) -> JvmResult<()> {
        tracing::debug!("net.wie.LocalStreamConnection::<init>({this:?}, {fd})");

        let _: () = jvm.invoke_special(&this, "java/lang/Object", "<init>", "()V", ()).await?;
        jvm.put_field(&mut this, "fd", "I", fd).await?;

        Ok(())
    }

    /// Binds a freshly opened local descriptor to a new connection.
    pub async fn from_descriptor(jvm: &Jvm, fd: i32) -> JvmResult<ClassInstanceRef<Self>> {
        Ok(jvm.new_class("net/wie/LocalStreamConnection", "(I)V", (fd,)).await?.into())
    }

    async fn open_input_stream(jvm: &Jvm, _: &mut WieJvmContext, this: ClassInstanceRef<Self>) -> JvmResult<ClassInstanceRef<InputStream>> {
        tracing::debug!("net.wie.LocalStreamConnection::openInputStream({this:?})");

        let fd: i32 = jvm.get_field(&this, "fd", "I").await?;
        Ok(jvm.new_class("net/wie/LocalSocketInputStream", "(I)V", (fd,)).await?.into())
    }

    async fn open_data_input_stream(
        jvm: &Jvm,
        context: &mut WieJvmContext,
        this: ClassInstanceRef<Self>,
    ) -> JvmResult<ClassInstanceRef<DataInputStream>> {
        tracing::debug!("net.wie.LocalStreamConnection::openDataInputStream({this:?})");

        let stream = Self::open_input_stream(jvm, context, this).await?;
        Ok(jvm
            .new_class("java/io/DataInputStream", "(Ljava/io/InputStream;)V", (stream,))
            .await?
            .into())
    }

    async fn open_output_stream(jvm: &Jvm, _: &mut WieJvmContext, this: ClassInstanceRef<Self>) -> JvmResult<ClassInstanceRef<OutputStream>> {
        tracing::debug!("net.wie.LocalStreamConnection::openOutputStream({this:?})");

        let fd: i32 = jvm.get_field(&this, "fd", "I").await?;
        Ok(jvm.new_class("net/wie/LocalSocketOutputStream", "(I)V", (fd,)).await?.into())
    }

    async fn open_data_output_stream(
        jvm: &Jvm,
        context: &mut WieJvmContext,
        this: ClassInstanceRef<Self>,
    ) -> JvmResult<ClassInstanceRef<DataOutputStream>> {
        tracing::debug!("net.wie.LocalStreamConnection::openDataOutputStream({this:?})");

        let stream = Self::open_output_stream(jvm, context, this).await?;
        Ok(jvm
            .new_class("java/io/DataOutputStream", "(Ljava/io/OutputStream;)V", (stream,))
            .await?
            .into())
    }

    async fn close(jvm: &Jvm, context: &mut WieJvmContext, mut this: ClassInstanceRef<Self>) -> JvmResult<()> {
        tracing::debug!("net.wie.LocalStreamConnection::close({this:?})");

        let fd: i32 = jvm.get_field(&this, "fd", "I").await?;
        if wie_backend::is_local_descriptor(fd) {
            context.system().local_network().close(fd);
        }
        jvm.put_field(&mut this, "fd", "I", -1).await
    }
}

// class net.wie.LocalSocketInputStream
pub struct LocalSocketInputStream;

impl LocalSocketInputStream {
    pub fn as_proto() -> WieJavaClassProto {
        WieJavaClassProto {
            name: "net/wie/LocalSocketInputStream",
            parent_class: Some("java/io/InputStream"),
            interfaces: vec![],
            methods: vec![
                JavaMethodProto::new("<init>", "(I)V", Self::init, Default::default()),
                JavaMethodProto::new("read", "()I", Self::read_byte, Default::default()),
                JavaMethodProto::new("read", "([BII)I", Self::read_array, Default::default()),
                JavaMethodProto::new("available", "()I", Self::available, Default::default()),
                JavaMethodProto::new("close", "()V", Self::close, Default::default()),
            ],
            fields: vec![JavaFieldProto::new("fd", "I", Default::default())],
            access_flags: Default::default(),
        }
    }

    async fn init(jvm: &Jvm, _: &mut WieJvmContext, mut this: ClassInstanceRef<Self>, fd: i32) -> JvmResult<()> {
        tracing::debug!("net.wie.LocalSocketInputStream::<init>({this:?}, {fd})");

        let _: () = jvm.invoke_special(&this, "java/io/InputStream", "<init>", "()V", ()).await?;
        jvm.put_field(&mut this, "fd", "I", fd).await?;

        Ok(())
    }

    async fn read_byte(jvm: &Jvm, context: &mut WieJvmContext, this: ClassInstanceRef<Self>) -> JvmResult<i32> {
        let mut byte = [0u8; 1];
        match Self::recv(jvm, context, &this, &mut byte).await? {
            0 => Ok(-1),
            _ => Ok(byte[0] as i32),
        }
    }

    async fn read_array(
        jvm: &Jvm,
        context: &mut WieJvmContext,
        this: ClassInstanceRef<Self>,
        mut buf: ClassInstanceRef<Array<i8>>,
        offset: i32,
        length: i32,
    ) -> JvmResult<i32> {
        tracing::debug!("net.wie.LocalSocketInputStream::read({this:?}, {buf:?}, {offset}, {length})");

        if length <= 0 {
            return Ok(0);
        }

        let mut bytes = alloc::vec![0u8; length as usize];
        let read = Self::recv(jvm, context, &this, &mut bytes).await?;
        if read == 0 {
            return Ok(-1);
        }

        let signed: alloc::vec::Vec<i8> = bytes[..read].iter().map(|&byte| byte as i8).collect();
        jvm.store_array(&mut buf, offset as _, signed).await?;

        Ok(read as i32)
    }

    /// Reads into `buf`, waiting out a would-block. Zero means the peer closed.
    async fn recv(jvm: &Jvm, context: &mut WieJvmContext, this: &ClassInstanceRef<Self>, buf: &mut [u8]) -> JvmResult<usize> {
        use wie_backend::{LocalRead, is_local_descriptor};

        let fd: i32 = jvm.get_field(this, "fd", "I").await?;

        if is_local_descriptor(fd) {
            loop {
                let read = {
                    let system = context.system();
                    let mut local_network = system.local_network();
                    local_network.read(fd, buf)
                };

                match read {
                    Some(LocalRead::Data(read)) => return Ok(read),
                    Some(LocalRead::Closed) => return Ok(0),
                    Some(LocalRead::Pending) => context.system().sleep(1).await,
                    None => return Err(jvm.exception("java/io/IOException", "Stream closed").await),
                }
            }
        }

        Err(jvm.exception("java/io/IOException", "Stream closed").await)
    }

    async fn available(_: &Jvm, _: &mut WieJvmContext, this: ClassInstanceRef<Self>) -> JvmResult<i32> {
        tracing::debug!("net.wie.LocalSocketInputStream::available({this:?})");

        Ok(0)
    }

    async fn close(jvm: &Jvm, _: &mut WieJvmContext, mut this: ClassInstanceRef<Self>) -> JvmResult<()> {
        tracing::debug!("net.wie.LocalSocketInputStream::close({this:?})");

        jvm.put_field(&mut this, "fd", "I", -1).await
    }
}

// class net.wie.LocalSocketOutputStream
pub struct LocalSocketOutputStream;

impl LocalSocketOutputStream {
    pub fn as_proto() -> WieJavaClassProto {
        WieJavaClassProto {
            name: "net/wie/LocalSocketOutputStream",
            parent_class: Some("java/io/OutputStream"),
            interfaces: vec![],
            methods: vec![
                JavaMethodProto::new("<init>", "(I)V", Self::init, Default::default()),
                JavaMethodProto::new("write", "(I)V", Self::write_byte, Default::default()),
                JavaMethodProto::new("write", "([BII)V", Self::write_array, Default::default()),
                JavaMethodProto::new("flush", "()V", Self::flush, Default::default()),
                JavaMethodProto::new("close", "()V", Self::close, Default::default()),
            ],
            fields: vec![JavaFieldProto::new("fd", "I", Default::default())],
            access_flags: Default::default(),
        }
    }

    async fn init(jvm: &Jvm, _: &mut WieJvmContext, mut this: ClassInstanceRef<Self>, fd: i32) -> JvmResult<()> {
        tracing::debug!("net.wie.LocalSocketOutputStream::<init>({this:?}, {fd})");

        let _: () = jvm.invoke_special(&this, "java/io/OutputStream", "<init>", "()V", ()).await?;
        jvm.put_field(&mut this, "fd", "I", fd).await?;

        Ok(())
    }

    async fn write_byte(jvm: &Jvm, context: &mut WieJvmContext, this: ClassInstanceRef<Self>, byte: i32) -> JvmResult<()> {
        Self::send(jvm, context, &this, &[byte as u8]).await
    }

    async fn write_array(
        jvm: &Jvm,
        context: &mut WieJvmContext,
        this: ClassInstanceRef<Self>,
        buf: ClassInstanceRef<Array<i8>>,
        offset: i32,
        length: i32,
    ) -> JvmResult<()> {
        tracing::debug!("net.wie.LocalSocketOutputStream::write({this:?}, {buf:?}, {offset}, {length})");

        if length <= 0 {
            return Ok(());
        }

        let signed: alloc::vec::Vec<i8> = jvm.load_array(&buf, offset as _, length as _).await?;
        let bytes: alloc::vec::Vec<u8> = signed.into_iter().map(|byte| byte as u8).collect();

        Self::send(jvm, context, &this, &bytes).await
    }

    /// Hands `bytes` to the endpoint. A local connection takes the whole write
    /// at once, so there is nothing to block on.
    async fn send(jvm: &Jvm, context: &mut WieJvmContext, this: &ClassInstanceRef<Self>, bytes: &[u8]) -> JvmResult<()> {
        let fd: i32 = jvm.get_field(this, "fd", "I").await?;

        if wie_backend::is_local_descriptor(fd) {
            let written = {
                let system = context.system();
                let mut local_network = system.local_network();
                local_network.write(fd, bytes)
            };

            return match written {
                Some(_) => Ok(()),
                None => Err(jvm.exception("java/io/IOException", "Stream closed").await),
            };
        }

        Err(jvm.exception("java/io/IOException", "Stream closed").await)
    }

    async fn flush(_: &Jvm, _: &mut WieJvmContext, this: ClassInstanceRef<Self>) -> JvmResult<()> {
        tracing::debug!("net.wie.LocalSocketOutputStream::flush({this:?})");

        Ok(())
    }

    async fn close(jvm: &Jvm, _: &mut WieJvmContext, mut this: ClassInstanceRef<Self>) -> JvmResult<()> {
        tracing::debug!("net.wie.LocalSocketOutputStream::close({this:?})");

        jvm.put_field(&mut this, "fd", "I", -1).await
    }
}
