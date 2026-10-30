//! Page layouts for the settings window, mirroring Synapse 4's mouse tabs.

#![allow(non_snake_case, unsafe_op_in_unsafe_fn)]

use windows_sys::Win32::Foundation::{POINT, RECT};
use windows_sys::Win32::Graphics::Gdi::HDC;

use super::draw::theme::*;
use super::draw::*;
use super::{FontKind, Id, Kind, Layer, Page, TileArt, TileItem, Ui, Widget};
use crate::accel::Mode;
use crate::config::{MacroConfig, parse_button};
use crate::device::{BUTTONS, TURBO_MAX, TURBO_MIN};
use crate::macros::{self, Playback};

/// Card padding, the label column of a settings row, and the row heights (DESIGN.md 3.3, 4.6).
const PAD: i32 = 16;
const LABEL_W: i32 = 400;
const ROW_H: i32 = 48;
const ROW_H_SUB: i32 = 64;
/// Control heights.
const BUTTON_H: i32 = 32;
const CHIP_H: i32 = 28;
/// Game tile size (2:3 cover) and the gap of the grid.
const TILE_W: i32 = 150;
const TILE_H: i32 = 212;
const TILE_GAP: i32 = 16;

/// Builds widgets top to bottom in logical pixels, grouping them in cards.
pub struct Form {
    scale: f32,
    dc: HDC,
    body: windows_sys::Win32::Graphics::Gdi::HFONT,
    x: i32,
    w: i32,
    y: i32,
    widgets: Vec<Widget>,
    card: Option<(usize, i32)>,
    /// The divider under the last settings row, removed when its card closes.
    last_divider: Option<usize>,
}

impl Form {
    pub fn new(scale: f32, dc: HDC, fonts: &Fonts, x: i32, w: i32) -> Form {
        Form { scale, dc, body: fonts.body, x: (x as f32 / scale) as i32, w: (w as f32 / scale) as i32, y: 24, widgets: Vec::new(), card: None, last_divider: None }
    }

    /// Right edge of the content inside a card.
    fn right(&self) -> i32 {
        self.x + self.w - PAD
    }

    /// Hairline under a settings row (dropped again if it is the card's last row).
    fn divider(&mut self, y: i32) {
        let i = self.push(Id::None, self.x + PAD, y, self.w - 2 * PAD, 1, Kind::Line);
        self.last_divider = Some(i);
    }

    fn s(&self, v: i32) -> i32 {
        (v as f32 * self.scale).round() as i32
    }

    fn measure(&self, font: windows_sys::Win32::Graphics::Gdi::HFONT, t: &str) -> i32 {
        (unsafe { text_width(self.dc, font, t) } as f32 / self.scale).ceil() as i32
    }

    fn push(&mut self, id: Id, x: i32, y: i32, w: i32, h: i32, kind: Kind) -> usize {
        self.widgets.push(Widget { id, rect: rect(self.s(x), self.s(y), self.s(w), self.s(h)), kind, fixed: false });
        self.widgets.len() - 1
    }

    fn cx(&self) -> i32 {
        self.x + PAD + LABEL_W
    }

    fn cw(&self) -> i32 {
        self.w - 2 * PAD - LABEL_W
    }

    /// Start a new card with a section title; closes the previous one.
    pub fn section(&mut self, title: &str) {
        self.section_icon(title, None);
    }

    /// Card with a title (DESIGN.md 4.5): Body strong, sentence case, no icon and no rule.
    /// The icon argument is kept for the call sites but not drawn.
    pub fn section_icon(&mut self, title: &str, _icon: Option<Icon>) {
        self.close_card();
        self.card = Some((self.widgets.len(), self.y));
        self.widgets.push(Widget { id: Id::None, rect: rect(0, 0, 0, 0), kind: Kind::Card, fixed: false });
        self.y += 12;
        self.push(Id::None, self.x + PAD, self.y, self.w - 2 * PAD, 24, Kind::Text { text: title.to_string(), font: FontKind::Bold, color: TEXT, flags: DT_ROW });
        self.y += 28;
    }

    /// A grid of game tiles, as many per row as fit.
    pub fn tiles(&mut self, items: Vec<TileItem>) {
        if items.is_empty() {
            return;
        }
        let cols = ((self.w - 2 * PAD + TILE_GAP) / (TILE_W + TILE_GAP)).max(1) as usize;
        let n = items.len();
        self.y += 4;
        for (k, it) in items.into_iter().enumerate() {
            let col = (k % cols) as i32;
            let x = self.x + PAD + col * (TILE_W + TILE_GAP);
            self.push(it.id, x, self.y, TILE_W, TILE_H, Kind::Tile { title: it.title, sub: it.sub, badge: it.badge, art: it.art, selected: it.selected });
            if k % cols == cols - 1 || k + 1 == n {
                self.y += TILE_H + TILE_GAP;
            }
        }
    }

    fn close_card(&mut self) {
        if let Some((i, y0)) = self.card.take() {
            if let Some(d) = self.last_divider.take() {
                self.widgets[d].rect = rect(0, 0, 0, 0);
            }
            self.y += 8;
            self.widgets[i].rect = rect(self.s(self.x), self.s(y0), self.s(self.w), self.s(self.y - y0));
            self.y += 12;
        }
    }

    pub fn finish(mut self) -> (Vec<Widget>, i32) {
        self.close_card();
        let h = self.s(self.y);
        (self.widgets, h)
    }

    /// Settings row (DESIGN.md 4.6): title in Body strong, description in Caption,
    /// a divider underneath; the caller puts the control at the right. Returns the row height.
    fn label(&mut self, title: &str, sub: &str) -> i32 {
        let h = if sub.is_empty() {
            ROW_H
        } else {
            // The description wraps inside the label column; the row grows with it.
            let lines = (self.measure(self.body, sub) * 12 / 14 / (LABEL_W - 16) + 1).clamp(1, 3);
            ROW_H_SUB + 16 * (lines - 1)
        };
        if sub.is_empty() {
            self.push(Id::None, self.x + PAD, self.y, LABEL_W - 10, h, Kind::Text { text: title.to_string(), font: FontKind::Bold, color: TEXT, flags: DT_ROW });
        } else {
            self.push(Id::None, self.x + PAD, self.y + 12, LABEL_W - 16, 20, Kind::Text { text: title.to_string(), font: FontKind::Bold, color: TEXT, flags: DT_ROW });
            self.push(Id::None, self.x + PAD, self.y + 32, LABEL_W - 16, h - 40, Kind::Text { text: sub.to_string(), font: FontKind::Small, color: MUTED, flags: DT_LEFT_ | DT_WORDBREAK_ | DT_NOPREFIX_ | DT_END_ELLIPSIS_ });
        }
        self.divider(self.y + h - 1);
        h
    }

    pub fn note(&mut self, text: &str) {
        // Descriptions wrap at about 60 characters (DESIGN.md 3.2).
        let w = (self.w - 2 * PAD).min(560);
        let lines = (self.measure(self.body, text) / w.max(1) + 1).clamp(1, 4);
        let h = 16 * lines + 12;
        self.push(Id::None, self.x + PAD, self.y, w, h, Kind::Text { text: text.to_string(), font: FontKind::Small, color: MUTED, flags: DT_LEFT_ | DT_WORDBREAK_ | DT_NOPREFIX_ });
        self.y += h;
    }

    pub fn info(&mut self, title: &str, value: &str) {
        self.push(Id::None, self.x + PAD, self.y, LABEL_W - 10, 36, Kind::Text { text: title.to_string(), font: FontKind::Body, color: MUTED, flags: DT_ROW });
        self.push(Id::None, self.cx(), self.y, self.cw(), 36, Kind::Text { text: value.to_string(), font: FontKind::Body, color: TEXT, flags: DT_ROW });
        self.divider(self.y + 35);
        self.y += 36;
    }

    pub fn toggle(&mut self, title: &str, sub: &str, on: bool, id: Id) {
        let h = self.label(title, sub);
        self.push(id, self.right() - 40, self.y + (h - 20) / 2, 40, 20, Kind::Toggle { on });
        self.y += h;
    }

    /// Chip spans for `options`, relative to the group's left edge; returns the spans and the total width.
    fn chip_spans(&self, options: &[String]) -> (Vec<(i32, i32)>, i32) {
        let mut xs = Vec::new();
        let mut x = 0;
        for o in options {
            let w = self.measure(self.body, o) + 24;
            xs.push((self.s(x), self.s(x + w)));
            x += w + 8;
        }
        (xs, (x - 8).max(0))
    }

    /// Chips right-aligned in a settings row.
    pub fn chips(&mut self, title: &str, sub: &str, options: &[String], selected: Option<usize>, id: Id) {
        let h = self.label(title, sub);
        let (xs, total) = self.chip_spans(options);
        let w = total.min(self.cw());
        self.push(id, self.right() - w, self.y + (h - CHIP_H) / 2, w, CHIP_H, Kind::Chips { options: options.to_vec(), selected, xs });
        self.y += h;
    }

    /// Chips spanning the whole card width, with the title on its own line (for long option lists).
    pub fn chips_wide(&mut self, title: &str, options: &[String], selected: Option<usize>, id: Id) {
        if !title.is_empty() {
            self.push(Id::None, self.x + PAD, self.y, self.w - 2 * PAD, 24, Kind::Text { text: title.to_string(), font: FontKind::Bold, color: TEXT, flags: DT_ROW });
            self.y += 28;
        }
        let (xs, _) = self.chip_spans(options);
        self.push(id, self.x + PAD, self.y, self.w - 2 * PAD, CHIP_H, Kind::Chips { options: options.to_vec(), selected, xs });
        self.y += CHIP_H + 12;
    }

    #[allow(clippy::too_many_arguments)]
    pub fn slider(&mut self, title: &str, sub: &str, min: i32, max: i32, step: i32, value: i32, fmt: fn(i32) -> String, id: Id) {
        self.slider_opt(title, sub, min, max, step, value, fmt, id, true);
    }

    /// Slider that can be greyed out (shown, but not adjustable).
    #[allow(clippy::too_many_arguments)]
    pub fn slider_opt(&mut self, title: &str, sub: &str, min: i32, max: i32, step: i32, value: i32, fmt: fn(i32) -> String, id: Id, enabled: bool) {
        let h = self.label(title, sub);
        self.push(id, self.cx(), self.y, self.cw(), h, Kind::Slider { min, max, step, value: value.clamp(min, max), fmt, ticks: Vec::new(), show_value: true, enabled });
        self.y += h;
    }

    /// Warning line in amber with its icon, e.g. why a control above is greyed out.
    pub fn warning(&mut self, text: &str) {
        self.push(Id::None, self.x + PAD, self.y + 4, 16, 16, Kind::Icon { icon: Icon::Warning, color: WARN });
        self.push(Id::None, self.x + PAD + 24, self.y, self.w - 2 * PAD - 24, 24, Kind::Text { text: text.to_string(), font: FontKind::Small, color: WARN, flags: DT_ROW });
        self.y += 28;
    }

    /// Discrete slider with one tick label per step.
    pub fn ticks(&mut self, title: &str, sub: &str, labels: &[String], selected: usize, id: Id) {
        let h = self.label(title, sub).max(64);
        let n = labels.len().max(1) as i32;
        self.push(id, self.cx(), self.y, self.cw(), h, Kind::Slider { min: 0, max: n - 1, step: 1, value: (selected as i32).min(n - 1), fmt: |v| v.to_string(), ticks: labels.to_vec(), show_value: false, enabled: true });
        self.y += h;
    }

    /// A DPI stage row: radio (active stage), slider, and a box to type the value.
    /// With `y`, a second row for the vertical DPI follows (independent X/Y).
    pub fn stage(&mut self, i: usize, dpi: u16, y: Option<u16>, active: bool, selectable: bool, dpi_max: u16) {
        let h = ROW_H;
        let title = if selectable { format!("Stage {}", i + 1) } else { "Sensitivity".to_string() };
        let title = if y.is_some() { format!("{title}  ·  X") } else { title };
        let title = if active && selectable { format!("{title}  ·  Active") } else { title };
        if selectable {
            self.push(Id::StageActive(i), self.x + PAD, self.y, LABEL_W - 10, h, Kind::Radio { on: active, text: title });
        } else {
            self.push(Id::None, self.x + PAD, self.y, LABEL_W - 10, h, Kind::Text { text: title, font: FontKind::Bold, color: TEXT, flags: DT_ROW });
        }
        self.dpi_controls(Id::StageValue(i), dpi, h, dpi_max);
        self.y += h;
        if let Some(yd) = y {
            self.push(Id::None, self.x + PAD + 28, self.y, LABEL_W - 38, h, Kind::Text { text: "Y".to_string(), font: FontKind::Body, color: MUTED, flags: DT_ROW });
            self.dpi_controls(Id::StageY(i), yd, h, dpi_max);
            self.y += h;
        }
        self.divider(self.y - 1);
    }

    fn dpi_controls(&mut self, id: Id, dpi: u16, h: i32, dpi_max: u16) {
        let edit_w = 88;
        self.push(id, self.cx(), self.y, self.cw() - edit_w - 16, h, Kind::Slider { min: 100, max: dpi_max as i32, step: 50, value: dpi as i32, fmt: |v| v.to_string(), ticks: Vec::new(), show_value: false, enabled: true });
        self.push(id, self.right() - edit_w, self.y + (h - BUTTON_H) / 2, edit_w, BUTTON_H, Kind::Edit { text: dpi.to_string(), numeric: true });
    }

    /// Acceleration curve plot (Dynamic Sensitivity); editable curves take an id.
    pub fn curve(&mut self, points: &[(u8, f32)], editable: bool, selected: Option<usize>) {
        let h = 230;
        let samples = crate::accel::curve(points).to_vec();
        let id = if editable { Id::Curve } else { Id::None };
        self.push(id, self.x + PAD, self.y, self.w - 2 * PAD, h, Kind::Curve { points: points.to_vec(), samples, editable, selected });
        self.y += h + 8;
    }

    pub fn edit(&mut self, title: &str, sub: &str, text: &str, numeric: bool, id: Id) {
        let h = self.label(title, sub);
        let w = self.cw().min(if numeric { 88 } else { 320 });
        self.push(id, self.right() - w, self.y + (h - BUTTON_H) / 2, w, BUTTON_H, Kind::Edit { text: text.to_string(), numeric });
        self.y += h;
    }

    /// A search box with its icon, full width of the card.
    pub fn search(&mut self, text: &str, id: Id) {
        self.push(Id::None, self.x + PAD + 4, self.y + (BUTTON_H - 16) / 2, 16, 16, Kind::Icon { icon: Icon::Search, color: MUTED });
        self.push(id, self.x + PAD + 28, self.y, (self.w - 2 * PAD - 28).min(320), BUTTON_H, Kind::Edit { text: text.to_string(), numeric: false });
        self.y += BUTTON_H + 12;
    }

    pub fn buttons(&mut self, items: &[(&str, Id, bool)]) {
        let mut x = self.x + PAD;
        self.y += 4;
        for (t, id, primary) in items {
            let icon = button_icon(*id, t);
            let w = self.measure(self.body, t) + 24 + if icon.is_some() { 24 } else { 0 };
            self.push(*id, x, self.y, w, BUTTON_H, Kind::Button { text: t.to_string(), primary: *primary, enabled: *id != Id::None, icon });
            x += w + 8;
        }
        self.y += BUTTON_H + 8;
    }

    pub fn battery(&mut self) {
        self.push(Id::None, self.x + PAD, self.y, self.w - 2 * PAD, 72, Kind::Battery);
        self.divider(self.y + 79);
        self.y += 80;
    }

    pub fn gap(&mut self, h: i32) {
        self.y += h;
    }

    /// One feature line on the Roadmap page: status pill, name, and a muted note.
    pub fn status(&mut self, st: Status, title: &str, note: &str) {
        let h = 36;
        let badge_w = 72;
        self.push(Id::None, self.x + PAD, self.y + 4, 16, 16 + 12, Kind::Icon { icon: st.icon(), color: st.color() });
        self.push(Id::None, self.x + PAD + 24, self.y + (h - 22) / 2, badge_w, 22, Kind::Badge { text: st.label().to_string(), color: st.color() });
        let title_x = self.x + PAD + 24 + badge_w + 12;
        let title_w = LABEL_W - badge_w - 36;
        self.push(Id::None, title_x, self.y, title_w, h, Kind::Text { text: title.to_string(), font: FontKind::Bold, color: TEXT, flags: DT_ROW });
        let note_x = title_x + title_w + 12;
        self.push(Id::None, note_x, self.y, (self.x + self.w - PAD - note_x).max(40), h, Kind::Text { text: note.to_string(), font: FontKind::Small, color: MUTED, flags: DT_ROW });
        self.divider(self.y + h - 1);
        self.y += h;
    }

    /// Bulleted roadmap item.
    pub fn bullet(&mut self, color: u32, text: &str) {
        let w = self.w - 2 * PAD - 24;
        let lines = (self.measure(self.body, text) / w.max(1) + 1).clamp(1, 4);
        let h = 20 * lines + 8;
        self.push(Id::None, self.x + PAD + 4, self.y + 4, 12, 20, Kind::Bullet { color });
        self.push(Id::None, self.x + PAD + 24, self.y + 4, w, h - 4, Kind::Text { text: text.to_string(), font: FontKind::Body, color: TEXT, flags: DT_LEFT_ | DT_WORDBREAK_ | DT_NOPREFIX_ });
        self.y += h;
    }
}

/// Icon in front of a button's label, by what the button does.
fn button_icon(id: Id, label: &str) -> Option<Icon> {
    Some(match id {
        Id::GameScan | Id::Refresh => Icon::Refresh,
        Id::GameBrowse | Id::OpenConfig => Icon::Folder,
        Id::GameRemove | Id::MacroDelete => Icon::Trash,
        Id::GameButtons => Icon::Mouse,
        Id::MacroNew | Id::MacroRecord => Icon::Record,
        Id::MacroStop => Icon::Stop,
        Id::Reimport => Icon::Sync,
        Id::Reapply => Icon::Check,
        Id::PairStart => Icon::Link,
        Id::PairCancel | Id::Unpair => Icon::Stop,
        Id::CurveAdd => Icon::Add,
        Id::CurveRemove => Icon::Trash,
        Id::None if label.starts_with("Scanning") => Icon::Refresh,
        _ => return None,
    })
}

/// Implementation state of a Synapse feature, for the Roadmap page.
#[derive(Clone, Copy)]
pub enum Status {
    Done,
    Partial,
    Todo,
    Never,
}

impl Status {
    fn label(self) -> &'static str {
        match self {
            Status::Done => "Done",
            Status::Partial => "Partial",
            Status::Todo => "Planned",
            Status::Never => "No",
        }
    }

    /// Colour is never the only signal (DESIGN.md 6): each state has an icon too.
    fn icon(self) -> Icon {
        match self {
            Status::Done => Icon::Check,
            Status::Partial => Icon::Warning,
            Status::Todo => Icon::Flag,
            Status::Never => Icon::Stop,
        }
    }

    fn color(self) -> u32 {
        match self {
            Status::Done => ACCENT,
            Status::Partial => WARN,
            Status::Todo => BLUE,
            Status::Never => MUTED,
        }
    }
}

// ----------------------------------------------------------------------
// Pages
// ----------------------------------------------------------------------

pub fn build(ui: &Ui, f: &mut Form) {
    if let Some(e) = ui.config_error() {
        f.section_icon("Config file problem", Some(Icon::Warning));
        f.note(&format!("{e}. Showing defaults; fix the file or press \"Re-import from mouse\" in Settings."));
    }
    match ui.page {
        Page::Performance => performance(ui, f),
        Page::Calibration => calibration(ui, f),
        Page::Power => power(ui, f),
        Page::Games => games(ui, f),
        Page::Customize => customize(ui, f),
        Page::Roadmap => roadmap(f),
        Page::App => app(ui, f),
    }
}

fn hz_labels(rates: &[u16]) -> Vec<String> {
    rates.iter().map(|r| format!("{r}")).collect()
}

fn performance(ui: &Ui, f: &mut Form) {
    let cfg = &ui.cfg;
    f.section_icon("Sensitivity", Some(Icon::Speed));
    let staged = cfg.dpi.stages.len() > 1;
    f.toggle("Enable DPI stages", "Cycle through them with the button under the mouse", staged, Id::StagesEnabled);
    let xy = !cfg.dpi.y_stages.is_empty();
    f.toggle("Independent X and Y", "Separate horizontal and vertical sensitivity", xy, Id::XySplit);
    let y_of = |i: usize| if xy { cfg.dpi.y_stages.get(i).copied() } else { None };
    if staged {
        let counts: Vec<String> = (1..=5).map(|n| n.to_string()).collect();
        f.chips("Number of stages", "", &counts, Some(cfg.dpi.stages.len() - 1), Id::StageCount);
        f.gap(4);
        for (i, d) in cfg.dpi.stages.iter().enumerate() {
            f.stage(i, *d, y_of(i), cfg.dpi.active as usize == i + 1, true, ui.spec.dpi_max);
        }
        f.note(&format!("Select the stage to use now. Range 100 to {} DPI in steps of 50; type an exact value in the box.", ui.spec.dpi_max));
    } else {
        f.stage(0, cfg.dpi.stages[0], y_of(0), true, false, ui.spec.dpi_max);
        f.note(&format!("Range 100 to {} DPI in steps of 50; type an exact value in the box.", ui.spec.dpi_max));
    }

    if ui.spec.supports_acceleration() {
        f.section_icon("Dynamic Sensitivity", Some(Icon::Sliders));
        let a = &cfg.acceleration;
        f.toggle("Enable Dynamic Sensitivity", "The mouse scales its sensitivity with how fast you move it (stored onboard)", a.enabled, Id::AccelOn);
        if a.enabled {
            let mode = a.mode().unwrap_or(Mode::Classic);
            let modes = [Mode::Classic, Mode::Natural, Mode::Jump, Mode::Custom].map(|m| m.label().to_string());
            f.chips("Curve", "", &modes, Some(mode as usize), Id::AccelMode);
            f.gap(4);
            if mode == Mode::Custom {
                f.curve(&a.curve, true, ui.curve_sel);
                let can_remove = a.curve.len() > 2 && ui.curve_sel.is_some_and(|i| i > 0);
                let can_add = a.curve.len() < crate::accel::MAX_CONTROL_POINTS;
                f.buttons(&[
                    ("Add point", if can_add { Id::CurveAdd } else { Id::None }, false),
                    ("Remove point", if can_remove { Id::CurveRemove } else { Id::None }, false),
                    ("Start from Classic", Id::CurveTemplate(0), false),
                    ("Natural", Id::CurveTemplate(1), false),
                    ("Jump", Id::CurveTemplate(2), false),
                ]);
                f.note("Drag the points: horizontal is the speed of the movement, vertical the sensitivity multiplier (0.1 to 1.5). The curve stays flat after the last point. Click a point, then \"Remove point\" to delete it.");
            } else {
                f.curve(&mode.template(), false, None);
                f.note("Classic, Natural and Jump are built into the mouse's firmware; the plot shows Synapse's drawing of each. Pick Custom to shape your own curve.");
            }
        }
    }
    f.section_icon("Polling rate", Some(Icon::Bolt));
    let rates: &[u16] = ui.spec.polling_rates;
    let sel = rates.iter().position(|r| *r == cfg.polling.rate).unwrap_or(0);
    f.ticks("Polling rate (Hz)", "Reports per second. 4000 and 8000 need the HyperPolling dongle and use more battery", &hz_labels(rates), sel, Id::Polling);

    f.note("Games can switch to their own polling rate, DPI stage and buttons while they run: see the Games page.");

    if ui.spec.supports_rotation() {
        f.section_icon("Rotation", Some(Icon::Sync));
        let on = cfg.sensor.rotation != 0;
        f.toggle("Enable rotation", "Compensate a tilted grip by rotating the sensor's axes", on, Id::RotationOn);
        if on {
            f.slider("Angle", "", -30, 30, 1, cfg.sensor.rotation as i32, |v| format!("{v}°"), Id::Rotation);
        }
    }
}

/// "1600 DPI  ·  2000 Hz" for a profile's tile.
fn tile_summary(cfg: &crate::config::Config, g: &crate::config::GameConfig) -> String {
    let stage = match g.dpi_stage.and_then(|s| cfg.dpi.stages.get(s as usize - 1)) {
        Some(d) => format!("{d} DPI"),
        None => "DPI kept".to_string(),
    };
    let polling = match g.polling {
        Some(p) => format!("{p} Hz"),
        None => format!("{} Hz", cfg.polling.game_rate),
    };
    format!("{stage}  ·  {polling}")
}

/// Launcher name shown on a tile while hovering it.
fn badge_of(source: Option<&str>) -> &'static str {
    match source {
        Some("Steam") => "Steam",
        Some("Epic Games") => "Epic Games",
        Some("GOG") => "GOG",
        Some("Xbox") => "Xbox",
        Some("EA app") => "EA app",
        Some("Battle.net") => "Battle.net",
        Some("Riot") => "Riot",
        Some("Ubisoft Connect") => "Ubisoft Connect",
        Some("Rockstar") => "Rockstar",
        Some(_) => "Launcher",
        None => "Program",
    }
}

fn games(ui: &Ui, f: &mut Form) {
    let cfg = &ui.cfg;
    let rates: &[u16] = ui.spec.polling_rates;

    f.section_icon("Game profiles", Some(Icon::Game));
    if cfg.games.is_empty() {
        f.note("No game profiles yet. Click an installed game below, or add a program, to give it its own DPI stage, polling rate and buttons. The tray app switches the mouse to them while the game is in the foreground.");
    } else {
        f.note("Click a profile to change it. The tray app applies it about two seconds after the game takes the foreground and restores your normal settings when you leave it.");
    }
    let mut tiles: Vec<TileItem> = cfg
        .games
        .iter()
        .enumerate()
        .map(|(i, g)| TileItem {
            id: Id::GameRow(i),
            title: g.label(),
            sub: tile_summary(cfg, g),
            badge: badge_of(ui.game_source(g)),
            art: ui.profile_art(g),
            selected: ui.game_open && ui.sel_game == i,
        })
        .collect();
    // The last tile adds any program (games outside a launcher).
    tiles.push(TileItem { id: Id::GameBrowse, title: "Add a program".to_string(), sub: "Any .exe".to_string(), badge: "", art: TileArt::Add, selected: false });
    f.tiles(tiles);

    if let Some(g) = cfg.games.get(ui.sel_game).filter(|_| ui.game_open) {
        f.section_icon(&format!("Profile: {}", g.label()), Some(Icon::Sliders));
        game_settings(ui, f, g, rates);
    }

    f.section_icon("Installed games", Some(Icon::Folder));
    let free = ui.games_without_profile();
    let sources = ui.library_sources();
    if ui.scanning && ui.library.is_empty() {
        f.note("Looking for installed games...");
    } else if ui.library.is_empty() {
        f.note("No games found in Steam, Epic Games, GOG, EA app, Battle.net, Riot, Ubisoft Connect, Rockstar or the Xbox app.");
    } else {
        f.note(&format!("{} games found. Click one to create its profile.", ui.library.len()));
    }
    // Filter chips per launcher (DESIGN.md 4.13), with counts, then the search box.
    if !sources.is_empty() {
        let mut chips = vec![format!("All {}", ui.library.len())];
        chips.extend(sources.iter().map(|s| format!("{s} {}", ui.library.iter().filter(|l| l.source == *s).count())));
        let sel = ui.lib_source.and_then(|s| sources.iter().position(|x| *x == s)).map(|i| i + 1).unwrap_or(0);
        f.chips_wide("", &chips, Some(sel), Id::GameSource);
    }
    if free.len() > 8 {
        f.search(&ui.game_filter, Id::GameFilter);
    }
    let filter = ui.game_filter.trim().to_lowercase();
    let shown: Vec<TileItem> = free
        .into_iter()
        .filter(|j| filter.is_empty() || ui.library[*j].name.to_lowercase().contains(&filter))
        .filter(|j| ui.lib_source.is_none_or(|s| ui.library[*j].source == s))
        .map(|j| {
            let l = &ui.library[j];
            TileItem { id: Id::GameLib(j), title: l.name.clone(), sub: String::new(), badge: badge_of(Some(l.source)), art: TileArt::Lib(j), selected: false }
        })
        .collect();
    let none = shown.is_empty();
    f.tiles(shown);
    if none && (!filter.is_empty() || ui.lib_source.is_some()) {
        f.note("No game matches the search.");
    }
    f.buttons(&[(if ui.scanning { "Scanning..." } else { "Scan again" }, if ui.scanning { Id::None } else { Id::GameScan }, false)]);

    f.section_icon("In-game polling rate", Some(Icon::Bolt));
    let gsel = rates.iter().position(|r| *r == cfg.polling.game_rate).unwrap_or(0);
    f.ticks("Rate while gaming (Hz)", "Used by game profiles set to \"In-game rate\"", &hz_labels(rates), gsel, Id::GameRate);
    if !cfg.polling.games.is_empty() {
        f.edit("More programs", "Executable names, comma separated: they get the in-game rate only", &cfg.polling.games.join(", "), false, Id::GamesEdit);
    }
    f.note("Like Synapse's in-game polling rate. Higher rates use more battery, so the rate only goes up while a game is in the foreground.");
}

/// The settings of the open game profile.
fn game_settings(ui: &Ui, f: &mut Form, g: &crate::config::GameConfig, rates: &[u16]) {
    let cfg = &ui.cfg;
    let mut stages = vec!["Keep".to_string()];
    stages.extend((1..=cfg.dpi.stages.len()).map(|i| format!("Stage {i} ({})", cfg.dpi.stages[i - 1])));
    f.chips_wide("DPI stage while playing", &stages, Some(g.dpi_stage.map(|s| s as usize).filter(|s| *s <= cfg.dpi.stages.len()).unwrap_or(0)), Id::GameStage);
    if cfg.dpi.stages.len() < 2 {
        f.note("Only one DPI stage is set up. Enable DPI stages on the Performance page to give games their own sensitivity.");
    }
    let mut polls = vec![format!("In-game rate ({})", cfg.polling.game_rate)];
    polls.extend(rates.iter().map(|r| format!("{r} Hz")));
    let psel = g.polling.and_then(|p| rates.iter().position(|r| *r == p)).map(|i| i + 1).unwrap_or(0);
    f.chips_wide("Polling rate while playing", &polls, Some(psel), Id::GamePolling);
    f.edit("Name", "Shown here and in the tray tooltip. Press Enter to save", &g.label(), false, Id::GameName);
    let exe_sub = if g.dir.is_empty() { "Executable name, e.g. cs2.exe. Press Enter to save" } else { "Optional: every program in the game's folder already counts" };
    f.edit("Program", exe_sub, &g.exe, false, Id::GameExe);
    if !g.dir.is_empty() {
        f.info("Folder", &g.dir);
    }
    let n = g.buttons.len();
    f.info("Buttons", &if n == 0 { "same as the Normal layout".to_string() } else { format!("{n} assignment{} of its own, the rest as in the Normal layout", if n == 1 { "" } else { "s" }) });
    f.buttons(&[("Edit button layout", Id::GameButtons, true), ("Remove profile", Id::GameRemove, false)]);
}

fn calibration(ui: &Ui, f: &mut Form) {
    use crate::sensor::{ASYM_LAND_MIN, ASYM_LIFT_MAX, ASYM_LIFT_MIN};
    let s = &ui.cfg.sensor;
    f.section_icon("Smart Tracking", Some(Icon::Track));
    let modes = ["Smart Tracking", "Keep current"].map(String::from);
    let sel = if s.tracking == "smart" { Some(0) } else { Some(1) };
    f.chips("Tracking", "The two lift-off modes Synapse offers for this mouse", &modes, sel, Id::TrackMode);
    if s.tracking != "smart" {
        f.note("The mouse keeps whatever lift-off setting is stored in it. Pick Smart Tracking to set it from here.");
        return;
    }
    f.toggle("Asymmetric cut-off", "Separate lift-off and landing levels instead of one distance", s.asymmetric, Id::Asym);
    if s.asymmetric {
        f.slider("Lift-off", "Tracking stops at this level (low to high)", ASYM_LIFT_MIN as i32, ASYM_LIFT_MAX as i32, 1, s.lift as i32, |v| v.to_string(), Id::Lift);
        f.slider("Landing", "Tracking resumes at this level, below the lift-off", ASYM_LAND_MIN as i32, (s.lift as i32 - 1).max(ASYM_LAND_MIN as i32), 1, s.land as i32, |v| v.to_string(), Id::Land);
        f.note("Same sliders as Synapse's Asymmetric Cut-off for the Viper V3 Pro (lift-off 2..26, landing 1..25, Synapse's defaults 13 / 12). The mouse accepts the values but cannot read them back, so they are rewritten on every apply.");
    } else {
        f.slider("Lift-off distance", "1 mm is the lowest the sensor supports", 1, 3, 1, s.lift as i32, |v| format!("{v} mm"), Id::Lift);
        f.note("Smart Tracking lets the sensor adapt to the surface by itself. Synapse offers no surface calibration or mouse-mat presets for this mouse, so neither does freenapse.");
    }
}

fn power(ui: &Ui, f: &mut Form) {
    let cfg = &ui.cfg;
    f.section_icon("Battery", Some(Icon::Battery));
    f.battery();
    f.slider("Battery notification", "The tray app pops a notification at this level", 5, 50, 1, cfg.tray.notify_percent as i32, |v| format!("{v} %"), Id::NotifyPercent);

    f.section_icon("Power saving", Some(Icon::Power));
    f.slider("Idle sleep", "Sleep after this much inactivity (wakes on the first movement)", 1, 15, 1, (cfg.power.idle_seconds / 60) as i32, |v| if v == 1 { "1 minute".to_string() } else { format!("{v} minutes") }, Id::Idle);
    // Same rule as Synapse: the firmware only honours low power mode at 1000 Hz or less.
    let lpm_ok = cfg.polling.rate <= 1000;
    f.slider_opt("Low power mode", "When wireless, the mouse enters low power mode once the battery is below this level", 5, 100, 5, cfg.power.low_battery_percent as i32, |v| format!("{v} %"), Id::LowBattery, lpm_ok);
    if !lpm_ok {
        f.warning("Low power mode is not supported at polling rates of 2000 Hz and above. Lower the polling rate to use it.");
    }

    if ui.wireless && ui.spec.pairing {
        f.section_icon("HyperPolling wireless dongle", Some(Icon::Wifi));
        let leds = ["Battery status", "Polling rate", "Off"].map(String::from);
        f.chips("Indicator LED", "What the light on the dongle shows", &leds, Some((cfg.dongle.led as usize).clamp(1, 3) - 1), Id::DongleLed);
    }
}

pub const CATEGORIES: [&str; 9] = ["Default", "Mouse", "Keyboard", "DPI", "Multimedia", "System", "Macro", "Hypershift", "Disabled"];
pub const CAT_MOUSE: usize = 1;
pub const CAT_KEYBOARD: usize = 2;
pub const CAT_SYSTEM: usize = 5;
pub const CAT_MACRO: usize = 6;
pub const CAT_HYPERSHIFT: usize = 7;
pub const CAT_DISABLED: usize = 8;

pub fn category_of(spec: Option<&str>) -> usize {
    match spec {
        None | Some("default") => 0,
        Some(s) if s.starts_with("mouse:") || s.starts_with("turbo:") || s == "double-click" => CAT_MOUSE,
        Some(s) if s.starts_with("key:") || s.starts_with("turbo-key:") => CAT_KEYBOARD,
        Some(s) if s.starts_with("dpi") => 3,
        Some(s) if s.starts_with("media:") => 4,
        Some(s) if s.starts_with("system:") => CAT_SYSTEM,
        Some(s) if s.starts_with("macro") => CAT_MACRO,
        Some("hypershift") => CAT_HYPERSHIFT,
        Some("disabled") => CAT_DISABLED,
        _ => 0,
    }
}

/// Playback options in the order of the chips on the Customize page.
pub const PLAYBACKS: [&str; 5] = ["Play once", "N times", "While held", "Toggle", "Sequence"];

pub fn playback_index(p: Playback) -> usize {
    match p {
        Playback::Times(1) => 0,
        Playback::Times(_) => 1,
        Playback::Held => 2,
        Playback::Toggle => 3,
        Playback::Sequence => 4,
    }
}

/// Split a turbo assignment into its plain form and the repeat rate:
/// `turbo:left:10` -> (`mouse:left`, Some(10)); `key:c` -> (`key:c`, None).
pub fn turbo_split(spec: &str) -> (String, Option<u8>) {
    let rate = |rest: &str| rest.rsplit_once(':').map(|(base, n)| (base.to_string(), n.parse::<u8>().ok()));
    if let Some((base, n)) = spec.strip_prefix("turbo:").and_then(rate) {
        return (format!("mouse:{base}"), n);
    }
    if let Some((base, n)) = spec.strip_prefix("turbo-key:").and_then(rate) {
        return (format!("key:{base}"), n);
    }
    (spec.to_string(), None)
}

/// Inverse of [`turbo_split`]: add a repeat rate to a mouse or key assignment.
pub fn with_turbo(base: &str, rate: Option<u8>) -> String {
    let Some(n) = rate else { return base.to_string() };
    let n = n.clamp(TURBO_MIN, TURBO_MAX);
    if let Some(b) = base.strip_prefix("mouse:") {
        format!("turbo:{b}:{n}")
    } else if let Some(k) = base.strip_prefix("key:") {
        format!("turbo-key:{k}:{n}")
    } else {
        base.to_string()
    }
}

/// (label, spec) choices inside a category.
pub fn options_for(cat: usize, stages: usize) -> Vec<(String, String)> {
    match cat {
        CAT_MOUSE => {
            let mut v: Vec<(String, String)> = BUTTONS.iter().map(|(n, _)| (pretty_button(n), format!("mouse:{n}"))).collect();
            v.push(("Double click".to_string(), "double-click".to_string()));
            v
        }
        CAT_SYSTEM => [("Sleep", "sleep"), ("Power", "power"), ("Wake", "wake")].iter().map(|(l, s)| (l.to_string(), format!("system:{s}"))).collect(),
        3 => {
            let mut v = vec![("Cycle up".to_string(), "dpi-up".to_string()), ("Cycle down".to_string(), "dpi-down".to_string())];
            for i in 1..=stages.clamp(1, 5) {
                v.push((format!("Stage {i}"), format!("dpi-stage:{i}")));
            }
            v
        }
        4 => [("Play / Pause", "play-pause"), ("Mute", "mute"), ("Volume up", "vol-up"), ("Volume down", "vol-down"), ("Next track", "next"), ("Previous track", "prev"), ("Stop", "stop")]
            .iter()
            .map(|(l, s)| (l.to_string(), format!("media:{s}")))
            .collect(),
        _ => Vec::new(),
    }
}

fn pretty_button(name: &str) -> String {
    match name {
        "left" => "Left click",
        "right" => "Right click",
        "middle" => "Middle click",
        "back" => "Back",
        "forward" => "Forward",
        "scroll-up" => "Scroll up",
        "scroll-down" => "Scroll down",
        other => other,
    }
    .to_string()
}

/// Human description of an assignment for the button list.
pub fn pretty_spec(spec: Option<&str>, own: &str, macros_: &[MacroConfig]) -> String {
    if let Some(s) = spec {
        let (base, rate) = turbo_split(s);
        if let Some(n) = rate {
            return format!("{} · turbo {n}/s", pretty_spec(Some(&base), own, macros_));
        }
        if let Some((id, play)) = parse_button(s).ok().flatten().and_then(|f| f.macro_binding()) {
            let name = macros_.iter().find(|m| m.id == id).map(|m| m.name.clone()).unwrap_or_else(|| format!("macro {id}"));
            return format!("Macro: {name} ({})", play.label().to_lowercase());
        }
    }
    match spec {
        None | Some("default") => format!("{} (default)", pretty_button(own)),
        Some("disabled") => "Disabled".into(),
        Some("hypershift") => "Hypershift".into(),
        Some("double-click") => "Double click".into(),
        Some("system:sleep") => "Sleep".into(),
        Some("system:power") => "Power".into(),
        Some("system:wake") => "Wake".into(),
        Some("dpi-up") => "DPI cycle up".into(),
        Some("dpi-down") => "DPI cycle down".into(),
        Some(s) if s.starts_with("dpi-stage:") => format!("DPI stage {}", &s[10..]),
        Some(s) if s.starts_with("mouse:") => pretty_button(&s[6..]),
        Some(s) if s.starts_with("media:") => options_for(4, 1).iter().find(|(_, v)| v == s).map(|(l, _)| l.clone()).unwrap_or_else(|| s.to_string()),
        Some("key:") => "Keyboard: type a key".into(),
        Some(s) if s.starts_with("key:") => {
            let parts: Vec<String> = s[4..]
                .split('+')
                .map(|p| {
                    let p = p.trim();
                    if p.len() == 1 { p.to_uppercase() } else { let mut c = p.chars(); c.next().map(|x| x.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default() }
                })
                .collect();
            format!("Key: {}", parts.join(" + "))
        }
        Some(s) => s.to_string(),
    }
}

fn customize(ui: &Ui, f: &mut Form) {
    let cfg = &ui.cfg;
    f.section_icon("Button assignment", Some(Icon::Mouse));
    let mut layers = vec!["Normal".to_string(), "Hypershift".to_string()];
    layers.extend(cfg.games.iter().map(|g| {
        let l = g.label();
        if l.chars().count() > 20 { format!("{}...", l.chars().take(18).collect::<String>().trim_end()) } else { l }
    }));
    let layer_sel = match ui.layer {
        Layer::Normal => 0,
        Layer::Hypershift => 1,
        Layer::Game(i) => 2 + i.min(cfg.games.len().saturating_sub(1)),
    };
    f.chips_wide("Layer", &layers, Some(layer_sel), Id::Layer);
    f.gap(2);

    // Left: list of the physical buttons. Right: the mouse with clickable areas.
    let list_w = 300;
    let row_h = 50;
    let y0 = f.y;
    let hs = ui.layer == Layer::Hypershift;
    let layer_buttons = ui.layer_buttons();
    for (i, (name, _)) in BUTTONS.iter().enumerate() {
        let key = if hs { format!("hypershift.{name}") } else { name.to_string() };
        let mut spec = layer_buttons.get(&key).map(String::as_str);
        let mut inherited = false;
        if let Layer::Game(_) = ui.layer {
            if spec.is_none() {
                spec = cfg.buttons.get(&key).map(String::as_str);
                inherited = spec.is_some();
            }
        }
        let mut sub = pretty_spec(spec, name, &cfg.macros);
        if inherited {
            sub.push_str(" · from Normal");
        }
        f.push(Id::SelectButton(i), f.x + PAD, f.y, list_w, row_h - 4, Kind::Row { title: pretty_button(name), sub, selected: ui.sel_button == i });
        f.y += row_h;
    }
    let art_h = row_h * BUTTONS.len() as i32 - 4;
    let art_x = f.x + PAD + list_w + 24;
    let art_w = f.w - 2 * PAD - list_w - 24;
    let regions = mouse_regions(f, art_x, y0, art_w, art_h);
    f.push(Id::SelectButton(ui.sel_button), art_x, y0, art_w, art_h, Kind::MouseArt { highlight: ui.sel_button, regions });
    if hs && !cfg.buttons.values().any(|v| v == "hypershift") {
        f.gap(6);
        f.note("No button is assigned to Hypershift yet, so this layer cannot be reached. Assign Hypershift to a button in the normal layer first.");
    }
    if let Layer::Game(i) = ui.layer {
        f.gap(6);
        let exe = cfg.games.get(i).map(|g| g.label()).unwrap_or_else(|| "?".into());
        f.note(&format!("Layout used while {exe} is in the foreground: the tray app writes it to the mouse when the game starts and restores the Normal layout when it ends. Buttons set to Default keep their Normal assignment."));
    }

    let name = BUTTONS[ui.sel_button].0;
    let key = ui.button_key();
    let spec = layer_buttons.get(&key).map(String::as_str);
    let cat = if ui.key_mode.as_deref() == Some(key.as_str()) { CAT_KEYBOARD } else { category_of(spec) };
    let (base, rate) = spec.map(turbo_split).unwrap_or((String::new(), None));
    let layer_title = match ui.layer {
        Layer::Normal => String::new(),
        Layer::Hypershift => " (Hypershift)".to_string(),
        Layer::Game(i) => format!(" ({})", cfg.games.get(i).map(|g| g.label()).unwrap_or_default()),
    };
    f.section_icon(&format!("Assign {}{layer_title}", pretty_button(name)), Some(Icon::Edit));
    let cats = CATEGORIES.map(String::from);
    f.chips_wide("Function", &cats, Some(cat), Id::Category(0));
    match cat {
        CAT_MOUSE | 3 | 4 | CAT_SYSTEM => {
            let opts = options_for(cat, cfg.dpi.stages.len());
            let labels: Vec<String> = opts.iter().map(|(l, _)| l.clone()).collect();
            let sel = opts.iter().position(|(_, s)| *s == base);
            f.chips_wide("", &labels, sel, Id::Option(0));
            if cat == CAT_MOUSE && base.starts_with("mouse:") {
                turbo_controls(f, rate);
            }
            if cat == CAT_SYSTEM {
                f.note("Sleep, Power and Wake send the system's power keys, like the keys on some keyboards. Windows decides what happens (sleep, shut down, wake).");
            }
        }
        CAT_KEYBOARD => {
            let cur = base.strip_prefix("key:").unwrap_or("");
            f.edit("Key combination", "Press Enter to save", cur, false, Id::KeyEdit);
            if base.starts_with("key:") {
                turbo_controls(f, rate);
            }
            f.note("Names: a-z, 0-9, f1-f24, enter, esc, tab, space, backspace, delete, home, end, pageup, pagedown, up, down, left, right, and the punctuation keys. Modifiers: ctrl, shift, alt, win, e.g. ctrl+shift+c.");
        }
        CAT_MACRO => {
            let bound = spec.and_then(|s| parse_button(s).ok().flatten()).and_then(|f| f.macro_binding());
            let names: Vec<String> = cfg.macros.iter().map(|m| m.name.clone()).collect();
            let sel = bound.and_then(|(id, _)| cfg.macros.iter().position(|m| m.id == id));
            f.chips_wide("Macro", &names, sel, Id::Option(0));
            let play = bound.map(|(_, p)| p).unwrap_or(Playback::Times(1));
            f.chips_wide("Playback", &PLAYBACKS.map(String::from), Some(playback_index(play)), Id::MacroPlay);
            if let Playback::Times(n) = play {
                if n > 1 {
                    f.slider("Repeat count", "", 2, macros::TIMES_MAX as i32, 1, n as i32, |v| format!("{v} x"), Id::MacroTimes);
                }
            }
            if ui.macro_onboard == Some(true) {
                f.note("The macro plays from the mouse's own memory, so it works without any software. While held repeats it until the button is released; Toggle starts and stops it; Sequence plays one step per press.");
            } else {
                f.note("The Viper V3 Pro has no macro memory, so the tray app plays the macro when the button is pressed (Synapse does the same for this mouse): keep the tray running. The button sends one of the keys F13 to F24, which the tray turns into the macro. While held repeats it until the button is released; Toggle starts and stops it; Sequence plays one step per press.");
            }
        }
        CAT_HYPERSHIFT => f.note("While this button is held, the Hypershift layer is active: all other buttons use their Hypershift assignment."),
        CAT_DISABLED => f.note("The button does nothing."),
        _ => f.note("The button's own function. Assignments are stored in the mouse (onboard profile 1), so they work without any software running."),
    }

    macros_section(ui, f);
}

/// The macro list with the recorder.
fn macros_section(ui: &Ui, f: &mut Form) {
    let cfg = &ui.cfg;
    f.section_icon("Macros", Some(Icon::Keyboard));
    if ui.is_recording() {
        f.note(&format!("Recording for {} s: {} events so far. Press keys and mouse buttons (the delays between them are recorded too), then click Stop or press Esc.", ui.rec_secs, ui.rec_count));
        f.buttons(&[("Stop recording", Id::MacroStop, true)]);
        return;
    }
    if cfg.macros.is_empty() {
        f.note("No macros yet. Record one: every key and mouse button you press, with the pauses between them, is saved and can be bound to a button above.");
    }
    for (i, m) in cfg.macros.iter().enumerate() {
        let events = m.events().unwrap_or_default();
        let bytes = macros::encode(&events).len();
        let mut sub = macros::describe(&events);
        if sub.len() > 70 {
            sub.truncate(67);
            sub.push_str("...");
        }
        if sub.is_empty() {
            sub = "empty".into();
        }
        f.push(Id::SelectMacro(i), f.x + PAD, f.y, f.w - 2 * PAD, 44, Kind::Row { title: format!("{}  ·  id {}  ·  {} bytes", m.name, m.id, bytes), sub, selected: ui.sel_macro == Some(i) });
        f.y += 48;
    }
    f.gap(4);
    let has_sel = ui.sel_macro.is_some_and(|i| i < cfg.macros.len());
    f.buttons(&[
        ("Record new macro", Id::MacroNew, true),
        ("Record again", if has_sel { Id::MacroRecord } else { Id::None }, false),
        ("Delete", if has_sel { Id::MacroDelete } else { Id::None }, false),
    ]);
    if let Some(m) = ui.sel_macro.and_then(|i| cfg.macros.get(i)) {
        f.edit("Name", "Press Enter to save", &m.name, false, Id::MacroName);
        let steps = m.events.join(", ");
        f.note(&format!("Steps: {steps}"));
    }
    match (ui.macro_onboard, ui.macro_storage) {
        (Some(true), Some(st)) => f.note(&format!("Macro memory in the mouse: {} of {} bytes free. Steps can also be edited in the config file ([[macros]]).", st.available, st.total)),
        (Some(false), _) => f.note(&format!("This mouse has no macro memory, so macros are played by the tray app (like Synapse does for it); up to {} different bindings. Steps can also be edited in the config file ([[macros]]).", macros::MAX_SOFT_SLOTS)),
        _ => f.note("Steps can also be edited in the config file ([[macros]])."),
    }
}

/// Turbo toggle and rate slider under a mouse or keyboard assignment.
fn turbo_controls(f: &mut Form, rate: Option<u8>) {
    f.toggle("Turbo", "Repeat the function while the button is held", rate.is_some(), Id::Turbo);
    if let Some(n) = rate {
        f.slider("Repeat rate", "", TURBO_MIN as i32, TURBO_MAX as i32, 1, n as i32, |v| format!("{v} / s"), Id::TurboRate);
    }
}

fn app(ui: &Ui, f: &mut Form) {
    let cfg = &ui.cfg;
    f.section_icon("Tray app", Some(Icon::Program));
    f.toggle("Start at login", "Run the tray app (battery icon, in-game polling) when you sign in", ui.autostart, Id::Autostart);
    f.slider("Battery refresh", "How often the tray icon reads the battery while you use the mouse", 10, 300, 10, cfg.tray.poll_seconds as i32, |v| format!("{v} s"), Id::PollSeconds);
    f.gap(4);
    f.buttons(&[("Open config file", Id::OpenConfig, false), ("Re-import from mouse", Id::Reimport, false), ("Re-apply everything", Id::Reapply, false), ("Refresh", Id::Refresh, false)]);
    f.note("Re-import rebuilds the config from what the mouse has stored. Re-apply writes every setting of the config to the mouse again.");

    f.section_icon("Onboard profile", Some(Icon::Layers));
    let (stored, max) = ui.profiles.unwrap_or((1, 1));
    let slots: Vec<String> = (1..=max.clamp(1, 5)).map(|n| n.to_string()).collect();
    f.chips("Profile in use", "Buttons and Dynamic Sensitivity are written to this onboard profile", &slots, Some((cfg.profile.active as usize).clamp(1, slots.len()) - 1), Id::Profile);
    if max <= 1 {
        f.note("This mouse reports a single onboard profile, so there is nothing to switch to. The selector is here for Razer mice that hold several.");
    } else {
        f.note(&format!("The mouse holds {stored} of {max} profiles. Selecting an empty slot creates it and writes this config to it."));
    }

    if ui.wireless && ui.spec.pairing {
        f.section_icon("Dongle pairing", Some(Icon::Link));
        match &ui.paired {
            Some(list) if !list.is_empty() => {
                for d in list {
                    let state = match d.status {
                        0 => "not connected",
                        1 => "connected",
                        _ => "state unknown",
                    };
                    f.info("Paired device", &format!("product id 0x{:04x}  ·  {state}", d.product_id));
                }
            }
            Some(_) => f.info("Paired device", "none reported"),
            None => f.info("Paired device", "-"),
        }
        if ui.pairing_since.is_some() {
            f.note("Pairing mode is on: switch the mouse off and on (or plug the dongle in next to it). A newly paired mouse shows up above.");
            f.buttons(&[("Cancel pairing", Id::PairCancel, true)]);
        } else {
            f.buttons(&[
                ("Pair a device", Id::PairStart, true),
                (if ui.confirm_unpair { "Yes, unpair" } else { "Unpair this mouse" }, Id::Unpair, false),
            ]);
        }
        if let Some(on) = ui.auto_pair {
            f.toggle("Auto-pairing", "Let the dongle pair with a compatible mouse by itself", on, Id::AutoPair);
        }
        f.note("Same commands as Synapse's device pairing. Unpairing makes the dongle forget the mouse; it stops working until paired again, so use it only to move the mouse to another dongle.");
    }

    f.section_icon("Device", Some(Icon::Info));
    let model = ui.spec.name.split(" (").next().unwrap_or(&ui.spec.name);
    let link = if ui.wireless { "via its wireless dongle" } else { "wired" };
    let trust = if ui.spec.verified {
        ""
    } else if ui.cfg.device.allow_unverified {
        "  ·  unverified, writes allowed by [device] allow_unverified"
    } else {
        "  ·  unverified: read-only until [device] allow_unverified is set"
    };
    f.info("Mouse", &format!("{model}, {link} (product id 0x{:04x}){trust}", ui.spec.product_id));
    f.info("Mouse firmware", &ui.fw.map(|(a, b)| format!("{a}.{b}")).unwrap_or_else(|| "-".into()));
    if ui.wireless {
        f.info("Dongle firmware", &ui.dongle_fw.map(|(a, b)| format!("{a}.{b}")).unwrap_or_else(|| "-".into()));
    }
    f.info("Serial", ui.serial.as_deref().unwrap_or("-"));
    f.info("Config file", &crate::config_path().to_string_lossy());
    f.info("State", if ui.pending { "changes pending (mouse asleep)" } else if ui.asleep { "mouse asleep" } else if ui.battery.is_some() { "connected" } else { "not connected" });
    f.info("Version", &format!("freenapse {}", crate::VERSION));
    f.info("Diagnostics", "after a crash, crash.txt in the config file's folder; freenapse --log records every exchange with the mouse in hid.log there");

    f.section_icon("About", Some(Icon::Help));
    f.note("freenapse replaces Razer Synapse 4 for the Viper V3 Pro with two small native programs and no services. Every setting is kept in the mouse's onboard memory.");
    f.note("Synapse 4 on this PC: 11 processes, about 1.5 GB of RAM. freenapse tray: about 10 MB. Esc closes this window; the tray icon stays.");
}

/// Feature status and release roadmap (same content as ROADMAP.md).
fn roadmap(f: &mut Form) {
    use Status::*;
    f.section_icon("Feature status", Some(Icon::Flag));
    f.note("What freenapse does today compared with Razer Synapse 4. Done is verified on the mouse; Partial works with limits; Planned is on the list; No is out of scope.");

    f.section("Performance");
    f.status(Done, "DPI stages and active stage", "up to 5 stages, CLI, config and this window");
    f.status(Done, "Live DPI 100..35000", "1 DPI steps");
    f.status(Done, "Independent X/Y DPI", "CLI, config (y_stages) and the X/Y toggle above");
    f.status(Done, "Polling rate 125..8000 Hz", "8000 needs the HyperPolling dongle");
    f.status(Done, "In-game polling rate", "while a game with a profile is in the foreground");
    f.status(Done, "Sensor rotation", "angle tune -30..30 degrees");
    f.status(Done, "Dynamic Sensitivity", "Classic / Natural / Jump presets and a custom curve editor, stored onboard");
    f.status(Never, "Sensitivity Matcher", "software-only Synapse helper; set the DPI you want");

    f.section("Calibration");
    f.status(Done, "Smart Tracking 1/2/3 mm", "");
    f.status(Done, "Asymmetric cut-off", "lift-off 2..26 and landing 1..25, written in the same sequence Synapse uses for this mouse");
    f.status(Never, "Surface calibration, mouse-mat presets", "Synapse offers none for the Viper V3 Pro; the firmware refuses the commands");
    f.status(Partial, "Read back current calibration", "the asymmetric levels cannot be read, so \"Keep current\" leaves them as they are");

    f.section("Power");
    f.status(Done, "Battery level and charging", "tray icon, window header, CLI");
    f.status(Done, "Low battery notification", "threshold in config and window");
    f.status(Done, "Idle sleep timer", "1..15 minutes");
    f.status(Done, "Low power mode threshold", "5..100 %; the firmware ignores it at 2000 Hz and above, like Synapse");
    f.status(Done, "Dongle LED mode", "HyperPolling dongle");

    f.section("Customize");
    f.status(Done, "Button remapping", "7 buttons, normal and Hypershift layer, onboard profile 1");
    f.status(Done, "Button functions", "mouse, keyboard with modifiers, media, DPI stage / cycle, Hypershift, disabled");
    f.status(Done, "Double-click, turbo, system keys", "double click, turbo with a repeat rate, sleep / power / wake");
    f.status(Never, "Profile switch button", "the mouse reports a single onboard profile, so there is nothing to switch to");
    f.status(Done, "Macros", "record here or write steps in the config; once / N times / held / toggle / sequence. This mouse has no macro memory, so the tray app plays them, like Synapse does");
    f.status(Todo, "Software bindings", "launch program, type text, Windows shortcuts; needs a resident input hook");

    f.section("App and device");
    f.status(Done, "Settings window, immediate apply", "pending while the mouse sleeps, written when it wakes");
    f.status(Done, "Tray icon", "battery, DPI stage, polling rate menu");
    f.status(Done, "Config file, import, re-apply", "%APPDATA%\\freenapse\\config.toml");
    f.status(Done, "Start at login", "registry Run key");
    f.status(Done, "Firmware and serial numbers", "mouse and dongle");
    f.status(Done, "Onboard profile selection", "config, CLI and the gear page; the Viper V3 Pro reports one profile, so only 1 is accepted");
    f.status(Done, "Per-game profiles", "DPI stage, polling rate and button layout per game, switched by the tray app");
    f.status(Done, "Game library", "Steam, Epic, GOG, EA app, Battle.net, Riot, Ubisoft, Rockstar, Xbox; any other program by file dialog");
    f.status(Done, "Dongle pairing", "pair, cancel, unpair and auto-pairing from the gear page and the CLI");
    f.status(Todo, "Bluetooth", "the Viper V3 Pro has no Bluetooth; kept for other mice");
    f.status(Todo, "Other Razer mice", "only the Viper V3 Pro product ids are recognised");
    f.status(Never, "Firmware update", "use Razer's updater once if you need it");
    f.status(Never, "Account, cloud, Chroma", "telemetry too; by design, and the Viper V3 Pro has no lighting");

    f.section_icon("Next release: done", Some(Icon::Checklist));
    f.bullet(ACCENT, "Macros: record on the Customize tab (keys, mouse buttons, wheel, delays) and bind with the five playback options. The Viper V3 Pro has no macro memory, so the tray app plays them from an F13..F24 key, as Synapse does; verified: slot key read back from the mouse, key press played the macro.");
    f.bullet(ACCENT, "Per-game profiles: DPI stage, polling rate and a button layout per program; verified: polling and buttons switched 2 s after Notepad took the foreground and came back after it closed.");
    f.bullet(ACCENT, "Dongle pairing: pair, cancel, unpair on the gear page and in the CLI; verified: paired list, pairing mode start and cancel (unpair not run on purpose; auto-pairing not supported by this dongle).");

    f.section("Previous release");
    f.bullet(MUTED, "Double-click, turbo and system keys; Dynamic Sensitivity with a curve editor; independent X/Y DPI; onboard profile selection; Calibration tab rebuilt on Synapse's engine for this mouse.");

    f.section_icon("Later", Some(Icon::Checklist));
    f.bullet(BLUE, "Software bindings with a resident input hook: launch a program, type text, Windows shortcuts.");
    f.bullet(BLUE, "A product table so other Razer mice sharing this protocol can be added without code changes.");
    f.bullet(BLUE, "Single-file signed download.");
    f.bullet(MUTED, "Bluetooth connection support for mice that have it.");

    f.section("Not planned");
    f.note("Razer account, cloud profile sync, telemetry, firmware updates and Sensitivity Matcher. The point of freenapse is to do without them.");
}

// ----------------------------------------------------------------------
// Mouse illustration (top view) with clickable button areas.
// ----------------------------------------------------------------------

/// Geometry of the drawing in logical units relative to the art box.
struct MouseGeom {
    cx: i32,
    top: i32,
    w: i32,
    h: i32,
}

fn geom(x: i32, y: i32, w: i32, h: i32) -> MouseGeom {
    let mh = (h - 20).min(300);
    let mw = (mh * 62 / 100).min(w - 140);
    MouseGeom { cx: x + w / 2 + 20, top: y + (h - mh) / 2, w: mw, h: mh }
}

/// Clickable regions (button index → scaled rect), computed with the same geometry as the drawing.
fn mouse_regions(f: &Form, x: i32, y: i32, w: i32, h: i32) -> Vec<(usize, RECT)> {
    let g = geom(x, y, w, h);
    let s = |v: i32| f.s(v);
    let l = g.cx - g.w / 2;
    let split = g.top + g.h * 42 / 100;
    let wheel_w = g.w / 6;
    let wheel_h = g.h / 7;
    let wheel_y = g.top + g.h * 12 / 100;
    let r = |x: i32, y: i32, w: i32, h: i32| RECT { left: s(x), top: s(y), right: s(x + w), bottom: s(y + h) };
    vec![
        (0, r(l, g.top, g.w / 2, split - g.top)),
        (1, r(g.cx, g.top, g.w / 2, split - g.top)),
        (2, r(g.cx - wheel_w / 2, wheel_y, wheel_w, wheel_h)),
        (5, r(g.cx - wheel_w / 2 - 4, wheel_y - 18, wheel_w + 8, 18)),
        (6, r(g.cx - wheel_w / 2 - 4, wheel_y + wheel_h, wheel_w + 8, 18)),
        (3, r(l - 14, g.top + g.h * 48 / 100, 20, g.h * 12 / 100)),
        (4, r(l - 14, g.top + g.h * 34 / 100, 20, g.h * 12 / 100)),
    ]
}

pub unsafe fn draw_mouse(ui: &Ui, dc: HDC, r: &RECT, highlight: usize, regions: &[(usize, RECT)]) {
    let s = |v: i32| ui.s(v);
    let scale = ui.scale;
    // Work in device pixels: regions already are; rebuild the geometry from the rect.
    let (x, y, w, h) = ((r.left as f32 / scale) as i32, 0, ((r.right - r.left) as f32 / scale) as i32, ((r.bottom - r.top) as f32 / scale) as i32);
    let g = geom(x, y, w, h);
    let oy = r.top; // device offset for the top of the art box
    let dx = |v: i32| s(v);
    let dy = |v: i32| s(v) + oy;

    let l = g.cx - g.w / 2;
    let rt = g.cx + g.w / 2;
    let split = g.top + g.h * 42 / 100;
    let body = RECT { left: dx(l), top: dy(g.top), right: dx(rt), bottom: dy(g.top + g.h) };
    let hl = |i: usize| if highlight == i { (ACCENT_HI, ACCENT) } else { (rgb(0x46, 0x47, 0x4b), rgb(0x34, 0x35, 0x38)) };

    // Body silhouette (rounded, lit from the top), then the two main buttons on top of it.
    shadow(dc, &body, s(g.w / 2), 4);
    clip_round(dc, &body, s(g.w / 2), || vgradient(dc, &body, rgb(0x3a, 0x3b, 0x3e), rgb(0x24, 0x25, 0x27)));
    let btn = |i: usize| -> RECT {
        if i == 0 { RECT { left: dx(l), top: dy(g.top), right: dx(g.cx), bottom: dy(split) } } else { RECT { left: dx(g.cx), top: dy(g.top), right: dx(rt), bottom: dy(split) } }
    };
    // Clip the button fills to the body outline.
    let rgn = windows_sys::Win32::Graphics::Gdi::CreateRoundRectRgn(body.left, body.top, body.right + 1, body.bottom + 1, s(g.w), s(g.w));
    windows_sys::Win32::Graphics::Gdi::SelectClipRgn(dc, rgn);
    for i in 0..2 {
        let (top, bot) = hl(i);
        vgradient(dc, &btn(i), top, bot);
    }
    // Light from the top left.
    reflection(dc, &body, 34);
    windows_sys::Win32::Graphics::Gdi::SelectClipRgn(dc, 0);
    windows_sys::Win32::Graphics::Gdi::DeleteObject(rgn);
    round(dc, &body, s(g.w / 2), None, Some((rgb(0x5a, 0x5b, 0x5e), s(2))));
    polyline(dc, &[POINT { x: dx(g.cx), y: dy(g.top + 6) }, POINT { x: dx(g.cx), y: dy(split) }], BG, s(2));
    polyline(dc, &[POINT { x: dx(l + 2), y: dy(split) }, POINT { x: dx(rt - 2), y: dy(split) }], BG, s(2));

    // Scroll wheel and the scroll up/down arrows.
    let wheel_w = g.w / 6;
    let wheel_h = g.h / 7;
    let wheel_y = g.top + g.h * 12 / 100;
    let wheel = RECT { left: dx(g.cx - wheel_w / 2), top: dy(wheel_y), right: dx(g.cx + wheel_w / 2), bottom: dy(wheel_y + wheel_h) };
    let (wt, wb) = hl(2);
    clip_round(dc, &wheel, s(wheel_w / 2), || vgradient(dc, &wheel, wt, wb));
    round(dc, &wheel, s(wheel_w / 2), None, Some((BG, s(2))));
    let arrow = |up: bool, col: u32| {
        let cx = dx(g.cx);
        let (y0, y1) = if up { (dy(wheel_y - 6), dy(wheel_y - 14)) } else { (dy(wheel_y + wheel_h + 6), dy(wheel_y + wheel_h + 14)) };
        polygon(dc, &[POINT { x: cx - s(6), y: y0 }, POINT { x: cx + s(6), y: y0 }, POINT { x: cx, y: y1 }], Some(col), None);
    };
    arrow(true, if highlight == 5 { ACCENT } else { rgb(0x66, 0x66, 0x66) });
    arrow(false, if highlight == 6 { ACCENT } else { rgb(0x66, 0x66, 0x66) });

    // Side buttons (left side of the mouse): forward above back.
    for (i, frac) in [(4usize, 34), (3usize, 48)] {
        let sb = RECT { left: dx(l - 10), top: dy(g.top + g.h * frac / 100), right: dx(l + 6), bottom: dy(g.top + g.h * (frac + 12) / 100) };
        let (st, sbt) = hl(i);
        clip_round(dc, &sb, s(4), || vgradient(dc, &sb, st, sbt));
        round(dc, &sb, s(4), None, Some((BG, s(2))));
    }

    // Callout labels with leader lines.
    let label = |i: usize, text_: &str, anchor: POINT, left_side: bool| {
        let col = if highlight == i { ACCENT } else { MUTED };
        let tw = text_width(dc, ui.fonts().small, text_);
        let lx = if left_side { anchor.x - s(26) - tw } else { anchor.x + s(26) };
        let end = POINT { x: if left_side { anchor.x - s(22) } else { anchor.x + s(22) }, y: anchor.y };
        polyline(dc, &[anchor, end], col, s(1));
        circle(dc, anchor.x, anchor.y, s(3), Some(col), None);
        text(dc, &rect(lx, anchor.y - s(9), tw + s(4), s(18)), text_, ui.fonts().small, col, DT_ROW);
    };
    // Left side: the left button and the two side buttons. Right side: wheel and right button.
    label(0, "Left click", POINT { x: dx(l + 4), y: dy(g.top + g.h * 24 / 100) }, true);
    label(4, "Forward", POINT { x: dx(l - 4), y: dy(g.top + g.h * 40 / 100) }, true);
    label(3, "Back", POINT { x: dx(l - 4), y: dy(g.top + g.h * 54 / 100) }, true);
    label(5, "Scroll up", POINT { x: dx(g.cx + 6), y: dy(wheel_y - 10) }, false);
    label(2, "Middle click", POINT { x: dx(g.cx + wheel_w / 2), y: dy(wheel_y + wheel_h / 2) }, false);
    label(6, "Scroll down", POINT { x: dx(g.cx + 6), y: dy(wheel_y + wheel_h + 10) }, false);
    label(1, "Right click", POINT { x: dx(rt - 6), y: dy(g.top + g.h * 38 / 100) }, false);
    let _ = regions;
}
