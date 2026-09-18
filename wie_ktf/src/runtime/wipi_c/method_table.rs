use alloc::{format, vec, vec::Vec};

use wipi_types::{
    ktf::wipic::{WIPICDatabaseInterface, WIPICGraphicsInterface, WIPICKnlInterface},
    wipic::WIPICWord,
};

use wie_core_arm::ArmCore;
use wie_util::{Result, WieError};
use wie_wipi_c::{
    MethodImpl, WIPICContext, WIPICMethodBody,
    api::{database, filesystem, graphics, im, kernel, media, misc, mxusermem, net, record_database, shared_buf, uic, util},
};

use crate::runtime::{
    SVC_CATEGORY_WIPIC,
    svc_ids::{WIPICDatabaseMethodId, WIPICGraphicsMethodId, WIPICKernelMethodId, WIPICTableId},
};

fn gen_stub(id: WIPICWord, name: &'static str) -> WIPICMethodBody {
    let body = move |_: &mut dyn WIPICContext| async move { Err::<(), _>(WieError::Unimplemented(format!("{id}: {name}"))) };

    body.into_body()
}

/// How many functions every WIPI C interface table holds.
///
/// A table is an array of function pointers in guest memory and the guest
/// indexes it directly, so a table written only as long as the functions we
/// serve is a table a title can index past: it reads a word that was never a
/// function, branches to it, and the run ends on whatever that word happened to
/// be. 데몬헌터 does exactly that - it asks the net table for slot 30, one past
/// `MC_netHttpClose`, and read a zero there, so the authentication attempt
/// faulted at `pc = 0` with nothing in the log to say which call it was.
///
/// So every table is this long whatever we have written for it, and the slots
/// past the end answer [`gen_missing`]. The number is the reference's, whose
/// largest original interface is the graphics table, well under it.
pub const WIPIC_TABLE_FUNCTIONS: u16 = 64;

/// The answer a table slot gives when the original interface had a function
/// there and this runtime has none.
///
/// It is a refusal rather than a fault, because a WIPI C call that cannot be
/// served has a documented way to say so and a game's own error path is written
/// for it. The line it logs names the table and the slot, which is the only
/// place either number is ever written down: the guest reaches a function
/// through an array index, so no name for it appears in its own code.
/// The answer a slot no function stands behind gives: -1, the error a WIPI C
/// function reports for anything it cannot do.
///
/// The call is named where it is dispatched rather than here - see
/// `describe_unserved_call`, which has the registers and the memory they point
/// at, and this has neither.
fn gen_missing(table_id: WIPICTableId, function_id: u16) -> WIPICMethodBody {
    let body = move |_: &mut dyn WIPICContext| async move {
        // Named, because a slot that answers in silence is a slot a title can
        // call six hundred times with nothing in the log to say so. 마스터오브
        // 소드4 draws no text and the run records no text call at all - the
        // only place the number it reached is ever written down is here.
        tracing::warn!("unserved {table_id:?}-{function_id}");
        Ok::<i32, WieError>(-1)
    };

    body.into_body()
}

pub fn get_kernel_interface(core: &mut ArmCore) -> Result<WIPICKnlInterface> {
    let table_id = WIPICTableId::Kernel;

    Ok(WIPICKnlInterface {
        printk: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICKernelMethodId::Printk))?,
        sprintk: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICKernelMethodId::Sprintk))?,
        get_exec_names: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICKernelMethodId::GetExecNames))?,
        execute: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICKernelMethodId::Execute))?,
        mexecute: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICKernelMethodId::Mexecute))?,
        load: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICKernelMethodId::Load))?,
        mload: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICKernelMethodId::Mload))?,
        exit: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICKernelMethodId::Exit))?,
        program_stop: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICKernelMethodId::ProgramStop))?,
        get_cur_program_id: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICKernelMethodId::GetCurProgramId))?,
        get_parent_program_id: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICKernelMethodId::GetParentProgramId))?,
        get_app_manager_id: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICKernelMethodId::GetAppManagerId))?,
        get_program_info: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICKernelMethodId::GetProgramInfo))?,
        get_access_level: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICKernelMethodId::GetAccessLevel))?,
        get_program_name: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICKernelMethodId::GetProgramName))?,
        create_shared_buf: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICKernelMethodId::CreateSharedBuf))?,
        destroy_shared_buf: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICKernelMethodId::DestroySharedBuf))?,
        get_shared_buf: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICKernelMethodId::GetSharedBuf))?,
        get_shared_buf_size: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICKernelMethodId::GetSharedBufSize))?,
        resize_shared_buf: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICKernelMethodId::ResizeSharedBuf))?,
        alloc: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICKernelMethodId::Alloc))?,
        calloc: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICKernelMethodId::Calloc))?,
        free: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICKernelMethodId::Free))?,
        get_total_memory: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICKernelMethodId::GetTotalMemory))?,
        get_free_memory: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICKernelMethodId::GetFreeMemory))?,
        def_timer: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICKernelMethodId::DefTimer))?,
        set_timer: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICKernelMethodId::SetTimer))?,
        unset_timer: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICKernelMethodId::UnsetTimer))?,
        current_time: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICKernelMethodId::CurrentTime))?,
        get_system_property: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICKernelMethodId::GetSystemProperty))?,
        set_system_property: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICKernelMethodId::SetSystemProperty))?,
        get_resource_id: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICKernelMethodId::GetResourceId))?,
        get_resource: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICKernelMethodId::GetResource))?,
        reserved1: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICKernelMethodId::Reserved1))?,
        reserved2: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICKernelMethodId::Reserved2))?,
        reserved3: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICKernelMethodId::Reserved3))?,
        reserved4: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICKernelMethodId::Reserved4))?,
        reserved5: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICKernelMethodId::Reserved5))?,
        reserved6: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICKernelMethodId::Reserved6))?,
        reserved7: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICKernelMethodId::Reserved7))?,
        reserved8: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICKernelMethodId::Reserved8))?,
        reserved9: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICKernelMethodId::Reserved9))?,
        reserved10: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICKernelMethodId::Reserved10))?,
        reserved11: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICKernelMethodId::Reserved11))?,
        send_message: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICKernelMethodId::SendMessage))?,
        set_timer_ex: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICKernelMethodId::SetTimerEx))?,
        get_system_state: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICKernelMethodId::GetSystemState))?,
        create_system_progress_bar: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICKernelMethodId::CreateSystemProgressBar))?,
        set_system_progress_bar: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICKernelMethodId::SetSystemProgressBar))?,
        destroy_system_progress_bar: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICKernelMethodId::DestroySystemProgressBar))?,
        execute_ex: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICKernelMethodId::ExecuteEx))?,
        get_proc_address: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICKernelMethodId::GetProcAddress))?,
        unload: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICKernelMethodId::Unload))?,
        create_sys_message_box: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICKernelMethodId::CreateSysMessageBox))?,
        destroy_sys_message_box: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICKernelMethodId::DestroySysMessageBox))?,
        get_program_id_list: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICKernelMethodId::GetProgramIdList))?,
        get_program_info2: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICKernelMethodId::GetProgramInfo2))?,
        reserved12: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICKernelMethodId::Reserved12))?,
        reserved13: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICKernelMethodId::Reserved13))?,
        create_app_private_area: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICKernelMethodId::CreateAppPrivateArea))?,
        get_app_private_area: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICKernelMethodId::GetAppPrivateArea))?,
        create_lib_private_area: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICKernelMethodId::CreateLibPrivateArea))?,
        get_lib_private_area: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICKernelMethodId::GetLibPrivateArea))?,
        get_platform_version: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICKernelMethodId::GetPlatformVersion))?,
        get_token: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICKernelMethodId::GetToken))?,
    })
}

pub fn get_graphics_interface(core: &mut ArmCore) -> Result<WIPICGraphicsInterface> {
    let table_id = WIPICTableId::Graphics;

    Ok(WIPICGraphicsInterface {
        get_image_property: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICGraphicsMethodId::GetImageProperty))?,
        get_image_framebuffer: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICGraphicsMethodId::GetImageFramebuffer))?,
        get_screen_framebuffer: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICGraphicsMethodId::GetScreenFramebuffer))?,
        destroy_offscreen_framebuffer: core.make_svc_stub(
            SVC_CATEGORY_WIPIC,
            table_id.function_id(WIPICGraphicsMethodId::DestroyOffscreenFramebuffer),
        )?,
        create_offscreen_framebuffer: core.make_svc_stub(
            SVC_CATEGORY_WIPIC,
            table_id.function_id(WIPICGraphicsMethodId::CreateOffscreenFramebuffer),
        )?,
        init_context: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICGraphicsMethodId::InitContext))?,
        set_context: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICGraphicsMethodId::SetContext))?,
        get_context: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICGraphicsMethodId::GetContext))?,
        put_pixel: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICGraphicsMethodId::PutPixel))?,
        draw_line: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICGraphicsMethodId::DrawLine))?,
        draw_rect: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICGraphicsMethodId::DrawRect))?,
        fill_rect: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICGraphicsMethodId::FillRect))?,
        copy_frame_buffer: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICGraphicsMethodId::CopyFrameBuffer))?,
        draw_image: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICGraphicsMethodId::DrawImage))?,
        copy_area: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICGraphicsMethodId::CopyArea))?,
        draw_arc: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICGraphicsMethodId::DrawArc))?,
        fill_arc: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICGraphicsMethodId::FillArc))?,
        draw_string: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICGraphicsMethodId::DrawString))?,
        draw_unicode_string: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICGraphicsMethodId::DrawUnicodeString))?,
        get_rgb_pixels: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICGraphicsMethodId::GetRgbPixels))?,
        set_rgb_pixels: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICGraphicsMethodId::SetRgbPixels))?,
        flush_lcd: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICGraphicsMethodId::FlushLcd))?,
        get_pixel_from_rgb: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICGraphicsMethodId::GetPixelFromRgb))?,
        get_rgb_from_pixel: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICGraphicsMethodId::GetRgbFromPixel))?,
        get_display_info: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICGraphicsMethodId::GetDisplayInfo))?,
        repaint: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICGraphicsMethodId::Repaint))?,
        get_font: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICGraphicsMethodId::GetFont))?,
        get_font_height: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICGraphicsMethodId::GetFontHeight))?,
        get_font_ascent: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICGraphicsMethodId::GetFontAscent))?,
        get_font_descent: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICGraphicsMethodId::GetFontDescent))?,
        get_string_width: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICGraphicsMethodId::GetStringWidth))?,
        get_unicode_string_width: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICGraphicsMethodId::GetUnicodeStringWidth))?,
        create_image: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICGraphicsMethodId::CreateImage))?,
        destroy_image: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICGraphicsMethodId::DestroyImage))?,
        decode_next_image: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICGraphicsMethodId::DecodeNextImage))?,
        encode_image: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICGraphicsMethodId::EncodeImage))?,
        post_event: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICGraphicsMethodId::PostEvent))?,
        handle_input: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICGraphicsMethodId::HandleInput))?,
        set_current_mode: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICGraphicsMethodId::SetCurrentMode))?,
        get_current_mode: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICGraphicsMethodId::GetCurrentMode))?,
        get_support_mode_count: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICGraphicsMethodId::GetSupportModeCount))?,
        get_supported_modes: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICGraphicsMethodId::GetSupportedModes))?,
        fill_polygon: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICGraphicsMethodId::FillPolygon))?,
        draw_polygon: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICGraphicsMethodId::DrawPolygon))?,
        show_annunciator: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICGraphicsMethodId::ShowAnnunciator))?,
        get_annunciator_info: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICGraphicsMethodId::GetAnnunciatorInfo))?,
        set_annunciator_icon: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICGraphicsMethodId::SetAnnunciatorIcon))?,
        get_idle_help_line_info: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICGraphicsMethodId::GetIdleHelpLineInfo))?,
        show_help_line: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICGraphicsMethodId::ShowHelpLine))?,
        get_char_glyph: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICGraphicsMethodId::GetCharGlyph))?,
        create_image_ex: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICGraphicsMethodId::CreateImageEx))?,
        hide_help_line: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICGraphicsMethodId::HideHelpLine))?,
        set_clone_screen_framebuffer: core
            .make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICGraphicsMethodId::SetCloneScreenFramebuffer))?,
        get_font_ex: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICGraphicsMethodId::GetFontEx))?,
        get_font_lists: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICGraphicsMethodId::GetFontLists))?,
        get_font_info: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICGraphicsMethodId::GetFontInfo))?,
        set_font_help_line: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICGraphicsMethodId::SetFontHelpLine))?,
        get_font_help_line: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICGraphicsMethodId::GetFontHelpLine))?,
        encode_image_ex: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICGraphicsMethodId::EncodeImageEx))?,
        get_image_info: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICGraphicsMethodId::GetImageInfo))?,
    })
}

pub fn get_util_method_table() -> Vec<WIPICMethodBody> {
    vec![
        util::htonl.into_body(),
        util::htons.into_body(),
        util::ntohl.into_body(),
        util::ntohs.into_body(),
        util::inet_addr_int.into_body(),
        util::inet_addr_str.into_body(),
        gen_stub(6, "OEMC_utilHashbySHA1"),
    ]
}

pub fn get_misc_method_table() -> Vec<WIPICMethodBody> {
    vec![
        misc::back_light.into_body(),
        misc::set_led.into_body(),
        misc::get_led.into_body(),
        misc::get_led_count.into_body(),
        gen_stub(4, "OEMC_miscGetCompassData"),
    ]
}

pub fn get_database_interface(core: &mut ArmCore) -> Result<WIPICDatabaseInterface> {
    let table_id = WIPICTableId::Database;

    Ok(WIPICDatabaseInterface {
        open_database: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICDatabaseMethodId::OpenDatabase))?,
        read_record_single: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICDatabaseMethodId::StreamRead))?,
        write_record_single: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICDatabaseMethodId::StreamWrite))?,
        close_database: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICDatabaseMethodId::CloseDatabase))?,
        select_record: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICDatabaseMethodId::SelectRecord))?,
        update_record: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICDatabaseMethodId::UpdateRecord))?,
        delete_record: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICDatabaseMethodId::DeleteRecord))?,
        list_record: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICDatabaseMethodId::ListRecord))?,
        // The interface struct's field names are the database reading of this
        // table; slots 8 and 12 are the filesystem's - see `WIPICDatabaseMethodId`.
        sort_records: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICDatabaseMethodId::MakeDirectory))?,
        get_access_mode: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICDatabaseMethodId::GetAccessMode))?,
        get_number_of_records: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICDatabaseMethodId::GetNumberOfRecords))?,
        get_record_size: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICDatabaseMethodId::GetRecordSize))?,
        list_databases: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICDatabaseMethodId::Available))?,
        unk13: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICDatabaseMethodId::Unk13))?,
        unk14: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICDatabaseMethodId::Unk14))?,
        unk15: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICDatabaseMethodId::Unk15))?,
        unk16: core.make_svc_stub(SVC_CATEGORY_WIPIC, table_id.function_id(WIPICDatabaseMethodId::Exists))?,
    })
}

pub fn get_uic_method_table() -> Vec<WIPICMethodBody> {
    vec![
        uic::create_application_context.into_body(),
        uic::get_class.into_body(),
        uic::create.into_body(),
        uic::destroy.into_body(),
        uic::repaint.into_body(),
        uic::paint.into_body(),
        uic::get_class_name.into_body(),
        uic::is_instance.into_body(),
        uic::handle_event.into_body(),
        uic::configure.into_body(),
        uic::get_geometry.into_body(),
        uic::set_enable.into_body(),
        uic::set_callback.into_body(),
        uic::set_event_handler.into_body(),
        uic::set_font.into_body(),
        uic::get_font.into_body(),
        uic::set_fg_color.into_body(),
        uic::set_bg_color.into_body(),
        uic::set_label.into_body(),
        uic::get_label.into_body(),
        uic::set_label_alignment.into_body(),
        uic::set_time_mask.into_body(),
        uic::set_time.into_body(),
        uic::set_time_long.into_body(),
        uic::get_time.into_body(),
        uic::add_menu_item.into_body(),
        uic::get_menu_item.into_body(),
        uic::remove_menu_item.into_body(),
        uic::set_active_menu_item.into_body(),
        uic::get_active_menu_item.into_body(),
        uic::insert_text.into_body(),
        uic::delete_text.into_body(),
        uic::get_max_text_size.into_body(),
        uic::set_max_text_size.into_body(),
        uic::get_text_size.into_body(),
        uic::get_text.into_body(),
        uic::add_list_item.into_body(),
        uic::get_list_item.into_body(),
        uic::remove_list_item.into_body(),
        uic::set_active_list_item.into_body(),
        uic::get_active_list_item.into_body(),
        uic::get_cursor_pos.into_body(),
        uic::set_cursor_pos.into_body(),
        gen_stub(43, "OEMC_uicSetLineGap"),
        gen_stub(44, "OEMC_uicGetLineGap"),
    ]
}

pub fn get_media_method_table() -> Vec<WIPICMethodBody> {
    vec![
        media::clip_create.into_body(),
        gen_stub(1, "MC_mdaUnk1"),
        gen_stub(2, "MC_mdaUnk2"),
        media::clip_free.into_body(),
        media::clip_put_data.into_body(),
        gen_stub(5, "MC_mdaUnk5"),
        gen_stub(6, "MC_mdaUnk6"),
        media::unk7.into_body(),
        media::play.into_body(),
        media::pause.into_body(),
        media::resume.into_body(),
        media::stop.into_body(),
        gen_stub(12, "MC_mdaUnk12"),
        gen_stub(13, "MC_mdaUnk13"),
        media::get_volume.into_body(),
        media::set_volume.into_body(),
        media::vibrator.into_body(),
        media::unk17.into_body(),
        media::unk18.into_body(),
        gen_stub(19, "MC_mdaUnk19"),
        gen_stub(20, "MC_mdaUnk20"),
        gen_stub(21, "MC_mdaUnk21"),
        gen_stub(22, "MC_mdaUnk22"),
        gen_stub(23, "MC_mdaUnk23"),
        gen_stub(24, "MC_mdaUnk24"),
        media::clip_get_volume.into_body(),
        media::clip_set_volume.into_body(),
    ]
}

pub fn get_net_method_table() -> Vec<WIPICMethodBody> {
    vec![
        // The same three the other vendor already serves for real. KTF was left
        // on stubs that refuse: `MC_netConnect` reported failure through the
        // caller's callback and nothing opened a socket afterwards, which is
        // where 데몬헌터 stops - one connect in a whole capture and then only
        // its own event loop. The refusal is the reference's deliberate answer
        // to having no network; this runtime answers protocols in process
        // instead, and a title that is never connected never reaches the
        // endpoint that would answer it.
        net::connect.into_body(),
        net::close.into_body(),
        net::socket.into_body(),
        net::socket_connect.into_body(),
        net::socket_write.into_body(),
        net::socket_read.into_body(),
        net::socket_close.into_body(),
        net::socket_bind.into_body(),
        net::get_max_packet_length.into_body(),
        net::socket_send_to.into_body(),
        net::socket_recv_from.into_body(),
        net::get_host_addr.into_body(),
        net::socket_accept.into_body(),
        net::set_read_callback.into_body(),
        net::set_write_callback.into_body(),
        net::http_open.into_body(),
        net::http_connect.into_body(),
        net::http_set_request_method.into_body(),
        net::http_get_request_method.into_body(),
        net::http_set_request_property.into_body(),
        net::http_get_request_property.into_body(),
        net::http_set_proxy.into_body(),
        net::http_get_proxy.into_body(),
        net::http_get_response_code.into_body(),
        net::http_get_response_message.into_body(),
        net::http_get_header_field.into_body(),
        net::http_get_length.into_body(),
        net::http_get_type.into_body(),
        net::http_get_encoding.into_body(),
        net::http_close.into_body(),
        // The carrier's own additions to the table start here; see
        // `net::socket_connect_by_name`.
        net::socket_connect_by_name.into_body(),
        // Slot 31 is the write on that connection, and 데몬헌터's own request is
        // what says so. Refused, it called this fourteen times in three seconds
        // with `(1, 0x1796a0, 0x30)` - its descriptor, a buffer, and 48 bytes -
        // and those 48 bytes are its authentication, already built:
        //
        //   00 00 00 2c  IR \t 01046119269 \t demon \t 1.0.2 \t 5080091 \t WIPIC \t yes
        //
        // A four-byte length ahead of the record the title's own format string
        // spells, `IR\t%s\t%s\t%s\t%s\tWIPIC\t%s`. A buffer a title has filled
        // is one it means to send, so the arguments are `MC_netSocketWrite`'s
        // and the framing is the title's own.
        net::socket_write.into_body(),
        // Slot 32 is the read that carries the answer back, and the title's own
        // loop says so: once its request went out it called this with
        // `(1, 0x179694, 0x10)` and then `(1, 0x179695, 0xf)`, `(1, 0x179696,
        // 0xe)` - one buffer, advanced by what it has taken, shortened by the
        // same. That is a transfer resuming where it left off, into a buffer
        // holding nothing, which is the direction the write is not.
        net::socket_read.into_body(),
        // Slot 33 is not known. No title here reaches it, and a slot nothing
        // has asked for is not one to invent - it refuses the way every
        // unwritten slot does, and says so.
        gen_missing(WIPICTableId::Net, 33),
        // Slot 34 is the question a title asks before it opens anything; see
        // `net::check_server`. 드래곤로드's data download stops dead without
        // it.
        net::check_server.into_body(),
    ]
}

/// A slot in a table whose meaning nothing has shown yet.
///
/// It takes four arguments and writes them down. A slot like this is only ever
/// identified by what a title hands it - there is no name for it anywhere in
/// the title's own code, which reaches it by index - so the arguments are the
/// whole of the evidence, and a line that says only that the slot was reached
/// throws that evidence away. Four is what the ARM calling convention passes in
/// registers, so they cost nothing to read and are the ones always there.
fn gen_unk_stub(id: u32, index: u32) -> WIPICMethodBody {
    let body = move |_: &mut dyn WIPICContext, a0: WIPICWord, a1: WIPICWord, a2: WIPICWord, a3: WIPICWord| async move {
        tracing::warn!("stub unk{id}-{index}({a0:#x}, {a1:#x}, {a2:#x}, {a3:#x})");
        Ok::<u32, _>(0)
    };

    body.into_body()
}

pub fn get_unk3_method_table() -> Vec<WIPICMethodBody> {
    vec![
        gen_unk_stub(3, 0),
        gen_unk_stub(3, 1),
        gen_unk_stub(3, 2),
        gen_unk_stub(3, 3),
        gen_unk_stub(3, 4),
    ]
}

/// Table 12 - what a title asks about the handset it is running on.
///
/// Not a drawing table, which is worth writing down because its traffic looks
/// like drawing traffic: 마스터오브소드4 calls slot 1 three hundred times in a
/// session, once per frame, and slot 0 once at startup. Following what its
/// arguments point at settles it - slot 0 is handed the handset's phone
/// number, `"01046119269"`, and both slots are handed this table's own array
/// of function pointers as their last argument, the way a C interface passes
/// itself. The words in the registers never change between calls; everything
/// that does is behind them.
pub fn get_unk12_method_table() -> Vec<WIPICMethodBody> {
    vec![gen_unk_stub(12, 0), gen_unk_stub(12, 1), gen_unk_stub(12, 2)]
}

pub fn get_method_body(table_id: WIPICTableId, function_id: u16) -> Option<WIPICMethodBody> {
    get_served_method_body(table_id, function_id).or_else(|| (function_id < WIPIC_TABLE_FUNCTIONS).then(|| gen_missing(table_id, function_id)))
}

/// The body this runtime has written for a slot, if it has written one.
pub fn get_served_method_body(table_id: WIPICTableId, function_id: u16) -> Option<WIPICMethodBody> {
    match table_id {
        WIPICTableId::Kernel => match WIPICKernelMethodId::try_from(function_id).ok()? {
            WIPICKernelMethodId::Printk => Some(kernel::printk.into_body()),
            WIPICKernelMethodId::Sprintk => Some(kernel::sprintk.into_body()),
            WIPICKernelMethodId::GetExecNames => Some(kernel::get_exec_names.into_body()),
            WIPICKernelMethodId::Execute => Some(kernel::execute.into_body()),
            WIPICKernelMethodId::Mexecute => Some(kernel::mexecute.into_body()),
            WIPICKernelMethodId::Load => Some(kernel::load.into_body()),
            WIPICKernelMethodId::Mload => Some(kernel::mload.into_body()),
            WIPICKernelMethodId::Exit => Some(kernel::exit.into_body()),
            WIPICKernelMethodId::ProgramStop => Some(kernel::program_stop.into_body()),
            WIPICKernelMethodId::GetCurProgramId => Some(kernel::get_cur_program_id.into_body()),
            WIPICKernelMethodId::GetParentProgramId => Some(kernel::get_parent_program_id.into_body()),
            WIPICKernelMethodId::GetAppManagerId => Some(kernel::get_app_manager_id.into_body()),
            WIPICKernelMethodId::GetProgramInfo => Some(kernel::get_program_info.into_body()),
            WIPICKernelMethodId::GetAccessLevel => Some(kernel::get_access_level.into_body()),
            WIPICKernelMethodId::GetProgramName => Some(kernel::get_program_name.into_body()),
            WIPICKernelMethodId::CreateSharedBuf => Some(shared_buf::create_shared_buf.into_body()),
            WIPICKernelMethodId::DestroySharedBuf => Some(shared_buf::destroy_shared_buf.into_body()),
            WIPICKernelMethodId::GetSharedBuf => Some(shared_buf::get_shared_buf.into_body()),
            WIPICKernelMethodId::GetSharedBufSize => Some(shared_buf::get_shared_buf_size.into_body()),
            WIPICKernelMethodId::ResizeSharedBuf => Some(shared_buf::resize_shared_buf.into_body()),
            WIPICKernelMethodId::Alloc => Some(kernel::alloc.into_body()),
            WIPICKernelMethodId::Calloc => Some(kernel::calloc.into_body()),
            WIPICKernelMethodId::Free => Some(kernel::free.into_body()),
            WIPICKernelMethodId::GetTotalMemory => Some(kernel::get_total_memory.into_body()),
            WIPICKernelMethodId::GetFreeMemory => Some(kernel::get_free_memory.into_body()),
            WIPICKernelMethodId::DefTimer => Some(kernel::def_timer.into_body()),
            WIPICKernelMethodId::SetTimer => Some(kernel::set_timer.into_body()),
            WIPICKernelMethodId::UnsetTimer => Some(kernel::unset_timer.into_body()),
            WIPICKernelMethodId::CurrentTime => Some(kernel::current_time.into_body()),
            WIPICKernelMethodId::GetSystemProperty => Some(kernel::get_system_property.into_body()),
            WIPICKernelMethodId::SetSystemProperty => Some(kernel::set_system_property.into_body()),
            WIPICKernelMethodId::GetResourceId => Some(kernel::get_resource_id.into_body()),
            WIPICKernelMethodId::GetResource => Some(kernel::get_resource.into_body()),
            WIPICKernelMethodId::Reserved1 => None,
            WIPICKernelMethodId::Reserved2 => Some(gen_stub(34, "MC_knlReserved2")),
            WIPICKernelMethodId::Reserved3 => Some(gen_stub(35, "MC_knlReserved3")),
            WIPICKernelMethodId::Reserved4 => Some(kernel::get_dll_interface.into_body()),
            WIPICKernelMethodId::Reserved5 => Some(gen_stub(37, "MC_knlReserved5")),
            WIPICKernelMethodId::Reserved6 => Some(gen_stub(38, "MC_knlReserved6")),
            WIPICKernelMethodId::Reserved7 => Some(gen_stub(39, "MC_knlReserved7")),
            WIPICKernelMethodId::Reserved8 => Some(gen_stub(40, "MC_knlReserved8")),
            WIPICKernelMethodId::Reserved9 => Some(gen_stub(41, "MC_knlReserved9")),
            WIPICKernelMethodId::Reserved10 => Some(gen_stub(42, "MC_knlReserved10")),
            WIPICKernelMethodId::Reserved11 => Some(gen_stub(43, "MC_knlReserved11")),
            WIPICKernelMethodId::SendMessage => Some(gen_stub(44, "OEMC_knlSendMessage")),
            WIPICKernelMethodId::SetTimerEx => Some(gen_stub(45, "OEMC_knlSetTimerEx")),
            WIPICKernelMethodId::GetSystemState => Some(gen_stub(46, "OEMC_knlGetSystemState")),
            WIPICKernelMethodId::CreateSystemProgressBar => Some(gen_stub(47, "OEMC_knlCreateSystemProgressBar")),
            WIPICKernelMethodId::SetSystemProgressBar => Some(gen_stub(48, "OEMC_knlSetSystemProgressBar")),
            WIPICKernelMethodId::DestroySystemProgressBar => Some(gen_stub(49, "OEMC_knlDestroySystemProgressBar")),
            WIPICKernelMethodId::ExecuteEx => Some(gen_stub(50, "OEMC_knlExecuteEx")),
            WIPICKernelMethodId::GetProcAddress => Some(gen_stub(51, "OEMC_knlGetProcAddress")),
            WIPICKernelMethodId::Unload => Some(gen_stub(52, "OEMC_knlUnload")),
            WIPICKernelMethodId::CreateSysMessageBox => Some(gen_stub(53, "OEMC_knlCreateSysMessageBox")),
            WIPICKernelMethodId::DestroySysMessageBox => Some(gen_stub(54, "OEMC_knlDestroySysMessageBox")),
            WIPICKernelMethodId::GetProgramIdList => Some(gen_stub(55, "OEMC_knlGetProgramIDList")),
            WIPICKernelMethodId::GetProgramInfo2 => Some(gen_stub(56, "OEMC_knlGetProgramInfo")),
            WIPICKernelMethodId::Reserved12 => Some(gen_stub(57, "MC_knlReserved12")),
            WIPICKernelMethodId::Reserved13 => Some(gen_stub(58, "MC_knlReserved13")),
            WIPICKernelMethodId::CreateAppPrivateArea => Some(gen_stub(59, "OEMC_knlCreateAppPrivateArea")),
            WIPICKernelMethodId::GetAppPrivateArea => Some(gen_stub(60, "OEMC_knlGetAppPrivateArea")),
            WIPICKernelMethodId::CreateLibPrivateArea => Some(gen_stub(61, "OEMC_knlCreateLibPrivateArea")),
            WIPICKernelMethodId::GetLibPrivateArea => Some(gen_stub(62, "OEMC_knlGetLibPrivateArea")),
            WIPICKernelMethodId::GetPlatformVersion => Some(gen_stub(63, "OEMC_knlGetPlatformVersion")),
            WIPICKernelMethodId::GetToken => Some(gen_stub(64, "OEMC_knlGetToken")),
        },
        WIPICTableId::Util => get_util_method_table().into_iter().nth(function_id as usize),
        WIPICTableId::Misc => get_misc_method_table().into_iter().nth(function_id as usize),
        WIPICTableId::Graphics => match WIPICGraphicsMethodId::try_from(function_id).ok()? {
            WIPICGraphicsMethodId::GetImageProperty => Some(graphics::get_image_property.into_body()),
            WIPICGraphicsMethodId::GetImageFramebuffer => Some(graphics::get_image_framebuffer.into_body()),
            WIPICGraphicsMethodId::GetScreenFramebuffer => Some(graphics::get_screen_framebuffer.into_body()),
            WIPICGraphicsMethodId::DestroyOffscreenFramebuffer => Some(graphics::destroy_offscreen_framebuffer.into_body()),
            WIPICGraphicsMethodId::CreateOffscreenFramebuffer => Some(graphics::create_offscreen_framebuffer.into_body()),
            WIPICGraphicsMethodId::InitContext => Some(graphics::init_context.into_body()),
            WIPICGraphicsMethodId::SetContext => Some(graphics::set_context.into_body()),
            WIPICGraphicsMethodId::GetContext => Some(graphics::get_context.into_body()),
            WIPICGraphicsMethodId::PutPixel => Some(graphics::put_pixel.into_body()),
            WIPICGraphicsMethodId::DrawLine => Some(graphics::draw_line.into_body()),
            WIPICGraphicsMethodId::DrawRect => Some(graphics::draw_rect.into_body()),
            WIPICGraphicsMethodId::FillRect => Some(graphics::fill_rect.into_body()),
            WIPICGraphicsMethodId::CopyFrameBuffer => Some(graphics::copy_frame_buffer.into_body()),
            WIPICGraphicsMethodId::DrawImage => Some(graphics::draw_image.into_body()),
            WIPICGraphicsMethodId::CopyArea => Some(graphics::copy_area.into_body()),
            WIPICGraphicsMethodId::DrawArc => Some(graphics::draw_arc.into_body()),
            WIPICGraphicsMethodId::FillArc => Some(graphics::fill_arc.into_body()),
            WIPICGraphicsMethodId::DrawString => Some(graphics::draw_string.into_body()),
            WIPICGraphicsMethodId::DrawUnicodeString => Some(graphics::draw_unicode_string.into_body()),
            WIPICGraphicsMethodId::GetRgbPixels => Some(graphics::get_rgb_pixels.into_body()),
            WIPICGraphicsMethodId::SetRgbPixels => Some(graphics::set_rgb_pixels.into_body()),
            WIPICGraphicsMethodId::FlushLcd => Some(graphics::flush_lcd.into_body()),
            WIPICGraphicsMethodId::GetPixelFromRgb => Some(graphics::get_pixel_from_rgb.into_body()),
            WIPICGraphicsMethodId::GetRgbFromPixel => Some(graphics::get_rgb_from_pixel.into_body()),
            WIPICGraphicsMethodId::GetDisplayInfo => Some(graphics::get_display_info.into_body()),
            WIPICGraphicsMethodId::Repaint => Some(graphics::repaint.into_body()),
            WIPICGraphicsMethodId::GetFont => Some(graphics::get_font.into_body()),
            WIPICGraphicsMethodId::GetFontHeight => Some(graphics::get_font_height.into_body()),
            WIPICGraphicsMethodId::GetFontAscent => Some(graphics::get_font_ascent.into_body()),
            WIPICGraphicsMethodId::GetFontDescent => Some(graphics::get_font_descent.into_body()),
            WIPICGraphicsMethodId::GetStringWidth => Some(graphics::get_string_width.into_body()),
            WIPICGraphicsMethodId::GetUnicodeStringWidth => Some(graphics::get_unicode_string_width.into_body()),
            WIPICGraphicsMethodId::CreateImage => Some(graphics::create_image.into_body()),
            WIPICGraphicsMethodId::DestroyImage => Some(graphics::destroy_image.into_body()),
            WIPICGraphicsMethodId::DecodeNextImage => Some(graphics::decode_next_image.into_body()),
            WIPICGraphicsMethodId::EncodeImage => Some(graphics::encode_image.into_body()),
            WIPICGraphicsMethodId::PostEvent => Some(graphics::post_event.into_body()),
            WIPICGraphicsMethodId::HandleInput => Some(im::handle_input.into_body()),
            WIPICGraphicsMethodId::SetCurrentMode => Some(im::set_current_mode.into_body()),
            WIPICGraphicsMethodId::GetCurrentMode => Some(im::get_current_mode.into_body()),
            WIPICGraphicsMethodId::GetSupportModeCount => Some(im::get_support_mode_count.into_body()),
            WIPICGraphicsMethodId::GetSupportedModes => Some(im::get_supported_modes.into_body()),
            WIPICGraphicsMethodId::FillPolygon => Some(graphics::fill_polygon.into_body()),
            WIPICGraphicsMethodId::DrawPolygon => Some(graphics::draw_polygon.into_body()),
            WIPICGraphicsMethodId::ShowAnnunciator => Some(gen_stub(44, "OEMC_grpShowAnnunciator")),
            WIPICGraphicsMethodId::GetAnnunciatorInfo => Some(gen_stub(45, "OEMC_grpGetAnnunciatorInfo")),
            WIPICGraphicsMethodId::SetAnnunciatorIcon => Some(gen_stub(46, "OEMC_grp  SetAnnunciatorIcon")),
            WIPICGraphicsMethodId::GetIdleHelpLineInfo => Some(gen_stub(47, "OEMC_grpGetIdleHelpLineInfo")),
            WIPICGraphicsMethodId::ShowHelpLine => Some(gen_stub(48, "OEMC_grpShowHelpLine")),
            WIPICGraphicsMethodId::GetCharGlyph => Some(gen_stub(49, "OEMC_grpGetCharGlyph")),
            WIPICGraphicsMethodId::CreateImageEx => Some(gen_stub(50, "OEMC_grpCreateImageEx")),
            WIPICGraphicsMethodId::HideHelpLine => Some(gen_stub(51, "OEMC_grpHideHelpLine")),
            WIPICGraphicsMethodId::SetCloneScreenFramebuffer => Some(gen_stub(52, "OEMC_grpSetCloneScreenFrameBuffer")),
            WIPICGraphicsMethodId::GetFontEx => Some(gen_stub(53, "OEMC_grpGetFontEx")),
            WIPICGraphicsMethodId::GetFontLists => Some(gen_stub(54, "OEMC_grpGetFontLists")),
            WIPICGraphicsMethodId::GetFontInfo => Some(gen_stub(55, "OEMC_grpGetFontInfo")),
            WIPICGraphicsMethodId::SetFontHelpLine => Some(gen_stub(56, "OEMC_grpSetFontHelpLine")),
            WIPICGraphicsMethodId::GetFontHelpLine => Some(gen_stub(57, "OEMC_grpGetFontHelpLine")),
            WIPICGraphicsMethodId::EncodeImageEx => Some(gen_stub(58, "OEMC_grpEncodeImageEx")),
            WIPICGraphicsMethodId::GetImageInfo => Some(gen_stub(59, "OEMC_grpGetImageInfo")),
        },
        WIPICTableId::Interface3 => get_unk3_method_table().into_iter().nth(function_id as usize),
        // Table 5 is the record database - KTF's other storage API. See
        // `wie_wipi_c::api::record_database` for what each slot is and how the
        // numbers were settled.
        WIPICTableId::Interface4 => match function_id {
            0 => Some(record_database::open.into_body()),
            1 => Some(record_database::close.into_body()),
            2 => Some(record_database::delete_database.into_body()),
            3 => Some(record_database::insert_record.into_body()),
            4 => Some(record_database::select_record.into_body()),
            5 => Some(record_database::update_record.into_body()),
            6 => Some(record_database::delete_record.into_body()),
            7 => Some(record_database::list_records.into_body()),
            10 => Some(record_database::number_of_records.into_body()),
            11 => Some(record_database::record_size.into_body()),
            12 => Some(database::available_storage_ktf.into_body()),
            // A slot nothing has shown the meaning of. Name the number rather
            // than the table: it is the only thing that says which call it was,
            // and one label for sixty-four functions says nothing at all.
            _ => (function_id < WIPIC_TABLE_FUNCTIONS).then(|| gen_stub(function_id as _, "table 5 (record database)")),
        },
        WIPICTableId::Interface5 => (function_id < WIPIC_TABLE_FUNCTIONS).then(|| gen_stub(function_id as _, "table 6")),
        // The extension library's four calls, in the order the reference
        // writes them and the order 마스터오브소드4 indexes them.
        WIPICTableId::MxUserMem => match function_id {
            0 => Some(mxusermem::add.into_body()),
            1 => Some(mxusermem::alloc.into_body()),
            2 => Some(mxusermem::realloc.into_body()),
            3 => Some(mxusermem::free.into_body()),
            _ => (function_id < WIPIC_TABLE_FUNCTIONS).then(|| gen_missing(WIPICTableId::MxUserMem, function_id)),
        },
        WIPICTableId::Database => match WIPICDatabaseMethodId::try_from(function_id).ok()? {
            WIPICDatabaseMethodId::OpenDatabase => Some(database::open_database.into_body()),
            WIPICDatabaseMethodId::StreamRead => Some(database::stream_read.into_body()),
            WIPICDatabaseMethodId::StreamWrite => Some(database::stream_write.into_body()),
            WIPICDatabaseMethodId::CloseDatabase => Some(database::close_database.into_body()),
            WIPICDatabaseMethodId::SelectRecord => Some(database::select_record_ktf.into_body()),
            WIPICDatabaseMethodId::UpdateRecord => Some(database::stat_by_name_ktf.into_body()),
            WIPICDatabaseMethodId::DeleteRecord => Some(database::delete_record_ktf.into_body()),
            WIPICDatabaseMethodId::ListRecord => Some(database::list_record.into_body()),
            WIPICDatabaseMethodId::MakeDirectory => Some(filesystem::mkdir.into_body()),
            WIPICDatabaseMethodId::GetAccessMode => Some(database::get_access_mode_ktf.into_body()),
            WIPICDatabaseMethodId::GetNumberOfRecords => Some(database::get_number_of_records_ktf.into_body()),
            WIPICDatabaseMethodId::GetRecordSize => Some(database::get_record_size_ktf.into_body()),
            WIPICDatabaseMethodId::Available => Some(database::available_storage_ktf.into_body()),
            WIPICDatabaseMethodId::Unk13 => Some(gen_stub(13, "MC_dbUnk13")),
            WIPICDatabaseMethodId::Unk14 => Some(gen_stub(14, "MC_dbUnk14")),
            WIPICDatabaseMethodId::Unk15 => Some(gen_stub(15, "MC_dbUnk15")),
            WIPICDatabaseMethodId::Exists => Some(database::exists_database_ktf.into_body()),
        },
        WIPICTableId::Interface7 => {
            if function_id < 64 {
                Some(gen_stub(7, "stub"))
            } else {
                None
            }
        }
        WIPICTableId::Uic => get_uic_method_table().into_iter().nth(function_id as usize),
        WIPICTableId::Media => get_media_method_table().into_iter().nth(function_id as usize),
        WIPICTableId::Net => get_net_method_table().into_iter().nth(function_id as usize),
        WIPICTableId::Interface11 => {
            if function_id < 64 {
                Some(gen_stub(11, "stub"))
            } else {
                None
            }
        }
        WIPICTableId::Interface12 => get_unk12_method_table().into_iter().nth(function_id as usize),
        WIPICTableId::Interface13 => {
            if function_id < 64 {
                Some(gen_stub(13, "stub"))
            } else {
                None
            }
        }
        WIPICTableId::Interface14 => {
            if function_id < 64 {
                Some(gen_stub(14, "stub"))
            } else {
                None
            }
        }
        WIPICTableId::Interface15 => {
            if function_id < 64 {
                Some(gen_stub(15, "stub"))
            } else {
                None
            }
        }
        WIPICTableId::Interface16 => {
            if function_id < 64 {
                Some(gen_stub(16, "stub"))
            } else {
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TABLES: [WIPICTableId; 18] = [
        WIPICTableId::Kernel,
        WIPICTableId::Util,
        WIPICTableId::Misc,
        WIPICTableId::Graphics,
        WIPICTableId::Interface3,
        WIPICTableId::Interface4,
        WIPICTableId::Interface5,
        WIPICTableId::Database,
        WIPICTableId::Interface7,
        WIPICTableId::Uic,
        WIPICTableId::Media,
        WIPICTableId::Net,
        WIPICTableId::Interface11,
        WIPICTableId::Interface12,
        WIPICTableId::Interface13,
        WIPICTableId::Interface14,
        WIPICTableId::Interface15,
        WIPICTableId::Interface16,
    ];

    #[test]
    fn every_slot_a_table_hands_out_has_something_behind_it() {
        for table_id in TABLES {
            for function_id in 0..WIPIC_TABLE_FUNCTIONS {
                assert!(
                    get_method_body(table_id, function_id).is_some(),
                    "table {} function {function_id} has no body, so its slot would be a zero to branch to",
                    table_id as u32
                );
            }
        }
    }

    #[test]
    fn the_slots_the_demon_hunter_authenticates_through_are_served() {
        // Net slot 30 is the named connect its authentication calls, 31 the
        // write that carries the request and 32 the read that brings the answer
        // back. A refusal at 30 is a title waiting for a callback it will never
        // get; at 31 it is one asking to send the same 48 bytes until it gives
        // up; at 32 it is one that asked and is never answered.
        assert!(get_served_method_body(WIPICTableId::Net, 30).is_some());
        assert!(get_served_method_body(WIPICTableId::Net, 31).is_some());
        assert!(get_served_method_body(WIPICTableId::Net, 32).is_some());
    }

    #[test]
    fn an_interface_longer_than_a_table_keeps_its_last_function() {
        // The kernel interface has 65 of them, so a table length applied as a
        // cap would take one away that this runtime serves.
        assert!(get_served_method_body(WIPICTableId::Kernel, WIPIC_TABLE_FUNCTIONS).is_some());
        assert!(get_method_body(WIPICTableId::Kernel, WIPIC_TABLE_FUNCTIONS).is_some());
    }

    #[test]
    fn nothing_answers_past_the_end_of_a_table() {
        assert!(get_method_body(WIPICTableId::Net, WIPIC_TABLE_FUNCTIONS).is_none());
    }
}
