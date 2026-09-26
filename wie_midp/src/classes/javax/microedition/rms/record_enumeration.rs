use alloc::vec;

use java_class_proto::{JavaFieldProto, JavaMethodProto};
use java_constants::ClassAccessFlags;
use jvm::{Array, ClassInstanceRef, Jvm, Result as JvmResult};

use wie_jvm_support::{WieJavaClassProto, WieJvmContext};

use crate::classes::javax::microedition::rms::RecordStore;

// interface javax.microedition.rms.RecordEnumeration
pub struct RecordEnumeration;

impl RecordEnumeration {
    pub fn as_proto() -> WieJavaClassProto {
        WieJavaClassProto {
            name: "javax/microedition/rms/RecordEnumeration",
            parent_class: None,
            interfaces: vec![],
            methods: vec![
                JavaMethodProto::new_abstract("numRecords", "()I", Default::default()),
                JavaMethodProto::new_abstract("nextRecord", "()[B", Default::default()),
                JavaMethodProto::new_abstract("nextRecordId", "()I", Default::default()),
                JavaMethodProto::new_abstract("previousRecord", "()[B", Default::default()),
                JavaMethodProto::new_abstract("previousRecordId", "()I", Default::default()),
                JavaMethodProto::new_abstract("hasNextElement", "()Z", Default::default()),
                JavaMethodProto::new_abstract("hasPreviousElement", "()Z", Default::default()),
                JavaMethodProto::new_abstract("reset", "()V", Default::default()),
                JavaMethodProto::new_abstract("rebuild", "()V", Default::default()),
                JavaMethodProto::new_abstract("keepUpdated", "(Z)V", Default::default()),
                JavaMethodProto::new_abstract("isKeptUpdated", "()Z", Default::default()),
                JavaMethodProto::new_abstract("destroy", "()V", Default::default()),
            ],
            fields: vec![],
            access_flags: ClassAccessFlags::INTERFACE,
        }
    }
}

// class net.wie.RecordEnumerationImpl
//
// What `RecordStore.enumerateRecords` hands back: the record ids it chose, in
// the order it chose, and a cursor over them. 드래곤아이즈 walks its store this
// way to find its settings record, and the class not being there at all ended
// the run in its resource loader.
//
// The ids are a snapshot. `keepUpdated` is accepted and answered, but a record
// added after the enumeration was made is not seen until `rebuild`, which reads
// the store again without the filter and comparator it was first built with.
pub struct RecordEnumerationImpl;

impl RecordEnumerationImpl {
    pub fn as_proto() -> WieJavaClassProto {
        WieJavaClassProto {
            name: "net/wie/RecordEnumerationImpl",
            parent_class: Some("java/lang/Object"),
            interfaces: vec!["javax/microedition/rms/RecordEnumeration"],
            methods: vec![
                JavaMethodProto::new("<init>", "(Ljavax/microedition/rms/RecordStore;[I)V", Self::init, Default::default()),
                JavaMethodProto::new("numRecords", "()I", Self::num_records, Default::default()),
                JavaMethodProto::new("nextRecord", "()[B", Self::next_record, Default::default()),
                JavaMethodProto::new("nextRecordId", "()I", Self::next_record_id, Default::default()),
                JavaMethodProto::new("previousRecord", "()[B", Self::previous_record, Default::default()),
                JavaMethodProto::new("previousRecordId", "()I", Self::previous_record_id, Default::default()),
                JavaMethodProto::new("hasNextElement", "()Z", Self::has_next_element, Default::default()),
                JavaMethodProto::new("hasPreviousElement", "()Z", Self::has_previous_element, Default::default()),
                JavaMethodProto::new("reset", "()V", Self::reset, Default::default()),
                JavaMethodProto::new("rebuild", "()V", Self::rebuild, Default::default()),
                JavaMethodProto::new("keepUpdated", "(Z)V", Self::keep_updated, Default::default()),
                JavaMethodProto::new("isKeptUpdated", "()Z", Self::is_kept_updated, Default::default()),
                JavaMethodProto::new("destroy", "()V", Self::destroy, Default::default()),
            ],
            fields: vec![
                JavaFieldProto::new("store", "Ljavax/microedition/rms/RecordStore;", Default::default()),
                JavaFieldProto::new("ids", "[I", Default::default()),
                // How many ids the cursor has passed: the next is `ids[index]`,
                // the previous `ids[index - 1]`.
                JavaFieldProto::new("index", "I", Default::default()),
                JavaFieldProto::new("keptUpdated", "Z", Default::default()),
            ],
            access_flags: Default::default(),
        }
    }

    async fn init(
        jvm: &Jvm,
        _: &mut WieJvmContext,
        mut this: ClassInstanceRef<Self>,
        store: ClassInstanceRef<RecordStore>,
        ids: ClassInstanceRef<Array<i32>>,
    ) -> JvmResult<()> {
        let _: () = jvm.invoke_special(&this, "java/lang/Object", "<init>", "()V", ()).await?;

        jvm.put_field(&mut this, "store", "Ljavax/microedition/rms/RecordStore;", store).await?;
        jvm.put_field(&mut this, "ids", "[I", ids).await?;

        Ok(())
    }

    async fn ids_and_index(jvm: &Jvm, this: &ClassInstanceRef<Self>) -> JvmResult<(ClassInstanceRef<Array<i32>>, i32, i32)> {
        let ids: ClassInstanceRef<Array<i32>> = jvm.get_field(this, "ids", "[I").await?;
        let length = jvm.array_length(&ids).await? as i32;
        let index: i32 = jvm.get_field(this, "index", "I").await?;

        Ok((ids, length, index))
    }

    async fn num_records(jvm: &Jvm, _: &mut WieJvmContext, this: ClassInstanceRef<Self>) -> JvmResult<i32> {
        Ok(Self::ids_and_index(jvm, &this).await?.1)
    }

    async fn next_record_id(jvm: &Jvm, _: &mut WieJvmContext, mut this: ClassInstanceRef<Self>) -> JvmResult<i32> {
        tracing::debug!("net.wie.RecordEnumerationImpl::nextRecordId({this:?})");

        let (ids, length, index) = Self::ids_and_index(jvm, &this).await?;
        if index >= length {
            return Err(jvm.exception("javax/microedition/rms/InvalidRecordIDException", "no next record").await);
        }

        let id: i32 = jvm.load_array(&ids, index as _, 1).await?[0];
        jvm.put_field(&mut this, "index", "I", index + 1).await?;

        Ok(id)
    }

    async fn previous_record_id(jvm: &Jvm, _: &mut WieJvmContext, mut this: ClassInstanceRef<Self>) -> JvmResult<i32> {
        let (ids, _, index) = Self::ids_and_index(jvm, &this).await?;
        if index <= 0 {
            return Err(jvm
                .exception("javax/microedition/rms/InvalidRecordIDException", "no previous record")
                .await);
        }

        let id: i32 = jvm.load_array(&ids, (index - 1) as _, 1).await?[0];
        jvm.put_field(&mut this, "index", "I", index - 1).await?;

        Ok(id)
    }

    async fn next_record(jvm: &Jvm, context: &mut WieJvmContext, this: ClassInstanceRef<Self>) -> JvmResult<ClassInstanceRef<Array<i8>>> {
        let id = Self::next_record_id(jvm, context, this.clone()).await?;
        Self::record(jvm, &this, id).await
    }

    async fn previous_record(jvm: &Jvm, context: &mut WieJvmContext, this: ClassInstanceRef<Self>) -> JvmResult<ClassInstanceRef<Array<i8>>> {
        let id = Self::previous_record_id(jvm, context, this.clone()).await?;
        Self::record(jvm, &this, id).await
    }

    async fn record(jvm: &Jvm, this: &ClassInstanceRef<Self>, id: i32) -> JvmResult<ClassInstanceRef<Array<i8>>> {
        let store: ClassInstanceRef<RecordStore> = jvm.get_field(this, "store", "Ljavax/microedition/rms/RecordStore;").await?;

        jvm.invoke_virtual(&store, "getRecord", "(I)[B", (id,)).await
    }

    async fn has_next_element(jvm: &Jvm, _: &mut WieJvmContext, this: ClassInstanceRef<Self>) -> JvmResult<bool> {
        let (_, length, index) = Self::ids_and_index(jvm, &this).await?;

        Ok(index < length)
    }

    async fn has_previous_element(jvm: &Jvm, _: &mut WieJvmContext, this: ClassInstanceRef<Self>) -> JvmResult<bool> {
        let (_, _, index) = Self::ids_and_index(jvm, &this).await?;

        Ok(index > 0)
    }

    async fn reset(jvm: &Jvm, _: &mut WieJvmContext, mut this: ClassInstanceRef<Self>) -> JvmResult<()> {
        jvm.put_field(&mut this, "index", "I", 0).await
    }

    async fn rebuild(jvm: &Jvm, _: &mut WieJvmContext, mut this: ClassInstanceRef<Self>) -> JvmResult<()> {
        let store: ClassInstanceRef<RecordStore> = jvm.get_field(&this, "store", "Ljavax/microedition/rms/RecordStore;").await?;
        let fresh: ClassInstanceRef<Self> = jvm
            .invoke_virtual(
                &store,
                "enumerateRecords",
                "(Ljavax/microedition/rms/RecordFilter;Ljavax/microedition/rms/RecordComparator;Z)Ljavax/microedition/rms/RecordEnumeration;",
                (ClassInstanceRef::<()>::new(None), ClassInstanceRef::<()>::new(None), false),
            )
            .await?;
        let ids: ClassInstanceRef<Array<i32>> = jvm.get_field(&fresh, "ids", "[I").await?;

        jvm.put_field(&mut this, "ids", "[I", ids).await?;
        jvm.put_field(&mut this, "index", "I", 0).await
    }

    async fn keep_updated(jvm: &Jvm, _: &mut WieJvmContext, mut this: ClassInstanceRef<Self>, keep: bool) -> JvmResult<()> {
        jvm.put_field(&mut this, "keptUpdated", "Z", keep).await
    }

    async fn is_kept_updated(jvm: &Jvm, _: &mut WieJvmContext, this: ClassInstanceRef<Self>) -> JvmResult<bool> {
        jvm.get_field(&this, "keptUpdated", "Z").await
    }

    async fn destroy(_: &Jvm, _: &mut WieJvmContext, _this: ClassInstanceRef<Self>) -> JvmResult<()> {
        Ok(())
    }
}
