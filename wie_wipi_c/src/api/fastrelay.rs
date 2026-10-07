//! `FastRelay` - the KTF carrier library (`01039ADD`) a title connects to its
//! servers through.
//!
//! 템페스트 (KTF `010100D4`, 컴투스) does not open its own sockets for its
//! 정품인증. At start it asks `MC_knlGetExecNames("01039ADD")` for the
//! library's listing, `MC_knlLoad`s it, and takes its interface from
//! `MC_knlGetDLLInterface("FastRelay")`, keeping the table in its own data.
//! With no such library here the table stayed 0, and the authentication thread
//! called through it anyway - a read at address 12 that ended the run.
//!
//! The table, as the title's own calls lay it out (its code at `0x153d7c`,
//! `0x1119a4`, `0x111bf8`, `0x11218c`, `0x111848`):
//!
//! | slot | call | what the title does with it |
//! |------|------|------------------------------|
//! | 2 | `init(model, length)` | once at load, with the handset's model name |
//! | 3 | `connect(address, port, mode)` | address and port as a socket takes them; the answer is a socket descriptor, or below zero for a failure it reports |
//! | 4 | `send(socket, buffer, length)` | what was sent, or -19 to try again |
//! | 5 | `recv(socket, buffer, length)` | what arrived, or -19 when nothing has yet |
//! | 6 | `close(socket)` | |
//! | 7 | `connected()` | polled on a timer after `connect`; above zero moves the title on to its request |
//!
//! `send` and `recv` are a socket's write and read, so that is all a relay
//! connection is here: a socket, answered in process by whatever endpoint this
//! run keeps for the server named - see
//! [`crate::api::net::open_relay_connection`].

use wipi_types::wipic::WIPICWord;

use wie_util::Result;

use crate::{api::net, context::WIPICContext};

/// The name a title asks `MC_knlGetDLLInterface` for.
pub const INTERFACE_NAME: &str = "FastRelay";

/// The library's own program id, the one `MC_knlGetExecNames` and
/// `MC_knlLoad` are asked about.
pub const LIBRARY_ID: &str = "01039ADD";

/// `init(model, length)`. Nothing here depends on the model.
pub async fn init(_context: &mut dyn WIPICContext, model: WIPICWord, length: i32) -> Result<i32> {
    tracing::debug!("FastRelay init({model:#x}, {length})");

    Ok(0)
}

/// `connect(address, port, mode)`.
pub async fn connect(context: &mut dyn WIPICContext, address: WIPICWord, port: WIPICWord, mode: i32) -> Result<i32> {
    tracing::info!("FastRelay connect({address:#x}, {port:#x}, {mode})");

    let socket = net::open_relay_connection(context, address, port)?;
    context.kernel_state().lock().relay_socket = (socket >= 0).then_some(socket);

    Ok(socket)
}

/// `send(socket, buffer, length)`, which is a socket write.
pub async fn send(context: &mut dyn WIPICContext, socket: i32, buffer: WIPICWord, length: i32) -> Result<i32> {
    net::socket_write(context, socket, buffer, length).await
}

/// `recv(socket, buffer, length)`, which is a socket read: what arrived, or
/// -19 when nothing has yet.
pub async fn recv(context: &mut dyn WIPICContext, socket: i32, buffer: WIPICWord, length: i32) -> Result<i32> {
    net::socket_read(context, socket, buffer, length).await
}

/// `close(socket)`.
pub async fn close(context: &mut dyn WIPICContext, socket: i32) -> Result<i32> {
    tracing::debug!("FastRelay close({socket})");

    {
        let state = context.kernel_state();
        let mut state = state.lock();
        if state.relay_socket == Some(socket) {
            state.relay_socket = None;
        }
    }

    net::socket_close(context, socket).await
}

/// `connected()`: whether the relay's connection is up. A connection answered
/// in process is up the moment it is made.
pub async fn connected(context: &mut dyn WIPICContext) -> Result<i32> {
    Ok(context.kernel_state().lock().relay_socket.is_some() as i32)
}
