//! The notes on a grand staff (same drawing as the Android app's StaffView).

use std::sync::OnceLock;

use eframe::egui::{epaint::Mesh, pos2, Color32, Painter, Pos2, Rect, Shape, Stroke};

use crate::chord::NOTE_NAMES;
use crate::glyphs as g;

/// Staff steps (one per white key, MIDI note 0 = 0) of E4 and A3.
const TREBLE_BOTTOM: i32 = 37;
const BASS_TOP: i32 = 33;
/// In staff spaces.
const STAFF_GAP: f32 = 4.0;
const STAFF_WIDTH: f32 = 11.2;
const NOTE_X: f32 = 7.4;
const WHITE_STEP: [i32; 12] = [0, 0, 1, 1, 2, 3, 3, 4, 4, 5, 5, 6];
const LINE: Color32 = Color32::from_rgb(0xaa, 0xaa, 0xaa);

fn staff_step(n: u8) -> i32 {
    7 * (n as i32 / 12) + WHITE_STEP[(n % 12) as usize]
}

/// Spaces below the treble staff's bottom line; middle C and up on the treble staff.
fn staff_y(n: u8) -> f32 {
    if n >= 60 {
        (TREBLE_BOTTOM - staff_step(n)) as f32 / 2.0
    } else {
        STAFF_GAP + (BASS_TOP - staff_step(n)) as f32 / 2.0
    }
}

fn inside(x: f32, y: f32, pts: &[f32]) -> bool {
    let mut hit = false;
    let mut j = pts.len() - 2;
    for i in (0..pts.len()).step_by(2) {
        let (xi, yi, xj, yj) = (pts[i], pts[i + 1], pts[j], pts[j + 1]);
        if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
            hit = !hit;
        }
        j = i;
    }
    hit
}

/// A glyph's contours, each triangulated, and whether it's a hole (inside an odd number of others).
struct Glyph {
    parts: Vec<(Vec<f32>, Vec<usize>, bool)>,
}

impl Glyph {
    fn new(contours: &[&[f32]]) -> Self {
        let parts = contours
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let hole = contours.iter().enumerate().filter(|&(j, o)| j != i && inside(c[0], c[1], o)).count() % 2 == 1;
                let tris = earcutr::earcut(c, &[], 2).unwrap_or_default();
                (c.to_vec(), tris, hole)
            })
            .collect();
        Glyph { parts }
    }
}

struct Glyphs {
    g_clef: Glyph,
    f_clef: Glyph,
    sharp: Glyph,
    whole: Glyph,
}

fn glyphs() -> &'static Glyphs {
    static G: OnceLock<Glyphs> = OnceLock::new();
    G.get_or_init(|| Glyphs {
        g_clef: Glyph::new(g::G_CLEF),
        f_clef: Glyph::new(g::F_CLEF),
        sharp: Glyph::new(g::SHARP),
        whole: Glyph::new(g::WHOLE),
    })
}

/// Draws the notes (one, or a chord) as whole notes on a grand staff filling rect.
pub fn draw(p: &Painter, rect: Rect, notes: &[u8], bg: Color32, fg: Color32) {
    let mut notes = notes.to_vec();
    notes.sort();
    let (mut top, mut bottom) = (-6.5f32, STAFF_GAP + 6.5);
    for &n in &notes {
        top = top.min(staff_y(n) - 1.0);
        bottom = bottom.max(staff_y(n) + 1.0);
    }
    let sp = (rect.height() / (bottom - top)).min(rect.width() / STAFF_WIDTH);
    let ox = rect.left() + (rect.width() - STAFF_WIDTH * sp) / 2.0;
    let oy = rect.top() + (rect.height() - (bottom - top) * sp) / 2.0 - top * sp;
    let at = |x: f32, y: f32| -> Pos2 { pos2(ox + x * sp, oy + y * sp) };
    let line = |x0: f32, y0: f32, x1: f32, y1: f32, w: f32| {
        p.line_segment([at(x0, y0), at(x1, y1)], Stroke::new((w * sp).max(1.0), LINE));
    };
    let glyph = |gl: &Glyph, x: f32, y: f32| {
        for (pts, tris, hole) in &gl.parts {
            let mut mesh = Mesh::default();
            let color = if *hole { bg } else { fg };
            for k in (0..pts.len()).step_by(2) {
                mesh.colored_vertex(at(x + pts[k], y + pts[k + 1]), color);
            }
            mesh.indices = tris.iter().map(|&i| i as u32).collect();
            p.add(Shape::mesh(mesh));
        }
    };

    for i in 0..5 {
        let i = i as f32;
        line(0.2, -i, STAFF_WIDTH - 0.2, -i, 0.12);
        line(0.2, STAFF_GAP + i, STAFF_WIDTH - 0.2, STAFF_GAP + i, 0.12);
    }
    line(0.2, -4.0, 0.2, STAFF_GAP + 4.0, 0.16);
    let gs = glyphs();
    glyph(&gs.g_clef, 0.7, 0.0);
    glyph(&gs.f_clef, 0.7, STAFF_GAP + 4.0);

    let head = g::WHOLE_WIDTH;
    // A second can't share a column: the upper head moves right.
    let mut right = vec![false; notes.len()];
    for i in 1..notes.len() {
        right[i] = (notes[i] >= 60) == (notes[i - 1] >= 60)
            && staff_step(notes[i]) - staff_step(notes[i - 1]) == 1
            && !right[i - 1];
    }
    let lf = NOTE_X - 0.45;
    let lt = NOTE_X + head * if right.iter().any(|&r| r) { 2.0 } else { 1.0 } + 0.45;
    for &n in &notes {
        let ny = staff_y(n);
        let (first, last) = if n >= 60 { (-4.0, 0.0) } else { (STAFF_GAP, STAFF_GAP + 4.0) };
        let mut l = first - 1.0;
        while l >= ny - 0.01 {
            line(lf, l, lt, l, 0.16);
            l -= 1.0;
        }
        let mut l = last + 1.0;
        while l <= ny + 0.01 {
            line(lf, l, lt, l, 0.16);
            l += 1.0;
        }
    }
    // Sharps, top down; one that would clash moves a column further left.
    let mut placed: Vec<(i32, f32)> = vec![];
    for &n in notes.iter().rev() {
        if !NOTE_NAMES[(n % 12) as usize].contains('#') {
            continue;
        }
        let ny = staff_y(n);
        let mut col = 0;
        while placed.iter().any(|&(c, y)| c == col && (y - ny).abs() < 3.0) {
            col += 1;
        }
        placed.push((col, ny));
        glyph(&gs.sharp, NOTE_X - 0.25 - g::SHARP_WIDTH / 2.0 - col as f32, ny);
    }
    for (&n, &r) in notes.iter().zip(&right) {
        glyph(&gs.whole, NOTE_X + head / 2.0 + if r { head } else { 0.0 }, staff_y(n));
    }
}
