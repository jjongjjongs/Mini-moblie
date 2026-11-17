//! The platform's services, each an opcode of its own (0x51 and up, besides
//! the instruction set's). Ported from wfeature's `ScriptSession.Call`,
//! `scriptGraphics.call` and the helpers beside them (MIT, Copyright (c) 2026
//! movingwoo); the drawing itself is this crate's.

use alloc::{format, vec, vec::Vec};

use super::{Vm, VmResult, c_string};
use crate::{
    graphics::TRANSPARENT,
    host::{Host, Sound},
    services::sin100,
};

/// Where the handset keeps the words `0x98`/`0x99` load and save.
const SAVE_FILE: &[u8] = b"nv/data";
const SAVE_MINIMUM: usize = 64;
/// The script palette's size; a colour is taken modulo it.
const COLOURS: i16 = 182;
/// Text cells, by style: tiny, small, normal, large.
const CELL_WIDTH: [i32; 4] = [4, 6, 6, 12];
const CELL_HEIGHT: [i32; 4] = [6, 8, 12, 24];
/// The most pictures `0x74` queues.
const QUEUE_LENGTH: usize = 20;

/// A picture `0x74` queued: the resource, where, and whether mirrored.
#[derive(Clone, Copy)]
pub struct Queued {
    resource: i16,
    x: i16,
    y: i16,
    mirror: bool,
}

fn colour(value: i16) -> i32 {
    value.rem_euclid(COLOURS) as i32
}

/// `values` into the words from `address` on.
fn write_words(vm: &mut Vm, address: i16, values: &[i16]) -> VmResult<()> {
    for (i, &value) in values.iter().enumerate() {
        vm.write(address.wrapping_add(i as i16), value)?;
    }
    Ok(())
}

fn string_length(data: &[u8]) -> VmResult<usize> {
    data.iter().position(|&x| x == 0).ok_or_else(|| "string not terminated".into())
}

/// Makes a resource `size` bytes long, as the handset's allocator does: it
/// grows by an even amount and keeps up to sixteen spare bytes when it would
/// shrink. New bytes are zero.
fn resize(vm: &mut Vm, id: i16, size: usize) -> VmResult<()> {
    let resource = vm.resource_mut(id)?;
    let current = resource.data.len() as i64;
    let mut delta = size as i64 - current;
    if delta > 0 && delta % 2 != 0 {
        delta += 1;
    }
    if (-16..=0).contains(&delta) {
        return Ok(());
    }
    let size = (current + delta) as usize;
    if size > 65535 {
        return Err(format!("resource {id} of {size} bytes"));
    }
    resource.data.resize(size, 0);
    Ok(())
}

/// `data` (NUL-terminated) as the resource's whole contents.
fn store_string(vm: &mut Vm, id: i16, data: &[u8]) -> VmResult<()> {
    resize(vm, id, data.len())?;
    vm.resource_mut(id)?.data[..data.len()].copy_from_slice(data);
    Ok(())
}

pub fn call(op: u8, vm: &mut Vm, host: &mut Host) -> VmResult<()> {
    tracing::trace!("service {op:#x} (stack {:?})", vm.stack.iter().rev().take(6).rev().collect::<Vec<_>>());
    if graphics(op, vm, host)? {
        return Ok(());
    }

    match op {
        // Reserved: the handset's handler does nothing.
        0xe3..=0xe5 | 0xe9..=0xef | 0xf2 | 0x93 | 0x95 => {}
        0xdb => {
            vm.args::<2>()?;
        }
        0xdc | 0xdd | 0x92 | 0x96 | 0x97 | 0xba => {
            vm.pop()?;
        }
        0x94 => {
            // Vibrate for this many seconds.
            vm.pop()?;
        }
        0xe0 => {
            vm.args::<6>()?;
        }
        0xe8 => {
            // Picture metadata: only the handset's own SIS pictures answer.
            vm.args::<7>()?;
            vm.push(-1)?;
        }
        0xe1 => {
            let value = vm.pop()?;
            vm.push(if value == 1 { 0 } else { -1 })?;
        }
        // The handset: its size class, colours, ...; and five more words.
        0x51 => {
            let address = vm.pop()?;
            let (w, h) = (host.graphics.width, host.graphics.height);
            let class = if w >= 176 && h >= 176 {
                8
            } else if w >= 128 && h >= 128 {
                4
            } else if w >= 80 && h >= 120 {
                2
            } else {
                1
            };
            write_words(vm, address, &[class, 4, 256, 1])?;
        }
        0x54 => {
            let address = vm.pop()?;
            write_words(vm, address, &[0; 5])?;
        }
        0x52 => {
            let id = vm.pop()?;
            resize(vm, id, 1)?;
            vm.resource_mut(id)?.data[0] = 0;
        }

        // Resources and strings.
        0x79 => {
            let id = vm.pop()?;
            let length = vm.resource(id)?.data.len() as i16;
            vm.push(length)?;
        }
        0x7a => {
            let [id, size] = vm.args::<2>()?;
            if size < 0 {
                return Err(format!("resource size {size}"));
            }
            resize(vm, id, size as usize)?;
            vm.push(1)?;
        }
        0x7b => {
            let [id, size] = vm.args::<2>()?;
            if size < 0 {
                return Err(format!("resource size {size}"));
            }
            resize(vm, id, size as usize)?;
            vm.resource_mut(id)?.data[..size as usize].fill(0);
        }
        0x7c => {
            let id = vm.pop()?;
            let length = string_length(&vm.resource(id)?.data)?;
            vm.push(length as i16)?;
        }
        0x7d => {
            let [destination, source] = vm.args::<2>()?;
            let data = &vm.resource(source)?.data;
            let length = string_length(data)?;
            let copied = data[..=length].to_vec();
            let size = data.len();
            resize(vm, destination, size)?;
            vm.resource_mut(destination)?.data[..copied.len()].copy_from_slice(&copied);
        }
        0x7e => {
            let [destination, source, offset, length] = vm.args::<4>()?;
            let data = &vm.resource(source)?.data;
            if offset < 0 || offset as usize > data.len() || length < 0 {
                return Err("substring out of bounds".into());
            }
            let available = &data[offset as usize..data.len().min(offset as usize + length as usize)];
            let copied = c_string(available);
            let mut result = vec![0; length as usize + 1];
            result[..copied.len()].copy_from_slice(copied);
            store_string(vm, destination, &result)?;
        }
        0x7f => {
            let [destination, source] = vm.args::<2>()?;
            let left = string_length(&vm.resource(destination)?.data)?;
            let right = &vm.resource(source)?.data;
            let suffix = right[..=string_length(right)?].to_vec();
            resize(vm, destination, left + suffix.len())?;
            vm.resource_mut(destination)?.data[left..left + suffix.len()].copy_from_slice(&suffix);
        }
        0x80 => {
            let [left, right] = vm.args::<2>()?;
            let (left, right) = (c_string(&vm.resource(left)?.data), c_string(&vm.resource(right)?.data));
            vm.push(left.cmp(right) as i16)?;
        }
        0x81 => {
            let [id, offset] = vm.args::<2>()?;
            let value = vm.resource_byte(id, offset as i32)? as i8 as i16;
            vm.push(value)?;
        }
        0x82 => {
            let [id, offset, value] = vm.args::<3>()?;
            let data = &mut vm.resource_mut(id)?.data;
            *data
                .get_mut(offset as usize)
                .filter(|_| offset >= 0)
                .ok_or("resource byte out of bounds")? = value as u8;
        }
        0x83 => {
            // A decimal number at the start of the string.
            let id = vm.pop()?;
            let data = c_string(&vm.resource(id)?.data);
            let mut position = data
                .iter()
                .position(|&x| !(x == b' ' || (b'\t'..=b'\r').contains(&x)))
                .unwrap_or(data.len());
            let negative = data.get(position) == Some(&b'-');
            if matches!(data.get(position), Some(b'+' | b'-')) {
                position += 1;
            }
            let mut value = 0u16;
            while let Some(&digit @ b'0'..=b'9') = data.get(position) {
                value = value.wrapping_mul(10).wrapping_add((digit - b'0') as u16);
                position += 1;
            }
            vm.push(if negative { value.wrapping_neg() as i16 } else { value as i16 })?;
        }
        0x84 => {
            let [id, value] = vm.args::<2>()?;
            let mut text = format!("{value}").into_bytes();
            text.push(0);
            store_string(vm, id, &text)?;
        }
        0x85 | 0x86 => {
            // A byte of the little-endian words from `base` on.
            let mut args = [0i16; 3];
            if op == 0x85 {
                let [base, offset] = vm.args::<2>()?;
                args[..2].copy_from_slice(&[base, offset]);
            } else {
                args = vm.args::<3>()?;
            }
            let (base, offset) = (args[0], args[1]);
            if base < 0 || offset < 0 {
                return Err("memory byte out of bounds".into());
            }
            let address = base.wrapping_add(offset / 2);
            let word = vm.read(address)? as u16;
            let shift = 8 * (offset % 2) as u16;
            if op == 0x85 {
                vm.push((word >> shift) as u8 as i16)?;
            } else {
                let word = word & !(0xff << shift) | (args[2] as u8 as u16) << shift;
                vm.write(address, word as i16)?;
            }
        }
        0x87 | 0x88 => {
            // Bytes between the words from `address` (starting `offset`
            // bytes in) and a resource: into it (0x87) or out of it.
            let [address, offset, id, count] = vm.args::<4>()?;
            if address < 0 || offset < 0 || count < 0 {
                return Err("transfer out of bounds".into());
            }
            let (offset, count) = (offset as usize, count as usize);
            let (first, last) = (offset / 2, (offset + count).div_ceil(2));
            let mut words = Vec::with_capacity((last - first) * 2);
            for i in first..last {
                words.extend_from_slice(&(vm.read(address.wrapping_add(i as i16))? as u16).to_le_bytes());
            }
            if op == 0x87 {
                let payload = words[offset % 2..offset % 2 + count].to_vec();
                resize(vm, id, count)?;
                vm.resource_mut(id)?.data[..count].copy_from_slice(&payload);
            } else if count > 0 {
                for i in 0..count {
                    words[offset % 2 + i] = vm.resource_byte(id, i as i32)?;
                }
                for i in first..last {
                    let at = (i - first) * 2;
                    vm.write(address.wrapping_add(i as i16), u16::from_le_bytes([words[at], words[at + 1]]) as i16)?;
                }
            }
        }
        0x89 => {
            let [destination, format, source] = vm.args::<3>()?;
            let format = c_string(&vm.resource(format)?.data).to_vec();
            let source = c_string(&vm.resource(source)?.data).to_vec();
            let result = string_format(&format, &source);
            store_string(vm, destination, &result)?;
        }
        0x8a..=0x8e => {
            let count = (op - 0x89) as usize;
            let mut values = [0i16; 5];
            if vm.stack.len() < count + 2 {
                return Err("operand stack underflow".into());
            }
            let base = vm.stack.len() - count;
            values[..count].copy_from_slice(&vm.stack[base..]);
            vm.stack.truncate(base);
            let [destination, format] = vm.args::<2>()?;
            let format = c_string(&vm.resource(format)?.data).to_vec();
            let result = number_format(&format, &values[..count])?;
            store_string(vm, destination, &result)?;
        }

        // Sound.
        0x90 => {
            let id = vm.pop()?;
            vm.resource(id)?;
            host.sounds.push(Sound::Play(id as i32));
        }
        0x91 => host.sounds.push(Sound::Stop),

        // The handset's saved words: load (0x98) or save (0x99) `count`
        // words from `address`.
        0x98 | 0x99 => {
            let [address, count] = vm.args::<2>()?;
            if !(0..=32767).contains(&count) {
                return Err(format!("save of {count} words"));
            }
            let count = count as usize;
            let mut data = vec![0u8; SAVE_MINIMUM.max(count * 2)];
            if op == 0x98 {
                if let Some(saved) = host.files.load(SAVE_FILE) {
                    let n = saved.len().min(data.len());
                    data[..n].copy_from_slice(&saved[..n]);
                }
                for i in 0..count {
                    vm.write(address.wrapping_add(i as i16), i16::from_le_bytes([data[i * 2], data[i * 2 + 1]]))?;
                }
            } else {
                for i in 0..count {
                    let value = vm.read(address.wrapping_add(i as i16))?;
                    data[i * 2..i * 2 + 2].copy_from_slice(&value.to_le_bytes());
                }
                host.files.store(SAVE_FILE, &data);
            }
        }

        // Three timers: start with a period and whether it repeats, or stop.
        0x9a..=0x9c => {
            let [period, repeat] = vm.args::<2>()?;
            if period >= 10 {
                host.start_timer((op - 0x9a) as usize, period as i32, repeat != 0);
            }
        }
        0x9d..=0x9f => host.timers[(op - 0x9d) as usize] = None,

        // Numbers.
        0xa0 => {
            let seed = vm.pop()?;
            host.seed(seed as u16 as i32);
        }
        0xa1 => {
            let [a, b] = vm.args::<2>()?;
            let (lo, hi) = (a.min(b) as i32, a.max(b) as i32);
            let value = if lo == hi { lo } else { lo + host.random((hi - lo) as u32) as i32 };
            vm.push(value as i16)?;
        }
        0xa2 => {
            let percent = vm.pop()?;
            let roll = host.random(100) as i32;
            vm.push((roll < percent as i32) as i16)?;
        }
        0xa3 => {
            let value = vm.pop()?;
            vm.push(value.wrapping_abs())?;
        }
        0xa4 => {
            let value = vm.pop()?;
            vm.push(value.signum())?;
        }
        0xa5..=0xaa => {
            let value = vm.pop()?;
            vm.push(trig(op, value))?;
        }
        0xab => {
            let [a, b] = vm.args::<2>()?;
            vm.push(((a as i32 + b as i32) / 2) as i16)?;
        }
        0xac => {
            let [a, b, c] = vm.args::<3>()?;
            vm.push(((a as i32 + b as i32 + c as i32) / 3) as i16)?;
        }
        0xad => {
            let [a, b] = vm.args::<2>()?;
            vm.push(a.max(b))?;
        }
        0xae => {
            let [a, b, c] = vm.args::<3>()?;
            vm.push(a.max(b).max(c))?;
        }
        0xaf => {
            let [a, b] = vm.args::<2>()?;
            vm.push(a.min(b))?;
        }
        0xb0 => {
            let [a, b, c] = vm.args::<3>()?;
            vm.push(a.min(b).min(c))?;
        }
        // The index of the largest, the smallest, or the nearest to a value
        // among `count` words.
        0xb1..=0xb3 => {
            let (address, count, target) = if op == 0xb3 {
                let [a, b, c] = vm.args::<3>()?;
                (a, b, c)
            } else {
                let [a, b] = vm.args::<2>()?;
                (a, b, 0)
            };
            let count = count.max(1);
            let (mut best, mut index, mut best_distance) = (0i16, 0i16, 0i32);
            for i in 0..count {
                let value = vm.read(address.wrapping_add(i))?;
                let distance = (value as i32 - target as i32).abs();
                let better = i == 0
                    || match op {
                        0xb1 => value > best,
                        0xb2 => value < best,
                        _ => distance < best_distance,
                    };
                if better {
                    (best, index, best_distance) = (value, i, distance);
                    if op == 0xb3 && i > 0 && distance == 0 {
                        break;
                    }
                }
            }
            vm.push(index)?;
        }
        0xb7 => {
            let [center, value, radius] = vm.args::<3>()?;
            let (center, value, radius) = (center as i32, value as i32, radius as i32);
            vm.push((center - radius < value && value < center + radius) as i16)?;
        }
        // The date and the time. The date is fixed; the time runs on the
        // guest clock, as the timers do.
        0xb8 => {
            let address = vm.pop()?;
            write_words(vm, address, &[2006, 1, 1, 0])?;
        }
        0xb9 => {
            let address = vm.pop()?;
            let ms = host.clock + 12 * 3_600_000;
            let fields = [
                (ms / 3_600_000 % 24) as i16,
                (ms / 60_000 % 60) as i16,
                (ms / 1000 % 60) as i16,
                (ms % 1000) as i16,
            ];
            write_words(vm, address, &fields)?;
        }
        // Opening a web page, which nothing here does.
        0xc4 => {
            vm.pop()?;
        }
        // Debug output.
        0xf0 => {
            vm.args::<4>()?;
        }
        0xf1 => {
            vm.args::<2>()?;
        }
        _ => return Err(format!("service {op:#x} is not known")),
    }
    Ok(())
}

/// The drawing services; false when `op` is not one of them.
fn graphics(op: u8, vm: &mut Vm, host: &mut Host) -> VmResult<bool> {
    let g = &mut host.graphics;
    match op {
        0x55 => g.fill(255),
        0x56 => g.fill(0),
        0x57 => {
            let c = colour(vm.pop()?);
            let v = g.mapped(c);
            g.fill(v);
        }
        0x58 => {
            let [x, y, c] = vm.args::<3>()?;
            let c = colour(c);
            if c != TRANSPARENT {
                let v = g.mapped(c);
                g.point(x as i32, y as i32, v);
            }
        }
        0x59 => g.bank = (vm.pop()? as i32).clamp(0, 6),
        0x5a..=0x5d => vm.push(if op == 0x5a { 0 } else { (op - 0x58) as i16 })?,
        0x5e => g.color = (vm.pop()? as u8 as i16 % COLOURS) as i32,
        0x5f => {
            let [x, y, x2, y2] = vm.args::<4>()?;
            let c = g.color;
            g.line(x as i32, y as i32, x2 as i32, y2 as i32, c);
        }
        0x60 => {
            let [x, x2, y] = vm.args::<3>()?;
            let c = g.color;
            g.line(x as i32, y as i32, x2 as i32, y as i32, c);
        }
        0x61 => {
            let [x, y, y2] = vm.args::<3>()?;
            let c = g.color;
            g.line(x as i32, y as i32, x as i32, y2 as i32, c);
        }
        0x62 => {
            let [x, y, x2, y2] = vm.args::<4>()?;
            let c = g.color;
            g.rect(x as i32, y as i32, x2 as i32, y2 as i32, c);
        }
        0x63 => {
            let [x, y, x2, y2] = vm.args::<4>()?;
            let c = g.color;
            g.fill_rect(x as i32, y as i32, x2 as i32, y2 as i32, c);
        }
        0x64 | 0x65 => {
            let [cx, cy, rx, ry] = vm.args::<4>()?;
            let (cx, cy, rx, ry) = (cx as i32, cy as i32, (rx as i32).abs(), (ry as i32).abs());
            if rx == 0 || ry == 0 {
                let c = g.color;
                g.line(cx - rx, cy - ry, cx + rx, cy + ry, c);
            } else {
                g.ellipse(cx, cy, rx, ry, op == 0x65);
            }
        }
        0x66 => {
            let [style, fg, bg, align] = vm.args::<4>()?;
            host.text_style = style as u8 as i32 % 4;
            g.text_fg = fg as u8 as i32 % 182;
            g.text_bg = bg as u8 as i32 % 182;
            g.text_align = align as u8 as i32 % 3;
        }
        0x67 => host.text_style = vm.pop()? as u8 as i32 % 4,
        0x68 => {
            let [fg, bg] = vm.args::<2>()?;
            g.text_fg = fg as u8 as i32 % 182;
            g.text_bg = bg as u8 as i32 % 182;
        }
        0x69 => g.text_align = vm.pop()? as u8 as i32 % 3,
        0x6a..=0x6d | 0xcf | 0xd0 => text(op, vm, host)?,
        0x6e => {
            let [destination, source] = vm.args::<2>()?;
            pack_palette(vm, destination, source)?;
        }
        0x6f | 0x70 => {
            let (x, y, id, mirror) = if op == 0x6f {
                let [x, y, id] = vm.args::<3>()?;
                (x, y, id, false)
            } else {
                let [x, y, id, mirror] = vm.args::<4>()?;
                (x, y, id, mirror != 0)
            };
            let data = &vm.resource(id)?.data;
            g.bitmap(data, x as i32, y as i32, mirror, None)
                .map_err(|x| format!("resource {id}: {x}"))?;
        }
        0x71 | 0x72 => {
            let (x, y, id, mirror, palette) = if op == 0x71 {
                let [x, y, id, palette] = vm.args::<4>()?;
                (x, y, id, false, palette)
            } else {
                let [x, y, id, mirror, palette] = vm.args::<5>()?;
                (x, y, id, mirror != 0, palette)
            };
            let count = match vm.resource(id)?.data.first() {
                Some(2) => 1,
                Some(3 | 5) => 2,
                Some(6) => 4,
                Some(7) => 16,
                _ => 0,
            };
            let mut bytes = Vec::with_capacity(count);
            for i in 0..count {
                let word = vm.read(palette.wrapping_add((i / 2) as i16))? as u16;
                bytes.push((word >> (8 * (i % 2))) as u8);
            }
            let data = &vm.resource(id)?.data;
            let g = &mut host.graphics;
            g.bitmap(data, x as i32, y as i32, mirror, Some(&bytes))
                .map_err(|x| format!("resource {id}: {x}"))?;
        }
        0x73 => vm.queue.clear(),
        0x74 => {
            // A result slot the call leaves below its four arguments.
            let [_, x, y, id, mirror] = vm.args::<5>()?;
            vm.resource(id)?;
            if vm.queue.len() < QUEUE_LENGTH {
                vm.queue.push(Queued {
                    resource: id,
                    x,
                    y,
                    mirror: mirror != 0,
                });
            }
        }
        0x75 => {
            // Draws the queue, sorted by x up or down (0, 1) or by y (2, 3).
            let mode = vm.pop()?;
            let mut queue = vm.queue.clone();
            if (0..=3).contains(&mode) {
                queue.sort_by_key(|x| {
                    let key = if mode >= 2 { x.y } else { x.x } as i32;
                    if mode % 2 == 0 { key } else { -key }
                });
            }
            for entry in queue {
                let data = &vm.resource(entry.resource)?.data;
                host.graphics
                    .bitmap(data, entry.x as i32, entry.y as i32, entry.mirror, None)
                    .map_err(|x| format!("resource {}: {x}", entry.resource))?;
            }
        }
        0x76 => g.backup.copy_from_slice(&g.pixels),
        0x77 => g.pixels.copy_from_slice(&g.backup),
        0x78 => g.present(),
        0xc8 => {
            let [x1, y1, x2, y2] = vm.args::<4>()?;
            let (x1, y1, x2, y2) = (x1 as i32, y1 as i32, x2 as i32, y2 as i32);
            g.clip = (
                x1.min(x2).max(0),
                y1.min(y2).max(0),
                (x1.max(x2) + 1).min(g.width),
                (y1.max(y2) + 1).min(g.height),
            );
        }
        0xc9 => g.clip = (0, 0, g.width, g.height),
        0xca => {
            let [x, y] = vm.args::<2>()?;
            let (x, y) = (x as i32, y as i32);
            let value = if (0..g.width).contains(&x) && (0..g.height).contains(&y) {
                g.pixels[(y * g.width + x) as usize] as i16
            } else {
                -1
            };
            vm.push(value)?;
        }
        0xcb | 0xcc => {
            let [x1, y1, x2, y2, radius] = vm.args::<5>()?;
            g.round_rect(x1 as i32, y1 as i32, x2 as i32, y2 as i32, radius as i32, op == 0xcc);
        }
        0xcd => {
            let [x1, y1, x2, y2] = vm.args::<4>()?;
            let (x0, y0, xe, ye) = g.clip;
            for y in (y1.min(y2) as i32).max(y0)..=(y1.max(y2) as i32).min(ye - 1) {
                for x in (x1.min(x2) as i32).max(x0)..=(x1.max(x2) as i32).min(xe - 1) {
                    let at = (y * g.width + x) as usize;
                    g.pixels[at] = !g.pixels[at];
                }
            }
        }
        0xce => {
            let [buffer, dx, dy, mode] = vm.args::<4>()?;
            g.scroll(buffer as i32, dx as i32, dy as i32, mode as i32);
        }
        _ => return Ok(false),
    }
    Ok(true)
}

/// Text from a string resource at x, y: `0x6a` in the current style, `0x6b`
/// over its background, `0x6c`/`0x6d` the same in the normal style, and
/// `0xcf`/`0xd0` with effect flags (bold, italic, underline).
fn text(op: u8, vm: &mut Vm, host: &mut Host) -> VmResult<()> {
    let extended = op == 0xcf || op == 0xd0;
    let (x, y, id, flags) = if extended {
        let [x, y, id, flags] = vm.args::<4>()?;
        (x as i32, y as i32, id, flags as i32 & 7)
    } else {
        let [x, y, id] = vm.args::<3>()?;
        (x as i32, y as i32, id, 0)
    };
    let data = c_string(&vm.resource(id)?.data);
    if data.is_empty() {
        return Ok(());
    }
    let (decoded, _, _) = encoding_rs::EUC_KR.decode(data);
    let style = if op == 0x6c || op == 0x6d { 2 } else { host.text_style } as usize;
    let (cell_w, cell_h) = (CELL_WIDTH[style], CELL_HEIGHT[style]);
    let width = data.len() as i32 * cell_w;
    let g = &mut host.graphics;
    let x = match g.text_align {
        1 => x - width / 2,
        2 => x - width,
        _ => x,
    };

    let background = op == 0x6b || op == 0x6d || op == 0xd0;
    if background && g.text_bg != TRANSPARENT {
        let extension = if extended && flags & 2 != 0 { cell_h / 4 - 1 } else { 0 } + (flags & 1);
        let bg = g.text_bg;
        g.fill_rect(x - 1, y - 1, x + width + extension - 1, y + cell_h - 1, bg);
    }
    if g.text_fg != TRANSPARENT {
        let fg = g.text_fg;
        if extended && flags & 4 != 0 {
            g.line(x - 1, y + cell_h, x + width - 1, y + cell_h, fg);
        }
        g.draw_string(x, y, &decoded, cell_h as f32, fg);
        if extended && flags & 1 != 0 {
            g.draw_string(x + 1, y, &decoded, cell_h as f32, fg);
        }
    }
    Ok(())
}

/// A palette in the words at `source` - its bitmap type, then one script
/// colour a word - packed into bytes at `destination` as a bitmap of that
/// type carries it: two to a byte for types 2-4, one for 5-7.
fn pack_palette(vm: &mut Vm, destination: i16, source: i16) -> VmResult<()> {
    let kind = vm.read(source)? as i32;
    if !(2..=7).contains(&kind) {
        return Err(format!("packed palette type {kind}"));
    }
    let count = [2, 4, 16][((kind - 2) % 3) as usize];
    let mut words = Vec::with_capacity(count);
    for i in 0..count {
        words.push(vm.read(source.wrapping_add(1 + i as i16))?);
    }
    let packed: Vec<u8> = if kind <= 4 {
        words.chunks(2).map(|x| (x[0] as u8) << 4 | (x[1] as u8) & 15).collect()
    } else {
        words.iter().map(|&x| x as u8).collect()
    };
    for (i, &b) in packed.iter().enumerate() {
        let address = destination.wrapping_add((i / 2) as i16);
        let shift = 8 * (i % 2) as u16;
        let word = vm.read(address)? as u16 & !(0xff << shift) | (b as u16) << shift;
        vm.write(address, word as i16)?;
    }
    Ok(())
}

/// `sin`, `cos`, `tan` of whole degrees and their inverses, ratios ×100.
fn trig(op: u8, value: i16) -> i16 {
    let cos100 = |d: i32| sin100(d + 90);
    match op {
        0xa5 => sin100(value as i32) as i16,
        0xa6 => cos100(value as i32) as i16,
        0xa7 => {
            let d = (value as i32).rem_euclid(180);
            if d == 90 {
                0
            } else {
                let (s, c) = (sin100(d), cos100(d));
                ((s * 100 + c / 2 * c.signum()) / c) as i16
            }
        }
        _ => {
            if !(-100..=100).contains(&value) || op == 0xaa && value == -100 {
                return 32767;
            }
            let v = value as i32;
            // The whole degree whose ratio is nearest.
            let (range, ratio): (core::ops::RangeInclusive<i32>, &dyn Fn(i32) -> i32) = match op {
                0xa8 => (-90..=90, &|d| sin100(d) * 100),
                0xa9 => (0..=180, &|d| cos100(d) * 100),
                _ => (-89..=89, &|d| if cos100(d) == 0 { i32::MAX } else { sin100(d) * 10000 / cos100(d) }),
            };
            range.min_by_key(|&d| (ratio(d) - v * 100).abs()).unwrap_or(0) as i16
        }
    }
}

/// `%d`, `%x`, `%X` and `%c` with flags, width and precision; at most 255
/// bytes, NUL-terminated.
fn number_format(format: &[u8], values: &[i16]) -> VmResult<Vec<u8>> {
    let mut out: Vec<u8> = Vec::with_capacity(256);
    let push = |out: &mut Vec<u8>, bytes: &[u8]| {
        let n = bytes.len().min(255usize.saturating_sub(out.len()));
        out.extend_from_slice(&bytes[..n]);
    };
    let mut used = 0;
    let mut i = 0;
    while i < format.len() && out.len() < 255 {
        if format[i] != b'%' {
            out.push(format[i]);
            i += 1;
            continue;
        }
        i += 1;
        let (mut left, mut zero, mut alternate) = (false, false, false);
        let mut sign: &[u8] = b"";
        let (mut width, mut precision) = (0usize, None::<usize>);
        while i < format.len() {
            match format[i] {
                b'-' => left = true,
                b'+' => sign = b"+",
                b' ' if sign.is_empty() => sign = b" ",
                b' ' => {}
                b'#' => alternate = true,
                b'0' => zero = true,
                b'.' => {
                    let mut p = 0;
                    while i + 1 < format.len() && format[i + 1].is_ascii_digit() {
                        i += 1;
                        p = (p * 10 + (format[i] - b'0') as usize).min(256);
                    }
                    precision = Some(p);
                }
                c @ b'1'..=b'9' => {
                    width = (c - b'0') as usize;
                    while i + 1 < format.len() && format[i + 1].is_ascii_digit() {
                        i += 1;
                        width = (width * 10 + (format[i] - b'0') as usize).min(256);
                    }
                }
                _ => break,
            }
            i += 1;
        }
        let Some(&verb) = format.get(i) else { break };
        i += 1;
        if !matches!(verb, b'd' | b'x' | b'X' | b'c') {
            push(&mut out, &[verb]);
            continue;
        }
        let value = *values.get(used).ok_or("format has too few arguments")?;
        used += 1;
        let (mut digits, mut prefix): (Vec<u8>, Vec<u8>) = (Vec::new(), Vec::new());
        match verb {
            b'c' => digits.push(value as u8),
            b'd' => {
                prefix = sign.to_vec();
                if value < 0 {
                    prefix = b"-".to_vec();
                }
                digits = format!("{}", (value as i32).abs()).into_bytes();
            }
            _ => {
                digits = format!("{:x}", value as i32 as u32).into_bytes();
                if verb == b'X' {
                    digits.make_ascii_uppercase();
                }
                if alternate && value != 0 {
                    prefix = vec![b'0', verb];
                }
            }
        }
        if verb != b'c' {
            if value == 0 && precision == Some(0) {
                digits.clear();
            }
            if let Some(p) = precision {
                let mut padded = vec![b'0'; p.saturating_sub(digits.len())];
                padded.extend_from_slice(&digits);
                digits = padded;
                zero = false;
            }
        }
        let padding = width.saturating_sub(prefix.len() + digits.len());
        if !left && !zero {
            push(&mut out, &vec![b' '; padding]);
        }
        push(&mut out, &prefix);
        if !left && zero {
            push(&mut out, &vec![b'0'; padding]);
        }
        push(&mut out, &digits);
        if left {
            push(&mut out, &vec![b' '; padding]);
        }
    }
    let mut out = c_string(&out).to_vec();
    out.push(0);
    Ok(out)
}

/// Every `%s` of `format` replaced by `source`, with flags, width and
/// precision; at most 255 bytes, NUL-terminated.
fn string_format(format: &[u8], source: &[u8]) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::with_capacity(256);
    let mut i = 0;
    while i < format.len() && out.len() < 255 {
        if format[i] != b'%' {
            out.push(format[i]);
            i += 1;
            continue;
        }
        i += 1;
        let (mut left, mut zero) = (false, false);
        let (mut width, mut precision) = (0usize, None::<usize>);
        while i < format.len() {
            match format[i] {
                b'-' => left = true,
                b'0' => zero = true,
                b'.' => {
                    let mut p = 0;
                    while i + 1 < format.len() && format[i + 1].is_ascii_digit() {
                        i += 1;
                        p = (p * 10 + (format[i] - b'0') as usize).min(256);
                    }
                    precision = Some(p);
                }
                c @ b'1'..=b'9' => {
                    width = (c - b'0') as usize;
                    while i + 1 < format.len() && format[i + 1].is_ascii_digit() {
                        i += 1;
                        width = (width * 10 + (format[i] - b'0') as usize).min(256);
                    }
                }
                _ => break,
            }
            i += 1;
        }
        let Some(&verb) = format.get(i) else { break };
        i += 1;
        if verb != b's' {
            if out.len() < 255 {
                out.push(verb);
            }
            continue;
        }
        let length = precision.map_or(source.len(), |p| p.min(source.len()));
        let padding = width.saturating_sub(length);
        if !left {
            out.extend(core::iter::repeat_n(if zero { b'0' } else { b' ' }, padding));
        }
        out.extend_from_slice(&source[..length]);
        if left {
            out.extend(core::iter::repeat_n(b' ', padding));
        }
        out.truncate(255);
    }
    out.push(0);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_are_formatted() {
        assert_eq!(number_format(b"%03d:%x", &[7, 255]).unwrap(), b"007:ff\0");
        assert_eq!(number_format(b"%-4d|", &[-5]).unwrap(), b"-5  |\0");
        assert_eq!(string_format(b"[%5s]", b"ab"), b"[   ab]\0");
    }

    #[test]
    fn trigonometry_is_scaled() {
        assert_eq!(trig(0xa5, 90), 100);
        assert_eq!(trig(0xa6, 180), -100);
        assert_eq!(trig(0xa8, 50), 30);
        assert_eq!(trig(0xaa, 100), 45);
    }
}
