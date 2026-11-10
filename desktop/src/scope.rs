//! A synth's real sound, for the spyglass's waveform (as in MIDI Bridge's and FM-1 Mirror's spyglass): an audio
//! input, the FM-1's own USB audio by default when it's plugged in, or one chosen from the menu. On a Mac the first
//! listen asks for the microphone: any audio input counts as one.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use std::io::Read;
use std::process::{Child, Command, Stdio};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

/// An FM-1's USB audio has one of these in its name: picked by itself.
const HINTS: [&str; 9] = ["fm-1", "fm1", "jieli", "usb-audio", "usb composite", "felucca", "melodee", "choralroot", "jiant"];
/// Samples kept: plenty for the window plus a period to find the trigger in.
const KEEP: usize = 8192;
/// How much of the sound is shown, in seconds: the same for every note.
const WINDOW: f32 = 0.02;
/// No sound for this long: it has gone.
const STALE: Duration = Duration::from_secs(1);

pub struct Ring {
    buf: Vec<f32>,
    head: usize,
    filled: u64,
    last: Option<Instant>,
    rate: f32,
}

impl Default for Ring {
    fn default() -> Ring {
        Ring { buf: vec![0.0; KEEP], head: 0, filled: 0, last: None, rate: 48000.0 }
    }
}

impl Ring {
    fn push(&mut self, sample: f32) {
        self.buf[self.head] = sample;
        self.head = (self.head + 1) % KEEP;
        self.filled += 1;
    }

    fn at(&self, i: usize) -> f32 {
        self.buf[(self.head + i) % KEEP]
    }

    /// Hearing it now.
    pub fn listening(&self) -> bool {
        self.last.is_some_and(|t| t.elapsed() < STALE)
    }

    /// The last WINDOW of the sound, starting where the wave crosses zero going up (searched within one period at
    /// `hz`, the note played), so it stands still like an oscilloscope's. Scaled so the loudest point is ±1; none
    /// while it's silent.
    pub fn wave(&self, hz: f64) -> Option<Vec<f32>> {
        if !self.listening() {
            return None;
        }
        let span = ((WINDOW * self.rate).round() as usize).clamp(32, KEEP / 2);
        if self.filled < span as u64 * 2 {
            return None;
        }
        let period = (self.rate as f64 / hz.max(30.0)) as usize;
        let last_start = KEEP - span;
        let first = last_start.saturating_sub(period + 1).max(1);
        let start = (first..=last_start).rev().find(|&i| self.at(i - 1) < 0.0 && self.at(i) >= 0.0).unwrap_or(last_start);
        let mut w: Vec<f32> = (0..span).map(|i| self.at(start + i)).collect();
        let peak = w.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        if peak < 0.002 {
            return None;
        }
        w.iter_mut().for_each(|s| *s /= peak);
        Some(w)
    }

    /// `wave`, with how loud it really was: layered, the spyglass shows whichever synth is louder.
    pub fn wave_and_peak(&self, hz: f64) -> Option<(f32, Vec<f32>)> {
        let w = self.wave(hz)?;
        let span = w.len();
        let start = KEEP - span;
        let peak = (start..KEEP).fold(0.0f32, |m, i| m.max(self.at(i).abs()));
        Some((peak, w))
    }
}

/// The audio inputs there are now, by name.
pub fn devices() -> Vec<String> {
    cpal::default_host().input_devices().map(|ds| ds.filter_map(|d| d.name().ok()).collect()).unwrap_or_default()
}

/// Everything about audio happens on one thread of its own: listing the inputs, opening one, listening. Opening
/// can wait a long time (macOS asking for the microphone, a busy device), and while it does every other audio call
/// in the app waits too, so the spyglass never makes one: it sets what it wants here and reads what's heard.
#[derive(Default)]
pub struct Audio {
    /// What the spyglass wants: the waveform on, and the choice (`wanted`).
    pub want: Mutex<(bool, Option<String>)>,
    /// The audio inputs there are, and the one listened to now.
    pub devices: Mutex<Vec<String>>,
    pub open: Mutex<Option<String>>,
}

/// Starts the audio thread; it looks at what's wanted twice a second.
pub fn start(ring: Arc<Mutex<Ring>>) -> Arc<Audio> {
    let audio = Arc::new(Audio::default());
    let a = audio.clone();
    let _ = std::thread::Builder::new().name("MIDI Spyglass sound".into()).spawn(move || {
        let mut stream: Option<(String, cpal::Stream)> = None;
        let mut dead = Arc::new(AtomicBool::new(false));
        let mut tried: Option<(String, Instant)> = None;
        let (mut listed, mut last_asked): (Option<Instant>, Option<(bool, Option<String>)>) = (None, None);
        loop {
            let (on, choice) = a.want.lock().unwrap().clone();
            // Listing the inputs is real Core Audio work: look every 3 s, or at once when what's wanted changes or the
            // input went.
            let asked = (on, choice.clone());
            let due = listed.is_none_or(|t: Instant| t.elapsed() > Duration::from_secs(3));
            if !due && last_asked.as_ref() == Some(&asked) && !(stream.is_some() && dead.load(Ordering::Relaxed)) {
                std::thread::sleep(Duration::from_millis(250));
                continue;
            }
            listed = Some(Instant::now());
            last_asked = Some(asked);
            let found = devices();
            *a.devices.lock().unwrap() = found.clone();
            let want = if !on {
                None
            } else {
                match choice.as_deref() {
                    Some("") => None,
                    Some(name) => found.iter().find(|n| *n == name).cloned(),
                    None => found.iter().find(|n| is_fm1(n)).cloned(),
                }
            };
            if stream.as_ref().map(|(n, _)| n) != want.as_ref() || dead.load(Ordering::Relaxed) {
                stream = None;
                *a.open.lock().unwrap() = None;
                // (one that failed is tried again every 5 s, not every look)
                let again = tried.as_ref().is_none_or(|(n, t)| Some(n) != want.as_ref() || t.elapsed() > Duration::from_secs(5));
                if let (Some(name), true) = (want, again) {
                    dead = Arc::new(AtomicBool::new(false));
                    tried = Some((name.clone(), Instant::now()));
                    if let Some(s) = open_stream(ring.clone(), &name, dead.clone()) {
                        *a.open.lock().unwrap() = Some(name.clone());
                        stream = Some((name, s));
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(250));
        }
    });
    audio
}

/// What the header calls an input's sound: "FM-1" for an FM-1's USB audio (as MIDI Bridge's spyglass), else its name.
pub fn label(name: &str) -> String {
    if is_fm1(name) { "FM-1".to_string() } else { name.to_string() }
}

fn is_fm1(name: &str) -> bool {
    let n = name.to_lowercase();
    HINTS.iter().any(|h| n.contains(h))
}

fn feed<T: Copy>(ring: &Mutex<Ring>, data: &[T], channels: usize, sample: impl Fn(T) -> f32) {
    if let Ok(mut r) = ring.lock() {
        for frame in data.chunks(channels.max(1)) {
            r.push(sample(frame[0]));
        }
        if !data.is_empty() {
            r.last = Some(Instant::now());
        }
    }
}

/// The input stream, playing; none if it isn't there or isn't allowed. Errors later set `dead`.
fn open_stream(ring: Arc<Mutex<Ring>>, name: &str, dead: Arc<AtomicBool>) -> Option<cpal::Stream> {
    let device = cpal::default_host().input_devices().ok()?.find(|d| d.name().is_ok_and(|n| n == name))?;
    let config = device.default_input_config().ok()?;
    let channels = config.channels() as usize;
    ring.lock().ok()?.rate = config.sample_rate().0 as f32;
    let on_error = move |_| dead.store(true, Ordering::Relaxed);
    let stream_config = config.config();
    let r = ring.clone();
    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => device.build_input_stream(&stream_config, move |d: &[f32], _: &_| feed(&r, d, channels, |s| s), on_error, None),
        cpal::SampleFormat::I16 => device.build_input_stream(&stream_config, move |d: &[i16], _: &_| feed(&r, d, channels, |s| s as f32 / 32768.0), on_error, None),
        cpal::SampleFormat::I32 => device.build_input_stream(&stream_config, move |d: &[i32], _: &_| feed(&r, d, channels, |s| s as f32 / 2147483648.0), on_error, None),
        _ => return None,
    }
    .ok()?;
    stream.play().ok()?;
    Some(stream)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wave_starts_rising_through_zero_and_fills_the_height() {
        let mut r = Ring::default();
        for i in 0..KEEP {
            r.push(0.25 * (i as f32 * 440.0 * std::f32::consts::TAU / 48000.0).sin());
        }
        r.last = Some(Instant::now());
        let w = r.wave(440.0).unwrap();
        assert_eq!(w.len(), 960);
        assert!(w[0].abs() < 0.1 && w[1] > w[0]);
        assert!((w.iter().fold(0.0f32, |m, s| m.max(s.abs())) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn silence_or_nothing_heard_is_no_wave() {
        let mut r = Ring::default();
        assert!(r.wave(440.0).is_none());
        r.filled = KEEP as u64;
        r.last = Some(Instant::now());
        assert!(r.wave(440.0).is_none());
    }

    #[test]
    fn finds_the_fm1_by_its_name() {
        assert!(is_fm1("Melodee FM-1") && is_fm1("USB-Audio") && !is_fm1("AIR 192 6") && !is_fm1("HD Webcam eMeet C960"));
    }
}

/// The soft synths' sound (Analog Lab V, CS-80 V…), from Synth Spyglass's helper `synth-tap` (helper/synth-tap.swift),
/// which sits next to the app's executable (Mac, macOS 14.2+). A Core Audio process tap listens to just those apps,
/// not a video playing beside them. The first time, macOS asks whether the app may record other apps' audio.
/// Its stdout: chunks of [u32 rate][u32 count][count × f32 mono], little-endian; rate 0: [count bytes], the names of
/// the apps it's listening to. Dropping it stops the helper.
pub struct SynthTap {
    pub ring: Arc<Mutex<Ring>>,
    child: Child,
    dead: Arc<AtomicBool>,
}

impl SynthTap {
    /// None when there's no helper (not a Mac build) or it can't run (an older macOS).
    pub fn start() -> Option<SynthTap> {
        let helper = std::env::current_exe().ok()?.with_file_name("synth-tap");
        if !helper.is_file() {
            return None;
        }
        let mut child = Command::new(&helper).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().ok()?;
        let mut out = child.stdout.take()?;
        let ring = Arc::new(Mutex::new(Ring::default()));
        let dead = Arc::new(AtomicBool::new(false));
        let (r, gone) = (ring.clone(), dead.clone());
        std::thread::Builder::new()
            .name("MIDI Spyglass synth-tap".into())
            .spawn(move || {
                let (mut head, mut body) = ([0u8; 8], Vec::new());
                while out.read_exact(&mut head).is_ok() {
                    let rate = u32::from_le_bytes(head[..4].try_into().unwrap());
                    let count = u32::from_le_bytes(head[4..].try_into().unwrap()) as usize;
                    if rate == 0 {
                        body.resize(count.min(4096), 0); // (the apps' names: the header says them already)
                        if out.read_exact(&mut body).is_err() {
                            break;
                        }
                        continue;
                    }
                    body.resize(count * 4, 0);
                    if count > KEEP || out.read_exact(&mut body).is_err() {
                        break;
                    }
                    if let Ok(mut ring) = r.lock() {
                        ring.rate = rate as f32;
                        for b in body.chunks_exact(4) {
                            ring.push(f32::from_le_bytes(b.try_into().unwrap()));
                        }
                        if count > 0 {
                            ring.last = Some(Instant::now());
                        }
                    }
                }
                gone.store(true, Ordering::Relaxed); // the helper ended: start it again later
            })
            .ok()?;
        Some(SynthTap { ring, child, dead })
    }

    pub fn dead(&self) -> bool {
        self.dead.load(Ordering::Relaxed)
    }
}

impl Drop for SynthTap {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
