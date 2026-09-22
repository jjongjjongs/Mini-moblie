//! What one MMF is made of, and what this build's player gets out of it.
//!
//! `mmf_signatures` answers whether two renders came out the same. This answers
//! the question before it: what is in the file at all - which tracks, which
//! system exclusive messages, how many notes, and whether any sample ever
//! reaches the mixer. A title whose effects sound wrong is nearly always a
//! message this player drops, and that is visible here and nowhere else.
//!
//! 데빌메이크라이 is why it exists. Its music is MA-3 (`f0 43 79 06`) and every
//! one of its effects is MA-5 (`f0 43 79 07`), carrying its sound as a wave
//! bulk this synthesiser has no path for - so each effect came out as the four
//! bare notes that were meant to trigger it, on whatever voice was loaded.
//!
//! ```text
//! cargo run -p wie_backend --example mmf_chunks -- res/att0_0.mmf res/bgm2.mmf
//! ```

use std::{env, fs};

use smaf_player::{SmafEvent, parse_smaf};

fn describe(event: &SmafEvent) -> String {
    match event {
        SmafEvent::Wave {
            channel,
            sampling_rate,
            data,
        } => format!("Wave ch={channel} rate={sampling_rate} samples={}", data.len()),
        SmafEvent::MidiNoteOn { channel, note, velocity } => format!("NoteOn ch={channel} note={note} velocity={velocity}"),
        SmafEvent::MidiNoteOff { channel, note, velocity } => format!("NoteOff ch={channel} note={note} velocity={velocity}"),
        SmafEvent::MidiProgramChange { channel, program } => format!("Program ch={channel} program={program}"),
        SmafEvent::MidiControlChange { channel, control, value } => format!("Control ch={channel} control={control} value={value}"),
        SmafEvent::MidiPitchBend { channel, value } => format!("PitchBend ch={channel} value={value}"),
        SmafEvent::MidiSysEx(data) => {
            let head = data.iter().take(8).map(|byte| format!("{byte:02x}")).collect::<Vec<_>>().join(" ");
            format!("SysEx {} bytes [{head} ...]", data.len())
        }
        SmafEvent::End => "End".to_string(),
    }
}

fn main() {
    let paths = env::args().skip(1).collect::<Vec<_>>();
    if paths.is_empty() {
        eprintln!("usage: mmf_chunks <file.mmf> [more.mmf ...]");
        return;
    }

    for path in paths {
        let raw = fs::read(&path).expect("MMF file");
        println!("=== {path} ({} bytes)", raw.len());

        match smaf::Smaf::parse(&raw) {
            Ok(smaf) => {
                for chunk in &smaf.chunks {
                    match chunk {
                        smaf::SmafChunk::ScoreTrack(track, score) => println!(
                            "  ScoreTrack {track}: format={:?} sequence={:?} timebase d={} g={} channels={} chunks={}",
                            score.format_type,
                            score.sequence_type,
                            score.timebase_d,
                            score.timebase_g,
                            score.channel_status.len(),
                            score.chunks.len()
                        ),
                        smaf::SmafChunk::PCMAudioTrack(track, _) => println!("  PCMAudioTrack {track}"),
                        _ => {}
                    }
                }
            }
            Err(error) => println!("  parse failed: {error:?}"),
        }

        let events = parse_smaf(&raw);
        let notes = events.iter().filter(|(_, event)| matches!(event, SmafEvent::MidiNoteOn { .. })).count();
        let waves = events.iter().filter(|(_, event)| matches!(event, SmafEvent::Wave { .. })).count();
        println!("  events={} noteOn={notes} wave={waves}", events.len());

        for (time, event) in events.iter().take(20) {
            println!("    {time:6} {}", describe(event));
        }

        if let Some((last, _)) = events.last() {
            println!("    ... last event at {last}");
        }
    }
}
