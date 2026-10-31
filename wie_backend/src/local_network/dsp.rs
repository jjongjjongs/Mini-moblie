//! The game server SKT's `DSP` titles keep their saves on.
//!
//! EA프로야구2010's SKT builds (`9000516913` 1.0.2, `0086345396` 1.0.3) look up
//! `ngph.gxg.com` - the name [`super::gxg`] answers - and dial port `7110`, to
//! ask whether the subscriber's game data (`아이템/시즌/게임설정`) is kept
//! there. The server has been gone for years, and the title gives up with
//! `서버와의 접속이 원활하지 않습니다`.
//!
//! FIFA10 (`0087278938`) carries an older build of the same library and dials
//! the same place. Left to the network it never connects - the name resolves
//! to an address nothing answers - and the title sits on `기존에 저장되어 있는
//! 데이터가 있는지 확인 중입니다` for good. Taken here, its library reports the
//! connect to the title, which says `서버 접속에 실패하였습니다` and goes on to
//! its menu.
//!
//! Every message either way is a 26-byte header, a body, and `0x17`:
//!
//! ```text
//! [0..12]   "DSP.01.00.01"
//! [12..15]  the type: a request ends in `R`, its answer in `C`
//! [15]      'C', either way
//! [16..26]  the body's length in ASCII digits, NUL-padded - or all NUL for
//!           an empty body, from 초코초코타이쿤2's build
//! ```
//!
//! 초코초코타이쿤2 (`9000516933`) carries a later build that dials
//! `ngph.gxg.com`'s old address, `211.234.231.177`, straight, and reads the
//! header more strictly: its reader (`0x133d6`) drops an answer whose `[15]` is
//! not `'C'`, and the title then offers to start over without checking. With
//! it `'C'`, the title hears there is nothing kept and says
//! `서버에 저장된 세이브데이터가 없습니다`.
//!
//! The addresses here are 1.0.2's; 1.0.3 is the same library a few bytes on.
//! The title builds that at `0x1b4078`. Its receive (`0x1b4404`) reads the 26
//! bytes, `atoi`s the length and reads that many more and the `0x17`, then
//! the dispatcher at `0x1b47ec` goes on the type. Every answer opens with a
//! status byte, zero for success; the readers take single bytes, strings as a
//! length byte and the bytes, and numbers as ten ASCII digits.
//!
//! - `IPR`, the login - the title's id (`0087067211`, 1.0.3 `0086066370`)
//!   and the subscriber number.
//!   `IPC` is the status alone.
//! - `SLR`, the saves kept for it. `SLC` after a zero status carries a byte, a
//!   count, and that many names; the title looks for `kickass`, its save
//!   store, and takes the status as 1 - nothing kept - when it is not there.
//! - `FPR`, sent once the title has heard there is nothing to fetch. `FPC` is
//!   the status alone.
//!
//! Nothing is kept here, so the subscriber has no saves on the server and the
//! title carries on with the ones on the handset.

use alloc::{boxed::Box, format, string::String, vec::Vec};

use super::{LocalConnection, LocalEndpoint, LocalRead};

/// Where [`super::gxg`] resolves `ngph.gxg.com`.
const ADDRESS: &str = "10.71.88.71";
const HOST: &str = "ngph.gxg.com";
/// The address `ngph.gxg.com` had, which 초코초코타이쿤2 (`9000516933`) dials
/// without looking the name up.
const FIXED_ADDRESS: &str = "211.234.231.177";
const PORT: u16 = 7110;

const VERSION: &[u8; 12] = b"DSP.01.00.01";
const HEADER_LEN: usize = 26;
const END: u8 = 0x17;

const OK: u8 = 0;

pub struct DspEndpoint;

impl LocalEndpoint for DspEndpoint {
    fn name(&self) -> &str {
        "dsp(ngph.gxg.com:7110)"
    }

    fn accepts(&self, scheme: &str, host: &str, port: u16) -> bool {
        scheme == "socket" && port == PORT && (host == ADDRESS || host == FIXED_ADDRESS || host.eq_ignore_ascii_case(HOST))
    }

    fn open(&self, _scheme: &str, _host: &str, _port: u16) -> Box<dyn LocalConnection> {
        Box::new(DspConnection::default())
    }
}

#[derive(Default)]
struct DspConnection {
    pending: Vec<u8>,
    outgoing: Vec<u8>,
    /// An answer is in that the title has not yet been told is coming.
    ///
    /// The server's answer arrived a round trip after the request, and the
    /// title is written for that: having sent `IPR`, the library reads once
    /// straight away and only then defines the timer (`MC_knlDefTimer`) its
    /// next request goes out on. Read there, 1.0.3's `IPC` sets that timer
    /// before it is defined, the timer fires with no callback, and nothing
    /// follows. So the first read after a write finds nothing, as it would
    /// have on the air, and the answer goes to the read callback after.
    held: bool,
}

/// One message, as the title reads them.
fn message(kind: &[u8; 3], body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(HEADER_LEN + body.len() + 1);
    out.extend_from_slice(VERSION);
    out.extend_from_slice(kind);
    out.push(b'C');

    let mut length = [0u8; 10];
    let digits = format!("{}", body.len());
    length[..digits.len()].copy_from_slice(digits.as_bytes());
    out.extend_from_slice(&length);

    out.extend_from_slice(body);
    out.push(END);
    out
}

/// Splits one whole message off the front of `pending`: its type and its body.
fn take_message(pending: &mut Vec<u8>) -> Option<([u8; 3], Vec<u8>)> {
    if pending.len() < HEADER_LEN {
        return None;
    }
    if !pending.starts_with(VERSION) {
        tracing::warn!("dsp: no message in {:02x?}", pending);
        pending.clear();
        return None;
    }

    // An empty body's length is `0` from EA's build and no digits at all from
    // 초코초코타이쿤2's, which leaves the field NUL.
    let digits: Vec<u8> = pending[16..26].iter().copied().take_while(u8::is_ascii_digit).collect();
    let length: usize = if digits.is_empty() {
        0
    } else {
        core::str::from_utf8(&digits).ok()?.parse().ok()?
    };
    if pending.len() < HEADER_LEN + length + 1 {
        return None;
    }

    let kind = [pending[12], pending[13], pending[14]];
    let body = pending.drain(..HEADER_LEN + length + 1).skip(HEADER_LEN).take(length).collect();
    Some((kind, body))
}

/// The answer to one request, or `None` for one nothing here knows.
fn answer(kind: &[u8; 3], _body: &[u8]) -> Option<([u8; 3], Vec<u8>)> {
    Some(match kind {
        b"IPR" => (*b"IPC", alloc::vec![OK]),
        // No names: `kickass` is not among them, and the title hears that
        // nothing is kept for the subscriber.
        b"SLR" => (*b"SLC", alloc::vec![OK, 0, 0]),
        b"FPR" => (*b"FPC", alloc::vec![OK]),
        _ => return None,
    })
}

impl LocalConnection for DspConnection {
    fn write(&mut self, bytes: &[u8]) {
        self.pending.extend_from_slice(bytes);

        while let Some((kind, body)) = take_message(&mut self.pending) {
            tracing::info!("dsp <- {} {:02x?}", String::from_utf8_lossy(&kind), body);
            match answer(&kind, &body) {
                Some((kind, body)) => {
                    self.outgoing.extend_from_slice(&message(&kind, &body));
                    self.held = true;
                }
                None => tracing::warn!("dsp: no answer for {}", String::from_utf8_lossy(&kind)),
            }
        }
    }

    fn read(&mut self, out: &mut [u8]) -> LocalRead {
        if self.outgoing.is_empty() || core::mem::take(&mut self.held) {
            return LocalRead::Pending;
        }

        let take = out.len().min(self.outgoing.len());
        out[..take].copy_from_slice(&self.outgoing[..take]);
        self.outgoing.drain(..take);
        LocalRead::Data(take)
    }

    fn readable(&self) -> bool {
        !self.outgoing.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use alloc::{vec, vec::Vec};

    use super::{DspConnection, DspEndpoint, take_message};
    use crate::local_network::{LocalConnection, LocalEndpoint, LocalRead};

    /// 1.0.2's login as it went out: `IPR`, 'C', a length of 21, the title's id
    /// and the subscriber number, and the closing `0x17`.
    const LOGIN: &[u8] = b"DSP.01.00.01IPRC21\0\0\0\0\0\0\0\0008706721101046119269\x17";

    fn read_answer(connection: &mut DspConnection) -> ([u8; 3], Vec<u8>) {
        assert_eq!(
            connection.read(&mut [0; 64]),
            LocalRead::Pending,
            "nothing on the first read after a write"
        );

        let mut out = vec![0u8; 256];
        let LocalRead::Data(read) = connection.read(&mut out) else {
            panic!("no answer");
        };
        let mut pending = out[..read].to_vec();
        let message = take_message(&mut pending).expect("a whole message");
        assert!(pending.is_empty());
        message
    }

    #[test]
    fn only_the_save_server_is_answered() {
        assert!(DspEndpoint.accepts("socket", "10.71.88.71", 7110));
        assert!(DspEndpoint.accepts("socket", "ngph.gxg.com", 7110));
        assert!(DspEndpoint.accepts("socket", "211.234.231.177", 7110));
        assert!(!DspEndpoint.accepts("socket", "10.71.88.71", 7300));
    }

    #[test]
    fn the_title_logs_in_and_hears_nothing_is_kept() {
        let mut connection = DspConnection::default();

        connection.write(LOGIN);
        assert_eq!(read_answer(&mut connection), (*b"IPC", vec![0]));

        connection.write(b"DSP.01.00.01SLRC0\0\0\0\0\0\0\0\0\0\x17");
        assert_eq!(read_answer(&mut connection), (*b"SLC", vec![0, 0, 0]));

        connection.write(b"DSP.01.00.01FPRC0\0\0\0\0\0\0\0\0\0\x17");
        assert_eq!(read_answer(&mut connection), (*b"FPC", vec![0]));
    }

    #[test]
    fn an_empty_body_may_have_no_length_digits() {
        let mut connection = DspConnection::default();

        // 초코초코타이쿤2's, its length field left NUL.
        connection.write(b"DSP.01.00.01SLRC\0\0\0\0\0\0\0\0\0\0\x17");
        assert_eq!(read_answer(&mut connection), (*b"SLC", vec![0, 0, 0]));
    }

    #[test]
    fn answers_are_marked_c_like_requests() {
        assert_eq!(super::message(b"IPC", &[0])[15], b'C');
    }
}
