use alloc::{boxed::Box, collections::BTreeMap, sync::Arc, vec::Vec};
use core::time::Duration;

use spin::Mutex;

use java_class_proto::JavaMethodProto;
use java_constants::MethodAccessFlags;
use java_runtime::{
    File, FileDescriptorId, FileSize, FileStat, FileType, IOError, IOResult, RT_RUSTJAR, Runtime, RuntimeClassProto, RuntimeContext, SpawnCallback,
    get_runtime_class_proto,
};
use jvm::{Array, ClassDefinition, ClassInstanceRef, JavaChar, Jvm, Result as JvmResult, runtime::JavaLangString};

use wie_backend::{AsyncCallable, System};
use wie_util::WieError;

use crate::{JvmImplementation, JvmSupport, WIE_RUSTJAR, WieJavaClassProto, WieJvmContext};

mod file;

use file::FileImpl;

const STDOUT_FD: u32 = 1;
const STDERR_FD: u32 = 2;

struct FileTableInner {
    files: BTreeMap<u32, Box<dyn File>>,
    next_id: u32,
}

impl FileTableInner {
    fn new() -> Self {
        Self {
            files: BTreeMap::new(),
            next_id: 3, // 0=stdin, 1=stdout, 2=stderr
        }
    }

    fn add(&mut self, file: Box<dyn File>) -> FileDescriptorId {
        let id = self.next_id;
        self.next_id += 1;
        self.files.insert(id, file);
        FileDescriptorId::new(id)
    }
}

#[derive(Clone)]
struct StdoutFile {
    system: System,
}

#[async_trait::async_trait]
impl File for StdoutFile {
    async fn read(&mut self, _buf: &mut [u8]) -> IOResult<usize> {
        Err(IOError::Unsupported)
    }

    async fn write(&mut self, buf: &[u8]) -> IOResult<usize> {
        self.system.platform().write_stdout(buf);

        Ok(buf.len())
    }

    async fn seek(&mut self, _pos: FileSize) -> IOResult<()> {
        Err(IOError::Unsupported)
    }

    async fn tell(&self) -> IOResult<FileSize> {
        Err(IOError::Unsupported)
    }

    async fn set_len(&mut self, _len: FileSize) -> IOResult<()> {
        Err(IOError::Unsupported)
    }

    async fn metadata(&self) -> IOResult<FileStat> {
        Err(IOError::Unsupported)
    }
}

#[derive(Clone)]
struct StderrFile {
    system: System,
}

#[async_trait::async_trait]
impl File for StderrFile {
    async fn read(&mut self, _buf: &mut [u8]) -> IOResult<usize> {
        Err(IOError::Unsupported)
    }

    async fn write(&mut self, buf: &[u8]) -> IOResult<usize> {
        self.system.platform().write_stderr(buf);

        Ok(buf.len())
    }

    async fn seek(&mut self, _pos: FileSize) -> IOResult<()> {
        Err(IOError::Unsupported)
    }

    async fn tell(&self) -> IOResult<FileSize> {
        Err(IOError::Unsupported)
    }

    async fn set_len(&mut self, _len: FileSize) -> IOResult<()> {
        Err(IOError::Unsupported)
    }

    async fn metadata(&self) -> IOResult<FileStat> {
        Err(IOError::Unsupported)
    }
}

#[derive(Clone)]
pub struct JvmRuntime<T>
where
    T: JvmImplementation + Sync + Send + 'static,
{
    system: System,
    implementation: T,
    protos: Arc<Mutex<Vec<WieJavaClassProto>>>,
    file_table: Arc<Mutex<FileTableInner>>,
}

impl<T> JvmRuntime<T>
where
    T: JvmImplementation + Sync + Send + 'static,
{
    pub fn new(system: System, implementation: T, protos: Box<[Box<[WieJavaClassProto]>]>) -> Self {
        let mut file_table = FileTableInner::new();
        file_table.files.insert(STDOUT_FD, Box::new(StdoutFile { system: system.clone() }));
        file_table.files.insert(STDERR_FD, Box::new(StderrFile { system: system.clone() }));

        Self {
            system,
            implementation,
            protos: Arc::new(Mutex::new(protos.into_vec().into_iter().flat_map(|x| x.into_vec()).collect())),
            file_table: Arc::new(Mutex::new(file_table)),
        }
    }
}

#[async_trait::async_trait]
impl<T> Runtime for JvmRuntime<T>
where
    T: JvmImplementation + Sync + Send + 'static,
{
    async fn sleep(&self, duration: Duration) {
        self.system.sleep(duration.as_millis() as _).await;
    }

    async fn r#yield(&self) {
        self.system.yield_now().await;
    }

    fn spawn(&self, jvm: &Jvm, callback: Box<dyn SpawnCallback>) {
        struct SpawnProxy {
            jvm: Jvm,
            callback: Box<dyn SpawnCallback>,
        }

        impl AsyncCallable<Result<(), WieError>> for SpawnProxy {
            async fn call(self) -> Result<(), WieError> {
                let result = self.callback.call().await;
                if let Err(err) = result {
                    return Err(JvmSupport::to_wie_err(&self.jvm, err).await);
                }

                Ok(())
            }
        }

        self.system.spawn(SpawnProxy { jvm: jvm.clone(), callback });
    }

    fn exit(&self, _status: i32) {
        self.system.platform().exit();
    }

    fn now(&self) -> u64 {
        self.system.platform().now().raw()
    }

    fn current_task_id(&self) -> u64 {
        self.system.current_task_id()
    }

    fn stdin(&self) -> IOResult<FileDescriptorId> {
        Err(IOError::Unsupported)
    }

    fn stdout(&self) -> IOResult<FileDescriptorId> {
        Ok(FileDescriptorId::new(STDOUT_FD))
    }

    fn stderr(&self) -> IOResult<FileDescriptorId> {
        Ok(FileDescriptorId::new(STDERR_FD))
    }

    async fn open(&self, path: &str, write: bool) -> IOResult<FileDescriptorId> {
        tracing::debug!("open({path:?}, {write:?})");

        let file = FileImpl::new(self.system.clone(), path, write).await?;
        Ok(self.file_table.lock().add(Box::new(file)))
    }

    fn get_file(&self, fd: FileDescriptorId) -> IOResult<Box<dyn File>> {
        self.file_table.lock().files.get(&fd.id()).cloned().ok_or(IOError::NotFound)
    }

    fn close_file(&self, fd: FileDescriptorId) {
        self.file_table.lock().files.remove(&fd.id());
    }

    async fn unlink(&self, path: &str) -> IOResult<()> {
        if self.system.filesystem().remove(path).await {
            Ok(())
        } else {
            Err(IOError::NotFound)
        }
    }

    async fn metadata(&self, path: &str) -> IOResult<FileStat> {
        if path.is_empty() || path.ends_with("/") {
            return Ok(FileStat {
                size: 0,
                r#type: FileType::Directory,
            });
        }

        if let Some(size) = self.system.filesystem().size(path).await {
            return Ok(FileStat {
                size: size as _,
                r#type: FileType::File,
            });
        }

        // No file of that name, but the packaged archive may still hold files
        // under it - a directory, which is what a title asking whether its data
        // is installed is asking about. See `FilesystemOverlay::is_directory`.
        if self.system.filesystem().is_directory(path).await {
            return Ok(FileStat {
                size: 0,
                r#type: FileType::Directory,
            });
        }

        Err(IOError::NotFound)
    }

    async fn find_rustjar_class(&self, jvm: &Jvm, classpath: &str, class: &str) -> JvmResult<Option<Box<dyn ClassDefinition>>> {
        if classpath == RT_RUSTJAR {
            let proto = get_runtime_class_proto(class).map(refuse_a_null_array).map(fill_in_string_buffer);
            if let Some(proto) = proto {
                return Ok(Some(
                    self.implementation
                        .define_class_rust(jvm, proto, Box::new(self.clone()) as Box<_>)
                        .await?,
                ));
            }
        } else if classpath == WIE_RUSTJAR {
            let proto_index = self.protos.lock().iter().position(|x| x.name == class);
            if let Some(proto_index) = proto_index {
                let proto = self.protos.lock().remove(proto_index);
                let context = Box::new(WieJvmContext::new(&self.system));

                return Ok(Some(self.implementation.define_class_rust(jvm, proto, context as Box<_>).await?));
            }
        }

        Ok(None)
    }

    async fn define_class(&self, jvm: &Jvm, data: &[u8]) -> JvmResult<Box<dyn ClassDefinition>> {
        self.implementation.define_class_java(jvm, data).await
    }

    async fn define_array_class(&self, _jvm: &Jvm, element_type_name: &str) -> JvmResult<Box<dyn ClassDefinition>> {
        self.implementation.define_array_class(_jvm, element_type_name).await
    }
}

/// Stand-ins for the classes the bodies below belong to, so each can name its
/// receiver the way every other proto does.
struct StringBuffer;
struct JavaString;

/// Gives `java.lang.StringBuffer` the methods the runtime does not carry.
///
/// A method the JVM cannot find is a fatal error, not a miss a title carries on
/// past: 미니러비 formats the 소행성 name plate as `append`, `length`, `insert` -
/// the zero-padding idiom - and the thread drawing the plate died on `Method
/// insert(ILjava/lang/String;)Ljava/lang/StringBuffer; not found from
/// java/lang/StringBuffer`, leaving the screen where it was. `deleteCharAt` is
/// the same class's other gap and the same formatting helpers' other half; the
/// title's constant pool names it too.
///
/// `setCharAt` is the third. 다크슬레이어2 writes its menu through it and died
/// on its own title screen, before a key was pressed.
///
/// `insert(int, int)` is the fourth, and 코에이삼국지2's - the same idiom with a
/// number rather than a string, which the runtime spells for `append` and not
/// for `insert`.
///
/// Anything that is not that class is handed back untouched, and so is a method
/// the runtime has grown since - the runtime's own is the one to keep.
fn fill_in_string_buffer(mut proto: RuntimeClassProto) -> RuntimeClassProto {
    if proto.name != "java/lang/StringBuffer" {
        return proto;
    }

    let missing = [
        JavaMethodProto::new(
            "insert",
            "(ILjava/lang/String;)Ljava/lang/StringBuffer;",
            string_buffer_insert_string,
            MethodAccessFlags::empty(),
        ),
        JavaMethodProto::new(
            "insert",
            "(II)Ljava/lang/StringBuffer;",
            string_buffer_insert_integer,
            MethodAccessFlags::empty(),
        ),
        JavaMethodProto::new(
            "deleteCharAt",
            "(I)Ljava/lang/StringBuffer;",
            string_buffer_delete_char_at,
            MethodAccessFlags::empty(),
        ),
        JavaMethodProto::new("setCharAt", "(IC)V", string_buffer_set_char_at, MethodAccessFlags::empty()),
    ];

    for method in missing {
        let present = proto.methods.iter().any(|x| x.name == method.name && x.descriptor == method.descriptor);

        if !present {
            proto.methods.push(method);
        }
    }

    proto
}

async fn string_buffer_insert_string(
    jvm: &Jvm,
    _: &mut RuntimeContext,
    this: ClassInstanceRef<StringBuffer>,
    offset: i32,
    string: ClassInstanceRef<JavaString>,
) -> JvmResult<ClassInstanceRef<StringBuffer>> {
    tracing::debug!("java.lang.StringBuffer::insert({this:?}, {offset}, {string:?})");

    // A null inserts the four letters, the way every other conversion in this
    // class spells one.
    let inserted = if string.is_null() {
        "null".into()
    } else {
        JavaLangString::to_rust_string(jvm, &string).await?
    };

    string_buffer_insert(jvm, this, offset, &inserted).await
}

/// `insert(int, int)`, which the runtime does not carry either.
///
/// 코에이삼국지2 builds its save-slot lines this way - a number written into a
/// buffer at a place the line's layout decides - and the thread that draws them
/// died on `Method insert(II)Ljava/lang/StringBuffer; not found from
/// java/lang/StringBuffer` while its title screen was still up, so the title
/// never left it.
async fn string_buffer_insert_integer(
    jvm: &Jvm,
    _: &mut RuntimeContext,
    this: ClassInstanceRef<StringBuffer>,
    offset: i32,
    value: i32,
) -> JvmResult<ClassInstanceRef<StringBuffer>> {
    tracing::debug!("java.lang.StringBuffer::insert({this:?}, {offset}, {value})");

    string_buffer_insert(jvm, this, offset, &alloc::format!("{value}")).await
}

/// Puts `inserted` into the buffer at `offset`.
async fn string_buffer_insert(
    jvm: &Jvm,
    mut this: ClassInstanceRef<StringBuffer>,
    offset: i32,
    inserted: &str,
) -> JvmResult<ClassInstanceRef<StringBuffer>> {
    let count: i32 = jvm.get_field(&this, "count", "I").await?;
    if offset < 0 || offset > count {
        return Err(jvm
            .exception("java/lang/StringIndexOutOfBoundsException", "insert offset is outside the buffer")
            .await);
    }

    let inserted = inserted.encode_utf16().collect::<Vec<_>>();

    let mut value: ClassInstanceRef<Array<JavaChar>> = jvm.get_field(&this, "value", "[C").await?;
    let chars: Vec<JavaChar> = jvm.load_array(&value, 0, count as _).await?;

    let (before, after) = chars.split_at(offset as _);
    let new_chars = before.iter().chain(inserted.iter()).chain(after.iter()).copied().collect::<Vec<_>>();

    // The buffer the class hands out is its own; it only has to be replaced
    // when what goes in no longer fits, and the doubling is the one the
    // runtime's own growth uses.
    if jvm.array_length(&value).await? < new_chars.len() {
        value = jvm.instantiate_array("C", new_chars.len() * 2).await?.into();
        jvm.put_field(&mut this, "value", "[C", value.clone()).await?;
    }

    let new_count = new_chars.len() as i32;
    jvm.store_array(&mut value, 0, new_chars).await?;
    jvm.put_field(&mut this, "count", "I", new_count).await?;

    Ok(this)
}

async fn string_buffer_delete_char_at(
    jvm: &Jvm,
    _: &mut RuntimeContext,
    mut this: ClassInstanceRef<StringBuffer>,
    index: i32,
) -> JvmResult<ClassInstanceRef<StringBuffer>> {
    tracing::debug!("java.lang.StringBuffer::deleteCharAt({this:?}, {index})");

    let count: i32 = jvm.get_field(&this, "count", "I").await?;
    if index < 0 || index >= count {
        return Err(jvm
            .exception("java/lang/StringIndexOutOfBoundsException", "no character at that index")
            .await);
    }

    let mut value: ClassInstanceRef<Array<JavaChar>> = jvm.get_field(&this, "value", "[C").await?;
    let chars: Vec<JavaChar> = jvm.load_array(&value, 0, count as _).await?;

    let new_chars = chars
        .iter()
        .take(index as _)
        .chain(chars.iter().skip(index as usize + 1))
        .copied()
        .collect::<Vec<_>>();

    let new_count = new_chars.len() as i32;
    jvm.store_array(&mut value, 0, new_chars).await?;
    jvm.put_field(&mut this, "count", "I", new_count).await?;

    Ok(this)
}

async fn string_buffer_set_char_at(
    jvm: &Jvm,
    _: &mut RuntimeContext,
    this: ClassInstanceRef<StringBuffer>,
    index: i32,
    character: JavaChar,
) -> JvmResult<()> {
    tracing::debug!("java.lang.StringBuffer::setCharAt({this:?}, {index}, {character})");

    let count: i32 = jvm.get_field(&this, "count", "I").await?;
    if index < 0 || index >= count {
        return Err(jvm
            .exception("java/lang/StringIndexOutOfBoundsException", "no character at that index")
            .await);
    }

    // One character over one character: the buffer neither grows nor shrinks,
    // so `count` is left where it is.
    let mut value: ClassInstanceRef<Array<JavaChar>> = jvm.get_field(&this, "value", "[C").await?;
    jvm.store_array(&mut value, index as _, [character]).await?;

    Ok(())
}

/// A stand-in for the class whose constructors are replaced below, so the
/// bodies can name their receiver the way every other proto does.
struct ByteArrayInputStream;

/// Makes `java.io.ByteArrayInputStream`'s constructors throw on a null array
/// instead of taking the emulator down with them.
///
/// A J2ME title reads a save file by asking whether it is there and handing
/// what it got to `new ByteArrayInputStream(...)`, null and all, because a real
/// handset answers that with `NullPointerException` and the title catches it -
/// that is its "no save yet" path. 놈3 does exactly this on a first run, three
/// times over, for `/a`, `/start` and `/nom`.
///
/// The runtime's own constructor reaches straight for the array's length, and a
/// null reference there is a `None` unwrapped inside the JVM - a Rust panic, so
/// the process died where the title expected to catch an exception. The bodies
/// here are the runtime's, with the check the reference makes in front.
///
/// Anything that is not that class is handed back untouched.
fn refuse_a_null_array(mut proto: RuntimeClassProto) -> RuntimeClassProto {
    if proto.name != "java/io/ByteArrayInputStream" {
        return proto;
    }

    for method in proto.methods.iter_mut() {
        if method.name != "<init>" {
            continue;
        }

        *method = match method.descriptor.as_str() {
            "([B)V" => JavaMethodProto::new("<init>", "([B)V", byte_array_input_stream_init, MethodAccessFlags::empty()),
            "([BII)V" => JavaMethodProto::new(
                "<init>",
                "([BII)V",
                byte_array_input_stream_init_with_offset_length,
                MethodAccessFlags::empty(),
            ),
            _ => continue,
        };
    }

    proto
}

async fn byte_array_input_stream_init(
    jvm: &Jvm,
    _: &mut RuntimeContext,
    this: ClassInstanceRef<ByteArrayInputStream>,
    data: ClassInstanceRef<Array<i8>>,
) -> JvmResult<()> {
    if data.is_null() {
        return Err(jvm.exception("java/lang/NullPointerException", "buf is null").await);
    }

    let count = jvm.array_length(&data).await?;

    jvm.invoke_special(&this, "java/io/ByteArrayInputStream", "<init>", "([BII)V", (data, 0, count as i32))
        .await
}

async fn byte_array_input_stream_init_with_offset_length(
    jvm: &Jvm,
    _: &mut RuntimeContext,
    mut this: ClassInstanceRef<ByteArrayInputStream>,
    data: ClassInstanceRef<Array<i8>>,
    offset: i32,
    length: i32,
) -> JvmResult<()> {
    if data.is_null() {
        return Err(jvm.exception("java/lang/NullPointerException", "buf is null").await);
    }

    let data_length = jvm.array_length(&data).await? as i32;
    if offset < 0 || length < 0 || offset > data_length {
        return Err(jvm.exception("java/lang/IndexOutOfBoundsException", "Invalid offset or length").await);
    }

    let _: () = jvm.invoke_special(&this, "java/io/InputStream", "<init>", "()V", ()).await?;

    jvm.put_field(&mut this, "buf", "[B", data).await?;
    jvm.put_field(&mut this, "pos", "I", offset).await?;
    jvm.put_field(&mut this, "count", "I", (offset + length).min(data_length)).await?;
    jvm.put_field(&mut this, "mark", "I", offset).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use alloc::{boxed::Box, vec::Vec};

    use java_runtime::get_runtime_class_proto;
    use jvm::{ClassInstanceRef, Result as JvmResult, runtime::JavaLangString};

    use test_utils::run_jvm_test;
    use wie_util::WieError;

    use super::{StringBuffer, refuse_a_null_array};

    #[test]
    fn byte_array_input_streams_constructors_are_the_ones_replaced() {
        let stock = get_runtime_class_proto("java/io/ByteArrayInputStream").unwrap();
        let stock: Vec<_> = stock
            .methods
            .iter()
            .map(|method| (method.name.clone(), method.descriptor.clone()))
            .collect();

        let guarded = refuse_a_null_array(get_runtime_class_proto("java/io/ByteArrayInputStream").unwrap());
        let guarded: Vec<_> = guarded
            .methods
            .iter()
            .map(|method| (method.name.clone(), method.descriptor.clone()))
            .collect();

        // The class keeps every method it had, in the order it had them - only
        // the two constructors' bodies change, and a body is not comparable.
        assert_eq!(stock, guarded);
        assert!(guarded.contains(&("<init>".into(), "([B)V".into())));
        assert!(guarded.contains(&("<init>".into(), "([BII)V".into())));
    }

    /// The method the runtime does not carry has to behave like the one it
    /// would have: insert at the front, in the middle and at the end, and grow
    /// the buffer past the sixteen characters a fresh one holds.
    #[test]
    fn a_string_buffer_inserts_where_it_is_told() -> Result<(), WieError> {
        run_jvm_test(Box::new([]), async |jvm| {
            let buffer = jvm.new_class("java/lang/StringBuffer", "()V", ()).await?;

            for (offset, inserted) in [(0, "middle"), (0, "the "), (10, " of it"), (16, ", and past what a new buffer holds")] {
                let inserted = JavaLangString::from_rust_string(&jvm, inserted).await?;
                let _: ClassInstanceRef<StringBuffer> = jvm
                    .invoke_virtual(&buffer, "insert", "(ILjava/lang/String;)Ljava/lang/StringBuffer;", (offset, inserted))
                    .await?;
            }

            let text = jvm.invoke_virtual(&buffer, "toString", "()Ljava/lang/String;", ()).await?;
            assert_eq!(
                JavaLangString::to_rust_string(&jvm, &text).await?,
                "the middle of it, and past what a new buffer holds"
            );

            // An offset outside the buffer is the throw a title catches, not
            // something that writes anywhere. The far end is not outside: a
            // buffer is inserted at, and appending is inserting at its length.
            for offset in [-1, 51] {
                let inserted = JavaLangString::from_rust_string(&jvm, "nowhere").await?;
                let result: JvmResult<ClassInstanceRef<StringBuffer>> = jvm
                    .invoke_virtual(&buffer, "insert", "(ILjava/lang/String;)Ljava/lang/StringBuffer;", (offset, inserted))
                    .await;

                assert!(result.is_err(), "{offset} is outside a buffer of 50");
            }

            Ok(())
        })
    }

    /// A number inserts as the digits it is written with, at the same places
    /// and with the same throw as a string.
    #[test]
    fn a_string_buffer_inserts_a_number_as_its_digits() -> Result<(), WieError> {
        run_jvm_test(Box::new([]), async |jvm| {
            let text = JavaLangString::from_rust_string(&jvm, "slot :").await?;
            let buffer = jvm.new_class("java/lang/StringBuffer", "(Ljava/lang/String;)V", (text,)).await?;

            for (offset, value) in [(5, 12), (5, -3)] {
                let _: ClassInstanceRef<StringBuffer> = jvm
                    .invoke_virtual(&buffer, "insert", "(II)Ljava/lang/StringBuffer;", (offset, value))
                    .await?;
            }

            let text = jvm.invoke_virtual(&buffer, "toString", "()Ljava/lang/String;", ()).await?;
            assert_eq!(JavaLangString::to_rust_string(&jvm, &text).await?, "slot -312:");

            let result: JvmResult<ClassInstanceRef<StringBuffer>> =
                jvm.invoke_virtual(&buffer, "insert", "(II)Ljava/lang/StringBuffer;", (11, 0)).await;
            assert!(result.is_err(), "11 is outside a buffer of 10");

            Ok(())
        })
    }

    /// The third gap: a character is written over the one already there, the
    /// buffer keeps its length, and an index it does not have throws.
    #[test]
    fn a_string_buffer_writes_over_the_character_it_is_told_to() -> Result<(), WieError> {
        run_jvm_test(Box::new([]), async |jvm| {
            let text = JavaLangString::from_rust_string(&jvm, "abxd").await?;
            let buffer = jvm.new_class("java/lang/StringBuffer", "(Ljava/lang/String;)V", (text,)).await?;

            let _: () = jvm.invoke_virtual(&buffer, "setCharAt", "(IC)V", (2, b'c' as u16)).await?;

            let text = jvm.invoke_virtual(&buffer, "toString", "()Ljava/lang/String;", ()).await?;
            assert_eq!(JavaLangString::to_rust_string(&jvm, &text).await?, "abcd");

            // Writing at the length is past the end - unlike `insert`, there is
            // no character there to write over.
            for index in [-1, 4] {
                let result: JvmResult<()> = jvm.invoke_virtual(&buffer, "setCharAt", "(IC)V", (index, b'z' as u16)).await;

                assert!(result.is_err(), "{index} is not a character of a buffer of 4");
            }

            Ok(())
        })
    }

    /// And the other half of the same formatting helpers: a character comes out
    /// of the middle, and an index the buffer does not have throws.
    #[test]
    fn a_string_buffer_deletes_the_character_it_is_told_to() -> Result<(), WieError> {
        run_jvm_test(Box::new([]), async |jvm| {
            let text = JavaLangString::from_rust_string(&jvm, "abxcd").await?;
            let buffer = jvm.new_class("java/lang/StringBuffer", "(Ljava/lang/String;)V", (text,)).await?;

            let _: ClassInstanceRef<StringBuffer> = jvm.invoke_virtual(&buffer, "deleteCharAt", "(I)Ljava/lang/StringBuffer;", (2,)).await?;

            let text = jvm.invoke_virtual(&buffer, "toString", "()Ljava/lang/String;", ()).await?;
            assert_eq!(JavaLangString::to_rust_string(&jvm, &text).await?, "abcd");

            for index in [-1, 4] {
                let result: JvmResult<ClassInstanceRef<StringBuffer>> =
                    jvm.invoke_virtual(&buffer, "deleteCharAt", "(I)Ljava/lang/StringBuffer;", (index,)).await;

                assert!(result.is_err(), "{index} is not a character of a buffer of 4");
            }

            Ok(())
        })
    }

    #[test]
    fn every_other_runtime_class_is_handed_back_as_it_was() {
        for class in ["java/io/DataInputStream", "java/lang/String", "java/io/InputStream"] {
            let stock = get_runtime_class_proto(class).unwrap();
            let name = stock.name;
            let methods = stock.methods.len();

            let out = refuse_a_null_array(get_runtime_class_proto(class).unwrap());

            assert_eq!(out.name, name);
            assert_eq!(out.methods.len(), methods);
        }
    }
}
