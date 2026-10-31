//! The share video's frames, drawn in Rust: what native/share.swift draws on macOS, for Windows (which encodes them
//! in platform/windows/share.rs). The same layout, colours and sizes, from the same JSON (share.rs's `spec`); the
//! text is in the system's own font (Segoe UI on Windows, SF on the Mac, where this is only checked against the
//! Swift drawing). Nothing here touches the OS but reading font files.
use std::cell::OnceCell;
use std::path::PathBuf;

use ab_glyph::{point, Font, FontVec, GlyphId, PxScale, VariableFont};
use serde_json::Value;
use tiny_skia::{
    Color, FillRule, FilterQuality, GradientStop, LineCap, LinearGradient, Mask, Paint, Path, PathBuilder, Pattern,
    Pixmap, PixmapPaint, Point, Rect, SpreadMode, Stroke, Transform,
};

pub const W: u32 = 1080;
pub const H: u32 = 1920;
pub const FPS: u32 = 30;
// the lane: where it sits, what it shows around the playhead (as share.swift)
const LX0: f32 = 40.0;
const LX1: f32 = 1040.0;
const LY0: f32 = 1210.0;
const LY1: f32 = 1650.0;
const BEFORE: f64 = 1.3;
const AFTER: f64 = 2.7;
/// Text sits with its baseline this far (in ems) below the top of its line: SF's ascender, so every font lands on
/// the Mac's baselines (Segoe UI's own ascender is taller and would push the text down).
const ASCENT: f32 = 0.952;

/// What share.rs asks to be drawn.
pub struct Spec {
    pub title: String,
    pub artist: String,
    pub footer: String,
    pub seconds: f64,
    /// the end card's lines (what, for what, where) and how long it's up at the end (s)
    pub end: Vec<String>,
    pub end_seconds: f64,
    pub lo: f64,
    pub hi: f64,
    /// start, end (s into the clip), MIDI note, how much of it was hit (0..1)
    pub notes: Vec<[f64; 4]>,
    /// time, your pitch (MIDI), how far off (cents)
    pub line: Vec<(f64, Option<f64>, Option<f64>)>,
}

impl Spec {
    pub fn parse(json: &str) -> Option<Spec> {
        let o: Value = serde_json::from_str(json).ok()?;
        let s = |k: &str| o[k].as_str().unwrap_or("").to_string();
        let f = |k: &str, d: f64| o[k].as_f64().unwrap_or(d);
        let spec = Spec {
            title: s("title"),
            artist: s("artist"),
            footer: s("footer"),
            seconds: f("seconds", 0.0),
            end: o["end"].as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default(),
            end_seconds: f("end_s", 0.0),
            lo: f("lo", 48.0),
            hi: f("hi", 72.0),
            notes: o["notes"].as_array().map(|a| a.iter().filter_map(|n| {
                let n = n.as_array()?;
                (n.len() >= 4).then(|| [n[0].as_f64(), n[1].as_f64(), n[2].as_f64(), n[3].as_f64()]).and_then(|[a, b, c, d]| Some([a?, b?, c?, d?]))
            }).collect()).unwrap_or_default(),
            line: o["line"].as_array().map(|a| a.iter().filter_map(|p| {
                let p = p.as_array()?;
                Some((p.first()?.as_f64()?, p.get(1).and_then(|v| v.as_f64()), p.get(2).and_then(|v| v.as_f64())))
            }).collect()).unwrap_or_default(),
        };
        (spec.seconds > 0.0).then_some(spec)
    }

    /// The frames in the clip (whole frames, as the Mac's).
    pub fn frames(&self) -> usize {
        (self.seconds * FPS as f64) as usize
    }
}

// ---------------------------------------------------------------- text

#[derive(Clone, Copy)]
pub enum Weight {
    Regular = 400,
    Medium = 500,
    Semibold = 600,
    Bold = 700,
}

struct Face {
    font: FontVec,
    /// a variable font with a weight axis (one file for every weight)
    variable: bool,
    /// a static font's weight
    weight: u16,
}

impl Face {
    fn load(path: &PathBuf, index: u32, weight: u16) -> Option<Face> {
        let font = FontVec::try_from_vec_and_index(std::fs::read(path).ok()?, index).ok()?;
        let variable = font.variations().iter().any(|a| &a.tag == b"wght");
        Some(Face { font, variable, weight })
    }

    /// Set to `weight` at `size` (variable fonts; a static one is what it is).
    fn style(&mut self, weight: Weight, size: f32) {
        if self.variable {
            self.font.set_variation(b"wght", weight as u16 as f32);
            self.font.set_variation(b"opsz", size);
        }
    }

    fn has(&self, c: char) -> bool {
        self.font.glyph_id(c).0 != 0
    }

    /// Pixels per font unit at `size` (an em, as the Mac's point sizes).
    fn scale(&self, size: f32) -> f32 {
        size / self.font.units_per_em().unwrap_or(1000.0)
    }
}

/// A fallback font, read the first time a character needs it.
struct Fallback {
    path: PathBuf,
    index: u32,
    face: OnceCell<Option<Face>>,
}

/// The system's font (one variable file, or one per weight) and fallbacks for what it hasn't (emoji, CJK…).
pub struct Fonts {
    main: Vec<Face>,
    fallback: Vec<Fallback>,
}

/// One laid-out glyph: which font (0 the main one, then fallbacks), which glyph, where (px from the line's start).
type Placed = (usize, GlyphId, f32);

impl Fonts {
    /// `main`: (file, index in a collection, weight); `fallback`: (file, index), in the order tried. Files that
    /// don't open are left out.
    pub fn load(main: &[(PathBuf, u32, u16)], fallback: &[(PathBuf, u32)]) -> Fonts {
        let mut faces: Vec<Face> = main.iter().filter_map(|(p, i, w)| Face::load(p, *i, *w)).collect();
        // a variable one covers every weight: just it
        if let Some(k) = faces.iter().position(|f| f.variable) {
            faces = vec![faces.swap_remove(k)];
        }
        Fonts { main: faces, fallback: fallback.iter().map(|(p, i)| Fallback { path: p.clone(), index: *i, face: OnceCell::new() }).collect() }
    }

    /// The OS's own UI font and its fallbacks.
    pub fn system() -> Fonts {
        let (main, fallback) = crate::platform::sys::ui_fonts();
        Fonts::load(&main, &fallback)
    }

    /// Whether there's a font to draw text in at all.
    pub fn any(&self) -> bool {
        !self.main.is_empty()
    }

    /// The main font for `weight`: the variable one, else the static one nearest (the heavier of two as near).
    fn main_for(&self, weight: Weight) -> usize {
        let w = weight as u16 as i32;
        (0..self.main.len()).min_by_key(|&k| ((self.main[k].weight as i32 - w).abs(), -(self.main[k].weight as i32))).unwrap_or(0)
    }

    fn face(&self, k: usize, main: usize) -> Option<&Face> {
        if k == 0 { self.main.get(main) } else { self.fallback.get(k - 1).and_then(|f| f.face.get()).and_then(|f| f.as_ref()) }
    }

    /// `s` laid out at `size`: its glyphs and its width.
    fn layout(&mut self, s: &str, size: f32, weight: Weight) -> (Vec<Placed>, f32, usize) {
        let m = self.main_for(weight);
        let Some(main) = self.main.get_mut(m) else { return (vec![], 0.0, m) };
        main.style(weight, size);
        let mut out = Vec::new();
        let mut x = 0.0;
        let mut last: Option<(usize, GlyphId)> = None;
        for c in s.chars() {
            if c.is_control() {
                continue;
            }
            // the main font, else the first fallback with the character, else the main font's "missing" box
            let mut k = 0;
            if !self.main[m].has(c) {
                for (i, fb) in self.fallback.iter_mut().enumerate() {
                    let face = fb.face.get_or_init(|| Face::load(&fb.path, fb.index, 400));
                    if face.as_ref().is_some_and(|f| f.has(c)) {
                        if let Some(Some(f)) = fb.face.get_mut() {
                            f.style(weight, size);
                        }
                        k = i + 1;
                        break;
                    }
                }
            }
            let face = self.face(k, m).unwrap();
            let id = face.font.glyph_id(c);
            let sc = face.scale(size);
            if let Some((lk, lid)) = last {
                if lk == k {
                    x += face.font.kern_unscaled(lid, id) * sc;
                }
            }
            out.push((k, id, x));
            x += face.font.h_advance_unscaled(id) * sc;
            last = Some((k, id));
        }
        (out, x, m)
    }

    /// How wide `s` is at `size`.
    pub fn width(&mut self, s: &str, size: f32, weight: Weight) -> f32 {
        self.layout(s, size, weight).1
    }

    /// Shortened with "…" to fit `max` at `size`.
    pub fn fitted(&mut self, s: &str, size: f32, weight: Weight, max: f32) -> String {
        if self.width(s, size, weight) <= max {
            return s.to_string();
        }
        let mut t: Vec<char> = s.chars().collect();
        while !t.is_empty() && self.width(&format!("{}…", t.iter().collect::<String>()), size, weight) > max {
            t.pop();
        }
        format!("{}…", t.into_iter().collect::<String>())
    }

    /// Text with the top of its line at `y`, centred (or from `x`).
    fn text(&mut self, pix: &mut Pixmap, s: &str, y: f32, size: f32, color: [u8; 4], weight: Weight, x: Option<f32>) {
        let (glyphs, width, m) = self.layout(s, size, weight);
        let x0 = x.unwrap_or((W as f32 - width) / 2.0);
        let base = y + ASCENT * size;
        for (k, id, gx) in glyphs {
            let Some(face) = self.face(k, m) else { continue };
            let px = face.scale(size) * face.font.height_unscaled();
            let g = id.with_scale_and_position(PxScale::from(px), point(x0 + gx, base));
            if let Some(o) = face.font.outline_glyph(g) {
                let b = o.px_bounds();
                o.draw(|gx, gy, c| blend(pix, b.min.x as i32 + gx as i32, b.min.y as i32 + gy as i32, color, c));
            }
        }
    }
}

/// `color` (straight alpha) over pixel (x, y), at `coverage`.
fn blend(pix: &mut Pixmap, x: i32, y: i32, color: [u8; 4], coverage: f32) {
    if x < 0 || y < 0 || x >= pix.width() as i32 || y >= pix.height() as i32 {
        return;
    }
    let a = coverage.clamp(0.0, 1.0) * color[3] as f32 / 255.0;
    let i = (y as usize * pix.width() as usize + x as usize) * 4;
    let d = &mut pix.data_mut()[i..i + 4];
    for c in 0..3 {
        d[c] = (color[c] as f32 * a + d[c] as f32 * (1.0 - a)).round() as u8;
    }
    d[3] = (255.0 * a + d[3] as f32 * (1.0 - a)).round() as u8;
}

// ---------------------------------------------------------------- shapes

fn paint(c: [u8; 4]) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color_rgba8(c[0], c[1], c[2], c[3]);
    p.anti_alias = true;
    p
}

/// A rounded rectangle (circular corners, as CGPath's).
fn rounded(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<Path> {
    let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
    let k = r * 0.552_284_8;
    let mut p = PathBuilder::new();
    p.move_to(x + r, y);
    p.line_to(x + w - r, y);
    p.cubic_to(x + w - r + k, y, x + w, y + r - k, x + w, y + r);
    p.line_to(x + w, y + h - r);
    p.cubic_to(x + w, y + h - r + k, x + w - r + k, y + h, x + w - r, y + h);
    p.line_to(x + r, y + h);
    p.cubic_to(x + r - k, y + h, x, y + h - r + k, x, y + h - r);
    p.line_to(x, y + r);
    p.cubic_to(x, y + r - k, x + r - k, y, x + r, y);
    p.close();
    p.finish()
}

fn fill(pix: &mut Pixmap, path: Option<Path>, c: [u8; 4], mask: Option<&Mask>) {
    if let Some(p) = path {
        pix.fill_path(&p, &paint(c), FillRule::Winding, Transform::identity(), mask);
    }
}

/// A full frame with the clip's gradient.
fn canvas() -> Pixmap {
    let mut pix = Pixmap::new(W, H).unwrap();
    let grad = LinearGradient::new(
        Point::from_xy(0.0, 0.0),
        Point::from_xy(W as f32 * 0.4, H as f32),
        vec![GradientStop::new(0.0, Color::from_rgba8(22, 20, 38, 255)), GradientStop::new(1.0, Color::from_rgba8(48, 38, 112, 255))],
        SpreadMode::Pad,
        Transform::identity(),
    );
    let mut p = Paint::default();
    if let Some(g) = grad {
        p.shader = g;
    }
    pix.fill_rect(Rect::from_xywh(0.0, 0.0, W as f32, H as f32).unwrap(), &p, Transform::identity(), None);
    pix
}

/// `icon` halved while that still leaves it at least `size` (a smoother small icon than one big scaling step).
fn shrunk(icon: &Pixmap, size: f32) -> Pixmap {
    let mut cur = icon.clone();
    while cur.width() as f32 / 2.0 >= size && cur.width() >= 2 {
        let (w, h) = (cur.width() / 2, cur.height() / 2);
        let mut half = Pixmap::new(w, h).unwrap();
        let (src, sw) = (cur.data(), cur.width() as usize);
        let dst = half.data_mut();
        for y in 0..h as usize {
            for x in 0..w as usize {
                for c in 0..4 {
                    let s = |dx: usize, dy: usize| src[((2 * y + dy) * sw + 2 * x + dx) * 4 + c] as u32;
                    dst[(y * w as usize + x) * 4 + c] = ((s(0, 0) + s(1, 0) + s(0, 1) + s(1, 1) + 2) / 4) as u8;
                }
            }
        }
        cur = half;
    }
    cur
}

/// The icon in a `size` square at (x, y), rounded by `corner`.
fn draw_icon(pix: &mut Pixmap, icon: &Pixmap, x: f32, y: f32, size: f32, corner: f32) {
    let img = shrunk(icon, size);
    let s = size / img.width() as f32;
    let mut p = Paint { anti_alias: true, ..Default::default() };
    p.shader = Pattern::new(img.as_ref(), SpreadMode::Pad, FilterQuality::Bicubic, 1.0, Transform::from_row(s, 0.0, 0.0, s, x, y));
    if let Some(path) = rounded(x, y, size, size, corner) {
        pix.fill_path(&path, &p, FillRule::Winding, Transform::identity(), None);
    }
}

/// Box widths whose three passes approximate a Gaussian blur of `sigma`.
fn boxes(sigma: f32) -> [usize; 3] {
    let ideal = (12.0 * sigma * sigma / 3.0 + 1.0).sqrt();
    let mut lo = ideal.floor() as i32;
    if lo % 2 == 0 {
        lo -= 1;
    }
    let up = lo + 2;
    let m = ((12.0 * sigma * sigma - 3.0 * (lo * lo) as f32 - 12.0 * lo as f32 - 9.0) / (-4.0 * lo as f32 - 4.0)).round() as i32;
    let b = |i: i32| (if i < m { lo } else { up }).max(1) as usize;
    [b(0), b(1), b(2)]
}

/// One pass of a box blur `width` wide over rows of `w` (`stride` apart, `step` between pixels).
fn box_pass(a: &mut [f32], n: usize, lines: usize, line_step: usize, step: usize, width: usize) {
    let r = width / 2;
    let mut tmp = vec![0f32; n];
    for l in 0..lines {
        let at = |i: usize| l * line_step + i * step;
        let mut acc = 0f32;
        // a running sum over [i - r, i + r], zero outside
        for i in 0..=r.min(n - 1) {
            acc += a[at(i)];
        }
        for (i, t) in tmp.iter_mut().enumerate() {
            *t = acc / width as f32;
            if i + r + 1 < n {
                acc += a[at(i + r + 1)];
            }
            if i >= r {
                acc -= a[at(i - r)];
            }
        }
        for (i, t) in tmp.iter().enumerate() {
            a[at(i)] = *t;
        }
    }
}

/// A soft glow behind a rounded square (`size` at x, y): CoreGraphics' shadow with this `blur`, in `color`.
fn glow(pix: &mut Pixmap, x: f32, y: f32, size: f32, corner: f32, blur: f32, color: [u8; 4]) {
    let sigma = blur / 2.0;
    let pad = (sigma * 3.0).ceil() as i32;
    let n = size.ceil() as i32 + 2 * pad;
    let Some(mut mask) = Mask::new(n as u32, n as u32) else { return };
    if let Some(p) = rounded(pad as f32, pad as f32, size, size, corner) {
        mask.fill_path(&p, FillRule::Winding, true, Transform::identity());
    }
    let mut a: Vec<f32> = mask.data().iter().map(|&v| v as f32 / 255.0).collect();
    let nu = n as usize;
    for b in boxes(sigma) {
        box_pass(&mut a, nu, nu, nu, 1, b);
        box_pass(&mut a, nu, nu, 1, nu, b);
    }
    let (ox, oy) = (x.round() as i32 - pad, y.round() as i32 - pad);
    for j in 0..nu {
        for i in 0..nu {
            blend(pix, ox + i as i32, oy + j as i32, color, a[j * nu + i]);
        }
    }
}

// ---------------------------------------------------------------- the frames

const WHITE: [u8; 4] = [255, 255, 255, 255];
const SOFT: [u8; 4] = [190, 185, 230, 255];
const VIOLET: [u8; 4] = [181, 168, 255, 255];
const PANEL: [u8; 4] = [18, 16, 34, 255];
const GOOD: [u8; 4] = [52, 210, 123, 255];
const CLOSE: [u8; 4] = [245, 184, 61, 255];
const OFF: [u8; 4] = [242, 98, 107, 255];
const NONE: [u8; 4] = [139, 148, 171, 255];

/// Draws a clip's frames: what stays the same is drawn once (the background, the end card), the rest per frame.
pub struct Frames {
    spec: Spec,
    fonts: Fonts,
    bg: Pixmap,
    end: Option<Pixmap>,
    lane: Option<Mask>,
    frame: Pixmap,
}

impl Frames {
    /// `icon`: the app icon as PNG bytes (none drawn if it doesn't decode).
    pub fn new(spec: Spec, mut fonts: Fonts, icon: &[u8]) -> Frames {
        let icon = Pixmap::decode_png(icon).ok();
        let bg = background(&spec, &mut fonts, icon.as_ref());
        let end = end_card(&spec, &mut fonts, icon.as_ref());
        let mut lane = Mask::new(W, H);
        if let (Some(m), Some(r)) = (lane.as_mut(), Rect::from_xywh(LX0, LY0 - 16.0, LX1 - LX0, LY1 - LY0 + 32.0)) {
            m.fill_path(&PathBuilder::from_rect(r), FillRule::Winding, false, Transform::identity());
        }
        Frames { frame: bg.clone(), spec, fonts, bg, end, lane }
    }

    pub fn spec(&self) -> &Spec {
        &self.spec
    }

    /// The frame `now` seconds into the clip (RGBA, opaque).
    pub fn draw(&mut self, now: f64) -> &Pixmap {
        let spec = &self.spec;
        let pix = &mut self.frame;
        pix.data_mut().copy_from_slice(self.bg.data());
        let pxs = (LX1 - LX0) as f64 / (BEFORE + AFTER);
        let head = LX0 + (BEFORE * pxs) as f32;
        let x_of = |t: f64| head + ((t - now) * pxs) as f32;
        let y_of = |m: f64| LY1 - 20.0 - ((m - spec.lo) / (spec.hi - spec.lo)) as f32 * (LY1 - LY0 - 40.0);
        let lane = self.lane.as_ref();
        // the singer's notes (a hit one brighter once it's passed)
        for n in &spec.notes {
            let (a, b) = (LX0.max(x_of(n[0])), LX1.min(x_of(n[1]) - 3.0));
            if b - a < 6.0 {
                continue;
            }
            let y = y_of(n[2]);
            let past = n[1] <= now;
            let col = if past && n[3] >= 0.5 { [150, 132, 255, 255] } else { [98, 82, 224, if past { 153 } else { 235 }] };
            fill(pix, rounded(a, y - 17.0, b - a, 34.0, 17f32.min((b - a) / 2.0)), col, lane);
        }
        // your pitch line, up to now, coloured by how close it was
        let stroke = Stroke { width: 11.0, line_cap: LineCap::Round, ..Default::default() };
        let mut prev: Option<&(f64, Option<f64>, Option<f64>)> = None;
        for p in &spec.line {
            if p.0 > now {
                break;
            }
            let q = prev.replace(p);
            let (Some(q), Some(m0), Some(m1)) = (q, q.and_then(|q| q.1), p.1) else { continue };
            if p.0 < now - BEFORE - 0.05 || (m1 - m0).abs() > 1.5 {
                continue;
            }
            let col = match p.2 {
                None => NONE,
                Some(e) if e < 50.0 => GOOD,
                Some(e) if e < 100.0 => CLOSE,
                _ => OFF,
            };
            let mut pb = PathBuilder::new();
            pb.move_to(x_of(q.0), y_of(m0));
            pb.line_to(x_of(p.0), y_of(m1));
            if let Some(path) = pb.finish() {
                pix.stroke_path(&path, &paint(col), &stroke, Transform::identity(), lane);
            }
        }
        // the playhead, and a dot where your voice is now
        let mut pb = PathBuilder::new();
        pb.move_to(head, LY0);
        pb.line_to(head, LY1);
        if let Some(path) = pb.finish() {
            pix.stroke_path(&path, &paint([255, 255, 255, 69]), &Stroke { width: 3.0, ..Default::default() }, Transform::identity(), None);
        }
        if let Some(&(t, Some(m), _)) = spec.line.iter().rev().find(|p| p.0 <= now) {
            if now - t < 0.05 {
                fill(pix, PathBuilder::from_circle(head, y_of(m), 13.0), WHITE, None);
            }
        }
        // the notes hit so far in the clip
        let hit = spec.notes.iter().filter(|n| n[0] >= 0.0 && n[0] < spec.seconds && n[1] <= now && n[3] >= 0.5).count();
        let label = format!("{hit} note{} hit", if hit == 1 { "" } else { "s" });
        self.fonts.text(pix, &label, 1690.0, 56.0, VIOLET, Weight::Semibold, None);
        // the end card, faded in over a quarter of a second at the start of its time
        if let Some(end) = &self.end {
            let into = now - (spec.seconds - spec.end_seconds);
            if into >= 0.0 {
                let pp = PixmapPaint { opacity: (into / 0.25).min(1.0) as f32, ..Default::default() };
                pix.draw_pixmap(0, 0, end.as_ref(), &pp, Transform::identity(), None);
            }
        }
        &self.frame
    }
}

/// What stays the same in every frame: the background, title, the lane's panel and the footer.
fn background(spec: &Spec, fonts: &mut Fonts, icon: Option<&Pixmap>) -> Pixmap {
    let mut pix = canvas();
    let title = fonts.fitted(&spec.title, 72.0, Weight::Semibold, 960.0);
    fonts.text(&mut pix, &title, 150.0, 72.0, WHITE, Weight::Semibold, None);
    let artist = fonts.fitted(&spec.artist, 40.0, Weight::Regular, 960.0);
    fonts.text(&mut pix, &artist, 245.0, 40.0, SOFT, Weight::Regular, None);
    fill(&mut pix, rounded(LX0 - 16.0, LY0 - 16.0, LX1 - LX0 + 32.0, LY1 - LY0 + 32.0, 36.0), PANEL, None);
    // footer: the icon (rounded) and the site
    let size = 40.0;
    let lw = fonts.width(&spec.footer, size, Weight::Medium);
    let (icon_size, gap, iy) = (76.0, 20.0, 1788.0);
    let x0 = (W as f32 - (icon_size + gap + lw)) / 2.0;
    if let Some(icon) = icon {
        draw_icon(&mut pix, icon, x0, iy, icon_size, 18.0);
    }
    fonts.text(&mut pix, &spec.footer, iy + (icon_size - size * 1.2) / 2.0, size, [225, 220, 250, 255], Weight::Medium, Some(x0 + icon_size + gap));
    pix
}

/// The end card: the icon, what OnKey is, for what and where (the site, in a pill like the lane's panel), centred
/// on the clip's gradient.
fn end_card(spec: &Spec, fonts: &mut Fonts, icon: Option<&Pixmap>) -> Option<Pixmap> {
    if spec.end.len() < 3 || spec.end_seconds <= 0.0 {
        return None;
    }
    let mut pix = canvas();
    let (size, iy) = (280.0, 600.0);
    let ix = (W as f32 - size) / 2.0;
    if let Some(icon) = icon {
        // a soft glow behind it, in the notes' violet
        glow(&mut pix, ix, iy, size, 64.0, 120.0, [124, 108, 246, 140]);
        fill(&mut pix, rounded(ix, iy, size, size, 64.0), [48, 38, 112, 255], None);
        draw_icon(&mut pix, icon, ix, iy, size, 64.0);
    }
    fonts.text(&mut pix, &spec.end[0], iy + size + 70.0, 100.0, WHITE, Weight::Bold, None);
    fonts.text(&mut pix, &spec.end[1], iy + size + 200.0, 52.0, SOFT, Weight::Regular, None);
    let (site, ss) = (&spec.end[2], 48.0);
    let sw = fonts.width(site, ss, Weight::Semibold);
    let (px, py, ph) = ((W as f32 - sw) / 2.0 - 44.0, iy + size + 320.0, 100.0);
    fill(&mut pix, rounded(px, py, sw + 88.0, ph, 50.0), PANEL, None);
    fonts.text(&mut pix, site, py + (ph - ss * 1.2) / 2.0, ss, VIOLET, Weight::Semibold, None);
    Some(pix)
}

/// An opaque RGBA frame (even sizes) as NV12 (BT.709, video range: what H.264 encoders take), 4:2:0 by averaging
/// each 2x2.
pub fn nv12(rgba: &[u8], w: usize, h: usize) -> Vec<u8> {
    debug_assert!(w % 2 == 0 && h % 2 == 0);
    let mut out = vec![0u8; w * h * 3 / 2];
    let (luma, chroma) = out.split_at_mut(w * h);
    let px = |x: usize, y: usize| {
        let i = (y * w + x) * 4;
        (rgba[i] as f32, rgba[i + 1] as f32, rgba[i + 2] as f32)
    };
    let yof = |(r, g, b): (f32, f32, f32)| 0.2126 * r + 0.7152 * g + 0.0722 * b;
    for y in 0..h {
        for x in 0..w {
            luma[y * w + x] = (16.0 + yof(px(x, y)) * 219.0 / 255.0).round() as u8;
        }
    }
    for y in (0..h).step_by(2) {
        for x in (0..w).step_by(2) {
            let (mut u, mut v) = (0.0, 0.0);
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let p = px(x + dx, y + dy);
                let l = yof(p);
                u += (p.2 - l) / 1.8556;
                v += (p.0 - l) / 1.5748;
            }
            let i = (y / 2) * w + x;
            chroma[i] = (128.0 + u / 4.0 * 224.0 / 255.0).round().clamp(0.0, 255.0) as u8;
            chroma[i + 1] = (128.0 + v / 4.0 * 224.0 / 255.0).round().clamp(0.0, 255.0) as u8;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn spec() -> Spec {
        // a note from 0.5 to 1.5 s at MIDI 60 (hit), one from 2 to 3 s at 64 (missed); the line on 60, good then off
        let line: Vec<Value> = (0..300).map(|i| {
            let t = i as f64 * 0.01;
            json!([t, 60.0, if t < 1.0 { 20.0 } else { 150.0 }])
        }).collect();
        let j = json!({"title": "Song", "artist": "Artist", "footer": "SingOnKey.app", "seconds": 4.0, "lo": 56.0, "hi": 68.0,
                       "notes": [[0.5, 1.5, 60.0, 1.0], [2.0, 3.0, 64.0, 0.2]], "line": line,
                       "end": ["Sing on key", "Free for Mac", "singonkey.app"], "end_s": 1.0});
        Spec::parse(&j.to_string()).unwrap()
    }

    fn at(p: &Pixmap, x: u32, y: u32) -> [u8; 3] {
        let i = ((y * p.width() + x) * 4) as usize;
        [p.data()[i], p.data()[i + 1], p.data()[i + 2]]
    }

    fn near(a: [u8; 3], b: [u8; 3], tol: i32) -> bool {
        (0..3).all(|c| (a[c] as i32 - b[c] as i32).abs() <= tol)
    }

    const ICON: &[u8] = include_bytes!("../../design/share-icon.png");

    #[test]
    fn the_spec_reads_as_the_mac_reads_it() {
        let s = spec();
        assert_eq!(s.frames(), 120);
        assert_eq!(s.notes.len(), 2);
        assert_eq!(s.line[0], (0.0, Some(60.0), Some(20.0)));
        assert!(Spec::parse(r#"{"seconds": 0}"#).is_none(), "no clip without a length");
        let gaps = Spec::parse(r#"{"seconds": 1, "line": [[0.1, null, null], [0.2, 61.5]]}"#).unwrap();
        assert_eq!(gaps.line, vec![(0.1, None, None), (0.2, Some(61.5), None)]);
    }

    #[test]
    fn a_frame_has_the_lane_the_notes_and_the_line_where_the_mac_draws_them() {
        let mut f = Frames::new(spec(), Fonts::system(), ICON);
        let p = f.draw(1.0).clone();
        assert_eq!((p.width(), p.height()), (W, H));
        assert!(near(at(&p, 0, 0), [22, 20, 38], 1), "the gradient starts dark at the top left");
        assert!(near(at(&p, 1079, 1919), [48, 38, 112], 2), "and ends violet at the bottom");
        assert!(near(at(&p, 30, 1300), [18, 16, 34], 0), "the lane's panel");
        // lane: 250 px a second, the playhead at x 365; MIDI 56..68 over y 1630..1230
        let y60 = (1630.0 - 4.0 / 12.0 * 400.0) as u32;
        let note = at(&p, 365 + 100, y60 - 14);
        assert!(near(note, [92, 77, 209], 2), "the note being sung, not passed: a little see-through, {note:?}");
        let line = at(&p, 365 - 100, y60);
        assert!(near(line, [52, 210, 123], 1), "your line 0.4 s ago, close: green, {line:?}");
        assert!(near(at(&p, 365, y60), [255, 255, 255], 1), "the dot where your voice is now");
        let later = f.draw(2.0).clone();
        let past = at(&later, 365 - 200, y60 - 14);
        assert!(near(past, [150, 132, 255], 1), "the hit note, passed, brighter: {past:?}");
        assert!(near(at(&later, 365 - 50, y60), [242, 98, 107], 1), "off by 150 cents: red");
        assert!(near(at(&later, 365, 1230), [82, 81, 94], 2), "the playhead over the panel");
    }

    #[test]
    fn the_end_card_fades_in_over_the_last_moments() {
        let mut f = Frames::new(spec(), Fonts::system(), ICON);
        // the end card from 3 s: the icon's middle (the clip's empty middle before), halfway there at 3.125 s
        let before = at(f.draw(2.9), 540, 740);
        let half = at(f.draw(3.125), 540, 740);
        let full = at(f.draw(3.9), 540, 740);
        assert!(!near(before, full, 20), "the icon, over the gradient: {before:?} {full:?}");
        let mid = [0, 1, 2].map(|c| ((before[c] as u16 + full[c] as u16) / 2) as u8);
        assert!(near(half, mid, 3), "half faded in: {half:?} {mid:?}");
        let glow = at(f.draw(3.9), 540, 590);
        assert!(glow[2] > 100, "the glow above the icon (the gradient alone is 64 there): {glow:?}");
    }

    #[test]
    fn the_hit_counter_counts_notes_as_they_pass() {
        let fonts = Fonts::system();
        if !fonts.any() {
            return;
        }
        let mut f = Frames::new(spec(), fonts, ICON);
        // the counter's band, y 1690..1760: the same text before and after the hit note passes would draw alike
        let band = |p: &Pixmap| -> Vec<u8> { p.data()[(1690 * W * 4) as usize..(1760 * W * 4) as usize].to_vec() };
        let a = band(f.draw(1.0));
        let b = band(f.draw(1.4));
        let c = band(f.draw(1.6));
        assert_eq!(a, b, "still 0 notes hit");
        assert_ne!(b, c, "1 note hit once it's passed");
        assert!(c.chunks(4).any(|p| near([p[0], p[1], p[2]], [181, 168, 255], 3)), "drawn in the counter's violet");
    }

    #[test]
    fn long_titles_are_shortened_to_fit() {
        let mut fonts = Fonts::system();
        if !fonts.any() {
            return;
        }
        let long = "A Very Long Song Title That Goes On And On Past The Edge Of The Video";
        let t = fonts.fitted(long, 72.0, Weight::Semibold, 960.0);
        assert!(t.ends_with('…') && t.len() < long.len(), "{t}");
        assert!(fonts.width(&t, 72.0, Weight::Semibold) <= 960.0);
        assert_eq!(fonts.fitted("Short", 72.0, Weight::Semibold, 960.0), "Short");
        assert!(fonts.width("Bold", 100.0, Weight::Bold) > fonts.width("Bold", 100.0, Weight::Regular), "weights differ");
    }

    #[test]
    fn missing_glyphs_come_from_a_fallback_font() {
        let mut fonts = Fonts::system();
        if !fonts.any() {
            return;
        }
        // CJK isn't in the UI font: a fallback draws it (its width is that of real glyphs, not the missing box)
        let (glyphs, _, _) = fonts.layout("Aあ漢", 72.0, Weight::Regular);
        assert_eq!(glyphs[0].0, 0, "Latin from the main font");
        assert!(glyphs[1].0 > 0 && glyphs[2].0 > 0, "CJK from a fallback");
        let mut p = Pixmap::new(400, 200).unwrap();
        fonts.text(&mut p, "漢", 20.0, 72.0, WHITE, Weight::Regular, Some(10.0));
        assert!(p.data().chunks(4).filter(|c| c[3] > 128).count() > 300, "drawn");
    }

    #[test]
    fn nv12_is_bt709_video_range() {
        let px = |c: [u8; 3]| [c[0], c[1], c[2], 255].repeat(4);
        assert_eq!(nv12(&px([255, 255, 255]), 2, 2), vec![235, 235, 235, 235, 128, 128]);
        assert_eq!(nv12(&px([0, 0, 0]), 2, 2), vec![16, 16, 16, 16, 128, 128]);
        let red = nv12(&px([255, 0, 0]), 2, 2);
        assert_eq!((red[0], red[4], red[5]), (63, 102, 240), "BT.709's red");
    }
}
