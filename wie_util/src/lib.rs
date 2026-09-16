#![no_std]
extern crate alloc;

use alloc::{format, string::String, vec::Vec};
use core::{
    any::Any,
    error::Error,
    fmt::{self, Display, Formatter},
    mem::{MaybeUninit, size_of},
    result,
    slice::from_raw_parts_mut,
};

use bytemuck::{AnyBitPattern, NoUninit, bytes_of};

#[derive(Debug)]
pub enum WieError {
    InvalidMemoryAccess(u32),
    AllocationFailure,
    JavaException(u32), // to pass java exception down to rust
    /// A guest `try` matched, and this is the long jump back into it.
    ///
    /// `frame_sp` is the stack pointer the handler's own frame saved, and it says
    /// which guest call the catch block belongs to: the guest stack is shared by
    /// every nested call the host has open, so a handler saved above a call's
    /// entry belongs to a caller the host has not returned to yet.
    JavaExceptionUnwind {
        context_base: u32,
        target: u32,
        next_pc: u32,
        frame_sp: u32,
    },
    Unimplemented(String),
    FatalError(String),
}

impl Display for WieError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            WieError::InvalidMemoryAccess(address) => write!(f, "Invalid memory access; address: {address}"),
            WieError::AllocationFailure => write!(f, "Allocation failure"),
            WieError::JavaException(exception) => write!(f, "Java exception: {exception:#x}"),
            WieError::JavaExceptionUnwind {
                context_base,
                target,
                next_pc,
                frame_sp,
            } => write!(
                f,
                "Java exception unwind: context_base={context_base:#x}, target={target:#x}, next_pc={next_pc:#x}, frame_sp={frame_sp:#x}"
            ),
            WieError::Unimplemented(message) => write!(f, "Unimplemented: {message}"),
            WieError::FatalError(message) => write!(f, "Fatal error: {message}"),
        }
    }
}

impl Error for WieError {}

pub type Result<T> = result::Result<T, WieError>;

pub trait ByteRead {
    fn read_bytes(&self, address: u32, result: &mut [u8]) -> Result<usize>;
}

pub trait ByteWrite {
    fn write_bytes(&mut self, address: u32, data: &[u8]) -> Result<()>;
}

pub fn read_generic<T, R>(reader: &R, address: u32) -> Result<T>
where
    T: Copy + AnyBitPattern + NoUninit,
    R: ?Sized + ByteRead,
{
    if address == 0 {
        return Err(WieError::InvalidMemoryAccess(address));
    }

    let mut destination = MaybeUninit::<T>::uninit();
    let destination_bytes = unsafe { from_raw_parts_mut(destination.as_mut_ptr().cast::<u8>(), size_of::<T>()) };
    let read = reader.read_bytes(address, destination_bytes)?;
    if read != destination_bytes.len() {
        return Err(WieError::FatalError(format!(
            "Short read at {address:#x}: expected {}, got {read}",
            destination_bytes.len()
        )));
    }

    Ok(unsafe { destination.assume_init() })
}

/// How much of a string to ask for at once, and the alignment a request is kept
/// inside.
///
/// A byte at a time is what this used to read, and a byte costs whatever the
/// reader costs: on `ArmCore` that is a mutex, a dynamic call and a page lookup
/// per character, and the guest's Java records are all named by strings - a
/// field's name is read again on every `get_field`, its class's on every lookup
/// that walks to it. A title whose paint loop reaches for a field per pixel
/// pays for those characters more than for its own drawing (귀혼 무사편 plots
/// its screen through `Graphics.setRGBPixels(x, y, 1, 1, ...)`, so a frame is
/// thousands of field lookups and tens of thousands of these reads).
///
/// The span is kept inside one 4 KiB page because a reader may serve memory in
/// pages and refuse a request that leaves a mapped one - `ArmCore` does - and
/// the terminator is normally a few characters away, not a page.
const STRING_CHUNK: u32 = 64;
const STRING_CHUNK_ALIGNMENT: u32 = 0x1000;

pub fn read_null_terminated_string_bytes<R>(reader: &R, address: u32) -> Result<Vec<u8>>
where
    R: ?Sized + ByteRead,
{
    if address == 0 {
        return Err(WieError::InvalidMemoryAccess(address));
    }

    let mut result = Vec::with_capacity(STRING_CHUNK as usize);
    let mut cursor = address;
    let mut chunk = [0; STRING_CHUNK as usize];
    loop {
        // A span that stops at the page the cursor is in, so a string at the end
        // of one never asks for the next.
        let span = STRING_CHUNK.min(STRING_CHUNK_ALIGNMENT - (cursor & (STRING_CHUNK_ALIGNMENT - 1))) as usize;
        if span > 1
            && let Ok(read) = reader.read_bytes(cursor, &mut chunk[..span])
            && read > 0
        {
            if let Some(end) = chunk[..read].iter().position(|byte| *byte == 0) {
                result.extend_from_slice(&chunk[..end]);
                return Ok(result);
            }

            result.extend_from_slice(&chunk[..read]);
            cursor += read as u32;

            continue;
        }

        // Whatever the reader would not serve as a span it still has to answer
        // for a byte, so the refusal a caller sees is the one it always saw.
        let mut byte = [0; 1];
        let read = reader.read_bytes(cursor, &mut byte)?;
        if read != 1 {
            return Err(WieError::FatalError(format!("Short read at {cursor:#x}: expected 1, got {read}")));
        }

        if byte[0] == 0 {
            return Ok(result);
        }

        result.push(byte[0]);
        cursor += 1;
    }
}

pub fn write_null_terminated_string_bytes<W>(writer: &mut W, address: u32, bytes: &[u8]) -> Result<()>
where
    W: ?Sized + ByteWrite,
{
    if address == 0 {
        return Err(WieError::InvalidMemoryAccess(address));
    }

    // TODO temp
    writer.write_bytes(address, bytes)?;
    writer.write_bytes(address + bytes.len() as u32, &[0])?;

    Ok(())
}

pub fn write_generic<W, T>(writer: &mut W, address: u32, data: T) -> Result<()>
where
    W: ?Sized + ByteWrite,
    T: NoUninit,
{
    if address == 0 {
        return Err(WieError::InvalidMemoryAccess(address));
    }

    let data_slice = bytes_of(&data);

    writer.write_bytes(address, data_slice)
}

pub fn read_null_terminated_table<R>(reader: &R, base_address: u32) -> Result<Vec<u32>>
where
    R: ?Sized + ByteRead,
{
    if base_address == 0 {
        return Err(WieError::InvalidMemoryAccess(base_address));
    }

    let mut cursor = base_address;
    let mut result = Vec::new();
    loop {
        let item: u32 = read_generic(reader, cursor)?;
        if item == 0 {
            break;
        }
        result.push(item);

        cursor += 4;
    }

    Ok(result)
}

pub fn write_null_terminated_table<W>(writer: &mut W, base_address: u32, items: &[u32]) -> Result<()>
where
    W: ?Sized + ByteWrite,
{
    if base_address == 0 {
        return Err(WieError::InvalidMemoryAccess(base_address));
    }

    let mut cursor = base_address;
    for &item in items {
        write_generic(writer, cursor, item)?;

        cursor += 4;
    }
    write_generic(writer, cursor, 0u32)
}

/// Decodes one `Key:Value` payload of a feature phone app descriptor
/// (KTF `__adf__`, LGT `app_info`).
///
/// Descriptors are written by the handset, so line endings are inconsistent:
/// archives dumped from LGT handsets frequently use CRLF. A trailing `\r` here
/// ends up in the AID and turns the jar lookup into `0002A4B1\r.jar`, so
/// surrounding whitespace is always trimmed.
pub fn descriptor_value(value: &[u8]) -> String {
    let text: String = String::from_utf8_lossy(value).into();

    text.trim().into()
}

pub trait AsAny {
    fn as_any(&self) -> &dyn Any;

    fn as_any_mut(&mut self) -> &mut dyn Any;
}

impl<T> AsAny for T
where
    T: Any,
{
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;

    struct StrictMemory {
        memory: Vec<u8>,
    }

    impl ByteRead for StrictMemory {
        fn read_bytes(&self, address: u32, result: &mut [u8]) -> Result<usize> {
            let address = address as usize;
            let end = address + result.len();
            if end > self.memory.len() {
                return Err(WieError::InvalidMemoryAccess(address as u32));
            }

            result.copy_from_slice(&self.memory[address..end]);

            Ok(result.len())
        }
    }

    #[test]
    fn read_generic_reads_into_initialized_storage() {
        let memory = StrictMemory {
            memory: vec![0, 0x78, 0x56, 0x34, 0x12],
        };

        let value: u32 = read_generic(&memory, 1).unwrap();

        assert_eq!(value, 0x1234_5678);
    }

    #[test]
    fn read_null_terminated_string_handles_four_byte_boundaries() {
        let memory = StrictMemory {
            memory: vec![0, b't', b'e', b's', b't', 0],
        };

        let value = read_null_terminated_string_bytes(&memory, 1).unwrap();

        assert_eq!(value, b"test");
    }

    /// A reader that serves whatever fits and says how much it served, which is
    /// what a paged memory does at the end of what it has.
    struct PagedMemory {
        memory: Vec<u8>,
    }

    impl ByteRead for PagedMemory {
        fn read_bytes(&self, address: u32, result: &mut [u8]) -> Result<usize> {
            let address = address as usize;
            if address >= self.memory.len() {
                return Err(WieError::InvalidMemoryAccess(address as u32));
            }

            let read = result.len().min(self.memory.len() - address);
            result[..read].copy_from_slice(&self.memory[address..address + read]);

            Ok(read)
        }
    }

    /// The span a read asks for is a saving, not a promise: a reader that hands
    /// back less than it was asked for still has to spell the string, and one
    /// that refuses the span outright still has to spell it a byte at a time.
    #[test]
    fn a_string_reads_the_same_however_much_the_reader_serves_at_once() {
        let mut memory = vec![0u8; 0x1000 - 8];
        memory.extend_from_slice(b"a name that runs past the end of its page\0");

        let start = 0x1000 - 8;
        let paged = PagedMemory { memory: memory.clone() };
        let strict = StrictMemory { memory };

        assert_eq!(
            read_null_terminated_string_bytes(&paged, start).unwrap(),
            b"a name that runs past the end of its page"
        );
        assert_eq!(
            read_null_terminated_string_bytes(&strict, start).unwrap(),
            b"a name that runs past the end of its page"
        );
    }

    /// An unreadable address is still an error, however the read was made.
    #[test]
    fn a_string_with_no_memory_under_it_is_refused() {
        let memory = PagedMemory { memory: vec![b'x'; 4] };

        assert!(read_null_terminated_string_bytes(&memory, 0).is_err());
        assert!(read_null_terminated_string_bytes(&memory, 8).is_err());
    }
}
