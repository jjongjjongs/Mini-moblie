use wie_util::{Result, read_null_terminated_string_bytes};

use wipi_types::wipic::WIPICWord;

use crate::context::WIPICContext;

/// `MC_utilHtonl` - host to network byte order, 32 bit. The emulated code always
/// runs little endian, so this is a byte swap to big endian.
pub async fn htonl(_context: &mut dyn WIPICContext, val: WIPICWord) -> Result<WIPICWord> {
    tracing::debug!("MC_utilHtonl({val:#x})");

    Ok(val.to_be())
}

/// `MC_utilHtons` (0x385) @ native 0x1b9ff8 - host to network byte order, 16 bit.
///
/// Native narrows its argument to sixteen bits, swaps them through
/// `dnetwork_htons` @ 0x1206d0, and then **sign-extends** the result back to
/// thirty-two: `lsl r0, r0, #0x10` / `asr r0, r0, #0x10`. It returns a `short`,
/// not an unsigned one, so any answer with bit fifteen set comes back negative.
///
/// That is not a detail. A title comparing a frame's `0xffff` marker - read out
/// of the frame as a `short`, so already `0xffffffff` - against
/// `MC_utilHtons(-1)` gets equality from native and inequality from a
/// zero-extended answer, and a marker check that cannot pass is a reply the
/// title throws away. 붉은보석 does exactly this on the answer to its billing
/// request.
pub async fn htons(_context: &mut dyn WIPICContext, val: WIPICWord) -> Result<WIPICWord> {
    tracing::debug!("MC_utilHtons({val:#x})");

    Ok(sign_extend_16((val as u16).to_be()))
}

/// `MC_utilNtohl` - network to host byte order, 32 bit. Symmetric with `htonl`
/// on a little endian host.
pub async fn ntohl(_context: &mut dyn WIPICContext, val: WIPICWord) -> Result<WIPICWord> {
    tracing::debug!("MC_utilNtohl({val:#x})");

    Ok(u32::from_be(val))
}

/// `MC_utilNtohs` (0x387) @ native 0x1b9fd0 - network to host byte order, 16 bit.
///
/// The mirror of [`htons`], sign extension included: native wraps
/// `dnetwork_ntohs` @ 0x12070c in the same `lsl`/`asr` pair.
pub async fn ntohs(_context: &mut dyn WIPICContext, val: WIPICWord) -> Result<WIPICWord> {
    tracing::debug!("MC_utilNtohs({val:#x})");

    Ok(sign_extend_16(u16::from_be(val as u16)))
}

/// A sixteen-bit result widened the way native's `lsl #16` / `asr #16` widens
/// it - as a signed short, so `0xffff` comes back as `0xffffffff`.
fn sign_extend_16(value: u16) -> WIPICWord {
    value as i16 as i32 as WIPICWord
}

/// `MC_utilInetAddrInt` - parse a dotted IPv4 address into the native
/// little-endian integer representation used by the LGT WIPI runtime.
///
/// This deliberately follows the native parser rather than a strict IPv4
/// parser: exactly three dots are required, empty components are accepted,
/// and each component accumulates in an 8-bit byte (wrapping modulo 256).
/// A null pointer or any non-digit/non-dot character returns 0xffffffff.
pub async fn inet_addr_int(context: &mut dyn WIPICContext, address: WIPICWord) -> Result<WIPICWord> {
    tracing::debug!("MC_utilInetAddrInt({address:#x})");

    if address == 0 {
        return Ok(u32::MAX as _);
    }

    let input = read_null_terminated_string_bytes(context, address)?;
    let mut octets = [0u8; 4];
    let mut index = 0usize;

    for byte in input {
        match byte {
            b'0'..=b'9' => {
                octets[index] = octets[index].wrapping_mul(10).wrapping_add(byte - b'0');
            }
            b'.' => {
                index += 1;
                if index > 3 {
                    return Ok(u32::MAX as _);
                }
            }
            _ => return Ok(u32::MAX as _),
        }
    }

    if index != 3 {
        return Ok(u32::MAX as _);
    }

    Ok(u32::from_le_bytes(octets) as _)
}

#[cfg(test)]
mod tests {
    use alloc::boxed::Box;

    use test_utils::TestPlatform;
    use wie_backend::{DefaultTaskRunner, System};
    use wie_util::ByteWrite;

    use crate::context::test::TestContext;

    use super::{htonl, htons, inet_addr_int, ntohl, ntohs};

    #[futures_test::test]
    async fn lgt_inet_addr_int_matches_native_byte_order() {
        let system = System::new(Box::new(TestPlatform::new()), "test-pid", "test-aid", DefaultTaskRunner);
        let mut context = TestContext::with_system(system);

        context.write_bytes(0x1000, b"1.2.3.4\0").unwrap();
        assert_eq!(inet_addr_int(&mut context, 0x1000).await.unwrap(), 0x0403_0201);
    }

    #[futures_test::test]
    async fn lgt_inet_addr_int_wraps_each_component_to_u8() {
        let system = System::new(Box::new(TestPlatform::new()), "test-pid", "test-aid", DefaultTaskRunner);
        let mut context = TestContext::with_system(system);

        context.write_bytes(0x1000, b"256.511.258.257\0").unwrap();
        assert_eq!(inet_addr_int(&mut context, 0x1000).await.unwrap(), 0x0102_ff00);
    }

    #[futures_test::test]
    async fn lgt_inet_addr_int_accepts_empty_components_like_native() {
        let system = System::new(Box::new(TestPlatform::new()), "test-pid", "test-aid", DefaultTaskRunner);
        let mut context = TestContext::with_system(system);

        context.write_bytes(0x1000, b".1.2.\0").unwrap();
        assert_eq!(inet_addr_int(&mut context, 0x1000).await.unwrap(), 0x0002_0100);
    }

    #[futures_test::test]
    async fn lgt_inet_addr_int_rejects_null_bad_characters_and_wrong_dot_count() {
        let system = System::new(Box::new(TestPlatform::new()), "test-pid", "test-aid", DefaultTaskRunner);
        let mut context = TestContext::with_system(system);

        assert_eq!(inet_addr_int(&mut context, 0).await.unwrap(), u32::MAX);

        context.write_bytes(0x1000, b"1.2.3\0").unwrap();
        assert_eq!(inet_addr_int(&mut context, 0x1000).await.unwrap(), u32::MAX);

        context.write_bytes(0x1100, b"1.2.3.4.5\0").unwrap();
        assert_eq!(inet_addr_int(&mut context, 0x1100).await.unwrap(), u32::MAX);

        context.write_bytes(0x1200, b"1.2.x.4\0").unwrap();
        assert_eq!(inet_addr_int(&mut context, 0x1200).await.unwrap(), u32::MAX);
    }

    #[futures_test::test]
    async fn a_sixteen_bit_swap_comes_back_signed_the_way_native_widens_it() {
        let mut context = TestContext::new();

        // Native `lsl #16` / `asr #16`: bit fifteen set means a negative answer.
        // A title checking a frame's 0xffff marker compares against this.
        assert_eq!(htons(&mut context, 0xffff).await.unwrap(), 0xffff_ffff);
        assert_eq!(htons(&mut context, 0xffff_ffff).await.unwrap(), 0xffff_ffff);
        assert_eq!(ntohs(&mut context, 0xffff).await.unwrap(), 0xffff_ffff);

        // 0x0100 swaps to 0x0001, which is positive and widens unchanged.
        assert_eq!(htons(&mut context, 0x0100).await.unwrap(), 0x0001);
        assert_eq!(ntohs(&mut context, 0x0100).await.unwrap(), 0x0001);

        // 0x0007 swaps to 0x0700 - still positive, still unchanged.
        assert_eq!(htons(&mut context, 7).await.unwrap(), 0x0700);

        // 0x0080 swaps to 0x8000, which is not.
        assert_eq!(htons(&mut context, 0x0080).await.unwrap(), 0xffff_8000);
    }

    #[futures_test::test]
    async fn a_thirty_two_bit_swap_is_a_plain_one() {
        let mut context = TestContext::new();

        // Native `MC_utilHtonl`/`MC_utilNtohl` tail-call the swap with nothing
        // around it - there is no narrower type to widen from.
        assert_eq!(htonl(&mut context, 0x1234_5678).await.unwrap(), 0x7856_3412);
        assert_eq!(ntohl(&mut context, 0x1234_5678).await.unwrap(), 0x7856_3412);
        assert_eq!(htonl(&mut context, 0xffff_ffff).await.unwrap(), 0xffff_ffff);
    }
}
