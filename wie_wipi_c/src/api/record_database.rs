//! KTF WIPI-C table 5: the record database.
//!
//! This is the other half of KTF's storage. The table in [`super::database`] is
//! one blob read and written through a cursor; this one is a numbered set of
//! records, and a title reaches for it when it wants to address entries rather
//! than offsets.
//!
//! It has to be served rather than refused, because a refusal is not neutral.
//! This runtime answered the whole table "unimplemented", which ends the run:
//! 겟앰프드 opens a record database in `startApp` and never drew a frame. The
//! reference's own note on this table says the softer failure is no better - a
//! title that cannot open one carries on with whatever its buffer held, which
//! is zeros, and then behaves as though those zeros were its settings.
//!
//! The slot numbers are the reference's, read off callers rather than off the
//! specification's print order.

use alloc::{
    borrow::ToOwned,
    format,
    string::{String, ToString},
    vec,
    vec::Vec,
};
use core::mem::size_of;

use bytemuck::{Pod, Zeroable};

use wipi_types::wipic::WIPICWord;

use wie_util::{Result, read_generic, read_null_terminated_string_bytes, write_generic};

use crate::context::WIPICContext;

use super::database::packaged_store_bytes;

/// "MCRD" - the sentinel at the front of a record-database handle.
///
/// It differs from the stream table's so a handle passed to the wrong table is
/// refused rather than silently addressing another store.
const RECORD_HANDLE_MAGIC: u32 = 0x4D435244;

const MAX_NAME_LEN: usize = 31;
const MAX_RECORD_BYTES: usize = 4 << 20;
const MAX_RECORDS: u32 = 0x1_0000;

/// What a record database's name becomes in the repository.
///
/// The two storage tables are two namespaces: a title may keep a stream
/// database and a record database of the same name, and the reference keeps
/// them apart the same way.
const STORE_PREFIX: &str = "rdb.";

const M_E_INVALID: i32 = -9;
const M_E_NOENT: i32 = -12;
const M_E_SHORTBUF: i32 = -18;

#[repr(C, packed)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct RecordDatabaseHandle {
    magic: u32,
    name: [u8; 32],
    /// What the open asked for, reported back by `MC_rdbGetRecordSize` and
    /// nothing else: the titles here write fixed-size records themselves and
    /// never ask this platform to enforce one.
    record_size: u32,
}

fn store_name(name: &str) -> String {
    format!("{STORE_PREFIX}{name}")
}

fn load_handle(context: &mut dyn WIPICContext, handle: WIPICWord) -> Result<Option<RecordDatabaseHandle>> {
    if handle < 0x1000 {
        return Ok(None);
    }

    let loaded: RecordDatabaseHandle = read_generic(context, handle)?;
    if loaded.magic != RECORD_HANDLE_MAGIC {
        return Ok(None);
    }

    Ok(Some(loaded))
}

fn handle_name(handle: &RecordDatabaseHandle) -> Option<String> {
    let end = handle.name.iter().position(|&byte| byte == 0).unwrap_or(handle.name.len());

    String::from_utf8(handle.name[..end].to_vec()).ok()
}

/// The records this store holds, oldest id first.
async fn records(context: &mut dyn WIPICContext, name: &str) -> Vec<(u32, Vec<u8>)> {
    let system = context.system();
    let pid = system.pid().to_owned();
    let db = system.platform().database_repository().open(&store_name(name), &pid).await;

    let mut ids = db.get_record_ids().await;
    ids.sort();

    let mut records = Vec::with_capacity(ids.len());
    for id in ids {
        if let Some(data) = db.get(id).await {
            records.push((id, data));
        }
    }

    records
}

/// The header both packaged shapes carry: a magic, then the record size and the
/// record count as big-endian words.
const PACKAGED_HEADER: usize = 45;
const PACKAGED_SIZE_AT: usize = 5;
const PACKAGED_COUNT_AT: usize = 9;

/// How much of a magic has to match.
///
/// Both magics are five bytes and only their third differs, so four is the
/// whole of what tells the two shapes apart. The reference leaves the last byte
/// out because a packaged index in its corpus has it overwritten, and takes the
/// record size from the data file for that one.
const MAGIC_MATCH: usize = 4;

const ONE_FILE_MAGIC: &[u8] = b"qtcdb";
const INDEX_MAGIC: &[u8] = b"qtpdb";

fn parse_packaged_header(data: &[u8], magic: &[u8]) -> Option<(u32, u32, bool)> {
    if data.len() < PACKAGED_HEADER || data[..MAGIC_MATCH] != magic[..MAGIC_MATCH] {
        return None;
    }

    let word = |at: usize| u32::from_be_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]]);
    let intact = data[..magic.len()] == *magic;

    Some((word(PACKAGED_SIZE_AT), word(PACKAGED_COUNT_AT), intact))
}

fn split_records(data: &[u8], record_size: u32) -> Vec<Vec<u8>> {
    if record_size == 0 {
        return Vec::new();
    }

    data.chunks_exact(record_size as usize).map(|record| record.to_vec()).collect()
}

/// The one-file shape: the header, then its records.
///
/// A record can carry a byte in front of it that is not part of what it holds:
/// 드래곤로드's `FUNTER_DL.db` is two eight-byte records in eighteen bytes, each
/// behind a `01`. The title reads its save key out of the first, and took the
/// marker and seven bytes of key while this split the file into eight-byte
/// pieces - so its save never decrypted, its settings came back zeroed, and a
/// zero speed is its slowest, a frame every 500 ms. Records sit behind that
/// byte when the file is exactly long enough for one each.
fn parse_one_file(data: &[u8]) -> Option<Vec<Vec<u8>>> {
    let (record_size, count, _) = parse_packaged_header(data, ONE_FILE_MAGIC)?;
    if count > MAX_RECORDS || record_size as usize > MAX_RECORD_BYTES {
        return None;
    }

    let body = &data[PACKAGED_HEADER..];
    if marked_records(body, record_size, count) {
        return Some(
            split_records(body, record_size + 1)
                .into_iter()
                .map(|record| record[1..].to_vec())
                .collect(),
        );
    }

    Some(split_records(body, record_size))
}

/// Whether a one-file body is `count` records each behind a byte of its own.
fn marked_records(body: &[u8], record_size: u32, count: u32) -> bool {
    record_size != 0 && count != 0 && body.len() as u64 == count as u64 * (record_size as u64 + 1)
}

/// What this used to make of a one-file database whose records are marked, if
/// this one is: its body cut into pieces of the record size, markers and all.
fn misread_one_file(data: &[u8]) -> Option<Vec<Vec<u8>>> {
    let (record_size, count, _) = parse_packaged_header(data, ONE_FILE_MAGIC)?;
    let body = &data[PACKAGED_HEADER..];
    if count > MAX_RECORDS || record_size as usize > MAX_RECORD_BYTES || !marked_records(body, record_size, count) {
        return None;
    }

    Some(split_records(body, record_size))
}

/// The split shape: an index that says how many records there are and how long
/// one is, and a data file that is those records and nothing else.
///
/// When the two disagree the data file wins, but only for the damage this is
/// for - an index whose magic is cut short, and whose record size therefore
/// cannot be believed. An intact header that disagrees with the file beside it
/// is a stale file rather than a bent number.
fn parse_split(index: &[u8], data: &[u8]) -> Option<Vec<Vec<u8>>> {
    let (mut record_size, count, intact) = parse_packaged_header(index, INDEX_MAGIC)?;
    if count > MAX_RECORDS {
        return None;
    }

    if count == 0 {
        // A database with no record still exists, whatever sits beside it.
        return Some(Vec::new());
    }

    if record_size as u64 * count as u64 != data.len() as u64 {
        if intact || data.is_empty() || !data.len().is_multiple_of(count as usize) {
            return None;
        }

        record_size = (data.len() / count as usize) as u32;
    }

    if record_size == 0 || record_size as usize > MAX_RECORD_BYTES {
        return None;
    }

    Some(split_records(data, record_size))
}

/// The records the archive ships for this database, if it ships any.
async fn packaged_records(context: &mut dyn WIPICContext, name: &str) -> Result<Option<Vec<Vec<u8>>>> {
    if let Some(index) = packaged_store_bytes(context, &format!("{name}.idx")).await? {
        let data = packaged_store_bytes(context, &format!("{name}.db")).await?.unwrap_or_default();
        if let Some(records) = parse_split(&index, &data) {
            return Ok(Some(records));
        }
    }

    for candidate in [name.to_string(), format!("{name}.db")] {
        let Some(data) = packaged_store_bytes(context, &candidate).await? else {
            continue;
        };

        if let Some(records) = parse_one_file(&data) {
            return Ok(Some(records));
        }
    }

    Ok(None)
}

/// Puts right a database that was seeded from a misread copy of the archive's.
///
/// Seeding happens once, on the first open, and the store keeps what it was
/// given; a handset that ran the title before [`parse_one_file`] read marked
/// records holds the misread ones still, and would go on handing the title a
/// wrong key. A store that holds exactly what the misreading made - nothing
/// the title has written since - is seeded again from the right reading.
async fn reseed_misread(context: &mut dyn WIPICContext, name: &str, stored: &str) -> Result<()> {
    let mut packaged = None;
    for candidate in [name.to_string(), format!("{name}.db")] {
        if let Some(data) = packaged_store_bytes(context, &candidate).await? {
            packaged = Some(data);
            break;
        }
    }
    let Some(data) = packaged else {
        return Ok(());
    };
    let (Some(misread), Some(records)) = (misread_one_file(&data), parse_one_file(&data)) else {
        return Ok(());
    };

    let system = context.system();
    let pid = system.pid().to_owned();
    let repository = system.platform().database_repository();

    let db = repository.open(stored, &pid).await;
    let mut held = Vec::new();
    for id in db.get_record_ids().await {
        held.push(db.get(id).await.unwrap_or_default());
    }
    drop(db);

    if held != misread {
        return Ok(());
    }

    repository.delete(stored, &pid).await;
    let mut db = repository.open(stored, &pid).await;
    for record in &records {
        db.add(record).await;
    }

    tracing::info!("MC_rdbOpen({name:?}): reseeded {} packaged records misread before", records.len());

    Ok(())
}

/// `MC_rdbOpen(name, record_size, create)` - slot 0.
///
/// A database exists when the player has written one or when the archive ships
/// one; `create == 0` against neither is `M_E_NOENT`, which is the answer a
/// title takes its fresh-start path on.
pub async fn open(context: &mut dyn WIPICContext, ptr_name: WIPICWord, record_size: WIPICWord, create: i32) -> Result<i32> {
    if ptr_name == 0 {
        return Ok(M_E_INVALID);
    }

    let Ok(name) = String::from_utf8(read_null_terminated_string_bytes(context, ptr_name)?) else {
        return Ok(M_E_INVALID);
    };

    if name.is_empty() || name.len() > MAX_NAME_LEN {
        return Ok(M_E_INVALID);
    }

    let stored = store_name(&name);
    let exists = {
        let system = context.system();
        let pid = system.pid().to_owned();

        system.platform().database_repository().exists(&stored, &pid).await
    };

    if exists {
        reseed_misread(context, &name, &stored).await?;
    } else {
        // The archive's own copy is the database's initial content, laid into
        // the store the first time the title opens it.
        let packaged = packaged_records(context, &name).await?;

        match packaged {
            Some(packaged) => {
                let system = context.system();
                let pid = system.pid().to_owned();
                let mut db = system.platform().database_repository().open(&stored, &pid).await;

                for record in &packaged {
                    db.add(record).await;
                }

                tracing::debug!("MC_rdbOpen({name:?}): seeded {} packaged records", packaged.len());
            }
            None if create == 0 => {
                tracing::debug!("MC_rdbOpen({name:?}, create=0) -> M_E_NOENT");

                return Ok(M_E_NOENT);
            }
            None => {
                let system = context.system();
                let pid = system.pid().to_owned();

                // Opening it is what creates it.
                system.platform().database_repository().open(&stored, &pid).await;
            }
        }
    }

    let name_bytes = name.as_bytes();
    let mut handle = RecordDatabaseHandle {
        magic: RECORD_HANDLE_MAGIC,
        name: [0; 32],
        record_size,
    };
    handle.name[..name_bytes.len()].copy_from_slice(name_bytes);

    let ptr_handle = context.alloc_raw(size_of::<RecordDatabaseHandle>() as _)?;
    write_generic(context, ptr_handle, handle)?;

    tracing::debug!("MC_rdbOpen({name:?}, record_size={record_size}, create={create}) -> {ptr_handle:#x}");

    Ok(ptr_handle as _)
}

/// `MC_rdbClose(handle)` - slot 1.
pub async fn close(context: &mut dyn WIPICContext, handle: WIPICWord) -> Result<i32> {
    tracing::debug!("MC_rdbClose({handle:#x})");

    let Some(mut loaded) = load_handle(context, handle)? else {
        return Ok(M_E_INVALID);
    };

    // The block stays allocated - a title may close and reopen through the same
    // pointer - but the magic goes, so a stale handle is refused rather than
    // read as a live one.
    loaded.magic = 0;
    write_generic(context, handle, loaded)?;

    Ok(0)
}

/// `MC_rdbDelete(name)` - slot 2.
pub async fn delete_database(context: &mut dyn WIPICContext, ptr_name: WIPICWord) -> Result<i32> {
    let Ok(name) = String::from_utf8(read_null_terminated_string_bytes(context, ptr_name)?) else {
        return Ok(M_E_INVALID);
    };

    let system = context.system();
    let pid = system.pid().to_owned();
    let deleted = system.platform().database_repository().delete(&store_name(&name), &pid).await;

    tracing::debug!("MC_rdbDelete({name:?}) -> {deleted}");

    Ok(if deleted { 0 } else { M_E_NOENT })
}

/// `MC_rdbInsertRecord(handle, buf, len)` - slot 3, answering the new id.
pub async fn insert_record(context: &mut dyn WIPICContext, handle: WIPICWord, buf: WIPICWord, len: WIPICWord) -> Result<i32> {
    let Some(loaded) = load_handle(context, handle)? else {
        return Ok(M_E_INVALID);
    };
    let Some(name) = handle_name(&loaded) else {
        return Ok(M_E_INVALID);
    };

    if len as usize > MAX_RECORD_BYTES {
        return Ok(M_E_INVALID);
    }

    let mut data = vec![0; len as usize];
    if len != 0 {
        context.read_bytes(buf, &mut data)?;
    }

    let system = context.system();
    let pid = system.pid().to_owned();
    let mut db = system.platform().database_repository().open(&store_name(&name), &pid).await;
    let id = db.add(&data).await;

    tracing::debug!("MC_rdbInsertRecord({handle:#x}, {len} bytes) -> id {id}");

    Ok(id as i32)
}

/// `MC_rdbSelectRecord(handle, id, buf, len)` - slot 4.
pub async fn select_record(context: &mut dyn WIPICContext, handle: WIPICWord, id: WIPICWord, buf: WIPICWord, len: WIPICWord) -> Result<i32> {
    let Some(loaded) = load_handle(context, handle)? else {
        return Ok(M_E_INVALID);
    };
    let Some(name) = handle_name(&loaded) else {
        return Ok(M_E_INVALID);
    };

    let record = {
        let system = context.system();
        let pid = system.pid().to_owned();
        let db = system.platform().database_repository().open(&store_name(&name), &pid).await;

        db.get(id).await
    };

    let Some(record) = record else {
        tracing::debug!("MC_rdbSelectRecord({handle:#x}, {id}) -> M_E_INVALID (no such record)");

        return Ok(M_E_INVALID);
    };

    if (len as usize) < record.len() {
        return Ok(M_E_SHORTBUF);
    }

    if !record.is_empty() {
        context.write_bytes(buf, &record)?;
    }

    tracing::debug!("MC_rdbSelectRecord({handle:#x}, {id}) -> {} bytes", record.len());

    Ok(0)
}

/// `MC_rdbUpdateRecord(handle, id, buf, len)` - slot 5.
pub async fn update_record(context: &mut dyn WIPICContext, handle: WIPICWord, id: WIPICWord, buf: WIPICWord, len: WIPICWord) -> Result<i32> {
    let Some(loaded) = load_handle(context, handle)? else {
        return Ok(M_E_INVALID);
    };
    let Some(name) = handle_name(&loaded) else {
        return Ok(M_E_INVALID);
    };

    if len as usize > MAX_RECORD_BYTES {
        return Ok(M_E_INVALID);
    }

    let mut data = vec![0; len as usize];
    if len != 0 {
        context.read_bytes(buf, &mut data)?;
    }

    let system = context.system();
    let pid = system.pid().to_owned();
    let mut db = system.platform().database_repository().open(&store_name(&name), &pid).await;
    let updated = db.set(id, &data).await;

    tracing::debug!("MC_rdbUpdateRecord({handle:#x}, {id}, {len} bytes) -> {updated}");

    Ok(if updated { 0 } else { M_E_INVALID })
}

/// `MC_rdbDeleteRecord(handle, id)` - slot 6.
///
/// The slot carries two call shapes with the same signature: this one, and a
/// name-keyed deletion of the whole database. A handle this runtime issued is
/// the only thing that tells them apart, so anything else is read as the name.
pub async fn delete_record(context: &mut dyn WIPICContext, handle: WIPICWord, id: WIPICWord) -> Result<i32> {
    let Some(loaded) = load_handle(context, handle)? else {
        return delete_database(context, handle).await;
    };
    let Some(name) = handle_name(&loaded) else {
        return Ok(M_E_INVALID);
    };

    let system = context.system();
    let pid = system.pid().to_owned();
    let mut db = system.platform().database_repository().open(&store_name(&name), &pid).await;
    let deleted = db.delete(id).await;

    tracing::debug!("MC_rdbDeleteRecord({handle:#x}, {id}) -> {deleted}");

    Ok(if deleted { 0 } else { M_E_INVALID })
}

/// `MC_rdbListRecords(handle, buf, capacity)` - slot 7, the record ids.
pub async fn list_records(context: &mut dyn WIPICContext, handle: WIPICWord, buf: WIPICWord, capacity: WIPICWord) -> Result<i32> {
    let Some(loaded) = load_handle(context, handle)? else {
        return Ok(M_E_INVALID);
    };
    let Some(name) = handle_name(&loaded) else {
        return Ok(M_E_INVALID);
    };

    let ids = records(context, &name).await.into_iter().map(|(id, _)| id).collect::<Vec<_>>();

    // The count is in ids, as the capacity is: a buffer that cannot hold them
    // is refused and left alone rather than half-filled.
    if (ids.len() as u32) > capacity {
        return Ok(M_E_SHORTBUF);
    }

    for (index, id) in ids.iter().enumerate() {
        write_generic(context, buf + (index as u32 * 4), *id)?;
    }

    tracing::debug!("MC_rdbListRecords({handle:#x}) -> {} records", ids.len());

    Ok(ids.len() as i32)
}

/// `MC_rdbGetNumberOfRecords(handle)` - slot 10.
pub async fn number_of_records(context: &mut dyn WIPICContext, handle: WIPICWord) -> Result<i32> {
    let Some(loaded) = load_handle(context, handle)? else {
        return Ok(M_E_INVALID);
    };
    let Some(name) = handle_name(&loaded) else {
        return Ok(M_E_INVALID);
    };

    let count = records(context, &name).await.len();

    tracing::debug!("MC_rdbGetNumberOfRecords({handle:#x}) -> {count}");

    Ok(count as i32)
}

/// `MC_rdbGetRecordSize(handle)` - slot 11.
///
/// The size the open asked for, which is what the reference reports: these
/// titles write fixed-size records of their own and nothing here enforces one.
pub async fn record_size(context: &mut dyn WIPICContext, handle: WIPICWord) -> Result<i32> {
    let Some(loaded) = load_handle(context, handle)? else {
        return Ok(M_E_INVALID);
    };

    let size = loaded.record_size;
    tracing::debug!("MC_rdbGetRecordSize({handle:#x}) -> {size}");

    Ok(size as i32)
}

#[cfg(test)]
mod tests {
    use alloc::{vec, vec::Vec};

    use super::{PACKAGED_HEADER, misread_one_file, parse_one_file};

    fn one_file(record_size: u32, count: u32, body: &[u8]) -> Vec<u8> {
        let mut data = vec![0; PACKAGED_HEADER];
        data[..5].copy_from_slice(b"qtcdb");
        data[5..9].copy_from_slice(&record_size.to_be_bytes());
        data[9..13].copy_from_slice(&count.to_be_bytes());
        data.extend_from_slice(body);

        data
    }

    #[test]
    fn marked_records_are_read_without_their_marker() {
        // 드래곤로드's FUNTER_DL.db, whose first record is its save key.
        let data = one_file(8, 2, b"\x01YVQZZQEX\x01EIHNTSAZ");

        assert_eq!(parse_one_file(&data), Some(vec![b"YVQZZQEX".to_vec(), b"EIHNTSAZ".to_vec()]));
        assert_eq!(misread_one_file(&data), Some(vec![b"\x01YVQZZQE".to_vec(), b"X\x01EIHNTS".to_vec()]));
    }

    #[test]
    fn unmarked_records_are_read_as_they_are() {
        let data = one_file(4, 2, b"abcdefgh");

        assert_eq!(parse_one_file(&data), Some(vec![b"abcd".to_vec(), b"efgh".to_vec()]));
        assert_eq!(misread_one_file(&data), None);
    }
}
