use alloc::{boxed::Box, string::String, vec::Vec};

pub type RecordId = u32;

#[async_trait::async_trait]
pub trait Database: Send {
    async fn next_id(&self) -> RecordId;
    async fn add(&mut self, data: &[u8]) -> RecordId;
    async fn get(&self, id: RecordId) -> Option<Vec<u8>>;
    async fn set(&mut self, id: RecordId, data: &[u8]) -> bool;
    async fn delete(&mut self, id: RecordId) -> bool;

    async fn get_record_ids(&self) -> Vec<RecordId>;
}

/// `Send + Sync` because a repository is reached through the shared `Platform`
/// and used from the emulator's own task, which has to be able to move between
/// threads. Every implementation is already behind a lock.
#[async_trait::async_trait]
pub trait DatabaseRepository: Send + Sync {
    async fn open(&self, name: &str, app_id: &str) -> Box<dyn Database>;
    async fn exists(&self, name: &str, app_id: &str) -> bool;
    async fn delete(&self, name: &str, app_id: &str) -> bool;

    /// Lists database names directly rooted in the application's database
    /// namespace. Nested storage directories are not themselves databases.
    async fn list(&self, app_id: &str) -> Vec<String>;

    /// Whether a store by this name holds at least one record.
    ///
    /// A store name is free-form and may be nested - 초밥의달인3 keeps its save
    /// in one called `file/data` - so it can name a directory that is not a
    /// direct child of the application's namespace. [`list`](Self::list) only
    /// enumerates that top level, so it never sees such a store and cannot be
    /// asked whether it is there. This resolves the whole name and answers for
    /// the store it actually points at.
    ///
    /// "Holds a record" rather than merely "was opened": the runtime creates a
    /// store's directory as soon as it is read, so its bare presence does not
    /// tell a written save from one a title only ever opened, and a title asks
    /// this to tell a save it can load from none.
    async fn has_records(&self, name: &str, app_id: &str) -> bool;
}
