# Meetings (beta)

Record a call locally: HEX captures the other participants (system audio) and
you (microphone) as separate tracks, transcribes them on this Mac, and saves a
Markdown transcript with `You` and `Them` turns. Recording never starts or stops
automatically. This map covers macOS; Linux has no meetings.

Linked checks are locators unless an executed result is recorded below.

## How To Get To It

```ts
Settings > MEETINGS > Meetings (beta)               // meetings.opt-in; off by default
├── On -> Meetings pane, menu-bar items, call detection, Option-Control-V
│   ├── Screen & System Audio Recording missing -> "Open Settings" row
│   └── Transcript folder -> ~/Documents/Hex Meetings by default; Reveal / choose
└── Off -> Controller idles; no microphone-activity inspection, pane, or shortcut

Debug builds -> Always available (DEVELOPER_FEATURES_ENABLED)
```

Requires the selected local dictation model, Microphone, and Screen & System
Audio Recording. The opt-in is `meetings.enabled` in `settings.json`
([app_settings.rs](../../src/app_settings.rs) `MeetingSettings`). Recording
refuses to start without Screen Recording and asks macOS to list HEX.

## Sub-features

### Detect And Offer

```ts
Zoom / Teams / Slack huddle / FaceTime / browser call uses the microphone
  -> 4 stable observations (about 2 s)              // meetings.detect
  -> Capsule below the menu bar, focus-free          // meetings.offer
     ├── Record -> Start recording
     ├── Not Now -> Close
     └── 20 s countdown -> Close                     // never records on timeout
  -> Same call is snoozed until its app is off the microphone for about 60 s
```

Sources: [meeting_detection.rs](../../src/meeting_detection.rs),
[microphone_activity.rs](../../src/microphone_activity.rs), and the
`MeetingPanel` capsule in [meeting_watcher.rs](../../src/meeting_watcher.rs).
Checks: `requires_stability_and_offers_once_per_session`,
`a_brief_microphone_release_does_not_re_offer_the_same_call`, and
`named_meetings_keep_their_title_and_app_calls_use_the_app`. They do not exercise
CoreAudio process enumeration or a real call.

Preview: `scripts/capture-preview.sh OUT meeting-offer [--meeting-panel call-ended|ready]`.

### Record

```ts
Record (offer, menu bar "Record Meeting", Meetings pane, or voice command)
  -> Title: named meeting OR "<App> call — 2:30 PM" OR "Meeting — 2:30 PM"
  -> system.wav + microphone.wav, owner-only, Application Support/meetings/<id>
  -> No idle sleep; no dictation mute or media pause // meetings.record
  -> Menu bar: record symbol + elapsed time
     ├── "Recording meeting · 12:34" -> Meetings pane
     └── Stop & Transcribe
  -> Settings window may stay closed

Detected app leaves the microphone for about 3 s     // meetings.call-ended
  -> "Zoom left the call" capsule: Keep Recording / Stop & Transcribe
  -> No timeout; nothing stops automatically
```

Starting a recording no longer opens the Settings window; failures still do.
Checks: `call_end_is_reported_once_per_sustained_absence` and
`meeting_menu_offers_one_action_per_state` in
[status_item.rs](../../src/status_item.rs). Native menu-bar and capsule clicks
are unverified.

### Transcribe

```ts
While recording                                      // meetings.rolling
  -> Each track -> 16 kHz streaming resampler -> chunker
     ├── First pause after 20 s (pause segmenter) -> chunk boundary
     └── No pause by 30 s -> hard cut; next chunk overlaps 2 s
  -> Utility-QoS worker transcribes a chunk only while dictation is idle
  -> transcript.rolling.ndjson replaces the Moonshine draft for covered time

Stop & Transcribe
  -> Flush tails -> finish pending chunks           // usually seconds
  -> Any track that fell behind or failed -> stream its WAV in 1 s blocks
  -> Join on one timeline -> Echo suppression -> Final publication
```

Design choice: a separate session of the selected model at utility QoS, loaded
lazily on the first completed chunk, rather than a job class inside the dictation
inference worker. Before each chunk it waits (up to 20 s per chunk) while
`parakeet::dictation_busy()` reports an intentional capture or dictation
inference. A chunk already running can still delay a dictation job by roughly
one chunk's inference time, and the second model stays resident while recording.

Seams: segments starting before the middle of an overlap belong to the earlier
chunk, and repeated words (two to twelve) at the join are removed.

Echo: a `You` utterance (word segments within 700 ms) or sentence (three or more
words) is dropped when at least 60% of its words match `Them` speech within
±1.5 s; words match exactly, or by 60% edit similarity when both have three or
more letters (`AICK` ≈ `AISDK`). Dropped counts are saved as
`echo_suppressed_utterances` in the manifest.

Sources: [meeting_transcript.rs](../../src/meeting_transcript.rs),
`StreamingResampler` in [dictation.rs](../../src/dictation.rs). Checks:
`continuous_speech_is_hard_cut_with_overlap_and_covers_every_sample`,
`chunks_end_at_the_first_pause_after_the_target_length`,
`overlapping_chunks_join_at_the_middle_of_the_overlap`, `seam_drops_repeated_words`,
`speaker_echo_of_remote_speech_is_dropped`, `word_level_echo_is_dropped_as_one_utterance`,
`distinct_or_distant_local_speech_is_kept`, and
`streaming_resampler_matches_whole_buffer_conversion`. They use synthetic audio
and transcripts, not model inference or real speaker echo.

### Save And Reuse

```ts
Final publication                                   // meetings.output
├── Internal: transcript.ndjson + transcript.md (atomic, owner-only)
└── Markdown copy -> <folder>/<YYYY-MM-DD HHMM> <title>.md
    ├── Title, date, duration, app, You / Them turns
    ├── Never overwrites; adds " (2)"
    └── Failure -> Meeting still Complete; "Markdown not saved: …"

Transcript ready capsule (15 s) -> Copy (Markdown) / Reveal
Meetings pane -> Reveal Markdown (or Reveal Files when none)
Option-Control-V -> Paste meeting turns added since the last paste
```

Checks: `markdown_lists_metadata_and_labeled_turns` and
`export_never_overwrites_and_uses_safe_names`.

## Gaps

- No executed end-to-end recording with a real call, Screen Recording grant, or
  speakers echo in this change; see the test script in the handoff.
- Calendar titles (EventKit) are not implemented.
- The live Moonshine draft needs the command model; without it, the pane shows
  rolling chunks only as they complete (about every 20–30 s).
- Settings and capsule previews launched, but screenshot capture failed on the
  build host (`could not create image from window`); layout is visually unverified.
