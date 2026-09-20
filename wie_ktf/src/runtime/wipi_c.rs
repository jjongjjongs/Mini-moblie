use alloc::{boxed::Box, sync::Arc, vec};

use jvm::Jvm;
use wie_backend::System;
use wie_core_arm::{ArmCore, EmulatedFunction, EmulatedFunctionParam, ResultWriter, SvcId};
use wie_util::{Result, WieError, write_generic};
use wie_wipi_c::api::graphics::WIPICGraphicsContextIdx;
use wie_wipi_c::{
    WIPICMethodBody, WIPICResult,
    api::{filesystem, graphics, im, kernel, net, serial, shared_buf},
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

    // A census of every WIPI-C call, named whether or not the body that serves
    // it writes anything down. A handler that answers in silence is invisible
    // at any log level, and so is the fast path, which never reaches a body at
    // all - so a screen that waits on one of those looks, in a capture, like a
    // screen that asks for nothing.
    #[cfg(feature = "wipic-probe")]
    tracing::warn!("svc {table_id:?}-{function_id} from lr={lr:#x}");
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
    const GET_RGB_FROM_PIXEL: u32 = ((WIPICTableId::Graphics as u32) << 16) | WIPICGraphicsMethodId::GetRgbFromPixel as u32;
    const GET_IMAGE_FRAMEBUFFER: u32 = ((WIPICTableId::Graphics as u32) << 16) | WIPICGraphicsMethodId::GetImageFramebuffer as u32;
    const INIT_CONTEXT: u32 = ((WIPICTableId::Graphics as u32) << 16) | WIPICGraphicsMethodId::InitContext as u32;
    const SET_CONTEXT: u32 = ((WIPICTableId::Graphics as u32) << 16) | WIPICGraphicsMethodId::SetContext as u32;
    const GET_CONTEXT: u32 = ((WIPICTableId::Graphics as u32) << 16) | WIPICGraphicsMethodId::GetContext as u32;

    #[cfg(feature = "wipic-probe")]
    {
        let id = core.read_svc_id();
        if id == GET_PIXEL_FROM_RGB || id == GET_RGB_FROM_PIXEL || id == GET_IMAGE_FRAMEBUFFER {
            tracing::warn!("svc fast-path {:#x}", id);
        }
    }

    // The graphics context: read a 0x38-byte struct out of the title's own
    // memory, change one word, write it back. Nothing else is touched, which is
    // what lets it be answered here - and the rules are not copied here either,
    // these are the bodies the generic handler runs. A title's own blitter
    // drives these harder than anything else it calls: 드래곤하트2's inventory
    // screen asks for 27,000 SetContext and 26,000 InitContext a second,
    // together 78% of all its WIPI-C traffic, against 8,700 PutPixel.
    if matches!(core.read_svc_id(), INIT_CONTEXT | SET_CONTEXT | GET_CONTEXT) {
        let id = core.read_svc_id();
        let p_grp_ctx = core.read_param(0)?;
        match id {
            INIT_CONTEXT => graphics::init_context_in(core, p_grp_ctx)?,
            SET_CONTEXT => {
                let op = WIPICGraphicsContextIdx::from_raw(core.read_param(1)?);
                let pv = core.read_param(2)?;
                graphics::set_context_in(core, p_grp_ctx, op, pv)?;
            }
            _ => {
                let op = WIPICGraphicsContextIdx::from_raw(core.read_param(1)?);
                let out_ptr = core.read_param(2)?;
                graphics::get_context_in(core, p_grp_ctx, op, out_ptr)?;
            }
        }

        // These three report nothing, so there is no return value to write -
        // the generic path returns unit for them too.
        let (_, lr) = core.read_pc_lr()?;
        core.set_next_pc(lr)?;

        return Ok(true);
    }

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
        // `MC_grpGetRGBFromPixel`, the unpacking side of the same pair, spread
        // back over the full 0..255 range the way
        // `wie_backend::canvas::Rgb565Pixel::to_color` rounds it. It writes its
        // three components through the caller's pointers and answers with the
        // pixel it was given.
        //
        // It comes in pairs with the packing call above and is the more
        // expensive half to leave behind: LOA-혼돈의 서곡 fades its story screen
        // by unpacking each pixel, lerping the three components towards a
        // target and packing them again (its helper is at `0x122104`), so a
        // 240x320 pass is 76,800 of each. With only the packing side answered
        // here the fade ran at about 14,000 calls a second and a single pass
        // took better than five seconds, which is what "the screen does not
        // move" looked like.
        GET_RGB_FROM_PIXEL => {
            let pixel = core.read_param(0)?;
            let ptr_r = core.read_param(1)?;
            let ptr_g = core.read_param(2)?;
            let ptr_b = core.read_param(3)?;

            let (r, g, b) = rgb565_components(pixel);

            write_generic(core, ptr_r, r)?;
            write_generic(core, ptr_g, g)?;
            write_generic(core, ptr_b, b)?;

            pixel
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

/// The three components `MC_grpGetRGBFromPixel` writes for an RGB565 pixel.
///
/// The generic body reads the pixel as a `u16`, so whatever a caller left in
/// the top half is not part of the colour, and it spreads each field back over
/// 0..255 with rounding rather than by shifting - `wie_backend`'s
/// `Rgb565Pixel::to_color` is the definition and the test below holds this to
/// it for every pixel there is.
fn rgb565_components(pixel: u32) -> (i32, i32, i32) {
    let raw = pixel as u16 as u32;

    let r = ((raw >> 11) & 0x1f) * 255;
    let g = ((raw >> 5) & 0x3f) * 255;
    let b = (raw & 0x1f) * 255;

    (((r + 15) / 31) as i32, ((g + 31) / 63) as i32, ((b + 15) / 31) as i32)
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

#[cfg(test)]
mod tests {
    use wie_backend::canvas::{PixelType, Rgb565Pixel};

    use super::rgb565_components;

    /// The fast path answers what the body in `wie_wipi_c` would have, for
    /// every pixel a caller can hand it.
    ///
    /// The two are written out separately - one on `Color`, one on the raw
    /// word - so nothing but a check like this keeps them in step, and a fade
    /// done per pixel would show the drift a shade at a time.
    #[test]
    fn the_fast_path_unpacks_a_pixel_the_way_the_generic_body_does() {
        for raw in 0..=u16::MAX {
            let color = Rgb565Pixel::to_color(raw);

            assert_eq!(
                rgb565_components(raw as u32),
                (color.r as i32, color.g as i32, color.b as i32),
                "pixel {raw:#06x}"
            );
        }
    }

    /// And it ignores anything above the low half-word, which is where a
    /// caller's own sign extension lands.
    #[test]
    fn the_fast_path_reads_only_the_pixel() {
        for raw in [0x0000u16, 0x1234, 0xf81f, 0xffff] {
            assert_eq!(rgb565_components(raw as u32), rgb565_components(0xdead_0000 | raw as u32));
        }
    }
}
