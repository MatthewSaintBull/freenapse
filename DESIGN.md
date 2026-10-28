# freenapse design system

How the settings window should look and behave, and the plan to get there.
This is the contract for every UI change: before touching `src/ui/draw.rs`,
`src/ui/mod.rs` or `src/ui/pages.rs`, check the change against the rules
below. Numbers are logical pixels (epx) unless stated; the window renders
them at the monitor's DPI scale.

Status: v1.1, 2026-10-28. Written after a research pass over the current
Windows 11 / Fluent 2 guidance and the Steam, Xbox and GOG clients, and after
measuring the build of that morning (section 2 describes that build, kept as
the record of why). Phases 1 to 6 of section 7 are implemented; 7 to 10
(keyboard model and tooltips, dialogs, DPI sweep, doc screenshots) are open.

---

## 1. What the research says (digest)

Primary sources are Microsoft's Windows app design pages (Fluent 2 applied to
Windows 11), Fluent 2's elevation page, and reviews / case studies of the
game clients. Full list in section 9.

### 1.1 Principles that apply to a hardware settings app

- **Calm, then clear.** Windows 11's own words: Effortless, Calm, Personal,
  Familiar, Coherent. Colour and depth are used to *focus*, not to decorate.
  Accent colour is used sparingly, to highlight the important thing and to
  show the state of an interactive control.
- **Two layers, not ten.** A base layer (navigation, commands, title) and a
  content layer (one continuous surface or a few cards). Everything the user
  edits lives on the content layer. Cards sit at elevation 8 with a 1 px
  stroke; controls at elevation 2 (rest) and 1 (pressed).
- **Strokes outline, shadows only for things that float.** On Windows the
  1 px contour replaces the key shadow. Shadows are reserved for surfaces that
  really float above others (flyouts, dialogs, tooltips, a dragged tile).
  Overused shadows are noise.
- **Materials are for transient surfaces.** Acrylic (frosted glass) is used
  only for flyouts and context menus. Mica is the opaque, subtly tinted window
  base. Glass is not a card style.
- **Dark mode is its own palette.** Lighter surface = more important or more
  elevated. Base surfaces in the #121212 to #1e1e1e range, each tier a few
  percent lighter than the one below. Saturated accents vibrate on dark
  backgrounds, so brand colours are desaturated or used as thin indicators and
  the text on them is dark, not white.
- **Type ramp, sentence case, no bold.** Segoe UI Variable. Caption 12/16,
  Body 14/20, Body strong 14/20 semibold, Body large 18/24, Subtitle 20/28,
  Title 28/36, Title large 40/52. Semibold is the strongest weight. Sentence
  case for *all* UI text including titles. Minimum legible: 12 px regular or
  14 px semibold. Lines of 50 to 60 characters; never more than 60.
- **Geometry.** 8 px radius on windows, dialogs and flyouts; 4 px on in-page
  elements (buttons, list backplates, bars, cards); 0 where edges touch.
- **Spacing is a 4 px scale.** 8 between buttons, 8 between a control and its
  header, 12 between a control and its label, 12 between content areas,
  16 between a surface edge and text, 24 page margins beside an expanded
  navigation pane, 48 indent for nested settings.
- **Navigation pane.** Left pane, expanded at 1008 px and wider; icon plus
  label per item; one selection indicator drawn along the left edge of the
  selected item; optional header, search box, settings item at the bottom.
- **Icons.** Segoe Fluent Icons, monoline 1 px stroke, a glyph at font size
  16 occupies a 16 x 16 box. Use the same metaphor for the same concept
  everywhere. Icons beside text, never instead of it, for commands.
- **Feedback on every action, states on every control.** Rest, hover,
  pressed, disabled and keyboard focus (a 2 px outer ring) are visible and
  distinct. Slow interaction undoes good design: the UI must stay under
  ~100 ms per action.
- **Accessibility is a launch requirement.** 4.5:1 for text, 3:1 for large
  text, icons and control boundaries. Keyboard operation for everything a
  mouse can do. Never colour as the only signal (8 % of men are red-green
  colour-blind, and the brand colour is green).
- **Consistency beats novelty.** Match conventions users already know from
  Windows and the launchers; keep the same behaviour on every page.

### 1.2 What the game clients actually do (2023 Steam, Xbox app, GOG Galaxy)

- Dark theme throughout; one accent colour; flat surfaces with 1 px
  separators; no gloss.
- Left navigation (Steam's redesign case study, Xbox app) with the library as
  a card / cover grid on the right; covers are the only "rich" element, the
  chrome around them is quiet so the art stands out.
- Portrait covers (Steam `library_600x900`), a hero banner behind the
  selected game's detail, and the title revealed on hover or beneath the
  cover.
- Hover lifts the tile slightly and brightens it; the selected tile has a
  strong outline.
- Filters (installed / owned / platform) as chips above the grid, not badges
  on every tile.
- Critiques of all three: visual overload from repeated elements, wasted
  empty space, oversized content that lowers the amount seen at once. The
  guidance is restraint and density, not bigger.

### 1.3 Why the 2026 "glass" trend does not mean sheen on cards

The 2026 glassmorphism guidance (and Apple's Liquid Glass) is about blurred,
translucent *overlays* (navigation bars, sheets, floating panels) over rich
content, applied sparingly, with contrast re-checked on the blurred panel.
It is not diagonal highlights on opaque cards, which read as 2010-era
skeuomorphism. In a GDI app with no real blur, "glass" should be expressed as:
a hairline top highlight, a very subtle vertical gradient (2 to 3 % luminance)
and a stroke. Reflections stay on *objects* that are physically glossy: the
mouse render and the battery cell, nothing else.

---

## 2. The current build, measured against that

Screenshots: `freenapse-ui-overview.png`, `f_games.png` (2560x1440 at 150 %).
Contrast computed with the WCAG formula.

### 2.1 Contrast

| Pair | Ratio | Verdict |
| --- | --- | --- |
| Body text #f0f0f0 on card #1f1f21 | 14.4 | pass |
| Muted #9a9b9e on card | 5.9 | pass |
| Muted on window #111112 | 6.8 | pass |
| **Dim #5e5f62 on card** (hints, "Battery --", disabled) | **2.6** | **fail** for text |
| **White text on accent #44d62c** (selected chip, primary button) | **1.9** | **fail** badly |
| Near-black #1b1b1c on accent | 8.9 | pass |
| Accent #44d62c as text on card | 8.5 | pass |
| Error #e5484d on card | 4.2 | fail for small text |
| Card stroke #38393b on card | 1.4 | fine for a hairline, invisible as a boundary |
| Track #47484a on card | 1.8 | below 3:1 for a control boundary |

Two real defects: every selected chip and primary button has white on green,
and every hint uses the dim grey.

### 2.2 Visual critique (what is dated or noisy)

1. **Diagonal sheen on every card and tile.** The reflection band on cards,
   chips and the "GO" tile is the single most dated element. It fights the
   cover art and makes flat text look printed on plastic. Remove it from
   surfaces and controls; keep a restrained version on the mouse render and
   the battery cell only.
2. **Glossy green gradient controls.** Primary buttons, selected chips, the
   toggle knob track and the "Normal" layer chip use a light-to-dark green
   gradient with white text. Fluent 2 and all three launchers use a flat
   accent fill with dark text (or a thin accent indicator). Flat, dark text,
   4 px radius.
3. **ALL-CAPS 11 px bold section titles with a full-width rule.** Below the
   legibility minimum, against sentence-case guidance, and the rule under
   every header makes each card heavy. Use Body strong (14 semibold),
   sentence case, no rule; or Subtitle (20 semibold) for the page's first
   group.
4. **A card per setting group with 22 px padding and generous rows.** The
   Performance page shows three cards in 1 100 px of height with one toggle
   in the middle one. Content stretches to ~1 500 px wide, so a label on the
   left and its control on the far right are 1 200 px apart. Windows
   Settings solves this with a *settings row* (icon, title, description,
   control at the right edge of a 1 000 px-wide column). Adopt that row and
   cap content width.
5. **The same information three times.** Link and battery appear as title
   bar pills, in the sidebar device card, and on the Power page. The title
   bar pills also look like buttons but are not. Keep one status area (the
   device card), make the Power page the detail, drop the pills.
6. **Permanent footer "Changes are saved…"** uses 36 px on every page for a
   sentence the user reads once. Replace with a transient status line (shown
   for ~2 s after a write, with an icon) in the same place, otherwise
   empty, or fold it into the device card.
7. **Wordmark with a green glow halo** reads as a gaming skin. A flat
   wordmark (icon tile + "freenapse" in Body large semibold) with the
   subtitle in Caption at 60 % is calmer and still branded.
8. **Launcher badge on every tile.** With 27 of 35 tiles saying "STEAM", the
   badge is noise and covers art. Move the launcher to a filter chip row
   (All, Steam, Xbox, Epic, EA, Programs) and to the tile's hover state /
   details card; keep a tiny monochrome mark only for non-Steam sources if
   needed.
9. **The mouse render on Customize** is a grey silhouette with a flat green
   left button and thin leader lines. It should be the hero of that page:
   top-down view with proper shading, a highlighted button that follows the
   selection, and labels in Body strong aligned to a grid, not scattered.
10. **Selected navigation item** uses a filled green-tinted box plus a bar
    plus green text; three signals for one state. Fluent uses the 3 x 16 px
    indicator on the left plus a subtle fill (white at 6 %), text stays
    primary.
11. **No keyboard model.** There is no Tab order, no focus ring, no tooltips,
    no Escape/Enter handling outside text boxes. A professional desktop app
    is fully operable from the keyboard.
12. **Hover states are near-invisible** on rows and chips (white at ~3 %).
    Hover should be clearly perceivable (white at 6 to 8 % on dark surfaces),
    pressed slightly darker than rest, and the cursor a hand only on
    buttons and links, not on every row.
13. **Accent is the only colour used for state.** Success / warning / error
    and the "stage active" dot all need a shape or icon too.

### 2.3 What already works and stays

- Left navigation with icon + label, Settings at the bottom, device card.
- Page header (icon, title, one-line subtitle).
- Cover tiles with real art and a bottom gradient for the title; the
  generated gradient tile with initials for programs.
- Dark neutral ramp (#111112 base, #1f1f21 cards) is in the recommended range.
- Pure GDI rendering with the double-buffered off-screen bitmap; no new deps.
- Window 1240 x 800, min 980 x 620, clamped to the work area.

---

## 3. Foundations (tokens)

Put every value here in `draw::theme` / `draw::Fonts` and use *only* these.
No ad-hoc colours or sizes in `pages.rs`.

### 3.1 Colour

Dark theme only (the app is dark by design; a light theme is out of scope).
Neutral surfaces are near-neutral greys with a 1 to 2 unit blue tint to sit
well with the green.

| Token | Value | Use |
| --- | --- | --- |
| `bg` | #111112 | window base (navigation + title region) |
| `content` | #161617 | content area behind cards |
| `card` | #1c1c1e | card / list backplate |
| `card_hover` | #222224 | card or row on hover |
| `field` | #262628 | text box, slider track backplate, chip rest |
| `field_hover` | #2c2c2e | chip / field hover |
| `stroke` | #2a2a2c | card and field outline (1 px) |
| `stroke_strong` | #3c3c3f | control boundary that must be seen (toggle off, slider track) |
| `divider` | #232325 | hairlines between rows |
| `text` | #f3f3f3 | primary text |
| `text_2` | #a9aaad | secondary text, descriptions (7.0:1 on card) |
| `text_3` | #7c7d81 | tertiary: placeholders, timestamps, disabled (3.9:1; never for essential text) |
| `accent` | #1db954 | the freenapse green: selection indicator, primary fill, on-state (deliberately not Razer's #44d62c) |
| `accent_hover` | #25cf61 | primary button hover |
| `accent_pressed` | #189b47 | primary button pressed |
| `accent_text` | #1db954 | accent text / icons on dark surfaces (6.6:1 on card) |
| `on_accent` | #06200f | text and icons on an accent fill (6.6:1) |
| `accent_soft` | #1db954 at 12 % | selected row / nav fill, on-state track |
| `success` | #4fd66a | with a check icon |
| `warning` | #f0b429 | with a warning icon |
| `error` | #ff6b6e | with an error icon (5.3:1 on card) |
| `info` | #4da3ff | links, info |
| `overlay` | #000000 at 40 % | smoke under dialogs |

Rules
- Text on `accent` is always `on_accent`. Never white on green.
- `text_3` only for text whose loss does not matter (placeholder, caption of
  a disabled control).
- The accent appears at most: once as a fill per view (the primary button
  or the selected chip), once as the navigation indicator, and as on-states.
  Section icons, titles and chrome are neutral.
- Cover art is the only saturated area allowed in quantity.

### 3.2 Typography

GDI font faces: "Segoe UI Variable Display" for Subtitle and above,
"Segoe UI Variable Text" for Body and Body strong, "Segoe UI Variable Small"
for Caption. Fall back to "Segoe UI" on Windows 10 (check `GetTextFaceW`).

| Style | Size / line | Weight | Use |
| --- | --- | --- | --- |
| Caption | 12 / 16 | Regular | hints, tile subtitle, status |
| Body | 14 / 20 | Regular | labels, descriptions, list text |
| Body strong | 14 / 20 | Semibold | row titles, section titles, nav labels, button labels |
| Body large | 18 / 24 | Semibold | wordmark, tile title on the detail card |
| Subtitle | 20 / 28 | Semibold | page sub-sections (optional) |
| Title | 28 / 36 | Semibold | page title |
| Display | 40 / 52 | Semibold | the battery percentage only |

Rules
- Sentence case everywhere ("Button assignment", not "BUTTON ASSIGNMENT").
- No bold (700), no italics, no uppercase tracking.
- Descriptions wrap at 60 characters; measure with `GetTextExtentPoint32W`
  and cap the description width (~440 px at 14 px).
- Ellipsis on single-line overflow; wrap on multi-line.
- Numbers that change (DPI, Hz, %) in Body strong, tabular alignment to the
  right of their control.

### 3.3 Spacing and sizing (4 px scale)

| Token | Value | Use |
| --- | --- | --- |
| `s1` | 4 | icon to text in a chip, badge padding |
| `s2` | 8 | between buttons, chips; control to its header |
| `s3` | 12 | control to label; between cards; row inner vertical padding |
| `s4` | 16 | card inner padding; surface edge to text |
| `s5` | 24 | page margin beside the nav pane; between sections |
| `s6` | 32 | page header top |
| `s7` | 48 | indent of nested settings |

Control heights: button 32, chip 28, text box 32, toggle 20 x 40, slider
track 4 with a 20 px knob (hit area 32), nav item 36, settings row 64
(68 with a description), list row 40, title bar 32 (system caption), page
header block 88, footer status 28.

Content column: max width 1 064, left-aligned at the 24 px margin. Cards
never stretch past it. On a wide window the right side stays empty (as in
Windows Settings and the Xbox app's settings), except the Games grid and
the mouse render, which may use the full width.

### 3.4 Geometry, elevation and materials

- Radius 8: window (DWM), flyouts, dialogs, the mouse render's card.
- Radius 4: cards, buttons, chips, text boxes, tiles, toggles' track
  (pill = height/2), sliders.
- Radius 0 where two elements touch (segmented control halves).
- Every surface has a 1 px `stroke`. Cards get a second 1 px highlight
  inside the top edge at white 4 % (that is the whole "glass" effect).
- Vertical gradient on cards: from `card` + 2 % to `card`; on buttons: from
  fill + 3 % to fill. Nothing stronger.
- Shadows: only under tiles on hover (lift 2 px, shadow 8), dialogs (shadow
  64) and flyouts (shadow 16). No shadow under cards at rest, no shadow
  under buttons.
- Reflection band: mouse render and battery cell only, at 10 % or less.
- Smoke (`overlay`) under modal dialogs.

### 3.5 Iconography

Segoe Fluent Icons (fallback Segoe MDL2 Assets), colour `text_2` at rest,
`text` on hover / selected, `accent_text` only for an on-state or the page
icon. Sizes: 16 in rows, chips, buttons and nav; 20 for the page title;
24 nowhere except empty states. One glyph per concept (the mouse glyph is
Customize and the device; the speedometer is Performance; the battery is
Power; the controller is Games; the flag is Roadmap; the gear is Settings).

### 3.6 Motion

GDI has no compositor animations; keep the UI instant. Allowed: hover and
pressed colour swaps on the next paint, a 2 px tile lift on hover. No fades,
no sliding panes. The invalidation must be per-widget rectangle, not the
whole window, so hover never repaints covers.

---

## 4. Components

Each component lists its anatomy, sizes, states and the token it uses. All
states are mandatory: rest, hover, pressed, disabled, focused, and selected
where it applies.

### 4.1 Navigation item
36 high, full pane width minus 8 px side margins, radius 4. Icon 16 at x 12,
label Body strong at x 40. Hover: white 6 % fill. Pressed: white 4 %.
Selected: white 6 % fill plus a 3 x 16 `accent` indicator centred on the left
edge; label `text`, icon `text`. Focus: 2 px `text` ring outside, 1 px `bg`
inside.

### 4.2 Navigation pane
Width 240 (icon-only 48 below 1008 px window width, optional). Background
`bg`, 1 px `stroke` on its right edge. Top: wordmark block 64 high (16 px
icon tile with the logo mark (open ring, assets/logo.svg) on the accent, "freenapse" Body large semibold,
"Mouse control" Caption `text_3`). Items from y 80. Bottom: Settings
item, then the device card.

### 4.3 Device card
`card` surface, radius 4, 16 padding, 88 high. Row 1: device name Body
strong + link icon state (wireless / wired / asleep, `text_2`). Row 2:
Caption `text_2` ("fw 1.11 · dongle 1.11" or "Asleep, move it to wake").
Row 3: battery bar 4 high, `stroke_strong` track, `accent` fill (warning
colour below the notification level), percentage Caption right-aligned.
This is the only persistent place for link and battery.

### 4.4 Page header
88 high. Title (Title 28) at the content margin, page icon 20 `text_2` to
its left, subtitle Body `text_2` under it with 4 px gap. Right side: page
level commands only (for example "Scan again" on Games). No status pills.

### 4.5 Card
`card`, 1 px `stroke`, top highlight, radius 4, 16 padding. Optional title
row: Body strong `text` + Caption `text_2` description under it, 12 px below
the title the first settings row. No icon in the card title, no rule.

### 4.6 Settings row (the main building block)
64 high (68 with description), full card width, divider between rows.
Left: 16 px icon `text_2` (optional), title Body strong `text`, description
Caption `text_2` wrapped at 60 characters. Right: the control, right-aligned
with 16 px to the card edge. Nested rows indent 48 and sit on `content`.
Hover on the row only when the whole row is a button (expander, link row).

### 4.7 Buttons
Height 32, padding 0 12 (0 16 with icon), radius 4, Body strong label.
- Primary: `accent` fill, `on_accent` text. Hover `accent_hover`, pressed
  `accent_pressed`, disabled `field` with `text_3`.
- Standard: `field` fill, 1 px `stroke_strong`, `text`. Hover `field_hover`,
  pressed `field` with `text_2`.
- Subtle (toolbar / in-row): transparent, hover white 6 %, pressed 4 %.
- Destructive: standard button with `error` text; confirm destructive
  actions in a dialog.
One primary per view at most.

### 4.8 Toggle
20 x 40 pill. Off: 1 px `stroke_strong` border, transparent, 12 px knob
`text_2`. On: `accent` fill, 12 px knob `on_accent`. Hover: knob grows to
14. Disabled: `field`, knob `text_3`. The label is the row title; the toggle
has no inner text.

### 4.9 Chips / segmented control
Height 28, padding 0 12, radius 4 (or 14 for a pill group). Rest: `field`
with 1 px `stroke`, `text`. Hover `field_hover`. Selected: `accent` fill,
`on_accent` text, no gradient. In a segmented group the items touch with
0 radius on shared edges and one `stroke` around the group.

### 4.10 Slider
Track 4 high `stroke_strong`, fill `accent` from the left, knob 20 with a
2 px `card` ring, radius 10. Hover: knob 22. Value label Body strong to the
right, 56 px wide, right-aligned; tick labels Caption `text_3`. Keyboard:
Left / Right step, Page Up / Down 10 steps, Home / End.

### 4.11 Text box / number box
Height 32, `field`, 1 px `stroke`, radius 4, 2 px `accent` bottom edge when
focused (Fluent's pattern). Placeholder `text_3`. Search box carries a
16 px search icon at the left.

### 4.12 Tile (Games)
150 x 212 cover area, radius 4, 1 px `stroke`. Title Body strong on a
bottom gradient (black 0 to 70 %), one line, ellipsis; Caption `text_2`
below for "1600 DPI · 2000 Hz" on profile tiles, nothing on installed
tiles. No badge at rest. Hover: lift 2 px, shadow 8, 1 px `text_2` border,
show the source Caption in the bottom-right. Selected: 2 px `accent` border
and a 20 px check in a circle top-right. Gap 16. Grid columns: as many as
fit the full content width; tiles do not stretch.

### 4.13 Filter chip row (Games)
Above the grid: "All · Steam · Xbox · Epic · EA · Programs" chips plus the
search box at the right. Counts in Caption inside the chip ("Steam 27").

### 4.14 Status line (replaces the footer)
28 high, Caption `text_2`, icon 16. Empty at rest. After a write to the
mouse: check icon + "Saved to the mouse" for 2 s. On error: error icon +
the message until the next action. Placed under the content column.

### 4.15 Tooltip
`card` at elevation 16, 1 px `stroke`, radius 4, 6 8 padding, Caption.
Shown after 500 ms hover on icons, truncated text and sliders (value).

### 4.16 Dialog
Centred, radius 8, `card`, shadow 64, smoke beneath. Title Subtitle,
body Body, buttons right-aligned (primary last). Escape cancels, Enter
confirms. Used for "Remove profile" and "Reset to defaults".

---

## 5. Layout and pages

Shell: system title bar (dark, caption colour = `bg`, rounded corners via
DWM) · nav pane 240 · content with 24 px margins · status line.
Window 1240 x 800 default, min 980 x 620, remembers size and position.

### 5.1 Customize
Hero: the mouse render in a card at the top (full width, 300 high), top-down
view with shading, the selected button highlighted in `accent` and all other
buttons `field_hover`; labels on a left and right column aligned to the
buttons, Body strong, with the current assignment in Caption. Below: a
"Layer" chip row and the assignment card as settings rows (one per button,
chevron at the right, the current function as description). Clicking a row
opens the "Assign" card under it, nested 48, with the function chip row and
the detail control.

### 5.2 Performance
One card "Sensitivity": rows Enable stages (toggle), Independent X/Y
(toggle), Number of stages (segmented 1 to 5), then one row per stage with
the slider and number box; the active stage has the `accent` radio and
"Active" Caption. Card "Dynamic sensitivity": one row (toggle) and, when on,
nested rows. Card "Polling rate": slider row with tick labels; "Rotation":
toggle row + angle slider nested. Everything inside the 1 064 column.

### 5.3 Calibration, Power, Settings
Settings rows in one or two cards each. Power keeps the big battery as a
hero row (Display 40 percentage, battery cell with the only allowed
reflection, "On battery" Caption), then rows for notification level, idle
sleep, low power, dongle LED.

### 5.4 Games
Header with the "Scan again" standard button at the right. Card "Game
profiles": description row, tile row with the "Add a program" tile as the
last tile (dashed `stroke_strong` border, plus icon, "Add a program"),
the selected profile's settings in a card under the tiles ("Profile:
ARC Raiders" Body strong, rows for DPI, polling, Hypershift, with "Remove
profile" as a destructive standard button at the bottom right). Card
"Installed games": filter chip row + search, then the grid. Empty state:
a 24 px icon, Body "No games found", Caption with what was scanned.

### 5.5 Roadmap
List rows with a status icon (check / in progress / planned) and text; no
cards per item.

---

## 6. Interaction and accessibility

- **Keyboard.** Tab / Shift+Tab move through nav, then page controls in
  reading order; arrows move inside a group (nav list, chips, slider,
  stages). Space / Enter activate. Escape closes the assign card or dialog.
  Ctrl+F focuses search on Games. F5 rescans. Focus ring: 2 px `text`
  outside the control's radius + 1 px `bg` inside, drawn only after a
  keyboard event (`WM_UPDATEUISTATE` style behaviour).
- **Pointer.** Hand cursor only on buttons, tiles, links and nav items.
  Rows that are not actionable keep the arrow.
- **Feedback.** Every write to the mouse shows the status line; a failed
  write shows the error inline and keeps the previous value in the control.
  Controls never stay in a "pending" look for more than one paint.
- **Contrast.** All text 4.5:1 or better on its real surface; control
  boundaries and icons 3:1 (`stroke_strong`, not `stroke`, around toggles
  off and slider tracks). Verify with the script in section 8.
- **Colour independence.** On-state = fill + knob position; active stage =
  filled radio + "Active" word; errors = icon + text.
- **Text scaling.** Everything derives from the DPI; test at 100, 125, 150
  and 200 %.
- **Reduced motion.** Nothing animates, so nothing to disable.
- **Screen reader.** Out of scope for GDI custom controls in this version;
  note it in ROADMAP.md as a known gap.

---

## 7. Implementation plan

Phase order is chosen so each step is visible in a screenshot and cannot
regress the previous one.

1. **Tokens and type ramp** (`draw.rs`): replace `theme` with section 3.1,
   fonts with 3.2 (Segoe UI Variable faces with fallback), spacing consts
   with 3.3. Delete `reflection()` calls from cards, chips, buttons, tiles;
   keep the function for the mouse and battery. Flat accent fills with
   `on_accent` text. Build, screenshot all pages, compare.
2. **Shell**: nav pane 240 with the 4.1 / 4.2 / 4.3 specs; page header
   without pills; footer becomes the status line; content column capped at
   1 064; DWM rounded corners. Screenshot.
3. **Settings row component** (`pages.rs`): a `row(title, description,
   control)` builder that lays out 4.6, and `card(title, rows)`. Port
   Performance, Power, Calibration, Settings onto it. Screenshot each.
4. **Controls**: toggle, chips, slider, text box, buttons to section 4
   with all states; hover invalidation per widget. Screenshot rest and a
   simulated hover (post `WM_MOUSEMOVE` through `cap.ps1`).
5. **Games**: filter chips + search row, tiles without badges, hover lift,
   "Add a program" tile, profile detail card. Screenshot.
6. **Customize hero**: new mouse render (shaded top-down, aligned labels,
   selection follows), assignment rows. Screenshot.
7. **Keyboard and tooltips**: focus order, focus ring, arrow keys in groups,
   tooltip window. Test with keyboard only.
8. **Dialogs and status**: Remove profile / Reset dialogs with smoke; status
   line after writes; error state.
9. **DPI sweep**: 100 / 125 / 150 / 200 % screenshots; fix clipping.
10. **Docs**: README / FEATURES screenshots and this file's status line.

Definition of done for every phase: build with zero warnings, 33+ tests
pass, screenshots of every touched page captured with
`cap.ps1`, contrast script passes, this document updated if a value changed.

---

## 8. Verification tools

- `cap.ps1` (scratchpad): PrintWindow capture of the live window, with
  `-ClickX/-ClickY` (client coords) and `-Wheel`. At 150 % the nav items
  are at x 150, y 178 / 244 / 310 / 376 / 442 / 508, Settings 899; these
  move when the pane is rebuilt, re-measure from the first screenshot.
- Contrast check: a 20-line Python snippet with the WCAG relative luminance
  formula (`0.2126 R + 0.7152 G + 0.0722 B` on linearised sRGB,
  `(L1 + 0.05) / (L2 + 0.05)`) over every text / surface pair in `theme`.
  Keep it in `tools/contrast.py` once the tokens land, and run it in CI.
- Font check: `GetTextFaceW` after `SelectObject` to confirm "Segoe UI
  Variable Text" was found, else log the fallback.

---

## 9. Sources

Windows / Fluent (primary)
- Layering and elevation: https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/layering
- Typography and type ramp: https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/typography
- Geometry (corner radii): https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/geometry
- Colour: https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/color
- Materials (Acrylic, Mica, Smoke): https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/materials
- Content layout and spacing: https://learn.microsoft.com/en-us/windows/apps/design/basics/content-basics
- Iconography / Segoe Fluent Icons: https://learn.microsoft.com/en-us/windows/apps/design/iconography/
- NavigationView (pane modes, breakpoints, margins): https://learn.microsoft.com/en-us/windows/apps/design/controls/navigationview
- Design principles and "make apps great on Windows 11": https://learn.microsoft.com/windows/apps/get-started/best-practices and https://learn.microsoft.com/windows/apps/get-started/make-apps-great-for-windows
- Fluent 2 elevation (shadow ramp 2 / 4 / 8 / 16 / 28 / 64): https://fluent2.microsoft.design/elevation

Game clients
- Steam desktop client redesign case study (consistent sidebar): https://www.maximilianlock.co.uk/post/steam-redesign
- Steam UX analysis (visual overload, repetition): https://medium.com/design-bootcamp/gaming-on-steam-what-works-so-well-a-ux-analysis-965ca58f89f6
- Steam 2023 client overhaul coverage: https://www.trustedreviews.com/news/steam-gets-a-major-ui-overhaul-4338016 and https://win.gg/news/steam-redesign-launches-brings-changes-to-steam-library-and-more/
- Launchers compared (Steam, Xbox app layout, GOG filters): https://www.pcgamer.com/pc-gamings-many-launchers-reviewed-for-2024-steam-still-puts-the-rest-to-shame/ and https://plarium.com/en/blog/best-game-launchers/
- Xbox PC app rebuild (faster, simpler navigation, 2026): https://propakistani.pk/2026/10/10/xbox-pc-app-to-get-major-overhaul-with-faster-loading-and-simpler-navigation/

General 2026 guidance (secondary)
- Dark mode practices (surface range, lighter = elevated, desaturated accents): https://www.onething.design/post/best-practices-for-dark-mode-ui-design and https://madegooddesigns.com/dark-mode-design/
- Glassmorphism and accessibility: https://axesslab.com/?p=5111 and https://uxpilot.ai/blogs/glassmorphism-ui
- 8 pt grid and spacing tokens: https://canvas.workday.com/tokens/space and https://breakdance.com/the-8-point-grid-system-a-practical-guide/
- UX best practices 2026 (feedback, one primary action, accessibility baseline): https://uxpilot.ai/blogs/ux-best-practices and https://fuselabcreative.com/ui-ux-design-trends-2026-modern-ui-trends-ux-trends-guide/
- Peripheral software critiques (G HUB cleaner / minimal vs Synapse flashy): https://evezone.evetech.co.za/deep-dives/peripheral-software-comparison-razer-g-hub-icue
