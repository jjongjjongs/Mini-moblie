//! The platform as an SKT native title reaches it: an array of tables of
//! function pointers, which the title finds through the word the loader
//! writes at [`INTERFACE_POINTER`].
//!
//! The title indexes both levels directly - `G->table[3]->fn[0]` is
//! `MC_fsOpen` - so the layout is the whole contract. The tables are the
//! WIPI-C libraries in the order the handset kept them, and each one's
//! functions are in the order the other carriers keep them too: the kernel and
//! graphics tables line up with KTF's word for word, and the file system, net,
//! util, serial and media ones with the order LGT's import ids are numbered in.
//! Where the two carriers' orders were not enough, the slot was read off what
//! 메이플스토리 해적편 passes it and does with the answer; each table below
//! says which.

use alloc::{boxed::Box, string::String, sync::Arc, vec, vec::Vec};

use jvm::Jvm;
use wipi_types::wipic::WIPICWord;

use wie_backend::System;
use wie_core_arm::{Allocator, ArmCore, EmulatedFunction, EmulatedFunctionParam, ResultWriter, SvcId};
use wie_util::{Result, read_null_terminated_string_bytes, write_generic, write_null_terminated_string_bytes};
use wie_wipi_c::{
    MethodImpl, WIPICContext, WIPICMethodBody, WIPICResult,
    api::{filesystem, graphics, im, kernel, media, misc, net, serial, shared_buf, util},
};

use super::{context::SktWIPICContext, daf::DATA_BASE, daf::Resources, libc};

pub const SVC_CATEGORY_WIPIC: u32 = 1;

/// Where the title reads the interface from: the word at `0x1000` into its
/// data, zero in the file, which every call it makes into the platform
/// dereferences.
pub const INTERFACE_POINTER: u32 = DATA_BASE + 0x1000;

const TABLE_COUNT: u32 = 32;

/// Every table is this long whatever is written for it, so a slot past the
/// last one served still answers - see `WIPIC_TABLE_FUNCTIONS` on KTF.
const TABLE_SLOTS: u32 = 64;

mod table {
    pub const KERNEL: u32 = 0;
    pub const GRAPHICS: u32 = 1;
    pub const FILE_SYSTEM: u32 = 3;
    pub const MISC: u32 = 4;
    pub const NET: u32 = 5;
    pub const UTIL: u32 = 6;
    pub const MEDIA: u32 = 7;
    pub const SERIAL: u32 = 9;
    pub const C_LIBRARY: u32 = 14;
}

/// Writes the tables, each slot an SVC stub naming its table and slot, and
/// the interface pointer the title reads them through.
pub fn install(core: &mut ArmCore) -> Result<()> {
    let interface = Allocator::alloc(core, TABLE_COUNT * 4)?;

    for table in 0..TABLE_COUNT {
        let address = Allocator::alloc(core, TABLE_SLOTS * 4)?;
        for slot in 0..TABLE_SLOTS {
            let stub = core.make_svc_stub(SVC_CATEGORY_WIPIC, (table << 8) | slot)?;
            write_generic(core, address + slot * 4, stub)?;
        }
        write_generic(core, interface + table * 4, address)?;
    }

    write_generic(core, INTERFACE_POINTER, interface)
}

fn body(table: u32, slot: u32) -> Option<WIPICMethodBody> {
    Some(match (table, slot) {
        (table::KERNEL, _) => kernel_body(slot)?,
        (table::GRAPHICS, _) => graphics_body(slot)?,
        (table::FILE_SYSTEM, _) => file_system_body(slot)?,
        (table::MISC, _) => misc_body(slot)?,
        (table::NET, _) => net_body(slot)?,
        (table::UTIL, _) => util_body(slot)?,
        (table::MEDIA, _) => media_body(slot)?,
        (table::SERIAL, _) => serial_body(slot)?,
        (table::C_LIBRARY, _) => libc::body(slot)?,
        _ => return None,
    })
}

/// KTF's order. `sprintk` at 1 formats every string 해적편 shows, `alloc`,
/// `calloc` and `free` are 20 to 22, and `getFreeMemory` at 24 is what
/// `startClet` sizes its heap from.
fn kernel_body(slot: u32) -> Option<WIPICMethodBody> {
    Some(match slot {
        0 => kernel::printk.into_body(),
        1 => kernel::sprintk.into_body(),
        2 => kernel::get_exec_names.into_body(),
        3 => kernel::execute.into_body(),
        4 => kernel::mexecute.into_body(),
        5 => kernel::load.into_body(),
        6 => kernel::mload.into_body(),
        7 => kernel::exit.into_body(),
        8 => kernel::program_stop.into_body(),
        9 => kernel::get_cur_program_id.into_body(),
        10 => kernel::get_parent_program_id.into_body(),
        11 => kernel::get_app_manager_id.into_body(),
        12 => kernel::get_program_info.into_body(),
        13 => kernel::get_access_level.into_body(),
        14 => kernel::get_program_name.into_body(),
        15 => shared_buf::create_shared_buf.into_body(),
        16 => shared_buf::destroy_shared_buf.into_body(),
        17 => shared_buf::get_shared_buf.into_body(),
        18 => shared_buf::get_shared_buf_size.into_body(),
        19 => shared_buf::resize_shared_buf.into_body(),
        20 => kernel::alloc.into_body(),
        21 => kernel::calloc.into_body(),
        22 => kernel::free.into_body(),
        23 => kernel::get_total_memory.into_body(),
        24 => kernel::get_free_memory.into_body(),
        25 => kernel::def_timer.into_body(),
        26 => kernel::set_timer.into_body(),
        27 => kernel::unset_timer.into_body(),
        28 => kernel::current_time.into_body(),
        29 => kernel::get_system_property.into_body(),
        30 => kernel::set_system_property.into_body(),
        31 => kernel::get_resource_id.into_body(),
        32 => kernel::get_resource.into_body(),
        _ => return None,
    })
}

/// KTF's order, which is the one the WIPI-C header lists.
fn graphics_body(slot: u32) -> Option<WIPICMethodBody> {
    Some(match slot {
        0 => graphics::get_image_property.into_body(),
        1 => graphics::get_image_framebuffer.into_body(),
        2 => graphics::get_screen_framebuffer.into_body(),
        3 => graphics::destroy_offscreen_framebuffer.into_body(),
        4 => graphics::create_offscreen_framebuffer.into_body(),
        5 => graphics::init_context.into_body(),
        6 => graphics::set_context.into_body(),
        7 => graphics::get_context.into_body(),
        8 => graphics::put_pixel.into_body(),
        9 => graphics::draw_line.into_body(),
        10 => graphics::draw_rect.into_body(),
        11 => graphics::fill_rect.into_body(),
        12 => graphics::copy_frame_buffer.into_body(),
        13 => graphics::draw_image.into_body(),
        14 => graphics::copy_area.into_body(),
        15 => graphics::draw_arc.into_body(),
        16 => graphics::fill_arc.into_body(),
        17 => graphics::draw_string.into_body(),
        18 => graphics::draw_unicode_string.into_body(),
        19 => graphics::get_rgb_pixels.into_body(),
        20 => graphics::set_rgb_pixels.into_body(),
        21 => graphics::flush_lcd.into_body(),
        22 => graphics::get_pixel_from_rgb.into_body(),
        23 => graphics::get_rgb_from_pixel.into_body(),
        24 => graphics::get_display_info.into_body(),
        25 => graphics::repaint.into_body(),
        26 => graphics::get_font.into_body(),
        27 => graphics::get_font_height.into_body(),
        28 => graphics::get_font_ascent.into_body(),
        29 => graphics::get_font_descent.into_body(),
        30 => graphics::get_string_width.into_body(),
        31 => graphics::get_unicode_string_width.into_body(),
        32 => graphics::create_image.into_body(),
        33 => graphics::destroy_image.into_body(),
        34 => graphics::decode_next_image.into_body(),
        35 => graphics::encode_image.into_body(),
        36 => graphics::post_event.into_body(),
        37 => im::handle_input.into_body(),
        38 => im::set_current_mode.into_body(),
        39 => im::get_current_mode.into_body(),
        40 => im::get_support_mode_count.into_body(),
        41 => im::get_supported_modes.into_body(),
        42 => graphics::fill_polygon.into_body(),
        43 => graphics::draw_polygon.into_body(),
        44 => show_annunciator.into_body(),
        45 => get_annunciator_info.into_body(),
        52 => get_screen_framebuffer_into.into_body(),
        53 => get_font_ex.into_body(),
        _ => return None,
    })
}

/// Shows or hides the handset's status strip. There is none here to show:
/// the title has the whole panel either way.
async fn show_annunciator(_context: &mut dyn WIPICContext, id: WIPICWord, show: WIPICWord) -> Result<i32> {
    tracing::debug!("SKT grpShowAnnunciator({id}, {show})");

    Ok(0)
}

/// Writes one fact about the status strip, as a decimal string, into the
/// caller's buffer. 해적편 and 도적편 ask for `"HEIGHT"` and `atoi` the
/// answer into how far down their screen starts; 0, because no strip is
/// drawn. A title that cannot read it - the answer is not 0 - exits on the
/// spot, which is what the first run of 0083070426 did.
async fn get_annunciator_info(context: &mut dyn WIPICContext, id: WIPICWord, key: WIPICWord, out: WIPICWord, out_size: WIPICWord) -> Result<i32> {
    let key = read_null_terminated_string_bytes(context, key)?;
    let value: &[u8] = match key.as_slice() {
        b"HEIGHT" => b"0",
        b"WIDTH" => b"240",
        _ => {
            tracing::warn!("SKT grpGetAnnunciatorInfo({id}, {:?}): unknown key", String::from_utf8_lossy(&key));
            return Ok(-1);
        }
    };

    if out_size as usize <= value.len() {
        return Ok(-1);
    }
    write_null_terminated_string_bytes(context, out, value)?;

    Ok(0)
}

/// Writes the screen's frame buffer handle where the caller says. 크레이지
/// 아케이드 BnB 2011 reaches its screen only through this - it never asks
/// slot 2 - and reads the handle back straight away, so leaving the word
/// alone is a null dereference; 해적편 hands it the handle slot 2 already
/// gave it, and gets the same one back.
async fn get_screen_framebuffer_into(context: &mut dyn WIPICContext, id: WIPICWord, out: WIPICWord) -> Result<i32> {
    let framebuffer = graphics::get_screen_framebuffer(context, id).await?;
    write_generic(context, out, framebuffer.0)?;

    Ok(0)
}

/// A font by face name, size and style. 해적편 asks for `"굴림"` and falls
/// back to a null name if that is refused; the handset faces here have no
/// names, so either is the default face at the size asked for.
async fn get_font_ex(context: &mut dyn WIPICContext, name: WIPICWord, size: i32, style: i32) -> Result<i32> {
    let _ = name;

    graphics::get_font(context, 0, size, style).await
}

/// LGT's order to `available` at 12, which 해적편 calls with nothing to
/// size its saves. The two after are the other way round from LGT's ids: 15
/// is handed `(name, 1)` and answers whether the file is there, 16 a file
/// descriptor and answers where in it the title is.
fn file_system_body(slot: u32) -> Option<WIPICMethodBody> {
    Some(match slot {
        0 => filesystem::open.into_body(),
        1 => filesystem::read.into_body(),
        2 => filesystem::write.into_body(),
        3 => filesystem::close.into_body(),
        4 => filesystem::seek.into_body(),
        5 => filesystem::file_attribute.into_body(),
        6 => filesystem::remove.into_body(),
        7 => filesystem::rename.into_body(),
        8 => filesystem::mkdir.into_body(),
        9 => filesystem::rmdir.into_body(),
        10 => filesystem::list.into_body(),
        11 => fs_total_space.into_body(),
        12 => fs_available.into_body(),
        13 => filesystem::set_mode.into_body(),
        14 => filesystem::get_counts.into_body(),
        15 => filesystem::is_exist.into_body(),
        16 => filesystem::tell.into_body(),
        17 => filesystem::get_mounted_names.into_body(),
        _ => return None,
    })
}

/// What the store answers for its size and its room: well under what a
/// title's `int` sums overflow on, as with the kernel's memory figures.
const FS_TOTAL_SPACE: i32 = 8 * 1024 * 1024;
const FS_AVAILABLE_SPACE: i32 = 4 * 1024 * 1024;

async fn fs_total_space(_context: &mut dyn WIPICContext) -> Result<i32> {
    Ok(FS_TOTAL_SPACE)
}

async fn fs_available(_context: &mut dyn WIPICContext) -> Result<i32> {
    Ok(FS_AVAILABLE_SPACE)
}

/// `MC_miscBackLight` first, handed the light, on or off, a colour and how
/// long, which is what LGT's misc table opens with too.
fn misc_body(slot: u32) -> Option<WIPICMethodBody> {
    Some(match slot {
        0 => misc::back_light.into_body(),
        1 => misc::set_led.into_body(),
        2 => misc::get_led.into_body(),
        3 => misc::get_led_count.into_body(),
        _ => return None,
    })
}

/// The order both carriers keep: `connect` handed a callback, `socket(2, 1)`,
/// `socketConnect`, `socketRead` into a stack buffer, and the read and write
/// callbacks at 13 and 14.
fn net_body(slot: u32) -> Option<WIPICMethodBody> {
    Some(match slot {
        0 => net::connect.into_body(),
        1 => net::close.into_body(),
        2 => net::socket.into_body(),
        3 => net::socket_connect.into_body(),
        4 => net::socket_write.into_body(),
        5 => net::socket_read.into_body(),
        6 => net::socket_close.into_body(),
        7 => net::socket_bind.into_body(),
        8 => net::get_max_packet_length.into_body(),
        9 => net::socket_send_to.into_body(),
        10 => net::socket_recv_from.into_body(),
        11 => net::get_host_addr.into_body(),
        12 => net::socket_accept.into_body(),
        13 => net::set_read_callback.into_body(),
        14 => net::set_write_callback.into_body(),
        15 => net::http_open.into_body(),
        16 => net::http_connect.into_body(),
        17 => net::http_set_request_method.into_body(),
        18 => net::http_get_request_method.into_body(),
        19 => net::http_set_request_property.into_body(),
        20 => net::http_get_request_property.into_body(),
        21 => net::http_set_proxy.into_body(),
        22 => net::http_get_proxy.into_body(),
        23 => net::http_get_response_code.into_body(),
        24 => net::http_get_response_message.into_body(),
        25 => net::http_get_header_field.into_body(),
        26 => net::http_get_length.into_body(),
        27 => net::http_get_type.into_body(),
        28 => net::http_get_encoding.into_body(),
        29 => net::http_close.into_body(),
        _ => return None,
    })
}

fn util_body(slot: u32) -> Option<WIPICMethodBody> {
    Some(match slot {
        0 => util::htonl.into_body(),
        1 => util::htons.into_body(),
        2 => util::ntohl.into_body(),
        3 => util::ntohs.into_body(),
        4 => util::inet_addr_int.into_body(),
        5 => util::inet_addr_str.into_body(),
        _ => return None,
    })
}

/// The WIPI-C order LGT's ids follow, with one more slot before
/// `clipPutData` and three more after it. 해적편 makes a clip, fills it
/// through 4, sets its volume through 13 and plays it through 14; it stops a
/// sound with 17, 10 and 1 in that order - stop, clear the data, free the
/// clip - which is LGT's 13, 6 and 1. Pause and resume are 15 and 16, and the
/// vibrator at 21 is handed `(0, 0)` to stop it.
fn media_body(slot: u32) -> Option<WIPICMethodBody> {
    Some(match slot {
        0 => media::clip_create.into_body(),
        1 => media::clip_free.into_body(),
        4 => media::clip_put_data.into_body(),
        10 => media::clip_clear_data.into_body(),
        12 => media::clip_get_volume.into_body(),
        13 => media::clip_set_volume.into_body(),
        14 => media::play.into_body(),
        15 => media::pause.into_body(),
        16 => media::resume.into_body(),
        17 => media::stop.into_body(),
        20 => media::set_volume.into_body(),
        21 => media::vibrator.into_body(),
        _ => return None,
    })
}

/// LGT's order: 해적편 opens its serial port with the configuration string
/// LGT's `MC_serialOpen` takes, and closes it through 5.
fn serial_body(slot: u32) -> Option<WIPICMethodBody> {
    Some(match slot {
        0 => serial::open.into_body(),
        1 => serial::write.into_body(),
        4 => serial::set_write_callback.into_body(),
        5 => serial::close.into_body(),
        _ => return None,
    })
}

#[derive(Clone)]
pub struct SvcState {
    pub system: System,
    pub jvm: Jvm,
    pub resources: Arc<Resources>,
    pub network_state: net::SharedNetworkState,
    pub serial_state: serial::SharedSerialState,
    pub filesystem_state: filesystem::SharedFilesystemState,
    pub shared_buf_state: shared_buf::SharedSharedBufState,
    pub im_state: im::SharedImState,
    pub media_state: media::SharedMediaState,
    pub kernel_state: kernel::SharedKernelState,
}

impl SvcState {
    pub fn new(system: System, jvm: Jvm, resources: Arc<Resources>) -> Self {
        Self {
            system,
            jvm,
            resources,
            network_state: net::new_state(),
            serial_state: serial::new_state(),
            filesystem_state: filesystem::new_state(),
            shared_buf_state: shared_buf::new_state(),
            im_state: im::new_state(),
            media_state: media::new_state(),
            kernel_state: kernel::new_state(),
        }
    }

    pub fn context(&self, core: &ArmCore) -> SktWIPICContext {
        SktWIPICContext {
            core: core.clone(),
            system: self.system.clone(),
            jvm: self.jvm.clone(),
            resources: self.resources.clone(),
            network_state: self.network_state.clone(),
            serial_state: self.serial_state.clone(),
            filesystem_state: self.filesystem_state.clone(),
            shared_buf_state: self.shared_buf_state.clone(),
            im_state: self.im_state.clone(),
            media_state: self.media_state.clone(),
            kernel_state: self.kernel_state.clone(),
        }
    }
}

struct MethodResult(WIPICResult);

impl ResultWriter<MethodResult> for MethodResult {
    fn write(self, core: &mut ArmCore, next_pc: u32) -> Result<()> {
        core.write_return_value(&self.0.results)?;
        core.set_next_pc(next_pc)
    }
}

struct MethodProxy {
    context: SktWIPICContext,
    body: WIPICMethodBody,
}

#[async_trait::async_trait]
impl EmulatedFunction<(), MethodResult, ()> for MethodProxy {
    async fn call(&self, core: &mut ArmCore, _: &mut ()) -> Result<MethodResult> {
        // Eleven words: four in registers and the rest off the caller's frame,
        // enough for `sprintk` with eight arguments after its buffer and format.
        let args = (0..11).map(|index| u32::get(core, index)).collect::<Vec<_>>();

        let result = self.body.call(&mut self.context.clone(), args.into_boxed_slice()).await?;

        Ok(MethodResult(result))
    }
}

async fn handle_svc(core: &mut ArmCore, state: &mut SvcState, id: SvcId) -> Result<()> {
    let table = id.0 >> 8;
    let slot = id.0 & 0xff;
    let (_, lr) = core.read_pc_lr()?;

    let Some(body) = body(table, slot) else {
        // Named, with what it was handed: the guest reaches a slot through an
        // array index, so this line is the only place the number is written
        // down, and the arguments are what tells what the slot was for.
        let args = (0..4).map(|index| u32::get(core, index)).collect::<Vec<_>>();
        tracing::warn!(
            "unserved SKT WIPI-C table {table} slot {slot}({:#x}, {:#x}, {:#x}, {:#x}) from {lr:#x}",
            args[0],
            args[1],
            args[2],
            args[3]
        );

        return MethodResult(WIPICResult {
            results: vec![-1i32 as WIPICWord],
        })
        .write(core, lr);
    };

    tracing::trace!("SKT WIPI-C table {table} slot {slot} from {lr:#x}");

    let proxy = MethodProxy {
        context: state.context(core),
        body,
    };

    EmulatedFunction::call(&proxy, core, &mut ()).await?.write(core, lr)
}

pub fn register_svc_handler(core: &mut ArmCore, state: SvcState) -> Result<()> {
    core.register_svc_handler(SVC_CATEGORY_WIPIC, handle_svc, &state)
}

#[cfg(test)]
mod tests {
    use super::body;

    /// The slots 해적편 is seen calling all have a body.
    #[test]
    fn the_slots_a_title_calls_are_served() {
        for (table, slot) in [
            (0, 1),
            (0, 21),
            (0, 24),
            (1, 22),
            (3, 0),
            (3, 16),
            (5, 5),
            (6, 1),
            (7, 0),
            (7, 4),
            (7, 14),
            (14, 18),
            (14, 20),
        ] {
            assert!(body(table, slot).is_some(), "table {table} slot {slot}");
        }
    }

    #[test]
    fn a_slot_past_the_table_is_not() {
        assert!(body(0, 63).is_none());
        assert!(body(20, 0).is_none());
    }
}
