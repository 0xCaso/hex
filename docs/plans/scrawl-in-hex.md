# Scrawl in Hex (experiment plan)

Status: M1a in progress on `experiment/scrawl` (`crates/scrawl-core` imported
with git history). Cut from `origin/main` at `8efd990`.

Scrawl ("show an agent the problem") records the screen, your voice, clicks,
and red ink, then packs them into a `.scrawl` bundle an agent can read. Hex
users have asked for it "connected with Hex". This plan makes Hex the one
app: Scrawl moves into the Hex repo as an opt-in macOS feature named **Scrawl
(beta)**, and the standalone Scrawl app is frozen once Hex reaches M4 parity
(section 5).

Scrawl is also the first feature built against Hex's Ghostty-style
**core / platform / runtime (`libhex`) / shell** architecture (section 3), so
both today's GPUI shell and the native AppKit shell on `feat/native-macos`
consume the same engine without duplicating capture or bundle logic.

Sources read: Hex `AGENTS.md`, `docs/features/*`, `docs/design-system.md`,
`src/app_settings.rs`, `src/suppression.rs`, `src/onboarding.rs`,
`src/meeting.rs`, `src/local_api.rs`, `origin/feat/native-macos`
(`src/ffi.rs`, `docs/plans/native-macos-app.md`); Ghostty `include/ghostty.h`,
`src/apprt.zig`, `src/apprt/embedded.zig`, `src/apprt/runtime.zig`,
`src/apprt/surface.zig`; Scrawl `README.md`, `AGENTS.md`, `Cargo.toml`,
`src/{main,model,keyframes,sketch,render,autocrop,hold,windows,zoom,export,capture,ui,laser_overlay}.rs`.
`/Users/kit/code/open-source/scrawl` was not modified.

## Decisions locked in with Kit

```ts
Locked decisions
├── One app            -> Fold Scrawl into Hex; freeze standalone Scrawl.app at M4 parity
├── Feature name       -> "Scrawl" (Settings pane: "Scrawl (beta)" until out of beta)
├── Shortcut           -> Separate ⌥⇧S chord matched in Hex's event tap (rebindable modifier+key)
├── Architecture       -> Ghostty-style split: GPUI/AppKit-free core crates + platform traits + libhex + thin shells
├── Escape key         -> Passes through to the foreground app during screen recording; capsule gets Discard + Undo
├── Storage            -> ~/Library/Application Support/voice-control/scrawl/ (owner-only, beside meetings/)
├── iPhone capture     -> Kept on the roadmap as M5 (cabled capture + ScrawlKit ingest)
└── Git history        -> Import Scrawl's core files into crates/scrawl-core with git history preserved
```

**Merge-to-`main` blocker (licensing):** Scrawl's `Cargo.toml` declares
`license = "MIT"`, but `/Users/kit/code/open-source/scrawl` has no `LICENSE`
file, while Hex is MIT under `anomalyco/hex`. Before `experiment/scrawl` merges
to `main`, Kit must land an explicit MIT `LICENSE` file in Scrawl (or record the
grant in `crates/scrawl-core/LICENSE`). Tracked here and in
`crates/scrawl-core/README.md`.

## Facts that shape the plan

```ts
Same stack, same pins
├── gpui 0.2.2 (crates.io, font-kit)          // Hex and Scrawl, identical
├── screencapturekit =11.0.0, macos_15_0      // identical
├── cpal 0.18.2, metal 0.29, objc2 0.6.4, objc2-app-kit 0.3.2, block2, core-video
└── Errors differ: Scrawl anyhow, Hex color-eyre   // bridged at crate boundary

Scrawl's modules by layer
├── Pure Rust (crates/scrawl-core): model, keyframes, sketch, render, autocrop,
│   export/{mod,crop,camera,ink}, zoom (viewport math), hold (state machine),
│   windows (pure types + filters), pipeline (bundle builder + artifacts)
├── macOS platform (no GPUI): capture (SCK recording, input telemetry), windows (CGWindowList),
│   laser_overlay (objc2 + Metal), retention, routing, delivery
├── GPUI shell: ui.rs (5.7k lines: capsule island, picker, review window -> replaced by Hex shell)
└── scrawl-helper (Swift executable): frame decode moves to a compiled Swift bridge in Hex

Hex today + feat/native-macos
├── One event tap (suppression.rs): modifying key tap + passive tap for flags and mouse-down
├── Paste Last ⌥⇧V and Paste Meeting ⌥⌃V are reserved chords inside DictationHotkey::process
├── Screen Recording helpers already exist in meeting.rs (permitted / request / open settings)
├── transcribe.cpp already returns word times (t0_ms, t1_ms) for Parakeet
├── Swift bridges compile directly into hex (apple_speech, apple_intelligence), no helper process
└── feat/native-macos already introduces src/ffi.rs (hex_engine_t, wakeup_cb, event_cb, tick, telemetry)
```

## 1. Settings

Scrawl gets its own pane, shaped like Voice Action: always visible in the
sidebar on macOS, with the opt-in toggle at the top and everything else hidden
or disabled until enabled. Release builds start with it off. Linux and Windows
omit the pane until their platform capture traits are implemented.

```ts
Hex > Scrawl (beta)                                // scrawl.opt-in; off by default
├── Off -> No shortcut reserved, no screen permission asked, no workers
└── On
    ├── Screen & System Audio Recording missing -> Request -> "Open settings" row (shared with Meetings)
    ├── Shortcut -> ⌥⇧S by default; Hex's shortcut capture; key chords only
    ├── Keep recordings -> Video 7 days, bundles 30 (default) / Video 30, bundles 90 / Forever
    ├── Crop to activity -> On                    // Scrawl auto-crop, Screen captures only
    ├── Send dev-site captures to OpenCode -> Off  // needs opencode2; asks for browser Automation once
    └── Show captures in Finder
```

Persisted as one block in `settings.json`, following `MeetingSettings`:

```ts
scrawl: {
  enabled: false,
  hotkey: { modifiers: ⌥⇧, key: S },
  retention: { media_days: 7, bundle_days: 30 } | null,   // null = forever
  auto_crop: true,
  send_to_opencode: false,
}
```

Turning it on is the only action that requests Screen Recording. Starting a
capture without the grant refuses cleanly and surfaces the settings row,
matching Meetings and Scrawl's rule that capture code never prompts.

What survives from Scrawl, and how it changes:

```ts
Scrawl option                    -> In Hex
├── Transcriber (Apple / Hex)    -> Gone. Always Hex's selected model, with real word times
├── Hex model picker             -> Gone. Uses the dictation model already loaded
├── Keep Recordings              -> Kept, same three choices and Trash-not-rm rules
├── Auto-crop on/off             -> Kept as one toggle; tuning numbers stay in scrawl-core
├── Send to OpenCode (autoSend)  -> Kept, default OFF (Scrawl defaults on)
├── Shortcut                     -> Kept, through Hex's shortcut field and conflict checks
└── Video export flag            -> Hidden until hex scrawl export ships
```

## 2. Hotkeys

### How `⌥⇧S` meets the Option dictation hotkey

Tracing `DictationHotkey::process` with the default Option binding:

```ts
Press ⌥, then ⇧ within 300 ms, then S        // normal fast chord
  -> ⌥ alone matches -> Start (pre-roll capture begins)
  -> ⇧ joins before MINIMUM_HOLD_DURATION -> extra_modifiers -> Discard -> Dirty
  -> Nothing transcribed or pasted; mute/pause/no-sleep never applied

Press ⌥, wait > 300 ms, then ⇧ S              // slow chord
  -> Recording is live; S key-down matches reserved Scrawl chord
  -> Dictation -> Dirty (discards in-flight Option capture) and starts Scrawl

Press ⇧ first, then ⌥, then S                 // Shift already down
  -> ⌥ never matches alone -> Scrawl chord fires cleanly
```

Paste Last (`⌥⇧V`) and Paste Meeting (`⌥⌃V`) already work this way inside
`DictationHotkey::process`: on key-down the dictation state transitions to
`Dirty`, the reserved action fires, and the modifying tap swallows the `S`
keystroke.

### Gesture state machine (`scrawl_core::hold`)

```ts
Scrawl enabled, shortcut ⌥⇧S                     // scrawl.shortcut
├── Tap (released < 250 ms) -> Record display under pointer, auto-cropped
│   └── Tap again (> 400 ms after start) -> Finish -> Build bundle (~1 s) -> Clipboard
├── Hold ⌥⇧ after S -> Switcher on Screen; S steps Screen → Window; drag = Area   // M3
│   └── Release ⌥⇧ -> Record selection; Esc -> close switcher, record nothing
└── Key-down always: Dictation -> Dirty (discard any < 300 ms or slow-chord capture)

While recording
├── Hold ⇧ + move (>= 6 px span) -> Red ink; clicks -> ripples
├── Dictation shortcut -> Works normally (its audio also lands in narration)
├── Voice Action shortcut -> Works normally
├── Escape -> Passed through to the foreground app (cancels dictation only if dictating)
└── Discard -> Capsule button or menu bar "Discard recording", with Undo window
```

## 3. Architecture: Ghostty-style library, runtime, and shells

Hex is transitioning from a single monolithic binary crate toward the structure
Ghostty uses across its macOS AppKit app and Linux GTK app, already started on
`origin/feat/native-macos` (`src/ffi.rs` and `docs/plans/native-macos-app.md`).

### What Hex borrows from Ghostty

```ts
Ghostty source                     -> Role in Hex
├── include/ghostty.h              -> Small C ABI (src/ffi.rs): opaque engine handle (hex_engine_t),
│   (ghostty_app_t,                   runtime config with userdata + wakeup_cb + event_cb,
│    ghostty_runtime_config_s,        main-thread hex_engine_tick, and lock-free high-frequency
│    ghostty_surface_t)               telemetry reads (hex_engine_read_hud_telemetry)
├── src/apprt.zig                  -> Runtime boundary (libhex): separates pure domain/core crates
│   (apprt.App, apprt.Surface,        and OS platform adapters from the UI toolkit hosting the app;
│    apprt.action)                    typed actions and notifications cross a single seam
├── src/apprt/embedded.zig         -> Embedded host contract: external shells (Swift/AppKit Xcode app,
│   (apprt.embedded.App / Surface)    GPUI desktop shell, CLI/MCP) drive libhex without libhex owning
│                                     NSApplication or a UI framework event loop
└── src/apprt/surface.zig          -> Interactive surface abstraction: transient overlays/windows
    (CoreSurface + per-surface        (dictation HUD, meeting capsule, Scrawl picker/ink overlay/capsule)
     input & size callbacks)          receive normalized pointer/key/scale inputs and emit actions
                                      targeted at either the app or a specific surface
```

### Four-layer workspace architecture

```ts
hex workspace
├── 1. Core crates (pure Rust; no GPUI, no AppKit/objc2/Win32/GTK; OS via traits only)
│   ├── crates/scrawl-core                 // M1a (this pass): bundle model, keyframes, sketch,
│   │   ├── model, keyframes, sketch       //   autocrop + corpus, CPU RGBA/PNG renderer,
│   │   ├── autocrop, render, zoom         //   export plan (crop/camera/ink), hold state machine,
│   │   ├── export/{mod,crop,camera,ink}   //   window filters, composable BundleBuilder pipeline,
│   │   ├── hold, windows, pipeline        //   and platform traits (FrameSource, InputSource,
│   │   └── traits                         //   Clock, WordTranscriber)
│   ├── crates/hex-dictation-core          // subsequent extraction: pause_segmenter, text_replacements,
│   │                                      //   command_grammar, shortcut state machine
│   └── crates/hex-meeting-core            // subsequent extraction: meeting_detection, meeting_echo,
│                                          //   seam deduplication, markdown/notes formatting
│
├── 2. Platform layer (OS capture, audio, input taps, Metal/compositor surfaces; no UI toolkit)
│   ├── macOS: ScreenCaptureKit (video + mic), CoreAudio/cpal, CGEventTap, CGWindowList,
│   │          AVFoundation frame decode (Swift bridge), Metal ink/ripple/laser surface,
│   │          transcribe.cpp Metal word-timed transcriber
│   ├── Linux: evdev/X11 input, PipeWire/ALSA audio, Vulkan/CPU GGUF transcriber
│   └── Windows: WASAPI, low-level keyboard hook, UI Automation context, GGUF transcriber
│
├── 3. libhex runtime (app core / Ghostty `apprt.embedded` equivalent)
│   ├── Owns settings, permissions, model lifecycle, dictation coordinator, meetings, and scrawl
│   ├── Drives active sessions (DictationSession, MeetingSession, ScrawlSession) through platform traits
│   ├── Rust API for in-process Rust shells (GPUI, CLI, `hex mcp`, tests)
│   └── C ABI (`src/ffi.rs` / `include/hex.h`) for out-of-process or Xcode shells:
│       ├── hex_engine_new / free / tick
│       ├── wakeup_cb + event_cb (app & surface actions: HUD state, Scrawl capsule/overlay state)
│       ├── hex_engine_dispatch_json (settings, dictation, meetings, scrawl start/finish/discard/undo)
│       └── Lock-free telemetry snapshots (HUD audio levels, Scrawl live ink/elapsed/mode)
│
└── 4. Thin shells (UI consumers only; never own capture, audio timelines, or bundle math)
    ├── GPUI shell (`src/app_window.rs`, `src/dictation_indicator.rs`)  // current production binary
    ├── Native AppKit shell (`feat/native-macos`: `macos/HexApp`)       // Swift/AppKit over libhex.a
    ├── Linux shell (`hex.service` + GTK layer-shell HUD + GPUI Settings)
    └── Windows shell (`windows_app` + tray + HUD + GPUI Settings)
```

### `scrawl-core` composable API and platform traits

`scrawl-core` has zero global state and zero platform framework dependencies
(even its badge/contact-sheet text renderer uses a deterministic portable glyph
rasterizer so `Cargo.toml` pulls in only `serde`, `serde_json`, `png`, and
`thiserror`). Callers compose a capture or reprocessing run explicitly:

```ts
Platform traits (`scrawl_core::traits`)
├── Clock           -> now_ms(&self) -> u64
├── FrameSource     -> capture_frame(&mut self, t_ms: u64) -> Result<Option<RgbaFrame>, ScrawlError>
│                      decode_frames(&mut self, video: &Path, times_ms: &[u64]) -> Result<Vec<(u64, RgbaFrame)>, ScrawlError>
├── InputSource     -> poll_events(&mut self) -> ScrawlInputBatch (clicks, drags, scrolls, keys, strokes, cursor, windows)
└── WordTranscriber -> transcribe_words(&mut self, wav_path: &Path) -> Result<TranscriptData, ScrawlError>

Composable bundle pipeline (`scrawl_core::pipeline`)
└── BundleBuilder::new(id, target)
    ├── .created_at_ms(ms) / .duration_ms(ms)
    ├── .transcript(TranscriptData { text, words, segments, estimated })
    ├── .strokes(strokes) / .events(events) / .cursor(cursor) / .windows(windows)
    ├── .raw_frames(candidates)            // or decoded video frames at selected keyframe times
    ├── .auto_crop(enabled)                // runs autocrop::compute_from_bundle + crops frames
    ├── .build() -> BuiltBundle            // pure in-memory: ScrawlBundle + annotated frames + contact sheet + summary.md
    └── .write_to(&bundle_dir)             // writes scrawl.json, summary.md, contact-sheet.png, frames/*.png
```

Why this split pays off immediately:

1. **Fast inner loop:** `cargo test -p scrawl-core` compiles and runs 80+ unit
   and autocrop-corpus tests in seconds without linking `transcribe.cpp`,
   Moonshine, ScreenCaptureKit, or GPUI.
2. **Mechanical boundary enforcement:** `crates/scrawl-core/Cargo.toml` cannot
   accidentally reach into GPUI, `objc2`, `app_settings`, or global paths.
3. **Zero rework for `feat/native-macos`:** when the AppKit shell replaces GPUI
   on macOS, `scrawl-core` and the macOS platform implementations of
   `FrameSource` / `InputSource` / `WordTranscriber` stay untouched; only the
   capsule and Settings view bind to `libhex` actions and telemetry.

### Incremental migration path from today's single `voice-control` crate

We do not rewrite Hex in one shot. Each step keeps `cargo build`,
`scripts/build-app.sh`, and CI green:

```ts
Step 1 (M1a, now)  -> Convert root Cargo.toml into a workspace (`members = [".", "crates/scrawl-core"]`)
                      Import Scrawl's pure modules with git history into `crates/scrawl-core`
                      Extract composable `BundleBuilder` + platform traits (`FrameSource`, `InputSource`, `Clock`, `WordTranscriber`)
Step 2 (M1b)       -> Implement macOS `FrameSource` (SCK + AVFoundation bridge), `InputSource` (passive tap),
                      and `WordTranscriber` (Hex Parakeet/Whisper word timings) inside Hex; wire `⌥⇧S`
Step 3 (M2–M4)     -> Add Scrawl surface telemetry/actions to `libhex` so GPUI and `feat/native-macos` share capsule + ink overlay
Step 4 (follow-on) -> Extract `crates/hex-dictation-core` and `crates/hex-meeting-core` along the same seam
```

### Milestones

```ts
M1a Core crate with history (this pass)
    Import Scrawl's GPUI-free core into `crates/scrawl-core` with git history preserved
    Shape composable `BundleBuilder` pipeline + platform traits (`FrameSource`, `InputSource`, `Clock`, `WordTranscriber`)
    All standalone unit + autocrop-corpus tests pass (`cargo test -p scrawl-core`); `hex` binary build unchanged

M1b Smallest end-to-end in-Hex capture (macOS, opt-in toggle, off by default)
    Tap ⌥⇧S -> record display under pointer (SCK video + mic) -> menu bar shows recording
    Tap ⌥⇧S -> Hex word-timed transcription + `scrawl-core` `BundleBuilder` ->
               click + scene-change keyframes, auto-crop, contact sheet, summary.md -> clipboard text + paths
    No ink, no switcher, no routing, fixed 7/30-day retention

M2  Ink, click ripples, laser (Metal overlay); capsule with Discard + Undo
    `hex scrawl inspect | frames | zoom`
M3  Hold switcher (Screen / Window / Area); remaining settings rows; retention worker
M4  Send dev-site captures to OpenCode (`opencode2` discovery Hex already has)
    -> Parity for Mac screen capture: freeze standalone Scrawl.app (section 5)
M5  iPhone: ScrawlKit ingest and cabled capture, behind their own opt-in
Later  `hex scrawl export` (video), `hex mcp` read-only access to captures
```

## 4. Permissions, onboarding, risks

```ts
Hex onboarding (unchanged)                       // gate for dictation only
├── Microphone, Input Monitoring, Accessibility, dictation model
└── Scrawl never blocks or delays this gate

Turn on Scrawl
├── Screen & System Audio Recording -> request once, then "Open settings"
│   └── Granted but this process can't see it -> "Relaunch Hex"
├── Input Monitoring -> already granted; passive tap watches mouse up/drag/scroll during capture
├── Microphone -> already granted
└── Automation (browser) -> only when "Send dev-site captures" is turned on
```

Risks:

- **Licensing (merge blocker).** Scrawl's `Cargo.toml` declares
  `license = "MIT"`, but the Scrawl repo has no `LICENSE` file. Do not merge
  `experiment/scrawl` into `main` until Kit adds the `LICENSE` file or confirms
  the license header for `crates/scrawl-core`.
- **Dictation regressions.** The only change to `suppression.rs` (in M1b) is
  one more reserved chord on the Paste Last path, inactive while Scrawl is off.
  Tests will cover fast and slow `⌥⇧S` orders with an Option binding, rebinding
  conflicts, and Escape still cancelling dictation during a screen recording.
- **Privacy and retention.** Bundles live in
  `~/Library/Application Support/voice-control/scrawl/` with `0700` owner-only
  permissions, 7/30-day default retention, Trash-only deletion, and visible
  recording state in the menu bar and capsule.
- **Running both apps during the transition.** Turning Scrawl on in Hex checks
  whether `dev.kitlangton.scrawl` is running and warns that `⌥⇧S` would trigger
  both apps.

## 5. Retiring standalone Scrawl.app

The standalone Scrawl app continues unchanged until Hex reaches M4 parity, then
receives one final Sparkle notice ("Scrawl is now part of Hex") without
auto-deleting or migrating settings. Existing bundles in
`~/Library/Application Support/scrawl/captures` stay untouched and readable via
`hex scrawl inspect|frames|zoom`.

```ts
Scrawl piece                                               -> Fate
├── Pure bundle, keyframe, autocrop, sketch, render, hold  -> crates/scrawl-core (M1a)
├── Mac screen capture, ink, keyframes, auto-crop, bundle  -> Hex, M1b–M3
├── Dev-site routing to OpenCode (lsof -> git root)        -> Hex, M4
├── scrawl inspect / frames / zoom / send / route          -> hex scrawl …, M2–M4
├── scrawl reprocess, autocrop lab, fixture, eval          -> hex dev scrawl … (debug builds only)
├── Video export (scrawl export, flagged)                  -> hex scrawl export, later
├── Retention (Keep Recordings, pin, clean)                -> Hex, M3; `hex scrawl pin`
├── Cabled iPhone capture + ScrawlKit iOS SDK ingest       -> Hex, M5
├── Transcriber choice (Apple / Hex)                       -> Retired; always Hex
└── Onboarding, Sparkle, menu bar, review window           -> Retired; Hex's own shells
```
