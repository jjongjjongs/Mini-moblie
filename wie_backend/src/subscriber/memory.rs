//! The subscriber number this handset has been told by an archive, kept across
//! titles.
//!
//! Stored in the database repository under an app id no title has, so it
//! outlives a title's own saves and a reset of them, and is reached the same
//! way on every platform. See [`super::subscriber_number_on_handset`].

use alloc::string::String;

use crate::DatabaseRepository;

/// The app id the number is kept under. No title's id has underscores in it.
const APP_ID: &str = "__wie_handset__";
/// The store, and its one record.
const NAME: &str = "subscriber";
const RECORD: u32 = 1;

/// The number an archive last named on this handset, if any has.
pub async fn remembered(repository: &dyn DatabaseRepository) -> Option<String> {
    if !repository.exists(NAME, APP_ID).await {
        return None;
    }
    let data = repository.open(NAME, APP_ID).await.get(RECORD).await?;

    String::from_utf8(data).ok().filter(|number| !number.is_empty())
}

/// Keeps `number` as the one this handset was told, unless it already is.
pub async fn remember(repository: &dyn DatabaseRepository, number: &str) {
    if remembered(repository).await.as_deref() == Some(number) {
        return;
    }

    tracing::info!("remembering subscriber number {number:?} for this handset");
    repository.open(NAME, APP_ID).await.set(RECORD, number.as_bytes()).await;
}
