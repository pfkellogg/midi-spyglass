//! The soft synths on this Mac (from Synth Spyglass): which are installed, which are open, and which MIDI input
//! each listens to, read from its own settings (the Arturia apps' tmp/standalone.pref, Splice INSTRUMENT.settings).
//! A soft synth has no MIDI port of its own: it plays what comes down the bus it listens to (IAC Driver Bus 1), so
//! ticking it under TO ticks that bus.

use std::path::{Path, PathBuf};
use std::process::Command;

pub struct SoftSynth {
    pub name: String,
    app: PathBuf,
    /// Where its MIDI settings are, and how to read them.
    prefs: Option<Prefs>,
    pub open: bool,
    /// The MIDI inputs it listens to; None when it can't be told.
    pub inputs: Option<Vec<String>>,
}

enum Prefs {
    Arturia(PathBuf),
    Splice(PathBuf),
}

impl SoftSynth {
    /// Will it play what's sent down `link` (it's open and listening there)?
    pub fn plays(&self, link: &str) -> bool {
        self.open && self.listens(link)
    }

    pub fn listens(&self, link: &str) -> bool {
        self.inputs.as_ref().is_some_and(|i| i.iter().any(|n| n == link))
    }

    pub fn launch(&self) {
        let _ = Command::new("open").arg(&self.app).spawn();
    }

    /// Asks it to quit, as the Dock's Quit does (it may ask to save).
    #[allow(dead_code)]
    pub fn quit(&self) {
        let script = format!("quit app \"{}\"", self.app.display());
        let _ = Command::new("osascript").args(["-e", &script]).spawn();
    }
}

/// The standalone synth apps installed (Mac only).
pub fn installed() -> Vec<SoftSynth> {
    let mut out = vec![];
    if !cfg!(target_os = "macos") {
        return out;
    }
    let arturia = Path::new("/Applications/Arturia");
    for app in entries(arturia, "app") {
        let name = stem(&app);
        if name.contains("Software Center") {
            continue;
        }
        let pref = Path::new("/Library/Arturia").join(&name).join("tmp/standalone.pref");
        out.push(SoftSynth { name, app, prefs: Some(Prefs::Arturia(pref)), open: false, inputs: None });
    }
    let splice = PathBuf::from("/Applications/Splice INSTRUMENT.app");
    if splice.exists() {
        let pref = dirs::config_dir().map(|d| d.join("Splice INSTRUMENT.settings"));
        out.push(SoftSynth { name: "Splice INSTRUMENT".into(), app: splice, prefs: pref.map(Prefs::Splice), open: false, inputs: None });
    }
    for name in ["Pianoteq 8", "Pianoteq 9", "Vital", "Surge XT", "Kontakt 7", "Kontakt 8"] {
        let app = PathBuf::from(format!("/Applications/{name}.app"));
        if app.exists() {
            out.push(SoftSynth { name: name.into(), app, prefs: None, open: false, inputs: None });
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    refresh(&mut out);
    out
}

/// Which are open now, and what they listen to (their settings change when you change them).
pub fn refresh(synths: &mut [SoftSynth]) {
    let running = Command::new("ps")
        .args(["-axo", "command="])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default();
    for s in synths {
        let exe = format!("{}/Contents/MacOS/", s.app.display());
        s.open = running.lines().any(|l| l.starts_with(&exe));
        s.inputs = s.prefs.as_ref().and_then(read_inputs);
    }
}

fn read_inputs(p: &Prefs) -> Option<Vec<String>> {
    match p {
        // <VALUE name="MidiDevice_1" val="IAC Driver Bus 1"/>
        Prefs::Arturia(path) => {
            let text = std::fs::read_to_string(path).ok()?;
            Some(attrs(&text, "name=\"MidiDevice_", "val=\"").into_iter().filter(|v| !v.is_empty()).collect())
        }
        // <MIDIINPUT name="IAC Driver Bus 1" identifier="…"/>
        Prefs::Splice(path) => {
            let text = std::fs::read_to_string(path).ok()?;
            Some(attrs(&text, "<MIDIINPUT ", "name=\""))
        }
    }
}

/// For each line containing `marker`, the value of the attribute that starts with `attr`.
fn attrs(text: &str, marker: &str, attr: &str) -> Vec<String> {
    text.lines()
        .filter(|l| l.contains(marker))
        .filter_map(|l| {
            let rest = &l[l.find(attr)? + attr.len()..];
            Some(rest[..rest.find('"')?].to_string())
        })
        .collect()
}

fn entries(dir: &Path, ext: &str) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .map(|r| r.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == ext)).collect())
        .unwrap_or_default()
}

fn stem(p: &Path) -> String {
    p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

