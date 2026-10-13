//! Plays Black Ops II's own sounds, read from the player's sound banks where they are.

use crate::game::Sfx;
use crate::sheets::{SOUNDS, Snd};
use bo2snd::{FORMAT_FLAC, FORMAT_PCMS16, SoundIndex};
use rodio::buffer::SamplesBuffer;
use rodio::{Decoder, OutputStream, Source};
use std::collections::HashMap;
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::mpsc::{Sender, channel};

struct Clip {
    channels: u16,
    rate: u32,
    samples: Vec<i16>,
    volume: f32,
}

pub struct Audio {
    tx: Sender<Snd>,
}

impl Sfx for Audio {
    fn play(&self, snd: Snd) {
        let _ = self.tx.send(snd);
    }
}

fn decode(bytes: Vec<u8>, s: &bo2snd::SoundRef) -> Result<(u16, u32, Vec<i16>), String> {
    match s.format {
        FORMAT_FLAC => {
            let d = Decoder::new(Cursor::new(bytes)).map_err(|e| e.to_string())?;
            let (ch, rate) = (d.channels(), d.sample_rate());
            Ok((ch, rate, d.collect()))
        }
        FORMAT_PCMS16 => {
            let samples = bytes.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect();
            Ok((s.channels.max(1) as u16, s.rate, samples))
        }
        f => Err(format!("format {f} not supported")),
    }
}

pub fn index_path() -> PathBuf {
    crate::plugin::data_dir().join("bo2_index.json")
}

/// Loads every sound in sounds.json from the player's BO2 banks and starts the audio thread.
pub fn start(log: impl Fn(String) + Send + 'static) -> Audio {
    let (tx, rx) = channel::<Snd>();
    std::thread::spawn(move || {
        let mut clips: HashMap<Snd, Clip> = HashMap::new();
        match std::fs::read_to_string(index_path()).map_err(|e| e.to_string()).and_then(|t| serde_json::from_str::<SoundIndex>(&t).map_err(|e| e.to_string())) {
            Ok(index) => {
                let bo2 = crate::plugin::setting("ELDENOPS_BO2").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(&index.bo2_dir));
                for row in SOUNDS {
                    let Some(s) = index.sounds.get(row.bo2_alias) else {
                        log(format!("audio: {} not in index", row.bo2_alias));
                        continue;
                    };
                    match bo2snd::read_sound(&bo2, s).map_err(|e| e.to_string()).and_then(|b| decode(b, s)) {
                        Ok((channels, rate, samples)) => {
                            log(format!("audio: {} {:.1}s from {}", row.bo2_alias, samples.len() as f32 / channels as f32 / rate as f32, s.bank));
                            clips.insert(row.snd, Clip { channels, rate, samples, volume: row.volume });
                        }
                        Err(e) => log(format!("audio: {} failed: {e}", row.bo2_alias)),
                    }
                }
            }
            Err(e) => log(format!("audio: no BO2 sound index ({e}); playing silently")),
        }
        let stream = OutputStream::try_default();
        let handle = match &stream {
            Ok((_, h)) => Some(h.clone()),
            Err(e) => {
                log(format!("audio: no output device: {e}"));
                None
            }
        };
        log(format!("audio: ready, {} of {} sounds", clips.len(), SOUNDS.len()));
        if crate::melty_test::active() {
            match (&handle, clips.len()) {
                (None, _) => crate::melty_test::report("bo2-sounds", false, Some("no audio output device")),
                (_, 0) => crate::melty_test::report("bo2-sounds", false, Some("no Black Ops II sound could be read from its sound banks (see eldenops.log)")),
                (Some(h), n) => {
                    let c = clips.get(&Snd::RoundStart).or_else(|| clips.values().next()).unwrap();
                    let ok = h.play_raw(SamplesBuffer::new(c.channels, c.rate, c.samples.clone()).amplify(c.volume).convert_samples()).is_ok();
                    let why = format!("{n} of {} read, but playback failed", SOUNDS.len());
                    crate::melty_test::report("bo2-sounds", ok, Some(&why));
                }
            }
        }
        for snd in rx {
            let (Some(h), Some(c)) = (&handle, clips.get(&snd)) else { continue };
            let src = SamplesBuffer::new(c.channels, c.rate, c.samples.clone()).amplify(c.volume);
            if let Err(e) = h.play_raw(src.convert_samples()) {
                log(format!("audio: play {snd:?}: {e}"));
            }
        }
    });
    Audio { tx }
}
