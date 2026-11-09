//! MIDI in: every input the system has, all at once, into one `Played` for the spyglass. Ports that
//! come and go (a keyboard plugged in, a Bluetooth device connecting) are picked up on the next look.
//! Nothing is ever sent anywhere: the spyglass only listens.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use midir::{Ignore, MidiInput, MidiInputConnection};

pub const CLIENT: &str = "MIDI Spyglass";

/// What's been played: the last note and how hard, and the keys held now (for chord names); the sound each
/// channel last picked (Program Change), and which input and channel the last note came from.
pub struct Played {
    down: [bool; 128],
    /// How hard each held key was hit.
    vel: [u8; 128],
    /// The sound each input's channel last picked, by input name.
    program: std::collections::HashMap<String, [Option<u8>; 16]>,
    last_channel: u8,
    last_source: Option<String>,
    pub last_note: Option<u8>,
    last_velocity: u8,
    first_velocity: Option<u8>,
    velocity_varies: bool,
    pub note_ons: u64,
}

impl Default for Played {
    fn default() -> Played {
        Played { down: [false; 128], vel: [0; 128], program: Default::default(), last_channel: 0, last_source: None, last_note: None, last_velocity: 0, first_velocity: None, velocity_varies: false, note_ons: 0 }
    }
}

impl Played {
    fn event(&mut self, status: u8, note: u8, value: u8, source: &str) {
        let note = note & 127;
        match status & 0xF0 {
            0xC0 => self.program.entry(source.to_string()).or_default()[(status & 15) as usize] = Some(note),
            0x90 if value > 0 => {
                self.last_channel = status & 15;
                if self.last_source.as_deref() != Some(source) {
                    self.last_source = Some(source.to_string());
                }
                self.down[note as usize] = true;
                self.vel[note as usize] = value;
                self.last_note = Some(note);
                self.last_velocity = value;
                match self.first_velocity {
                    None => self.first_velocity = Some(value),
                    Some(v) if v != value => self.velocity_varies = true,
                    _ => {}
                }
                self.note_ons += 1;
            }
            0x80 | 0x90 => self.down[note as usize] = false,
            0xB0 if note == 123 || note == 120 => self.down = [false; 128],
            _ => {}
        }
    }

    /// What's playing: the sound the last note's channel picked, if a Program Change said; else the input it came from.
    pub fn sound(&self) -> Option<String> {
        let source = self.last_source.as_ref()?;
        match self.program.get(source).and_then(|p| p[self.last_channel as usize]) {
            Some(p) => Some(crate::gm::sound_name(self.last_channel, p)),
            None => Some(source.clone()),
        }
    }

    /// How hard the last key was hit; none until two different ones came (some keyboards always send the same).
    pub fn velocity(&self) -> Option<u8> {
        self.velocity_varies.then_some(self.last_velocity)
    }

    /// The keys held now and how hard each was hit (all the same, 100, from a keyboard that doesn't send it).
    pub fn held(&self) -> Vec<(u8, u8)> {
        (0..128u8).filter(|&n| self.down[n as usize]).map(|n| (n, if self.velocity_varies { self.vel[n as usize] } else { 100 })).collect()
    }

    pub fn keys_down(&self) -> Vec<u8> {
        (0..128u8).filter(|&n| self.down[n as usize]).collect()
    }
}

/// One input's running status; SysEx passes through untouched. `source`: the input's name.
#[derive(Default)]
pub struct Parser {
    pub source: String,
    status: u8,
    d1: u8,
    have: bool,
}

impl Parser {
    pub fn feed(&mut self, bytes: &[u8], played: &mut Played) {
        for &b in bytes {
            if b >= 0xF8 {
                continue; // clock and the like
            }
            if b >= 0x80 {
                self.status = if b < 0xF0 { b } else { 0 };
                self.have = false;
                continue;
            }
            let kind = self.status & 0xF0;
            if self.status == 0 || kind == 0xD0 {
                continue; // nothing to track
            }
            if kind == 0xC0 {
                played.event(self.status, b, 0, &self.source); // one data byte: the sound picked
                continue;
            }
            if !self.have {
                self.d1 = b;
                self.have = true;
            } else {
                self.have = false; // running status: the next pair reuses it
                played.event(self.status, self.d1, b, &self.source);
            }
        }
    }
}

/// The inputs there are now, by name.
pub fn ports() -> Vec<String> {
    let Ok(input) = MidiInput::new(CLIENT) else { return vec![] };
    input.ports().iter().filter_map(|p| input.port_name(p).ok()).filter(|n| !n.contains(CLIENT)).collect()
}

/// The open inputs. `skip`: names the user turned off.
#[derive(Default)]
pub struct Inputs {
    open: BTreeMap<String, MidiInputConnection<()>>,
    pub skip: BTreeSet<String>,
}

impl Inputs {
    /// Opens what's new, closes what's gone or turned off; `on_note` is called (from MIDI threads) after each message.
    pub fn refresh(&mut self, played: &Arc<Mutex<Played>>, on_note: &(impl Fn() + Clone + Send + 'static)) {
        let now = ports();
        self.open.retain(|name, _| now.contains(name) && !self.skip.contains(name));
        for name in now {
            if self.open.contains_key(&name) || self.skip.contains(&name) {
                continue;
            }
            if let Some(c) = connect(&name, played.clone(), on_note.clone()) {
                self.open.insert(name, c);
            }
        }
    }

    pub fn names(&self) -> Vec<String> {
        self.open.keys().cloned().collect()
    }
}

fn connect(name: &str, played: Arc<Mutex<Played>>, on_note: impl Fn() + Send + 'static) -> Option<MidiInputConnection<()>> {
    let mut input = MidiInput::new(CLIENT).ok()?;
    input.ignore(Ignore::All); // no SysEx, clock or active sensing: notes are all the spyglass reads
    let port = input.ports().into_iter().find(|p| input.port_name(p).is_ok_and(|n| n == name))?;
    let mut parser = Parser { source: name.to_string(), ..Default::default() };
    input
        .connect(&port, "MIDI Spyglass in", move |_, bytes, _| {
            parser.feed(bytes, &mut played.lock().unwrap());
            on_note();
        }, ())
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracks_notes_and_chords() {
        let (mut p, mut played) = (Parser::default(), Played::default());
        p.feed(&[0x90, 60, 90, 64, 90, 67, 90], &mut played); // running status
        assert_eq!(played.keys_down(), vec![60, 64, 67]);
        assert_eq!(played.last_note, Some(67));
        assert_eq!(played.velocity(), None); // all the same so far
        p.feed(&[0x80, 64, 0, 0x90, 72, 40], &mut played);
        assert_eq!(played.keys_down(), vec![60, 67, 72]);
        assert_eq!(played.velocity(), Some(40));
        p.feed(&[0xF0, 0x7D, 1, 0, 0xF7], &mut played);
        assert_eq!(played.note_ons, 4);
    }

    #[test]
    fn names_the_sound() {
        let (mut p, mut played) = (Parser { source: "Keystation 49".into(), ..Default::default() }, Played::default());
        p.feed(&[0x90, 60, 90], &mut played);
        assert_eq!(played.sound().as_deref(), Some("Keystation 49"), "no Program Change yet: the input");
        p.feed(&[0xC0, 4, 0x90, 62, 90], &mut played);
        assert_eq!(played.sound().as_deref(), Some("Electric Piano 1 · 5"));
        p.feed(&[0x99, 36, 100], &mut played);
        assert_eq!(played.sound().as_deref(), Some("Keystation 49"), "channel 10 picked nothing");
        let mut other = Parser { source: "Other".into(), ..Default::default() };
        other.feed(&[0x90, 60, 90], &mut played);
        assert_eq!(played.sound().as_deref(), Some("Other"), "another input's sound isn't this one's");
    }
}
