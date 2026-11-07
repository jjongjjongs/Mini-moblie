//! A game's icon, out of its archive, for the list - read as the Android
//! and iOS apps read theirs (see `wie_android::icon`).

use std::path::Path;

use crate::ui::Picture;

/// The icon of the game in `path`, if it has one that reads.
pub(crate) fn extract(path: &Path) -> Option<Picture> {
    let data = std::fs::read(path).ok()?;
    let icon = wie_android::icon::extract(&data)?;
    Some(Picture {
        width: icon.width,
        height: icon.height,
        rgba: icon.rgba,
    })
}
