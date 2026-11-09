//! The spyglass: the last note or chord played on any MIDI input, in a round brass eyepiece (from FM-1
//! Mirror's Spyglass, after Synth Spyglass). The note name big, the notes on a grand staff, the full
//! name and pitch, and how hard the key was hit.

use std::time::Instant;

use eframe::egui::{self, vec2, Color32, CornerRadius, FontId, Painter, Pos2, Rect, Stroke};

use crate::chord::{chord_name, interval_name, is_pair, note_hz, note_name, CHORD_SETTLE, NOTE_NAMES, PAIR_EXTRA};
use crate::midi::Played;
use crate::staff;

const BRASS: Color32 = Color32::from_rgb(0xc9, 0x9a, 0x45);
const BRASS_LIGHT: Color32 = Color32::from_rgb(0xf3, 0xd6, 0x8a);
const BRASS_DARK: Color32 = Color32::from_rgb(0x8a, 0x62, 0x24);
const BRASS_SHADOW: Color32 = Color32::from_rgb(0x3e, 0x2a, 0x0e);
const GLASS: Color32 = Color32::from_rgb(0x0e, 0x14, 0x1a);
const TEXT: Color32 = Color32::WHITE;
/// The sound's name on top: warm gold letters on smoked glass with a faint shine.
const SOUND: Color32 = Color32::from_rgb(0xf7, 0xdc, 0x92);
const SOUND_GLASS: Color32 = Color32::from_rgba_premultiplied(14, 11, 5, 225);
const SOUND_SHINE: Color32 = Color32::from_rgba_premultiplied(10, 9, 6, 18);
const WAVE: Color32 = Color32::from_rgb(80, 200, 255);
const WAVE_QUIET: Color32 = Color32::from_rgb(110, 120, 130);

/// The resize grip's place on the rim, lower right, for a spyglass centred on `c`, `d` across.
pub fn grip_at(c: Pos2, d: f32) -> Pos2 {
    c + vec2(1.0, 1.0) * (d / 2.0 * std::f32::consts::FRAC_1_SQRT_2)
}

/// The resize grip: a brass stud with two diagonal grooves.
pub fn draw_grip(p: &Painter, at: Pos2, hot: bool) {
    p.circle(at, 12.0, if hot { BRASS_LIGHT } else { BRASS }, Stroke::new(1.5_f32, BRASS_SHADOW));
    for k in [-3.0, 3.0] {
        let o = vec2(k, k);
        p.line_segment([at + o + vec2(-4.0, 4.0), at + o + vec2(4.0, -4.0)], Stroke::new(1.5_f32, BRASS_SHADOW));
    }
}

enum Shown {
    Note(u8, Option<u8>),
    Chord { keys: Vec<u8>, name: String, held: String },
}

/// What the spyglass shows, kept up to date from `Played` (Synth Spyglass's chord rule: name it
/// once the keys stay put).
#[derive(Default)]
pub struct Glass {
    shown: Option<Shown>,
    seen: u64,
    armed: bool,
    settling: Vec<u8>,
    since: Option<Instant>,
    released: Option<Instant>,
}

/// How long a note or chord stays after the last key comes up, then the glass is empty again.
const HOLD: f64 = 3.0;

impl Glass {
    /// True when there's something new to draw.
    pub fn tick(&mut self, played: &Played) -> bool {
        let mut changed = false;
        if played.note_ons != self.seen {
            self.seen = played.note_ons;
            if let Some(n) = played.last_note {
                self.shown = Some(Shown::Note(n, played.velocity()));
                self.armed = true;
                changed = true;
            }
        }
        if self.armed {
            let keys = played.keys_down();
            if keys != self.settling {
                self.settling = keys;
                self.since = Some(Instant::now());
            } else if self.since.is_some_and(|t| t.elapsed().as_secs_f64() >= CHORD_SETTLE + if is_pair(&keys) { PAIR_EXTRA } else { 0.0 }) {
                self.armed = false;
                if let Some(name) = chord_name(&keys) {
                    let mut held = keys.iter().map(|&n| note_name(n)).collect::<Vec<_>>().join("  ");
                    if let Some(i) = interval_name(&keys) {
                        held += &format!("   ·   {i}");
                    }
                    self.shown = Some(Shown::Chord { keys, name, held });
                    changed = true;
                }
            }
        }
        if self.shown.is_none() || !played.keys_down().is_empty() {
            self.released = None;
        } else if self.released.get_or_insert_with(Instant::now).elapsed().as_secs_f64() >= HOLD {
            self.shown = None;
            self.released = None;
            changed = true;
        }
        changed
    }

    /// Still settling a chord, or waiting to empty the glass: draw again soon.
    pub fn busy(&self) -> bool {
        self.armed || self.released.is_some()
    }

    /// The eyepiece centred on `c`, `d` across. `idle`: what it says while nothing has been played.
    /// `sound`: what's playing, across the top. `wave`: the waveform beside the staff.
    pub fn draw(&self, p: &Painter, c: Pos2, d: f32, idle: &str, sound: Option<&str>, wave: bool) {
        lens(p, c, d / 2.0, d);
        if let Some(sound) = sound {
            // The sound's name across the top of the glass: warm gold letters, a little spaced, on a rounded band of
            // dark smoked glass with a thin brass edge (the rim's), a soft highlight on its upper half.
            let room = d * 0.5;
            let job = |size: f32, color: Color32| {
                let mut j = egui::text::LayoutJob::default();
                j.append(&format!("\u{266a}  {sound}"), 0.0, egui::TextFormat {
                    font_id: FontId::proportional(size),
                    color,
                    extra_letter_spacing: size * 0.06,
                    ..Default::default()
                });
                j
            };
            let mut size = (d * 0.06).max(12.0);
            let w = p.fonts(|f| f.layout_job(job(size, SOUND))).size().x;
            if w > room {
                size *= room / w;
            }
            let g = p.fonts(|f| f.layout_job(job(size, SOUND)));
            let at = c + vec2(0.0, -d * 0.325);
            let band = Rect::from_center_size(at, vec2(g.size().x + size * 1.4, g.size().y + size * 0.5));
            let round = CornerRadius::same((band.height() / 2.0).min(255.0) as u8);
            p.rect_filled(band.translate(vec2(0.0, 2.0)), round, Color32::from_black_alpha(90));   // its shadow
            p.rect_filled(band, round, SOUND_GLASS);
            let upper = Rect::from_min_max(band.min + vec2(2.0, 2.0), Pos2::new(band.max.x - 2.0, band.center().y));
            p.rect_filled(upper, CornerRadius::same(((upper.height()).min(255.0)) as u8), SOUND_SHINE);
            p.rect_stroke(band, round, Stroke::new(1.25_f32, BRASS_LIGHT), egui::StrokeKind::Inside);
            p.rect_stroke(band.shrink(1.25), round, Stroke::new(1.0_f32, Color32::from_black_alpha(140)), egui::StrokeKind::Inside);
            let top_left = at - g.size() / 2.0;
            p.galley(top_left + vec2(0.0, 1.0), p.fonts(|f| f.layout_job(job(size, Color32::from_black_alpha(200)))), Color32::BLACK);
            p.galley(top_left + vec2(0.45, 0.0), g.clone(), SOUND);    // (twice, a hair apart: a touch bolder)
            p.galley(top_left, g, SOUND);
        }
        let fit = |text: String, size: f32, room: f32| {
            let g = p.layout_no_wrap(text.clone(), FontId::proportional(size), TEXT);
            if g.size().x <= room { g } else { p.layout_no_wrap(text, FontId::proportional(size * room / g.size().x), TEXT) }
        };
        let (name, detail, keys, vel) = match &self.shown {
            None => (String::new(), idle.to_string(), vec![], None),
            Some(Shown::Note(n, vel)) => {
                let mut detail = format!("{}   {:.0} Hz", note_name(*n), note_hz(*n));
                if let Some(v) = vel {
                    detail += &format!("   vel {v}");
                }
                (NOTE_NAMES[(*n % 12) as usize].to_string(), detail, vec![*n], *vel)
            }
            Some(Shown::Chord { keys, name, held }) => (name.clone(), held.clone(), keys.clone(), None),
        };
        // The staff is what's read: the name small above it, the staff as big as the glass allows. With nothing
        // played the staves stay, empty, over "play a note".
        let size = if matches!(self.shown, Some(Shown::Chord { .. })) { d * 0.10 } else { d * 0.115 };
        if !name.is_empty() {
            let g = fit(name, size, d * 0.72);
            p.galley(c + vec2(0.0, -d * 0.215) - g.size() / 2.0, g, TEXT);
        }
        let w = d * if wave { 0.42 } else { 0.45 };
        let staff_x = if wave { -d * 0.165 } else { 0.0 };
        staff::draw(p, Rect::from_center_size(c + vec2(staff_x, d * 0.087), vec2(w, w * 1.06)), &keys, GLASS, TEXT);
        if wave {
            // The notes' waveform, right of the staff; a flat grey line while nothing is showing.
            let r = Rect::from_min_max(c + vec2(d * 0.075, -d * 0.02), c + vec2(d * 0.355, d * 0.19));
            let width = (d * 0.006).max(2.0);
            match waveform(&keys) {
                Some(w) => {
                    let at = |i: usize, s: f32| Pos2::new(r.left() + r.width() * i as f32 / (w.len() - 1) as f32, r.center().y - s * r.height() / 2.0);
                    let line = w.iter().enumerate().map(|(i, &s)| at(i, s)).collect();
                    p.add(egui::Shape::line(line, Stroke::new(width, WAVE)));
                }
                None => { p.line_segment([r.left_center(), r.right_center()], Stroke::new(width, WAVE_QUIET)); }
            }
        }
        let g = fit(detail, (d * 0.055).max(12.0), d * 0.54);
        p.galley(c + vec2(0.0, d * 0.365) - g.size() / 2.0, g, TEXT);
        // How hard the key was hit: a short brass bar under it, full at 127.
        if let Some(v) = vel {
            let (w, h) = (d * 0.26, (d * 0.012).max(3.0));
            let track = Rect::from_center_size(c + vec2(0.0, d * 0.405), vec2(w, h));
            p.rect_filled(track, CornerRadius::same(2), BRASS_SHADOW);
            p.rect_filled(Rect::from_min_size(track.min, vec2(w * v as f32 / 127.0, h)), CornerRadius::same(2), BRASS_LIGHT);
        }
    }
}

/// How many points the waveform is drawn with.
const WAVE_POINTS: usize = 240;

/// The notes' sound as a scope would show it, made from MIDI alone: a sine at each note's pitch, added up (a
/// chord's beating shape), over four periods of the lowest note, starting at zero going up. Scaled so the
/// highest point is ±1; none for no notes.
pub fn waveform(keys: &[u8]) -> Option<Vec<f32>> {
    let low = note_hz(*keys.iter().min()?);
    let span = 4.0 / low;
    let w: Vec<f32> = (0..WAVE_POINTS)
        .map(|i| {
            let t = span * i as f64 / (WAVE_POINTS - 1) as f64;
            keys.iter().map(|&n| (std::f64::consts::TAU * note_hz(n) * t).sin()).sum::<f64>() as f32
        })
        .collect();
    let peak = w.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    (peak > 0.0).then(|| w.iter().map(|s| s / peak).collect())
}

/// A spyglass's eyepiece: see-through dark glass, darker toward the edge (no dot of light at 11 o'clock: nothing on the glass),
/// and a knurled brass rim lit from the top left (Synth Spyglass's lens()).
fn lens(p: &Painter, c: Pos2, r: f32, d: f32) {
    use std::f32::consts::TAU;
    let rim = (d * 0.05).max(7.0);
    let glass_r = r - rim;
    p.circle_filled(c, glass_r, Color32::from_rgba_unmultiplied(0x0e, 0x14, 0x1a, 235));
    for i in 0..8 {
        let t = i as f32 / 8.0;
        let w = glass_r * 0.035;
        p.circle_stroke(c, glass_r - w * (i as f32 + 0.5), Stroke::new(w, Color32::from_rgba_unmultiplied(0, 0, 0, (60.0 * (1.0 - t)) as u8)));
    }
    let arc = |radius: f32, from: f32, to: f32| -> Vec<Pos2> {
        (0..=24).map(|k| from + (to - from) * k as f32 / 24.0).map(|a| c + vec2(a.cos(), a.sin()) * radius).collect()
    };
    let bands = [BRASS_SHADOW, BRASS_DARK, BRASS, BRASS_LIGHT, BRASS, BRASS_DARK];
    let w = rim / bands.len() as f32;
    for (i, col) in bands.iter().enumerate() {
        p.circle_stroke(c, glass_r + w * (i as f32 + 0.5), Stroke::new(w + 0.6, *col));
    }
    p.add(egui::Shape::line(arc(r - rim * 0.45, TAU * 0.55, TAU * 0.8), Stroke::new(rim * 0.35, Color32::from_rgba_unmultiplied(255, 240, 200, 70))));
    p.add(egui::Shape::line(arc(r - rim * 0.5, TAU * 0.05, TAU * 0.3), Stroke::new(rim * 0.5, Color32::from_rgba_unmultiplied(40, 25, 5, 90))));
    let ticks = ((r * TAU) / (d * 0.025).max(5.0)) as usize;
    for k in 0..ticks {
        let a = TAU * k as f32 / ticks as f32;
        let dir = vec2(a.cos(), a.sin());
        p.line_segment([c + dir * (r - rim * 0.42), c + dir * (r - 0.5)], Stroke::new((d * 0.004).max(1.0), Color32::from_rgba_unmultiplied(40, 25, 5, 130)));
    }
    p.circle_stroke(c, glass_r, Stroke::new((d * 0.004).max(1.0), BRASS_SHADOW));
    p.circle_stroke(c, r - 0.5, Stroke::new(1.0_f32, BRASS_SHADOW));
}

#[cfg(test)]
mod wave_tests {
    use super::*;

    #[test]
    fn one_note_is_four_periods_of_a_sine() {
        let w = waveform(&[69]).unwrap(); // A4
        assert_eq!(w.len(), WAVE_POINTS);
        assert!(w[0].abs() < 1e-6 && w[WAVE_POINTS - 1].abs() < 0.01, "starts and ends at zero");
        assert!((w.iter().cloned().fold(f32::MIN, f32::max) - 1.0).abs() < 1e-6);
        assert!(waveform(&[]).is_none());
        let chord = waveform(&[60, 64, 67]).unwrap();
        assert!(chord.iter().all(|s| s.abs() <= 1.0));
    }
}

#[cfg(test)]
mod hold_tests {
    use super::*;
    use crate::midi::{Parser, Played};
    use std::time::Duration;

    #[test]
    fn the_last_note_goes_after_the_keys_are_up() {
        let (mut p, mut played, mut g) = (Parser::default(), Played::default(), Glass::default());
        p.feed(&[0x90, 60, 90], &mut played);
        g.tick(&played);
        assert!(g.shown.is_some());
        assert!(g.released.is_none(), "a held key stays");
        p.feed(&[0x80, 60, 0], &mut played);
        g.tick(&played);
        assert!(g.shown.is_some() && g.busy(), "it stays a while, and the glass keeps looking");
        g.released = Some(Instant::now() - Duration::from_secs_f64(HOLD + 0.1));
        assert!(g.tick(&played));
        assert!(g.shown.is_none());
        p.feed(&[0x90, 64, 90], &mut played);
        g.tick(&played);
        assert!(g.shown.is_some() && g.released.is_none(), "a new note shows again");
    }
}
