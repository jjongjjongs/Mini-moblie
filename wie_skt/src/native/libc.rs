//! The C library table an SKT native title reaches through the fifteenth
//! interface slot.
//!
//! It is the string and memory half of the C library, in the order the
//! handset's `string.h` lists it, then the conversions and the clock. The
//! slots were read off 메이플스토리 해적편's call sites: `strlen` sizes the
//! buffer `strcpy` fills, `memset` is handed `(p, 0, n)` on every allocation,
//! `atoi` reads the `SERIALDSPORT` property back as a number, and `time(NULL)`
//! is handed to `localtime` straight after.

use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};

use wipi_types::wipic::WIPICWord;

use wie_util::{Result, read_null_terminated_string_bytes, write_generic};
use wie_wipi_c::{MethodImpl, WIPICContext, WIPICMethodBody};

/// The body for a C library slot, or `None` for one nothing has shown the
/// meaning of yet.
pub fn body(slot: u32) -> Option<WIPICMethodBody> {
    Some(match slot {
        0 => strcpy.into_body(),
        1 => strncpy.into_body(),
        2 => strcat.into_body(),
        3 => strncat.into_body(),
        4 => strcmp.into_body(),
        5 => strncmp.into_body(),
        6 => strchr.into_body(),
        7 => strrchr.into_body(),
        11 => strstr.into_body(),
        12 => strlen.into_body(),
        14 => memcpy.into_body(),
        15 => memmove.into_body(),
        16 => memcmp.into_body(),
        17 => memchr.into_body(),
        18 => memset.into_body(),
        20 => atoi.into_body(),
        26 => time.into_body(),
        29 => localtime.into_body(),
        _ => return None,
    })
}

fn read_string(context: &dyn WIPICContext, address: WIPICWord) -> Result<Vec<u8>> {
    read_null_terminated_string_bytes(context, address)
}

fn read(context: &dyn WIPICContext, address: WIPICWord, size: WIPICWord) -> Result<Vec<u8>> {
    let mut bytes = alloc::vec![0; size as usize];
    context.read_bytes(address, &mut bytes)?;

    Ok(bytes)
}

fn compare(left: &[u8], right: &[u8]) -> i32 {
    for (a, b) in left.iter().zip(right) {
        if a != b {
            return *a as i32 - *b as i32;
        }
    }

    left.len() as i32 - right.len() as i32
}

async fn strcpy(context: &mut dyn WIPICContext, dst: WIPICWord, src: WIPICWord) -> Result<WIPICWord> {
    let mut bytes = read_string(context, src)?;
    bytes.push(0);
    context.write_bytes(dst, &bytes)?;

    Ok(dst)
}

async fn strncpy(context: &mut dyn WIPICContext, dst: WIPICWord, src: WIPICWord, size: WIPICWord) -> Result<WIPICWord> {
    let mut bytes = read_string(context, src)?;
    // Padded with NULs to the size, and not terminated if the source fills it.
    bytes.resize(size as usize, 0);
    context.write_bytes(dst, &bytes)?;

    Ok(dst)
}

async fn strcat(context: &mut dyn WIPICContext, dst: WIPICWord, src: WIPICWord) -> Result<WIPICWord> {
    let end = dst + read_string(context, dst)?.len() as WIPICWord;
    let mut bytes = read_string(context, src)?;
    bytes.push(0);
    context.write_bytes(end, &bytes)?;

    Ok(dst)
}

async fn strncat(context: &mut dyn WIPICContext, dst: WIPICWord, src: WIPICWord, size: WIPICWord) -> Result<WIPICWord> {
    let end = dst + read_string(context, dst)?.len() as WIPICWord;
    let mut bytes = read_string(context, src)?;
    bytes.truncate(size as usize);
    bytes.push(0);
    context.write_bytes(end, &bytes)?;

    Ok(dst)
}

async fn strcmp(context: &mut dyn WIPICContext, left: WIPICWord, right: WIPICWord) -> Result<i32> {
    Ok(compare(&read_string(context, left)?, &read_string(context, right)?))
}

async fn strncmp(context: &mut dyn WIPICContext, left: WIPICWord, right: WIPICWord, size: WIPICWord) -> Result<i32> {
    let mut left = read_string(context, left)?;
    let mut right = read_string(context, right)?;
    left.truncate(size as usize);
    right.truncate(size as usize);

    Ok(compare(&left, &right))
}

async fn strchr(context: &mut dyn WIPICContext, string: WIPICWord, character: WIPICWord) -> Result<WIPICWord> {
    let character = character as u8;
    let bytes = read_string(context, string)?;

    // The terminator is part of the string as far as `strchr` is concerned.
    if character == 0 {
        return Ok(string + bytes.len() as WIPICWord);
    }

    Ok(bytes.iter().position(|x| *x == character).map(|x| string + x as WIPICWord).unwrap_or(0))
}

async fn strrchr(context: &mut dyn WIPICContext, string: WIPICWord, character: WIPICWord) -> Result<WIPICWord> {
    let character = character as u8;
    let bytes = read_string(context, string)?;

    if character == 0 {
        return Ok(string + bytes.len() as WIPICWord);
    }

    Ok(bytes.iter().rposition(|x| *x == character).map(|x| string + x as WIPICWord).unwrap_or(0))
}

async fn strstr(context: &mut dyn WIPICContext, haystack: WIPICWord, needle: WIPICWord) -> Result<WIPICWord> {
    let haystack_bytes = read_string(context, haystack)?;
    let needle = read_string(context, needle)?;

    if needle.is_empty() {
        return Ok(haystack);
    }

    Ok(haystack_bytes
        .windows(needle.len())
        .position(|x| x == needle.as_slice())
        .map(|x| haystack + x as WIPICWord)
        .unwrap_or(0))
}

async fn strlen(context: &mut dyn WIPICContext, string: WIPICWord) -> Result<WIPICWord> {
    Ok(read_string(context, string)?.len() as WIPICWord)
}

async fn memcpy(context: &mut dyn WIPICContext, dst: WIPICWord, src: WIPICWord, size: WIPICWord) -> Result<WIPICWord> {
    let bytes = read(context, src, size)?;
    context.write_bytes(dst, &bytes)?;

    Ok(dst)
}

/// The whole source is read before anything is written, so an overlap copies
/// what was there - `memcpy` above is the same thing for the same reason.
async fn memmove(context: &mut dyn WIPICContext, dst: WIPICWord, src: WIPICWord, size: WIPICWord) -> Result<WIPICWord> {
    memcpy(context, dst, src, size).await
}

async fn memcmp(context: &mut dyn WIPICContext, left: WIPICWord, right: WIPICWord, size: WIPICWord) -> Result<i32> {
    let left = read(context, left, size)?;
    let right = read(context, right, size)?;

    Ok(compare(&left, &right))
}

async fn memchr(context: &mut dyn WIPICContext, memory: WIPICWord, character: WIPICWord, size: WIPICWord) -> Result<WIPICWord> {
    let bytes = read(context, memory, size)?;

    Ok(bytes
        .iter()
        .position(|x| *x == character as u8)
        .map(|x| memory + x as WIPICWord)
        .unwrap_or(0))
}

async fn memset(context: &mut dyn WIPICContext, dst: WIPICWord, value: WIPICWord, size: WIPICWord) -> Result<WIPICWord> {
    context.write_bytes(dst, &alloc::vec![value as u8; size as usize])?;

    Ok(dst)
}

async fn atoi(context: &mut dyn WIPICContext, string: WIPICWord) -> Result<i32> {
    Ok(c_atoi(&read_string(context, string)?))
}

fn c_atoi(bytes: &[u8]) -> i32 {
    let mut digits = bytes.iter().skip_while(|x| x.is_ascii_whitespace()).peekable();
    let negative = match digits.peek() {
        Some(b'-') => {
            digits.next();
            true
        }
        Some(b'+') => {
            digits.next();
            false
        }
        _ => false,
    };

    let mut value: i32 = 0;
    for digit in digits.take_while(|x| x.is_ascii_digit()) {
        value = value.wrapping_mul(10).wrapping_add((digit - b'0') as i32);
    }

    if negative { value.wrapping_neg() } else { value }
}

/// The handset's clock is Korean time, and a title's `localtime` reads it as
/// such: the day a title rolls over its daily rewards is the Korean one.
const UTC_OFFSET_SECONDS: i64 = 9 * 60 * 60;

fn now_seconds(context: &mut dyn WIPICContext) -> i64 {
    (context.system().platform().now().raw() / 1000) as i64
}

async fn time(context: &mut dyn WIPICContext, out: WIPICWord) -> Result<WIPICWord> {
    let now = now_seconds(context) as WIPICWord;
    if out != 0 {
        write_generic(context, out, now)?;
    }

    Ok(now)
}

/// The `struct tm` `localtime` answers with - one, kept for the run, as the C
/// library's own static one is.
static TM: AtomicU32 = AtomicU32::new(0);

async fn localtime(context: &mut dyn WIPICContext, time: WIPICWord) -> Result<WIPICWord> {
    let seconds = if time == 0 {
        now_seconds(context)
    } else {
        wie_util::read_generic::<u32, _>(context, time)? as i64
    };

    let mut tm = TM.load(Ordering::Relaxed);
    if tm == 0 {
        tm = context.alloc_raw(9 * 4)?;
        TM.store(tm, Ordering::Relaxed);
    }

    for (index, field) in broken_down(seconds + UTC_OFFSET_SECONDS).iter().enumerate() {
        write_generic(context, tm + index as WIPICWord * 4, *field)?;
    }

    Ok(tm)
}

/// `struct tm`'s nine fields for a count of seconds since 1970: seconds,
/// minutes, hours, day of month, month from 0, years since 1900, day of week
/// from Sunday, day of year from 0, and no daylight saving.
fn broken_down(seconds: i64) -> [i32; 9] {
    let days = seconds.div_euclid(86400);
    let second_of_day = seconds.rem_euclid(86400);

    // Howard Hinnant's days-to-civil.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era = (day_of_era - day_of_era / 1460 + day_of_era / 36524 - day_of_era / 146_096) / 365;
    let day_of_year_march = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_march = (5 * day_of_year_march + 2) / 153;
    let day = day_of_year_march - (153 * month_march + 2) / 5 + 1;
    let month = if month_march < 10 { month_march + 3 } else { month_march - 9 };
    let year = year_of_era + era * 400 + if month <= 2 { 1 } else { 0 };

    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    const DAYS_BEFORE_MONTH: [i64; 12] = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];
    let day_of_year = DAYS_BEFORE_MONTH[(month - 1) as usize] + day - 1 + if leap && month > 2 { 1 } else { 0 };

    [
        (second_of_day % 60) as i32,
        (second_of_day / 60 % 60) as i32,
        (second_of_day / 3600) as i32,
        day as i32,
        (month - 1) as i32,
        (year - 1900) as i32,
        (days + 4).rem_euclid(7) as i32,
        day_of_year as i32,
        0,
    ]
}

#[cfg(test)]
mod tests {
    use super::{broken_down, c_atoi};

    #[test]
    fn atoi_reads_a_leading_number() {
        assert_eq!(c_atoi(b"115200"), 115200);
        assert_eq!(c_atoi(b"  -12abc"), -12);
        assert_eq!(c_atoi(b"port"), 0);
    }

    #[test]
    fn a_day_breaks_down_into_its_fields() {
        // 2026-10-18 07:34:30 UTC, a Sunday, the 291st day of the year.
        assert_eq!(broken_down(1_792_308_870), [30, 34, 7, 18, 9, 126, 0, 290, 0]);
        // 2024-02-29, a leap day, a Thursday.
        assert_eq!(broken_down(1_709_164_800), [0, 0, 0, 29, 1, 124, 4, 59, 0]);
    }
}
