use alloc::{collections::BTreeMap, format, string::String, vec::Vec};

/// A database the handset already had, as the package carries it.
///
/// KTF's `org.kwis.msp.db.DataBase` is a pair of files on the handset: a title
/// that opens `/D/FConfig` is opening `/D/FConfig.db` beside `/D/FConfig.idx`.
/// An archive's `P/` directory is the handset's own copy of its data, so a
/// package that has been through a download once carries the databases that
/// download left behind.
///
/// 파랜드택틱스 is what needs them. Its data lives in `P/D` and `P/I`, and
/// before it will play it opens `/D/FConfig` and asks how many records are in
/// it. Told none, it decides its data is missing and offers to download its ten
/// files again - from a server that has been gone for years, so it waits on the
/// first of them for ever.
#[derive(Debug, PartialEq, Eq)]
pub struct PackagedDatabase {
    /// The name a title opens it by: the packaged path without its extension,
    /// rooted the way the handset roots it.
    pub name: String,
    pub records: Vec<Vec<u8>>,
}

/// The `.idx` header: the signature, then the record size and the number of
/// records, both big-endian words.
const SIGNATURE: &[u8] = b"qtpdb";
const RECORD_SIZE_OFFSET: usize = SIGNATURE.len();
const RECORD_COUNT_OFFSET: usize = RECORD_SIZE_OFFSET + 4;
const HEADER_LEN: usize = RECORD_COUNT_OFFSET + 4;

/// Every database the package carries, in the order their index files are
/// named.
///
/// `files` is keyed by packaged path - what `packaged_name` leaves of an
/// archive entry - so `P/D/FConfig.idx` arrives here as `D/FConfig.idx`.
pub fn packaged_databases(files: &BTreeMap<String, Vec<u8>>) -> Vec<PackagedDatabase> {
    files
        .iter()
        .filter_map(|(path, index)| {
            let stem = path.strip_suffix(".idx")?;
            let data = files.get(&format!("{stem}.db"))?;
            let records = records_of(index, data)?;

            Some(PackagedDatabase {
                name: format!("/{stem}"),
                records,
            })
        })
        .collect()
}

/// The records an index and its data file hold, or `None` for a pair that is
/// not one of these databases.
///
/// A record is a fixed-size slice of the data file, and the index says how big
/// and how many. A file too short for what the index claims is not taken as far
/// as it goes: a database read back wrong is worse than one that was not there,
/// because a title believes it.
fn records_of(index: &[u8], data: &[u8]) -> Option<Vec<Vec<u8>>> {
    if index.len() < HEADER_LEN || !index.starts_with(SIGNATURE) {
        return None;
    }

    let record_size = big_endian_word(index, RECORD_SIZE_OFFSET) as usize;
    let record_count = big_endian_word(index, RECORD_COUNT_OFFSET) as usize;

    if record_size == 0 {
        return None;
    }

    let wanted = record_size.checked_mul(record_count)?;
    if data.len() < wanted {
        return None;
    }

    Some(data[..wanted].chunks(record_size).map(<[u8]>::to_vec).collect())
}

fn big_endian_word(bytes: &[u8], offset: usize) -> u32 {
    u32::from_be_bytes([bytes[offset], bytes[offset + 1], bytes[offset + 2], bytes[offset + 3]])
}

#[cfg(test)]
mod test {
    use alloc::{collections::BTreeMap, vec, vec::Vec};

    use super::{PackagedDatabase, packaged_databases};

    /// An index file the way the handset writes one.
    fn index(record_size: u32, record_count: u32) -> Vec<u8> {
        let mut index = b"qtpdb".to_vec();
        index.extend_from_slice(&record_size.to_be_bytes());
        index.extend_from_slice(&record_count.to_be_bytes());
        index.resize(45, 0);
        index
    }

    fn files(entries: &[(&str, Vec<u8>)]) -> BTreeMap<alloc::string::String, Vec<u8>> {
        entries.iter().map(|(path, data)| ((*path).into(), data.clone())).collect()
    }

    /// The pair 파랜드택틱스 carries, cut into records the way its index says.
    #[test]
    fn an_index_and_its_data_become_the_records_the_index_counts() {
        let found = packaged_databases(&files(&[
            ("D/FConfig.idx", index(4, 3)),
            ("D/FConfig.db", vec![1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3]),
        ]));

        assert_eq!(
            found,
            [PackagedDatabase {
                name: "/D/FConfig".into(),
                records: vec![vec![1; 4], vec![2; 4], vec![3; 4]],
            }]
        );
    }

    /// The index is what says how much of the data file is records; 파랜드택틱스'
    /// own files are exactly `record_size * count` long, but a longer one keeps
    /// only what was counted.
    #[test]
    fn data_past_the_last_record_is_not_a_record() {
        let found = packaged_databases(&files(&[("d.idx", index(2, 1)), ("d.db", vec![7, 7, 9, 9])]));

        assert_eq!(found[0].records, [vec![7, 7]]);
    }

    /// A short, unsigned or sizeless pair is not one of these, and a `.idx`
    /// without its `.db` is not either.
    #[test]
    fn only_a_whole_pair_is_a_database() {
        assert!(packaged_databases(&files(&[("d.idx", index(4, 1))])).is_empty());
        assert!(packaged_databases(&files(&[("d.idx", index(4, 2)), ("d.db", vec![0; 4])])).is_empty());
        assert!(packaged_databases(&files(&[("d.idx", index(0, 1)), ("d.db", vec![0; 4])])).is_empty());
        assert!(packaged_databases(&files(&[("d.idx", vec![0; 45]), ("d.db", vec![0; 4])])).is_empty());
        assert!(packaged_databases(&files(&[("d.idx", b"qtpdb".to_vec()), ("d.db", vec![0; 4])])).is_empty());
    }

    /// A database with nothing in it is still one, and answering it as one is
    /// what keeps a title from being told it is missing.
    #[test]
    fn a_counted_zero_is_an_empty_database() {
        let found = packaged_databases(&files(&[("d.idx", index(4, 0)), ("d.db", Vec::new())]));

        assert_eq!(found[0].records.len(), 0);
    }
}
