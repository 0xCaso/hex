import AppKit
import SwiftUI

public struct HexHistoryPane: View {
    public var store: HexSettingsStore

    public init(store: HexSettingsStore) {
        self.store = store
    }

    public var body: some View {
        let retention = store.runtime.snapshot?.settings.historyRetention ?? "week"

        VStack(spacing: 0) {
            HStack(spacing: 12) {
                TextField(
                    "Search retained history…",
                    text: Binding(
                        get: { store.historySearchQuery },
                        set: { query in
                            store.historySearchQuery = query
                            store.refreshHistory()
                        }
                    )
                )
                .textFieldStyle(.roundedBorder)
                .frame(maxWidth: 260)

                Spacer()

                if let copied = store.copiedFeedbackLabel {
                    Text(copied)
                        .font(.caption)
                        .foregroundStyle(.green)
                }

                Picker(
                    "Keep History",
                    selection: Binding(
                        get: { retention },
                        set: { store.runtime.dispatch(.setHistoryRetention(retention: $0)) }
                    )
                ) {
                    Text("Off").tag("off")
                    Text("24 hours").tag("day")
                    Text("7 days").tag("week")
                    Text("30 days").tag("month")
                    Text("Forever").tag("forever")
                }
                .frame(width: 175)

                if !store.historyEntries.isEmpty {
                    if store.confirmingClearHistory {
                        Button("Cancel") {
                            store.confirmingClearHistory = false
                        }
                        .controlSize(.small)
                        Button("Confirm Clear All", role: .destructive) {
                            store.confirmClearAllHistory()
                        }
                        .buttonStyle(.borderedProminent)
                        .tint(.red)
                        .controlSize(.small)
                    } else {
                        Button("Clear History…", role: .destructive) {
                            store.confirmingClearHistory = true
                        }
                        .controlSize(.small)
                    }
                }
            }
            .padding(.horizontal, 14)
            .padding(.vertical, 10)

            Divider()

            if retention == "off" && store.historyEntries.isEmpty {
                ContentUnavailableView(
                    "History Retention is Off",
                    systemImage: "clock.badge.xmark",
                    description: Text("New dictations are not saved while retention is Off.")
                )
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else if store.historyEntries.isEmpty {
                ContentUnavailableView(
                    store.historySearchQuery.isEmpty ? "No Dictation History Yet" : "No Matching Entries",
                    systemImage: "clock.arrow.circlepath",
                    description: Text(
                        store.historySearchQuery.isEmpty
                            ? "Completed dictations and Voice Actions appear here."
                            : "Try a different search query."
                    )
                )
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                HSplitView {
                    List(store.historyEntries, selection: Bindable(store).selectedHistoryEntryId) { entry in
                        VStack(alignment: .leading, spacing: 3) {
                            Text(entry.finalText)
                                .font(.body)
                                .lineLimit(2)
                            HStack(spacing: 6) {
                                Text(kindLabel(entry.kind))
                                    .font(.caption2.weight(.medium))
                                    .foregroundStyle(.secondary)
                                if let app = entry.application {
                                    Text("· \(app)")
                                        .font(.caption2)
                                        .foregroundStyle(.secondary)
                                }
                                Spacer()
                                Text(Self.formatTimestamp(entry.timestampMs))
                                    .font(.caption2.monospacedDigit())
                                    .foregroundStyle(.tertiary)
                            }
                        }
                        .padding(.vertical, 2)
                        .tag(entry.id)
                    }
                    .listStyle(.sidebar)
                    .frame(
                        minWidth: 220,
                        idealWidth: HexTuning.StatusWindow.listColumnWidth,
                        maxWidth: 300,
                        maxHeight: .infinity
                    )

                    if let entry = store.selectedHistoryEntry {
                        historyDetailView(entry)
                            .frame(maxWidth: .infinity, maxHeight: .infinity)
                    }
                }
            }
        }
    }

    @ViewBuilder
    private func historyDetailView(_ entry: HexHistoryEntry) -> some View {
        Form {
            Section("Inserted Text") {
                Text(entry.finalText)
                    .textSelection(.enabled)
                    .font(.body)
                    .padding(.vertical, 4)
                HStack {
                    Button {
                        store.copyToPasteboard(entry.finalText, feedback: "Copied text")
                    } label: {
                        Label("Copy Text", systemImage: "doc.on.doc")
                    }
                    .controlSize(.small)

                    Spacer()

                    Button(role: .destructive) {
                        store.deleteHistoryEntry(id: entry.id)
                    } label: {
                        Label("Delete Entry", systemImage: "trash")
                    }
                    .controlSize(.small)
                }
            }

            if entry.rawText != entry.finalText && !entry.rawText.isEmpty {
                Section("Transcript Before Rewrite") {
                    Text(entry.rawText)
                        .textSelection(.enabled)
                        .font(.callout)
                        .foregroundStyle(.secondary)
                    Button {
                        store.copyToPasteboard(entry.rawText, feedback: "Copied raw transcript")
                    } label: {
                        Label("Copy Raw Transcript", systemImage: "doc.on.doc")
                    }
                    .controlSize(.small)
                }
            }

            Section("Details") {
                LabeledContent("Type", value: kindLabel(entry.kind))
                LabeledContent("Application", value: entry.application ?? "Unknown")
                if let proc = entry.processing {
                    LabeledContent("Mode / Profile", value: "\(proc.profile) (\(proc.latencyMs) ms)")
                    if let fallback = proc.fallback {
                        LabeledContent("Rewrite Fallback", value: fallback)
                    }
                }
                LabeledContent("Audio Duration", value: String(format: "%.1f s", Double(entry.audioMs) / 1000.0))
                LabeledContent("Inference Time", value: "\(entry.inferenceMs) ms")
                LabeledContent("Total Latency", value: "\(entry.totalMs) ms")
            }
        }
        .formStyle(.grouped)
    }

    private func kindLabel(_ kind: String) -> String {
        switch kind {
        case "voice_action": return "Voice Action"
        case "send": return "Send"
        default: return "Dictation"
        }
    }

    private static func formatTimestamp(_ ms: UInt64) -> String {
        let date = Date(timeIntervalSince1970: TimeInterval(ms) / 1000.0)
        let formatter = RelativeDateTimeFormatter()
        formatter.unitsStyle = .abbreviated
        return formatter.localizedString(for: date, relativeTo: Date())
    }
}

public struct HexCommandsPane: View {
    public var store: HexSettingsStore

    public init(store: HexSettingsStore) {
        self.store = store
    }

    public var body: some View {
        let snapshot = store.runtime.snapshot
        let cmdSnap = snapshot?.commandsSnapshot ?? HexCommandsSnapshot()
        let commandsEnabled = snapshot?.commandsEnabled ?? false

        VStack(spacing: 0) {
            VStack(alignment: .leading, spacing: 8) {
                HStack(spacing: 12) {
                    Toggle(
                        "Enable Voice Commands",
                        isOn: Binding(
                            get: { commandsEnabled },
                            set: { store.setCommandsEnabled($0) }
                        )
                    )
                    .toggleStyle(.switch)

                    statusBadge(status: cmdSnap.commandModelStatus, enabled: commandsEnabled)

                    if cmdSnap.commandModelStatus == "failed" {
                        Button("Retry Model") {
                            store.runtime.dispatch(.retryCommandsModel)
                        }
                        .controlSize(.small)
                        Button("Turn Off") {
                            store.setCommandsEnabled(false)
                        }
                        .controlSize(.small)
                    }

                    Spacer()

                    if let copied = store.copiedFeedbackLabel {
                        Text(copied)
                            .font(.caption)
                            .foregroundStyle(.green)
                    }

                    if cmdSnap.workspaceExists {
                        Button("Open hex.config.ts") {
                            store.runtime.dispatch(.openPersonalCommandsConfig)
                        }
                        .controlSize(.small)
                    } else {
                        Button("Create Custom Config") {
                            store.runtime.dispatch(.provisionPersonalCommandsWorkspace)
                        }
                        .controlSize(.small)
                    }

                    Button("Copy Agent Prompt") {
                        if store.runtime.dispatch(.copyPersonalCommandsAgentPrompt) {
                            store.copiedFeedbackLabel = "Copied agent prompt"
                        }
                    }
                    .controlSize(.small)
                }

                if let reloadError = cmdSnap.lastReloadError, !reloadError.isEmpty {
                    HStack(spacing: 6) {
                        Image(systemName: "exclamationmark.triangle.fill")
                            .foregroundStyle(.orange)
                        Text(reloadError)
                            .font(.caption)
                            .lineLimit(2)
                    }
                }

                HStack {
                    TextField("Search spoken phrases, apps, or tasks…", text: Bindable(store).commandsSearchQuery)
                        .textFieldStyle(.roundedBorder)
                        .frame(maxWidth: 320)
                    Spacer()
                    Text("\(store.filteredCommands.count) commands · Context: \(cmdSnap.currentContextLabel)")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
            .padding(.horizontal, 14)
            .padding(.vertical, 10)

            Divider()

            HSplitView {
                List(store.filteredCommands, selection: Bindable(store).selectedCommandId) { item in
                    VStack(alignment: .leading, spacing: 2) {
                        HStack {
                            Text("\"\(item.phrase)\"")
                                .font(.body.weight(.medium))
                                .lineLimit(1)
                            Spacer()
                            Text(item.scope)
                                .font(.caption2)
                                .padding(.horizontal, 5)
                                .padding(.vertical, 1)
                                .background(Capsule().fill(Color.primary.opacity(0.08)))
                        }
                        Text(item.description)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                    }
                    .padding(.vertical, 2)
                    .tag(item.id)
                }
                .listStyle(.sidebar)
                .frame(
                    minWidth: 230,
                    idealWidth: 260,
                    maxWidth: 320,
                    maxHeight: .infinity
                )

                if let selected = store.selectedCommandItem {
                    Form {
                        Section("Spoken Trigger") {
                            LabeledContent("Primary Phrase", value: "\"\(selected.phrase)\"")
                            if !selected.aliases.isEmpty {
                                LabeledContent(
                                    "Aliases",
                                    value: selected.aliases.map { "\"\($0)\"" }.joined(separator: ", ")
                                )
                            }
                        }
                        Section("Command Details") {
                            LabeledContent("Action", value: selected.description)
                            LabeledContent("Scope", value: selected.scope)
                            LabeledContent("Group", value: selected.group)
                            LabeledContent("Identifier", value: selected.id)
                        }
                        Section("Custom Commands Workspace") {
                            LabeledContent("Config Path", value: cmdSnap.configFilePath)
                        }
                    }
                    .formStyle(.grouped)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                } else {
                    ContentUnavailableView(
                        "No Matching Commands",
                        systemImage: "command",
                        description: Text("Try a different search filter.")
                    )
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                }
            }
        }
        .alert(
            "Keep Microphone Ready?",
            isPresented: Bindable(store).pendingKeepMicrophoneReadyConfirmation
        ) {
            Button("Cancel", role: .cancel) {}
            Button("Keep Microphone Ready & Enable Commands") {
                store.confirmKeepMicrophoneReadyAndEnableCommands()
            }
        } message: {
            Text("Voice Commands require keeping the microphone open while HEX is listening.")
        }
    }

    @ViewBuilder
    private func statusBadge(status: String, enabled: Bool) -> some View {
        if !enabled {
            Text("Off")
                .font(.caption)
                .foregroundStyle(.secondary)
        } else {
            switch status {
            case "ready":
                Label("Ready", systemImage: "checkmark.circle.fill")
                    .font(.caption)
                    .foregroundStyle(.green)
            case "loading":
                HStack(spacing: 4) {
                    ProgressView().controlSize(.mini)
                    Text("Preparing Moonshine…")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            case "failed":
                Label("Model Load Failed", systemImage: "exclamationmark.triangle.fill")
                    .font(.caption)
                    .foregroundStyle(.orange)
            default:
                Text("Standby")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
    }
}

public struct HexMeetingsPane: View {
    public var store: HexSettingsStore

    public init(store: HexSettingsStore) {
        self.store = store
    }

    public var body: some View {
        let snapshot = store.runtime.snapshot
        let meetingMenu = snapshot?.statusMenu.meeting

        VStack(spacing: 0) {
            HStack(spacing: 12) {
                TextField("Search meetings…", text: Bindable(store).meetingsSearchQuery)
                    .textFieldStyle(.roundedBorder)
                    .frame(maxWidth: 260)

                if let statusTitle = meetingMenu?.statusTitle {
                    Label(statusTitle, systemImage: meetingMenu?.recordingIcon == true ? "record.circle.fill" : "waveform")
                        .font(.caption.weight(.medium))
                        .foregroundStyle(meetingMenu?.recordingIcon == true ? .red : .secondary)
                }

                Spacer()

                if let copied = store.copiedFeedbackLabel {
                    Text(copied)
                        .font(.caption)
                        .foregroundStyle(.green)
                }

                if snapshot?.permissions.screenRecordingGranted == false {
                    Button("Allow Screen Audio…") {
                        store.runtime.dispatch(.requestPermission(kind: "screen_recording"))
                    }
                    .controlSize(.small)
                }

                if meetingMenu?.canStop == true {
                    Button("Stop & Transcribe") {
                        store.runtime.dispatch(.stopMeeting)
                    }
                    .buttonStyle(.borderedProminent)
                    .tint(.red)
                    .controlSize(.small)
                } else if meetingMenu?.canStart == true {
                    Button(meetingMenu?.buttonTitle ?? "Start Recording") {
                        store.runtime.dispatch(.startMeeting())
                    }
                    .buttonStyle(.borderedProminent)
                    .controlSize(.small)
                }

                Button {
                    store.runtime.dispatch(.refreshMeetings)
                    store.refreshSelectedMeetingDetail()
                } label: {
                    Image(systemName: "arrow.clockwise")
                }
                .controlSize(.small)
                .help("Refresh Meetings")
            }
            .padding(.horizontal, 14)
            .padding(.vertical, 10)

            Divider()

            if store.filteredMeetings.isEmpty {
                ContentUnavailableView(
                    store.meetingsSearchQuery.isEmpty ? "No Recorded Meetings" : "No Matching Meetings",
                    systemImage: "person.2.wave.2",
                    description: Text(
                        store.meetingsSearchQuery.isEmpty
                            ? "Start a recording above or accept the menu-bar offer when a call begins."
                            : "Try a different search query."
                    )
                )
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                HSplitView {
                    List(
                        store.filteredMeetings,
                        selection: Binding(
                            get: { store.selectedMeetingId },
                            set: { store.selectMeeting(id: $0) }
                        )
                    ) { row in
                        VStack(alignment: .leading, spacing: 3) {
                            HStack {
                                Text(row.displayTitle)
                                    .font(.body.weight(.medium))
                                    .lineLimit(1)
                                Spacer()
                                statusPill(row.status)
                            }
                            HStack {
                                Text(row.durationLabel)
                                    .font(.caption2)
                                    .foregroundStyle(.secondary)
                                if row.summarizing {
                                    Text("· Summarizing…")
                                        .font(.caption2)
                                        .foregroundStyle(.secondary)
                                }
                            }
                        }
                        .padding(.vertical, 2)
                        .tag(Optional(row.id))
                    }
                    .listStyle(.sidebar)
                    .frame(
                        minWidth: 220,
                        idealWidth: HexTuning.StatusWindow.listColumnWidth,
                        maxWidth: 300,
                        maxHeight: .infinity
                    )

                    if let detail = store.selectedMeetingDetail {
                        meetingDetailView(detail: detail, snapshot: snapshot)
                            .frame(maxWidth: .infinity, maxHeight: .infinity)
                    } else {
                        ProgressView("Loading meeting transcript…")
                            .frame(maxWidth: .infinity, maxHeight: .infinity)
                    }
                }
            }
        }
    }

    @ViewBuilder
    private func meetingDetailView(detail: HexMeetingDetailSnapshot, snapshot: HexBootstrapSnapshot?) -> some View {
        let row = detail.manifest
        let isBusy = row.status == "starting" || row.status == "recording" || row.status == "transcribing" || row.summarizing

        ScrollView {
            VStack(alignment: .leading, spacing: 14) {
                HStack(alignment: .center, spacing: 8) {
                    if store.renamingMeetingId == row.id {
                        TextField("Meeting title", text: Bindable(store).meetingRenameDraft)
                            .textFieldStyle(.roundedBorder)
                            .onSubmit {
                                store.commitMeetingRename()
                            }
                        Button("Save") {
                            store.commitMeetingRename()
                        }
                        .buttonStyle(.borderedProminent)
                        .controlSize(.small)
                        Button("Cancel") {
                            store.renamingMeetingId = nil
                        }
                        .controlSize(.small)
                    } else {
                        VStack(alignment: .leading, spacing: 2) {
                            Text(row.displayTitle)
                                .font(.title3.weight(.semibold))
                            Text("\(row.durationLabel) · Status: \(row.status.capitalized)")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                        Spacer()
                        Button("Rename") {
                            store.meetingRenameDraft = row.displayTitle
                            store.renamingMeetingId = row.id
                        }
                        .controlSize(.small)
                    }

                    if !detail.transcriptMarkdown.isEmpty {
                        Button("Copy Transcript") {
                            store.copyToPasteboard(detail.transcriptMarkdown, feedback: "Copied transcript")
                        }
                        .controlSize(.small)
                    }

                    if let exportPath = row.exportPath {
                        Button("Reveal Markdown") {
                            store.runtime.dispatch(.revealMeetingTranscript(path: exportPath))
                        }
                        .controlSize(.small)
                    }

                    if store.confirmingDeleteMeetingId == row.id {
                        Button("Cancel") {
                            store.confirmingDeleteMeetingId = nil
                        }
                        .controlSize(.small)
                        Button("Confirm Delete", role: .destructive) {
                            store.confirmDeleteMeeting(id: row.id)
                        }
                        .buttonStyle(.borderedProminent)
                        .tint(.red)
                        .controlSize(.small)
                    } else {
                        Button(role: .destructive) {
                            store.confirmingDeleteMeetingId = row.id
                        } label: {
                            Image(systemName: "trash")
                        }
                        .controlSize(.small)
                        .disabled(isBusy)
                        .help(isBusy ? "Cannot delete while recording, transcribing, or summarizing." : "Move meeting to Trash")
                    }
                }

                if let error = row.error ?? row.exportError ?? detail.transcriptError {
                    HStack(spacing: 8) {
                        Image(systemName: "exclamationmark.triangle.fill")
                            .foregroundStyle(.orange)
                        Text(error)
                            .font(.caption)
                    }
                    .padding(10)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(RoundedRectangle(cornerRadius: 8).fill(Color.orange.opacity(0.12)))
                }

                // Apple Intelligence Notes Card
                VStack(alignment: .leading, spacing: 6) {
                    HStack {
                        Label("Meeting Notes (Apple Intelligence)", systemImage: "apple.intelligence")
                            .font(.subheadline.weight(.semibold))
                        Spacer()
                        if let notesMd = row.notesMarkdown {
                            Button("Copy Notes") {
                                store.copyToPasteboard(notesMd, feedback: "Copied notes")
                            }
                            .controlSize(.small)
                        }
                        if snapshot?.appleIntelligenceAvailable == true && !row.summarizing && !detail.turns.isEmpty {
                            Button(row.notesSummary == nil ? "Summarize" : "Regenerate") {
                                store.runtime.dispatch(.summarizeMeeting(id: row.id))
                            }
                            .controlSize(.small)
                        }
                    }

                    if row.summarizing {
                        HStack(spacing: 8) {
                            ProgressView().controlSize(.small)
                            Text("Summarizing with Apple Intelligence…")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                    } else if let summary = row.notesSummary {
                        Text(summary)
                            .font(.callout)
                            .textSelection(.enabled)
                    } else {
                        Text(snapshot?.appleIntelligenceStatus ?? "No summary generated yet.")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                }
                .padding(12)
                .background(RoundedRectangle(cornerRadius: 8).fill(Color(nsColor: .controlBackgroundColor)))

                Divider()

                // Turns
                if detail.turns.isEmpty {
                    Text(row.status == "recording" ? "Listening for speech…" : "No transcript turns recorded.")
                        .font(.callout)
                        .foregroundStyle(.secondary)
                        .padding(.vertical, 20)
                } else {
                    VStack(alignment: .leading, spacing: 10) {
                        ForEach(detail.turns) { turn in
                            let isYou = (turn.source == "you")
                            VStack(alignment: isYou ? .trailing : .leading, spacing: 3) {
                                if let pauseMs = turn.pauseBeforeMs {
                                    HStack {
                                        VStack { Divider() }
                                        Text("\(pauseMs / 1000)s pause")
                                            .font(.caption2)
                                            .foregroundStyle(.tertiary)
                                        VStack { Divider() }
                                    }
                                    .padding(.vertical, 4)
                                }
                                HStack(spacing: 6) {
                                    Text(isYou ? "You" : "Them")
                                        .font(.caption.weight(.semibold))
                                        .foregroundStyle(isYou ? Color.accentColor : .secondary)
                                    Text(turn.clock)
                                        .font(.caption2.monospacedDigit())
                                        .foregroundStyle(.tertiary)
                                }
                                Text(turn.text)
                                    .font(.body)
                                    .textSelection(.enabled)
                                    .padding(.horizontal, 12)
                                    .padding(.vertical, 8)
                                    .background(
                                        RoundedRectangle(cornerRadius: 10, style: .continuous)
                                            .fill(isYou ? Color.accentColor.opacity(0.14) : Color(nsColor: .controlBackgroundColor))
                                    )
                            }
                            .frame(maxWidth: .infinity, alignment: isYou ? .trailing : .leading)
                        }
                    }
                }
            }
            .padding(16)
        }
    }

    @ViewBuilder
    private func statusPill(_ status: String) -> some View {
        let color: Color = switch status {
        case "recording", "starting": .red
        case "transcribing": .orange
        case "complete": .green
        case "failed", "interrupted": .orange
        default: .secondary
        }
        Text(status.capitalized)
            .font(.caption2.weight(.medium))
            .padding(.horizontal, 6)
            .padding(.vertical, 1)
            .background(Capsule().fill(color.opacity(0.16)))
            .foregroundStyle(color)
    }
}

public struct HexActivityPane: View {
    public var store: HexSettingsStore

    public init(store: HexSettingsStore) {
        self.store = store
    }

    public var body: some View {
        let activity = store.runtime.snapshot?.activity ?? HexActivitySnapshot()

        VStack(spacing: 0) {
            HStack {
                Label(
                    "Listener: \(activity.listenerState ?? "Idle")",
                    systemImage: "waveform.path.ecg"
                )
                .font(.subheadline.weight(.medium))
                if let device = activity.device {
                    Text("· \(device)")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer()
                Button("Refresh") {
                    store.runtime.dispatch(.refreshActivity)
                }
                .controlSize(.small)
            }
            .padding(.horizontal, 14)
            .padding(.vertical, 10)

            Divider()

            List(activity.events) { ev in
                VStack(alignment: .leading, spacing: 3) {
                    HStack {
                        Text(ev.kind)
                            .font(.caption.weight(.semibold))
                            .foregroundStyle(Color.accentColor)
                        Spacer()
                        Text("\(ev.timestampMs)")
                            .font(.caption2.monospacedDigit())
                            .foregroundStyle(.tertiary)
                    }
                    Text(ev.summary)
                        .font(.caption.monospaced())
                        .textSelection(.enabled)
                }
                .padding(.vertical, 2)
            }
        }
    }
}

public struct HexHudLabPane: View {
    public var store: HexSettingsStore

    public init(store: HexSettingsStore) {
        self.store = store
    }

    public var body: some View {
        Form {
            Section("Simulate Dictation HUD Phase") {
                HStack(spacing: 8) {
                    Button("Listening") {
                        store.runtime.dispatch(.simulateHudState(state: "listening", rms: 0.08, peak: 0.45))
                    }
                    Button("Locked") {
                        store.runtime.dispatch(.simulateHudState(state: "locked", rms: 0.08, peak: 0.45))
                    }
                    Button("Voice Action") {
                        store.runtime.dispatch(.simulateHudState(state: "voice_action", rms: 0.09, peak: 0.52))
                    }
                    Button("Transcribing") {
                        store.runtime.dispatch(.simulateHudState(state: "transcribing", rms: nil, peak: nil))
                    }
                    Button("Processing") {
                        store.runtime.dispatch(.simulateHudState(state: "processing", rms: nil, peak: nil))
                    }
                    Button("Completed") {
                        store.runtime.dispatch(.simulateHudState(state: "completed", rms: nil, peak: nil))
                    }
                    Button("Hide") {
                        store.runtime.dispatch(.simulateHudState(state: "hidden", rms: nil, peak: nil))
                    }
                }
            }

            Section("Simulate Meeting Capsule") {
                HStack(spacing: 8) {
                    Button("Offer") {
                        store.runtime.dispatch(.simulateMeetingPanel(panel: .offer(title: "Design Sync", sourceLabel: "Zoom")))
                    }
                    Button("Recording") {
                        store.runtime.dispatch(.simulateMeetingPanel(panel: .recording(title: "Design Sync", appLabel: "Zoom", elapsedText: "04:12")))
                    }
                    Button("Call Ended") {
                        store.runtime.dispatch(.simulateMeetingPanel(panel: .callEnded(appLabel: "Zoom", title: "Zoom left the call", subtitle: "HEX is still recording.")))
                    }
                    Button("Ready") {
                        store.runtime.dispatch(.simulateMeetingPanel(panel: .ready(title: "Ready · Design Sync", subtitle: "Saved in Meetings", exportPath: nil)))
                    }
                    Button("Hide") {
                        store.runtime.dispatch(.simulateMeetingPanel(panel: .hidden))
                    }
                }
            }

            Section("Live Shader Parameters") {
                hudSlider("Curvature", value: Bindable(store).hudCurvature, range: 0...1)
                hudSlider("Speed", value: Bindable(store).hudSpeed, range: 1...30)
                hudSlider("Sharpness", value: Bindable(store).hudSharpness, range: 0.05...1)
                hudSlider("Glow", value: Bindable(store).hudGlow, range: 0...1.5)
                hudSlider("Sphere Depth", value: Bindable(store).hudDepth, range: 0...1)
                hudSlider("Light Angle", value: Bindable(store).hudLightAngle, range: 0...1)
            }
        }
        .formStyle(.grouped)
    }

    @ViewBuilder
    private func hudSlider(_ label: String, value: Binding<Float>, range: ClosedRange<Float>) -> some View {
        HStack {
            Text(label)
            Spacer()
            Slider(
                value: Binding(
                    get: { Double(value.wrappedValue) },
                    set: {
                        value.wrappedValue = Float($0)
                        store.applyHudTuning()
                    }
                ),
                in: Double(range.lowerBound)...Double(range.upperBound)
            )
            .frame(width: 220)
            Text(String(format: "%.2f", value.wrappedValue))
                .font(.caption.monospacedDigit())
                .frame(width: 44, alignment: .trailing)
        }
    }
}
