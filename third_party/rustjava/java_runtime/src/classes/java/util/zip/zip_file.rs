use alloc::{string::ToString, sync::Arc, vec, vec::Vec};
use core::iter;

// XXX for zip..
extern crate std;
use std::io::{Cursor, Read};

use parking_lot::Mutex;
use zip::ZipArchive;

use java_class_proto::{JavaFieldProto, JavaMethodProto};
use jvm::{Array, ClassInstanceRef, Jvm, Result, runtime::JavaLangString};

use crate::{
    RuntimeClassProto, RuntimeContext,
    classes::java::{
        io::{File, InputStream},
        lang::String,
        util::{Enumeration, zip::ZipEntry},
    },
};

/// How many parsed archives [`ZipFile::get_zip_archive`] keeps. A title has
/// its own jar open and, at most, a few others.
const MAX_ARCHIVES: usize = 4;
/// How much of each end of an archive's bytes a kept archive is checked
/// against.
const FINGERPRINT_BYTES: usize = 64;

#[derive(PartialEq, Eq)]
struct Fingerprint {
    length: usize,
    head: [u8; FINGERPRINT_BYTES],
    tail: [u8; FINGERPRINT_BYTES],
}

/// The archives parsed so far, oldest first, by the identity of the array
/// each was read from.
static ARCHIVES: Mutex<Vec<(usize, Fingerprint, ZipArchive<Cursor<Arc<[u8]>>>)>> = Mutex::new(Vec::new());

// class java.util.zip.ZipFile
pub struct ZipFile;

impl ZipFile {
    pub fn as_proto() -> RuntimeClassProto {
        RuntimeClassProto {
            name: "java/util/zip/ZipFile",
            parent_class: Some("java/lang/Object"),
            interfaces: vec![],
            methods: vec![
                JavaMethodProto::new("<init>", "(Ljava/io/File;)V", Self::init, Default::default()),
                JavaMethodProto::new(
                    "getEntry",
                    "(Ljava/lang/String;)Ljava/util/zip/ZipEntry;",
                    Self::get_entry,
                    Default::default(),
                ),
                JavaMethodProto::new(
                    "getInputStream",
                    "(Ljava/util/zip/ZipEntry;)Ljava/io/InputStream;",
                    Self::get_input_stream,
                    Default::default(),
                ),
                JavaMethodProto::new("entries", "()Ljava/util/Enumeration;", Self::entries, Default::default()),
            ],
            fields: vec![JavaFieldProto::new("zipData", "[B", Default::default())],
            access_flags: Default::default(),
        }
    }

    /// This file's archive, parsed.
    ///
    /// Every entry looked up or opened used to copy the whole of `zipData` out
    /// of its Java array - clearing a buffer that size first - and parse the
    /// central directory again: twice per resource, once for `getEntry` and
    /// once for `getInputStream`. For a title's 2.6MB jar that was megabytes of
    /// copying for every few kilobytes of image it loaded, and what a scene
    /// change that loads fifty of them waited on.
    ///
    /// So an archive is parsed once and kept, against the array it came from.
    /// The array is this file's own and is never written after the constructor,
    /// but an identity can be reused once its object is gone, so a kept archive
    /// is only taken while the array still has its length and its first and
    /// last bytes.
    async fn get_zip_archive(jvm: &Jvm, this: &ClassInstanceRef<Self>) -> Result<ZipArchive<Cursor<Arc<[u8]>>>> {
        let zip_data: ClassInstanceRef<Array<i8>> = jvm.get_field(this, "zipData", "[B").await?;
        let length = jvm.array_length(&zip_data).await?;
        let identity = zip_data.identity();

        let mut fingerprint = Fingerprint {
            length,
            head: [0; FINGERPRINT_BYTES],
            tail: [0; FINGERPRINT_BYTES],
        };
        let sampled = length.min(FINGERPRINT_BYTES);
        {
            let buffer = jvm.array_raw_buffer(&zip_data).await?;
            buffer.read(0, &mut fingerprint.head[..sampled])?;
            buffer.read(length - sampled, &mut fingerprint.tail[..sampled])?;
        }

        if let Some(archive) = ARCHIVES
            .lock()
            .iter()
            .find(|(id, kept, _)| *id == identity && *kept == fingerprint)
            .map(|(_, _, archive)| archive.clone())
        {
            return Ok(archive);
        }

        let mut buf = vec![0u8; length];
        jvm.array_raw_buffer(&zip_data).await?.read(0, &mut buf)?;

        let archive = match ZipArchive::new(Cursor::new(Arc::<[u8]>::from(buf))) {
            Ok(x) => x,
            Err(err) => return Err(jvm.exception("java/util/zip/ZipException", &err.to_string()).await),
        };

        let mut archives = ARCHIVES.lock();
        archives.retain(|(id, _, _)| *id != identity);
        if archives.len() >= MAX_ARCHIVES {
            archives.remove(0);
        }
        archives.push((identity, fingerprint, archive.clone()));

        Ok(archive)
    }

    async fn init(jvm: &Jvm, _: &mut RuntimeContext, mut this: ClassInstanceRef<Self>, file: ClassInstanceRef<File>) -> Result<()> {
        tracing::debug!("java.util.zip.ZipFile::<init>({this:?}, {file:?})");

        let _: () = jvm.invoke_special(&this, "java/lang/Object", "<init>", "()V", ()).await?;

        let length: i64 = jvm.invoke_virtual(&file, "length", "()J", ()).await?;
        let is = jvm.new_class("java/io/FileInputStream", "(Ljava/io/File;)V", (file,)).await?;

        let buf = jvm.instantiate_array("B", length as _).await?;
        let _: i32 = jvm.invoke_virtual(&is, "read", "([B)I", (buf.clone(),)).await?;

        jvm.put_field(&mut this, "zipData", "[B", buf).await?;

        // the constructor throws ZipException for a malformed archive
        let _ = Self::get_zip_archive(jvm, &this).await?;

        Ok(())
    }

    async fn get_entry(
        jvm: &Jvm,
        _: &mut RuntimeContext,
        this: ClassInstanceRef<Self>,
        name: ClassInstanceRef<String>,
    ) -> Result<ClassInstanceRef<ZipEntry>> {
        tracing::debug!("java.util.zip.ZipFile::getEntry({this:?}, {name:?})");

        let entry = jvm.new_class("java/util/zip/ZipEntry", "(Ljava/lang/String;)V", (name.clone(),)).await?;
        let name = JavaLangString::to_rust_string(jvm, &name).await?;

        let mut zip = Self::get_zip_archive(jvm, &this).await?;
        let file_size = zip.by_name(&name).map(|x| x.size());

        if let Ok(x) = file_size {
            let _: () = jvm.invoke_virtual(&entry, "setSize", "(J)V", (x as i64,)).await?;

            Ok(entry.into())
        } else {
            Ok(None.into())
        }
    }

    async fn entries(jvm: &Jvm, _: &mut RuntimeContext, this: ClassInstanceRef<Self>) -> Result<ClassInstanceRef<Enumeration>> {
        tracing::debug!("java.util.zip.ZipFile::entries({this:?})");

        let zip = Self::get_zip_archive(jvm, &this).await?;
        let names = zip.file_names().map(|x| x.to_string()).collect::<Vec<_>>();

        let mut name_array = jvm.instantiate_array("Ljava/lang/String;", names.len() as _).await?;
        for (i, name) in names.iter().enumerate() {
            let name = JavaLangString::from_rust_string(jvm, name).await?;
            jvm.store_array(&mut name_array, i as _, iter::once(name)).await?;
        }

        let entries = jvm
            .new_class(
                "java/util/zip/ZipFile$Entries",
                "(Ljava/util/zip/ZipFile;[Ljava/lang/String;)V",
                (this, name_array),
            )
            .await?;

        Ok(entries.into())
    }

    async fn get_input_stream(
        jvm: &Jvm,
        _: &mut RuntimeContext,
        this: ClassInstanceRef<Self>,
        entry: ClassInstanceRef<ZipEntry>,
    ) -> Result<ClassInstanceRef<InputStream>> {
        tracing::debug!("java.util.zip.ZipFile::getInputStream({this:?}, {entry:?})");

        let entry_name = jvm.invoke_virtual(&entry, "getName", "()Ljava/lang/String;", ()).await?;
        let entry_name = JavaLangString::to_rust_string(jvm, &entry_name).await?;

        let data = {
            let mut zip = Self::get_zip_archive(jvm, &this).await?;
            let file = zip.by_name(&entry_name);
            let Ok(mut file) = file else {
                // getInputStream returns null when the entry is not in this zip
                return Ok(None.into());
            };

            let mut buf = Vec::new();
            file.read_to_end(&mut buf).map(|_| buf)
        };
        let Ok(data) = data else {
            return Err(jvm.exception("java/util/zip/ZipException", "invalid entry data").await);
        };

        // TODO do we have to use InflaterInputStream?
        let mut java_buf = jvm.instantiate_array("B", data.len() as _).await?;
        jvm.array_raw_buffer_mut(&mut java_buf).await?.write(0, &data)?;

        let input_stream = jvm.new_class("java/io/ByteArrayInputStream", "([B)V", (java_buf,)).await?;

        Ok(input_stream.into())
    }
}
