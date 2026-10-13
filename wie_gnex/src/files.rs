//! The handset's files, as the scripts' file services see them: named by a
//! string, opened to a handle, read and written as 32-bit words or as bytes.
//! They live in the system's filesystem, so what a title saves is still there
//! the next time it runs, and the data files a package ships beside its
//! script are there to read from the start.
//!
//! The filesystem's calls are asynchronous, but every platform answers them
//! at once; the interpreter runs a callback to its end without yielding, so a
//! call that does not answer at once is taken as having failed.

use alloc::{string::String, vec, vec::Vec};

use futures::FutureExt;
use wie_backend::System;

/// How `open` treats the file.
#[derive(Clone, Copy)]
enum Mode {
    /// An existing file, to read and write.
    Existing,
    /// A new, empty file, replacing any there was.
    Create,
    /// An existing file, to read.
    Read,
}

struct Open {
    path: String,
    position: usize,
}

pub struct Files {
    system: Option<System>,
    handles: Vec<Option<Open>>,
}

impl Files {
    pub fn new(system: Option<System>) -> Self {
        Self { system, handles: Vec::new() }
    }

    fn path(name: &[u8]) -> String {
        let (text, _, _) = encoding_rs::EUC_KR.decode(name);
        text.trim_start_matches('/').into()
    }

    pub fn exists(&self, name: &[u8]) -> bool {
        let Some(system) = &self.system else { return false };
        let path = Self::path(name);
        system.filesystem().exists(&path).now_or_never().unwrap_or(false)
    }

    pub fn is_directory(&self, name: &[u8]) -> bool {
        let Some(system) = &self.system else { return false };
        let path = Self::path(name);
        system.filesystem().is_directory(&path).now_or_never().unwrap_or(false)
    }

    pub fn make_directory(&self, name: &[u8]) -> bool {
        let Some(system) = &self.system else { return false };
        let path = Self::path(name);
        matches!(system.filesystem().mkdir(&path).now_or_never(), Some(Ok(())))
    }

    pub fn size(&self, name: &[u8]) -> Option<usize> {
        let system = self.system.as_ref()?;
        let path = Self::path(name);
        system.filesystem().size(&path).now_or_never().flatten()
    }

    pub fn remove(&self, name: &[u8]) -> bool {
        let Some(system) = &self.system else { return false };
        let path = Self::path(name);
        system.filesystem().remove(&path).now_or_never().unwrap_or(false)
    }

    /// A handle, or -1. Mode 1 opens an existing file, 2 creates one afresh
    /// and 3 opens an existing one to read.
    pub fn open(&mut self, name: &[u8], mode: i32) -> i32 {
        let Some(system) = &self.system else { return -1 };
        let mode = match mode {
            2 => Mode::Create,
            3 => Mode::Read,
            _ => Mode::Existing,
        };
        let path = Self::path(name);
        let filesystem = system.filesystem();
        match mode {
            Mode::Create => {
                if filesystem.write(&path, 0, &[]).now_or_never().is_none() {
                    return -1;
                }
                filesystem.truncate(&path, 0).now_or_never();
            }
            Mode::Existing | Mode::Read => {
                if !filesystem.exists(&path).now_or_never().unwrap_or(false) {
                    return -1;
                }
            }
        }

        let open = Some(Open { path, position: 0 });
        match self.handles.iter().position(|x| x.is_none()) {
            Some(free) => {
                self.handles[free] = open;
                free as i32 + 1
            }
            None => {
                self.handles.push(open);
                self.handles.len() as i32
            }
        }
    }

    fn handle(&mut self, handle: i32) -> Option<&mut Open> {
        self.handles.get_mut((handle - 1) as usize)?.as_mut()
    }

    pub fn close(&mut self, handle: i32) -> bool {
        match self.handles.get_mut((handle - 1) as usize) {
            Some(slot @ Some(_)) => {
                *slot = None;
                true
            }
            _ => false,
        }
    }

    /// Moves to `offset` bytes from the start (origin 0 or 1), from where it
    /// is (2), or from the end (anything else).
    pub fn seek(&mut self, handle: i32, offset: i32, origin: i32) -> bool {
        let size = {
            let Some(open) = self.handle(handle) else { return false };
            let path = open.path.clone();
            self.system
                .as_ref()
                .and_then(|x| x.filesystem().size(&path).now_or_never().flatten())
                .unwrap_or(0)
        };
        let Some(open) = self.handle(handle) else { return false };
        let base = match origin {
            0 | 1 => 0,
            2 => open.position as i64,
            _ => size as i64,
        };
        open.position = (base + offset as i64).max(0) as usize;
        true
    }

    pub fn read(&mut self, handle: i32, count: usize) -> Option<Vec<u8>> {
        let system = self.system.clone()?;
        let open = self.handle(handle)?;
        let mut buffer = vec![0; count];
        let read = system
            .filesystem()
            .read(&open.path, open.position, count, &mut buffer)
            .now_or_never()
            .flatten()?;
        buffer.truncate(read);
        open.position += read;
        Some(buffer)
    }

    pub fn write(&mut self, handle: i32, data: &[u8]) -> Option<usize> {
        let system = self.system.clone()?;
        let open = self.handle(handle)?;
        let written = system.filesystem().write(&open.path, open.position, data).now_or_never()?;
        open.position += written;
        Some(written)
    }
}
