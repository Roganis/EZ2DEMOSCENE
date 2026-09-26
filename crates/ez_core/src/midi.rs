//! Minimal Standard MIDI File reader: note timings (in seconds, following
//! tempo changes), keys, velocities and channels. Enough to drive visuals
//! from exact notes instead of audio analysis.

#[derive(Clone, Debug, PartialEq)]
pub struct MidiNote {
    /// Start in seconds.
    pub time: f32,
    /// Length in seconds.
    pub length: f32,
    pub key: u8,
    pub velocity: u8,
    /// 0-based channel (9 = General MIDI drums).
    pub channel: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DrumKind {
    Kick,
    Snare,
    Hat,
    Other,
}

impl MidiNote {
    /// The General MIDI drum this note plays, if it is on the drum channel.
    pub fn drum(&self) -> Option<DrumKind> {
        if self.channel != 9 {
            return None;
        }
        Some(match self.key {
            35 | 36 => DrumKind::Kick,
            37..=40 => DrumKind::Snare,
            42 | 44 | 46 | 49 | 51 | 52 | 55 | 57 | 59 => DrumKind::Hat,
            _ => DrumKind::Other,
        })
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MidiData {
    pub notes: Vec<MidiNote>,
    /// End of the last note, in seconds.
    pub duration: f32,
    /// First tempo found (BPM).
    pub bpm: Option<f32>,
}

struct Reader<'a> {
    d: &'a [u8],
    p: usize,
}

impl<'a> Reader<'a> {
    fn u8(&mut self) -> Result<u8, String> {
        let v = *self
            .d
            .get(self.p)
            .ok_or("unexpected end of the MIDI file")?;
        self.p += 1;
        Ok(v)
    }
    fn be(&mut self, n: usize) -> Result<u32, String> {
        let mut v = 0u32;
        for _ in 0..n {
            v = (v << 8) | self.u8()? as u32;
        }
        Ok(v)
    }
    fn var(&mut self) -> Result<u32, String> {
        let mut v = 0u32;
        for _ in 0..4 {
            let b = self.u8()?;
            v = (v << 7) | (b & 0x7f) as u32;
            if b & 0x80 == 0 {
                return Ok(v);
            }
        }
        Err("bad variable-length number".into())
    }
    fn skip(&mut self, n: usize) -> Result<(), String> {
        if self.p + n > self.d.len() {
            return Err("unexpected end of the MIDI file".into());
        }
        self.p += n;
        Ok(())
    }
}

/// Parse a .mid file.
pub fn parse(data: &[u8]) -> Result<MidiData, String> {
    let mut r = Reader { d: data, p: 0 };
    if data.len() < 14 || &data[0..4] != b"MThd" {
        return Err("not a MIDI file".into());
    }
    r.skip(4)?;
    let hlen = r.be(4)? as usize;
    let _format = r.be(2)?;
    let ntracks = r.be(2)?;
    let division = r.be(2)?;
    r.skip(hlen.saturating_sub(6))?;
    if division & 0x8000 != 0 {
        return Err("SMPTE-timed MIDI files are not supported".into());
    }
    let tpq = division.max(1) as f64;

    // (tick, kind): tempo changes and note events from all tracks.
    let mut tempos: Vec<(u64, u32)> = Vec::new();
    // (tick, on, channel, key, velocity)
    let mut events: Vec<(u64, bool, u8, u8, u8)> = Vec::new();
    for _ in 0..ntracks {
        if r.p + 8 > data.len() {
            break;
        }
        let id = &data[r.p..r.p + 4];
        r.skip(4)?;
        let len = r.be(4)? as usize;
        let end = (r.p + len).min(data.len());
        if id != b"MTrk" {
            r.p = end;
            continue;
        }
        let mut tick = 0u64;
        let mut status = 0u8;
        while r.p < end {
            tick += r.var()? as u64;
            let mut b = r.u8()?;
            if b < 0x80 {
                // Running status: reuse the last status byte.
                r.p -= 1;
                b = status;
            } else if b < 0xf0 {
                status = b;
            }
            match b {
                0xff => {
                    let kind = r.u8()?;
                    let l = r.var()? as usize;
                    if kind == 0x51 && l == 3 {
                        tempos.push((tick, r.be(3)?));
                    } else {
                        r.skip(l)?;
                    }
                }
                0xf0 | 0xf7 => {
                    let l = r.var()? as usize;
                    r.skip(l)?;
                }
                _ => {
                    let ch = b & 0x0f;
                    match b & 0xf0 {
                        0x80 => {
                            let k = r.u8()?;
                            let _ = r.u8()?;
                            events.push((tick, false, ch, k, 0));
                        }
                        0x90 => {
                            let k = r.u8()?;
                            let v = r.u8()?;
                            events.push((tick, v > 0, ch, k, v));
                        }
                        0xa0 | 0xb0 | 0xe0 => r.skip(2)?,
                        0xc0 | 0xd0 => r.skip(1)?,
                        _ => return Err("corrupt MIDI track".into()),
                    }
                }
            }
        }
        r.p = end;
    }
    tempos.sort_by_key(|t| t.0);
    let bpm = tempos.first().map(|t| 60_000_000.0 / t.1 as f32);
    // Ticks -> seconds through the tempo map (default 120 BPM).
    let seconds = |tick: u64| -> f64 {
        let mut t = 0.0f64;
        let mut last_tick = 0u64;
        let mut us_per_q = 500_000.0f64;
        for &(tt, tempo) in &tempos {
            if tt >= tick {
                break;
            }
            t += (tt - last_tick) as f64 / tpq * us_per_q / 1e6;
            last_tick = tt;
            us_per_q = tempo as f64;
        }
        t + (tick - last_tick) as f64 / tpq * us_per_q / 1e6
    };
    events.sort_by_key(|e| (e.0, e.1));
    let mut notes = Vec::new();
    // Open notes per (channel, key): index into `notes`.
    let mut open: std::collections::HashMap<(u8, u8), usize> = Default::default();
    for (tick, on, ch, key, vel) in events {
        let t = seconds(tick) as f32;
        if let Some(i) = open.remove(&(ch, key)) {
            let n: &mut MidiNote = &mut notes[i];
            n.length = (t - n.time).max(0.0);
        }
        if on {
            open.insert((ch, key), notes.len());
            notes.push(MidiNote {
                time: t,
                length: 0.1,
                key,
                velocity: vel,
                channel: ch,
            });
        }
    }
    let duration = notes
        .iter()
        .map(|n| n.time + n.length)
        .fold(0.0f32, f32::max);
    Ok(MidiData {
        notes,
        duration,
        bpm,
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A one-track file at `bpm`: kick (ch 10) on every beat for `beats`
    /// beats and an A4 then C5 melody note in the first two beats.
    pub fn sample_file(bpm: f32, beats: u32) -> Vec<u8> {
        let mut trk: Vec<u8> = Vec::new();
        let tempo = (60_000_000.0 / bpm) as u32;
        trk.extend_from_slice(&[0x00, 0xff, 0x51, 0x03]);
        trk.extend_from_slice(&tempo.to_be_bytes()[1..]);
        // melody: A4 at 0 for one beat, C5 at 1 beat
        trk.extend_from_slice(&[0x00, 0x90, 69, 100]);
        for b in 0..beats {
            // kick note on (running status not used)
            let dt: &[u8] = if b == 0 { &[0x00] } else { &[0x83, 0x60] }; // 480 ticks
            trk.extend_from_slice(dt);
            trk.extend_from_slice(&[0x99, 36, 110]);
            if b == 1 {
                trk.extend_from_slice(&[0x00, 0x80, 69, 0, 0x00, 0x90, 72, 90]);
            }
        }
        trk.extend_from_slice(&[0x83, 0x60, 0x80, 72, 0]);
        trk.extend_from_slice(&[0x00, 0xff, 0x2f, 0x00]);
        let mut f = Vec::new();
        f.extend_from_slice(b"MThd");
        f.extend_from_slice(&6u32.to_be_bytes());
        f.extend_from_slice(&0u16.to_be_bytes());
        f.extend_from_slice(&1u16.to_be_bytes());
        f.extend_from_slice(&480u16.to_be_bytes());
        f.extend_from_slice(b"MTrk");
        f.extend_from_slice(&(trk.len() as u32).to_be_bytes());
        f.extend_from_slice(&trk);
        f
    }

    #[test]
    fn parses_notes_and_tempo() {
        let m = parse(&sample_file(120.0, 8)).unwrap();
        assert_eq!(m.bpm.map(|b| b.round()), Some(120.0));
        let kicks: Vec<_> = m
            .notes
            .iter()
            .filter(|n| n.drum() == Some(DrumKind::Kick))
            .collect();
        assert_eq!(kicks.len(), 8);
        assert!((kicks[3].time - 1.5).abs() < 1e-3);
        let mel: Vec<_> = m.notes.iter().filter(|n| n.drum().is_none()).collect();
        assert_eq!(mel.len(), 2);
        assert_eq!(mel[1].key, 72);
        assert!((mel[1].time - 0.5).abs() < 1e-3);
        assert!(parse(b"nope").is_err());
    }

    #[test]
    fn midi_drives_an_envelope() {
        use crate::audio::{AudioEnvelope, Curve, HitKind};
        let m = parse(&sample_file(120.0, 8)).unwrap();
        let env = AudioEnvelope::from_midi(&m);
        assert_eq!(env.hits[HitKind::Kick as usize].len(), 8);
        assert!(env.midi_hits[HitKind::Kick as usize]);
        // C (pitch class 0) plays after 0.5 s, A (9) before.
        assert!((env.value(Curve::Pitch, false, 0.25) * 12.0 - 9.0).abs() < 0.01);
        assert!(env.value(Curve::Pitch, false, 0.75).abs() < 0.01);
    }
}
