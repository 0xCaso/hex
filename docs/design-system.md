# Hex Design System

The GPUI desktop app (Settings window, meeting capsules, transcription picker,
and the Linux Settings window) is built from two modules:

- [`src/design_tokens.rs`](../src/design_tokens.rs) holds every raw value:
  colours, the type scale, spacing, radii, and control geometry.
- [`src/desktop_ui.rs`](../src/desktop_ui.rs) holds every shared component.

Panes compose components and name tokens. They never write a raw colour, type
size, line height, font family, or radius. The test
`design_tokens::tests::desktop_sources_use_tokens_instead_of_raw_styles` fails on
`rgb(0x…)`, `rgba(0x…)`, `.text_size(`, `.line_height(px(`, `.font_family(`,
`.rounded(px(`, `.rounded_sm/md/lg()`, `.text_xs/sm/base/lg/xl()`,
`.border_t/b/l/r_1()` (separators go through `Hairlines`), `.cursor_pointer()`
(macOS controls keep the arrow), and `.truncate()` in
`app_window` (with its macOS, Linux, and Windows modules), `desktop_ui`,
`desktop_transcription_picker`, `linux_app`, `meeting_watcher`, and
`text_input`. `windows_ui` owns the Windows UI face and Segoe glyph sizes.

The direction is **quiet native**: the window should read as a Mac app first.
Structure comes from the sidebar material and hairlines, not boxes; controls use
macOS metrics; colour is spent only on the user's accent, state dots, and
danger. Native-feel research and its sources are summarised in
[Materials and native behaviour](#materials-and-native-behaviour).

The dictation HUD ([`dictation_indicator.rs`](../src/dictation_indicator.rs)) is
outside the system: its glossy capsule is hand-tuned in HUD Lab.

## Rules

1. **Tokens only.** If a value is missing, add a named token to
   `design_tokens.rs` with a one-line purpose. Don't inline it.
2. **One component per job.** Use the table below. If no component fits,
   extend one in `desktop_ui.rs` and delete whatever it replaces. Never keep two
   helpers for the same job.
3. **Five type steps.** Pick a step by role, not by look. Don't set weights a
   step already owns.
4. **Sentence case, one name.** Labels, buttons, section labels, and titles use
   sentence case. The product is "Hex" everywhere in the UI.
5. **One icon set.** Use `Icon` / `NavigationIcon`: line-weight SF Symbols at
   medium weight on macOS, never `.fill` variants. Never draw an icon with text
   glyphs or letters.
6. **One accent per screen.** The accent fill (`ButtonKind::Primary`) marks at
   most one action per screen: the thing the person came to do (see
   [Colour](#colour)). Everything else is neutral.
7. **Few visible actions.** A detail header shows its one to three most common
   actions; the rest go in an `Icon::Ellipsis` overflow menu, with the
   destructive choice last after a separator.
8. **Verify by looking.** Capture every pane you touched with
   `scripts/capture-preview.sh` (see [Verification](#verification)).

## Tokens

### Surfaces

The palette is dark-only for now; windows and capsules force the DarkAqua
appearance so AppKit materials match it. Values follow the macOS dark system
colours, precomposited over the window base so panes keep painting opaque
`rgb()` tokens.

```ts
CANVAS            #1e1e1e  // window base (windowBackgroundColor); panes sit directly on it
SIDEBAR           #252525  // nav column where no material exists (Linux, Windows), picker language list
SURFACE           #2a2a2a  // menus and popovers: the one floating surface in the window
FILL_QUIET        #2e2e2e  // recessed fills: segmented tracks, search fields, keycap wells, group wells
FILL              #353535  // control fill: buttons, pop-up buttons, keycaps
FILL_STRONG       #454545  // pressed or open control, off switch track, slider track
SURFACE_SELECTED  #464646  // selected list row, segmented thumb
LINE              #343434  // separators, always one device pixel (HAIRLINE 0.5)
LINE_STRONG       #4a4a4a  // floating-surface outlines
SCRIM             #00000099 // modal backdrops
```

Over a material (RGBA): `SIDEBAR_SELECTED` / `SIDEBAR_SELECTED_INACTIVE` for
the sidebar selection in key and non-key windows, `GLASS_FILL`, `GLASS_RIM`,
`GLASS_OVER_MATERIAL` (graphite laid over HUD glass so capsules stay legible
over bright windows), and `GLASS_TINT` where no material exists.

### Text

```ts
TEXT_PRIMARY    #dedede  // titles, values, selected items
TEXT_SECONDARY  #b3b3b3  // descriptions, unselected list titles
TEXT_TERTIARY   #919191  // metadata, section labels, placeholders
TEXT_DISABLED   #5c5c5c  // unavailable controls only
ACCENT_TEXT     #419cff  // links (linkColor)
DANGER          #ff6961  // errors, destructive labels
SUCCESS         #30d158  // ready and live states
```

`TEXT_TERTIARY` is the weakest colour allowed for readable text. It measures at
least 4.5:1 on every surface (`readable_text_meets_aa_contrast_on_every_surface`);
the system's own tertiaryLabel (white at 25%) is decorative and too faint.

### Colour

Saturated colour is spent on meaning, never decoration:

- **Primary** is a white fill (`PRIMARY_FILL`) with an ink label
  (`PRIMARY_INK`): at most one per screen, for the main action. No hue marks an
  action anywhere else.
- **Accent** is the user's own `controlAccentColor`, read once through
  `accent()`: the switch's on state, progress, and the menu item under the
  pointer. Nothing else.
- **States are not actions:** an ongoing state uses a Secondary button with a
  `status_dot`. Recording (`#ff453a`, systemRed) is only ever a dot.
- **Danger** marks destructive choices and armed confirmations only; **success**
  marks ready and live states only. Tags are coloured caption text with no
  fill or border.

### Type scale

The window frame sets `BODY`, so text with no step inherits 13 pt. CoreText
already applies SF Pro's optical tracking per size; never add manual tracking.

| Step | Size / line | Weight | Use |
|---|---|---|---|
| `CAPTION` | 11 / 14 | inherited | metadata, timestamps, tags, section labels |
| `FOOTNOTE` | 12 / 16 | inherited | descriptions, small control labels, menu items |
| `BODY` | 13 / 18 | inherited | window default, values, inputs, row titles, buttons |
| `READING` | 14 / 20 | inherited | long-form reading: meeting transcripts and summaries |
| `HEADLINE` | 15 / 20 | semibold | toolbar (pane) titles, dialog titles |
| `TITLE` | 20 / 24 | semibold | detail titles, onboarding page titles |

Apply steps with the `Typography` trait (`.text_caption()` … `.text_title()`,
`.text_reading()`, `.text_step(STEP)`). `.text_tabular()` turns on tabular
figures for anything that counts or sits in a column (durations, timestamps,
counts, sizes); `.text_monospace()` is for code, logs, and paths.

### Spacing, radii, geometry

Use GPUI spacing utilities on the 4 pt grid. `PANE_GUTTER` (28) is the one
horizontal inset for pane headers and scrolling bodies. List+detail panes use
`split_body()` edge to edge: the list column is divided from the detail by one
hairline, with no outer inset.

Smaller text beside a larger line shares its baseline: inset it from the top by
`baseline_offset(LARGER, SMALLER)`, never `items_center`.

```ts
RADIUS_KNOB 2 · RADIUS_SMALL 4 · RADIUS_CONTROL 6 · RADIUS_PANEL 8 · RADIUS_MODAL 12
HAIRLINE 0.5                 // every separator: one device pixel on Retina

CONTROL_HEIGHT 24            // macOS regular controls: buttons, inputs, pop-ups, segmented
CONTROL_HEIGHT_SMALL 20      // small buttons, keycaps
CONTROL_HEIGHT_LARGE 28      // toolbar main action and search field
TOGGLE 36×20                 // capsule switch, accent when on
TAG_HEIGHT 18 · MENU_ITEM_HEIGHT 22 · NAV_ITEM_HEIGHT 28
MENU_WIDTH 200 · MENU_OFFSET 4
PANE_HEADER_HEIGHT 52        // toolbar row, level with the traffic lights (= TITLEBAR_INSET)
PANEL_HEADER_HEIGHT 36 · SETTINGS_ROW_MIN_HEIGHT 48
SIDEBAR_WIDTH 216 · PANE_CONTENT_WIDTH 760
PANE_LIST_WIDTH 272 · PANE_LIST_WIDTH_COMPACT 220 (Modes)
TRANSCRIPT_SPEAKER_WIDTH 52 · TRANSCRIPT_TIME_WIDTH 44
CAPSULE_HEIGHT 60 · CAPSULE_BUTTON_HEIGHT 36 · RECORDING_PILL_HEIGHT 44  // concentric
```

### Motion

Springs live in [`src/motion.rs`](../src/motion.rs) and are named through
`design_tokens`:

```ts
SPRING_INSTANT      0.10s · bounce 0.00  // pointer-tied meters, live waveform scrub
SPRING_SNAP         0.25s · bounce 0.00  // switches, segmented thumbs, menu open, sliders
SPRING_SNAP_EXIT    0.20s · bounce 0.00  // menu close
SPRING_SETTLE       0.30s · bounce 0.00  // default state change
SPRING_REFLOW       0.40s · bounce 0.00  // disclosures, row height/width reflow
SPRING_GLIDE        0.32s · bounce 0.00  // modal sheets, onboarding pages
SPRING_MORPH        0.30s · bounce 0.16  // chip-to-panel morphs
SPRING_POP          0.25s · bounce 0.15  // press/release confirmation
SPRING_REDUCED_FADE 0.16s · bounce 0.00  // opacity-only fallback under Reduce Motion
MENU_ENTER_OFFSET   4 pt                  // menus drop from their trigger
```

What moves, and what doesn't, follows AppKit:

- **Instant:** sidebar selection and pane switches, list selection, hover
  (there is none outside menus). Mac apps never cross-fade a selection.
- **Sprung in GPUI:** switches, segmented thumbs, menus and popovers (fade plus
  a 4 pt drop from the anchor), disclosures, the transcription sheet.
- **Core Animation, off the main thread:** capsule entrance and exit
  (`window_material::animate_panel_in/out`): a 0.2 s fade plus a
  `CASpringAnimation` slide (SwiftUI `.spring(duration: 0.32, bounce: 0)`), and
  a 0.18 s fade out. The render server drives these, so they stay smooth while
  the app is busy; the window never moves.

Every GPUI spring retargets with velocity carry-over, requests frames only
while unsettled, and honours `accessibilityDisplayShouldReduceMotion` (or
`HEX_REDUCE_MOTION=1`), which reduces movement to short fades.

## Materials and native behaviour

[`src/window_material.rs`](../src/window_material.rs) puts real AppKit
materials behind GPUI's transparent Metal layer:

- **Sidebar:** an `NSVisualEffectView` (`.sidebar`, follows the window's active
  state) under the navigation column, which paints nothing itself. AppKit dims
  it in a non-key window and makes it opaque under Reduce Transparency;
  `navigation_item(…, window_active)` dims the selection to match.
- **Toolbar:** an empty unified `NSToolbar` places the traffic lights in the
  52 pt toolbar row, with their native hit areas and full-screen behaviour.
- **Capsules:** HUD glass behind each meeting capsule, masked to the pill and
  resized with it. The panel's window-server shadow is off (it would outline
  the whole transparent panel); the pill draws `capsule_shadow()` inside its
  8 pt margin.

GPUI's `WindowBackgroundAppearance::Blurred` is not used: it strips the
material's tint and saturation.

Native behaviour the components keep: no hover highlight or pointer cursor on
buttons and rows (menus are the one place that highlights under the pointer),
keyboard focus rings (`focus_ring()`) on text fields, accent-coloured switches,
and inactive-window dimming.

## Components

| Job | Component |
|---|---|
| Window root (canvas + `BODY`) / content column | `window_frame`, `content_frame` |
| Navigation column / item | `sidebar_frame`, `navigation_item(NavigationIcon, selected, window_active)` |
| Toolbar row: pane title, optional controls | `pane_header`, `pane_header_with_action` |
| Scrolling pane body | `pane_scroll_body(id, content)` |
| List + detail | `split_body()` holding `pane_list(id, width, empty, error_title, error, rows)` and `pane_detail(content)` |
| Row in a list | `list_row(selected)` with `list_row_title`, `list_row_meta` |
| Group heading in a list | `list_section_label(label, first)` |
| Top of a detail | `detail_header(detail_title(…), meta, actions)` over `detail_body(id)`; actions sit on the title line |
| Any text that must fit one line | `truncated_line(STEP, text)` (ends in "…") |
| Group of rows | `panel()` (no box: rows divided by hairlines); titled: `panel_header(title, action)` |
| Separators | `Hairlines`: `.hairline_t()`, `.hairline_b()`, `.hairline_r()` |
| Settings | `settings_section_label` → `panel` → `settings_row(title, description, control)` |
| Label above a group | `section_label` (caption, semibold, tertiary) |
| Label / value in a detail | `detail_row` |
| Main action of a view (at most one) | `button(ButtonKind::Primary, …)` |
| Any other action | `button(ButtonKind::Secondary, …)` |
| Destructive or armed confirmation | `button(ButtonKind::Danger, …)` |
| Action inside a row | `small_button(kind, …)` |
| Icon-only action | `icon_button(ButtonKind::Ghost, Icon)`; `Secondary` when it stands alone |
| Work in progress where a button was | `busy_label` |
| Pop-up button | `disclosure_button(label)` (up/down chevron); custom content: `disclosure_frame()` |
| Popover / menu and its choices | `menu()`, `menu_item(selected)`; under a control: `dropdown_menu()` / `dropdown_menu_with_progress` |
| Destructive / unavailable menu choice | `danger_menu_item()` after `menu_separator()`; `disabled_menu_item()` |
| Overflow of less common actions | `icon_button(ButtonKind::Secondary, Icon::Ellipsis)` opening a `dropdown_menu()` |
| On/off | `toggle(position)`; never repeat "On"/"Off" beside it |
| A few exclusive choices | `segmented_control` + `segmented_item`, or the sliding variants |
| Status (never clickable) | `tag(Tone, label)`: coloured caption text |
| Count | `count_badge(n)` |
| Live / recorded state | `status_dot(colour)` |
| Shortcut | `hotkey_keycaps`; onboarding art: `hero_keycaps(parts, pressed)` |
| Progress | `progress_track(indicator)` with its label beside it |
| Position in a multi-step flow | `step_dots(index, count)` |
| Text entry | `TextInput` ([`text_input.rs`](../src/text_input.rs)): `FILL_QUIET`, hairline, focus ring |
| Nothing to show | `empty_state`; in a detail: `detail_placeholder` |
| Errors | `error_panel`, `error_message`, `inline_error` |

A destructive menu choice ends in "…" ("Delete…") and keeps its two-step
confirmation: choosing it closes the menu and replaces the action row with the
armed Danger button and Cancel until one is chosen.

Buttons are always filled. Only icon buttons and menu items are borderless, and
nothing changes under the pointer except menu items. Tags never have a border or hover state, so they can't be mistaken
for buttons. A row carries at most two tags, beside its title; an ongoing
state on its trailing edge ("Active") is a `status_dot` and a footnote label,
never a third tag; progress carries its label beside the bar. Put errors at the top of their scroll area,
never below the fold.

Meeting capsules are concentric pills on HUD glass: a 60 pt capsule, 12 pt
padding, 36 pt pill buttons, and a 32 pt lens well for the leading glyph. They
use the shared buttons with `.rounded_full()` and the same type and dots as the
window.

## Copy

- Sentence case for every label, button, title, and section label:
  "Open settings", "Stop & transcribe", "Permissions needed". Proper nouns keep
  their capitals: Hex, OpenCode, Apple Intelligence, Finder, Zoom.
- One product name: "Hex". Never "HEX" in UI strings, window titles, or menus.
  File and artifact names (`HEX.app`, `HEX-<version>…`) and `HEX_*` environment
  variables are identifiers, not copy.
- Use "…" (one character) for ellipses, for both progress ("Saving…") and for
  buttons that open a further choice ("Choose from Finder…").
- Settings and permission row descriptions are one phrase with no closing
  period ("Hold to dictate, release to transcribe"). Two sentences keep their
  periods.
- Keep one direct line per control. Don't add intro paragraphs above cards,
  repeat field labels per row, put state words next to toggles, or warn "cannot
  be undone" beside a two-step confirmation.
- Exception: native macOS menus (`status_item.rs` and the app menu in
  `meeting_watcher.rs`) follow the macOS title-case convention ("Check for
  Updates…", "Paste Last Dictation", "Stop & Transcribe").

## Icons

- Use `Icon` (`ArrowRight`, `ChevronDown`, `Close`, `DragHandle`, `Ellipsis`,
  `Globe`, `Plus`) via `icon(Icon, colour)` at `ICON_SIZE_SMALL`. `Ellipsis`
  draws at `ICON_SIZE` so its dots match the other glyphs optically. Use
  `NavigationIcon` for the sidebar.
- On macOS these are line-weight SF Symbols (no `.fill` variants) at medium
  weight, monochrome. On Linux they fall back to Heroicons (navigation) and
  matching glyphs (inline). Windows uses the Heroicons for navigation and the
  matching Segoe Fluent Icons glyphs inline.
- Colour icons with a text token: secondary at rest, primary when selected.
- macOS symbols are rasterised by AppKit into premultiplied bitmaps;
  `desktop_ui::sf_symbol` un-premultiplies them before GPUI's straight-alpha
  sprite pipeline, or thin strokes fade out over materials.
- A web page or other item without an app icon shows `Icon::Globe` inside the
  same rounded tile, never a letter monogram.
- To add an icon, add an `Icon` variant with both its SF Symbol and its Linux
  fallback.

## Verification

```sh
cargo test --bin voice-control design_tokens desktop_ui   # tokens, contrast, layout, truncation
cargo clippy --all-targets --all-features -- -D warnings
./scripts/capture-preview.sh /tmp/hex-settings.png settings
HEX_PREVIEW_SKIP_BUILD=1 ./scripts/capture-preview.sh /tmp/hex-modes.png modes
```

Capture every pane and state you touched. The preview targets and flags are
listed in [AGENTS.md](../AGENTS.md#development). Check at least these:
alignment with the header rule, shared baselines, no clipped text without "…",
at most one Primary button per screen, and sentence case.

For motion, record real frames instead of judging by eye:

```sh
# 60 fps frames of the main window while a scripted choreography runs
HEX_PREVIEW_CHOREOGRAPHY=controls HEX_PROFILE_FRAMES=1 \
  HEX_RECORD_FRAMES_DIR=/tmp/frames HEX_RECORD_DURATION_MS=7600 \
  target/release/voice-control preview settings
# a capsule entrance and exit, composited over the desktop
HEX_PREVIEW_PANEL_DELAY_MS=700 HEX_PREVIEW_PANEL_DISMISS_MS=1300 \
  HEX_RECORD_FRAMES_DIR=/tmp/capsule HEX_RECORD_SCREEN_RECT=1200,20,564,130 \
  HEX_RECORD_DURATION_MS=2600 target/release/voice-control preview meeting-offer --meeting-panel offer
```

`HEX_PROFILE_FRAMES` logs render and frame-interval p50/p95/max per pane.

## Audit: before → after

This is the consolidation from `origin/main` 1c3dacf, measured across the six
desktop sources.

```ts
Text sizes      11 explicit (7 8 9 10 11 12 13 14 15 18 20) + GPUI's inherited 16
                  -> 5 steps (11 12 13 15 20) + the 7 px keycap badge
                  uses before: 11×44 12×41 10×33 13×8 14×4 15×3 18×3 8×2 20×2 9×1 7×1
                  mapping: 7–11 -> CAPTION, 12 -> FOOTNOTE, 13–14 -> BODY,
                           15–18 -> HEADLINE, 20 -> TITLE (pane titles only)
                  inputs 14, unsized text 16 -> BODY 13 from window_frame
Line heights    8 distinct explicit (16 17 18 19 20 24 26) -> owned by each step
Weights         SEMIBOLD×40 MEDIUM×6 NORMAL×3
                  -> MEDIUM for emphasis (15), SEMIBOLD owned by HEADLINE/TITLE
                     and card titles (7), NORMAL for unselected items (2)
Text colours    TEXT TEXT_SOFT MUTED FAINT + 3 raw greys (f1f1f1 8f8f8f 171717)
                  -> TEXT_PRIMARY TEXT_SECONDARY TEXT_TERTIARY (+ TEXT_DISABLED)
                  FAINT #6e6e6e was below 4.5:1 on SURFACE; merged into TEXT_TERTIARY
Raw colours     35 literals, 29 distinct, beside 18 tokens -> 0 literals, 23 tokens
Radii           2 4 5 6 8 10 + rounded_sm/md/lg (60 call sites)
                  -> RADIUS_KNOB 2, SMALL 4, CONTROL 6, PANEL 10, MODAL 12
Buttons         7 looks (header_button, canvas_button, compact_button, blue fill,
                white fill, danger tint, capsule grey)
                  -> 4 kinds (primary, secondary, ghost, danger) × 2 sizes + 1 tag
Control height  24 26 28 30 32 34 36 -> 30 (CONTROL_HEIGHT) / 24 (small)
Section labels  16 ALL CAPS -> sentence case
Product name    "HEX" in window title, menus, copy -> "Hex"
Icons           mixed filled and line SF Symbols, "×" "+" "≡" "→" glyphs, letter
                monograms -> one outline Icon set, globe for web pages
Truncation      .truncate() clipped mid-glyph or kept a stale width -> truncated_line
Pane geometry   header inset 32, list+detail bodies inset 20 (cards overhung the
                header rule by 12 px) -> one PANE_GUTTER owned by pane_body
```
