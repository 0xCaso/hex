import AppKit
import Foundation
import Observation
import ServiceManagement

public enum HexWindowSection: String, CaseIterable, Identifiable, Sendable {
    case onboarding = "onboarding"
    case settings = "settings"
    case modes = "modes"
    case voiceAction = "voice-action"
    case history = "history"
    case commands = "commands"
    case meetings = "meetings"
    case activity = "activity"
    case hudLab = "hud-lab"

    public var id: String { rawValue }

    public var title: String {
        switch self {
        case .onboarding: return "Get Ready"
        case .settings: return "Settings"
        case .modes: return "Modes"
        case .voiceAction: return "Voice Action"
        case .history: return "History"
        case .commands: return "Commands"
        case .meetings: return "Meetings"
        case .activity: return "Activity"
        case .hudLab: return "HUD Lab"
        }
    }

    public var symbolName: String {
        switch self {
        case .onboarding: return "checkmark.shield"
        case .settings: return "gearshape"
        case .modes: return "slider.horizontal.3"
        case .voiceAction: return "waveform.and.magnifyingglass"
        case .history: return "clock.arrow.circlepath"
        case .commands: return "command"
        case .meetings: return "person.2.wave.2"
        case .activity: return "waveform.path.ecg"
        case .hudLab: return "sparkles"
        }
    }

    public static func visibleSections(for snapshot: HexBootstrapSnapshot?) -> [HexWindowSection] {
        var items: [HexWindowSection] = []
        if let snapshot, !snapshot.setupReady {
            items.append(.onboarding)
        }
        items.append(contentsOf: [.settings, .modes, .voiceAction, .history, .commands])
        if let snapshot, snapshot.meetingsEnabled || snapshot.developerFeaturesEnabled {
            items.append(.meetings)
        }
        if snapshot?.developerFeaturesEnabled == true {
            items.append(contentsOf: [.activity, .hudLab])
        }
        return items
    }
}

public enum HexLaunchAtLoginState: String, Sendable, Equatable {
    case enabled
    case disabled
    case requiresApproval
    case unavailable
}

@MainActor
public enum LaunchAtLoginController {
    public static func currentState() -> HexLaunchAtLoginState {
        switch SMAppService.mainApp.status {
        case .enabled:
            return .enabled
        case .notRegistered, .notFound:
            return .disabled
        case .requiresApproval:
            return .requiresApproval
        @unknown default:
            return .unavailable
        }
    }

    public static func setEnabled(_ enabled: Bool) throws -> HexLaunchAtLoginState {
        if enabled {
            try SMAppService.mainApp.register()
        } else {
            try SMAppService.mainApp.unregister()
        }
        return currentState()
    }

    public static func openSystemSettings() {
        SMAppService.openSystemSettingsLoginItems()
    }
}

public enum HexModeSelection: Hashable, Sendable {
    case global
    case custom(Int)
}

@MainActor
@Observable
public final class HexSettingsStore {
    public let runtime: HexEngineRuntime

    // Navigation & Sheets
    public var selectedSection: HexWindowSection = .settings
    public var isModelPickerPresented: Bool = false
    public var pendingReleaseMicrophoneConfirmation: Bool = false
    public var pendingKeepMicrophoneReadyConfirmation: Bool = false
    public var bannerError: String?
    public var alertError: String?

    // Shortcut capture
    public var activeShortcutTarget: String?
    public var shortcutCaptureError: String?

    // Launch at Login
    public var launchAtLoginState: HexLaunchAtLoginState = .disabled
    public var launchAtLoginError: String?

    // Modes editor state
    public var selectedMode: HexModeSelection = .global
    public var isAddingMode: Bool = false
    public var newModeNameDraft: String = ""
    public var modeValidationError: String?
    public var confirmingDeleteModeIndex: Int?
    public var isApplicationPickerOpen: Bool = false
    public var applicationSearchQuery: String = ""
    public var newBrowserDomainDraft: String = ""
    public var browserDomainError: String?
    public var isTransformationPickerOpen: Bool = false

    // History state
    public var historySearchQuery: String = ""
    public var historyEntries: [HexHistoryEntry] = []
    public var selectedHistoryEntryId: UInt64?
    public var confirmingClearHistory: Bool = false
    public var copiedFeedbackLabel: String?

    // Commands state
    public var commandsSearchQuery: String = ""
    public var selectedCommandId: String?

    // Meetings state
    public var meetingsSearchQuery: String = ""
    public var selectedMeetingId: String?
    public var selectedMeetingDetail: HexMeetingDetailSnapshot?
    public var renamingMeetingId: String?
    public var meetingRenameDraft: String = ""
    public var confirmingDeleteMeetingId: String?

    // HUD Lab state
    public var hudStyle: Float = HexTuning.HUD.lineStyle
    public var hudLineCount: Float = HexTuning.HUD.lineCount
    public var hudCurvature: Float = HexTuning.HUD.lineCurvature
    public var hudSpeed: Float = HexTuning.HUD.lineSpeed
    public var hudSharpness: Float = HexTuning.HUD.lineSharpness
    public var hudGlow: Float = HexTuning.HUD.lineGlow
    public var hudDepth: Float = HexTuning.HUD.sphereDepth
    public var hudLightAngle: Float = HexTuning.HUD.lightAngle
    public var hudOutline: Float = HexTuning.HUD.sphereOutline

    public init(runtime: HexEngineRuntime) {
        self.runtime = runtime
        self.launchAtLoginState = LaunchAtLoginController.currentState()
        if let snapshot = runtime.snapshot, !snapshot.setupReady && snapshot.instanceLockHeld {
            self.selectedSection = .onboarding
        }
        runtime.onCommandFailed = { [weak self] command, message in
            guard let self else { return }
            self.bannerError = message
            switch command {
            case .deleteMeeting, .deleteTranscriptionModel, .deleteHistoryEntry,
                 .clearHistory, .quitOtherHexInstancesAndRetry:
                self.alertError = message
            default:
                break
            }
        }
        runtime.onEventReceived = { [weak self] tag in
            guard let self else { return }
            switch tag {
            case .historyUpdated:
                self.refreshHistory()
            case .meetingTranscriptDelta:
                if self.selectedSection == .meetings {
                    self.refreshSelectedMeetingDetailAsync()
                }
            default:
                break
            }
        }
        refreshHistory()
        syncSelectionWithSnapshot()
    }

    public func dismissBannerError() {
        bannerError = nil
    }

    public func dismissAlertError() {
        alertError = nil
    }

    public func retryInstanceLock() {
        if runtime.dispatch(.retryInstanceLock) {
            bannerError = nil
        }
    }

    public func quitOtherHexAndRetry() {
        let myPid = ProcessInfo.processInfo.processIdentifier
        for app in NSRunningApplication.runningApplications(withBundleIdentifier: "com.kitlangton.hex2")
        where app.processIdentifier != myPid {
            app.terminate()
        }
        if runtime.dispatch(.quitOtherHexInstancesAndRetry) {
            bannerError = nil
        }
    }

    public func handleSnapshotUpdate(_ snapshot: HexBootstrapSnapshot) {
        if selectedSection == .onboarding && snapshot.setupReady {
            if !snapshot.onboardingCompleted {
                runtime.dispatch(.recordOnboardingCompletion)
            }
            selectedSection = .settings
        }
        syncSelectionWithSnapshot()
        if selectedSection == .history {
            refreshHistory()
        }
        if selectedSection == .meetings {
            refreshSelectedMeetingDetailIfChanged(in: snapshot)
        }
    }

    public func selectSection(_ section: HexWindowSection) {
        cancelRecordingShortcut()
        bannerError = nil
        selectedSection = section
        switch section {
        case .history:
            refreshHistory()
        case .modes:
            if runtime.snapshot?.installedApplications.isEmpty == true {
                runtime.dispatch(.discoverInstalledApplications)
            }
            if runtime.snapshot?.opencodeCatalog.state == "loading" {
                runtime.dispatch(.reloadOpenCodeCatalog)
            }
        case .voiceAction:
            if runtime.snapshot?.opencodeCatalog.state == "loading" {
                runtime.dispatch(.reloadOpenCodeCatalog)
            }
        case .meetings:
            runtime.dispatch(.refreshMeetings)
            refreshSelectedMeetingDetailAsync()
        case .activity:
            runtime.dispatch(.refreshActivity)
        default:
            break
        }
    }

    public func openDeepLink(_ destination: String?) {
        guard let destination = destination?.lowercased() else {
            if let snapshot = runtime.snapshot, !snapshot.setupReady {
                selectSection(.onboarding)
            }
            return
        }
        switch destination {
        case "models", "model":
            selectSection(.settings)
            isModelPickerPresented = true
        case "onboarding", "setup":
            selectSection(.onboarding)
        case "settings":
            selectSection(.settings)
        case "modes":
            selectSection(.modes)
        case "voice-action", "voice_action":
            selectSection(.voiceAction)
        case "history":
            selectSection(.history)
        case "commands":
            selectSection(.commands)
        case "meetings":
            selectSection(.meetings)
        case "activity":
            selectSection(.activity)
        case "hud-lab", "hud_lab", "hud":
            selectSection(.hudLab)
        default:
            selectSection(.settings)
        }
    }

    // MARK: - Shortcut Capture

    public func startRecordingShortcut(target: String) {
        shortcutCaptureError = nil
        activeShortcutTarget = target
        runtime.dispatch(.setHotkeyCaptureActive(active: true))
    }

    public func cancelRecordingShortcut() {
        if activeShortcutTarget != nil {
            activeShortcutTarget = nil
            shortcutCaptureError = nil
            runtime.dispatch(.setHotkeyCaptureActive(active: false))
        }
    }

    public func captureShortcut(target: String, key: String?, keyCode: UInt16?, modifierFlags: UInt64) {
        if runtime.dispatch(.captureHotkey(target: target, key: key, keyCode: keyCode, modifierFlags: modifierFlags)) {
            activeShortcutTarget = nil
            shortcutCaptureError = nil
        } else {
            shortcutCaptureError = runtime.lastError
        }
    }

    public func resetShortcutToDefault(target: String) {
        cancelRecordingShortcut()
        let defaultBinding: HexHotkeyBinding?
        switch target {
        case "dictation":
            defaultBinding = HexHotkeyBinding()
        case "edit":
            defaultBinding = .editDefault
        case "paste_last":
            defaultBinding = .pasteLastDefault
        default:
            return
        }
        runtime.dispatch(.setHotkeyBinding(target: target, binding: defaultBinding))
    }

    public func clearOptionalShortcut(target: String) {
        cancelRecordingShortcut()
        runtime.dispatch(.setHotkeyBinding(target: target, binding: nil))
    }

    public func setShortcutSide(target: String, side: String) {
        if !runtime.dispatch(.setHotkeySide(target: target, side: side)) {
            shortcutCaptureError = runtime.lastError
        } else {
            shortcutCaptureError = nil
        }
    }

    // MARK: - Microphone & Commands Mutual Exclusion

    public func setReleaseMicrophoneWhileIdle(_ enabled: Bool) {
        guard let snapshot = runtime.snapshot else { return }
        if enabled && snapshot.commandsEnabled {
            pendingReleaseMicrophoneConfirmation = true
            return
        }
        pendingReleaseMicrophoneConfirmation = false
        runtime.dispatch(.setReleaseMicrophoneWhileIdle(enabled: enabled, confirmDisableCommands: false))
    }

    public func confirmDisableCommandsAndReleaseMicrophone() {
        pendingReleaseMicrophoneConfirmation = false
        runtime.dispatch(.setReleaseMicrophoneWhileIdle(enabled: true, confirmDisableCommands: true))
    }

    public func setCommandsEnabled(_ enabled: Bool) {
        guard let snapshot = runtime.snapshot else { return }
        if enabled && snapshot.releaseMicrophoneWhileIdle {
            pendingKeepMicrophoneReadyConfirmation = true
            return
        }
        pendingKeepMicrophoneReadyConfirmation = false
        runtime.dispatch(.setCommandsEnabled(enabled: enabled, confirmKeepMicrophoneReady: false))
    }

    public func confirmKeepMicrophoneReadyAndEnableCommands() {
        pendingKeepMicrophoneReadyConfirmation = false
        runtime.dispatch(.setCommandsEnabled(enabled: true, confirmKeepMicrophoneReady: true))
    }

    // MARK: - Launch at Login

    public func refreshLaunchAtLogin() {
        launchAtLoginState = LaunchAtLoginController.currentState()
    }

    public func setLaunchAtLogin(_ enabled: Bool) {
        do {
            launchAtLoginError = nil
            launchAtLoginState = try LaunchAtLoginController.setEnabled(enabled)
        } catch {
            launchAtLoginState = LaunchAtLoginController.currentState()
            launchAtLoginError = error.localizedDescription
        }
    }

    // MARK: - Modes Editor

    public var currentModeValue: HexDictationMode {
        guard let settings = runtime.snapshot?.settings else {
            return HexDictationMode(name: "Global")
        }
        switch selectedMode {
        case .global:
            return settings.dictationProcessing.defaultMode
        case let .custom(idx):
            if settings.dictationProcessing.modes.indices.contains(idx) {
                return settings.dictationProcessing.modes[idx]
            }
            return settings.dictationProcessing.defaultMode
        }
    }

    public func mutateCurrentMode(_ block: (inout HexDictationMode) -> Void) {
        guard var settings = runtime.snapshot?.settings else { return }
        switch selectedMode {
        case .global:
            block(&settings.dictationProcessing.defaultMode)
            settings.dictationProcessing.defaultMode.name = "Global"
        case let .custom(idx):
            guard settings.dictationProcessing.modes.indices.contains(idx) else { return }
            block(&settings.dictationProcessing.modes[idx])
        }
        if runtime.dispatch(.saveSettings(settings: settings)) {
            runtime.refreshBootstrapSnapshot()
        }
    }

    public func addMode(named rawName: String) -> Bool {
        guard var settings = runtime.snapshot?.settings else { return false }
        let trimmed = rawName.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else {
            modeValidationError = "Enter a mode name."
            return false
        }
        if trimmed.caseInsensitiveCompare("Global") == .orderedSame {
            modeValidationError = "Global is reserved for the default mode."
            return false
        }
        if settings.dictationProcessing.modes.contains(where: { $0.name.caseInsensitiveCompare(trimmed) == .orderedSame }) {
            modeValidationError = "A mode with that name already exists."
            return false
        }
        let newMode = HexDictationMode(name: trimmed)
        settings.dictationProcessing.modes.append(newMode)
        if runtime.dispatch(.saveSettings(settings: settings)) {
            let newIndex = settings.dictationProcessing.modes.count - 1
            selectedMode = .custom(newIndex)
            newModeNameDraft = ""
            isAddingMode = false
            modeValidationError = nil
            runtime.refreshBootstrapSnapshot()
            return true
        }
        modeValidationError = runtime.lastError
        return false
    }

    public func deleteCustomMode(at index: Int) {
        guard var settings = runtime.snapshot?.settings,
              settings.dictationProcessing.modes.indices.contains(index)
        else { return }
        settings.dictationProcessing.modes.remove(at: index)
        confirmingDeleteModeIndex = nil
        selectedMode = .global
        if runtime.dispatch(.saveSettings(settings: settings)) {
            runtime.refreshBootstrapSnapshot()
        }
    }

    public func addBrowserDomainToCurrentMode(_ rawDomain: String) -> Bool {
        let trimmed = rawDomain.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        let stripped: String
        if let url = URL(string: trimmed), let host = url.host {
            stripped = host.lowercased()
        } else {
            stripped = trimmed
                .replacingOccurrences(of: "https://", with: "")
                .replacingOccurrences(of: "http://", with: "")
                .split(separator: "/")
                .first
                .map(String.init) ?? ""
        }
        guard !stripped.isEmpty, stripped.contains("."), !stripped.contains(" ") else {
            browserDomainError = "Enter a domain like github.com or mail.google.com."
            return false
        }
        browserDomainError = nil
        newBrowserDomainDraft = ""
        mutateCurrentMode { mode in
            if !mode.browserHosts.contains(stripped) {
                mode.browserHosts.append(stripped)
                mode.browserHosts.sort()
            }
        }
        return true
    }

    // MARK: - History

    public func refreshHistory() {
        let entries = runtime.fetchHistoryEntries(query: historySearchQuery, limit: 200)
        historyEntries = entries
        if let selected = selectedHistoryEntryId, entries.contains(where: { $0.id == selected }) {
            return
        }
        selectedHistoryEntryId = entries.first?.id
    }

    public var selectedHistoryEntry: HexHistoryEntry? {
        guard let id = selectedHistoryEntryId else { return historyEntries.first }
        return historyEntries.first(where: { $0.id == id }) ?? historyEntries.first
    }

    public func deleteHistoryEntry(id: UInt64) {
        if runtime.dispatch(.deleteHistoryEntry(id: id)) {
            refreshHistory()
        }
    }

    public func confirmClearAllHistory() {
        confirmingClearHistory = false
        if runtime.dispatch(.clearHistory) {
            refreshHistory()
        }
    }

    public func copyToPasteboard(_ text: String, feedback: String) {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(text, forType: .string)
        copiedFeedbackLabel = feedback
        Task { @MainActor [weak self] in
            try? await Task.sleep(for: .seconds(1.5))
            if self?.copiedFeedbackLabel == feedback {
                self?.copiedFeedbackLabel = nil
            }
        }
    }

    // MARK: - Commands

    public var filteredCommands: [HexCommandCatalogItem] {
        guard let catalog = runtime.snapshot?.commandsSnapshot.catalog else { return [] }
        let query = commandsSearchQuery.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        guard !query.isEmpty else { return catalog }
        return catalog.filter { item in
            item.phrase.lowercased().contains(query)
                || item.description.lowercased().contains(query)
                || item.group.lowercased().contains(query)
                || item.scope.lowercased().contains(query)
                || item.aliases.contains(where: { $0.lowercased().contains(query) })
        }
    }

    public var selectedCommandItem: HexCommandCatalogItem? {
        let items = filteredCommands
        if let id = selectedCommandId, let match = items.first(where: { $0.id == id }) {
            return match
        }
        return items.first
    }

    // MARK: - Meetings

    public var filteredMeetings: [HexMeetingRowSnapshot] {
        guard let meetings = runtime.snapshot?.meetings else { return [] }
        let query = meetingsSearchQuery.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        guard !query.isEmpty else { return meetings }
        return meetings.filter { item in
            item.displayTitle.lowercased().contains(query)
                || item.title.lowercased().contains(query)
                || (item.notesSummary?.lowercased().contains(query) ?? false)
        }
    }

    public func selectMeeting(id: String?) {
        selectedMeetingId = id
        renamingMeetingId = nil
        confirmingDeleteMeetingId = nil
        refreshSelectedMeetingDetailAsync()
    }

    public func refreshSelectedMeetingDetail() {
        let meetings = filteredMeetings
        if selectedMeetingId == nil || !meetings.contains(where: { $0.id == selectedMeetingId }) {
            selectedMeetingId = meetings.first?.id
        }
        guard let id = selectedMeetingId else {
            selectedMeetingDetail = nil
            return
        }
        selectedMeetingDetail = runtime.fetchMeetingDetail(meetingId: id)
    }

    public func refreshSelectedMeetingDetailAsync() {
        let meetings = filteredMeetings
        if selectedMeetingId == nil || !meetings.contains(where: { $0.id == selectedMeetingId }) {
            selectedMeetingId = meetings.first?.id
        }
        guard let id = selectedMeetingId else {
            selectedMeetingDetail = nil
            return
        }
        Task { @MainActor [weak self] in
            guard let self else { return }
            let detail = await self.runtime.fetchMeetingDetailAsync(meetingId: id)
            if self.selectedMeetingId == id {
                self.selectedMeetingDetail = detail
            }
        }
    }

    private func refreshSelectedMeetingDetailIfChanged(in snapshot: HexBootstrapSnapshot) {
        let meetings = filteredMeetings
        if selectedMeetingId == nil || !meetings.contains(where: { $0.id == selectedMeetingId }) {
            selectedMeetingId = meetings.first?.id
        }
        guard let id = selectedMeetingId,
              let row = snapshot.meetings.first(where: { $0.id == id })
        else {
            selectedMeetingDetail = nil
            return
        }
        guard var currentDetail = selectedMeetingDetail, currentDetail.manifest.id == id else {
            refreshSelectedMeetingDetailAsync()
            return
        }
        let prev = currentDetail.manifest
        let structuralChange = prev.status != row.status
            || prev.title != row.title
            || prev.displayTitle != row.displayTitle
            || prev.summarizing != row.summarizing
            || prev.notesSummary != row.notesSummary
            || prev.notesMarkdown != row.notesMarkdown
            || prev.exportPath != row.exportPath
            || prev.exportError != row.exportError
            || prev.error != row.error
        if structuralChange {
            refreshSelectedMeetingDetailAsync()
        } else if prev.durationMs != row.durationMs || prev.durationLabel != row.durationLabel {
            currentDetail.manifest = row
            selectedMeetingDetail = currentDetail
        }
    }

    public func commitMeetingRename() {
        guard let id = renamingMeetingId else { return }
        let trimmed = meetingRenameDraft.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else {
            bannerError = "Meeting title cannot be empty."
            return
        }
        if runtime.dispatch(.renameMeeting(id: id, title: trimmed)) {
            renamingMeetingId = nil
            refreshSelectedMeetingDetailAsync()
        }
    }

    public func confirmDeleteMeeting(id: String) {
        confirmingDeleteMeetingId = nil
        if runtime.dispatch(.deleteMeeting(id: id)) {
            if selectedMeetingId == id {
                selectedMeetingId = nil
            }
            refreshSelectedMeetingDetailAsync()
        }
    }

    // MARK: - HUD Lab

    public func applyHudTuning() {
        runtime.dispatch(
            .configureHudTuning(
                style: hudStyle,
                lineCount: hudLineCount,
                curvature: hudCurvature,
                speed: hudSpeed,
                sharpness: hudSharpness,
                glow: hudGlow,
                depth: hudDepth,
                lightAngle: hudLightAngle,
                outline: hudOutline
            )
        )
    }

    private func syncSelectionWithSnapshot() {
        guard let snapshot = runtime.snapshot else { return }
        if selectedCommandId == nil {
            selectedCommandId = snapshot.commandsSnapshot.catalog.first?.id
        }
        if selectedMeetingId == nil {
            selectedMeetingId = snapshot.meetings.first?.id
        }
    }
}
