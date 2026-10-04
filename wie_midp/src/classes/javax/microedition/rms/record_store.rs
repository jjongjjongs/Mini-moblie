use alloc::{borrow::ToOwned, boxed::Box, vec, vec::Vec};

use bytemuck::cast_vec;

use java_class_proto::{JavaFieldProto, JavaMethodProto};
use java_constants::MethodAccessFlags;
use java_runtime::classes::java::lang::String;
use jvm::{Array, ClassInstanceRef, Jvm, Result as JvmResult, runtime::JavaLangString};

use wie_backend::Database;

use crate::classes::javax::microedition::rms::{RecordComparator, RecordEnumeration, RecordFilter};
use wie_jvm_support::{WieJavaClassProto, WieJvmContext};

// class javax.microedition.rms.RecordStore
pub struct RecordStore;

impl RecordStore {
    pub fn as_proto() -> WieJavaClassProto {
        WieJavaClassProto {
            name: "javax/microedition/rms/RecordStore",
            parent_class: Some("java/lang/Object"),
            interfaces: vec![],
            methods: vec![
                JavaMethodProto::new("<init>", "(Ljava/lang/String;)V", Self::init, Default::default()),
                JavaMethodProto::new("addRecord", "([BII)I", Self::add_record, Default::default()),
                JavaMethodProto::new("deleteRecord", "(I)V", Self::delete_record, Default::default()),
                JavaMethodProto::new("getSizeAvailable", "()I", Self::get_size_available, Default::default()),
                JavaMethodProto::new("getNextRecordID", "()I", Self::get_next_record_id, Default::default()),
                JavaMethodProto::new("getRecord", "(I)[B", Self::get_record, Default::default()),
                JavaMethodProto::new("getRecord", "(I[BI)I", Self::get_record_array, Default::default()),
                JavaMethodProto::new("getRecordSize", "(I)I", Self::get_record_size, Default::default()),
                JavaMethodProto::new("setRecord", "(I[BII)V", Self::set_record, Default::default()),
                JavaMethodProto::new("getNumRecords", "()I", Self::get_num_records, Default::default()),
                JavaMethodProto::new("closeRecordStore", "()V", Self::close_record_store, Default::default()),
                JavaMethodProto::new(
                    "enumerateRecords",
                    "(Ljavax/microedition/rms/RecordFilter;Ljavax/microedition/rms/RecordComparator;Z)Ljavax/microedition/rms/RecordEnumeration;",
                    Self::enumerate_records,
                    Default::default(),
                ),
                JavaMethodProto::new(
                    "openRecordStore",
                    "(Ljava/lang/String;Z)Ljavax/microedition/rms/RecordStore;",
                    Self::open_record_store,
                    MethodAccessFlags::STATIC,
                ),
                JavaMethodProto::new(
                    "deleteRecordStore",
                    "(Ljava/lang/String;)V",
                    Self::delete_record_store,
                    MethodAccessFlags::STATIC,
                ),
                JavaMethodProto::new(
                    "listRecordStores",
                    "()[Ljava/lang/String;",
                    Self::list_record_stores,
                    MethodAccessFlags::STATIC,
                ),
            ],
            fields: vec![JavaFieldProto::new("dbName", "Ljava/lang/String;", Default::default())],
            access_flags: Default::default(),
        }
    }

    async fn init(jvm: &Jvm, _context: &mut WieJvmContext, mut this: ClassInstanceRef<Self>, db_name: ClassInstanceRef<String>) -> JvmResult<()> {
        tracing::debug!("javax.microedition.rms.RecordStore::<init>({this:?}, {db_name:?})");

        let _: () = jvm.invoke_special(&this, "java/lang/Object", "<init>", "()V", ()).await?;

        jvm.put_field(&mut this, "dbName", "Ljava/lang/String;", db_name).await?;

        Ok(())
    }

    async fn add_record(
        jvm: &Jvm,
        context: &mut WieJvmContext,
        this: ClassInstanceRef<Self>,
        data: ClassInstanceRef<Array<i8>>,
        offset: i32,
        length: i32,
    ) -> JvmResult<i32> {
        tracing::debug!("javax.microedition.rms.RecordStore::addRecord({this:?}, {data:?}, {offset}, {length})");

        let mut database = Self::get_database(jvm, context, &this).await?;

        let data: Vec<i8> = jvm.load_array(&data, offset as _, length as _).await?;

        let id = database.add(&cast_vec(data)).await;

        Ok(id as _)
    }

    async fn delete_record(jvm: &Jvm, context: &mut WieJvmContext, this: ClassInstanceRef<Self>, record_id: i32) -> JvmResult<()> {
        tracing::debug!("javax.microedition.rms.RecordStore::deleteRecord({this:?}, {record_id})");

        let mut database = Self::get_database(jvm, context, &this).await?;
        if !database.delete(record_id as _).await {
            return Err(jvm.exception("javax/microedition/rms/InvalidRecordIDException", "Record not found").await);
        }

        Ok(())
    }

    async fn get_size_available(_jvm: &Jvm, _context: &mut WieJvmContext, this: ClassInstanceRef<Self>) -> JvmResult<i32> {
        tracing::warn!("stub javax.microedition.rms.RecordStore::getSizeAvailable({this:?})");

        Ok(1000000 as _)
    }

    async fn get_next_record_id(jvm: &Jvm, context: &mut WieJvmContext, this: ClassInstanceRef<Self>) -> JvmResult<i32> {
        tracing::debug!("javax.microedition.rms.RecordStore::getNextRecordID({this:?})");

        let database = Self::get_database(jvm, context, &this).await?;

        let next_id = database.next_id().await;

        Ok(next_id as _)
    }

    async fn get_record(
        jvm: &Jvm,
        context: &mut WieJvmContext,
        this: ClassInstanceRef<Self>,
        record_id: i32,
    ) -> JvmResult<ClassInstanceRef<Array<i8>>> {
        tracing::debug!("javax.microedition.rms.RecordStore::getRecord({this:?}, {record_id})");

        let database = Self::get_database(jvm, context, &this).await?;

        let result = database.get(record_id as _).await;
        if result.is_none() {
            return Err(jvm.exception("javax/microedition/rms/InvalidRecordIDException", "Record not found").await);
        }

        let data = result.unwrap();

        let mut array = jvm.instantiate_array("B", data.len() as _).await?;
        jvm.store_array(&mut array, 0, cast_vec::<u8, i8>(data)).await?;

        Ok(array.into())
    }

    async fn get_record_array(
        jvm: &Jvm,
        context: &mut WieJvmContext,
        this: ClassInstanceRef<Self>,
        record_id: i32,
        mut buffer: ClassInstanceRef<Array<i8>>,
        offset: i32,
    ) -> JvmResult<i32> {
        tracing::debug!("javax.microedition.rms.RecordStore::getRecord({this:?}, {record_id}, {buffer:?}, {offset})");

        let database = Self::get_database(jvm, context, &this).await?;

        let result = database.get(record_id as _).await;
        if result.is_none() {
            return Err(jvm.exception("javax/microedition/rms/InvalidRecordIDException", "Record not found").await);
        }

        let data = result.unwrap();
        let data_length = data.len();
        jvm.store_array(&mut buffer, offset as _, cast_vec::<u8, i8>(data)).await?;

        Ok(data_length as _)
    }

    async fn get_record_size(jvm: &Jvm, context: &mut WieJvmContext, this: ClassInstanceRef<Self>, record_id: i32) -> JvmResult<i32> {
        tracing::debug!("javax.microedition.rms.RecordStore::getRecordSize({this:?}, {record_id})");

        let database = Self::get_database(jvm, context, &this).await?;

        let result = database.get(record_id as _).await;
        if result.is_none() {
            return Err(jvm.exception("javax/microedition/rms/InvalidRecordIDException", "Record not found").await);
        }

        let data = result.unwrap();

        Ok(data.len() as _)
    }

    async fn set_record(
        jvm: &Jvm,
        context: &mut WieJvmContext,
        this: ClassInstanceRef<Self>,
        record_id: i32,
        data: ClassInstanceRef<Array<i8>>,
        offset: i32,
        length: i32,
    ) -> JvmResult<()> {
        tracing::debug!("javax.microedition.rms.RecordStore::setRecord({this:?}, {record_id}, {data:?}, {offset}, {length})");

        let data: Vec<i8> = jvm.load_array(&data, offset as _, length as _).await?;

        let mut database = Self::get_database(jvm, context, &this).await?;
        database.set(record_id as _, &cast_vec(data)).await;

        Ok(())
    }

    async fn get_num_records(jvm: &Jvm, context: &mut WieJvmContext, this: ClassInstanceRef<Self>) -> JvmResult<i32> {
        tracing::debug!("javax.microedition.rms.RecordStore::getNumRecords({this:?})");

        let database = Self::get_database(jvm, context, &this).await?;

        let count = database.get_record_ids().await.len();

        Ok(count as _)
    }

    /// The store's records, as an enumeration over their ids.
    ///
    /// Without a filter every record is in it; without a comparator they come
    /// in the order they were added, which is ascending id. With a filter, a
    /// record is in it when `matches` says so; with a comparator they are
    /// ordered by what `compare` answers, the first before the second when it
    /// answers `PRECEDES`.
    async fn enumerate_records(
        jvm: &Jvm,
        context: &mut WieJvmContext,
        this: ClassInstanceRef<Self>,
        filter: ClassInstanceRef<RecordFilter>,
        comparator: ClassInstanceRef<RecordComparator>,
        keep_updated: bool,
    ) -> JvmResult<ClassInstanceRef<RecordEnumeration>> {
        tracing::debug!("javax.microedition.rms.RecordStore::enumerateRecords({this:?}, {filter:?}, {comparator:?}, {keep_updated})");

        let database = Self::get_database(jvm, context, &this).await?;
        let mut ids = database.get_record_ids().await;
        ids.sort_unstable();

        let mut chosen: Vec<(i32, Option<ClassInstanceRef<Array<i8>>>)> = Vec::with_capacity(ids.len());
        for id in ids {
            let id = id as i32;
            if filter.is_null() && comparator.is_null() {
                chosen.push((id, None));
                continue;
            }

            let record: ClassInstanceRef<Array<i8>> = jvm.invoke_virtual(&this, "getRecord", "(I)[B", (id,)).await?;
            if !filter.is_null() {
                let matches: bool = jvm.invoke_virtual(&filter, "matches", "([B)Z", (record.clone(),)).await?;
                if !matches {
                    continue;
                }
            }
            chosen.push((id, Some(record)));
        }

        // An insertion sort, since every comparison is a call into the title.
        if !comparator.is_null() {
            for i in 1..chosen.len() {
                let mut j = i;
                while j > 0 {
                    let (Some(left), Some(right)) = (chosen[j - 1].1.clone(), chosen[j].1.clone()) else {
                        break;
                    };
                    let order: i32 = jvm.invoke_virtual(&comparator, "compare", "([B[B)I", (right, left)).await?;
                    if order != RecordComparator::PRECEDES {
                        break;
                    }
                    chosen.swap(j - 1, j);
                    j -= 1;
                }
            }
        }

        let ids: Vec<i32> = chosen.into_iter().map(|(id, _)| id).collect();
        let mut array = jvm.instantiate_array("I", ids.len() as _).await?;
        jvm.store_array(&mut array, 0, ids).await?;

        let enumeration: ClassInstanceRef<RecordEnumeration> = jvm
            .new_class(
                "net/wie/RecordEnumerationImpl",
                "(Ljavax/microedition/rms/RecordStore;[I)V",
                (this, array),
            )
            .await?
            .into();

        let mut enumeration_mut = enumeration.clone();
        jvm.put_field(&mut enumeration_mut, "keptUpdated", "Z", keep_updated).await?;

        Ok(enumeration)
    }

    async fn close_record_store(_jvm: &Jvm, _context: &mut WieJvmContext, this: ClassInstanceRef<Self>) -> JvmResult<()> {
        tracing::warn!("stub javax.microedition.rms.RecordStore::closeRecordStore({this:?})");

        Ok(())
    }

    async fn open_record_store(
        jvm: &Jvm,
        context: &mut WieJvmContext,
        name: ClassInstanceRef<String>,
        create: bool,
    ) -> JvmResult<ClassInstanceRef<Self>> {
        tracing::debug!("javax.microedition.rms.RecordStore::openRecordStore({name:?}, {create:?})");

        // An open that was told not to create has nothing to open when the
        // store is not there. This opened one anyway, so a title asking "do I
        // have a save?" was always told yes, and the read that followed found
        // no records.
        //
        // 시네마타이쿤 is where that ends a run. Its `com.mc.util.a` keeps the
        // title's settings in a store called `config`, and asks for them in two
        // steps: a static `a(String)Z` that opens the store to see whether it is
        // there, and a static `a(String)String` that opens it again and reads
        // record one. The first is seven `try` blocks deep and answers false for
        // a store that is not there; the second assumes it is. Told the store
        // existed, the title read a record that was never written, got null, and
        // died building a tokenizer over it - `String.toCharArray` on null,
        // inside `GameAppMain.startApp`, before its first frame.
        if !create {
            let store_name = JavaLangString::to_rust_string(jvm, &name).await?;
            let app_id = context.system().pid().to_owned();

            // Whether the store is there, by its own resolved name rather than
            // the top-level listing: 초밥의달인3 keeps its save in a store called
            // `file/data`, whose directory is nested a level below the
            // application's namespace where `list` never looks. Asked the
            // listing, a save written under such a name was always reported
            // missing, and the load screen showed empty slots over a save that
            // was on disk the whole time.
            let has_save = context.system().platform().database_repository().has_records(&store_name, &app_id).await;

            if !has_save {
                tracing::debug!("javax.microedition.rms.RecordStore::openRecordStore({store_name}) -> no such store");

                // The specific type, not the base RecordStoreException: a title
                // catches this on its own to tell "no save yet" apart from a
                // store failure and create the store in response. 크레이지버스
                // opens with create=false, catches this, and opens again with
                // create=true to write its defaults.
                return Err(jvm
                    .exception("javax/microedition/rms/RecordStoreNotFoundException", "Record store not found")
                    .await);
            }
        }

        let store = jvm
            .new_class("javax/microedition/rms/RecordStore", "(Ljava/lang/String;)V", (name,))
            .await?;

        Ok(store.into())
    }

    async fn delete_record_store(jvm: &Jvm, context: &mut WieJvmContext, name: ClassInstanceRef<String>) -> JvmResult<()> {
        tracing::debug!("javax.microedition.rms.RecordStore::deleteRecordStore({name:?})");

        let name = JavaLangString::to_rust_string(jvm, &name).await?;

        let system = context.system();
        let app_id = system.pid().to_owned();

        // Drop the backing store. Deletion is idempotent: removing a store that
        // was never created is a no-op success, which keeps a title that clears
        // an as-yet-unwritten save from taking an exception the reference does
        // not raise here.
        system.platform().database_repository().delete(&name, &app_id).await;

        Ok(())
    }

    async fn list_record_stores(jvm: &Jvm, context: &mut WieJvmContext) -> JvmResult<ClassInstanceRef<Array<String>>> {
        tracing::debug!("javax.microedition.rms.RecordStore::listRecordStores()");

        let system = context.system();
        let app_id = system.pid().to_owned();
        let names = system.platform().database_repository().list(&app_id).await;

        let mut result = jvm.instantiate_array("Ljava/lang/String;", names.len()).await?;
        for (index, name) in names.iter().enumerate() {
            let name = JavaLangString::from_rust_string(jvm, name).await?;
            jvm.store_array(&mut result, index, [name]).await?;
        }

        Ok(result.into())
    }

    async fn get_database(jvm: &Jvm, context: &mut WieJvmContext, this: &ClassInstanceRef<Self>) -> JvmResult<Box<dyn Database>> {
        let db_name = jvm.get_field(this, "dbName", "Ljava/lang/String;").await?;
        let db_name_str = JavaLangString::to_rust_string(jvm, &db_name).await?;

        let system = context.system();
        let pid = system.pid().to_owned();

        Ok(system.platform().database_repository().open(&db_name_str, &pid).await)
    }
}

#[cfg(test)]
mod test {
    use alloc::boxed::Box;

    use java_runtime::classes::java::lang::String;
    use jvm::{Array, ClassInstanceRef, JavaError, Result as JvmResult, runtime::JavaLangString};
    use test_utils::run_jvm_test;
    use wie_util::Result;

    use crate::get_protos;

    use super::RecordStore;

    #[test]
    fn delete_record_removes_record_and_rejects_unknown_id() -> Result<()> {
        run_jvm_test(Box::new([get_protos().into()]), |jvm| async move {
            let name: ClassInstanceRef<String> = JavaLangString::from_rust_string(&jvm, "delete-record").await?.into();
            let store: ClassInstanceRef<RecordStore> = jvm
                .invoke_static(
                    "javax/microedition/rms/RecordStore",
                    "openRecordStore",
                    "(Ljava/lang/String;Z)Ljavax/microedition/rms/RecordStore;",
                    (name, true),
                )
                .await?;

            let mut data = jvm.instantiate_array("B", 2).await?;
            jvm.store_array(&mut data, 0, [1i8, 2]).await?;
            let record_id: i32 = jvm.invoke_virtual(&store, "addRecord", "([BII)I", (data, 0, 2)).await?;
            assert_eq!(record_id, 1);

            let count: i32 = jvm.invoke_virtual(&store, "getNumRecords", "()I", ()).await?;
            assert_eq!(count, 1);

            let _: () = jvm.invoke_virtual(&store, "deleteRecord", "(I)V", (record_id,)).await?;
            let count: i32 = jvm.invoke_virtual(&store, "getNumRecords", "()I", ()).await?;
            assert_eq!(count, 0);

            let deleted: JvmResult<ClassInstanceRef<Array<i8>>> = jvm.invoke_virtual(&store, "getRecord", "(I)[B", (record_id,)).await;
            let Err(JavaError::JavaException(exception)) = deleted else {
                panic!("deleted record lookup succeeded");
            };
            assert!(jvm.is_instance(&*exception, "javax/microedition/rms/InvalidRecordIDException"));

            let unknown: JvmResult<()> = jvm.invoke_virtual(&store, "deleteRecord", "(I)V", (99,)).await;
            let Err(JavaError::JavaException(exception)) = unknown else {
                panic!("unknown record deletion succeeded");
            };
            assert!(jvm.is_instance(&*exception, "javax/microedition/rms/InvalidRecordIDException"));

            Ok(())
        })
    }

    /// An open that was told not to create fails when the store is not there,
    /// and succeeds once it is.
    ///
    /// 시네마타이쿤 asks that question about its `config` store to decide
    /// whether it has settings to read. Answered yes for a store that was never
    /// written, it read a record that does not exist, got null, and died on the
    /// first thing it did with it.
    #[test]
    fn opening_a_store_that_is_not_there_without_creating_it_fails() -> Result<()> {
        run_jvm_test(Box::new([get_protos().into()]), |jvm| async move {
            let name: ClassInstanceRef<String> = JavaLangString::from_rust_string(&jvm, "config").await?.into();

            let missing: JvmResult<ClassInstanceRef<RecordStore>> = jvm
                .invoke_static(
                    "javax/microedition/rms/RecordStore",
                    "openRecordStore",
                    "(Ljava/lang/String;Z)Ljavax/microedition/rms/RecordStore;",
                    (name.clone(), false),
                )
                .await;
            let Err(JavaError::JavaException(exception)) = missing else {
                panic!("opening a store that was never written succeeded");
            };
            assert!(jvm.is_instance(&*exception, "javax/microedition/rms/RecordStoreException"));

            // Written once, it opens without being asked to create it.
            let created: ClassInstanceRef<RecordStore> = jvm
                .invoke_static(
                    "javax/microedition/rms/RecordStore",
                    "openRecordStore",
                    "(Ljava/lang/String;Z)Ljavax/microedition/rms/RecordStore;",
                    (name.clone(), true),
                )
                .await?;
            let mut data = jvm.instantiate_array("B", 1).await?;
            jvm.store_array(&mut data, 0, [7i8]).await?;
            let _: i32 = jvm.invoke_virtual(&created, "addRecord", "([BII)I", (data, 0, 1)).await?;

            let reopened: ClassInstanceRef<RecordStore> = jvm
                .invoke_static(
                    "javax/microedition/rms/RecordStore",
                    "openRecordStore",
                    "(Ljava/lang/String;Z)Ljavax/microedition/rms/RecordStore;",
                    (name, false),
                )
                .await?;
            let count: i32 = jvm.invoke_virtual(&reopened, "getNumRecords", "()I", ()).await?;
            assert_eq!(count, 1);

            Ok(())
        })
    }

    /// A store saved under a nested name opens again without being asked to
    /// create it.
    ///
    /// 초밥의달인3 keeps its save in a store called `file/data`, whose directory
    /// is nested a level below the application's namespace. The existence check
    /// once read the top-level listing, which never sees such a store, so the
    /// save was reported missing every time the game looked for it.
    #[test]
    fn a_store_saved_under_a_nested_name_is_found_again() -> Result<()> {
        run_jvm_test(Box::new([get_protos().into()]), |jvm| async move {
            let name: ClassInstanceRef<String> = JavaLangString::from_rust_string(&jvm, "file/data").await?.into();

            // Not there before it is written, even though a name with a slash
            // in it would once have slipped past the listing.
            let missing: JvmResult<ClassInstanceRef<RecordStore>> = jvm
                .invoke_static(
                    "javax/microedition/rms/RecordStore",
                    "openRecordStore",
                    "(Ljava/lang/String;Z)Ljavax/microedition/rms/RecordStore;",
                    (name.clone(), false),
                )
                .await;
            let Err(JavaError::JavaException(exception)) = missing else {
                panic!("opening a nested store that was never written succeeded");
            };
            assert!(jvm.is_instance(&*exception, "javax/microedition/rms/RecordStoreNotFoundException"));

            let created: ClassInstanceRef<RecordStore> = jvm
                .invoke_static(
                    "javax/microedition/rms/RecordStore",
                    "openRecordStore",
                    "(Ljava/lang/String;Z)Ljavax/microedition/rms/RecordStore;",
                    (name.clone(), true),
                )
                .await?;
            let mut data = jvm.instantiate_array("B", 3).await?;
            jvm.store_array(&mut data, 0, [4i8, 5, 6]).await?;
            let _: i32 = jvm.invoke_virtual(&created, "addRecord", "([BII)I", (data, 0, 3)).await?;

            let reopened: ClassInstanceRef<RecordStore> = jvm
                .invoke_static(
                    "javax/microedition/rms/RecordStore",
                    "openRecordStore",
                    "(Ljava/lang/String;Z)Ljavax/microedition/rms/RecordStore;",
                    (name, false),
                )
                .await?;
            let count: i32 = jvm.invoke_virtual(&reopened, "getNumRecords", "()I", ()).await?;
            assert_eq!(count, 1, "the save under the nested name is found again");

            Ok(())
        })
    }

    /// A store saved under a nested name is offered by listRecordStores, which
    /// is how a load screen that enumerates saves finds it.
    ///
    /// 초밥의달인3 saves under `file/data` and its load screen lists the stores.
    /// The listing once dropped any name with a slash, so the save was written
    /// and persisted but never shown, and the slots read empty after a restart.
    #[test]
    fn a_store_saved_under_a_nested_name_is_listed() -> Result<()> {
        run_jvm_test(Box::new([get_protos().into()]), |jvm| async move {
            let name: ClassInstanceRef<String> = JavaLangString::from_rust_string(&jvm, "file/data").await?.into();
            let store: ClassInstanceRef<RecordStore> = jvm
                .invoke_static(
                    "javax/microedition/rms/RecordStore",
                    "openRecordStore",
                    "(Ljava/lang/String;Z)Ljavax/microedition/rms/RecordStore;",
                    (name, true),
                )
                .await?;

            let mut data = jvm.instantiate_array("B", 1).await?;
            jvm.store_array(&mut data, 0, [7i8]).await?;
            let _: i32 = jvm.invoke_virtual(&store, "addRecord", "([BII)I", (data, 0, 1)).await?;

            let listed: ClassInstanceRef<Array<String>> = jvm
                .invoke_static("javax/microedition/rms/RecordStore", "listRecordStores", "()[Ljava/lang/String;", ())
                .await?;
            assert_eq!(jvm.array_length(&listed).await?, 1);
            let first: ClassInstanceRef<String> = jvm.load_array(&listed, 0, 1).await?.pop().unwrap();
            assert_eq!(JavaLangString::to_rust_string(&jvm, &first).await?.as_str(), "file/data");

            Ok(())
        })
    }

    #[test]
    fn delete_record_store_removes_it_from_the_listing() -> Result<()> {
        run_jvm_test(Box::new([get_protos().into()]), |jvm| async move {
            let name: ClassInstanceRef<String> = JavaLangString::from_rust_string(&jvm, "save-slot").await?.into();
            let store: ClassInstanceRef<RecordStore> = jvm
                .invoke_static(
                    "javax/microedition/rms/RecordStore",
                    "openRecordStore",
                    "(Ljava/lang/String;Z)Ljavax/microedition/rms/RecordStore;",
                    (name.clone(), true),
                )
                .await?;

            // The backing store is materialized by the first record operation,
            // so write one record before expecting it in the listing.
            let mut data = jvm.instantiate_array("B", 1).await?;
            jvm.store_array(&mut data, 0, [7i8]).await?;
            let _: i32 = jvm.invoke_virtual(&store, "addRecord", "([BII)I", (data, 0, 1)).await?;

            let listed: ClassInstanceRef<Array<String>> = jvm
                .invoke_static("javax/microedition/rms/RecordStore", "listRecordStores", "()[Ljava/lang/String;", ())
                .await?;
            assert_eq!(jvm.array_length(&listed).await?, 1);
            let first: ClassInstanceRef<String> = jvm.load_array(&listed, 0, 1).await?.pop().unwrap();
            assert_eq!(JavaLangString::to_rust_string(&jvm, &first).await?.as_str(), "save-slot");

            let _: () = jvm
                .invoke_static(
                    "javax/microedition/rms/RecordStore",
                    "deleteRecordStore",
                    "(Ljava/lang/String;)V",
                    (name.clone(),),
                )
                .await?;

            let after: ClassInstanceRef<Array<String>> = jvm
                .invoke_static("javax/microedition/rms/RecordStore", "listRecordStores", "()[Ljava/lang/String;", ())
                .await?;
            assert_eq!(jvm.array_length(&after).await?, 0);

            // Deleting an absent store is a no-op success, not an exception.
            let _: () = jvm
                .invoke_static(
                    "javax/microedition/rms/RecordStore",
                    "deleteRecordStore",
                    "(Ljava/lang/String;)V",
                    (name,),
                )
                .await?;

            Ok(())
        })
    }
}
