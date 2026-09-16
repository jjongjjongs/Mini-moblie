use core::mem::{size_of, size_of_val};

use bytemuck::Pod;

use wipi_types::ktf::wipic::WIPICInterface;

use wie_core_arm::{Allocator, ArmCore};
use wie_util::{Result, write_generic};
use wie_wipi_c::WIPICContext;

use crate::runtime::wipi_c::method_table::{self, WIPIC_TABLE_FUNCTIONS, get_database_interface, get_graphics_interface};

use crate::runtime::svc_ids::WIPICTableId;

/// How many slots a table holds in guest memory.
///
/// [`WIPIC_TABLE_FUNCTIONS`], or the interface's own length where that is
/// longer - the kernel interface is, and shortening it would take away
/// functions this runtime serves.
fn table_slots(interface_size: usize) -> u16 {
    WIPIC_TABLE_FUNCTIONS.max((interface_size / 4) as u16)
}

/// Fills a table's slots from `first` to `slots` with their SVC stubs.
fn write_stubs(core: &mut ArmCore, context: &mut dyn WIPICContext, table_id: WIPICTableId, address: u32, first: u16, slots: u16) -> Result<()> {
    for index in first..slots {
        let stub = core.make_svc_stub(crate::runtime::SVC_CATEGORY_WIPIC, table_id.function_id(index))?;

        write_generic(context, address + index as u32 * 4, stub)?;
    }

    Ok(())
}

/// Writes one WIPI C interface table: [`table_slots`] function pointers,
/// whatever the runtime has a body for.
///
/// The length is the table's, not ours. A guest reaches a function by indexing
/// this array, so a table cut short at the last function we serve is one a title
/// can index past - it reads whatever word follows and branches to it. Every
/// slot therefore gets a stub, and `get_method_body` decides what the slot
/// answers.
fn write_methods(core: &mut ArmCore, context: &mut dyn WIPICContext, table_id: WIPICTableId) -> Result<u32> {
    let slots = table_slots(0);
    let address = context.alloc_raw(slots as u32 * 4)?;

    write_stubs(core, context, table_id, address, 0, slots)?;

    Ok(address)
}

pub fn get_wipic_knl_interface(core: &mut ArmCore) -> Result<u32> {
    let kernel_interface = method_table::get_kernel_interface(core)?;

    // A full table's worth of room, for the reason [`write_methods`] gives.
    let slots = table_slots(size_of_val(&kernel_interface));
    let address = Allocator::alloc(core, slots as u32 * 4)?;
    write_generic(core, address, kernel_interface)?;

    for index in (size_of_val(&kernel_interface) / 4) as u16..slots {
        let stub = core.make_svc_stub(crate::runtime::SVC_CATEGORY_WIPIC, WIPICTableId::Kernel.function_id(index))?;

        write_generic(core, address + index as u32 * 4, stub)?;
    }

    Ok(address)
}

/// Writes an interface whose functions are a named struct rather than a list.
///
/// The struct is still an array of function pointers as far as the guest is
/// concerned, so it gets a full table's worth of room and its own fields are
/// followed by stubs for the rest - see [`write_methods`].
fn write_interface<T: Pod>(core: &mut ArmCore, context: &mut dyn WIPICContext, table_id: WIPICTableId, interface: T) -> Result<u32> {
    let slots = table_slots(size_of_val(&interface));
    let address = context.alloc_raw(slots as u32 * 4)?;
    write_generic(context, address, interface)?;

    write_stubs(core, context, table_id, address, (size_of_val(&interface) / 4) as u16, slots)?;

    Ok(address)
}

pub async fn get_wipic_interfaces(core: &mut ArmCore, context: &mut dyn WIPICContext) -> Result<u32> {
    tracing::trace!("get_wipic_interfaces");

    let graphics_interface = get_graphics_interface(core)?;
    let database_interface = get_database_interface(core)?;

    let util_interface = write_methods(core, context, WIPICTableId::Util)?;
    let misc_interface = write_methods(core, context, WIPICTableId::Misc)?;
    let graphics_interface = write_interface(core, context, WIPICTableId::Graphics, graphics_interface)?;
    let interface_3 = write_methods(core, context, WIPICTableId::Interface3)?;
    let interface_4 = write_methods(core, context, WIPICTableId::Interface4)?;
    let interface_5 = write_methods(core, context, WIPICTableId::Interface5)?;
    let database_interface = write_interface(core, context, WIPICTableId::Database, database_interface)?;
    let interface_7 = write_methods(core, context, WIPICTableId::Interface7)?;
    let uic_interface = write_methods(core, context, WIPICTableId::Uic)?;
    let media_interface = write_methods(core, context, WIPICTableId::Media)?;
    let net_interface = write_methods(core, context, WIPICTableId::Net)?;
    let interface_11 = write_methods(core, context, WIPICTableId::Interface11)?;
    let interface_12 = write_methods(core, context, WIPICTableId::Interface12)?;
    let interface_13 = write_methods(core, context, WIPICTableId::Interface13)?;
    let interface_14 = write_methods(core, context, WIPICTableId::Interface14)?;
    let interface_15 = write_methods(core, context, WIPICTableId::Interface15)?;
    let interface_16 = write_methods(core, context, WIPICTableId::Interface16)?;

    let interface = WIPICInterface {
        util_interface,
        misc_interface,
        graphics_interface,
        interface_3,
        interface_4,
        interface_5,
        database_interface,
        interface_7,
        uic_interface,
        media_interface,
        net_interface,
        interface_11,
        interface_12,
        interface_13,
        interface_14,
        interface_15,
        interface_16,
    };

    let address = context.alloc_raw(size_of::<WIPICInterface>() as u32)?;

    write_generic(context, address, interface)?;

    Ok(address)
}

#[cfg(test)]
mod tests {
    use super::{WIPIC_TABLE_FUNCTIONS, table_slots};

    #[test]
    fn a_short_interface_still_gets_a_whole_table() {
        assert_eq!(table_slots(0), WIPIC_TABLE_FUNCTIONS);
        assert_eq!(table_slots(30 * 4), WIPIC_TABLE_FUNCTIONS);
    }

    #[test]
    fn a_long_interface_is_never_cut_down_to_one() {
        assert_eq!(table_slots(65 * 4), 65);
    }
}
