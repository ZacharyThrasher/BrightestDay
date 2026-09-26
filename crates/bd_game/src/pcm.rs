//! Pre-decoded sound effects: no decoder work when a voice starts.
//!
//! Starting a Bevy `AudioPlayer<AudioSource>` builds a `rodio::Decoder` on the main schedule —
//! format probing + Vorbis header/codebook setup, 1–8 ms per voice. In a brawl that was the #1
//! source of p99 frame spikes. Every SFX file is decoded once, on worker threads during loading,
//! into a [`PcmSound`] (an `Arc`-backed sample buffer), so starting a voice is an `Arc` clone.
//!
//! Use: [`PcmLibrary::request`] each `Handle<AudioSource>` after loading it; play
//! `AudioPlayer::<PcmSound>(lib.get(&h))` (fall back to the source while it is not decoded yet).
//! The loading screen waits until every file is [`PcmLibrary::settled`] (see `audio::track_loaded`).

use bevy::audio::{AddAudioSource, AudioSource, Decodable};
use bevy::platform::collections::{HashMap, HashSet};
use bevy::prelude::*;
use bevy::tasks::{futures::check_ready, AsyncComputeTaskPool, Task};
use rodio::buffer::SamplesBuffer;
use rodio::Source;
use std::io::Cursor;

/// A fully decoded sound (interleaved f32 samples, shared).
#[derive(Asset, TypePath, Clone)]
pub struct PcmSound(pub SamplesBuffer);

impl Decodable for PcmSound {
    type Decoder = SamplesBuffer;
    fn decoder(&self) -> SamplesBuffer {
        self.0.clone()
    }
}

#[derive(Resource, Default)]
pub struct PcmLibrary {
    ready: HashMap<AssetId<AudioSource>, Handle<PcmSound>>,
    requested: HashSet<AssetId<AudioSource>>,
    tasks: Vec<(AssetId<AudioSource>, Task<Option<SamplesBuffer>>)>,
    /// Requested but failed (unsupported / corrupt): never block loading on these.
    failed: HashSet<AssetId<AudioSource>>,
}

impl PcmLibrary {
    /// Decode this source once it has loaded (idempotent).
    pub fn request(&mut self, h: &Handle<AudioSource>) {
        self.requested.insert(h.id());
    }
    /// The decoded sound, if ready.
    pub fn get(&self, h: &Handle<AudioSource>) -> Option<Handle<PcmSound>> {
        self.ready.get(&h.id()).cloned()
    }
    /// Decoded, or given up on (plays through the streaming decoder).
    pub fn settled(&self, h: &Handle<AudioSource>) -> bool {
        self.ready.contains_key(&h.id()) || self.failed.contains(&h.id())
    }
}

pub struct PcmPlugin;

impl Plugin for PcmPlugin {
    fn build(&self, app: &mut App) {
        app.add_audio_source::<PcmSound>().init_resource::<PcmLibrary>().add_systems(PreUpdate, decode_requested);
    }
}

fn decode(bytes: AudioSource) -> Option<SamplesBuffer> {
    let dec = rodio::Decoder::new(Cursor::new(bytes)).ok()?;
    let (ch, rate) = (dec.channels(), dec.sample_rate());
    let samples: Vec<f32> = dec.collect();
    Some(SamplesBuffer::new(ch, rate, samples))
}

fn decode_requested(mut lib: ResMut<PcmLibrary>, sources: Res<Assets<AudioSource>>, mut pcm: ResMut<Assets<PcmSound>>) {
    let lib = &mut *lib;
    if lib.requested.len() == lib.ready.len() + lib.failed.len() && lib.tasks.is_empty() {
        return;
    }
    // Start decodes for loaded sources.
    let pool = AsyncComputeTaskPool::get();
    for id in lib.requested.iter() {
        if lib.ready.contains_key(id) || lib.failed.contains(id) || lib.tasks.iter().any(|(t, _)| t == id) {
            continue;
        }
        let Some(src) = sources.get(*id) else { continue };
        let src = src.clone();
        lib.tasks.push((*id, pool.spawn(async move { decode(src) })));
    }
    // Collect finished ones.
    let mut i = 0;
    while i < lib.tasks.len() {
        if let Some(res) = check_ready(&mut lib.tasks[i].1) {
            let (id, _) = lib.tasks.swap_remove(i);
            match res {
                Some(buf) => {
                    lib.ready.insert(id, pcm.add(PcmSound(buf)));
                }
                None => {
                    warn!("sfx: could not decode {id:?}; it plays through the streaming decoder");
                    lib.failed.insert(id);
                }
            }
        } else {
            i += 1;
        }
    }
}
