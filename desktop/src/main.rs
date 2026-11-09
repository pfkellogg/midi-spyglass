//! MIDI Spyglass: just the spyglass from FM-1 Mirror, for any MIDI keyboard or app. A round brass eyepiece
//! that floats over everything and shows the last note or chord played on any MIDI input: its name, the
//! notes on a grand staff with their waveform beside it, the pitch, how hard the key was hit, and the sound
//! (the General MIDI name of the channel's Program Change, or the keyboard's name until one comes).
//!
//! Drag it to move it; drag the grip on its rim, or scroll over it, to resize it; right-click it for the
//! inputs it listens to, and to quit. Where it is and its size are remembered.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // no console window on Windows

mod chord;
mod glass;
mod glyphs;
mod gm;
mod midi;
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
    dirty: bool,
    saved_at: Instant,
}

impl App {
    fn new(saved: &serde_json::Value) -> App {
        let mut inputs = midi::Inputs::default();
        if let Some(skip) = saved["skip"].as_array() {
            inputs.skip = skip.iter().filter_map(|s| s.as_str().map(String::from)).collect();
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
            dirty: false,
            saved_at: Instant::now(),
        }
    }

    fn store(&mut self) {
        let mut s = json!({ "size": self.size, "on_top": self.on_top, "wave": self.wave, "sound": self.sound, "skip": self.inputs.skip.iter().collect::<Vec<_>>() });
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
    }

    /// The right-click menu: the inputs (untick one to stop listening to it), always on top, quit.
    fn menu(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.label(egui::RichText::new("Listening to").strong());
        let mut all: Vec<String> = midi::ports();
        all.sort();
        all.dedup();
        if all.is_empty() {
            ui.label("No MIDI inputs. Plug in a keyboard.");
        }
        for name in all {
            let mut on = !self.inputs.skip.contains(&name);
            if ui.checkbox(&mut on, &name).changed() {
                if on {
                    self.inputs.skip.remove(&name);
                } else {
                    self.inputs.skip.insert(name.clone());
                }
                self.looked = None;
                self.dirty = true;
            }
        }
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
        let (changed, sound) = {
            let played = self.played.lock().unwrap();
            (self.glass.tick(&played), if self.sound { played.sound() } else { None })
        };
        // Settling a chord or emptying the glass needs another look soon; otherwise a new message wakes it.
        ctx.request_repaint_after(if self.glass.busy() || changed { Duration::from_millis(40) } else { LOOK_EVERY });
        let (at, scroll) = ctx.input(|i| (i.viewport().outer_rect.map(|r| r.min), i.smooth_scroll_delta.y));
        if at.is_some() && at != self.at {
            self.at = at;
            self.dirty = true;
        }
        let idle = if self.inputs.names().is_empty() { "no MIDI input" } else { "play a note" };
        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx, |ui| {
            let area = ui.max_rect();
            let (c, d) = (area.center(), self.size);
            let body = ui.interact(area, ui.id().with("glass"), egui::Sense::click_and_drag());
            if body.drag_started_by(egui::PointerButton::Primary) {
                ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }
            body.context_menu(|ui| self.menu(ui, ctx));
            let grip_at = glass::grip_at(c, d);
            let grip = ui.interact(Rect::from_center_size(grip_at, vec2(30.0, 30.0)), ui.id().with("grip"), egui::Sense::drag());
            let mut size = d;
            if grip.dragged() {
                let m = grip.drag_delta(); // the window's top left stays, so the grip moves about 0.85 of the size's change
                size += (m.x + m.y) * 0.59;
            }
            if body.hovered() {
                size += scroll * 0.6;
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
            self.glass.draw(ui.painter(), c, d, idle, sound.as_deref(), self.wave);
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
