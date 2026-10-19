# Scrawl in Hex (experiment plan)

Status: plan only, nothing built. Branch `experiment/scrawl`, cut from
`origin/main` at `8efd990`.

Scrawl ("show an agent the problem") records the screen, your voice, clicks,
and red ink, then packs them into a `.scrawl` bundle an agent can read. Hex
users have asked for it "connected with Hex". This plan covers how it would
fit as an opt-in Hex feature on macOS, working name **Show an agent (beta)**.

Sources read: Hex `AGENTS.md`, `docs/features/*`, `docs/design-system.md`,
`src/app_settings.rs`, `src/suppression.rs`, `src/onboarding.rs`,
`src/meeting.rs`, `src/local_api.rs`; Scrawl `README.md`, `AGENTS.md`,
`Cargo.toml`, `src/{hex,transcribe,hold,capture,ui,laser_overlay}.rs` and the
module imports of the rest. Scrawl was not modified.

## Facts that shape the plan

```ts
Same stack, same pins
├── gpui 0.2.2 (crates.io, font-kit)          // Hex and Scrawl, identical
├── screencapturekit =11.0.0, macos_15_0      // identical
├── cpal 0.18.2, metal 0.29, objc2 0.6.4, objc2-app-kit 0.3.2, block2, core-video
└── Errors differ: Scrawl anyhow, Hex color-eyre   // small adapter

Scrawl's modules by portability
├── Pure Rust, no platform: model, keyframes, sketch, render, autocrop, export/{crop,camera,ink}, hold (state machine)
├── macOS, no GPUI: capture (SCK recording, telemetry, event tap), windows, laser_overlay (objc2 + Metal), retention, routing, delivery
├── GPUI: ui.rs (5.7k lines: capsule island, picker, review window)
├── Bundle pipeline lives in main.rs (finalize_capture_bundle, build_bundle_from_parts)  // needs extracting
└── scrawl-helper (Swift, separate executable): SpeechAnalyzer, frame decode, iPhone mirror, export, onboarding

Hex today
├── One event tap (suppression.rs): a modifying key tap + a passive tap for flags and mouse-down
├── Paste Last ⌥⇧V and Paste Meeting ⌥⌃V are reserved chords inside DictationHotkey::process
├── Screen Recording helpers already exist in meeting.rs (permitted / request / open settings)
├── transcribe.cpp already returns word times (t0_ms, t1_ms) for Parakeet; local API returns text only
└── Swift bridges are compiled into the hex binary (apple_speech, apple_intelligence), no helper process
```

Licence: Scrawl's `Cargo.toml` says `license = "MIT"` but the repo has no
`LICENSE` file. Hex is MIT under `anomalyco/hex`. See Risks.

## 1. Settings

Show an agent gets its own pane, shaped like Voice Action: the pane is always
in the sidebar on macOS, with the opt-in toggle at the top and everything
else hidden or disabled until it's on. Release builds start with it off.
Linux and Windows never show it.

```ts
Hex > Show an agent (beta)                        // show-agent.opt-in; off by default
├── Off -> No shortcut reserved, no screen permission asked, no workers
└── On
    ├── Screen & System Audio Recording missing -> Request -> "Open settings" row (Meetings' control)
    ├── Shortcut -> ⌥⇧S by default; Hex's shortcut capture; key chords only
    ├── Keep recordings -> Video 7 days, bundles 30 (default) / Video 30, bundles 90 / Forever
    ├── Crop to activity -> On                    // Scrawl auto-crop, Screen captures only
    ├── Send dev-site captures to OpenCode -> Off  // needs opencode2; asks for browser Automation once
    └── Show captures in Finder
```

Persisted as one new block in `settings.json`, following `MeetingSettings`:

```ts
show_agent: {
  enabled: false,
  hotkey: { modifiers: ⌥⇧, key: S },
  retention: { media_days: 7, bundle_days: 30 } | null,   // null = forever
  auto_crop: true,
  send_to_opencode: false,
}
```

Turning it on is the only thing that asks for Screen Recording. Recording
refuses to start without the grant and opens the settings row instead,
matching Meetings and Scrawl's own rule that capture code never prompts.

What survives from Scrawl, and how it changes:

```ts
Scrawl option                    -> In Hex
├── Transcriber (Apple / Hex)    -> Gone. Always Hex's selected model, with real word times
├── Hex model picker             -> Gone. Uses the dictation model already loaded
├── Keep Recordings              -> Kept, same three choices and Trash-not-rm rules
├── Auto-crop on/off             -> Kept as one toggle; tuning numbers stay in code
├── Send to OpenCode (autoSend)  -> Kept, but default OFF (Scrawl defaults on)
├── Shortcut                     -> Kept, through Hex's shortcut field and conflict checks
└── Video export flag            -> Hidden. Not in Hex until it leaves Scrawl's flag
```

Hex users should never see these:

- iPhone capture, ScrawlKit ingest, `scrawl link`, the ingest token, port
  47777, `usbApps`, and the `sdk.js` page registration (it needs that server).
  Zero-setup localhost routing (`lsof` to cwd to git root) still works.
- The autocrop playground, corpus, and eval tools.
- Scrawl's own onboarding, Sparkle feed, menu bar item, and review window.
- `⌥⇧V` "paste contact sheet + summary". It collides with Hex's Paste Last.
  The clipboard already holds the text and paths after a capture, so `⌘V`
  covers it.
- The Scrawl CLI surface. Hex keeps none of it in release; a debug-only
  `hex dev scrawl reprocess <bundle>` is enough for development.

## 2. Hotkeys

### How ⌥⇧S meets the Option dictation hotkey today

I traced `DictationHotkey::process` with the default Option binding:

```ts
Press ⌥, then ⇧ within 300 ms, then S        // a normal fast chord
  -> ⌥ alone matches exactly -> Start (pre-roll capture begins)
  -> ⇧ joins before MINIMUM_HOLD_DURATION -> extra_modifiers -> Discard -> Dirty
  -> Nothing transcribed or pasted; mute/pause/no-sleep never applied

Press ⌥, wait > 300 ms, then ⇧ S              // slow chord
  -> Recording stays live; S is just an unrelated key after the threshold
  -> Release ⌥ -> Finish -> transcribes and pastes whatever was said   // the real conflict

Press ⇧ first, then ⌥                         // Shift already down
  -> ⌥ never matches exactly -> no dictation
```

Paste Last (`⌥⇧V`) already lives with this by being matched inside
`process` as a reserved chord: on its key-down the state goes `Dirty`, the
action fires, and the tap swallows the key. Show an agent should use the
same path, which also closes the slow-chord case: the `S` key-down while
Option dictation is live becomes "discard this dictation, start Show an agent".

Voice Action (`⌥⌘`) is modifier-only and has no S key, so it never matches
`⌥⇧S`. Its conflict check (`HotkeyBinding::overlaps`) covers the new binding
the same way it covers Paste Last.

### Options

**A. Separate key chord with Scrawl's tap/hold behaviour (recommended).**
`⌥⇧S` by default, rebindable to any modifier+key chord, matched in Hex's one
event tap like Paste Last. Tap records the screen under the pointer; tap again
finishes. Hold opens the mode switcher later (milestone 3). Off means not
reserved, the same as Voice Action.

**B. Dictation hotkey plus a modifier.** For example hold `⌥⇧` to record the
screen. This fails for two reasons. A screen recording runs for minutes while
you click, type, and draw, so it can't be hold-to-record. And Shift is the ink
modifier, so it can't also be part of the held chord.

**C. No global chord.** Start from the menu bar, or from a Hex API/CLI call
the user binds elsewhere. This has no hotkey risk, but it loses the main
gesture: "something's wrong, tap, talk, tap".

A variant of A, "hold Option to dictate, then press S to promote into a
screen recording", was also considered. It means swallowing `⌥S` during
dictation, which breaks typing `ß` and other Option characters for some
layouts. Rejected.

### Behaviour under option A

```ts
Show an agent enabled, shortcut ⌥⇧S             // show-agent.shortcut
├── Tap (released < 250 ms) -> Record display under pointer, auto-cropped
│   └── Tap again (> 400 ms after start) -> Finish -> Build bundle (~1 s) -> Clipboard
├── Hold ⌥⇧ after S -> Switcher on Screen; S steps Screen → Window; drag = Area   // milestone 3
│   └── Release ⌥⇧ -> Record selection; Esc -> close switcher, record nothing
└── Key-down always: Dictation -> Dirty (discard any < 300 ms or slow-chord capture)

While recording
├── Hold ⇧ + move -> Red ink; clicks -> ripples
├── Dictation shortcut -> Still works (its audio also lands in the narration)
├── Voice Action shortcut -> Still works
├── Escape -> Passed to the app, NOT a cancel        // see below
└── Discard -> Capsule button or menu bar "Discard recording", with a few seconds of Undo
```

Escape needs a decision. In Hex, Escape cancels the active capture, then the
newest unfinished job. In Scrawl, Escape cancels the recording. During a
screen recording the user is demonstrating an app, where Escape is an
ordinary key (closing a dialog, leaving a field), so one press would throw
away minutes of work. I recommend that Escape not be reserved during a screen
recording, and that the switcher (milestone 3) still close on Escape because
nothing has been recorded yet. Hex's existing Escape rules for dictation
captures and jobs stay as they are.

Shift for ink is observed, never suppressed, so typing capitals still works.
Whether a Shift held while typing with a resting mouse leaves stray ink is an
open question below.

What's configurable: the chord (modifier+key, at least one modifier, not
modifier-only), checked against Dictation, Voice Action, Paste Last, and
Paste Meeting. The 250 ms tap window, the 400 ms double-press guard, and the
Shift ink modifier stay fixed.

## 3. Architecture

```ts
(a) Sidecar: ship Scrawl's binary inside Hex.app, Hex drives it
├── + Least porting; process boundary survives the AppKit rewrite untouched
├── − Two event taps, two hotkey systems, two mic streams, two GPUI copies (~+18 MB)
├── − Two settings stores to keep in sync; Scrawl's menu/onboarding must be stripped anyway
└── − The ⌥⇧S/dictation conflict above stays unsolved across processes

(b) Copy Scrawl's source into hex/src
├── + One process, one tap, one mic owner, one settings file
├── − ~30k lines forked; Scrawl and Hex drift immediately
└── − ui.rs is GPUI, so it is thrown away again by the AppKit rewrite

(c) Shared crate (recommended)
├── scrawl-core: model, keyframes, sketch, render, autocrop, export plan, bundle pipeline   // no GPUI, no objc
├── scrawl-macos: SCK screen recording, input telemetry, window snapshots, Metal ink overlay
├── Hex owns: hotkey, permissions, settings, microphone, transcription, HUD/capsule, menu bar
├── Scrawl app depends on the same crates, so fixes land in both
└── − Upfront extraction work in Scrawl (main.rs pipeline, capture.rs's own tap/hotkeys)
```

Recommendation: **(c)**. Hex and Scrawl already pin the same GPUI,
ScreenCaptureKit, cpal, Metal, and objc2 versions, so a shared dependency
costs little now. Scrawl's valuable parts (keyframe choice, auto-crop,
rendering, bundle format) are already platform-neutral Rust. The ink overlay
is objc2 + Metal, not GPUI. Only the capsule and picker UI are GPUI, and Hex
would draw those with its own components anyway.

How each option survives `feat/native-macos` (AppKit app over the `libhex`
C ABI in `src/ffi.rs` / `hex.h`):

```ts
(a) Sidecar   -> Unaffected, and still carries every cost above
(b) Copy      -> Pipeline survives in libhex; the copied GPUI UI is redone in Swift
(c) Crates    -> scrawl-core + scrawl-macos link into libhex unchanged
                 Capsule/picker become Swift views fed by engine events, like the HUD telemetry
                 The Metal ink overlay can stay in Rust or move to Swift; either works
```

Where the crates live: a Cargo workspace in the Scrawl repo
(`crates/scrawl-core`, `crates/scrawl-macos`), consumed by Hex as a pinned git
dependency (`rev = …`), as Hex already does for `transcribe-rs`. Publishing to
crates.io can wait.

Inside Hex (new modules, following AGENTS.md naming):

```ts
show_agent            // owner: start/finish/discard, bundle pipeline call, clipboard, routing
show_agent_overlay    // the ink/ripple overlay and capsule; observational, click-through
screen_recording      // permission helpers moved out of meeting.rs (now two real users)
```

Narration audio: Meetings already records the microphone through
ScreenCaptureKit (`SCStreamOutputType::Microphone`) beside Hex's own cpal
dictation stream. Doing the same here puts voice and video on one clock and
leaves dictation's microphone owner alone. Scrawl currently uses its own cpal
stream plus `videoOffsetMs`, so this needs a check (open question 4).

Transcription: after Finish, the bundle's audio goes through Hex's
transcription worker at utility QoS and yields to dictation, the same rule as
meeting transcription. It asks for word timestamps, so `estimated` is false
and spoken cues ("now", "here") cut keyframes again. Whisper models that only
report segment timestamps fall back to Scrawl's existing island spreading.

Frame decoding: Scrawl decodes final keyframes from `recording.mp4` through
`scrawl-helper frames` (AVFoundation). Hex would compile that piece as a
third Swift bridge in `build.rs`, like `apple_speech`, and would not ship a
helper executable.

### Milestones

```ts
M0  Word times in Hex's local API                 // small; helps today's Scrawl users too
    POST /transcriptions?timestamps=word -> { text, words: [{ text, t0_ms, t1_ms }] }
    Scrawl's Hex engine stops estimating when words are present
    Proves: Hex's timings drive Scrawl's keyframes as well as SpeechAnalyzer does

M1  Smallest in-Hex proof (macOS, opt-in, debug + release behind the toggle)
    Tap ⌥⇧S -> record display under pointer (SCK video + mic) -> menu bar shows recording
    Tap ⌥⇧S -> scrawl-core builds bundle: click + scene-change keyframes, auto-crop,
               contact sheet, summary.md with Hex word times -> clipboard text + paths
    No ink, no switcher, no routing, no retention UI (fixed 7/30 days)
    Proves: the shared crate works inside Hex, the chord doesn't hurt dictation,
            and Hex users get a useful bundle from one gesture

M2  Ink, click ripples, laser (Metal overlay), capsule with Discard + Undo
M3  Hold switcher: Screen / Window / Area; the remaining settings rows
M4  Send dev-site captures to OpenCode (opencode2 discovery Hex already has)
Later  hex mcp read-only access to captures, like meetings; video export if it leaves its flag
Never  iPhone / ScrawlKit in Hex
```

M1 needs Scrawl-side work first: extract the pipeline out of `main.rs`, split
`capture.rs` so recording and telemetry don't install their own hotkeys or
tap, and put both crates in a workspace.

## 4. Permissions, onboarding, risks

```ts
Hex onboarding (unchanged)                       // gate for dictation only
├── Microphone, Input Monitoring, Accessibility, dictation model
└── Show an agent never blocks or delays this gate

Turn on Show an agent
├── Screen & System Audio Recording -> request once, then "Open settings"
│   └── Granted but this process can't see it -> "Relaunch Hex" (Scrawl's known case)
├── Input Monitoring -> already granted; the tap additionally watches mouse up/drag/scroll
├── Microphone -> already granted
└── Automation (browser) -> only when "Send dev-site captures" is turned on
```

Hex already asks for Screen Recording when Meetings is enabled, so users
with both features see one grant.

Risks:

- **Dictation regressions.** The only change to `suppression.rs` is one more
  reserved chord on the Paste Last path, inactive while the feature is off.
  New tests must cover the fast and slow `⌥⇧S` orders with an Option
  binding, rebinding conflicts, and Escape still cancelling dictation during a
  screen recording. Mouse up/drag/scroll go to the passive tap only; the
  modifying key tap's mask does not change.
- **Privacy and retention.** A bundle keeps video, `audio.wav`, window titles
  (in `scrawl.json`), and anything on screen, including secrets. This is an
  explicit, visible recording, the same exception Meetings has to "don't
  persist captured audio". Files are owner-only, retention defaults to 7/30
  days, deletion goes to the Trash, and the menu bar shows recording the whole
  time. Sending to OpenCode puts screenshots in front of a provider, so it is
  a separate opt-in that starts off.
- **Binary size.** Hex's installed binary is about 30 MB and Scrawl's about
  18 MB. Most of Scrawl's size is GPUI and other crates Hex already links, so
  the expected addition is a few MB. M1 should measure the stripped release
  binary before and after.
- **GPUI skew.** Both use gpui 0.2.2 today. scrawl-core must not depend on
  GPUI at all, and scrawl-macos only on objc2/Metal/SCK, so a later GPUI bump
  in either app doesn't block the other.
- **Running both apps.** A user with Scrawl.app and Hex's feature on gets two
  recordings from one `⌥⇧S`. Hex should check for a running
  `dev.kitlangton.scrawl` when the toggle is turned on and say so.
- **Licensing.** Scrawl declares MIT in `Cargo.toml` but ships no `LICENSE`
  file, and Hex lives in the `anomalyco` organization. Add an MIT `LICENSE`
  (copyright Kit) to Scrawl before any code is shared, and decide whether the
  crates move under `anomalyco`.
- **macOS Screen Recording re-prompts.** macOS 15 asks again periodically for
  apps using screen capture. That affects Meetings already, but a feature
  triggered by a hotkey makes the prompt more visible.

## Decisions for Kit

1. Option A (separate `⌥⇧S` chord) and crates (c): yes or no.
2. Escape during a screen recording is passed to the app, and Discard lives
   on the capsule and menu with Undo. Or keep Scrawl's Escape-cancels.
3. Feature name in the UI: "Show an agent", or something else.
4. Where bundles go: `Application Support/voice-control/captures`
   (owner-only, like meetings) or a visible folder like `~/Documents/Hex Captures`.
5. Should M0 ship on its own first, before any folding work?

## Open questions for Scrawl

1. How the laser pointer and pins are triggered on the Mac. The README covers
   ink (`⇧` + move) and clicks but not the laser gesture.
2. Is `⌘Z` (undo stroke) swallowed during a recording, or does it also reach
   the app underneath?
3. Does holding Shift while typing, with the mouse still, ever produce ink?
   Does ink need mouse movement, or is it sampled from a polled cursor?
4. Can narration come from ScreenCaptureKit's microphone output instead of a
   separate cpal stream, and what does `videoOffsetMs` assume about the clocks?
5. Why Scrawl uses three shortcut sources (Carbon hotkeys, event tap, HID
   poller, deduplicated within 90 ms). Which failure did each one fix
   (secure input, focus, a missed release)? Hex only has the event tap.
6. Does `finalize_capture_bundle` / `build_bundle_from_parts` depend on any
   `ui.rs` state, or can it be lifted into a crate as is?
7. With real word times but no recognizer segments, does clause splitting in
   `keyframes.rs` behave as it does with SpeechAnalyzer, which reports segments?
8. Which parts of `capture.rs`'s `SharedCaptureTelemetry` are needed for a
   Screen capture without the picker, the island, or iPhone?
9. Can frame decoding (`scrawl-helper frames`) move to objc2 AVFoundation
   bindings, or must it stay Swift?
