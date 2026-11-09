//! Note, chord and interval names (from Synth Spyglass; the Android apps' Chord.java).

use std::collections::BTreeSet;

pub const NOTE_NAMES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];

/// Intervals above the root (semitones), and the suffix printed after the root.
const CHORD_SHAPES: &[(&[u8], &str)] = &[
    (&[0, 4, 7], " major"), (&[0, 3, 7], " minor"), (&[0, 3, 6], "dim"), (&[0, 4, 8], "aug"),
    (&[0, 2, 7], "sus2"), (&[0, 5, 7], "sus4"),
    (&[0, 4, 7, 10], "7"), (&[0, 4, 7, 11], "maj7"), (&[0, 3, 7, 10], "m7"), (&[0, 3, 6, 10], "m7b5"),
    (&[0, 3, 6, 9], "dim7"), (&[0, 3, 7, 11], "m(maj7)"), (&[0, 4, 7, 9], "6"), (&[0, 3, 7, 9], "m6"),
    (&[0, 5, 7, 10], "7sus4"), (&[0, 2, 4, 7], "add9"), (&[0, 2, 3, 7], "m(add9)"),
    (&[0, 2, 4, 7, 10], "9"), (&[0, 2, 4, 7, 11], "maj9"), (&[0, 2, 3, 7, 10], "m9"),
    (&[0, 4, 10], "7"), (&[0, 4, 11], "maj7"), (&[0, 3, 10], "m7"), // sevenths with the fifth left out
];

/// Semitones from the lower note up to the higher, within an octave.
const INTERVALS: [&str; 12] = ["octave", "minor 2nd", "major 2nd", "minor 3rd", "major 3rd", "4th",
    "tritone", "5th", "minor 6th", "major 6th", "minor 7th", "major 7th"];

/// Keys must stay put this long (seconds) before the chord is named…
pub const CHORD_SETTLE: f64 = 0.1;
/// …and two keys longer still: often it's only legato playing.
pub const PAIR_EXTRA: f64 = 0.2;

fn pitch_classes(notes: &[u8]) -> BTreeSet<u8> {
    notes.iter().map(|n| n % 12).collect()
}

/// (lower, upper) pitch classes if these keys are an interval (two notes, or one in octaves).
fn pair(notes: &[u8]) -> Option<(u8, u8)> {
    let pcs = pitch_classes(notes);
    if notes.len() < 2 || pcs.len() > 2 {
        return None;
    }
    let bass = notes.iter().min()? % 12;
    let other = pcs.iter().copied().filter(|&p| p != bass).min().unwrap_or(bass);
    Some((bass, other))
}

pub fn is_pair(notes: &[u8]) -> bool {
    pair(notes).is_some()
}

/// "minor 3rd" for E3 G3, "5th" for C3 G3 C4, "octave" for C3 C4.
pub fn interval_name(notes: &[u8]) -> Option<&'static str> {
    pair(notes).map(|(lo, hi)| INTERVALS[((hi + 12 - lo) % 12) as usize])
}

/// "C major", "C major / E", "Am7", "E + G" for an interval; None if it isn't a chord it knows.
pub fn chord_name(notes: &[u8]) -> Option<String> {
    if let Some((lo, hi)) = pair(notes) {
        return Some(format!("{} + {}", NOTE_NAMES[lo as usize], NOTE_NAMES[hi as usize]));
    }
    let pcs = pitch_classes(notes);
    if pcs.len() < 3 {
        return None;
    }
    let bass = notes.iter().min()? % 12;
    let mut best = None;
    for &root in &pcs {
        for (shape, suffix) in CHORD_SHAPES {
            let set: BTreeSet<u8> = shape.iter().map(|i| (root + i) % 12).collect();
            if set != pcs {
                continue;
            }
            let name = format!("{}{}", NOTE_NAMES[root as usize], suffix);
            if root == bass {
                return Some(name); // root position wins outright
            }
            if best.is_none() {
                best = Some(format!("{} / {}", name, NOTE_NAMES[bass as usize]));
            }
        }
    }
    best
}

pub fn note_name(n: u8) -> String {
    format!("{}{}", NOTE_NAMES[(n % 12) as usize], n as i32 / 12 - 1)
}

pub fn note_hz(n: u8) -> f64 {
    440.0 * 2f64.powf((n as f64 - 69.0) / 12.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!(chord_name(&[60, 64, 67]).as_deref(), Some("C major"));
        assert_eq!(chord_name(&[64, 67, 72]).as_deref(), Some("C major / E"));
        assert_eq!(chord_name(&[57, 60, 64, 67]).as_deref(), Some("Am7"));
        assert_eq!(chord_name(&[64, 67]).as_deref(), Some("E + G"));
        assert_eq!(interval_name(&[64, 67]), Some("minor 3rd"));
        assert_eq!(interval_name(&[48, 55, 60]), Some("5th"));
        assert_eq!(interval_name(&[48, 60]), Some("octave"));
        assert_eq!(chord_name(&[60]), None);
        assert_eq!(chord_name(&[60, 61, 62]), None);
        assert_eq!(chord_name(&[60, 64, 70]).as_deref(), Some("C7"));
        assert_eq!(note_name(60), "C4");
    }
}
