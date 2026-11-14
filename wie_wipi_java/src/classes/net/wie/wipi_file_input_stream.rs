use alloc::vec;

use java_class_proto::{JavaFieldProto, JavaMethodProto};
use java_runtime::classes::java::io::RandomAccessFile;
use jvm::{Array, ClassInstanceRef, Jvm, Result as JvmResult};

use wie_jvm_support::{WieJavaClassProto, WieJvmContext};

use crate::classes::org::kwis::msp::io::File;

/// The stream `org.kwis.msp.io.File.openInputStream` hands out.
///
/// A WIPI `File` is one open handle with one position, and a stream taken from
/// it is a view onto that handle rather than a second one: what the stream
/// reads, the file's own `read`/`tell` carry on after. A `FileInputStream` of
/// its own starts at zero and keeps its own position, which is only the same
/// thing for a title that uses one or the other.
///
/// 강철의 연금술사2 uses both on the same file. It reads `tk_evt3.dat`'s
/// 48-byte record table through a `DataInputStream` and then reads the ten
/// records with `File.read`, so with two positions every record came back
/// shifted by the table - the title parsed the shift as records of its own,
/// asked for 84,490 bytes of a 78,586-byte file, and drew what it made of
/// them: an empty `String[]` that its own `paint` then indexed.
///
/// The output side is already written this way - see [`super::WIPIFileOutputStream`].
pub struct WIPIFileInputStream;

impl WIPIFileInputStream {
    pub fn as_proto() -> WieJavaClassProto {
        WieJavaClassProto {
            name: "net/wie/WIPIFileInputStream",
            parent_class: Some("java/io/InputStream"),
            interfaces: vec![],
            methods: vec![
                JavaMethodProto::new("<init>", "(Lorg/kwis/msp/io/File;)V", Self::init, Default::default()),
                JavaMethodProto::new("read", "()I", Self::read, Default::default()),
                JavaMethodProto::new("read", "([BII)I", Self::read_with_offset_length, Default::default()),
                JavaMethodProto::new("available", "()I", Self::available, Default::default()),
                JavaMethodProto::new("close", "()V", Self::close, Default::default()),
            ],
            fields: vec![
                JavaFieldProto::new("file", "Lorg/kwis/msp/io/File;", Default::default()),
                JavaFieldProto::new("closed", "Z", Default::default()),
            ],
            access_flags: Default::default(),
        }
    }

    async fn init(jvm: &Jvm, _: &mut WieJvmContext, mut this: ClassInstanceRef<Self>, file: ClassInstanceRef<File>) -> JvmResult<()> {
        tracing::debug!("net.wie.WIPIFileInputStream::<init>({this:?}, {file:?})");

        let _: () = jvm.invoke_special(&this, "java/io/InputStream", "<init>", "()V", ()).await?;
        jvm.put_field(&mut this, "file", "Lorg/kwis/msp/io/File;", file).await?;
        jvm.put_field(&mut this, "closed", "Z", false).await?;

        Ok(())
    }

    /// The handle this stream reads through, or an error once either end of it
    /// is closed.
    async fn raf(jvm: &Jvm, this: &ClassInstanceRef<Self>) -> JvmResult<ClassInstanceRef<RandomAccessFile>> {
        let stream_closed: bool = jvm.get_field(this, "closed", "Z").await?;
        let file: ClassInstanceRef<File> = jvm.get_field(this, "file", "Lorg/kwis/msp/io/File;").await?;
        let file_closed: bool = jvm.get_field(&file, "closed", "Z").await?;
        if stream_closed || file_closed {
            return Err(jvm.exception("java/io/IOException", "Stream closed").await);
        }

        jvm.get_field(&file, "raf", "Ljava/io/RandomAccessFile;").await
    }

    async fn read(jvm: &Jvm, _: &mut WieJvmContext, this: ClassInstanceRef<Self>) -> JvmResult<i32> {
        let raf = Self::raf(jvm, &this).await?;

        let buffer = jvm.instantiate_array("B", 1).await?;
        let read: i32 = jvm.invoke_virtual(&raf, "read", "([BII)I", (buffer.clone(), 0, 1)).await?;
        if read <= 0 {
            return Ok(-1);
        }

        let mut value = [0u8; 1];
        jvm.array_raw_buffer(&buffer).await?.read(0, &mut value)?;

        Ok(value[0] as i32)
    }

    async fn read_with_offset_length(
        jvm: &Jvm,
        _: &mut WieJvmContext,
        this: ClassInstanceRef<Self>,
        buf: ClassInstanceRef<Array<i8>>,
        offset: i32,
        length: i32,
    ) -> JvmResult<i32> {
        tracing::debug!("net.wie.WIPIFileInputStream::read({this:?}, {buf:?}, {offset}, {length})");

        let raf = Self::raf(jvm, &this).await?;

        // The end of the file is -1 here and not the exception the WIPI `File`
        // raises: this is `InputStream.read`, whose callers - `DataInputStream`
        // among them - are written to test for it.
        jvm.invoke_virtual(&raf, "read", "([BII)I", (buf, offset, length)).await
    }

    async fn available(jvm: &Jvm, _: &mut WieJvmContext, this: ClassInstanceRef<Self>) -> JvmResult<i32> {
        tracing::debug!("net.wie.WIPIFileInputStream::available({this:?})");

        let raf = Self::raf(jvm, &this).await?;

        let length: i64 = jvm.invoke_virtual(&raf, "length", "()J", ()).await?;
        let position: i64 = jvm.invoke_virtual(&raf, "getFilePointer", "()J", ()).await?;

        Ok((length - position).max(0) as i32)
    }

    async fn close(jvm: &Jvm, _: &mut WieJvmContext, mut this: ClassInstanceRef<Self>) -> JvmResult<()> {
        tracing::debug!("net.wie.WIPIFileInputStream::close({this:?})");

        // Only the stream: the handle belongs to the `File`, which a title goes
        // on reading from after it has done with a stream over it.
        jvm.put_field(&mut this, "closed", "Z", true).await?;

        Ok(())
    }
}
