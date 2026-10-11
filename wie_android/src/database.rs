use std::{fs, path::PathBuf};

use wie_backend::RecordId;

/// Record stores rooted at the app-private directory, laid out as
/// `<base>/<app_id>/<name>/<record id>`.
pub struct AndroidDatabaseRepository {
    base_path: PathBuf,
}

impl AndroidDatabaseRepository {
    pub fn new(base_path: PathBuf) -> Self {
        Self { base_path }
    }

    fn path_for_database(&self, name: &str, app_id: &str) -> PathBuf {
        let sanitized_app_id: String = app_id.chars().filter(|c| !matches!(c, '/' | '\\' | '\0')).collect();
        let app_id = if sanitized_app_id.is_empty() || sanitized_app_id == "." || sanitized_app_id == ".." {
            "_"
        } else {
            &sanitized_app_id
        };

        // Guest database names are free-form and routinely contain a leading
        // slash, so they are normalized into a relative path instead of being
        // rejected - a rejected name would lose the game's save data.
        let name: String = name.chars().map(|c| if matches!(c, '\\' | '\0') { '_' } else { c }).collect();
        let mut normalized_name = PathBuf::new();
        for segment in name.trim_start_matches('/').split('/') {
            match segment {
                "" | "." => {}
                ".." => normalized_name.push("_"),
                segment => normalized_name.push(segment),
            }
        }
        if normalized_name.as_os_str().is_empty() {
            normalized_name.push("_");
        }

        self.base_path.join(app_id).join(normalized_name)
    }

    fn list_databases(&self, app_id: &str) -> Vec<String> {
        let sanitized_app_id: String = app_id.chars().filter(|c| !matches!(c, '/' | '\\' | '\0')).collect();
        let app_id = if sanitized_app_id.is_empty() || sanitized_app_id == "." || sanitized_app_id == ".." {
            "_"
        } else {
            &sanitized_app_id
        };

        let mut names = Vec::new();
        Self::collect_stores(&self.base_path.join(app_id), "", 0, &mut names);
        names.sort();
        names
    }

    /// Whether a directory directly holds a record, which is what makes it a
    /// store rather than a step on the way to one.
    fn directory_holds_a_record(dir: &std::path::Path) -> bool {
        fs::read_dir(dir)
            .ok()
            .into_iter()
            .flatten()
            .filter_map(|record| record.ok())
            .any(|record| record.path().is_file() && record.file_name().to_str().is_some_and(|name| name.parse::<RecordId>().is_ok()))
    }

    /// Every store rooted under `dir`, by the name a title opens it with.
    ///
    /// A store name is free-form and may carry a slash - 초밥의달인3 keeps its
    /// save in one called `file/data` - so a store is not always a direct child
    /// of the application's namespace: it can be nested a level down, under an
    /// intermediate directory that is only part of the name and holds no record
    /// of its own. Walking the tree and naming every directory that holds a
    /// record (joined back with `/`) is what lets `listDataBases` see such a
    /// store, so a save written under a nested name is offered on the load
    /// screen rather than passed over. The reference keeps the same names in an
    /// explicit `rms/.index`; this reads them back off the tree instead.
    fn collect_stores(dir: &std::path::Path, prefix: &str, depth: usize, names: &mut Vec<String>) {
        // A save tree is a handful of levels at most; the bound only stops a
        // crafted or corrupt tree from recursing without end.
        const MAX_DEPTH: usize = 16;
        if depth >= MAX_DEPTH {
            return;
        }

        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };

        for entry in entries.filter_map(|entry| entry.ok()) {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let Ok(component) = entry.file_name().into_string() else {
                continue;
            };
            let name = if prefix.is_empty() { component } else { format!("{prefix}/{component}") };

            if Self::directory_holds_a_record(&path) {
                names.push(name.clone());
            }

            Self::collect_stores(&path, &name, depth + 1, names);
        }
    }
}

#[async_trait::async_trait]
impl wie_backend::DatabaseRepository for AndroidDatabaseRepository {
    async fn open(&self, name: &str, app_id: &str) -> Box<dyn wie_backend::Database> {
        let path = self.path_for_database(name, app_id);

        if let Err(error) = fs::create_dir_all(&path) {
            tracing::warn!("Failed to create database at {path:?}: {error}");
        }

        Box::new(AndroidDatabase { base_path: path })
    }

    async fn exists(&self, name: &str, app_id: &str) -> bool {
        self.path_for_database(name, app_id).exists()
    }

    async fn delete(&self, name: &str, app_id: &str) -> bool {
        let path = self.path_for_database(name, app_id);

        match fs::remove_dir_all(&path) {
            Ok(()) => true,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(error) => {
                tracing::warn!("Failed to delete database at {path:?}: {error}");
                false
            }
        }
    }

    async fn list(&self, app_id: &str) -> Vec<String> {
        self.list_databases(app_id)
    }

    async fn has_records(&self, name: &str, app_id: &str) -> bool {
        let path = self.path_for_database(name, app_id);

        fs::read_dir(&path)
            .ok()
            .into_iter()
            .flatten()
            .filter_map(|record| record.ok())
            .any(|record| record.path().is_file() && record.file_name().to_str().is_some_and(|name| name.parse::<RecordId>().is_ok()))
    }
}

struct AndroidDatabase {
    base_path: PathBuf,
}

impl AndroidDatabase {
    fn find_empty_record_id(&self) -> RecordId {
        let mut record_id = 1; // XXX midp requires first record to be 1

        while self.path_for_record(record_id).exists() {
            record_id += 1;
        }

        record_id
    }

    fn path_for_record(&self, id: RecordId) -> PathBuf {
        self.base_path.join(id.to_string())
    }
}

#[async_trait::async_trait]
impl wie_backend::Database for AndroidDatabase {
    async fn next_id(&self) -> RecordId {
        self.find_empty_record_id()
    }

    async fn add(&mut self, data: &[u8]) -> RecordId {
        let id = self.find_empty_record_id();

        if let Err(error) = fs::write(self.path_for_record(id), data) {
            tracing::warn!("Failed to add record {id} to {:?}: {error}", self.base_path);
        }

        id
    }

    async fn get(&self, id: RecordId) -> Option<Vec<u8>> {
        fs::read(self.path_for_record(id)).ok()
    }

    async fn set(&mut self, id: RecordId, data: &[u8]) -> bool {
        fs::write(self.path_for_record(id), data).is_ok()
    }

    async fn delete(&mut self, id: RecordId) -> bool {
        fs::remove_file(self.path_for_record(id)).is_ok()
    }

    async fn get_record_ids(&self) -> Vec<RecordId> {
        let Ok(entries) = fs::read_dir(&self.base_path) else {
            return Vec::new();
        };

        entries
            .filter_map(|x| x.ok())
            .filter(|x| x.path().is_file())
            .filter_map(|x| x.file_name().to_str()?.parse().ok())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::AndroidDatabaseRepository;

    #[test]
    fn database_list_returns_direct_and_nested_stores_but_not_bare_parents() {
        use std::{
            fs,
            time::{SystemTime, UNIX_EPOCH},
        };
        let unique = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let base = std::env::temp_dir().join(format!("wie_android_db_list_{}_{}", std::process::id(), unique));

        let repository = AndroidDatabaseRepository::new(base.clone());

        // Canonical LGT databases contain reserved metadata record 0.
        let direct = base.join("test-aid").join("root");
        fs::create_dir_all(&direct).unwrap();
        fs::write(direct.join("0"), b"metadata").unwrap();

        // Logical name "parent/child": a store saved under a nested name, the
        // way 초밥의달인3 saves under "file/data". The store is listed by its
        // full name; the "parent" directory above it, which holds no record of
        // its own, is not - it is only a step on the way to the name.
        let nested = base.join("test-aid").join("parent").join("child");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("0"), b"metadata").unwrap();

        let names = repository.list_databases("test-aid");
        assert_eq!(names, vec!["parent/child".to_string(), "root".to_string()]);

        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn database_path_normalizes_guest_names() {
        let repository = AndroidDatabaseRepository::new(PathBuf::from("/data/db"));

        assert_eq!(
            repository.path_for_database("records", "game123"),
            PathBuf::from("/data/db/game123/records")
        );
        assert_eq!(
            repository.path_for_database("/save0.dat", "PD140106"),
            PathBuf::from("/data/db/PD140106/save0.dat")
        );
        assert!(
            repository
                .path_for_database("/../save0.dat", "PD140106")
                .starts_with(PathBuf::from("/data/db/PD140106"))
        );
    }
}
