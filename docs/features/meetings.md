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
│   ├── Transcript folder -> ~/Documents/Hex Meetings by default; Reveal / choose
│   └── Summarize with Apple Intelligence -> on by default; one-line availability
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
  -> Each track -> one-second blocks -> bounded queue (about 5 minutes)
  -> 16 kHz streaming resampler -> chunker + echo envelope
     ├── First pause after 20 s (pause segmenter) -> chunk boundary
     └── No pause by 30 s -> hard cut; next chunk overlaps 2 s
  -> Utility-QoS worker transcribes a chunk only while dictation is idle
  -> Timed words -> sentences (punctuation or a 600 ms pause)
  -> transcript.rolling.ndjson replaces the Moonshine draft for covered time
     └── Queue full -> that track keeps its entries; the draft continues after
         them; rebuilt from its WAV after Stop

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

### Echo Suppression

```ts
Call on speakers -> the microphone hears the remote side 0-300 ms later
Each microphone utterance or sentence                // meetings.echo
├── Signal (meeting_echo): 20 ms energy of both tracks on one timeline
│   ├── Lag (-100..+400 ms) and coupling from envelope correlation over ±60 s
│   │   └── Correlation < 0.2 (headphones) -> every sound is local
│   ├── Bleed gain = 35th percentile of mic − delayed system over ±5 s
│   └── Frame is local when mic > predicted bleed (160 ms reverb) + 9 dB
│       ├── ≤ 20% local -> Bleed -> dropped, whatever the words
│       ├── ≥ 50% local -> Local -> kept, even with matching words (double-talk)
│       └── otherwise   -> decided by words
└── Words: ≥ 60% match Them speech within ±1.5 s (60% edit similarity) -> echo

Applied in: rolling snapshot, final transcript, live view, Option-Control-V
Live view (meeting_live::live_view)
├── Live worker stamps each microphone draft line Bleed / Local
├── Rolling microphone entries are already judged; not re-judged by words
└── Undecided draft line overlapping remote speech -> hidden until it completes
    └── No twin by then -> shown as You
```

Dropped counts are saved as `echo_suppressed_utterances` in the manifest.

Root cause of `You` twins (Kit's 2026-10-15 call on speakers, Pro): ten minutes
in, the packet-bounded rolling queue filled (`rolling meeting transcription fell
behind` for both sources), the rolling snapshot became empty, and the pane
showed the raw Moonshine draft, which had no echo suppression. Rolling
transcripts were also one model segment per 20-30 s chunk, so a chunk mixing
echo and Kit's speech was kept or dropped whole.

Observed (`hex meeting echo <dir> [--microphone-offset-ms 300] [--transcribe]`,
copied 17-minute recording, Mac Studio):

```ts
Live draft You lines shown      212 -> 72
├── Twins of remote lines (word match)          105 -> 2
├── Kit's lines (240-274 s, 780 s on)            67 -> 67 kept
└── Other unmatched lines (mostly misheard bleed) 31 -> 3
   Word matching alone would show 80

Final, parakeet-unified-en
├── Before: 35 chunk-sized You entries, 10 kept, 25 dropped whole
└── After: 205 sentences, 74 kept; Kit's 240-274 s and 780 s+ speech kept,
    about 8 short fragments of uncertain origin kept
```

Acoustic echo cancellation (Voice Processing IO) was evaluated, not adopted:
it replaces ScreenCaptureKit's raw microphone with an AVAudioEngine/VPIO input,
ducks other audio, moves Bluetooth headsets to the call profile, adds gain and
noise processing to the transcribed track, runs beside the call app's own voice
processor, and loses ScreenCaptureKit's shared clock. See ROADMAP.md.

Sources: [meeting_transcript.rs](../../src/meeting_transcript.rs),
[meeting_echo.rs](../../src/meeting_echo.rs),
[meeting_live.rs](../../src/meeting_live.rs),
`StreamingResampler` in [dictation.rs](../../src/dictation.rs). Echo checks:
`speaker_bleed_is_classified_as_bleed`, `talking_over_the_remote_side_is_local`,
`speech_during_remote_silence_is_local`, `headphones_make_every_microphone_sound_local`,
`the_signal_overrides_word_matching_both_ways`,
`synthetic_speaker_bleed_is_dropped_and_double_talk_kept`,
`live_view_hides_microphone_twins_of_remote_speech`,
`an_undecided_draft_line_waits_while_the_remote_side_speaks`,
`live_view_keeps_rolling_microphone_entries_that_rolling_already_judged`, and
`timed_words_become_sentences_at_punctuation_and_pauses` (synthetic signals).
Other checks:
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

### Meetings Pane

```ts
Header: Search meetings (title, app, notes, final transcript) + Start / Stop
List (grouped Today / Yesterday / date)
└── Row: title (generated when present) · time · length · app · status chip
    └── Recording (red dot) / Transcribing / Interrupted / Failed; Ready in detail
Detail
├── Title, status chip, date · time · duration · app
├── Actions: Copy Notes, Copy Transcript, Reveal Markdown, Write/Regenerate
│   Notes, Rename, Delete (finished meetings only for the last four)
├── Errors at the top of the scroll area
├── Notes: Summary / Action items (checkbox + owner) / Decisions
└── Transcript: Them left, You right in accent; merged turns; timestamps;
    30 s+ pause dividers; live view follows the tail; "Listening..." while recording

Rename -> inline field; Enter or Save          // meetings.rename
  -> manifest title; Markdown heading only while it reads the old title
  -> file name unchanged; empty title refused
Delete -> "Trash Recording & Markdown" (or "Trash Recording") / Cancel
  -> exported Markdown, then Application Support/meetings/<id>, to the Trash
  -> refused while recording, transcribing, or writing notes   // meetings.delete
```

Choice: Delete trashes the Markdown copy too, since it holds the transcript;
both stay recoverable from the Trash. Sources: `render_meetings` in
[app_window.rs](../../src/app_window.rs), `trash` / `rename` in
[meeting.rs](../../src/meeting.rs). Checks:
`deleting_trashes_the_markdown_then_the_recording_and_refuses_while_recording`,
`renaming_updates_the_manifest_and_an_unedited_markdown_heading`,
`retitling_replaces_only_a_matching_first_heading`,
`meeting_search_delete_and_rename_work_on_the_selected_meeting`, and the GPUI
layout check `meeting_turns_alternate_sides_inside_the_detail`.

Preview: `scripts/capture-preview.sh OUT meetings [--meeting-recording]
[--confirm-meeting-delete] [--meetings-empty]`.

Checks: `markdown_lists_metadata_and_labeled_turns` and
`export_never_overwrites_and_uses_safe_names`.

### Notes (Apple Intelligence)

```ts
Final publication complete, setting on, model available   // meetings.notes
  -> meeting-notes thread, utility QoS; waits (≤30 s) while dictation is busy
  -> Coalesced turns "[mm:ss] You|Them: text" -> chunks
     ├── Chunk budget = (context − 1,800 tokens) × 3 chars (8,192 → ~19k chars)
     ├── More than 12 chunks -> 12 sampled evenly; Markdown says so
     ├── Map: one guided generation (@Generable) per chunk
     │   └── Context exceeded -> split that chunk in half once
     └── Reduce: batches that fit one chunk until one set remains
         └── Action items are the union of the chunks' items (≤12)
  -> 4-minute total deadline; each call ≤90 s
  -> Manifest: notes { title ≤8 words, summary ≤5, action items + owner, decisions ≤5 }
  -> Markdown copy, same file name, atomic replace
     ├── Heading -> generated title, only while it still reads HEX's last title
     ├── <!-- hex:notes --> Summary / Action items / Decisions above ## Transcript
     ├── Text outside the block is preserved; changed mid-write -> not written
     └── Missing file -> manifest only
  -> Failure / timeout -> notes_error; Markdown untouched; recording stays Complete

Unavailable (not eligible / Apple Intelligence off / model not ready / macOS < 26)
  -> Settings row says why; nothing runs
Meetings pane -> generated title in list and detail; Summary / Action items /
  Decisions above the transcript; "Write Notes" / "Regenerate Notes" / "Writing notes..."
Transcript ready capsule -> "Writing notes with Apple Intelligence…", then
  "Ready · <title>" when notes arrive while it is open
```

Title rule: the generated title replaces only a default `<App> call — 2:30 PM`
title; named meetings keep their name. The file is never renamed, so links,
open editors, and Reveal keep working.

Sources: [meeting_notes.rs](../../src/meeting_notes.rs),
[apple_intelligence.rs](../../src/apple_intelligence.rs), and the
FoundationModels bridge [apple_intelligence.swift](../../src/apple_intelligence.swift),
weak-linked from [build.rs](../../build.rs). Checks in `meeting::notes::tests`:
`chunks_map_then_reduce_into_one_set_of_notes`, `an_oversized_chunk_is_split_once`,
`failures_and_the_deadline_stop_generation`,
`notes_are_inserted_above_the_transcript_and_regenerate_idempotently`,
`user_edits_outside_the_notes_block_are_preserved`, and
`notes_title_replaces_only_default_titles`. They use a fake model.

Real model (debug CLI): `hex meeting summarize <file.md> [--chunk-chars N]` prints
notes without saving; `hex meeting summarize <id>` writes them like Regenerate.
Observed on the Mac Studio (macOS 27.0.1, context 8,192 tokens) with a synthetic
19.5-minute, 1,963-word transcript: one call, 11–14 s; with `--chunk-chars 4000`,
3 chunks + 1–2 reduces, ~42 s. Small chunks label more owners `You` wrongly.

## Gaps

- No executed end-to-end recording with a real call, Screen Recording grant, or
  speakers echo in this change; see the test script in the handoff.
- Calendar titles (EventKit) are not implemented.
- The live Moonshine draft needs the command model; without it, the pane shows
  rolling chunks only as they complete (about every 20–30 s).
- Settings, capsule, and Meetings pane previews launched, but screenshot capture
  failed on the build host (`could not create image from window`; the Studio
  session was locked, `CGSSessionScreenIsLocked 1`). The redesigned pane is
  verified only by GPUI layout checks, not visually.
- Echo thresholds are tuned on one speaker-mode call. The offline replay assumed
  the microphone started 300 ms after the system track because the recording's
  manifest had no arrival times yet. Real NSFileManager trashing and the native
  rename field are unexercised in the app.
- Why the rolling worker stalled for over a minute on the Pro is unknown; slow
  chunks (> 5 s) are now logged.
- Notes have not run after a real recording in the app; the CLI path exercised
  the real model, manifest update, and Markdown rewrite.
