//! The formats MIDP's `Manager.createPlayer` hands over besides SMAF.
//!
//! A MIDP 2.0 title names its sounds by MIME type, and the European handsets
//! Gameloft built for - 리얼풋볼매니저2010's Nokia N95 build among them - play
//! `audio/midi` and `audio/x-wav`. The audio path plays a SMAF file as a list
//! of timed MIDI events and recorded waves, so both are read into that same
//! list here and played the same way.

use alloc::vec::Vec;

use smaf_player::SmafEvent;

/// The microseconds a quarter note lasts until a file says otherwise.
const DEFAULT_TEMPO: u64 = 500_000;

/// A Standard MIDI File as timed events, the time in milliseconds.
///
/// Every track of a format 0 or 1 file is read and merged; a tempo change in
/// any of them sets the pace for all, as the format has it. Note on with no
/// velocity is a note off. Aftertouch and channel pressure are dropped - the
/// synthesiser has no use for them. The list ends with [`SmafEvent::End`] at
/// the last event, so a looping play waits the file out before going round.
///
/// `None` for anything that does not open with a header and a track.
pub fn parse_midi(data: &[u8]) -> Option<Vec<(usize, SmafEvent)>> {
    let header = data.get(..14)?;
    if &header[..4] != b"MThd" {
        return None;
    }
    let header_length = u32::from_be_bytes([header[4], header[5], header[6], header[7]]) as usize;
    let division = u16::from_be_bytes([header[12], header[13]]);
    if division == 0 {
        return None;
    }

    // (tick, order read, event) for every track; tempo changes apart.
    let mut events: Vec<(u64, usize, SmafEvent)> = Vec::new();
    let mut tempos: Vec<(u64, u64)> = Vec::new();

    let mut offset = 8usize.checked_add(header_length)?;
    while let Some(chunk) = data.get(offset..offset + 8) {
        let length = u32::from_be_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]) as usize;
        let body = data.get(offset + 8..(offset + 8).saturating_add(length).min(data.len()))?;
        if &chunk[..4] == b"MTrk" {
            read_track(body, &mut events, &mut tempos);
        }
        offset = offset.checked_add(8 + length)?;
    }

    if events.is_empty() {
        return None;
    }

    events.sort_by_key(|(tick, order, _)| (*tick, *order));
    tempos.sort_by_key(|(tick, _)| *tick);

    let to_ms = |tick: u64| -> usize {
        if division & 0x8000 != 0 {
            // SMPTE: frames a second (negated) times ticks a frame.
            let frames = u64::from((-((division >> 8) as i8)) as u8);
            let per_frame = u64::from(division & 0xff);
            return (tick * 1000 / (frames * per_frame).max(1)) as usize;
        }

        let per_quarter = u64::from(division);
        let mut micros = 0u64;
        let mut last_tick = 0u64;
        let mut tempo = DEFAULT_TEMPO;
        for &(at, next) in &tempos {
            if at >= tick {
                break;
            }
            micros += (at - last_tick) * tempo / per_quarter;
            last_tick = at;
            tempo = next;
        }
        micros += (tick - last_tick) * tempo / per_quarter;

        (micros / 1000) as usize
    };

    let mut result: Vec<(usize, SmafEvent)> = events.into_iter().map(|(tick, _, event)| (to_ms(tick), event)).collect();
    let end = result.last().map_or(0, |(time, _)| *time);
    result.push((end, SmafEvent::End));

    Some(result)
}

/// One track's events, appended with their tick and the order they were read.
fn read_track(track: &[u8], events: &mut Vec<(u64, usize, SmafEvent)>, tempos: &mut Vec<(u64, u64)>) {
    let mut at = 0usize;
    let mut tick = 0u64;
    let mut running = 0u8;

    while at < track.len() {
        let Some(delta) = read_varint(track, &mut at) else {
            return;
        };
        tick += u64::from(delta);

        let Some(&first) = track.get(at) else {
            return;
        };
        let status = if first & 0x80 != 0 {
            at += 1;
            first
        } else {
            running
        };

        match status {
            0xff => {
                let Some(&kind) = track.get(at) else {
                    return;
                };
                at += 1;
                let Some(length) = read_varint(track, &mut at) else {
                    return;
                };
                let Some(body) = track.get(at..at + length as usize) else {
                    return;
                };
                at += length as usize;

                match kind {
                    0x2f => return,
                    0x51 if body.len() == 3 => tempos.push((tick, u64::from(body[0]) << 16 | u64::from(body[1]) << 8 | u64::from(body[2]))),
                    _ => {}
                }
            }
            0xf0 | 0xf7 => {
                let Some(length) = read_varint(track, &mut at) else {
                    return;
                };
                let Some(body) = track.get(at..at + length as usize) else {
                    return;
                };
                at += length as usize;

                let mut message = Vec::with_capacity(body.len() + 1);
                if status == 0xf0 {
                    message.push(0xf0);
                }
                message.extend_from_slice(body);
                if message.last() != Some(&0xf7) {
                    message.push(0xf7);
                }
                let order = events.len();
                events.push((tick, order, SmafEvent::MidiSysEx(message)));
            }
            0x80..=0xef => {
                running = status;
                let channel = status & 0x0f;
                let size = if matches!(status & 0xf0, 0xc0 | 0xd0) { 1 } else { 2 };
                let Some(bytes) = track.get(at..at + size) else {
                    return;
                };
                at += size;

                let event = match status & 0xf0 {
                    0x80 => Some(SmafEvent::MidiNoteOff {
                        channel,
                        note: bytes[0],
                        velocity: bytes[1],
                    }),
                    0x90 if bytes[1] == 0 => Some(SmafEvent::MidiNoteOff {
                        channel,
                        note: bytes[0],
                        velocity: 0,
                    }),
                    0x90 => Some(SmafEvent::MidiNoteOn {
                        channel,
                        note: bytes[0],
                        velocity: bytes[1],
                    }),
                    0xb0 => Some(SmafEvent::MidiControlChange {
                        channel,
                        control: bytes[0],
                        value: bytes[1],
                    }),
                    0xc0 => Some(SmafEvent::MidiProgramChange { channel, program: bytes[0] }),
                    0xe0 => Some(SmafEvent::MidiPitchBend {
                        channel,
                        value: u16::from(bytes[0] & 0x7f) | u16::from(bytes[1] & 0x7f) << 7,
                    }),
                    _ => None,
                };
                if let Some(event) = event {
                    let order = events.len();
                    events.push((tick, order, event));
                }
            }
            // A system common or real-time byte has no place in a file, and a
            // data byte with no status before it is a broken track.
            _ => return,
        }
    }
}

fn read_varint(data: &[u8], at: &mut usize) -> Option<u32> {
    let mut value = 0u32;
    for _ in 0..4 {
        let byte = *data.get(*at)?;
        *at += 1;
        value = value << 7 | u32::from(byte & 0x7f);
        if byte & 0x80 == 0 {
            return Some(value);
        }
    }

    None
}

/// A RIFF WAVE file as one recorded wave, played from its start.
///
/// PCM of eight or sixteen bits and IMA ADPCM - what Gameloft's builds carry,
/// four bits at 8 kHz - are read; more than one channel is mixed down to one,
/// which is what the wave voice plays. `None` for anything else.
pub fn parse_wave(data: &[u8]) -> Option<Vec<(usize, SmafEvent)>> {
    if data.len() < 12 || &data[..4] != b"RIFF" || &data[8..12] != b"WAVE" {
        return None;
    }

    let mut format = None;
    let mut samples = None;

    let mut offset = 12usize;
    while let Some(chunk) = data.get(offset..offset + 8) {
        let length = u32::from_le_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]) as usize;
        let body = &data[offset + 8..(offset + 8).saturating_add(length).min(data.len())];
        match &chunk[..4] {
            b"fmt " if body.len() >= 16 => format = Some(body),
            b"data" => samples = Some(body),
            _ => {}
        }
        // Chunks are padded to an even length.
        offset = offset.checked_add(8 + length + (length & 1))?;
    }

    let format = format?;
    let samples = samples?;

    let tag = u16::from_le_bytes([format[0], format[1]]);
    let channels = u16::from_le_bytes([format[2], format[3]]).max(1) as usize;
    let sampling_rate = u32::from_le_bytes([format[4], format[5], format[6], format[7]]);
    let block_align = u16::from_le_bytes([format[12], format[13]]) as usize;
    let bits = u16::from_le_bytes([format[14], format[15]]);

    let interleaved = match (tag, bits) {
        (1, 8) => samples.iter().map(|&x| (i16::from(x) - 128) << 8).collect(),
        (1, 16) => samples.chunks_exact(2).map(|x| i16::from_le_bytes([x[0], x[1]])).collect(),
        (0x11, 4) => decode_ima_adpcm(samples, channels, block_align),
        _ => return None,
    };

    let mono: Vec<i16> = interleaved
        .chunks_exact(channels)
        .map(|frame| (frame.iter().map(|&x| i32::from(x)).sum::<i32>() / channels as i32) as i16)
        .collect();
    if mono.is_empty() || sampling_rate == 0 {
        return None;
    }

    let length_ms = (mono.len() as u64 * 1000 / u64::from(sampling_rate)) as usize;

    Some(alloc::vec![
        (
            0,
            SmafEvent::Wave {
                channel: 0,
                sampling_rate,
                data: mono,
            }
        ),
        (length_ms, SmafEvent::End),
    ])
}

const IMA_INDEX: [i32; 16] = [-1, -1, -1, -1, 2, 4, 6, 8, -1, -1, -1, -1, 2, 4, 6, 8];

const IMA_STEP: [i32; 89] = [
    7, 8, 9, 10, 11, 12, 13, 14, 16, 17, 19, 21, 23, 25, 28, 31, 34, 37, 41, 45, 50, 55, 60, 66, 73, 80, 88, 97, 107, 118, 130, 143, 157, 173, 190,
    209, 230, 253, 279, 307, 337, 371, 408, 449, 494, 544, 598, 658, 724, 796, 876, 963, 1060, 1166, 1282, 1411, 1552, 1707, 1878, 2066, 2272, 2499,
    2749, 3024, 3327, 3660, 4026, 4428, 4871, 5358, 5894, 6484, 7132, 7845, 8630, 9493, 10442, 11487, 12635, 13899, 15289, 16818, 18500, 20350,
    22385, 24623, 27086, 29794, 32767,
];

/// Microsoft's IMA ADPCM: blocks of `block_align` bytes, each opening with a
/// sample and a step index per channel, then four-byte runs of nibbles per
/// channel in turn, low nibble first. Interleaved samples come out.
fn decode_ima_adpcm(data: &[u8], channels: usize, block_align: usize) -> Vec<i16> {
    let header = 4 * channels;
    if block_align <= header {
        return Vec::new();
    }

    let mut out = Vec::new();
    for block in data.chunks(block_align) {
        if block.len() <= header {
            break;
        }

        let mut predictors = [0i32; 2];
        let mut indices = [0i32; 2];
        let channels = channels.min(2);
        for channel in 0..channels {
            let at = channel * 4;
            predictors[channel] = i32::from(i16::from_le_bytes([block[at], block[at + 1]]));
            indices[channel] = i32::from(block[at + 2]).clamp(0, 88);
        }

        let frames = (block.len() - header) * 2 / channels;
        let mut decoded = alloc::vec![0i16; (frames + 1) * channels];
        for channel in 0..channels {
            decoded[channel] = predictors[channel] as i16;
        }

        // Each channel's nibbles come in runs of four bytes - eight samples.
        for (run, bytes) in block[header..].chunks(4).enumerate() {
            let channel = run % channels;
            let first = (run / channels) * 8;
            for (i, &byte) in bytes.iter().enumerate() {
                for (half, nibble) in [byte & 0x0f, byte >> 4].into_iter().enumerate() {
                    let frame = first + i * 2 + half + 1;
                    let (predictor, index) = (&mut predictors[channel], &mut indices[channel]);
                    let step = IMA_STEP[*index as usize];
                    let mut diff = step >> 3;
                    if nibble & 1 != 0 {
                        diff += step >> 2;
                    }
                    if nibble & 2 != 0 {
                        diff += step >> 1;
                    }
                    if nibble & 4 != 0 {
                        diff += step;
                    }
                    *predictor = if nibble & 8 != 0 { *predictor - diff } else { *predictor + diff }.clamp(-32768, 32767);
                    *index = (*index + IMA_INDEX[nibble as usize]).clamp(0, 88);

                    if let Some(slot) = decoded.get_mut(frame * channels + channel) {
                        *slot = *predictor as i16;
                    }
                }
            }
        }

        out.extend_from_slice(&decoded);
    }

    out
}

#[cfg(test)]
mod tests {
    use alloc::{vec, vec::Vec};

    use smaf_player::SmafEvent;

    use super::{parse_midi, parse_wave};

    #[test]
    fn a_midi_file_is_read_at_its_tempo() {
        // Format 0, 96 ticks a quarter, 120 bpm then 60 bpm.
        let mut track = vec![
            0x00, 0xc0, 0x05, // program 5 on channel 0
            0x00, 0x90, 0x3c, 0x64, // note on
            0x60, 0x3c, 0x00, // a quarter later, running status, note off
            0x00, 0xff, 0x51, 0x03, 0x0f, 0x42, 0x40, // tempo: a second a quarter
            0x60, 0x90, 0x3e, 0x64, // a quarter at the new tempo
        ];
        track.extend_from_slice(&[0x00, 0xff, 0x2f, 0x00]);

        let mut file = Vec::from(&b"MThd"[..]);
        file.extend_from_slice(&[0, 0, 0, 6, 0, 0, 0, 1, 0, 96]);
        file.extend_from_slice(b"MTrk");
        file.extend_from_slice(&(track.len() as u32).to_be_bytes());
        file.extend_from_slice(&track);

        let events = parse_midi(&file).expect("a file");
        let times: Vec<usize> = events.iter().map(|(time, _)| *time).collect();
        assert_eq!(times, [0, 0, 500, 1500, 1500]);
        assert!(matches!(events[2].1, SmafEvent::MidiNoteOff { note: 0x3c, .. }));
        assert!(matches!(events[3].1, SmafEvent::MidiNoteOn { note: 0x3e, .. }));
        assert!(matches!(events[4].1, SmafEvent::End));
    }

    #[test]
    fn a_wave_file_is_one_recording() {
        let samples: [u8; 8] = [0x00, 0x00, 0xff, 0x7f, 0x00, 0x80, 0x10, 0x00];
        let mut file = Vec::from(&b"RIFF"[..]);
        file.extend_from_slice(&(36 + samples.len() as u32).to_le_bytes());
        file.extend_from_slice(b"WAVEfmt ");
        file.extend_from_slice(&16u32.to_le_bytes());
        file.extend_from_slice(&[1, 0, 1, 0]);
        file.extend_from_slice(&8000u32.to_le_bytes());
        file.extend_from_slice(&16000u32.to_le_bytes());
        file.extend_from_slice(&[2, 0, 16, 0]);
        file.extend_from_slice(b"data");
        file.extend_from_slice(&(samples.len() as u32).to_le_bytes());
        file.extend_from_slice(&samples);

        let events = parse_wave(&file).expect("a recording");
        match &events[0].1 {
            SmafEvent::Wave { sampling_rate, data, .. } => {
                assert_eq!(*sampling_rate, 8000);
                assert_eq!(data, &[0, 0x7fff, -0x8000, 0x10]);
            }
            _ => panic!("not a wave"),
        }
        assert!(matches!(events[1].1, SmafEvent::End));
    }

    #[test]
    fn an_ima_adpcm_block_starts_from_its_header_sample() {
        // One mono block: header sample 100, index 0, then nibbles of zero.
        let block = [100, 0, 0, 0, 0, 0, 0, 0];
        let mut file = Vec::from(&b"RIFF"[..]);
        file.extend_from_slice(&(40 + block.len() as u32).to_le_bytes());
        file.extend_from_slice(b"WAVEfmt ");
        file.extend_from_slice(&20u32.to_le_bytes());
        file.extend_from_slice(&[0x11, 0, 1, 0]);
        file.extend_from_slice(&8000u32.to_le_bytes());
        file.extend_from_slice(&4055u32.to_le_bytes());
        file.extend_from_slice(&[block.len() as u8, 0, 4, 0, 2, 0, 9, 0]);
        file.extend_from_slice(b"data");
        file.extend_from_slice(&(block.len() as u32).to_le_bytes());
        file.extend_from_slice(&block);

        let events = parse_wave(&file).expect("a recording");
        match &events[0].1 {
            SmafEvent::Wave { data, .. } => {
                // The header sample and eight decoded ones, each nudged up by
                // the smallest step.
                assert_eq!(data.len(), 9);
                assert_eq!(data[0], 100);
                assert!(data[1..].windows(2).all(|pair| pair[1] >= pair[0]));
            }
            _ => panic!("not a wave"),
        }
    }
}
