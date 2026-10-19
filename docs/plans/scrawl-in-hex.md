# Scrawl in Hex (experiment plan)

Status: plan only, nothing built. Branch `experiment/scrawl`, cut from
`origin/main` at `8efd990`.

Scrawl ("show an agent the problem") records the screen, your voice, clicks,
and red ink, then packs them into a `.scrawl` bundle an agent can read. Hex
users have asked for it "connected with Hex". This plan makes Hex the one
app: Scrawl's capability moves into the Hex repo as an opt-in macOS feature,
working name **Show an agent (beta)**, and the standalone Scrawl app is
retired once Hex covers what its users need (section 5). The logic stays
modular inside Hex (section 3).

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

Hex users should never see these in Settings:

- The ingest token, port 47777, `usbApps`, and pairing details. If iPhone
  ingest comes to Hex later (section 5), it is one toggle plus a "Pair
  iPhone app" action, nothing more.
- The autocrop playground, corpus, and eval tools (debug builds only).
- Scrawl's own onboarding, Sparkle feed, menu bar item, and review window.
- `⌥⇧V` "paste contact sheet + summary". It collides with Hex's Paste Last.
  The clipboard already holds the text and paths after a capture, so `⌘V`
  covers it.

Agent-facing commands move under `hex scrawl …` (section 5), next to the
existing `hex meeting …`.

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

## 3. Architecture: one app, modular inside

Hex is the only app. Scrawl's code moves into the Hex repo and Hex owns the
hotkey, permissions, settings, microphone, transcription, HUD/capsule, and
menu bar. Nothing outside Hex consumes these modules.

```ts
hex repo
├── crates/scrawl-core            // new workspace crate; pure Rust, no gpui/objc2/SCK in its Cargo.toml
│   ├── model, bundle (scrawl.json, summary.md, write/read), keyframes, sketch
│   ├── autocrop, render (frames, contact sheet), export plan (crop, camera, ink)
│   └── tests + autocrop corpus json move with it
└── src/ (the hex crate)
    ├── show_agent.rs             // owner: start/finish/discard, call scrawl-core, clipboard, routing, retention
    ├── show_agent_capture.rs     // macOS: SCK screen recording, input telemetry, window snapshots
    ├── show_agent_overlay.rs     // macOS: Metal ink/ripple/laser overlay; observational, click-through
    ├── screen_recording.rs       // permission helpers moved out of meeting.rs (two real users now)
    └── suppression.rs            // one more reserved chord; no second tap
```

Why exactly one workspace crate: Hex is a single crate today, and a full
build pulls in transcribe.cpp, GPUI, and Moonshine. scrawl-core is about
10k lines of pure logic and tests, and Scrawl's tuning loop
(autocrop eval, keyframe rules) runs those tests constantly. As its own crate,
`cargo test -p scrawl-core` builds in seconds without Hex's native
dependencies. Its `Cargo.toml` also enforces "no GPUI, no objc" mechanically
instead of by review. The macOS capture and overlay code shares Hex's objc2,
ScreenCaptureKit, and event-tap code, so splitting it into a crate would only
add a boundary to maintain. Those parts stay as modules in the main crate.

Code that does not come across: `ui.rs` (Hex draws the capsule with its own
`desktop_ui` components), Scrawl's Carbon hotkeys, its own event tap and HID
poller, its onboarding, Sparkle, `scrawl-helper` as a process, and the Apple
transcriber path.

How this maps onto `feat/native-macos` (AppKit app over `libhex`, `src/ffi.rs`):

```ts
scrawl-core                 -> linked into libhex unchanged
show_agent, _capture        -> engine side of libhex; same as meetings
suppression chord           -> already engine side
capsule / review UI         -> Swift views fed by engine events and a telemetry read,
                               like hex_engine_read_hud_telemetry for the HUD
show_agent_overlay          -> stays Rust (objc2 + Metal) or moves to Swift; either works
new C ABI                   -> start/finish/discard actions via hex_engine_dispatch_json,
                               capture events via the existing event callback
```

Narration audio: Meetings already records the microphone through
ScreenCaptureKit (`SCStreamOutputType::Microphone`) beside Hex's own cpal
dictation stream. Doing the same here puts voice and video on one clock and
leaves dictation's microphone owner alone. Scrawl currently uses its own cpal
stream plus `videoOffsetMs`, so this needs a check (open question 4).

Transcription: after Finish, the bundle's audio goes through Hex's
transcription worker at utility QoS and yields to dictation, the same rule as
meeting transcription. It reads word timestamps straight from transcribe.cpp,
so `estimated` is false and spoken cues ("now", "here") cut keyframes again.
Whisper models that only report segment timestamps fall back to Scrawl's
existing island spreading.

Frame decoding: Scrawl decodes final keyframes from `recording.mp4` through
`scrawl-helper frames` (AVFoundation). Hex compiles that piece as a third
Swift bridge in `build.rs`, like `apple_speech`, with no helper executable.

### Milestones

M0 from the first draft (word timings in Hex's local API for Scrawl's sake)
is dropped. It would improve an app that is being retired, and M1 reads word
times inside Hex without the API.

```ts
M1  Smallest in-Hex proof (macOS, opt-in toggle, off by default)
    Extract scrawl-core into crates/scrawl-core (pipeline out of Scrawl's main.rs)
    Tap ⌥⇧S -> record display under pointer (SCK video + mic) -> menu bar shows recording
    Tap ⌥⇧S -> scrawl-core builds bundle: click + scene-change keyframes, auto-crop,
               contact sheet, summary.md with Hex word times -> clipboard text + paths
    No ink, no switcher, no routing, no retention UI (fixed 7/30 days)
    Proves: the core works inside Hex, the chord doesn't hurt dictation,
            and Hex users get a useful bundle from one gesture

M2  Ink, click ripples, laser (Metal overlay); capsule with Discard + Undo
    hex scrawl inspect | frames | zoom   // what agents run on bundles
M3  Hold switcher (Screen / Window / Area); remaining settings rows; retention worker
M4  Send dev-site captures to OpenCode (opencode2 discovery Hex already has)
    -> Parity for Mac screen capture: freeze the Scrawl app (section 5)
M5  iPhone: ScrawlKit ingest and cabled capture, behind their own opt-in, if kept
Later  hex scrawl export (video), hex mcp read-only access to captures
```

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
- **GPUI.** Both apps use gpui 0.2.2 today, so the port has no version
  skew. With one app it can't recur. scrawl-core has no GPUI dependency, so
  the AppKit rewrite doesn't touch it.
- **Running both apps during the transition.** A user with Scrawl.app and
  Hex's feature on gets two recordings from one `⌥⇧S`. Hex warns when the
  toggle is turned on while `dev.kitlangton.scrawl` is running (section 5).
- **Licensing.** Scrawl declares MIT in `Cargo.toml` but ships no `LICENSE`
  file, and Hex lives in the `anomalyco` organization. Add an MIT `LICENSE`
  (copyright Kit) to Scrawl before its code is copied into Hex, so the
  provenance of the moved code is clear.
- **History.** Copying files loses Scrawl's git history for that code. A
  `git subtree`/filtered import of the moved files would keep it, at the cost
  of a noisier Hex log. Plain copy with a link to the Scrawl commit is simpler.
- **macOS Screen Recording re-prompts.** macOS 15 asks again periodically for
  apps using screen capture. That affects Meetings already, but a feature
  triggered by a hotkey makes the prompt more visible.

## 5. The Scrawl app, its users, and its extras

The Scrawl app keeps working, unchanged, until Hex reaches M4. Then it gets
one last Sparkle release and is frozen. The Scrawl repo stays read-only as
history and is not maintained.

```ts
Scrawl piece                     -> Fate
├── Mac screen capture, ink, keyframes, auto-crop, bundle  -> Hex, M1–M3
├── Dev-site routing to OpenCode (lsof -> git root)        -> Hex, M4
├── sdk.js page registration                               -> With iPhone ingest (needs the 47777 server), else retired
├── scrawl inspect / frames / zoom / send / route          -> hex scrawl …, M2–M4
├── scrawl reprocess, autocrop lab, fixture, eval          -> hex dev scrawl … (debug builds only)
├── Video export (scrawl export, flagged)                  -> hex scrawl export, later; menu item stays off
├── Retention (Keep Recordings, pin, clean)                -> Hex, M3; `hex scrawl pin`
├── Cabled iPhone capture (mirror + native video)          -> Hex M5 if Kit keeps it, else retired
├── ScrawlKit iOS SDK + ingest (scrawl serve, link)        -> Hex M5 if kept: package moves to hex/ios/ScrawlKit,
│                                                             same protocol, port, and link scheme so paired phones keep working
├── scrawl serve (headless on Studio)                      -> hex scrawl serve with M5, else retired
├── Transcriber choice (Apple / Hex)                       -> Retired; always Hex
├── Onboarding, Sparkle, menu bar, review window           -> Retired; Hex's own
└── scrawl.kitlangton.com                                  -> Redirect to a Hex site section at M4
```

Migrating existing Scrawl users:

- The final Scrawl release shows a one-time notice and menu item, "Scrawl is
  now part of Hex", linking to the Hex download. It never quits, deletes, or
  replaces itself, the same rule Hex used for the Swift app.
- Fresh setup in Hex, with no settings import. Scrawl's config is a shortcut,
  retention, and a transcriber choice, so re-picking takes seconds. Hex needs
  its own Screen Recording grant either way (new bundle identity).
- Old bundles stay where they are (`~/Library/Application Support/scrawl/captures`).
  `hex scrawl inspect|frames|zoom` accept any bundle path, since the format is
  unchanged. Hex's retention cleans only its own captures folder, so it never
  deletes old Scrawl bundles. The notice says where they are.
- While both apps are installed, Hex warns when the toggle is turned on and
  `dev.kitlangton.scrawl` is running, because one `⌥⇧S` would start two
  recordings.
- Paired iPhones: if M5 happens, Hex answers on the same port and protocol,
  so ScrawlKit apps keep sending without re-pairing. Until then, keep
  `scrawl serve` running on Studio.

## Decisions for Kit

1. Hotkey option A (separate `⌥⇧S` chord matched in Hex's tap): yes or no.
2. Layout: one `crates/scrawl-core` workspace crate plus modules in the hex
   crate, or everything as modules in the hex crate.
3. Escape during a screen recording is passed to the app, and Discard lives
   on the capsule and menu with Undo. Or keep Scrawl's Escape-cancels.
4. Feature name in the UI: "Show an agent", or something else.
5. Where bundles go: `Application Support/voice-control/captures`
   (owner-only, like meetings) or a visible folder like `~/Documents/Hex Captures`.
6. iPhone: bring cabled capture and ScrawlKit ingest into Hex (M5), or retire them.
7. When to freeze Scrawl.app: at M4 parity as proposed, or earlier.
8. Copy Scrawl's files plainly or import them with git history.

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
