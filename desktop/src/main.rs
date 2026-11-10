//! MIDI Spyglass: just the spyglass from FM-1 Mirror, for any MIDI keyboard or app. A round brass eyepiece
//! that floats over everything and shows the last note or chord played on any MIDI input: its name, the
//! notes on a grand staff with their waveform beside it (the synth's real sound from an audio input, the FM-1's
//! by itself, or drawn from the notes), the pitch, how hard the key was hit, and the sound
//! ("FM-1" while its sound is heard, else the General MIDI name of a Program Change).
//!
//! Click it for its MIDI connections: FROM (what it shows, and what plays) and TO (the synths it plays, ticked
//! together to layer them), as in Synth Spyglass. Drag it to move it; drag the grip on its rim, or scroll over it, to
//! resize it; right-click it for its settings, and to quit. Where it is and its size are remembered.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // no console window on Windows

mod chord;
mod glass;
mod glyphs;
mod gm;
mod midi;
mod scope;
mod softsynth;
mod staff;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eframe::egui::{self, vec2, Pos2, Rect};
use serde_json::json;

const APP_NAME: &str = "MIDI Spyglass";
/// Room around the eyepiece in its window, and its smallest and largest size.
const PAD: f32 = 14.0;
const SIZES: (f32, f32) = (180.0, 900.0);
/// How often to look for MIDI inputs plugged in or gone.
const LOOK_EVERY: Duration = Duration::from_secs(2);

fn settings_file() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join(APP_NAME).join("settings.json"))
}

fn load() -> serde_json::Value {
    settings_file().and_then(|f| std::fs::read_to_string(f).ok()).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or(json!({}))
}

struct App {
    played: Arc<Mutex<midi::Played>>,
    inputs: midi::Inputs,
    looked: Option<Instant>,
    glass: glass::Glass,
    size: f32,
    at: Option<Pos2>,
    on_top: bool,
    wave: bool,
    sound: bool,
    /// The audio input the waveform comes from: None, an FM-1's USB audio if one is plugged in; "" none (drawn
    /// from the notes); else that input. And the one open now, by name.
    audio: Option<String>,
    sound_in: Arc<scope::Audio>,
    /// The MIDI connections fill the glass (a click on it).
    picker: bool,
    /// The soft synths on this Mac, and when they were last looked at.
    synths: Vec<softsynth::SoftSynth>,
    synths_at: Instant,
    ring: Arc<Mutex<scope::Ring>>,
    dirty: bool,
    saved_at: Instant,
}

impl App {
    fn new(saved: &serde_json::Value) -> App {
        let ring = Arc::new(Mutex::new(scope::Ring::default()));
        let mut inputs = midi::Inputs::default();
        inputs.from = midi::From::from_key(saved["from"].as_str());
        if let Some(to) = saved["to"].as_array() {
            inputs.to = to.iter().filter_map(|s| s.as_str().map(String::from)).collect();
        }
        App {
            played: Arc::new(Mutex::new(midi::Played::default())),
            inputs,
            looked: None,
            glass: glass::Glass::default(),
            size: size_of(saved),
            at: at_of(saved),
            on_top: saved["on_top"].as_bool().unwrap_or(true),
            wave: saved["wave"].as_bool().unwrap_or(true),
            sound: saved["sound"].as_bool().unwrap_or(true),
            audio: saved["audio"].as_str().map(String::from),
            sound_in: scope::start(ring.clone()),
            picker: std::env::var_os("MIDI_SPYGLASS_PICKER").is_some(), // (open at start: for screenshots)
            synths: softsynth::installed(),
            synths_at: Instant::now(),
            ring,
            dirty: false,
            saved_at: Instant::now(),
        }
    }

    fn store(&mut self) {
        let mut s = json!({ "size": self.size, "on_top": self.on_top, "wave": self.wave, "sound": self.sound, "audio": self.audio, "from": self.inputs.from.key(), "to": self.inputs.to.iter().collect::<Vec<_>>() });
        if let Some(at) = self.at {
            s["x"] = json!(at.x);
            s["y"] = json!(at.y);
        }
        if let Some(f) = settings_file() {
            if let Some(dir) = f.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::write(f, s.to_string());
        }
        self.dirty = false;
        self.saved_at = Instant::now();
    }

    fn look_for_inputs(&mut self, ctx: &egui::Context) {
        if self.looked.is_some_and(|t| t.elapsed() < LOOK_EVERY) {
            return;
        }
        self.looked = Some(Instant::now());
        let ctx = ctx.clone();
        self.inputs.refresh(&self.played, &move || ctx.request_repaint());
        if self.synths_at.elapsed() > Duration::from_secs(3) {
            softsynth::refresh(&mut self.synths); // (opened, quit, or set to another input since)
            self.synths_at = Instant::now();
        }
        self.listen();
    }

    /// Tells the audio thread what the waveform should come from: the input chosen, or an FM-1's when plugged in.
    fn listen(&mut self) {
        *self.sound_in.want.lock().unwrap() = (self.wave, self.audio.clone());
    }

    fn listener(&self) -> Option<String> {
        self.sound_in.open.lock().unwrap().clone()
    }

    /// The waveform: the real sound when an audio input is heard (a flat line while it's quiet), else drawn from
    /// the notes. None: the waveform is off.
    fn wave_now(&self) -> Option<Option<Vec<f32>>> {
        if !self.wave {
            return None;
        }
        let ring = self.ring.lock().unwrap();
        // The heard sound is the FM-1's (or the input chosen): it's the right wave only while that's what plays.
        let (fm1, soft) = self.playing();
        if self.listener().is_some() && ring.listening() && (fm1 || soft.is_empty()) {
            return Some(self.glass.hz().and_then(|hz| ring.wave(hz)));
        }
        Some(self.glass.drawn_wave())
    }

    /// What TO plays now: an FM-1 (ticked and open), and the soft synths open on a ticked bus.
    fn playing(&self) -> (bool, Vec<String>) {
        let open = self.inputs.playing_to();
        let fm1 = open.iter().any(|n| midi::is_fm1(n));
        let soft = self.synths.iter().filter(|s| open.iter().any(|b| s.plays(b))).map(|s| s.name.clone()).collect();
        (fm1, soft)
    }

    /// The right-click menu: the inputs (untick one to stop listening to it), always on top, quit.
    /// The MIDI connections, filling the glass: FROM (what the spyglass shows and what plays) and TO (the synths it
    /// plays: tick several to layer them). Scrolls when there are more than fit.
    fn picker(&mut self, ui: &mut egui::Ui, c: Pos2, d: f32) {
        ui.painter().circle_filled(c, d / 2.0 - (d * 0.05).max(7.0), egui::Color32::from_rgb(10, 13, 17));
        let area = Rect::from_center_size(c, vec2(d * 0.64, d * 0.74));
        let size = (d * 0.042).clamp(11.0, 22.0);
        let gold = egui::Color32::from_rgb(0xf3, 0xd6, 0x8a);
        let mut changed = false;
        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(area), |ui| {
            ui.style_mut().override_font_id = Some(egui::FontId::proportional(size));
            ui.style_mut().spacing.item_spacing.y = size * 0.25;
            ui.vertical_centered(|ui| ui.label(egui::RichText::new("\u{266a} MIDI").size(size * 1.3).color(gold)));
            let head = |ui: &mut egui::Ui, t: &str| {
                ui.label(egui::RichText::new(t).size(size * 0.8).strong().color(gold));
            };
            // A row: its mark and name on one line (cut short with "…"), gold when it's on; clicking it is the choice.
            let row = |ui: &mut egui::Ui, on: bool, mark: (&str, &str), name: &str| -> bool {
                let text = format!("{}  {name}", if on { mark.0 } else { mark.1 });
                let color = if on { gold } else { egui::Color32::from_gray(215) };
                ui.add(egui::Label::new(egui::RichText::new(text).color(color)).truncate().sense(egui::Sense::click()))
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .clicked()
            };
            egui::ScrollArea::vertical().max_height(area.height() - size * 4.6).auto_shrink([false, true]).show(ui, |ui| {
                head(ui, "FROM: shown, and plays");
                let now = midi::ports();
                let first = midi::order(now.clone()).into_iter().next();
                let mut pick = |ui: &mut egui::Ui, label: String, choice: midi::From| {
                    if row(ui, self.inputs.from == choice, ("\u{23fa}", "\u{25cb}"), &label) && self.inputs.from != choice {
                        self.inputs.from = choice;
                        changed = true;
                    }
                };
                pick(ui, format!("Auto: {}", first.as_deref().unwrap_or("none")), midi::From::Auto);
                for n in midi::order(now.clone()) {
                    pick(ui, n.clone(), midi::From::Named(n));
                }
                pick(ui, "All inputs".to_string(), midi::From::All);
                ui.add_space(size * 0.4);
                head(ui, "TO: synths it plays");
                let outs = midi::order(midi::out_ports());
                if outs.is_empty() {
                    ui.label("No MIDI outputs");
                }
                for n in outs.clone() {
                    let on = self.inputs.to.contains(&n);
                    // A bus a soft synth listens to says who plays it: "IAC Driver Bus 1: Analog Lab V".
                    let who: Vec<&str> = self.synths.iter().filter(|s| s.plays(&n)).map(|s| s.name.as_str()).collect();
                    let label = if midi::is_fm1(&n) && !n.to_lowercase().contains("fm-1") {
                        format!("{n} (FM-1)")
                    } else if !who.is_empty() {
                        format!("{n}: {}", who.join(", "))
                    } else {
                        n.clone()
                    };
                    if row(ui, on, ("\u{2611}", "\u{2610}"), &label) {
                        if !on {
                            self.inputs.to.insert(n);
                        } else {
                            self.inputs.to.remove(&n);
                        }
                        changed = true;
                    }
                }
                // The soft synths, by name: each plays what comes down the bus it listens to. Ticking one ticks that
                // bus and opens it; it's on while it's open and its bus is ticked.
                let soft: Vec<(String, String, bool)> = self
                    .synths
                    .iter()
                    .filter_map(|s| s.inputs.as_ref().and_then(|i| i.iter().find(|b| outs.contains(*b))).map(|b| (s.name.clone(), b.clone(), s.open)))
                    .collect();
                if !soft.is_empty() {
                    ui.add_space(size * 0.3);
                    head(ui, "Soft synths");
                }
                for (name, bus, open) in soft {
                    let on = open && self.inputs.to.contains(&bus);
                    let label = if open { format!("{name}  ({bus})") } else { format!("{name}  (opens it)") };
                    if row(ui, on, ("\u{2611}", "\u{2610}"), &label) {
                        if on {
                            self.inputs.to.remove(&bus);
                        } else {
                            self.inputs.to.insert(bus.clone());
                            if !open {
                                if let Some(s) = self.synths.iter().find(|s| s.name == name) {
                                    s.launch();
                                }
                            }
                        }
                        changed = true;
                    }
                }
            });
            ui.add_space(size * 0.3);
            ui.vertical_centered(|ui| {
                if ui.button(egui::RichText::new("Done").size(size)).clicked() {
                    self.picker = false;
                }
            });
        });
        if changed {
            self.looked = None; // (open and close them now)
            self.dirty = true;
        }
    }

    fn menu(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.label("Click the glass for its MIDI connections");
        ui.separator();
        if ui.checkbox(&mut self.on_top, "Always on top").changed() {
            let level = if self.on_top { egui::WindowLevel::AlwaysOnTop } else { egui::WindowLevel::Normal };
            ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(level));
            self.dirty = true;
        }
        if ui.checkbox(&mut self.sound, "Sound name").changed() {
            self.dirty = true;
        }
        if ui.checkbox(&mut self.wave, "Waveform").changed() {
            self.dirty = true;
            self.looked = None;
        }
        if self.wave {
            ui.menu_button("Waveform from", |ui| {
                let heard = self.listener();
                let auto = match (&self.audio, &heard) {
                    (None, Some(n)) => format!("FM-1's sound, when plugged in (now: {n})"),
                    _ => "FM-1's sound, when plugged in".to_string(),
                };
                let mut pick = |ui: &mut egui::Ui, label: String, choice: Option<String>| {
                    if ui.radio(self.audio == choice, label).clicked() {
                        self.audio = choice;
                        self.looked = None;
                        self.dirty = true;
                        ui.close_menu();
                    }
                };
                pick(ui, auto, None);
                pick(ui, "The notes (drawn, no audio)".to_string(), Some(String::new()));
                ui.separator();
                let inputs = self.sound_in.devices.lock().unwrap().clone();
                for name in inputs {
                    pick(ui, name.clone(), Some(name));
                }
            });
        }
        if ui.button("Quit MIDI Spyglass").clicked() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

fn size_of(saved: &serde_json::Value) -> f32 {
    saved["size"].as_f64().map_or(320.0, |d| d as f32).clamp(SIZES.0, SIZES.1)
}

fn at_of(saved: &serde_json::Value) -> Option<Pos2> {
    saved["x"].as_f64().zip(saved["y"].as_f64()).map(|(x, y)| Pos2::new(x as f32, y as f32))
}

impl eframe::App for App {
    /// See-through around the round eyepiece.
    fn clear_color(&self, _: &egui::Visuals) -> [f32; 4] {
        [0.0; 4]
    }

    fn on_exit(&mut self, _: Option<&eframe::glow::Context>) {
        self.store();
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.look_for_inputs(ctx);
        self.inputs.greet();
        let (changed, said, sound) = {
            let played = self.played.lock().unwrap();
            (self.glass.tick(&played), played.fm1_sound(), played.sound())
        };
        // The header: the sound the FM-1 says it's playing ("T1 FM6 PIANO 1"), else whose sound is heard ("FM-1",
        // from its USB audio), else a Program Change's sound name.
        let heard = self.listener().filter(|_| self.ring.lock().unwrap().listening()).map(|n| scope::label(&n));
        // The header follows TO: the FM-1's sound while it plays, the soft synths' names ("Analog Lab V"), both
        // when they're layered; with nothing ticked, the FM-1's or a Program Change's.
        let (fm1, soft) = self.playing();
        let fm1_sound = said.or(heard);
        let sound = if !self.sound {
            None
        } else if fm1 || !soft.is_empty() {
            let mut parts: Vec<String> = if fm1 { fm1_sound.into_iter().collect() } else { vec![] };
            parts.extend(soft);
            (!parts.is_empty()).then(|| parts.join(" + "))
        } else {
            fm1_sound.or(sound)
        };
        // Settling a chord or emptying the glass needs another look soon; otherwise a new message wakes it.
        ctx.request_repaint_after(if self.glass.busy() || changed { Duration::from_millis(40) } else { Duration::from_millis(900) });
        let (at, scroll) = ctx.input(|i| (i.viewport().outer_rect.map(|r| r.min), i.smooth_scroll_delta.y));
        if at.is_some() && at != self.at {
            self.at = at;
            self.dirty = true;
        }
        let wave = self.wave_now();
        if self.listener().is_some() && self.glass.hz().is_some() {
            ctx.request_repaint_after(Duration::from_millis(40)); // the real sound keeps moving
        }
        let no_midi = self.inputs.names().is_empty();
        let idle = if no_midi { "" } else { "play a note" };
        let sound = if no_midi { None } else { sound };
        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx, |ui| {
            let area = ui.max_rect();
            let (c, d) = (area.center(), self.size);
            let body = ui.interact(area, ui.id().with("glass"), egui::Sense::click_and_drag());
            if body.drag_started_by(egui::PointerButton::Primary) {
                ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }
            if body.clicked_by(egui::PointerButton::Primary) {
                self.picker = !self.picker;
            }
            body.context_menu(|ui| self.menu(ui, ctx));
            let grip_at = glass::grip_at(c, d);
            let grip = ui.interact(Rect::from_center_size(grip_at, vec2(30.0, 30.0)), ui.id().with("grip"), egui::Sense::drag());
            let mut size = d;
            if grip.dragged() {
                let m = grip.drag_delta(); // the window's top left stays, so the grip moves about 0.85 of the size's change
                size += (m.x + m.y) * 0.59;
            }
            if body.hovered() && !self.picker {
                size += scroll * 0.6; // (with the connections showing, scrolling scrolls them)
            }
            let size = size.clamp(SIZES.0, SIZES.1);
            if size != d {
                self.size = size;
                self.dirty = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::Vec2::splat(size + 2.0 * PAD)));
            }
            if grip.hovered() || grip.dragged() {
                ctx.set_cursor_icon(egui::CursorIcon::ResizeNwSe);
            }
            if self.picker {
                self.glass.draw(ui.painter(), c, d, "", None, None); // (just the eyepiece: the connections cover its face)
            } else {
                self.glass.draw(ui.painter(), c, d, idle, sound.as_deref(), wave);
            }
            if self.picker {
                self.picker(ui, c, d);
            } else if no_midi {
                let line = if midi::ports().is_empty() { "Plug in a MIDI keyboard" } else { "Click the glass to choose one" };
                glass::draw_alert(ui.painter(), c, d, "No MIDI found", line);
            }
            if ui.rect_contains_pointer(area) || grip.dragged() {
                glass::draw_grip(ui.painter(), grip_at, grip.hovered() || grip.dragged());
            }
        });
        if self.dirty && self.saved_at.elapsed() > Duration::from_secs(1) {
            self.store();
        }
    }
}

fn main() -> eframe::Result {
    let saved = load();
    let side = size_of(&saved) + 2.0 * PAD;
    let mut viewport = egui::ViewportBuilder::default()
        .with_title(APP_NAME)
        .with_app_id("midi-spyglass")
        .with_inner_size([side, side])
        .with_decorations(false)
        .with_transparent(true)
        .with_resizable(false);
    if saved["on_top"].as_bool().unwrap_or(true) {
        viewport = viewport.with_always_on_top();
    }
    if let Some(at) = at_of(&saved) {
        viewport = viewport.with_position(at);
    }
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    eframe::run_native(APP_NAME, options, Box::new(move |_| Ok(Box::new(App::new(&saved)))))
}
