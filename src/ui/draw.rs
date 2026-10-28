//! Small GDI helpers: colours, fonts, rounded shapes, gradients, translucent
//! layers (AlphaBlend), icon glyphs from the Segoe icon font, and text.

#![allow(non_snake_case, unsafe_op_in_unsafe_fn)]

use std::ffi::c_void;
use std::mem;
use std::ptr;

use windows_sys::Win32::Foundation::{POINT, RECT, SIZE};
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::UI::WindowsAndMessaging::{DI_NORMAL, DrawIconEx, HICON};

pub const fn rgb(r: u8, g: u8, b: u8) -> u32 {
    (r as u32) | ((g as u32) << 8) | ((b as u32) << 16)
}

/// Blend `t` of colour `a` over `b` (t = 1 gives `a`).
pub fn mix(a: u32, b: u32, t: f32) -> u32 {
    let ch = |shift: u32| {
        let x = ((a >> shift) & 0xff) as f32;
        let y = ((b >> shift) & 0xff) as f32;
        (y + (x - y) * t).round().clamp(0.0, 255.0) as u32
    };
    ch(0) | (ch(8) << 8) | (ch(16) << 16)
}

/// Colour tokens of the window (DESIGN.md section 3.1): a dark neutral ramp
/// where lighter means more elevated, and one green accent.
/// Text on the accent is always `ON_ACCENT` (white on the accent fails contrast).
pub mod theme {
    use super::rgb;
    /// Window base: navigation pane and title region.
    pub const BG: u32 = rgb(0x11, 0x11, 0x12);
    /// Content area behind the cards.
    pub const PANEL: u32 = rgb(0x16, 0x16, 0x17);
    /// Card / list backplate, and its hover shade.
    pub const CARD: u32 = rgb(0x1c, 0x1c, 0x1e);
    pub const CARD_TOP: u32 = rgb(0x20, 0x20, 0x22);
    pub const CARD_HI: u32 = rgb(0x22, 0x22, 0x24);
    /// Text boxes, chips at rest, standard buttons; and their hover shade.
    pub const FIELD: u32 = rgb(0x26, 0x26, 0x28);
    pub const FIELD_HI: u32 = rgb(0x2c, 0x2c, 0x2e);
    /// 1 px outline of cards and fields.
    pub const LINE: u32 = rgb(0x2a, 0x2a, 0x2c);
    /// Control boundary that must be seen (toggle off, slider track): 3:1 on a card.
    pub const TRACK: u32 = rgb(0x3c, 0x3c, 0x3f);
    /// Hairline between settings rows.
    pub const DIVIDER: u32 = rgb(0x23, 0x23, 0x25);
    pub const TEXT: u32 = rgb(0xf3, 0xf3, 0xf3);
    /// Secondary text (7:1 on a card).
    pub const MUTED: u32 = rgb(0xa9, 0xaa, 0xad);
    /// Tertiary text: placeholders and disabled only (3.9:1).
    pub const DIM: u32 = rgb(0x7c, 0x7d, 0x81);
    pub const ACCENT: u32 = rgb(0x1d, 0xb9, 0x54);
    /// Primary button pressed / hover.
    pub const ACCENT_DK: u32 = rgb(0x18, 0x9b, 0x47);
    pub const ACCENT_HI: u32 = rgb(0x25, 0xcf, 0x61);
    /// Text and icons on an accent fill.
    pub const ON_ACCENT: u32 = rgb(0x06, 0x20, 0x0f);
    pub const WARN: u32 = rgb(0xf0, 0xb4, 0x29);
    pub const ERR: u32 = rgb(0xff, 0x6b, 0x6e);
    pub const BLUE: u32 = rgb(0x4d, 0xa3, 0xff);
    pub const WHITE: u32 = rgb(0xff, 0xff, 0xff);
    pub const BLACK: u32 = rgb(0, 0, 0);
}

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn rect(x: i32, y: i32, w: i32, h: i32) -> RECT {
    RECT { left: x, top: y, right: x + w, bottom: y + h }
}

pub fn inset(r: &RECT, dx: i32, dy: i32) -> RECT {
    RECT { left: r.left + dx, top: r.top + dy, right: r.right - dx, bottom: r.bottom - dy }
}

pub fn offset(r: &RECT, dx: i32, dy: i32) -> RECT {
    RECT { left: r.left + dx, top: r.top + dy, right: r.right + dx, bottom: r.bottom + dy }
}

pub fn contains(r: &RECT, x: i32, y: i32) -> bool {
    x >= r.left && x < r.right && y >= r.top && y < r.bottom
}

pub const DT_LEFT_: u32 = 0;
pub const DT_CENTER_: u32 = 1;
pub const DT_VCENTER_: u32 = 4;
pub const DT_WORDBREAK_: u32 = 16;
pub const DT_SINGLELINE_: u32 = 32;
pub const DT_NOPREFIX_: u32 = 2048;
pub const DT_END_ELLIPSIS_: u32 = 32768;
/// Single line, vertically centred, left aligned.
pub const DT_ROW: u32 = DT_LEFT_ | DT_VCENTER_ | DT_SINGLELINE_ | DT_NOPREFIX_ | DT_END_ELLIPSIS_;
/// Single line, centred both ways.
pub const DT_MID: u32 = DT_CENTER_ | DT_VCENTER_ | DT_SINGLELINE_ | DT_NOPREFIX_;

/// The Windows 11 type ramp (DESIGN.md section 3.2) in Segoe UI Variable,
/// falling back to Segoe UI where the variable faces are missing.
pub struct Fonts {
    /// Body 14 regular.
    pub body: HFONT,
    /// Body strong 14 semibold: row titles, card titles, nav labels, buttons.
    pub bold: HFONT,
    /// Caption 12 regular.
    pub small: HFONT,
    /// Caption 12 semibold: badges and the tile source.
    pub caps: HFONT,
    /// Subtitle 20 semibold.
    pub h2: HFONT,
    /// Title 28 semibold: the page title.
    pub title: HFONT,
    /// Display 40 semibold: the battery percentage.
    pub big: HFONT,
    /// Body large 18 semibold: the wordmark.
    pub logo: HFONT,
    /// Segoe Fluent Icons (Segoe MDL2 Assets on Windows 10) at three sizes.
    pub icon: HFONT,
    pub icon_lg: HFONT,
    pub icon_sm: HFONT,
}

impl Fonts {
    pub unsafe fn new(scale: f32) -> Fonts {
        let px = |v: f32| (v * scale).round() as i32;
        Fonts {
            body: ui_font(Face::Text, px(14.0), false),
            bold: ui_font(Face::Text, px(14.0), true),
            small: ui_font(Face::Small, px(12.0), false),
            caps: ui_font(Face::Small, px(12.0), true),
            h2: ui_font(Face::Display, px(20.0), true),
            title: ui_font(Face::Display, px(28.0), true),
            big: ui_font(Face::Display, px(40.0), true),
            logo: ui_font(Face::Display, px(18.0), true),
            icon: icon_font(px(16.0)),
            icon_lg: icon_font(px(20.0)),
            icon_sm: icon_font(px(12.0)),
        }
    }

    pub unsafe fn free(&self) {
        for f in [self.body, self.bold, self.small, self.caps, self.h2, self.title, self.big, self.logo, self.icon, self.icon_lg, self.icon_sm] {
            DeleteObject(f);
        }
    }
}

unsafe fn font(face: &str, px: i32, weight: u32) -> HFONT {
    CreateFontW(
        -px,
        0,
        0,
        0,
        weight as i32,
        0,
        0,
        0,
        DEFAULT_CHARSET as u32,
        OUT_DEFAULT_PRECIS as u32,
        CLIP_DEFAULT_PRECIS as u32,
        CLEARTYPE_QUALITY as u32,
        (DEFAULT_PITCH | FF_DONTCARE) as u32,
        wide(face).as_ptr(),
    )
}

/// The three optical sizes of Segoe UI Variable.
#[derive(Clone, Copy)]
enum Face {
    Small,
    Text,
    Display,
}

/// A font of the type ramp: the first of `faces` that is installed (GDI
/// silently substitutes a missing face, so the face name is read back).
unsafe fn first_font(faces: &[&str], px: i32, weight: u32) -> HFONT {
    for face in faces {
        let f = font(face, px, weight);
        let dc = GetDC(0);
        let old = SelectObject(dc, f);
        let mut name = [0u16; 64];
        let n = GetTextFaceW(dc, name.len() as i32, name.as_mut_ptr());
        SelectObject(dc, old);
        ReleaseDC(0, dc);
        let got = String::from_utf16_lossy(&name[..n.max(1) as usize - 1]);
        if got.eq_ignore_ascii_case(face) {
            return f;
        }
        DeleteObject(f);
    }
    font(faces[faces.len() - 1], px, weight)
}

/// Segoe UI Variable at the given optical size; the semibold cut is its own
/// GDI family, so it is asked for by name (Windows 10: Segoe UI Semibold).
unsafe fn ui_font(face: Face, px: i32, semibold: bool) -> HFONT {
    let faces: &[&str] = match (face, semibold) {
        (Face::Small, false) => &["Segoe UI Variable Small", "Segoe UI"],
        (Face::Small, true) => &["Segoe UI Variable Small Semibol", "Segoe UI Semibold", "Segoe UI"],
        (Face::Text, false) => &["Segoe UI Variable Text", "Segoe UI"],
        (Face::Text, true) => &["Segoe UI Variable Text Semibold", "Segoe UI Semibold", "Segoe UI"],
        (Face::Display, false) => &["Segoe UI Variable Display", "Segoe UI"],
        (Face::Display, true) => &["Segoe UI Variable Display Semib", "Segoe UI Semibold", "Segoe UI"],
    };
    first_font(faces, px, if semibold { FW_SEMIBOLD } else { FW_NORMAL })
}

/// The system icon font: Fluent on Windows 11, MDL2 on Windows 10 (same code points).
unsafe fn icon_font(px: i32) -> HFONT {
    first_font(&["Segoe Fluent Icons", "Segoe MDL2 Assets", "Segoe UI Symbol"], px, FW_NORMAL)
}

/// Glyphs of the Segoe icon font used in the window.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Icon {
    Mouse,
    Speed,
    Track,
    Battery,
    Game,
    Flag,
    Gear,
    Search,
    Folder,
    Refresh,
    Trash,
    Add,
    Edit,
    Check,
    Info,
    Wifi,
    Keyboard,
    Record,
    Stop,
    Bolt,
    Warning,
    Layers,
    Program,
    Power,
    Sync,
    Document,
    Sliders,
    Checklist,
    Link,
    Play,
    Help,
    Cable,
    ChevronRight,
    ChevronDown,
    Filter,
}

impl Icon {
    pub fn glyph(self) -> u16 {
        match self {
            Icon::Mouse => 0xE962,
            Icon::Speed => 0xEC4A,
            Icon::Track => 0xE9A6,
            Icon::Battery => 0xE83F,
            Icon::Game => 0xE7FC,
            Icon::Flag => 0xE7C1,
            Icon::Gear => 0xE713,
            Icon::Search => 0xE721,
            Icon::Folder => 0xE8B7,
            Icon::Refresh => 0xE72C,
            Icon::Trash => 0xE74D,
            Icon::Add => 0xE710,
            Icon::Edit => 0xE70F,
            Icon::Check => 0xE73E,
            Icon::Info => 0xE946,
            Icon::Wifi => 0xE701,
            Icon::Keyboard => 0xE765,
            Icon::Record => 0xE7C8,
            Icon::Stop => 0xE71A,
            Icon::Bolt => 0xE945,
            Icon::Warning => 0xE7BA,
            Icon::Layers => 0xE81E,
            Icon::Program => 0xE8FC,
            Icon::Power => 0xE7E8,
            Icon::Sync => 0xE895,
            Icon::Document => 0xE8A5,
            Icon::Sliders => 0xE9E9,
            Icon::Checklist => 0xE9D5,
            Icon::Link => 0xE71B,
            Icon::Play => 0xE768,
            Icon::Help => 0xE9CE,
            Icon::Cable => 0xE839,
            Icon::ChevronRight => 0xE76C,
            Icon::ChevronDown => 0xE70D,
            Icon::Filter => 0xE71C,
        }
    }
}

/// Draw an icon glyph centred on (cx, cy).
pub unsafe fn icon(dc: HDC, font: HFONT, cx: i32, cy: i32, ic: Icon, color: u32) {
    let s = String::from_utf16_lossy(&[ic.glyph()]);
    text(dc, &rect(cx - 40, cy - 40, 80, 80), &s, font, color, DT_MID);
}

pub unsafe fn fill(dc: HDC, r: &RECT, color: u32) {
    let b = CreateSolidBrush(color);
    FillRect(dc, r, b);
    DeleteObject(b);
}

unsafe fn with_brush_pen(dc: HDC, fill: Option<u32>, border: Option<(u32, i32)>, f: impl FnOnce()) {
    let brush = match fill {
        Some(c) => CreateSolidBrush(c),
        None => GetStockObject(NULL_BRUSH),
    };
    let pen = match border {
        Some((c, w)) => CreatePen(PS_SOLID, w, c),
        None => GetStockObject(NULL_PEN),
    };
    let ob = SelectObject(dc, brush);
    let op = SelectObject(dc, pen);
    f();
    SelectObject(dc, ob);
    SelectObject(dc, op);
    if fill.is_some() {
        DeleteObject(brush);
    }
    if border.is_some() {
        DeleteObject(pen);
    }
}

pub unsafe fn round(dc: HDC, r: &RECT, radius: i32, fill: Option<u32>, border: Option<(u32, i32)>) {
    with_brush_pen(dc, fill, border, || {
        RoundRect(dc, r.left, r.top, r.right, r.bottom, radius * 2, radius * 2);
    });
}

pub unsafe fn ellipse(dc: HDC, r: &RECT, fill: Option<u32>, border: Option<(u32, i32)>) {
    with_brush_pen(dc, fill, border, || {
        Ellipse(dc, r.left, r.top, r.right, r.bottom);
    });
}

pub unsafe fn circle(dc: HDC, cx: i32, cy: i32, radius: i32, fill: Option<u32>, border: Option<(u32, i32)>) {
    ellipse(dc, &rect(cx - radius, cy - radius, radius * 2, radius * 2), fill, border);
}

/// The freenapse mark (assets/logo.svg) on a 64-unit grid scaled to `size`:
/// an open ring, centre (32, 32) radius 17 with its gap at the top right, and
/// a dot leaving through the gap. Draws only the glyph; the caller paints the tile.
pub unsafe fn logo_mark(dc: HDC, x: i32, y: i32, size: i32, ring: u32, dot: u32) {
    let s = |v: f32| (v * size as f32 / 64.0).round() as i32;
    let pen = CreatePen(PS_SOLID, s(5.0).max(2), ring);
    let op = SelectObject(dc, pen);
    Arc(dc, x + s(15.0), y + s(15.0), x + s(49.0), y + s(49.0), x + s(41.26), y + s(17.74), x + s(46.26), y + s(22.74));
    SelectObject(dc, op);
    DeleteObject(pen);
    circle(dc, x + s(50.0), y + s(14.0), s(4.5).max(2), Some(dot), None);
}

pub unsafe fn polyline(dc: HDC, pts: &[POINT], color: u32, width: i32) {
    let pen = CreatePen(PS_SOLID, width, color);
    let op = SelectObject(dc, pen);
    Polyline(dc, pts.as_ptr(), pts.len() as i32);
    SelectObject(dc, op);
    DeleteObject(pen);
}

pub unsafe fn polygon(dc: HDC, pts: &[POINT], fill: Option<u32>, border: Option<(u32, i32)>) {
    with_brush_pen(dc, fill, border, || {
        Polygon(dc, pts.as_ptr(), pts.len() as i32);
    });
}

pub unsafe fn text(dc: HDC, r: &RECT, s: &str, font: HFONT, color: u32, flags: u32) {
    let w = wide(s);
    let of = SelectObject(dc, font);
    SetBkMode(dc, TRANSPARENT as i32);
    SetTextColor(dc, color);
    let mut rr = *r;
    DrawTextW(dc, w.as_ptr(), -1, &mut rr, flags);
    SelectObject(dc, of);
}

pub unsafe fn text_width(dc: HDC, font: HFONT, s: &str) -> i32 {
    let w: Vec<u16> = s.encode_utf16().collect();
    let of = SelectObject(dc, font);
    let mut sz = SIZE { cx: 0, cy: 0 };
    GetTextExtentPoint32W(dc, w.as_ptr(), w.len() as i32, &mut sz);
    SelectObject(dc, of);
    sz.cx
}

// ----------------------------------------------------------------------
// Gradients, translucency and clipping
// ----------------------------------------------------------------------

fn vertex(x: i32, y: i32, c: u32) -> TRIVERTEX {
    TRIVERTEX { x, y, Red: ((c & 0xff) as u16) << 8, Green: (((c >> 8) & 0xff) as u16) << 8, Blue: (((c >> 16) & 0xff) as u16) << 8, Alpha: 0 }
}

/// Vertical gradient from `top` to `bottom`.
pub unsafe fn vgradient(dc: HDC, r: &RECT, top: u32, bottom: u32) {
    let v = [vertex(r.left, r.top, top), vertex(r.right, r.bottom, bottom)];
    let mesh = GRADIENT_RECT { UpperLeft: 0, LowerRight: 1 };
    GradientFill(dc, v.as_ptr(), 2, &mesh as *const GRADIENT_RECT as *const c_void, 1, GRADIENT_FILL_RECT_V);
}

/// Blend `color` over the rectangle with an alpha running from `a_top` to `a_bottom`.
pub unsafe fn alpha_vgradient(dc: HDC, r: &RECT, color: u32, a_top: u8, a_bottom: u8) {
    let w = r.right - r.left;
    let h = r.bottom - r.top;
    if w <= 0 || h <= 0 {
        return;
    }
    let n = if a_top == a_bottom { 1 } else { h.min(256) };
    let mut bi: BITMAPINFO = mem::zeroed();
    bi.bmiHeader.biSize = mem::size_of::<BITMAPINFOHEADER>() as u32;
    bi.bmiHeader.biWidth = 1;
    bi.bmiHeader.biHeight = -n;
    bi.bmiHeader.biPlanes = 1;
    bi.bmiHeader.biBitCount = 32;
    bi.bmiHeader.biCompression = BI_RGB;
    let mut bits: *mut c_void = ptr::null_mut();
    let bmp = CreateDIBSection(dc, &bi, DIB_RGB_COLORS, &mut bits, 0, 0);
    if bmp == 0 || bits.is_null() {
        return;
    }
    let (cr, cg, cb) = (color & 0xff, (color >> 8) & 0xff, (color >> 16) & 0xff);
    let px = bits as *mut u32;
    for i in 0..n {
        let t = if n == 1 { 0.0 } else { i as f32 / (n - 1) as f32 };
        let a = (a_top as f32 + (a_bottom as f32 - a_top as f32) * t).round() as u32;
        // Premultiplied BGRA, as AlphaBlend wants it.
        *px.add(i as usize) = (a << 24) | ((cr * a / 255) << 16) | ((cg * a / 255) << 8) | (cb * a / 255);
    }
    let mdc = CreateCompatibleDC(dc);
    let old = SelectObject(mdc, bmp);
    let bf = BLENDFUNCTION { BlendOp: AC_SRC_OVER as u8, BlendFlags: 0, SourceConstantAlpha: 255, AlphaFormat: AC_SRC_ALPHA as u8 };
    AlphaBlend(dc, r.left, r.top, w, h, mdc, 0, 0, 1, n, bf);
    SelectObject(mdc, old);
    DeleteObject(bmp);
    DeleteDC(mdc);
}

/// Blend a flat `color` at `alpha` over the rectangle.
pub unsafe fn alpha_rect(dc: HDC, r: &RECT, color: u32, alpha: u8) {
    alpha_vgradient(dc, r, color, alpha, alpha);
}

/// Run `f` with the clip region intersected with a rounded rectangle.
pub unsafe fn clip_round(dc: HDC, r: &RECT, radius: i32, f: impl FnOnce()) {
    let rgn = CreateRoundRectRgn(r.left, r.top, r.right + 1, r.bottom + 1, radius * 2, radius * 2);
    clip_rgn(dc, rgn, f);
}

/// Run `f` with the clip region intersected with an ellipse.
pub unsafe fn clip_ellipse(dc: HDC, r: &RECT, f: impl FnOnce()) {
    let rgn = CreateEllipticRgn(r.left, r.top, r.right + 1, r.bottom + 1);
    clip_rgn(dc, rgn, f);
}

/// Run `f` with the clip region intersected with a polygon.
pub unsafe fn clip_polygon(dc: HDC, pts: &[POINT], f: impl FnOnce()) {
    let rgn = CreatePolygonRgn(pts.as_ptr(), pts.len() as i32, WINDING);
    clip_rgn(dc, rgn, f);
}

/// A diagonal glass reflection across the upper left of a rectangle.
pub unsafe fn reflection(dc: HDC, r: &RECT, alpha: u8) {
    let w = r.right - r.left;
    let h = r.bottom - r.top;
    let band = [
        POINT { x: r.left, y: r.top },
        POINT { x: r.left + w * 70 / 100, y: r.top },
        POINT { x: r.left, y: r.top + h * 50 / 100 },
    ];
    clip_polygon(dc, &band, || alpha_vgradient(dc, &rect(r.left, r.top, w, h / 2), theme::WHITE, alpha, 0));
}

unsafe fn clip_rgn(dc: HDC, rgn: HRGN, f: impl FnOnce()) {
    let saved = CreateRectRgn(0, 0, 0, 0);
    let had = GetClipRgn(dc, saved);
    ExtSelectClipRgn(dc, rgn, RGN_AND);
    f();
    SelectClipRgn(dc, if had == 1 { saved } else { 0 });
    DeleteObject(rgn);
    DeleteObject(saved);
}

/// Soft drop shadow under a rounded rectangle.
pub unsafe fn shadow(dc: HDC, r: &RECT, radius: i32, spread: i32) {
    for i in 1..=spread {
        let rr = offset(&inset(r, -i, -i), 0, i);
        let a = (46 / spread).max(6) as u8;
        clip_round(dc, &rr, radius + i, || alpha_rect(dc, &rr, theme::BLACK, a));
    }
}

/// An elevated surface (DESIGN.md 3.4): an optional shadow, a 2 to 3 % vertical
/// gradient, a hairline highlight inside the top edge and a 1 px stroke.
/// That is the whole "glass" effect; there is no sheen.
pub unsafe fn glass(dc: HDC, r: &RECT, radius: i32, top: u32, bottom: u32, border: u32, spread: i32) {
    if spread > 0 {
        shadow(dc, r, radius, spread);
    }
    let w = r.right - r.left;
    clip_round(dc, r, radius, || {
        vgradient(dc, r, top, bottom);
        alpha_rect(dc, &rect(r.left + 1, r.top + 1, w - 2, 1), theme::WHITE, 10);
    });
    round(dc, r, radius, None, Some((border, 1)));
}

/// A flat card: fill, top highlight and stroke, no shadow.
pub unsafe fn card(dc: HDC, r: &RECT, radius: i32) {
    glass(dc, r, radius, theme::CARD_TOP, theme::CARD, theme::LINE, 0);
}

/// A dashed outline (the "add" tile).
pub unsafe fn dashed_round(dc: HDC, r: &RECT, radius: i32, color: u32) {
    let pen = CreatePen(PS_DASH, 1, color);
    let op = SelectObject(dc, pen);
    let ob = SelectObject(dc, GetStockObject(NULL_BRUSH));
    SetBkMode(dc, TRANSPARENT as i32);
    RoundRect(dc, r.left, r.top, r.right, r.bottom, radius * 2, radius * 2);
    SelectObject(dc, ob);
    SelectObject(dc, op);
    DeleteObject(pen);
}

/// Soft radial glow of `color` around (cx, cy).
pub unsafe fn glow(dc: HDC, cx: i32, cy: i32, radius: i32, color: u32, alpha: u8) {
    let rings = 6;
    for i in 0..rings {
        let rr = radius - radius * i / rings;
        let r = rect(cx - rr, cy - rr, rr * 2, rr * 2);
        clip_ellipse(dc, &r, || alpha_rect(dc, &r, color, (alpha as i32 / rings).max(1) as u8));
    }
}

/// Draw a bitmap so that it covers `r` (centre crop, smooth scaling).
pub unsafe fn draw_cover(dc: HDC, r: &RECT, hbm: HBITMAP, bw: i32, bh: i32) {
    let rw = r.right - r.left;
    let rh = r.bottom - r.top;
    if rw <= 0 || rh <= 0 || bw <= 0 || bh <= 0 {
        return;
    }
    let scale = f32::max(rw as f32 / bw as f32, rh as f32 / bh as f32);
    let sw = ((rw as f32 / scale).round() as i32).clamp(1, bw);
    let sh = ((rh as f32 / scale).round() as i32).clamp(1, bh);
    let sx = (bw - sw) / 2;
    let sy = (bh - sh) / 2;
    let mdc = CreateCompatibleDC(dc);
    let old = SelectObject(mdc, hbm);
    SetStretchBltMode(dc, HALFTONE);
    SetBrushOrgEx(dc, 0, 0, ptr::null_mut());
    StretchBlt(dc, r.left, r.top, rw, rh, mdc, sx, sy, sw, sh, SRCCOPY);
    SelectObject(mdc, old);
    DeleteDC(mdc);
}

/// Draw a shell icon at the given size.
pub unsafe fn draw_hicon(dc: HDC, x: i32, y: i32, size: i32, hicon: HICON) {
    DrawIconEx(dc, x, y, hicon, size, size, 0, 0, DI_NORMAL);
}
