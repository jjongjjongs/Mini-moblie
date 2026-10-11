use alloc::vec;

use java_class_proto::{JavaFieldProto, JavaMethodProto};
use java_runtime::classes::java::{io::FileDescriptor, lang::String};
use jvm::{ClassInstanceRef, Jvm, Result as JvmResult, runtime::JavaLangString};

use wie_jvm_support::{WieJavaClassProto, WieJvmContext};

use crate::classes::com::xce::io::x_file::XFile;

// class com.xce.io.FileOutputStream
pub struct FileOutputStream;

impl FileOutputStream {
    pub fn as_proto() -> WieJavaClassProto {
        WieJavaClassProto {
            name: "com/xce/io/FileOutputStream",
            parent_class: Some("java/io/OutputStream"),
            interfaces: vec![],
            methods: vec![
                JavaMethodProto::new("<init>", "(Ljava/lang/String;)V", Self::init, Default::default()),
                JavaMethodProto::new("<init>", "(Ljava/lang/String;Z)V", Self::init_append, Default::default()),
                JavaMethodProto::new("<init>", "(Lcom/xce/io/XFile;)V", Self::init_with_file, Default::default()),
                JavaMethodProto::new("write", "(I)V", Self::write, Default::default()),
                JavaMethodProto::new("close", "()V", Self::close, Default::default()),
            ],
            fields: vec![JavaFieldProto::new("os", "Ljava/io/OutputStream;", Default::default())],
            access_flags: Default::default(),
        }
    }

    async fn init(jvm: &Jvm, _context: &mut WieJvmContext, mut this: ClassInstanceRef<Self>, name: ClassInstanceRef<String>) -> JvmResult<()> {
        tracing::debug!("com.xce.io.FileOutputStream::<init>({this:?}, {name:?})");

        let _: () = jvm.invoke_special(&this, "java/io/OutputStream", "<init>", "()V", ()).await?;

        let file = jvm.new_class("java/io/File", "(Ljava/lang/String;)V", (name,)).await?;
        let os = jvm.new_class("java/io/FileOutputStream", "(Ljava/io/File;)V", (file,)).await?;

        jvm.put_field(&mut this, "os", "Ljava/io/OutputStream;", os).await?;

        Ok(())
    }

    /// Opens `name` to be written, `append` deciding whether the writing starts
    /// at the end of what is there or replaces it.
    ///
    /// 몬스터보이 saves by opening its store this way with `append` true and
    /// writing a record onto the end, on the thread that stands behind its
    /// "로딩쭝..." between maps; without the constructor that thread died on a
    /// `NoSuchMethodError` and the loading box never cleared. A writable handle
    /// here starts its cursor at the front and leaves what is already in the file
    /// alone, so append seeks to the end first and a non-append open clears the
    /// file so the old contents cannot trail past the new. Both go through a
    /// `RandomAccessFile`, whose descriptor this stream then writes through.
    async fn init_append(
        jvm: &Jvm,
        _context: &mut WieJvmContext,
        mut this: ClassInstanceRef<Self>,
        name: ClassInstanceRef<String>,
        append: bool,
    ) -> JvmResult<()> {
        tracing::debug!("com.xce.io.FileOutputStream::<init>({this:?}, {name:?}, {append})");

        let _: () = jvm.invoke_special(&this, "java/io/OutputStream", "<init>", "()V", ()).await?;

        let mode = JavaLangString::from_rust_string(jvm, "rw").await?;
        let raf = jvm
            .new_class("java/io/RandomAccessFile", "(Ljava/lang/String;Ljava/lang/String;)V", (name, mode))
            .await?;

        if append {
            let length: i64 = jvm.invoke_virtual(&raf, "length", "()J", ()).await?;
            let _: () = jvm.invoke_virtual(&raf, "seek", "(J)V", (length,)).await?;
        } else {
            let _: () = jvm.invoke_virtual(&raf, "setLength", "(J)V", (0i64,)).await?;
            let _: () = jvm.invoke_virtual(&raf, "seek", "(J)V", (0i64,)).await?;
        }

        let fd: ClassInstanceRef<FileDescriptor> = jvm.invoke_virtual(&raf, "getFD", "()Ljava/io/FileDescriptor;", ()).await?;
        let os = jvm.new_class("java/io/FileOutputStream", "(Ljava/io/FileDescriptor;)V", (fd,)).await?;

        jvm.put_field(&mut this, "os", "Ljava/io/OutputStream;", os).await?;

        Ok(())
    }

    async fn init_with_file(
        jvm: &Jvm,
        _context: &mut WieJvmContext,
        mut this: ClassInstanceRef<Self>,
        file: ClassInstanceRef<XFile>,
    ) -> JvmResult<()> {
        tracing::debug!("com.xce.io.FileOutputStream::<init>({file:?})");

        let raf = XFile::raf(jvm, file).await?;
        let fd: ClassInstanceRef<FileDescriptor> = jvm.invoke_virtual(&raf, "getFD", "()Ljava/io/FileDescriptor;", ()).await?;
        let os = jvm.new_class("java/io/FileOutputStream", "(Ljava/io/FileDescriptor;)V", (fd,)).await?;

        jvm.put_field(&mut this, "os", "Ljava/io/OutputStream;", os).await?;

        Ok(())
    }

    async fn write(jvm: &Jvm, _context: &mut WieJvmContext, this: ClassInstanceRef<Self>, byte: i32) -> JvmResult<()> {
        tracing::debug!("com.xce.io.FileOutputStream::write({this:?}, {byte:?})");

        let os = jvm.get_field(&this, "os", "Ljava/io/OutputStream;").await?;
        let _: () = jvm.invoke_virtual(&os, "write", "(I)V", (byte,)).await?;

        Ok(())
    }

    async fn close(jvm: &Jvm, _context: &mut WieJvmContext, this: ClassInstanceRef<Self>) -> JvmResult<()> {
        tracing::debug!("com.xce.io.FileOutputStream::close({this:?})");

        let os = jvm.get_field(&this, "os", "Ljava/io/OutputStream;").await?;
        let _: () = jvm.invoke_virtual(&os, "close", "()V", ()).await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use alloc::{boxed::Box, vec, vec::Vec};

    use jvm::{Array, ClassInstanceRef, Jvm, Result as JvmResult, runtime::JavaLangString};

    use test_utils::run_jvm_test;
    use wie_util::Result;

    use crate::get_protos;

    fn protos() -> Box<[Box<[wie_jvm_support::WieJavaClassProto]>]> {
        Box::new([wie_midp::get_protos().into(), get_protos().into()])
    }

    /// Opens `name` through `com.xce.io.FileOutputStream(String, append)`, writes
    /// `bytes` a byte at a time, and closes.
    async fn write(jvm: &Jvm, name: &str, append: bool, bytes: &[u8]) -> JvmResult<()> {
        let name = JavaLangString::from_rust_string(jvm, name).await?;
        let out = jvm
            .new_class("com/xce/io/FileOutputStream", "(Ljava/lang/String;Z)V", (name, append))
            .await?;

        for &byte in bytes {
            let _: () = jvm.invoke_virtual(&out, "write", "(I)V", (byte as i32,)).await?;
        }
        let _: () = jvm.invoke_virtual(&out, "close", "()V", ()).await?;

        Ok(())
    }

    /// Reads the whole of `name` back through a `RandomAccessFile`.
    async fn read_all(jvm: &Jvm, name: &str) -> JvmResult<Vec<u8>> {
        let name = JavaLangString::from_rust_string(jvm, name).await?;
        let mode = JavaLangString::from_rust_string(jvm, "r").await?;
        let raf = jvm
            .new_class("java/io/RandomAccessFile", "(Ljava/lang/String;Ljava/lang/String;)V", (name, mode))
            .await?;

        let length: i64 = jvm.invoke_virtual(&raf, "length", "()J", ()).await?;
        let buffer = jvm.instantiate_array("B", length as _).await?;
        let read: i32 = jvm.invoke_virtual(&raf, "read", "([B)I", (buffer.clone(),)).await?;
        let _: () = jvm.invoke_virtual(&raf, "close", "()V", ()).await?;

        let buffer: ClassInstanceRef<Array<i8>> = buffer.into();
        let bytes: Vec<i8> = jvm.load_array(&buffer, 0, read.max(0) as _).await?;

        Ok(bytes.into_iter().map(|x| x as u8).collect())
    }

    /// A second append open writes onto the end of the first, the way a save
    /// that adds a record expects.
    #[test]
    fn append_writes_land_after_what_is_already_there() -> Result<()> {
        run_jvm_test(protos(), |jvm| async move {
            write(&jvm, "save.dat", true, b"AB").await?;
            write(&jvm, "save.dat", true, b"CD").await?;

            assert_eq!(read_all(&jvm, "save.dat").await?, b"ABCD");

            Ok(())
        })
    }

    /// A non-append open clears what was there so the old contents cannot trail
    /// past the new.
    #[test]
    fn a_non_append_open_clears_the_old_contents() -> Result<()> {
        run_jvm_test(protos(), |jvm| async move {
            write(&jvm, "save.dat", true, b"ABCD").await?;
            write(&jvm, "save.dat", false, b"X").await?;

            assert_eq!(read_all(&jvm, "save.dat").await?, b"X");

            Ok(())
        })
    }
}
