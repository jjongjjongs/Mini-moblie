//! Writes out a KTF archive's image the way the emulator loads it.
//!
//! A `client.bin` in a title's jar is not what runs: the file's pointers are
//! relocated by the self-extractor at `IMAGE_BASE + 1`, so disassembling the
//! file gives code at the right addresses and class metadata full of garbage.
//! This runs that extractor and writes the result, which is the image a
//! disassembler and the KTF class/method structures can both be read from -
//! 미니러비's game loop was found this way, by walking the class registered at
//! the address its log named.
//!
//! Driven by the environment, like the archive probe next to it, so it is a
//! no-op in an ordinary `cargo test`:
//!
//! - `WIE_DUMP_ZIP` - the archive to load. Unset, this does nothing.
//! - `WIE_DUMP_OUT` - where to write the image. Its addresses start at
//!   `IMAGE_BASE`, which is `0x100000`.

use std::{
    future::Future,
    pin::pin,
    task::{Context, Poll, RawWaker, RawWakerVTable, Waker},
};

/// The loader runs to completion on the thread it is given, so a poll loop is
/// the whole executor it needs.
fn block_on<F: Future>(future: F) -> F::Output {
    static VTABLE: RawWakerVTable = RawWakerVTable::new(|_| RawWaker::new(core::ptr::null(), &VTABLE), |_| {}, |_| {}, |_| {});

    let waker = unsafe { Waker::from_raw(RawWaker::new(core::ptr::null(), &VTABLE)) };
    let mut context = Context::from_waker(&waker);
    let mut future = pin!(future);

    loop {
        if let Poll::Ready(output) = future.as_mut().poll(&mut context) {
            return output;
        }
    }
}

#[test]
fn ktf_image_dump() {
    let Ok(archive) = std::env::var("WIE_DUMP_ZIP") else {
        return;
    };
    let out = std::env::var("WIE_DUMP_OUT").expect("WIE_DUMP_OUT says where to write the image");

    let image = block_on(wie_ktf::dump_image(&std::fs::read(archive).unwrap())).unwrap();
    std::fs::write(&out, &image).unwrap();

    println!("[dump] wrote {} bytes to {out}", image.len());
}
