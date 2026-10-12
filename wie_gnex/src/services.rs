//! The services a script calls by number (`0x92 n`). Each was identified from
//! how the scripts use it - its arguments, what it leaves on the stack, what
//! follows - and, where it is the same service as in the earlier SGS
//! versions, given that version's contract. A number not listed here stops
//! the callback rather than being guessed at.

use alloc::{format, string::String, vec::Vec};

use crate::{
    host::{Host, Sound},
    vm::{Vm, VmResult},
};

/// The handset's phone number, as the script reads it.
const PHONE_NUMBER: &[u8] = b"01012345678";

pub fn call(service: u16, vm: &mut Vm, host: &mut Host) -> VmResult<()> {
    match service {
        // Handset facts: device classes, phone number, user id, date, time.
        0x00 => {
            let address = vm.pop()?;
            for (i, value) in [8, 3, 65536, 1].into_iter().enumerate() {
                vm.write(address + i as i32, value)?;
            }
        }
        0x01 => {
            let resource = vm.pop()?;
            vm.set_string(resource, PHONE_NUMBER)?;
        }
        0x02 => {
            let resource = vm.pop()?;
            vm.set_string(resource, b"gnex")?;
        }
        0x04 => {
            let address = vm.pop()?;
            for (i, value) in [2006, 1, 1, 0].into_iter().enumerate() {
                vm.write(address + i as i32, value)?;
            }
        }
        0x05 => {
            // Read off the guest clock, so a title timing a scene by it sees
            // the same time its timers do.
            let address = vm.pop()?;
            let ms = host.clock + 12 * 3_600_000;
            let fields = [
                (ms / 3_600_000 % 24) as i32,
                (ms / 60_000 % 60) as i32,
                (ms / 1000 % 60) as i32,
                (ms % 1000) as i32,
            ];
            for (i, value) in fields.into_iter().enumerate() {
                vm.write(address + i as i32, value)?;
            }
        }

        // Drawing.
        0x22 => {
            // Clip to x, y, width, height.
            let [x, y, w, h] = vm.args::<4>()?;
            let g = &mut host.graphics;
            g.clip = (x.max(0), y.max(0), (x + w).min(g.width), (y + h).min(g.height));
        }
        0x25 => {
            // A small setting (1..4) changed around scene changes; its effect
            // is not known.
            vm.pop()?;
        }
        0x26 => host.graphics.color = vm.pop()?.rem_euclid(182),
        0x27 => {
            let [r, g, b] = vm.args::<3>()?;
            let rgb = (((r & 255) * 7 / 255) << 5) | (((g & 255) * 7 / 255) << 2) | ((b & 255) * 3 / 255);
            host.graphics.color = crate::graphics::RAW_COLOR + rgb;
        }
        0x28 => {
            // Text style, foreground, background and alignment.
            let [_style, fg, bg, align] = vm.args::<4>()?;
            let g = &mut host.graphics;
            g.text_fg = fg.rem_euclid(182);
            g.text_bg = bg.rem_euclid(182);
            g.text_align = align.rem_euclid(3);
        }
        0x2a => {
            let [fg, bg] = vm.args::<2>()?;
            host.graphics.text_fg = fg.rem_euclid(182);
            host.graphics.text_bg = bg.rem_euclid(182);
        }
        0x2b => host.graphics.text_align = vm.pop()?.rem_euclid(3),
        0x2d => {
            // Pack a palette: a type word then its colours, into the bytes a
            // bitmap's palette override reads.
            let [destination, source] = vm.args::<2>()?;
            let kind = vm.read(source)?;
            if !(2..=7).contains(&kind) {
                return Err(format!("palette type {kind}"));
            }
            let count = [2, 4, 16][((kind - 2) % 3) as usize];
            let words = (0..count).map(|i| vm.read(source + 1 + i)).collect::<VmResult<Vec<_>>>()?;
            let packed = if kind <= 4 {
                words.chunks(2).map(|w| ((w[0] & 255) << 4 | (w[1] & 15)) as u8).collect()
            } else {
                words.iter().map(|&w| w as u8).collect()
            };
            host.palettes.insert(destination, packed);
        }
        0x36 => host.graphics.bank = vm.pop()?.clamp(0, 6),
        0x3a => host.graphics.fill(255),
        0x3b => host.graphics.fill(0),
        0x42 => {
            let [x1, y1, x2, y2] = vm.args::<4>()?;
            let c = host.graphics.color;
            host.graphics.line(x1, y1, x2, y2, c);
        }
        0x43 => {
            let [x1, x2, y] = vm.args::<3>()?;
            let c = host.graphics.color;
            host.graphics.line(x1, y, x2, y, c);
        }
        0x44 => {
            let [x, y1, y2] = vm.args::<3>()?;
            let c = host.graphics.color;
            host.graphics.line(x, y1, x, y2, c);
        }
        0x45 => {
            let [x1, y1, x2, y2] = vm.args::<4>()?;
            let c = host.graphics.color;
            host.graphics.rect(x1, y1, x2, y2, c);
        }
        0x46 => {
            let [x1, y1, x2, y2] = vm.args::<4>()?;
            let c = host.graphics.color;
            host.graphics.fill_rect(x1, y1, x2, y2, c);
        }
        // Rounded boxes: filled, and outlined.
        0x48 => {
            let [x1, y1, x2, y2, radius] = vm.args::<5>()?;
            host.graphics.round_rect(x1, y1, x2, y2, radius, true);
        }
        0x47 | 0x49 => {
            let [x1, y1, x2, y2, radius] = vm.args::<5>()?;
            host.graphics.round_rect(x1, y1, x2, y2, radius, false);
        }
        0x51 => {
            let g = &mut host.graphics;
            g.backup.copy_from_slice(&g.pixels);
        }
        0x52 => {
            let g = &mut host.graphics;
            g.pixels.copy_from_slice(&g.backup);
        }
        0x54 => {
            let [buffer, dx, dy, mode] = vm.args::<4>()?;
            host.graphics.scroll(buffer, dx, dy, mode);
        }
        0x55 => {
            let [x, y, resource] = vm.args::<3>()?;
            let bytes = vm.string(resource)?;
            let (text, _, _) = encoding_rs::EUC_KR.decode(&bytes);
            host.graphics.text(x, y, &text, false);
        }
        0x5f => host.graphics.present(),

        // Sound: a SMAF resource played once, and stopped.
        0x82 => {
            let resource = vm.pop()?;
            host.sounds.push(Sound::Play(resource));
        }
        0x83 => host.sounds.push(Sound::Stop),
        0x8f => {}

        // Device switches taking one value, whose effect is not modelled.
        0x8e | 0x90 | 0x91 => {
            vm.pop()?;
        }

        // Saved words.
        0x93 => {
            let [address, count] = vm.args::<2>()?;
            for i in 0..count {
                let value = host.saved.get(i as usize).copied().unwrap_or(0);
                vm.write(address + i, value)?;
            }
        }
        0x94 => {
            let [address, count] = vm.args::<2>()?;
            if host.saved.len() < count as usize {
                host.saved.resize(count as usize, 0);
            }
            for i in 0..count {
                host.saved[i as usize] = vm.read(address + i)?;
            }
        }

        // Timers 0..2: start (period, repeat) and stop.
        0x95..=0x97 => {
            let [period, repeat] = vm.args::<2>()?;
            host.start_timer((service - 0x95) as usize, period, repeat != 0);
        }
        0x98..=0x9a => host.timers[(service - 0x98) as usize] = None,

        // Resources as byte strings.
        0x9d => {
            let resource = vm.pop()?;
            let size = vm.resource(resource)?.len() as i32;
            vm.push(size);
        }
        0x9e => {
            let [resource, size] = vm.args::<2>()?;
            let data = vm.resource_mut(resource)?;
            if size >= 0 {
                data.resize(size as usize, 0);
            }
            vm.push((size >= 0) as i32);
        }
        0x9f => {
            let [resource, size] = vm.args::<2>()?;
            let data = vm.resource_mut(resource)?;
            data.clear();
            data.resize(size.max(0) as usize, 0);
        }
        0xa0 => {
            let resource = vm.pop()?;
            let length = vm.string(resource)?.len() as i32;
            vm.push(length);
        }
        0xa1 => {
            let [destination, source] = vm.args::<2>()?;
            let value = vm.string(source)?;
            vm.set_string(destination, &value)?;
        }
        0xa2 => {
            let [destination, source, offset, length] = vm.args::<4>()?;
            let value = vm.string(source)?;
            let start = (offset.max(0) as usize).min(value.len());
            let end = (start + length.max(0) as usize).min(value.len());
            vm.set_string(destination, &value[start..end])?;
        }
        0xa3 => {
            let [destination, source] = vm.args::<2>()?;
            let mut value = vm.string(destination)?;
            value.extend(vm.string(source)?);
            vm.set_string(destination, &value)?;
        }
        0xa4 => {
            let [a, b] = vm.args::<2>()?;
            let order = vm.string(a)?.cmp(&vm.string(b)?);
            vm.push(order as i32);
        }
        0xa5 => {
            let [resource, offset] = vm.args::<2>()?;
            let byte = *vm
                .resource(resource)?
                .get(offset as usize)
                .ok_or_else(|| format!("byte {offset} of resource {resource}"))?;
            vm.push(byte as i8 as i32);
        }
        0xa6 => {
            let [resource, offset, value] = vm.args::<3>()?;
            let slot = vm
                .resource_mut(resource)?
                .get_mut(offset as usize)
                .ok_or_else(|| format!("byte {offset} of resource {resource}"))?;
            *slot = value as u8;
        }
        0xa7 => {
            let resource = vm.pop()?;
            let text = String::from_utf8_lossy(&vm.string(resource)?).trim().parse().unwrap_or(0);
            vm.push(text);
        }
        0xa8 => {
            let [resource, value] = vm.args::<2>()?;
            vm.set_string(resource, format!("{value}").as_bytes())?;
        }
        // Bytes of the word memory, four to a word.
        0xa9 => {
            let [address, offset, value] = vm.args::<3>()?;
            write_byte(vm, address, offset, value as u8)?;
        }
        0xaa => {
            let [address, offset] = vm.args::<2>()?;
            let byte = read_byte(vm, address, offset)?;
            vm.push(byte as i32);
        }
        0xab => {
            let [address, offset, resource, count] = vm.args::<4>()?;
            let data = vm.resource(resource)?.to_vec();
            for i in 0..count {
                write_byte(vm, address, offset + i, data.get(i as usize).copied().unwrap_or(0))?;
            }
        }
        0xac => {
            let [address, offset, resource, count] = vm.args::<4>()?;
            let bytes = (0..count).map(|i| read_byte(vm, address, offset + i)).collect::<VmResult<Vec<_>>>()?;
            *vm.resource_mut(resource)? = bytes;
        }
        // Formatting into a resource: %s with one string, or one to five numbers.
        0xad => {
            let [destination, format, source] = vm.args::<3>()?;
            let value = vm.string(source)?;
            let text = format_c(&vm.string(format)?, &[], Some(&value));
            vm.set_string(destination, &text)?;
        }
        0xae..=0xb2 => {
            let count = (service - 0xae + 1) as usize;
            let mut values = Vec::with_capacity(count);
            for _ in 0..count {
                values.push(vm.pop()?);
            }
            values.reverse();
            let [destination, format] = vm.args::<2>()?;
            let text = format_c(&vm.string(format)?, &values, None);
            vm.set_string(destination, &text)?;
        }

        // Numbers.
        0xc8 => {
            let seed = vm.pop()?;
            host.seed(seed);
        }
        0xc9 => {
            let [a, b] = vm.args::<2>()?;
            let (low, high) = (a.min(b), a.max(b));
            let value = if low == high {
                low
            } else {
                low + host.random((high - low) as u32) as i32
            };
            vm.push(value);
        }
        0xca => {
            let percent = vm.pop()?;
            let value = (host.random(100) as i32) < percent;
            vm.push(value as i32);
        }
        0xcb => {
            let v = vm.pop()?;
            vm.push(v.wrapping_abs());
        }
        0xcc => {
            let v = vm.pop()?;
            vm.push(v.signum());
        }
        0xd6 => {
            let [a, b] = vm.args::<2>()?;
            vm.push(a.max(b));
        }
        0xd7 => {
            let [a, b, c] = vm.args::<3>()?;
            vm.push(a.max(b).max(c));
        }
        0xd8 => {
            let [a, b] = vm.args::<2>()?;
            vm.push(a.min(b));
        }
        0xd9 => {
            let [a, b, c] = vm.args::<3>()?;
            vm.push(a.min(b).min(c));
        }
        0xe0 => {
            // Whether `value` lies within `radius` of `center`, exclusive.
            let [center, value, radius] = vm.args::<3>()?;
            vm.push((center - radius < value && value < center + radius) as i32);
        }
        // Vector arithmetic over word spans: with a scalar, with a second
        // span, and from two spans.
        0xdd => {
            let [destination, scalar, count, operation] = vm.args::<4>()?;
            for i in 0..count {
                let value = vector_op(operation, vm.read(destination + i)?, scalar)?;
                vm.write(destination + i, value)?;
            }
        }
        0xde => {
            let [destination, source, count, operation] = vm.args::<4>()?;
            for i in 0..count {
                let value = vector_op(operation, vm.read(destination + i)?, vm.read(source + i)?)?;
                vm.write(destination + i, value)?;
            }
        }
        0xdf => {
            let [destination, left, right, count, operation] = vm.args::<5>()?;
            for i in 0..count {
                let l = vm.read(left + i)?;
                let value = if operation == 0 || operation == 8 {
                    vector_op(operation, 0, l)?
                } else {
                    vector_op(operation, l, vm.read(right + i)?)?
                };
                vm.write(destination + i, value)?;
            }
        }

        // The handset's system properties: "PLATFORM" is WIPI.
        0x1d8 => {
            let [key, destination] = vm.args::<2>()?;
            let value: &[u8] = if vm.string(key)? == b"PLATFORM" { b"WIPI" } else { b"" };
            vm.set_string(destination, value)?;
            vm.push(value.len() as i32);
        }

        _ => {
            let top = vm.stack.iter().rev().take(6).rev().collect::<Vec<_>>();
            return Err(format!("unknown service {service:#x} (stack {top:?})"));
        }
    }
    Ok(())
}

fn read_byte(vm: &Vm, address: i32, offset: i32) -> VmResult<u8> {
    let word = vm.read(address + offset.div_euclid(4))? as u32;
    Ok((word >> (8 * offset.rem_euclid(4))) as u8)
}

fn write_byte(vm: &mut Vm, address: i32, offset: i32, value: u8) -> VmResult<()> {
    let at = address + offset.div_euclid(4);
    let shift = 8 * offset.rem_euclid(4);
    let word = vm.read(at)? as u32;
    vm.write(at, ((word & !(255 << shift)) | (value as u32) << shift) as i32)
}

fn vector_op(operation: i32, a: i32, b: i32) -> VmResult<i32> {
    Ok(match operation {
        0 => b,
        1 => a.wrapping_add(b),
        2 => a.wrapping_sub(b),
        3 => a.wrapping_mul(b),
        4 | 5 => {
            if b == 0 {
                return Err("vector division by zero".into());
            }
            if operation == 4 { a.wrapping_div(b) } else { a.wrapping_rem(b) }
        }
        6 => a & b,
        7 => a | b,
        8 => !b,
        9 => a ^ b,
        10 => a >> (b & 31),
        11 => a << (b & 31),
        _ => return Err(format!("vector operation {operation}")),
    })
}

/// printf-style formatting as the scripts use it: `%d`, `%x`, `%X`, `%c` and
/// `%s` with flags and a width, bytes kept as they are (EUC-KR), at most 255.
fn format_c(format: &[u8], values: &[i32], string: Option<&[u8]>) -> Vec<u8> {
    let mut out = Vec::new();
    let mut values = values.iter();
    let mut i = 0;
    while i < format.len() {
        if format[i] != b'%' {
            out.push(format[i]);
            i += 1;
            continue;
        }
        i += 1;
        let (mut left, mut zero) = (false, false);
        while i < format.len() && matches!(format[i], b'-' | b'0' | b' ' | b'+' | b'#') {
            left |= format[i] == b'-';
            zero |= format[i] == b'0';
            i += 1;
        }
        let mut width = 0usize;
        while i < format.len() && format[i].is_ascii_digit() {
            width = width * 10 + (format[i] - b'0') as usize;
            i += 1;
        }
        if i < format.len() && format[i] == b'.' {
            i += 1;
            while i < format.len() && format[i].is_ascii_digit() {
                i += 1;
            }
        }
        let Some(&conversion) = format.get(i) else { break };
        i += 1;
        let body: Vec<u8> = match conversion {
            b'%' => {
                out.push(b'%');
                continue;
            }
            b'd' | b'i' | b'u' => format!("{}", values.next().copied().unwrap_or(0)).into_bytes(),
            b'x' => format!("{:x}", values.next().copied().unwrap_or(0) as u32).into_bytes(),
            b'X' => format!("{:X}", values.next().copied().unwrap_or(0) as u32).into_bytes(),
            b'c' => [values.next().copied().unwrap_or(0) as u8].to_vec(),
            b's' => string.unwrap_or(&[]).to_vec(),
            other => [b'%', other].to_vec(),
        };
        let pad = width.saturating_sub(body.len());
        if left {
            out.extend(&body);
            out.extend(core::iter::repeat_n(b' ', pad));
        } else if zero && conversion != b's' && body.first() == Some(&b'-') {
            out.push(b'-');
            out.extend(core::iter::repeat_n(b'0', pad));
            out.extend(&body[1..]);
        } else {
            out.extend(core::iter::repeat_n(if zero { b'0' } else { b' ' }, pad));
            out.extend(&body);
        }
    }
    out.truncate(255);
    out
}
