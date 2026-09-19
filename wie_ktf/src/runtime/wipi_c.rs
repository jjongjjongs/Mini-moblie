use alloc::{boxed::Box, sync::Arc, vec};

use jvm::Jvm;
use wie_backend::System;
use wie_core_arm::{ArmCore, EmulatedFunction, EmulatedFunctionParam, ResultWriter, SvcId};
use wie_util::{Result, WieError};
use wie_wipi_c::{
    WIPICMethodBody, WIPICResult,
    api::{filesystem, im, kernel, net, serial, shared_buf},
};

use crate::runtime::SVC_CATEGORY_WIPIC;
use crate::runtime::svc_ids::{WIPICGraphicsMethodId, WIPICKernelMethodId, WIPICTableId};

mod context;
pub mod interface;
mod method_table;

use context::KtfWIPICContext;

struct WIPICMethodResult {
    result: WIPICResult,
}

impl ResultWriter<WIPICMethodResult> for WIPICMethodResult {
    fn write(self, core: &mut ArmCore, next_pc: u32) -> Result<()> {
        core.write_return_value(&self.result.results)?;
        core.set_next_pc(next_pc)?;

        Ok(())
    }
}

struct CMethodProxy {
    context: KtfWIPICContext,
    body: WIPICMethodBody,
}

#[async_trait::async_trait]
impl EmulatedFunction<(), WIPICMethodResult, ()> for CMethodProxy {
    async fn call(&self, core: &mut ArmCore, _: &mut ()) -> Result<WIPICMethodResult> {
        let a0 = u32::get(core, 0);
        let a1 = u32::get(core, 1);
        let a2 = u32::get(core, 2);
        let a3 = u32::get(core, 3);
        let a4 = u32::get(core, 4);
        let a5 = u32::get(core, 5);
        let a6 = u32::get(core, 6);
        let a7 = u32::get(core, 7);
        let a8 = u32::get(core, 8);

        let result = self
            .body
            .call(&mut self.context.clone(), vec![a0, a1, a2, a3, a4, a5, a6, a7, a8].into_boxed_slice())
            .await?;

        Ok(WIPICMethodResult { result })
    }
}

async fn handle_wipic_svc(
    core: &mut ArmCore,
    (system, jvm, network_state, serial_state, filesystem_state, shared_buf_state, im_state, kernel_state): &mut (
        System,
        Jvm,
        net::SharedNetworkState,
        serial::SharedSerialState,
        filesystem::SharedFilesystemState,
        shared_buf::SharedSharedBufState,
        im::SharedImState,
        kernel::SharedKernelState,
    ),
    id: SvcId,
) -> Result<()> {
    let table_id = WIPICTableId::try_from(id.0 >> 16)?;
    let function_id = id.0 as u16;
    let (_, lr) = core.read_pc_lr()?;
    if table_id == WIPICTableId::Kernel && function_id == WIPICKernelMethodId::Reserved1 as u16 {
        return interface::get_wipic_interfaces(
            core,
            &mut KtfWIPICContext::new(
                core.clone(),
                system.clone(),
                jvm.clone(),
                network_state.clone(),
                serial_state.clone(),
                filesystem_state.clone(),
                shared_buf_state.clone(),
                im_state.clone(),
                kernel_state.clone(),
            ),
        )
        .await?
        .write(core, lr);
    }

    if method_table::get_served_method_body(table_id, function_id).is_none() {
        describe_unserved_call(core, table_id, function_id)?;
    }

    let body = method_table::get_method_body(table_id, function_id)
        .ok_or_else(|| WieError::FatalError(alloc::format!("Unknown KTF WIPIC SVC id {:#x}", id.0)))?;

    EmulatedFunction::call(
        &CMethodProxy {
            context: KtfWIPICContext::new(
                core.clone(),
                system.clone(),
                jvm.clone(),
                network_state.clone(),
                serial_state.clone(),
                filesystem_state.clone(),
                shared_buf_state.clone(),
                im_state.clone(),
                kernel_state.clone(),
            ),
            body,
        },
        core,
        &mut (),
    )
    .await?
    .write(core, lr)
}

pub fn register_wipic_svc_handler(core: &mut ArmCore, system: &System, jvm: &Jvm) -> Result<()> {
    core.register_svc_handler(
        SVC_CATEGORY_WIPIC,
        handle_wipic_svc,
        &(
            system.clone(),
            jvm.clone(),
            net::new_state(),
            serial::new_state(),
            filesystem::new_state(),
            shared_buf::new_state(),
            im::new_state(),
            kernel::new_state(),
        ),
    )?;

    core.set_fast_svc_handler(Arc::new(|core: &mut ArmCore, category: u32, _lr: u32| -> Result<bool> {
        match category {
            SVC_CATEGORY_WIPIC => try_fast_wipic_call(core),
            _ => Ok(false),
        }
    }));

    Ok(())
}

/// Synchronous answers for the two WIPI C calls that are arithmetic.
///
/// On the handset these are a few instructions inline in the caller. Here every
/// one of them is a supervisor call: the core leaves its run loop, the async
/// dispatch clones a context of nine shared states, allocates two futures for
/// the `async_trait` hop and unwinds back - for a shift and two ors.
///
/// That is most of what a title costs when it asks per pixel, and 록맨X does:
/// a device trace of one gameplay frame holds **32,000 calls to
/// `MC_grpGetPixelFromRGB`** against 128 `MC_grpDrawImage`, naming five
/// distinct colours over and over - the colour key it tests each pixel
/// against, resolved again for every pixel rather than once. Nothing about
/// that is wrong on a handset, where the call is free; it is the round trip
/// that is not free here, so the round trip is what goes.
///
/// Answered here rather than in [`graphics`] because the point is to not
/// reach the generic path at all; the bodies there stay as they are and still
/// answer every other caller. `Ok(true)` means the call is done, `Ok(false)`
/// that the generic handler should take it.
fn try_fast_wipic_call(core: &mut ArmCore) -> Result<bool> {
    const GET_PIXEL_FROM_RGB: u32 = ((WIPICTableId::Graphics as u32) << 16) | WIPICGraphicsMethodId::GetPixelFromRgb as u32;
    const GET_IMAGE_FRAMEBUFFER: u32 = ((WIPICTableId::Graphics as u32) << 16) | WIPICGraphicsMethodId::GetImageFramebuffer as u32;

    let result = match core.read_svc_id() {
        // `MC_grpGetPixelFromRGB`, the same RGB565 packing
        // `wie_backend::canvas::Rgb565Pixel::from_color` does, on the low byte
        // of each component as the generic body's `as u8` takes it.
        GET_PIXEL_FROM_RGB => {
            let r = core.read_param(0)? & 0xff;
            let g = core.read_param(1)? & 0xff;
            let b = core.read_param(2)? & 0xff;

            ((r >> 3) << 11) | ((g >> 2) << 5) | (b >> 3)
        }
        // `MC_grpGetImageFrameBuffer`: a WIPICImage begins with its own
        // framebuffer, so the image handle is already the answer.
        GET_IMAGE_FRAMEBUFFER => core.read_param(0)?,
        _ => return Ok(false),
    };

    // Back to the caller the way the generic path returns: the link register,
    // which carries the caller's Thumb bit, not the SVC exception's own even
    // return address.
    let (_, lr) = core.read_pc_lr()?;

    core.write_return_value(&[result])?;
    core.set_next_pc(lr)?;

    Ok(true)
}

/// Names a call to a slot no function stands behind.
///
/// A guest reaches a WIPI C function by indexing a table, so the slot number is
/// the only name the call has, and it is not one anybody can look up. Its
/// arguments are what identify it: a descriptor and a buffer and a length read
/// as a transfer, a descriptor alone as a close. Where those arguments do look
/// like a buffer, the bytes settle which transfer it is - a request a title has
/// just built reads as itself, where a buffer it means to have filled reads as
/// nothing.
///
/// Written here rather than in the table's own refusal because this is where the
/// registers and the memory they point at are.
fn describe_unserved_call(core: &mut ArmCore, table_id: WIPICTableId, function_id: u16) -> Result<()> {
    let arguments: [u32; 4] = core::array::from_fn(|index| u32::get(core, index));
    let [_, pointer, length, _] = arguments;
    // Where the title called from, which is the only way to find the call in
    // its own code and read what it does with the answer.
    let (_, lr) = core.read_pc_lr().unwrap_or((0, 0));

    tracing::warn!(
        "unserved WIPIC table {} function {function_id}({:#x}, {:#x}, {:#x}, {:#x}) from {lr:#x}{}",
        table_id as u32,
        arguments[0],
        arguments[1],
        arguments[2],
        arguments[3],
        buffer_preview(core, pointer, length)
    );

    Ok(())
}

/// What a `(pointer, length)` pair points at, bounded to what one log line can
/// carry. Empty unless the pair is plausible and the memory is really there.
fn buffer_preview(core: &ArmCore, address: u32, length: u32) -> alloc::string::String {
    use wie_util::ByteRead;

    if address == 0 || !(1..=0x1000).contains(&length) {
        return alloc::string::String::new();
    }

    let mut bytes = vec![0u8; (length as usize).min(64)];
    let Ok(read) = core.read_bytes(address, &mut bytes) else {
        return alloc::string::String::new();
    };
    bytes.truncate(read);

    let text: alloc::string::String = bytes
        .iter()
        .map(|&byte| match byte {
            b'\t' => '\u{2192}',
            0x20..=0x7e => char::from(byte),
            _ => '.',
        })
        .collect();

    alloc::format!(" [{read} of {length}: {text:?}]")
}
