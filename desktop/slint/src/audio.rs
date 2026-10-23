//! Plays built-in and imported sounds (FX-3).
use overlay_studio::domain::sound::{BUILTINS, SoundRef, SoundRequest};
use overlay_studio::model::SoundAsset;
use rodio::{Decoder, DeviceSinkBuilder, MixerDeviceSink, Player, Source, buffer::SamplesBuffer};
use std::{collections::{BTreeMap, VecDeque}, io::Cursor, num::NonZero};

const RATE: u32 = 44_100;
const MAX_VOICES: usize = 16;
const MAX_DECODED_SAMPLES: usize = 48_000 * 2 * 30;

struct Clip {
    channels: u16,
    rate: u32,
    samples: Vec<f32>,
    source_len: usize,
}

#[derive(Default)]
pub struct Engine {
    sink: Option<MixerDeviceSink>,
    failed: Option<std::time::Instant>,
    voices: VecDeque<Player>,
    builtins: BTreeMap<String, Clip>,
    assets: BTreeMap<u64, Clip>,
    reported: Option<String>,
}

fn envelope(i: usize, n: usize, decay: f32) -> f32 {
    let t = i as f32 / n as f32;
    let attack = (i as f32 / (RATE as f32 * 0.004)).min(1.0);
    let release = ((n - i) as f32 / (RATE as f32 * 0.01)).min(1.0);
    attack * release * (-decay * t).exp()
}

fn tone(secs: f32, decay: f32, mut sample: impl FnMut(f32, f32) -> f32) -> Vec<f32> {
    let n = (secs * RATE as f32) as usize;
    (0..n).map(|i| sample(i as f32 / RATE as f32, i as f32 / n as f32) * envelope(i, n, decay)).collect()
}

/// Sine with a frequency gliding from `from` to `to` over `secs`.
fn glide(secs: f32, decay: f32, from: f32, to: f32) -> Vec<f32> {
    let mut phase = 0.0f32;
    tone(secs, decay, move |_, p| {
        phase += std::f32::consts::TAU * (from + (to - from) * p) / RATE as f32;
        phase.sin() * 0.8
    })
}

pub fn synth(name: &str) -> Option<Vec<f32>> {
    let tau = std::f32::consts::TAU;
    let triangle = |x: f32| 4.0 * ((x - (x + 0.5).floor()).abs()) - 1.0;
    Some(match name {
        "ding" => glide(0.35, 6.0, 880.0, 1320.0),
        "chime" => tone(0.6, 5.0, |t, _| 0.55 * triangle(660.0 * t) + 0.25 * triangle(990.0 * t)),
        "pop" => glide(0.12, 5.0, 300.0, 80.0),
        "buzz" => tone(0.35, 3.0, |t, _| 0.5 * (2.0 * (110.0 * t - (110.0 * t + 0.5).floor()))),
        "thud" => glide(0.3, 6.0, 90.0, 40.0),
        "whoosh" => {
            let (mut seed, mut low) = (0x9E37_79B9u32, 0.0f32);
            tone(0.4, 4.0, move |_, p| {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let noise = (seed >> 8) as f32 / (1u32 << 23) as f32 - 1.0;
                let cutoff = 400.0 + 3600.0 * (p * std::f32::consts::PI).sin();
                low += (1.0 - (-tau * cutoff / RATE as f32).exp()) * (noise - low);
                low * 1.6
            })
        }
        _ => return None,
    })
}

fn decode(bytes: &[u8], limit: usize) -> Result<Clip, String> {
    let decoder = Decoder::new(Cursor::new(bytes.to_vec())).map_err(|e| format!("Cannot decode sound: {e}"))?;
    let (channels, rate) = (decoder.channels().get(), decoder.sample_rate().get());
    let samples: Vec<f32> = decoder.take(limit).collect();
    if samples.is_empty() {
        return Err("Sound file contains no audio".into());
    }
    Ok(Clip { channels, rate, samples, source_len: bytes.len() })
}

impl Engine {
    /// Checks that the bytes decode to audio, reading only the first frames.
    pub fn validate(bytes: &[u8]) -> Result<(), String> {
        decode(bytes, 4096).map(|_| ())
    }

    pub fn test_play(&mut self, request: &SoundRequest, sounds: &[SoundAsset]) -> Result<(), String> {
        self.play(request, sounds)
    }

    /// Plays every request and returns the first error, once per distinct text.
    pub fn play_all(&mut self, requests: &[SoundRequest], sounds: &[SoundAsset]) -> Result<(), String> {
        let mut first = None;
        for request in requests {
            if let Err(error) = self.play(request, sounds) {
                first.get_or_insert(error);
            }
        }
        match first {
            Some(error) if self.reported.as_ref() != Some(&error) => {
                self.reported = Some(error.clone());
                Err(error)
            }
            _ => Ok(()),
        }
    }

    pub fn play(&mut self, request: &SoundRequest, sounds: &[SoundAsset]) -> Result<(), String> {
        let clip = match &request.sound {
            SoundRef::Builtin(name) => {
                if !self.builtins.contains_key(name) {
                    let samples = BUILTINS.contains(&name.as_str()).then(|| synth(name)).flatten();
                    let samples = samples.ok_or_else(|| format!("Unknown sound {name}"))?;
                    self.builtins.insert(name.clone(), Clip { channels: 1, rate: RATE, samples, source_len: 0 });
                }
                &self.builtins[name]
            }
            SoundRef::Asset(id) => {
                let asset = sounds.iter().find(|s| s.id == *id).ok_or_else(|| format!("Sound {id} is missing from the project"))?;
                if self.assets.get(id).is_none_or(|c| c.source_len != asset.bytes.len()) {
                    self.assets.remove(id);
                    self.assets.insert(*id, decode(&asset.bytes, MAX_DECODED_SAMPLES).map_err(|e| format!("{} : {e}", asset.name))?);
                }
                &self.assets[id]
            }
        };
        let (channels, rate) = (NonZero::new(clip.channels.max(1)).unwrap(), NonZero::new(clip.rate.max(1)).unwrap());
        let source = SamplesBuffer::new(channels, rate, clip.samples.clone());
        if self.sink.is_none() && self.failed.is_none_or(|at| at.elapsed() >= std::time::Duration::from_secs(5)) {
            match DeviceSinkBuilder::open_default_sink() {
                Ok(sink) => {
                    self.sink = Some(sink);
                    self.failed = None;
                }
                Err(e) => {
                    self.failed = Some(std::time::Instant::now());
                    return Err(format!("No audio output available: {e}"));
                }
            }
        }
        let Some(sink) = &self.sink else { return Ok(()) };
        let player = Player::connect_new(sink.mixer());
        player.set_volume(request.volume.clamp(0.0, 1.0));
        player.append(source);
        self.voices.retain(|p| !p.empty());
        while self.voices.len() >= MAX_VOICES {
            self.voices.pop_front();
        }
        self.voices.push_back(player);
        self.reported = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_are_bounded_and_fade_out() {
        for name in BUILTINS {
            let buf = synth(name).unwrap();
            let secs = buf.len() as f32 / RATE as f32;
            assert!((0.1..0.7).contains(&secs), "{name} {secs}");
            let peak = buf.iter().fold(0.0f32, |m, s| m.max(s.abs()));
            assert!(peak > 0.05 && peak < 1.0, "{name} peak {peak}");
            assert!(buf.last().unwrap().abs() < 0.01, "{name} tail");
            assert!(buf[0].abs() < 0.05, "{name} head");
        }
        assert!(synth("nope").is_none());
    }

    #[test]
    fn rejects_garbage_bytes() {
        assert!(Engine::validate(&[1, 2, 3, 4]).is_err());
    }

    #[test]
    fn missing_asset_reports_once() {
        let mut engine = Engine::default();
        let request = SoundRequest { sound: SoundRef::Asset(5), volume: 0.5 };
        assert!(engine.play_all(&[request.clone()], &[]).is_err());
        assert!(engine.play_all(&[request], &[]).is_ok());
    }
}
