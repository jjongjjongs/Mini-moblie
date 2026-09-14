use alloc::{boxed::Box, vec};

use jvm::Jvm;
use wie_backend::System;
use wie_core_arm::{ArmCore, EmulatedFunction, EmulatedFunctionParam, ResultWriter, SvcId};
use wie_util::{Result, WieError};
use wie_wipi_c::{
    WIPICMethodBody, WIPICResult,
    api::{filesystem, im, kernel, net, serial, shared_buf},
};

use crate::runtime::SVC_CATEGORY_WIPIC;
use crate::runtime::svc_ids::{WIPICKernelMethodId, WIPICTableId};

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
    )
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

    tracing::warn!(
        "unserved WIPIC table {} function {function_id}({:#x}, {:#x}, {:#x}, {:#x}){}",
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
