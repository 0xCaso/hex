//! Meeting notes from Apple's on-device model: a title, summary, action items,
//! and decisions generated after final publication, stored in the manifest,
//! and written into the user's Markdown copy.

use std::collections::BTreeSet;
use std::fs::{self, File};
use std::path::Path;
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

use color_eyre::eyre::{Result, WrapErr, eyre};
use serde::{Deserialize, Serialize};

use super::{
    MeetingManifest, MeetingStatus, TranscriptEntry, TranscriptPublication, coalesce_transcript,
    format_duration, read_manifest, root, sync_directory, write_manifest,
};
use crate::apple_intelligence::{self, GeneratedNotes, GenerationError};
use crate::events::now_ms;

/// Room left in the context for instructions, the schema, and the response.
const RESERVED_TOKENS: usize = 1_800;
const MIN_USABLE_TOKENS: usize = 1_000;
/// Conservative for English speech, which averages about four per token.
const CHARS_PER_TOKEN: usize = 3;
const FALLBACK_CONTEXT_TOKENS: usize = 4_096;
/// Longer transcripts are sampled evenly down to this many chunks.
const MAX_CHUNKS: usize = 12;
const TOTAL_DEADLINE: Duration = Duration::from_secs(4 * 60);
const CALL_TIMEOUT: Duration = Duration::from_secs(90);
const DICTATION_WAIT: Duration = Duration::from_secs(30);
const MAX_TITLE_WORDS: usize = 8;
const MAX_SUMMARY: usize = 5;
const MAX_ACTION_ITEMS: usize = 12;
const MAX_DECISIONS: usize = 5;

pub(super) const NOTES_START: &str = "<!-- hex:notes -->";
pub(super) const NOTES_END: &str = "<!-- /hex:notes -->";

const MAP_INSTRUCTIONS: &str = "You write concise, factual notes from a meeting transcript. \
Each line starts with a timestamp and a speaker label: You is the person who recorded the \
meeting, and Them is one or more other participants. Use only what the transcript says. \
The summary covers the topics discussed and their outcomes; do not repeat action items there. \
List every task someone agreed to do as an action item, including tasks You agreed to do. \
Write each task as a short imperative without the owner's name. Set the owner to You when the \
recording person committed to it, to a name only when that name was stated, and otherwise \
leave it empty. Never invent names, dates, or numbers.";

const REDUCE_INSTRUCTIONS: &str = "You merge notes from consecutive parts of one meeting into \
final notes for the whole meeting. Write one short title for the whole meeting. The summary \
covers the main topics and their outcomes in at most five short sentences, without repeating \
action items. Keep every distinct decision as one short sentence. Combine duplicates and use \
only what the notes say.";

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct MeetingNotes {
    pub title: String,
    pub summary: Vec<String>,
    pub action_items: Vec<ActionItem>,
    pub decisions: Vec<String>,
    pub generated_at_ms: u64,
    pub elapsed_ms: u64,
    /// Transcript chunks summarized; fewer than `total_chunks` when a long
    /// meeting was sampled.
    pub chunks: usize,
    pub total_chunks: usize,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ActionItem {
    pub task: String,
    #[serde(default)]
    pub owner: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Budget {
    pub chunk_chars: usize,
    pub max_chunks: usize,
}

impl Budget {
    pub(super) fn for_context(tokens: usize) -> Self {
        let tokens = if tokens == 0 {
            FALLBACK_CONTEXT_TOKENS
        } else {
            tokens
        };
        Self {
            chunk_chars: tokens
                .saturating_sub(RESERVED_TOKENS)
                .max(MIN_USABLE_TOKENS)
                * CHARS_PER_TOKEN,
            max_chunks: MAX_CHUNKS,
        }
    }
}

/// Renders coalesced turns as labeled lines packed into chunks of at most
/// `chunk_chars`. A turn longer than one chunk is split at word boundaries.
pub(super) fn transcript_chunks(entries: &[TranscriptEntry], chunk_chars: usize) -> Vec<String> {
    let chunk_chars = chunk_chars.max(80);
    let mut lines = Vec::new();
    for turn in coalesce_transcript(entries.iter().cloned()) {
        let prefix = format!(
            "[{}] {}: ",
            format_duration(turn.start_ms),
            turn.source.label()
        );
        let mut line = prefix.clone();
        for word in turn.text.split_whitespace() {
            if line.len() > prefix.len() && line.len() + 1 + word.len() > chunk_chars {
                lines.push(std::mem::replace(&mut line, prefix.clone()));
            }
            if line.len() > prefix.len() {
                line.push(' ');
            }
            line.push_str(word);
        }
        if line.len() > prefix.len() {
            lines.push(line);
        }
    }
    pack(lines, chunk_chars)
}

fn pack(lines: Vec<String>, chunk_chars: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut chunk = String::new();
    for line in lines {
        if !chunk.is_empty() && chunk.len() + 1 + line.len() > chunk_chars {
            chunks.push(std::mem::take(&mut chunk));
        }
        if !chunk.is_empty() {
            chunk.push('\n');
        }
        chunk.push_str(&line);
    }
    if !chunk.is_empty() {
        chunks.push(chunk);
    }
    chunks
}

/// Keeps at most `max` chunks, evenly spaced across the meeting.
pub(super) fn sample_chunks(chunks: Vec<String>, max: usize) -> Vec<String> {
    let total = chunks.len();
    if total <= max || max == 0 {
        return chunks;
    }
    let keep = (0..max)
        .map(|index| index * total / max)
        .collect::<BTreeSet<_>>();
    chunks
        .into_iter()
        .enumerate()
        .filter_map(|(index, chunk)| keep.contains(&index).then_some(chunk))
        .collect()
}

pub(super) struct Summary {
    pub notes: GeneratedNotes,
    pub chunks: usize,
    pub total_chunks: usize,
    pub calls: usize,
}

/// Map each chunk to partial notes, then reduce them in batches that fit
/// one chunk until one set remains. A chunk that still exceeds the context is
/// split in half once. Every call receives the time left before `deadline`.
pub(super) fn summarize_entries(
    entries: &[TranscriptEntry],
    budget: Budget,
    deadline: Instant,
    mut generate: impl FnMut(&str, &str, Duration) -> Result<GeneratedNotes, GenerationError>,
) -> Result<Summary> {
    let all_chunks = transcript_chunks(entries, budget.chunk_chars);
    if all_chunks.is_empty() {
        return Err(eyre!("the transcript has no speech to summarize"));
    }
    let total_chunks = all_chunks.len();
    let chunks = sample_chunks(all_chunks, budget.max_chunks);
    let chunk_count = chunks.len();
    let mut calls = 0;
    let mut call = |instructions: &str, prompt: &str| {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(GenerationError::Failed(
                "Apple Intelligence did not finish in time".into(),
            ));
        }
        calls += 1;
        generate(instructions, prompt, remaining.min(CALL_TIMEOUT))
    };

    let mut partials = Vec::new();
    for (index, chunk) in chunks.iter().enumerate() {
        let prompt = map_prompt(chunk, index, chunk_count);
        match call(MAP_INSTRUCTIONS, &prompt) {
            Ok(notes) => partials.push(normalize(notes)),
            Err(GenerationError::ContextWindow) => {
                let (first, second) = split_chunk(chunk);
                for half in [first, second].into_iter().filter(|half| !half.is_empty()) {
                    let notes = call(MAP_INSTRUCTIONS, &map_prompt(&half, index, chunk_count))
                        .map_err(|error| eyre!("{error}"))?;
                    partials.push(normalize(notes));
                }
            }
            Err(error) => return Err(eyre!("{error}")),
        }
    }

    while partials.len() > 1 {
        let mut merged = Vec::new();
        for batch in reduce_batches(&partials, budget.chunk_chars) {
            if batch.len() == 1 {
                merged.push(batch[0].clone());
                continue;
            }
            match call(REDUCE_INSTRUCTIONS, &reduce_prompt(batch)) {
                // The model drops tasks when merging many parts; keep every
                // partial action item instead.
                Ok(notes) => merged.push(normalize(GeneratedNotes {
                    action_items: merged_action_items(batch),
                    ..notes
                })),
                Err(GenerationError::ContextWindow) => merged.push(merge_mechanically(batch)),
                Err(error) => return Err(eyre!("{error}")),
            }
        }
        partials = merged;
    }
    Ok(Summary {
        notes: partials.pop().unwrap_or_default(),
        chunks: chunk_count,
        total_chunks,
        calls,
    })
}

fn map_prompt(chunk: &str, index: usize, count: usize) -> String {
    if count == 1 {
        format!("Meeting transcript:\n\n{chunk}")
    } else {
        format!(
            "Meeting transcript, part {} of {count}:\n\n{chunk}",
            index + 1
        )
    }
}

fn split_chunk(chunk: &str) -> (String, String) {
    let lines = chunk.lines().collect::<Vec<_>>();
    if lines.len() > 1 {
        let middle = lines.len() / 2;
        return (lines[..middle].join("\n"), lines[middle..].join("\n"));
    }
    let middle = chunk.len() / 2;
    let split = chunk[..middle]
        .rfind(char::is_whitespace)
        .or_else(|| {
            chunk
                .char_indices()
                .map(|(index, _)| index)
                .find(|i| *i >= middle)
        })
        .unwrap_or(chunk.len());
    (chunk[..split].trim().into(), chunk[split..].trim().into())
}

/// Groups consecutive partials so each batch fits one chunk, with at least
/// two per batch so every round makes progress.
fn reduce_batches(partials: &[GeneratedNotes], chunk_chars: usize) -> Vec<&[GeneratedNotes]> {
    let mut batches = Vec::new();
    let mut start = 0;
    while start < partials.len() {
        let mut end = start;
        let mut chars = 0;
        while end < partials.len() {
            let size = render_partial(&partials[end], end - start).len();
            if end - start >= 2 && chars + size > chunk_chars {
                break;
            }
            chars += size;
            end += 1;
        }
        batches.push(&partials[start..end]);
        start = end;
    }
    batches
}

fn render_partial(notes: &GeneratedNotes, index: usize) -> String {
    let mut text = format!("Part {} notes\nTitle: {}\n", index + 1, notes.title);
    let mut section = |label: &str, items: Vec<String>| {
        if !items.is_empty() {
            text.push_str(label);
            text.push_str(":\n");
            for item in items {
                text.push_str("- ");
                text.push_str(&item);
                text.push('\n');
            }
        }
    };
    section("Points", notes.summary.clone());
    section(
        "Action items",
        notes
            .action_items
            .iter()
            .map(|item| match item.owner.trim() {
                "" => item.task.clone(),
                owner => format!("{} (owner: {owner})", item.task),
            })
            .collect(),
    );
    section("Decisions", notes.decisions.clone());
    text
}

fn reduce_prompt(batch: &[GeneratedNotes]) -> String {
    let parts = batch
        .iter()
        .enumerate()
        .map(|(index, notes)| render_partial(notes, index))
        .collect::<Vec<_>>()
        .join("\n");
    format!("Notes from consecutive parts of one meeting:\n\n{parts}")
}

fn merged_action_items(batch: &[GeneratedNotes]) -> Vec<apple_intelligence::GeneratedActionItem> {
    batch
        .iter()
        .flat_map(|notes| notes.action_items.clone())
        .collect()
}

fn merge_mechanically(batch: &[GeneratedNotes]) -> GeneratedNotes {
    normalize(GeneratedNotes {
        title: batch
            .first()
            .map(|notes| notes.title.clone())
            .unwrap_or_default(),
        summary: batch
            .iter()
            .flat_map(|notes| notes.summary.clone())
            .collect(),
        action_items: merged_action_items(batch),
        decisions: batch
            .iter()
            .flat_map(|notes| notes.decisions.clone())
            .collect(),
    })
}

fn clean_item(item: &str) -> String {
    item.trim()
        .trim_start_matches(['-', '*', '•'])
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn clean_list(items: Vec<String>, max: usize) -> Vec<String> {
    let mut seen = BTreeSet::new();
    items
        .iter()
        .map(|item| clean_item(item))
        .filter(|item| !item.is_empty() && seen.insert(item.to_lowercase()))
        .take(max)
        .collect()
}

pub(super) fn clean_title(title: &str) -> String {
    let words = title
        .trim()
        .trim_matches(['"', '\'', '“', '”', '#', '*'])
        .split_whitespace()
        .take(MAX_TITLE_WORDS)
        .collect::<Vec<_>>()
        .join(" ");
    words.trim_end_matches(['.', ':', ';', ',']).to_string()
}

fn clean_owner(owner: &str) -> Option<String> {
    let owner = clean_item(owner);
    let lower = owner.to_lowercase();
    (!owner.is_empty()
        && ![
            "none",
            "n/a",
            "na",
            "unknown",
            "unspecified",
            "not stated",
            "unassigned",
            "tbd",
        ]
        .contains(&lower.as_str()))
    .then(|| if lower == "you" { "You".into() } else { owner })
}

fn normalize(notes: GeneratedNotes) -> GeneratedNotes {
    let mut seen = BTreeSet::new();
    GeneratedNotes {
        title: clean_title(&notes.title),
        summary: clean_list(notes.summary, MAX_SUMMARY),
        action_items: notes
            .action_items
            .into_iter()
            .filter_map(|item| {
                let task = clean_item(&item.task);
                (!task.is_empty() && seen.insert(task.to_lowercase())).then(|| {
                    apple_intelligence::GeneratedActionItem {
                        task,
                        owner: clean_owner(&item.owner).unwrap_or_default(),
                    }
                })
            })
            .take(MAX_ACTION_ITEMS)
            .collect(),
        decisions: clean_list(notes.decisions, MAX_DECISIONS),
    }
}

impl MeetingNotes {
    fn from_summary(summary: Summary, elapsed: Duration) -> Self {
        Self {
            title: summary.notes.title,
            summary: summary.notes.summary,
            action_items: summary
                .notes
                .action_items
                .into_iter()
                .map(|item| ActionItem {
                    owner: (!item.owner.is_empty()).then_some(item.owner),
                    task: item.task,
                })
                .collect(),
            decisions: summary.notes.decisions,
            generated_at_ms: now_ms(),
            elapsed_ms: elapsed.as_millis() as u64,
            chunks: summary.chunks,
            total_chunks: summary.total_chunks,
        }
    }
}

/// The title shown for a meeting: the generated title replaces only a
/// default app-and-time title, so a named meeting keeps its name.
pub fn display_title(manifest: &MeetingManifest) -> String {
    match &manifest.notes {
        Some(notes)
            if !notes.title.trim().is_empty()
                && manifest.title
                    == super::default_title(manifest.app.as_deref(), manifest.created_at_ms) =>
        {
            notes.title.clone()
        }
        _ => manifest.title.clone(),
    }
}

pub(super) fn render_notes_block(notes: &MeetingNotes) -> String {
    let mut block = format!("{NOTES_START}\n## Summary\n\n");
    if notes.summary.is_empty() {
        block.push_str("_Nothing to summarize._\n");
    }
    for point in &notes.summary {
        block.push_str(&format!("- {point}\n"));
    }
    block.push_str("\n## Action items\n\n");
    if notes.action_items.is_empty() {
        block.push_str("_None noted._\n");
    }
    for item in &notes.action_items {
        match &item.owner {
            Some(owner) => block.push_str(&format!("- [ ] {} — **{owner}**\n", item.task)),
            None => block.push_str(&format!("- [ ] {}\n", item.task)),
        }
    }
    block.push_str("\n## Decisions\n\n");
    if notes.decisions.is_empty() {
        block.push_str("_None noted._\n");
    }
    for decision in &notes.decisions {
        block.push_str(&format!("- {decision}\n"));
    }
    if notes.chunks < notes.total_chunks {
        block.push_str(&format!(
            "\n_Long meeting: notes cover {} of {} transcript sections._\n",
            notes.chunks, notes.total_chunks
        ));
    }
    block.push_str("\n_Notes written on this Mac by Apple Intelligence._\n");
    block.push_str(NOTES_END);
    block.push('\n');
    block
}

/// Rewrites only HEX's parts of the Markdown: the heading when it still
/// reads `previous_title`, and the notes block between its markers (inserted
/// above `## Transcript` the first time). Everything else is preserved.
pub(super) fn apply_notes(
    markdown: &str,
    previous_title: &str,
    title: &str,
    block: &str,
) -> String {
    let mut output = String::with_capacity(markdown.len() + block.len());
    let mut lines = markdown.split_inclusive('\n').peekable();
    let mut heading_seen = false;
    let mut inserted = false;
    let has_markers = markdown.contains(NOTES_START) && markdown.contains(NOTES_END);
    while let Some(line) = lines.next() {
        let bare = line.trim_end_matches(['\n', '\r']);
        if !heading_seen && bare.starts_with("# ") {
            heading_seen = true;
            if bare[2..].trim() == previous_title.trim() {
                output.push_str(&format!("# {}\n", title.trim()));
                continue;
            }
        }
        if !inserted && has_markers && bare == NOTES_START {
            for skipped in lines.by_ref() {
                if skipped.trim_end_matches(['\n', '\r']) == NOTES_END {
                    break;
                }
            }
            output.push_str(block);
            inserted = true;
            continue;
        }
        if !inserted && !has_markers && bare == "## Transcript" {
            output.push_str(block);
            output.push('\n');
            inserted = true;
        }
        output.push_str(line);
    }
    if !inserted {
        if !output.is_empty() && !output.ends_with("\n\n") {
            output.push_str(if output.ends_with('\n') { "\n" } else { "\n\n" });
        }
        output.push_str(block);
    }
    output
}

/// Applies the notes to the exported file in place, keeping its name. The
/// file is replaced atomically, and only if nothing changed it meanwhile.
fn update_markdown_file(path: &Path, previous_title: &str, title: &str, block: &str) -> Result<()> {
    let original =
        fs::read_to_string(path).wrap_err_with(|| format!("could not read {}", path.display()))?;
    let updated = apply_notes(&original, previous_title, title, block);
    if updated == original {
        return Ok(());
    }
    let folder = path
        .parent()
        .ok_or_else(|| eyre!("{} has no folder", path.display()))?;
    let name = path
        .file_name()
        .ok_or_else(|| eyre!("{} has no name", path.display()))?
        .to_string_lossy();
    let temporary = folder.join(format!(".{name}.notes.tmp"));
    fs::write(&temporary, &updated)?;
    if let Ok(metadata) = fs::metadata(path) {
        let _ = fs::set_permissions(&temporary, metadata.permissions());
    }
    File::open(&temporary)?.sync_all()?;
    if fs::read_to_string(path).ok().as_deref() != Some(original.as_str()) {
        let _ = fs::remove_file(&temporary);
        return Err(eyre!("the Markdown changed while notes were being written"));
    }
    fs::rename(&temporary, path)?;
    sync_directory(folder)
}

static IN_PROGRESS: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());

struct InProgress(String);

impl InProgress {
    fn claim(id: &str) -> Option<Self> {
        IN_PROGRESS
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .insert(id.to_string())
            .then(|| Self(id.to_string()))
    }
}

impl Drop for InProgress {
    fn drop(&mut self) {
        IN_PROGRESS
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .remove(&self.0);
    }
}

pub fn notes_in_progress(id: &str) -> bool {
    IN_PROGRESS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .contains(id)
}

/// Whether finished meetings should be summarized automatically.
pub fn notes_wanted() -> bool {
    crate::app_settings::meeting_notes_enabled()
        && apple_intelligence::availability().is_available()
}

fn wait_for_dictation() {
    let started = Instant::now();
    while crate::parakeet::dictation_busy() && started.elapsed() < DICTATION_WAIT {
        thread::sleep(Duration::from_millis(100));
    }
}

/// Generates notes for one completed meeting with the real model and records
/// them. Blocks; call from a background thread. Failures are recorded in
/// `notes_error` and leave the Markdown untouched.
#[cfg(debug_assertions)]
pub fn summarize(id: &str) -> Result<MeetingManifest> {
    let claim = InProgress::claim(id).ok_or_else(|| eyre!("notes are already being written"))?;
    summarize_claimed(id, claim)
}

/// Generates and records notes while holding this meeting's claim.
fn summarize_claimed(id: &str, _claim: InProgress) -> Result<MeetingManifest> {
    let availability = apple_intelligence::availability();
    if !availability.is_available() {
        return Err(eyre!("{}", availability.description()));
    }
    let directory = root()?.join(id);
    let manifest = read_manifest(&directory)?;
    if manifest.status != MeetingStatus::Complete {
        return Err(eyre!("only completed meetings can be summarized"));
    }
    let entries = super::completed_transcript(id, Some(TranscriptPublication::Final))?.entries;
    let started = Instant::now();
    let result = summarize_entries(
        &entries,
        Budget::for_context(apple_intelligence::context_tokens()),
        started + TOTAL_DEADLINE,
        |instructions, prompt, timeout| {
            wait_for_dictation();
            apple_intelligence::generate_notes(instructions, prompt, timeout)
        },
    );
    // Re-read so concurrent manifest updates are not lost.
    let mut manifest = read_manifest(&directory)?;
    match result {
        Ok(summary) => {
            tracing::info!(
                meeting = id,
                chunks = summary.chunks,
                calls = summary.calls,
                elapsed_ms = started.elapsed().as_millis() as u64,
                "generated meeting notes"
            );
            let previous_title = display_title(&manifest);
            manifest.notes = Some(MeetingNotes::from_summary(summary, started.elapsed()));
            manifest.notes_error = None;
            let title = display_title(&manifest);
            if let Some(path) = manifest.export_path.clone().filter(|path| path.exists()) {
                let block = render_notes_block(manifest.notes.as_ref().expect("notes were set"));
                if let Err(error) = update_markdown_file(&path, &previous_title, &title, &block) {
                    tracing::warn!(%error, meeting = id, "could not add notes to meeting Markdown");
                    manifest.notes_error = Some(format!("Notes not added to Markdown: {error}"));
                }
            }
            write_manifest(&directory, &manifest)?;
            Ok(manifest)
        }
        Err(error) => {
            tracing::warn!(%error, meeting = id, "could not generate meeting notes");
            manifest.notes_error = Some(error.to_string());
            write_manifest(&directory, &manifest)?;
            Err(error)
        }
    }
}

/// Parses the turns of an exported meeting Markdown file (`**You** · 01:02`
/// followed by the turn's text).
#[cfg(any(debug_assertions, test))]
pub(super) fn entries_from_markdown(markdown: &str) -> Vec<TranscriptEntry> {
    let mut entries: Vec<TranscriptEntry> = Vec::new();
    let mut in_turn = false;
    for line in markdown.lines() {
        let header = line
            .strip_prefix("**")
            .and_then(|rest| rest.split_once("** · "))
            .and_then(|(label, clock)| {
                let source = match label {
                    "You" => super::MeetingSource::Microphone,
                    "Them" => super::MeetingSource::System,
                    _ => return None,
                };
                let (minutes, seconds) = clock.trim().split_once(':')?;
                let start_ms =
                    (minutes.parse::<u64>().ok()? * 60 + seconds.parse::<u64>().ok()?) * 1_000;
                Some((source, start_ms))
            });
        if let Some((source, start_ms)) = header {
            entries.push(TranscriptEntry {
                source,
                start_ms,
                end_ms: start_ms,
                text: String::new(),
            });
            in_turn = true;
        } else if line.starts_with('#') || line.starts_with(NOTES_START) {
            in_turn = false;
        } else if in_turn
            && !line.trim().is_empty()
            && let Some(entry) = entries.last_mut()
        {
            if !entry.text.is_empty() {
                entry.text.push(' ');
            }
            entry.text.push_str(line.trim());
        }
    }
    entries.retain(|entry| !entry.text.is_empty());
    entries
}

/// Runs the real model on a transcript file without recording anything.
/// Debug CLI: `hex meeting summarize <path>`.
#[cfg(debug_assertions)]
pub fn summarize_file(path: &Path, chunk_chars: Option<usize>) -> Result<String> {
    let availability = apple_intelligence::availability();
    if !availability.is_available() {
        return Err(eyre!("{}", availability.description()));
    }
    let text =
        fs::read_to_string(path).wrap_err_with(|| format!("could not read {}", path.display()))?;
    let entries = entries_from_markdown(&text);
    let context = apple_intelligence::context_tokens();
    let mut budget = Budget::for_context(context);
    if let Some(chunk_chars) = chunk_chars {
        budget.chunk_chars = chunk_chars;
    }
    let started = Instant::now();
    let mut timings = Vec::new();
    let summary = summarize_entries(
        &entries,
        budget,
        started + TOTAL_DEADLINE,
        |instructions, prompt, timeout| {
            let call = Instant::now();
            let result = apple_intelligence::generate_notes(instructions, prompt, timeout);
            timings.push(format!(
                "{} chars {:.1}s",
                prompt.len(),
                call.elapsed().as_secs_f32()
            ));
            result
        },
    )?;
    let elapsed = started.elapsed();
    let chars = entries.iter().map(|entry| entry.text.len()).sum::<usize>();
    let notes = MeetingNotes::from_summary(summary, elapsed);
    Ok(format!(
        "# {}\n\n{}\ncontext {context} tokens · transcript {} turns, {chars} chars · chunk budget {} chars · {} of {} chunks · calls: {}\ntotal {:.1}s\n",
        notes.title,
        render_notes_block(&notes),
        entries.len(),
        budget.chunk_chars,
        notes.chunks,
        notes.total_chunks,
        timings.join(", "),
        elapsed.as_secs_f32()
    ))
}

/// Summarizes on a utility-QoS thread and reports the outcome. Returns false
/// when notes for this meeting are already being written.
pub fn spawn_summary(
    id: String,
    done: impl FnOnce(Result<MeetingManifest>) + Send + 'static,
) -> bool {
    let Some(claim) = InProgress::claim(&id) else {
        return false;
    };
    thread::Builder::new()
        .name("meeting-notes".into())
        .spawn(move || {
            crate::parakeet::deprioritize_background_thread();
            done(summarize_claimed(&id, claim));
        })
        .map_err(|error| tracing::error!(%error, "could not start meeting notes"))
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apple_intelligence::GeneratedActionItem;
    use crate::meeting::MeetingSource;

    fn entry(source: MeetingSource, start_ms: u64, text: &str) -> TranscriptEntry {
        TranscriptEntry {
            source,
            start_ms,
            end_ms: start_ms + 1_000,
            text: text.into(),
        }
    }

    fn conversation(turns: usize) -> Vec<TranscriptEntry> {
        (0..turns)
            .map(|index| {
                entry(
                    if index % 2 == 0 {
                        MeetingSource::Microphone
                    } else {
                        MeetingSource::System
                    },
                    index as u64 * 10_000,
                    &format!("Turn {index} talks about the launch plan and the docs."),
                )
            })
            .collect()
    }

    fn notes(title: &str, points: &[&str]) -> GeneratedNotes {
        GeneratedNotes {
            title: title.into(),
            summary: points.iter().map(|point| point.to_string()).collect(),
            action_items: vec![GeneratedActionItem {
                task: format!("Follow up on {title}"),
                owner: "none".into(),
            }],
            decisions: Vec::new(),
        }
    }

    #[test]
    fn budget_scales_with_the_context_window() {
        assert_eq!(Budget::for_context(4_096).chunk_chars, 2_296 * 3);
        assert_eq!(Budget::for_context(8_192).chunk_chars, 6_392 * 3);
        assert_eq!(Budget::for_context(0), Budget::for_context(4_096));
        assert_eq!(Budget::for_context(1_000).chunk_chars, 3_000);
    }

    #[test]
    fn chunks_pack_labeled_turns_and_split_long_turns() {
        let entries = vec![
            entry(MeetingSource::Microphone, 0, "Hello there."),
            entry(MeetingSource::System, 5_000, &"word ".repeat(60)),
        ];
        let chunks = transcript_chunks(&entries, 120);
        assert_eq!(chunks[0], "[00:00] You: Hello there.");
        assert!(chunks.iter().all(|chunk| chunk.len() <= 120), "{chunks:?}");
        assert!(
            chunks[1..]
                .iter()
                .all(|chunk| chunk.starts_with("[00:05] Them: word"))
        );
        let words = chunks
            .iter()
            .flat_map(|chunk| chunk.split_whitespace())
            .filter(|word| *word == "word")
            .count();
        assert_eq!(words, 60);
    }

    #[test]
    fn long_meetings_are_sampled_evenly_to_the_chunk_limit() {
        let chunks = (0..30).map(|index| index.to_string()).collect::<Vec<_>>();
        let sampled = sample_chunks(chunks, 12);
        assert_eq!(sampled.len(), 12);
        assert_eq!(sampled.first().map(String::as_str), Some("0"));
        assert_eq!(sampled.last().map(String::as_str), Some("27"));
        assert_eq!(sample_chunks(vec!["a".into()], 12), ["a"]);
    }

    #[test]
    fn a_short_transcript_needs_one_call() {
        let mut prompts = Vec::new();
        let summary = summarize_entries(
            &conversation(4),
            Budget::for_context(8_192),
            Instant::now() + Duration::from_secs(60),
            |instructions, prompt, _| {
                prompts.push((instructions.to_string(), prompt.to_string()));
                Ok(notes("\"Launch plan review.\"", &["- Plan", "plan", ""]))
            },
        )
        .unwrap();
        assert_eq!(summary.calls, 1);
        assert_eq!(prompts[0].0, MAP_INSTRUCTIONS);
        assert!(
            prompts[0]
                .1
                .starts_with("Meeting transcript:\n\n[00:00] You: Turn 0")
        );
        assert_eq!(summary.notes.title, "Launch plan review");
        assert_eq!(summary.notes.summary, ["Plan"]);
        assert_eq!(summary.notes.action_items[0].owner, "");
    }

    #[test]
    fn chunks_map_then_reduce_into_one_set_of_notes() {
        let mut maps = 0;
        let mut reduce_prompts = Vec::new();
        let summary = summarize_entries(
            &conversation(40),
            Budget {
                chunk_chars: 700,
                max_chunks: 12,
            },
            Instant::now() + Duration::from_secs(60),
            |instructions, prompt, _| {
                if instructions == MAP_INSTRUCTIONS {
                    maps += 1;
                    assert!(prompt.contains(" of "), "{prompt}");
                    Ok(notes(&format!("Part {maps}"), &[&format!("Point {maps}")]))
                } else {
                    reduce_prompts.push(prompt.to_string());
                    Ok(notes("Launch plan", &["Merged"]))
                }
            },
        )
        .unwrap();
        assert!(summary.total_chunks > 2);
        assert_eq!(maps, summary.chunks);
        assert!(!reduce_prompts.is_empty());
        assert!(reduce_prompts[0].contains("Part 1 notes\nTitle: Part 1\nPoints:\n- Point 1"));
        assert!(reduce_prompts.iter().all(|prompt| prompt.len() < 2_000));
        assert_eq!(summary.notes.title, "Launch plan");
        assert_eq!(summary.calls, maps + reduce_prompts.len());
        let tasks = summary
            .notes
            .action_items
            .iter()
            .map(|item| item.task.as_str())
            .collect::<Vec<_>>();
        assert_eq!(tasks.len(), maps.min(MAX_ACTION_ITEMS));
        assert_eq!(tasks[0], "Follow up on Part 1");
    }

    #[test]
    fn an_oversized_chunk_is_split_once() {
        let mut calls = 0;
        let summary = summarize_entries(
            &conversation(6),
            Budget::for_context(8_192),
            Instant::now() + Duration::from_secs(60),
            |instructions, _, _| {
                calls += 1;
                match (calls, instructions == MAP_INSTRUCTIONS) {
                    (1, true) => Err(GenerationError::ContextWindow),
                    (_, true) => Ok(notes(&format!("Half {calls}"), &[&format!("P{calls}")])),
                    (_, false) => Ok(notes("Merged", &["All"])),
                }
            },
        )
        .unwrap();
        assert_eq!(calls, 4);
        assert_eq!(summary.notes.title, "Merged");
    }

    #[test]
    fn failures_and_the_deadline_stop_generation() {
        let error = summarize_entries(
            &conversation(4),
            Budget::for_context(8_192),
            Instant::now() + Duration::from_secs(60),
            |_, _, _| Err(GenerationError::Failed("model busy".into())),
        )
        .err()
        .unwrap();
        assert_eq!(error.to_string(), "model busy");

        let mut called = false;
        let error = summarize_entries(
            &conversation(4),
            Budget::for_context(8_192),
            Instant::now(),
            |_, _, _| {
                called = true;
                Ok(GeneratedNotes::default())
            },
        )
        .err()
        .unwrap();
        assert!(!called);
        assert!(error.to_string().contains("in time"));
        assert!(
            summarize_entries(&[], Budget::for_context(0), Instant::now(), |_, _, _| {
                unreachable!()
            })
            .is_err()
        );
    }

    #[test]
    fn titles_and_owners_are_normalized() {
        assert_eq!(
            clean_title("“One two three four five six seven eight nine ten.”"),
            "One two three four five six seven eight"
        );
        assert_eq!(clean_owner(" you "), Some("You".into()));
        assert_eq!(clean_owner("Not stated"), None);
        assert_eq!(clean_owner("Dana"), Some("Dana".into()));
    }

    fn meeting_notes() -> MeetingNotes {
        MeetingNotes {
            title: "Launch plan review".into(),
            summary: vec!["Ship v2 on Friday".into()],
            action_items: vec![
                ActionItem {
                    task: "Write release notes".into(),
                    owner: Some("You".into()),
                },
                ActionItem {
                    task: "Update docs".into(),
                    owner: None,
                },
            ],
            decisions: Vec::new(),
            chunks: 1,
            total_chunks: 1,
            ..Default::default()
        }
    }

    const EXPORT: &str = "# Zoom call — 2:30 PM\n\n- **Date:** Thursday, October 15, 2026, 2:30 PM\n\n## Transcript\n\n**You** · 00:01\nHi.\n\n**Them** · 00:04\nHello.\n\n";

    #[test]
    fn notes_are_inserted_above_the_transcript_and_regenerate_idempotently() {
        let block = render_notes_block(&meeting_notes());
        assert!(block.contains("## Summary\n\n- Ship v2 on Friday\n"));
        assert!(block.contains("- [ ] Write release notes — **You**\n- [ ] Update docs\n"));
        assert!(block.contains("## Decisions\n\n_None noted._\n"));

        let first = apply_notes(EXPORT, "Zoom call — 2:30 PM", "Launch plan review", &block);
        assert!(first.starts_with("# Launch plan review\n\n- **Date:**"));
        let notes_at = first.find(NOTES_START).unwrap();
        assert!(notes_at < first.find("## Transcript").unwrap());
        assert!(first.ends_with("**Them** · 00:04\nHello.\n\n"));

        let again = apply_notes(&first, "Launch plan review", "Launch plan review", &block);
        assert_eq!(again, first);

        let mut changed = meeting_notes();
        changed.title = "Release sync".into();
        changed.summary = vec!["Ship v2 next week".into()];
        let regenerated = apply_notes(
            &first,
            "Launch plan review",
            "Release sync",
            &render_notes_block(&changed),
        );
        assert!(regenerated.starts_with("# Release sync\n"));
        assert!(regenerated.contains("- Ship v2 next week\n"));
        assert!(!regenerated.contains("Ship v2 on Friday"));
        assert_eq!(regenerated.matches(NOTES_START).count(), 1);
        assert_eq!(regenerated.matches("## Transcript").count(), 1);
    }

    #[test]
    fn user_edits_outside_the_notes_block_are_preserved() {
        let block = render_notes_block(&meeting_notes());
        let edited = EXPORT
            .replace("# Zoom call — 2:30 PM", "# My own title")
            .replace("Hello.", "Hello, edited.");
        let updated = apply_notes(&edited, "Zoom call — 2:30 PM", "Launch plan review", &block);
        assert!(updated.starts_with("# My own title\n"));
        assert!(updated.contains("Hello, edited."));
        assert!(updated.contains(NOTES_START));

        let without_transcript = "# Notes\n\nSome text";
        let appended = apply_notes(without_transcript, "Other", "Title", &block);
        assert!(appended.starts_with("# Notes\n\nSome text\n\n<!-- hex:notes -->"));
    }

    #[test]
    fn notes_title_replaces_only_default_titles() {
        let mut manifest: MeetingManifest = serde_json::from_value(serde_json::json!({
            "schema_version": 1,
            "id": "1",
            "title": "placeholder",
            "status": "complete",
            "created_at_ms": 1_760_000_000_000_u64,
            "ended_at_ms": null,
            "duration_ms": 1_000,
            "system": {"file": "system.wav", "samples": 0, "dropped_packets": 0},
            "microphone": {"file": "microphone.wav", "samples": 0, "dropped_packets": 0},
            "error": null
        }))
        .unwrap();
        assert!(manifest.notes.is_none());
        manifest.app = Some("Zoom".into());
        manifest.title = super::super::default_title(Some("Zoom"), manifest.created_at_ms);
        assert_eq!(display_title(&manifest), manifest.title);
        manifest.notes = Some(meeting_notes());
        assert_eq!(display_title(&manifest), "Launch plan review");
        manifest.title = "Design sync".into();
        assert_eq!(display_title(&manifest), "Design sync");
    }

    #[test]
    fn exported_markdown_parses_back_into_turns() {
        let block = render_notes_block(&meeting_notes());
        let markdown = apply_notes(EXPORT, "Zoom call — 2:30 PM", "Launch", &block);
        let entries = entries_from_markdown(&markdown);
        assert_eq!(
            entries,
            [
                TranscriptEntry {
                    source: MeetingSource::Microphone,
                    start_ms: 1_000,
                    end_ms: 1_000,
                    text: "Hi.".into()
                },
                TranscriptEntry {
                    source: MeetingSource::System,
                    start_ms: 4_000,
                    end_ms: 4_000,
                    text: "Hello.".into()
                },
            ]
        );
    }

    #[test]
    fn notes_files_are_rewritten_in_place_only_when_unchanged() {
        let folder = std::env::temp_dir().join(format!(
            "hex-meeting-notes-{}-{}",
            std::process::id(),
            now_ms()
        ));
        fs::create_dir_all(&folder).unwrap();
        let path = folder.join("2026-10-15 1430 Zoom call — 2-30 PM.md");
        fs::write(&path, EXPORT).unwrap();
        let block = render_notes_block(&meeting_notes());
        update_markdown_file(&path, "Zoom call — 2:30 PM", "Launch plan review", &block).unwrap();
        let written = fs::read_to_string(&path).unwrap();
        assert!(written.starts_with("# Launch plan review\n"));
        assert!(written.contains(NOTES_START));
        assert_eq!(fs::read_dir(&folder).unwrap().count(), 1);
        fs::remove_dir_all(folder).unwrap();
    }
}
