//! F-30: General MIDI playback with a SoundFont (GeneralUser GS, shipped with the installers), through `rustysynth`.
//!
//! The bank is loaded on a background thread when the sound starts; until it is ready, or when the file is missing or
//! damaged, the built-in synthesizer in `audio.rs` plays as before. The external MIDI output is not involved.
use rustysynth::{SoundFont, Synthesizer, SynthesizerSettings};
use std::path::PathBuf;
use std::sync::Arc;

/// The bank's file name, next to the app (macOS: Fermata.app/Contents/Resources; Windows: beside Fermata.exe).
pub const SF_FILE: &str = "GeneralUser-GS.sf2";

/// Where the bank is looked for, in order: `FERMATA_SOUNDFONT`, the app bundle's Resources, beside the program, a
/// `soundfont` folder beside it, and (development) `assets/soundfont/` in the source tree.
#[cfg(not(target_arch = "wasm32"))]
pub fn candidates() -> Vec<PathBuf> {
    let mut v = vec![];
    if let Some(p) = std::env::var_os("FERMATA_SOUNDFONT") {
        v.push(PathBuf::from(p));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            v.push(dir.join("../Resources").join(SF_FILE));
            v.push(dir.join(SF_FILE));
            v.push(dir.join("soundfont").join(SF_FILE));
        }
    }
    v.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/soundfont").join(SF_FILE));
    v
}

#[cfg(not(target_arch = "wasm32"))]
pub fn find() -> Option<PathBuf> {
    candidates().into_iter().find(|p| p.is_file())
}

#[cfg(not(target_arch = "wasm32"))]
static BANK: std::sync::OnceLock<Result<Arc<SoundFont>, String>> = std::sync::OnceLock::new();
#[cfg(not(target_arch = "wasm32"))]
static LOADING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Start reading the bank in the background (once). The app calls this at start, so the bank is usually ready by
/// the first note; until then the built-in synthesizer plays.
pub fn preload() {
    #[cfg(not(target_arch = "wasm32"))]
    if !LOADING.swap(true, std::sync::atomic::Ordering::SeqCst) {
        std::thread::spawn(|| {
            let t0 = std::time::Instant::now();
            let r = match find() {
                None => Err(format!("{SF_FILE} not found; using the built-in sound")),
                Some(p) => std::fs::read(&p).map_err(|e| format!("{}: {e}", p.display())).and_then(|b| parse(&b)),
            };
            match &r {
                Ok(_) => eprintln!("SoundFont loaded in {} ms", t0.elapsed().as_millis()),
                Err(e) => eprintln!("SoundFont: {e}"),
            }
            let _ = BANK.set(r);
        });
    }
}

/// The bank once it has been read: None while loading (or in the browser), Some(Err) when missing or damaged.
pub fn bank() -> Option<Result<Arc<SoundFont>, String>> {
    #[cfg(not(target_arch = "wasm32"))]
    return BANK.get().cloned();
    #[cfg(target_arch = "wasm32")]
    None
}

/// Read a SoundFont from its bytes. A damaged file gives an error, never a crash.
pub fn parse(bytes: &[u8]) -> Result<Arc<SoundFont>, String> {
    let r = std::panic::catch_unwind(|| {
        let mut c = std::io::Cursor::new(bytes);
        SoundFont::new(&mut c).map_err(|e| format!("{e:?}"))
    });
    match r {
        Ok(Ok(sf)) if !sf.get_presets().is_empty() => Ok(Arc::new(sf)),
        Ok(Ok(_)) => Err("the SoundFont has no instruments".into()),
        Ok(Err(e)) => Err(format!("damaged SoundFont ({e})")),
        Err(_) => Err("damaged SoundFont".into()),
    }
}

/// A General MIDI synthesizer: 16 channels, channel 10 (index 9) the drum kit, with what each channel was last set
/// to (instrument, volume, pan), so only changes are sent.
pub struct GmSynth {
    synth: Synthesizer,
    program: [i32; 16],
    volume: [i32; 16],
    pan: [i32; 16],
    left: Vec<f32>,
    right: Vec<f32>,
}

/// The channel the editor's audition and a MIDI keyboard play on (the score's staves use 1–15 and 10 for drums).
pub const LIVE_CHANNEL: u8 = 15;

impl GmSynth {
    pub fn new(sf: &Arc<SoundFont>, sample_rate: f64) -> Result<GmSynth, String> {
        let mut st = SynthesizerSettings::new(sample_rate.round() as i32);
        st.maximum_polyphony = 128;
        let synth = Synthesizer::new(sf, &st).map_err(|e| format!("{e:?}"))?;
        Ok(GmSynth { synth, program: [-1; 16], volume: [-1; 16], pan: [-1; 16], left: vec![], right: vec![] })
    }

    /// Instrument, volume (0..127) and pan (-1..1) of a channel, sent only when they change.
    pub fn set_channel(&mut self, ch: u8, program: u8, volume: u8, pan: f32) {
        let c = (ch & 15) as usize;
        if c != 9 && self.program[c] != program as i32 {
            self.synth.process_midi_message(c as i32, 0xC0, program as i32, 0);
            self.program[c] = program as i32;
        }
        let v = volume.min(127) as i32;
        if self.volume[c] != v {
            self.synth.process_midi_message(c as i32, 0xB0, 7, v);
            self.volume[c] = v;
        }
        let p = (64.0 + pan.clamp(-1.0, 1.0) * 63.0).round() as i32;
        if self.pan[c] != p {
            self.synth.process_midi_message(c as i32, 0xB0, 10, p);
            self.pan[c] = p;
        }
    }

    pub fn note_on(&mut self, ch: u8, key: i32, vel: f32) {
        if !(0..128).contains(&key) {
            return;
        }
        let v = (vel * 127.0).round().clamp(1.0, 127.0) as i32;
        self.synth.note_on((ch & 15) as i32, key, v);
    }

    pub fn note_off(&mut self, ch: u8, key: i32) {
        if (0..128).contains(&key) {
            self.synth.note_off((ch & 15) as i32, key);
        }
    }

    pub fn controller(&mut self, ch: u8, cc: u8, val: u8) {
        self.synth.process_midi_message((ch & 15) as i32, 0xB0, cc as i32, val.min(127) as i32);
    }

    /// Stop: every note released (the natural release tails ring out), the pedal up.
    pub fn all_off(&mut self) {
        for c in 0..16 {
            self.synth.process_midi_message(c, 0xB0, 64, 0);
        }
        self.synth.note_off_all(false);
    }

    /// `frames` stereo samples into the synth's own buffers.
    pub fn render(&mut self, frames: usize) -> (&[f32], &[f32]) {
        if self.left.len() < frames {
            self.left.resize(frames, 0.0);
            self.right.resize(frames, 0.0);
        }
        let (l, r) = (&mut self.left[..frames], &mut self.right[..frames]);
        self.synth.render(l, r);
        (&self.left[..frames], &self.right[..frames])
    }
}

/// A metronome click on the GM drum kit: wood blocks (high = first beat of the bar, low = the others).
pub fn click_key(bar: bool) -> i32 {
    if bar {
        76
    } else {
        77
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn damaged_soundfonts_are_refused_without_breaking() {
        let junk: Vec<u8> = (0..20_000u32).map(|i| (i * 13 + 5) as u8).collect();
        let mut riff = b"RIFF\x10\0\0\0sfbkLIST".to_vec();
        riff.extend([0xff; 64]);
        for b in [&b""[..], b"RIFF", &junk, &riff] {
            assert!(parse(b).is_err());
        }
        // a real bank cut short
        if let Some(p) = find() {
            let bytes = std::fs::read(p).unwrap();
            for cut in [12, 1000, bytes.len() / 2] {
                assert!(parse(&bytes[..cut]).is_err(), "cut at {cut}");
            }
        }
    }

    fn rms(pcm: &[f32]) -> f32 {
        (pcm.iter().map(|x| x * x).sum::<f32>() / pcm.len().max(1) as f32).sqrt()
    }

    #[test]
    fn a_score_plays_with_the_bank_and_without_it() {
        use crate::audio::{build, render_offline, PlayNote};
        let mut a = crate::app::App::new(true);
        for (i, k) in [60, 64, 67, 72].into_iter().enumerate() {
            a.enter(0, 0, i as u32 * crate::model::TPQ, Some(crate::model::Pitch::from_midi(k, false)), 3, 0, false);
        }
        let (notes, _, _) = build(&a.score, 0);
        assert_eq!(notes.len(), 4);
        // without the bank: the built-in sound, as before
        let builtin = render_offline(notes.clone(), 44100.0, 0.7, None, 0.5);
        assert!(rms(&builtin) > 0.005);
        let Some(p) = find() else { return };
        let sf = parse(&std::fs::read(p).unwrap()).unwrap();
        let pcm = render_offline(notes, 44100.0, 0.7, Some(GmSynth::new(&sf, 44100.0).unwrap()), 0.5);
        // about as loud as the built-in sound (within 6 dB), never past full scale
        let (a, b) = (rms(&pcm), rms(&builtin));
        assert!(a > b / 2.0 && a < b * 2.0, "SoundFont {a} vs built-in {b}");
        assert!(pcm.iter().all(|x| x.abs() <= 1.0));
        // the drum kit and the click sound on channel 10
        let drums = vec![
            PlayNote { at: 0.0, len: 0.2, midi: 38, vel: 0.8, gain: 1.0, drum: true, channel: 9, ..Default::default() },
            PlayNote { at: 0.3, len: 0.03, midi: 96, vel: 0.8, gain: 1.0, drum: true, channel: 9, click: true, port: 2, ..Default::default() },
        ];
        let pcm = render_offline(drums, 44100.0, 0.7, Some(GmSynth::new(&sf, 44100.0).unwrap()), 0.3);
        let half = pcm.len() / 2;
        assert!(rms(&pcm[..(0.25 * 44100.0 * 2.0) as usize]) > 0.002, "snare");
        assert!(rms(&pcm[(0.3 * 44100.0 * 2.0) as usize..half.max((0.5 * 44100.0 * 2.0) as usize)]) > 0.001, "click");
    }

    #[test]
    fn the_bank_plays_a_note_when_it_is_there() {
        // the bank isn't in git (tools/fetch_soundfont.sh downloads it); without it this test has nothing to check
        let Some(p) = find() else { return };
        let sf = parse(&std::fs::read(p).unwrap()).unwrap();
        let mut g = GmSynth::new(&sf, 44100.0).unwrap();
        g.set_channel(0, 0, 100, 0.0);
        g.note_on(0, 60, 0.8);
        let (l, _) = g.render(4410);
        let peak = l.iter().fold(0f32, |a, x| a.max(x.abs()));
        assert!(peak > 0.01, "piano C4 is silent ({peak})");
        g.all_off();
    }
}
