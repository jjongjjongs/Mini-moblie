//! The object tables an older SDK's titles reach past the C library: a
//! growable vector, a boxed integer, and the call that deletes either.
//!
//! 메이플스토리 도적편 is built against them; the later titles keep their own
//! lists. Each slot was read off what 도적편 does with it: it makes a vector
//! with `new()` or `new(capacity)`, fills it with `addElement`, walks it with
//! `size` and `elementAt`, takes things out with `removeElementAt` and
//! `removeElement`, cuts it back with `setSize` after deleting what lies past
//! the cut, and empties it with `removeAllElements` before handing it to the
//! delete call. The integer is made from a number, read back with
//! `intValue`, and parsed out of a string with `parseInt`.
//!
//! Both live in the title's own heap - a vector is `[tag, length, capacity,
//! elements]` and an integer `[tag, value]` - so what the title holds is an
//! ordinary pointer.

use wipi_types::wipic::WIPICWord;

use wie_util::{Result, WieError, read_generic, read_null_terminated_string_bytes, write_generic};
use wie_wipi_c::{MethodImpl, WIPICContext, WIPICMethodBody};

use super::libc::c_atoi;

const VECTOR_TAG: WIPICWord = u32::from_be_bytes(*b"VECT");
const INTEGER_TAG: WIPICWord = u32::from_be_bytes(*b"INTG");

const VECTOR_SIZE: WIPICWord = 16;
const INTEGER_SIZE: WIPICWord = 8;

/// What a vector made without a capacity starts with.
const DEFAULT_CAPACITY: WIPICWord = 10;

/// Table 15: six calls that take nothing, made as the title starts, pauses
/// and resumes. Nothing it does afterwards reads the answers, so they are
/// answered with nothing to report.
pub fn lifecycle_body(slot: u32) -> Option<WIPICMethodBody> {
    Some(match slot {
        0..=5 => nothing.into_body(),
        _ => return None,
    })
}

/// Table 37: the boxed integer.
pub fn integer_body(slot: u32) -> Option<WIPICMethodBody> {
    Some(match slot {
        0 => integer_new.into_body(),
        4 => integer_value.into_body(),
        8 => parse_int.into_body(),
        _ => return None,
    })
}

/// Table 45: slot 9 deletes an object the other tables made.
pub fn object_body(slot: u32) -> Option<WIPICMethodBody> {
    Some(match slot {
        9 => delete.into_body(),
        _ => return None,
    })
}

/// Table 59: the vector.
pub fn vector_body(slot: u32) -> Option<WIPICMethodBody> {
    Some(match slot {
        1 => vector_with_capacity.into_body(),
        2 => vector_new.into_body(),
        7 => element_at.into_body(),
        11 => remove_element_at.into_body(),
        13 => add_element.into_body(),
        14 => remove_element.into_body(),
        15 => remove_all_elements.into_body(),
        18 => set_size.into_body(),
        20 => size.into_body(),
        _ => return None,
    })
}

async fn nothing(_context: &mut dyn WIPICContext) -> Result<WIPICWord> {
    Ok(0)
}

async fn integer_new(context: &mut dyn WIPICContext, value: WIPICWord) -> Result<WIPICWord> {
    let integer = context.alloc_raw(INTEGER_SIZE)?;
    write_generic(context, integer, INTEGER_TAG)?;
    write_generic(context, integer + 4, value)?;
    tracing::debug!("SKT Integer({value}) -> {integer:#x}");

    Ok(integer)
}

async fn integer_value(context: &mut dyn WIPICContext, integer: WIPICWord) -> Result<WIPICWord> {
    if integer == 0 {
        return Ok(0);
    }

    read_generic(context, integer + 4)
}

async fn parse_int(context: &mut dyn WIPICContext, string: WIPICWord) -> Result<i32> {
    Ok(c_atoi(&read_null_terminated_string_bytes(context, string)?))
}

async fn delete(context: &mut dyn WIPICContext, object: WIPICWord) -> Result<()> {
    tracing::debug!("SKT delete({object:#x})");
    if object == 0 {
        return Ok(());
    }

    match read_generic::<WIPICWord, _>(context, object)? {
        VECTOR_TAG => {
            let capacity: WIPICWord = read_generic(context, object + 8)?;
            let elements: WIPICWord = read_generic(context, object + 12)?;
            context.free_raw(elements, capacity * 4)?;
            context.free_raw(object, VECTOR_SIZE)
        }
        INTEGER_TAG => context.free_raw(object, INTEGER_SIZE),
        tag => {
            tracing::warn!("SKT delete({object:#x}): not an object the platform made (tag {tag:#x})");
            Ok(())
        }
    }
}

async fn vector_new(context: &mut dyn WIPICContext) -> Result<WIPICWord> {
    make_vector(context, DEFAULT_CAPACITY)
}

async fn vector_with_capacity(context: &mut dyn WIPICContext, capacity: WIPICWord) -> Result<WIPICWord> {
    make_vector(context, capacity.max(1))
}

fn make_vector(context: &mut dyn WIPICContext, capacity: WIPICWord) -> Result<WIPICWord> {
    let elements = context.alloc_raw(capacity * 4)?;
    let vector = context.alloc_raw(VECTOR_SIZE)?;
    write_generic(context, vector, VECTOR_TAG)?;
    write_generic(context, vector + 4, 0u32)?;
    write_generic(context, vector + 8, capacity)?;
    write_generic(context, vector + 12, elements)?;
    tracing::debug!("SKT Vector({capacity}) -> {vector:#x}");

    Ok(vector)
}

/// The vector's elements, read out whole.
fn elements(context: &dyn WIPICContext, vector: WIPICWord) -> Result<alloc::vec::Vec<WIPICWord>> {
    if read_generic::<WIPICWord, _>(context, vector)? != VECTOR_TAG {
        return Err(WieError::FatalError(alloc::format!("{vector:#x} is not a vector")));
    }

    let length: WIPICWord = read_generic(context, vector + 4)?;
    let address: WIPICWord = read_generic(context, vector + 12)?;

    (0..length).map(|index| read_generic(context, address + index * 4)).collect()
}

/// Writes the elements back, growing the storage when they no longer fit.
fn store(context: &mut dyn WIPICContext, vector: WIPICWord, values: &[WIPICWord]) -> Result<()> {
    let mut capacity: WIPICWord = read_generic(context, vector + 8)?;
    let mut address: WIPICWord = read_generic(context, vector + 12)?;

    if values.len() as WIPICWord > capacity {
        let grown = (capacity * 2).max(values.len() as WIPICWord);
        let new_address = context.alloc_raw(grown * 4)?;
        context.free_raw(address, capacity * 4)?;
        capacity = grown;
        address = new_address;
        write_generic(context, vector + 8, capacity)?;
        write_generic(context, vector + 12, address)?;
    }

    for (index, value) in values.iter().enumerate() {
        write_generic(context, address + index as WIPICWord * 4, *value)?;
    }
    write_generic(context, vector + 4, values.len() as WIPICWord)
}

async fn size(context: &mut dyn WIPICContext, vector: WIPICWord) -> Result<WIPICWord> {
    Ok(elements(context, vector)?.len() as WIPICWord)
}

async fn element_at(context: &mut dyn WIPICContext, vector: WIPICWord, index: WIPICWord) -> Result<WIPICWord> {
    Ok(elements(context, vector)?.get(index as usize).copied().unwrap_or(0))
}

async fn add_element(context: &mut dyn WIPICContext, vector: WIPICWord, value: WIPICWord) -> Result<()> {
    let mut values = elements(context, vector)?;
    values.push(value);

    store(context, vector, &values)
}

async fn remove_element_at(context: &mut dyn WIPICContext, vector: WIPICWord, index: WIPICWord) -> Result<()> {
    let mut values = elements(context, vector)?;
    if (index as usize) < values.len() {
        values.remove(index as usize);
    }

    store(context, vector, &values)
}

async fn remove_element(context: &mut dyn WIPICContext, vector: WIPICWord, value: WIPICWord) -> Result<i32> {
    let mut values = elements(context, vector)?;
    let Some(index) = values.iter().position(|x| *x == value) else {
        return Ok(0);
    };
    values.remove(index);
    store(context, vector, &values)?;

    Ok(1)
}

async fn remove_all_elements(context: &mut dyn WIPICContext, vector: WIPICWord) -> Result<()> {
    store(context, vector, &[])
}

async fn set_size(context: &mut dyn WIPICContext, vector: WIPICWord, length: WIPICWord) -> Result<()> {
    let mut values = elements(context, vector)?;
    values.resize(length as usize, 0);

    store(context, vector, &values)
}
