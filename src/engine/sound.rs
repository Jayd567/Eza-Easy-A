//! `play "jump.wav" loop=true volume=0.5 speed=1.2`, `stop "music.ogg"`, `stop all`, `change sound.volume to 0.5`.
use super::*;
use crate::interp::SoundCmd;
use bevy::audio::{AudioPlayer, AudioSink, AudioSinkPlayback, PlaybackMode, PlaybackSettings, Volume};
use std::path::PathBuf;

#[derive(Component)]
pub(super) struct EzaSound {
    path: PathBuf,
    pub(super) looping: bool,
    volume: f32,
    /// a looping sound from the previous script, stopped unless the new script plays it again
    pub(super) orphan: bool,
}

fn source(rt: &mut Runtime, audio: &mut Assets<AudioSource>, path: &PathBuf) -> Option<Handle<AudioSource>> {
    if let Some(h) = rt.sounds.get(path) {
        return Some(h.clone());
    }
    match std::fs::read(path) {
        Ok(bytes) => {
            let h = audio.add(AudioSource { bytes: bytes.into() });
            rt.sounds.insert(path.clone(), h.clone());
            Some(h)
        }
        Err(e) => {
            eprintln!("[Notice] couldn't read the sound {}: {}", path.display(), e);
            None
        }
    }
}

pub(super) fn play_sounds(
    mut commands: Commands,
    mut rt: NonSendMut<Runtime>,
    mut audio: ResMut<Assets<AudioSource>>,
    mut q: Query<(Entity, &mut EzaSound, Option<&AudioSink>)>,
) {
    let rt = &mut *rt;
    let master = match rt.it.global("sound") {
        Some(Value::Obj(o)) => num(&o, "volume", 1.0).max(0.0),
        _ => 1.0,
    };
    for cmd in std::mem::take(&mut rt.it.sound_cmds) {
        match cmd {
            SoundCmd::Play { path, looping, volume, speed } => {
                // playing a looping sound that's already on just updates it (music keeps going)
                if looping {
                    if let Some((_, mut s, sink)) = q.iter_mut().find(|(_, s, _)| s.looping && s.path == path) {
                        s.volume = volume;
                        s.orphan = false;
                        if let Some(sink) = sink {
                            sink.set_volume(volume * master);
                            sink.set_speed(speed);
                        }
                        continue;
                    }
                }
                let Some(h) = source(rt, &mut audio, &path) else { continue };
                let mode = if looping { PlaybackMode::Loop } else { PlaybackMode::Despawn };
                commands.spawn((
                    AudioPlayer::new(h),
                    PlaybackSettings { mode, volume: Volume::new(volume * master), speed, ..default() },
                    EzaSound { path, looping, volume, orphan: false },
                ));
            }
            SoundCmd::Stop(which) => {
                for (e, s, _) in &q {
                    if which.as_ref().map_or(true, |p| *p == s.path) {
                        commands.entity(e).despawn();
                    }
                }
            }
        }
    }
    if (master - rt.master).abs() > 1e-4 {
        rt.master = master;
        for (_, s, sink) in &q {
            if let Some(sink) = sink {
                sink.set_volume(s.volume * master);
            }
        }
    }
    if rt.orphan_check {
        rt.orphan_check = false;
        for (e, s, _) in &q {
            if s.orphan {
                commands.entity(e).despawn();
            }
        }
    }
}
