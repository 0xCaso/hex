import Foundation
import HexEngineFFI

public enum HexHudPhase: UInt32, Codable, Sendable, Equatable {
    case hidden = 0
    case listening = 1
    case locked = 2
    case transcribing = 3
    case voiceAction = 4
    case completed = 5
    case cancelled = 6
    case error = 7

    public init(cPhase: hex_hud_phase_e) {
        self = HexHudPhase(rawValue: cPhase.rawValue) ?? .hidden
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.singleValueContainer()
        if let raw = try? container.decode(UInt32.self) {
            self = HexHudPhase(rawValue: raw) ?? .hidden
            return
        }
        let string = try container.decode(String.self)
        switch string {
        case "hex_hud_phase_listening", "listening":
            self = .listening
        case "hex_hud_phase_locked", "locked":
            self = .locked
        case "hex_hud_phase_transcribing", "transcribing":
            self = .transcribing
        case "hex_hud_phase_voice_action", "voice_action":
            self = .voiceAction
        case "hex_hud_phase_completed", "completed":
            self = .completed
        case "hex_hud_phase_cancelled", "cancelled":
            self = .cancelled
        case "hex_hud_phase_error", "error":
            self = .error
        default:
            self = .hidden
        }
    }
}

public struct HexHudTelemetry: Codable, Sendable, Equatable {
    public var phase: HexHudPhase
    public var rmsLevel: Float
    public var peakLevel: Float
    public var activeQueueCount: UInt32
    public var modeColorRgba: UInt32
    public var phaseStartedNs: UInt64
    public var isCommandModeAwake: Bool
    public var isVoiceAction: Bool
    public var isPostProcessing: Bool
    public var isCapturing: Bool

    public init(
        phase: HexHudPhase = .hidden,
        rmsLevel: Float = 0,
        peakLevel: Float = 0,
        activeQueueCount: UInt32 = 0,
        modeColorRgba: UInt32 = 0,
        phaseStartedNs: UInt64 = 0,
        isCommandModeAwake: Bool = false,
        isVoiceAction: Bool = false,
        isPostProcessing: Bool = false,
        isCapturing: Bool = false
    ) {
        self.phase = phase
        self.rmsLevel = rmsLevel
        self.peakLevel = peakLevel
        self.activeQueueCount = activeQueueCount
        self.modeColorRgba = modeColorRgba
        self.phaseStartedNs = phaseStartedNs
        self.isCommandModeAwake = isCommandModeAwake
        self.isVoiceAction = isVoiceAction
        self.isPostProcessing = isPostProcessing
        self.isCapturing = isCapturing
    }

    public init(cStruct: hex_hud_telemetry_s) {
        self.phase = HexHudPhase(cPhase: cStruct.phase)
        self.rmsLevel = cStruct.rms_level
        self.peakLevel = cStruct.peak_level
        self.activeQueueCount = cStruct.active_queue_count
        self.modeColorRgba = cStruct.mode_color_rgba
        self.phaseStartedNs = cStruct.phase_started_ns
        self.isCommandModeAwake = cStruct.is_command_mode_awake
        self.isVoiceAction = cStruct.is_voice_action
        self.isPostProcessing = cStruct.is_post_processing
        self.isCapturing = cStruct.is_capturing
    }

    enum CodingKeys: String, CodingKey {
        case phase
        case rmsLevel = "rms_level"
        case peakLevel = "peak_level"
        case activeQueueCount = "active_queue_count"
        case modeColorRgba = "mode_color_rgba"
        case phaseStartedNs = "phase_started_ns"
        case isCommandModeAwake = "is_command_mode_awake"
        case isVoiceAction = "is_voice_action"
        case isPostProcessing = "is_post_processing"
        case isCapturing = "is_capturing"
    }
}

public struct HexPermissionSnapshot: Codable, Sendable, Equatable {
    public var microphoneGranted: Bool
    public var microphoneState: String
    public var accessibilityGranted: Bool
    public var inputMonitoringGranted: Bool
    public var modelInstalled: Bool
    public var screenRecordingGranted: Bool

    public init(
        microphoneGranted: Bool = false,
        microphoneState: String = "needs_request",
        accessibilityGranted: Bool = false,
        inputMonitoringGranted: Bool = false,
        modelInstalled: Bool = false,
        screenRecordingGranted: Bool = false
    ) {
        self.microphoneGranted = microphoneGranted
        self.microphoneState = microphoneState
        self.accessibilityGranted = accessibilityGranted
        self.inputMonitoringGranted = inputMonitoringGranted
        self.modelInstalled = modelInstalled
        self.screenRecordingGranted = screenRecordingGranted
    }

    enum CodingKeys: String, CodingKey {
        case microphoneGranted = "microphone_granted"
        case microphoneState = "microphone_state"
        case accessibilityGranted = "accessibility_granted"
        case inputMonitoringGranted = "input_monitoring_granted"
        case modelInstalled = "model_installed"
        case screenRecordingGranted = "screen_recording_granted"
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        self.microphoneGranted = try c.decodeIfPresent(Bool.self, forKey: .microphoneGranted) ?? false
        self.microphoneState = try c.decodeIfPresent(String.self, forKey: .microphoneState)
            ?? (self.microphoneGranted ? "ready" : "needs_request")
        self.accessibilityGranted = try c.decodeIfPresent(Bool.self, forKey: .accessibilityGranted) ?? false
        self.inputMonitoringGranted = try c.decodeIfPresent(Bool.self, forKey: .inputMonitoringGranted) ?? false
        self.modelInstalled = try c.decodeIfPresent(Bool.self, forKey: .modelInstalled) ?? false
        self.screenRecordingGranted = try c.decodeIfPresent(Bool.self, forKey: .screenRecordingGranted) ?? false
    }
}

public struct HexLanguageOption: Codable, Sendable, Equatable, Identifiable {
    public var code: String
    public var name: String
    public var id: String { code }

    public init(code: String = "en", name: String = "English") {
        self.code = code
        self.name = name
    }
}

public struct HexTranscriptionModelRow: Codable, Sendable, Equatable, Identifiable {
    public var id: String
    public var name: String
    public var summary: String
    public var realtime: String
    public var sizeLabel: String
    public var quality: String
    public var qualityContext: String
    public var coverage: String
    public var languages: [String]
    public var supportsRecognitionHints: Bool
    public var installed: Bool
    public var active: Bool
    public var recommendedForLanguage: Bool
    public var recommendationLabel: String?
    public var canDelete: Bool
    public var preparing: Bool
    public var downloadedBytes: UInt64
    public var totalBytes: UInt64?
    public var progressFraction: Float?
    public var statusLabel: String

    enum CodingKeys: String, CodingKey {
        case id, name, summary, realtime, quality, coverage, languages, installed, active, preparing
        case sizeLabel = "size_label"
        case qualityContext = "quality_context"
        case supportsRecognitionHints = "supports_recognition_hints"
        case recommendedForLanguage = "recommended_for_language"
        case recommendationLabel = "recommendation_label"
        case canDelete = "can_delete"
        case downloadedBytes = "downloaded_bytes"
        case totalBytes = "total_bytes"
        case progressFraction = "progress_fraction"
        case statusLabel = "status_label"
    }
}

public struct HexTranscriptionCatalogSnapshot: Codable, Sendable, Equatable {
    public var selectedLanguage: String
    public var languages: [HexLanguageOption]
    public var models: [HexTranscriptionModelRow]
    public var preparationError: String?
    public var deletionError: String?

    public init(
        selectedLanguage: String = "en",
        languages: [HexLanguageOption] = [],
        models: [HexTranscriptionModelRow] = [],
        preparationError: String? = nil,
        deletionError: String? = nil
    ) {
        self.selectedLanguage = selectedLanguage
        self.languages = languages
        self.models = models
        self.preparationError = preparationError
        self.deletionError = deletionError
    }

    enum CodingKeys: String, CodingKey {
        case selectedLanguage = "selected_language"
        case languages, models
        case preparationError = "preparation_error"
        case deletionError = "deletion_error"
    }
}

public struct HexOpenCodeModelChoice: Codable, Sendable, Equatable, Identifiable {
    public var key: String
    public var name: String
    public var provider: String
    public var variants: [String]
    public var recommendedVariant: String?

    public var id: String { key }

    enum CodingKeys: String, CodingKey {
        case key, name, provider, variants
        case recommendedVariant = "recommended_variant"
    }
}

public struct HexOpenCodeCatalogSnapshot: Codable, Sendable, Equatable {
    public var state: String
    public var error: String?
    public var defaultKey: String?
    public var defaultName: String?
    public var defaultVariant: String?
    public var models: [HexOpenCodeModelChoice]

    public init(
        state: String = "loading",
        error: String? = nil,
        defaultKey: String? = nil,
        defaultName: String? = nil,
        defaultVariant: String? = nil,
        models: [HexOpenCodeModelChoice] = []
    ) {
        self.state = state
        self.error = error
        self.defaultKey = defaultKey
        self.defaultName = defaultName
        self.defaultVariant = defaultVariant
        self.models = models
    }

    enum CodingKeys: String, CodingKey {
        case state, error, models
        case defaultKey = "default_key"
        case defaultName = "default_name"
        case defaultVariant = "default_variant"
    }
}

public struct HexCommandCatalogItem: Codable, Sendable, Equatable, Identifiable {
    public var id: String
    public var group: String
    public var phrase: String
    public var aliases: [String]
    public var description: String
    public var scope: String
}

public struct HexTransformationCatalogItem: Codable, Sendable, Equatable, Identifiable {
    public var id: String
    public var name: String
    public var description: String
}

public struct HexCommandsSnapshot: Codable, Sendable, Equatable {
    public var commandModelStatus: String
    public var workspacePath: String
    public var configFilePath: String
    public var workspaceExists: Bool
    public var workspaceProvisioning: Bool
    public var lastReloadError: String?
    public var currentContextLabel: String
    public var catalog: [HexCommandCatalogItem]
    public var transformations: [HexTransformationCatalogItem]

    public init(
        commandModelStatus: String = "disabled",
        workspacePath: String = "",
        configFilePath: String = "",
        workspaceExists: Bool = false,
        workspaceProvisioning: Bool = false,
        lastReloadError: String? = nil,
        currentContextLabel: String = "Global",
        catalog: [HexCommandCatalogItem] = [],
        transformations: [HexTransformationCatalogItem] = []
    ) {
        self.commandModelStatus = commandModelStatus
        self.workspacePath = workspacePath
        self.configFilePath = configFilePath
        self.workspaceExists = workspaceExists
        self.workspaceProvisioning = workspaceProvisioning
        self.lastReloadError = lastReloadError
        self.currentContextLabel = currentContextLabel
        self.catalog = catalog
        self.transformations = transformations
    }

    enum CodingKeys: String, CodingKey {
        case commandModelStatus = "command_model_status"
        case workspacePath = "workspace_path"
        case configFilePath = "config_file_path"
        case workspaceExists = "workspace_exists"
        case workspaceProvisioning = "workspace_provisioning"
        case lastReloadError = "last_reload_error"
        case currentContextLabel = "current_context_label"
        case catalog, transformations
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        self.commandModelStatus = try c.decodeIfPresent(String.self, forKey: .commandModelStatus) ?? "disabled"
        self.workspacePath = try c.decodeIfPresent(String.self, forKey: .workspacePath) ?? ""
        self.configFilePath = try c.decodeIfPresent(String.self, forKey: .configFilePath) ?? ""
        self.workspaceExists = try c.decodeIfPresent(Bool.self, forKey: .workspaceExists) ?? false
        self.workspaceProvisioning = try c.decodeIfPresent(Bool.self, forKey: .workspaceProvisioning) ?? false
        self.lastReloadError = try c.decodeIfPresent(String.self, forKey: .lastReloadError)
        self.currentContextLabel = try c.decodeIfPresent(String.self, forKey: .currentContextLabel) ?? "Global"
        self.catalog = try c.decodeIfPresent([HexCommandCatalogItem].self, forKey: .catalog) ?? []
        self.transformations = try c.decodeIfPresent([HexTransformationCatalogItem].self, forKey: .transformations) ?? []
    }
}

public struct HexMeetingRowSnapshot: Codable, Sendable, Equatable, Identifiable {
    public var id: String
    public var title: String
    public var displayTitle: String
    public var status: String
    public var createdAtMs: UInt64
    public var durationMs: UInt64
    public var durationLabel: String
    public var summarizing: Bool
    public var notesTitle: String?
    public var notesSummary: String?
    public var notesMarkdown: String?
    public var exportPath: String?
    public var exportError: String?
    public var error: String?

    enum CodingKeys: String, CodingKey {
        case id, title, status, summarizing, error
        case displayTitle = "display_title"
        case createdAtMs = "created_at_ms"
        case durationMs = "duration_ms"
        case durationLabel = "duration_label"
        case notesTitle = "notes_title"
        case notesSummary = "notes_summary"
        case notesMarkdown = "notes_markdown"
        case exportPath = "export_path"
        case exportError = "export_error"
    }
}

public struct HexMeetingTurnSnapshot: Codable, Sendable, Equatable, Identifiable {
    public var source: String
    public var clock: String
    public var startMs: UInt64
    public var endMs: UInt64
    public var pauseBeforeMs: UInt64?
    public var text: String

    public var id: String { "\(source)-\(startMs)-\(endMs)" }

    enum CodingKeys: String, CodingKey {
        case source, clock, text
        case startMs = "start_ms"
        case endMs = "end_ms"
        case pauseBeforeMs = "pause_before_ms"
    }
}

public struct HexMeetingDetailSnapshot: Codable, Sendable, Equatable {
    public var manifest: HexMeetingRowSnapshot
    public var turns: [HexMeetingTurnSnapshot]
    public var transcriptMarkdown: String
    public var transcriptError: String?

    enum CodingKeys: String, CodingKey {
        case manifest, turns
        case transcriptMarkdown = "transcript_markdown"
        case transcriptError = "transcript_error"
    }
}

public struct HexActivityEventSnapshot: Codable, Sendable, Equatable, Identifiable {
    public var kind: String
    public var timestampMs: UInt64
    public var summary: String
    public var rawJson: String

    public var id: String { "\(timestampMs)-\(kind)-\(summary.prefix(24))" }

    enum CodingKeys: String, CodingKey {
        case kind, summary
        case timestampMs = "timestamp_ms"
        case rawJson = "raw_json"
    }
}

public struct HexActivitySnapshot: Codable, Sendable, Equatable {
    public var listenerState: String?
    public var device: String?
    public var error: String?
    public var events: [HexActivityEventSnapshot]

    public init(
        listenerState: String? = nil,
        device: String? = nil,
        error: String? = nil,
        events: [HexActivityEventSnapshot] = []
    ) {
        self.listenerState = listenerState
        self.device = device
        self.error = error
        self.events = events
    }

    enum CodingKeys: String, CodingKey {
        case listenerState = "listener_state"
        case device, error, events
    }
}

public struct HexInstalledApplicationSnapshot: Codable, Sendable, Equatable, Identifiable {
    public var name: String
    public var bundleId: String?
    public var path: String

    public var id: String { path }

    enum CodingKeys: String, CodingKey {
        case name, path
        case bundleId = "bundle_id"
    }
}

public struct HexHistoryProcessing: Codable, Sendable, Equatable {
    public var profile: String
    public var latencyMs: UInt64
    public var fallback: String?

    enum CodingKeys: String, CodingKey {
        case profile
        case latencyMs = "latency_ms"
        case fallback
    }
}

public struct HexHistoryEntry: Codable, Sendable, Equatable, Identifiable {
    public var id: UInt64
    public var timestampMs: UInt64
    public var kind: String
    public var rawText: String
    public var finalText: String
    public var application: String?
    public var processing: HexHistoryProcessing?
    public var audioMs: UInt64
    public var inferenceMs: UInt64
    public var totalMs: UInt64

    enum CodingKeys: String, CodingKey {
        case id, kind, application, processing
        case timestampMs = "timestamp_ms"
        case rawText = "raw_text"
        case finalText = "final_text"
        case audioMs = "audio_ms"
        case inferenceMs = "inference_ms"
        case totalMs = "total_ms"
    }
}

// MARK: - Persisted AppSettings Mirror (`settings.json` / `SaveSettings`)

public struct HexHotkeyKey: Codable, Sendable, Equatable {
    public var code: UInt16
    public var label: String

    public init(code: UInt16, label: String) {
        self.code = code
        self.label = label
    }
}

public struct HexHotkeyModifiers: Codable, Sendable, Equatable {
    public var control: String?
    public var option: String?
    public var shift: String?
    public var command: String?
    public var function: Bool

    public init(
        control: String? = nil,
        option: String? = "either",
        shift: String? = nil,
        command: String? = nil,
        function: Bool = false
    ) {
        self.control = control
        self.option = option
        self.shift = shift
        self.command = command
        self.function = function
    }
}

public struct HexHotkeyBinding: Codable, Sendable, Equatable {
    public var modifiers: HexHotkeyModifiers
    public var key: HexHotkeyKey?

    public init(modifiers: HexHotkeyModifiers = HexHotkeyModifiers(), key: HexHotkeyKey? = nil) {
        self.modifiers = modifiers
        self.key = key
    }

    public static var pasteLastDefault: HexHotkeyBinding {
        HexHotkeyBinding(
            modifiers: HexHotkeyModifiers(option: "either", shift: "either"),
            key: HexHotkeyKey(code: 9, label: "V")
        )
    }

    public static var editDefault: HexHotkeyBinding {
        HexHotkeyBinding(
            modifiers: HexHotkeyModifiers(option: "either", command: "either"),
            key: nil
        )
    }
}

public struct HexTranscriptionSelection: Codable, Sendable, Equatable {
    public var model: String
    public var language: String
    public var recognitionHints: String

    public init(
        model: String = "parakeet_unified_en",
        language: String = "en",
        recognitionHints: String = ""
    ) {
        self.model = model
        self.language = language
        self.recognitionHints = recognitionHints
    }

    enum CodingKeys: String, CodingKey {
        case model, language
        case recognitionHints = "recognition_hints"
    }
}

public struct HexTextReplacement: Codable, Sendable, Equatable {
    public var matchedPhrase: String
    public var output: String

    public init(matchedPhrase: String = "", output: String = "") {
        self.matchedPhrase = matchedPhrase
        self.output = output
    }

    enum CodingKeys: String, CodingKey {
        case matchedPhrase = "matched_phrase"
        case output
    }
}

public struct HexDictationPostProcessing: Codable, Sendable, Equatable {
    public var enabled: Bool
    public var prompt: String
    public var model: String?
    public var variant: String?
    public var deadlineSeconds: UInt64

    public init(
        enabled: Bool = false,
        prompt: String = "Rewrite the transcript into clear, natural text while preserving the speaker's intended meaning.",
        model: String? = nil,
        variant: String? = nil,
        deadlineSeconds: UInt64 = 30
    ) {
        self.enabled = enabled
        self.prompt = prompt
        self.model = model
        self.variant = variant
        self.deadlineSeconds = deadlineSeconds
    }

    enum CodingKeys: String, CodingKey {
        case enabled, prompt, model, variant
        case deadlineSeconds = "deadline_seconds"
    }
}

public struct HexDictationMode: Codable, Sendable, Equatable {
    public var name: String
    public var applications: [String]
    public var browserHosts: [String]
    public var replacements: [HexTextReplacement]
    public var transformations: [String]
    public var postProcessing: HexDictationPostProcessing

    public init(
        name: String = "Global",
        applications: [String] = [],
        browserHosts: [String] = [],
        replacements: [HexTextReplacement] = [],
        transformations: [String] = [],
        postProcessing: HexDictationPostProcessing = HexDictationPostProcessing()
    ) {
        self.name = name
        self.applications = applications
        self.browserHosts = browserHosts
        self.replacements = replacements
        self.transformations = transformations
        self.postProcessing = postProcessing
    }

    enum CodingKeys: String, CodingKey {
        case name, applications, replacements, transformations
        case browserHosts = "browser_hosts"
        case postProcessing = "post_processing"
    }
}

public struct HexDictationProcessingSettings: Codable, Sendable, Equatable {
    public var defaultMode: HexDictationMode
    public var modes: [HexDictationMode]

    public init(
        defaultMode: HexDictationMode = HexDictationMode(name: "Global"),
        modes: [HexDictationMode] = []
    ) {
        self.defaultMode = defaultMode
        self.modes = modes
    }

    enum CodingKeys: String, CodingKey {
        case defaultMode = "default_mode"
        case modes
    }
}

public struct HexVoiceActionSettings: Codable, Sendable, Equatable {
    public var enabled: Bool
    public var model: String?
    public var variant: String?
    public var deadlineSeconds: UInt64

    public init(
        enabled: Bool = false,
        model: String? = nil,
        variant: String? = nil,
        deadlineSeconds: UInt64 = 60
    ) {
        self.enabled = enabled
        self.model = model
        self.variant = variant
        self.deadlineSeconds = deadlineSeconds
    }

    enum CodingKeys: String, CodingKey {
        case enabled, model, variant
        case deadlineSeconds = "deadline_seconds"
    }
}

public struct HexMeetingSettings: Codable, Sendable, Equatable {
    public var enabled: Bool
    public var outputFolder: String?
    public var summarizeWithAppleIntelligence: Bool

    public init(
        enabled: Bool = false,
        outputFolder: String? = nil,
        summarizeWithAppleIntelligence: Bool = true
    ) {
        self.enabled = enabled
        self.outputFolder = outputFolder
        self.summarizeWithAppleIntelligence = summarizeWithAppleIntelligence
    }

    enum CodingKeys: String, CodingKey {
        case enabled
        case outputFolder = "output_folder"
        case summarizeWithAppleIntelligence = "summarize_with_apple_intelligence"
    }
}

public struct HexAppSettings: Codable, Sendable, Equatable {
    public var commandsEnabled: Bool
    public var releaseMicrophoneWhileIdle: Bool
    public var soundEffects: Bool
    public var soundEffectVolume: Float
    public var microphone: String?
    public var recordingAudioBehavior: String
    public var doubleTapLock: Bool
    public var doubleTapOnly: Bool
    public var incrementalTranscription: Bool
    public var dictationHotkey: HexHotkeyBinding
    public var editHotkey: HexHotkeyBinding
    public var pasteLastHotkey: HexHotkeyBinding?
    public var showDockIcon: Bool
    public var transcription: HexTranscriptionSelection
    public var transcriptionRecents: [HexTranscriptionSelection]
    public var dictationProcessing: HexDictationProcessingSettings
    public var voiceAction: HexVoiceActionSettings
    public var historyRetention: String
    public var meetings: HexMeetingSettings

    public init(
        commandsEnabled: Bool = false,
        releaseMicrophoneWhileIdle: Bool = false,
        soundEffects: Bool = true,
        soundEffectVolume: Float = 0.5,
        microphone: String? = nil,
        recordingAudioBehavior: String = "do_nothing",
        doubleTapLock: Bool = true,
        doubleTapOnly: Bool = false,
        incrementalTranscription: Bool = true,
        dictationHotkey: HexHotkeyBinding = HexHotkeyBinding(),
        editHotkey: HexHotkeyBinding = .editDefault,
        pasteLastHotkey: HexHotkeyBinding? = .pasteLastDefault,
        showDockIcon: Bool = true,
        transcription: HexTranscriptionSelection = HexTranscriptionSelection(),
        transcriptionRecents: [HexTranscriptionSelection] = [],
        dictationProcessing: HexDictationProcessingSettings = HexDictationProcessingSettings(),
        voiceAction: HexVoiceActionSettings = HexVoiceActionSettings(),
        historyRetention: String = "week",
        meetings: HexMeetingSettings = HexMeetingSettings()
    ) {
        self.commandsEnabled = commandsEnabled
        self.releaseMicrophoneWhileIdle = releaseMicrophoneWhileIdle
        self.soundEffects = soundEffects
        self.soundEffectVolume = soundEffectVolume
        self.microphone = microphone
        self.recordingAudioBehavior = recordingAudioBehavior
        self.doubleTapLock = doubleTapLock
        self.doubleTapOnly = doubleTapOnly
        self.incrementalTranscription = incrementalTranscription
        self.dictationHotkey = dictationHotkey
        self.editHotkey = editHotkey
        self.pasteLastHotkey = pasteLastHotkey
        self.showDockIcon = showDockIcon
        self.transcription = transcription
        self.transcriptionRecents = transcriptionRecents
        self.dictationProcessing = dictationProcessing
        self.voiceAction = voiceAction
        self.historyRetention = historyRetention
        self.meetings = meetings
    }

    enum CodingKeys: String, CodingKey {
        case commandsEnabled = "commands_enabled"
        case releaseMicrophoneWhileIdle = "release_microphone_while_idle"
        case soundEffects = "sound_effects"
        case soundEffectVolume = "sound_effect_volume"
        case microphone
        case recordingAudioBehavior = "recording_audio_behavior"
        case doubleTapLock = "double_tap_lock"
        case doubleTapOnly = "double_tap_only"
        case incrementalTranscription = "incremental_transcription"
        case dictationHotkey = "dictation_hotkey"
        case editHotkey = "edit_hotkey"
        case pasteLastHotkey = "paste_last_hotkey"
        case showDockIcon = "show_dock_icon"
        case transcription
        case transcriptionRecents = "transcription_recents"
        case dictationProcessing = "dictation_processing"
        case voiceAction = "voice_action"
        case historyRetention = "history_retention"
        case meetings
    }
}

public struct HexMenuModelItem: Codable, Sendable, Equatable, Identifiable {
    public var modelId: String
    public var title: String
    public var selected: Bool
    public var enabled: Bool

    public var id: String { modelId }

    enum CodingKeys: String, CodingKey {
        case modelId = "model_id"
        case title
        case selected
        case enabled
    }
}

public struct HexMeetingMenuSnapshot: Codable, Sendable, Equatable {
    public var state: String
    public var visible: Bool
    public var statusTitle: String?
    public var buttonTitle: String
    public var recordingIcon: Bool
    public var canStart: Bool
    public var canStop: Bool
    public var startedElapsedMs: UInt64
    public var rawTitle: String?

    enum CodingKeys: String, CodingKey {
        case state
        case visible
        case statusTitle = "status_title"
        case buttonTitle = "button_title"
        case recordingIcon = "recording_icon"
        case canStart = "can_start"
        case canStop = "can_stop"
        case startedElapsedMs = "started_elapsed_ms"
        case rawTitle = "raw_title"
    }
}

public struct HexStatusMenuSnapshot: Codable, Sendable, Equatable {
    public var models: [HexMenuModelItem]
    public var preparingModelTitle: String?
    public var modelSwitchError: String?
    public var meeting: HexMeetingMenuSnapshot
    public var updatesManaged: Bool
    public var managedUpdatesLabel: String

    enum CodingKeys: String, CodingKey {
        case models
        case preparingModelTitle = "preparing_model_title"
        case modelSwitchError = "model_switch_error"
        case meeting
        case updatesManaged = "updates_managed"
        case managedUpdatesLabel = "managed_updates_label"
    }
}

public enum HexMeetingPanelSnapshot: Codable, Sendable, Equatable {
    case hidden
    case offer(
        key: String = "offer",
        title: String,
        subtitle: String = "Record this meeting?",
        sourceLabel: String,
        remainingSeconds: UInt64 = 45
    )
    case recording(title: String, appLabel: String, elapsedText: String)
    case callEnded(appLabel: String, title: String, subtitle: String)
    case ready(
        meetingId: String = "meeting",
        title: String,
        subtitle: String,
        exportPath: String?,
        summarizing: Bool = false
    )

    private enum CodingKeys: String, CodingKey {
        case kind
        case key
        case title
        case subtitle
        case sourceLabel = "source_label"
        case remainingSeconds = "remaining_seconds"
        case appLabel = "app_label"
        case elapsedText = "elapsed_text"
        case meetingId = "meeting_id"
        case exportPath = "export_path"
        case summarizing
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let kind = try container.decode(String.self, forKey: .kind)
        switch kind {
        case "offer":
            self = .offer(
                key: try container.decodeIfPresent(String.self, forKey: .key) ?? "offer",
                title: try container.decode(String.self, forKey: .title),
                subtitle: try container.decodeIfPresent(String.self, forKey: .subtitle) ?? "",
                sourceLabel: try container.decode(String.self, forKey: .sourceLabel),
                remainingSeconds: try container.decodeIfPresent(UInt64.self, forKey: .remainingSeconds) ?? 0
            )
        case "recording":
            self = .recording(
                title: try container.decode(String.self, forKey: .title),
                appLabel: try container.decode(String.self, forKey: .appLabel),
                elapsedText: try container.decode(String.self, forKey: .elapsedText)
            )
        case "call_ended":
            self = .callEnded(
                appLabel: try container.decodeIfPresent(String.self, forKey: .appLabel) ?? "Meeting",
                title: try container.decode(String.self, forKey: .title),
                subtitle: try container.decode(String.self, forKey: .subtitle)
            )
        case "ready":
            self = .ready(
                meetingId: try container.decodeIfPresent(String.self, forKey: .meetingId) ?? "meeting",
                title: try container.decode(String.self, forKey: .title),
                subtitle: try container.decode(String.self, forKey: .subtitle),
                exportPath: try container.decodeIfPresent(String.self, forKey: .exportPath),
                summarizing: try container.decodeIfPresent(Bool.self, forKey: .summarizing) ?? false
            )
        default:
            self = .hidden
        }
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .hidden:
            try container.encode("hidden", forKey: .kind)
        case let .offer(key, title, subtitle, sourceLabel, remainingSeconds):
            try container.encode("offer", forKey: .kind)
            try container.encode(key, forKey: .key)
            try container.encode(title, forKey: .title)
            try container.encode(subtitle, forKey: .subtitle)
            try container.encode(sourceLabel, forKey: .sourceLabel)
            try container.encode(remainingSeconds, forKey: .remainingSeconds)
        case let .recording(title, appLabel, elapsedText):
            try container.encode("recording", forKey: .kind)
            try container.encode(title, forKey: .title)
            try container.encode(appLabel, forKey: .appLabel)
            try container.encode(elapsedText, forKey: .elapsedText)
        case let .callEnded(appLabel, title, subtitle):
            try container.encode("call_ended", forKey: .kind)
            try container.encode(appLabel, forKey: .appLabel)
            try container.encode(title, forKey: .title)
            try container.encode(subtitle, forKey: .subtitle)
        case let .ready(meetingId, title, subtitle, exportPath, summarizing):
            try container.encode("ready", forKey: .kind)
            try container.encode(meetingId, forKey: .meetingId)
            try container.encode(title, forKey: .title)
            try container.encode(subtitle, forKey: .subtitle)
            try container.encodeIfPresent(exportPath, forKey: .exportPath)
            try container.encode(summarizing, forKey: .summarizing)
        }
    }
}

public struct HexBootstrapSnapshot: Codable, Sendable, Equatable {
    public var version: String
    public var developerFeaturesEnabled: Bool
    public var pid: UInt32
    public var instanceLockHeld: Bool
    public var instanceConflictError: String?
    public var setupReady: Bool
    public var onboardingCompleted: Bool
    public var permissions: HexPermissionSnapshot
    public var statusMenu: HexStatusMenuSnapshot
    public var hudTelemetry: HexHudTelemetry
    public var meetingPanel: HexMeetingPanelSnapshot
    public var activeModelId: String
    public var activeLanguage: String
    public var commandsEnabled: Bool
    public var voiceActionEnabled: Bool
    public var meetingsEnabled: Bool
    public var releaseMicrophoneWhileIdle: Bool
    public var showDockIcon: Bool
    public var soundVolume: Float
    public var settings: HexAppSettings
    public var dictationHotkeyKeycaps: [String]
    public var dictationHotkeySide: String?
    public var editHotkeyKeycaps: [String]
    public var editHotkeySide: String?
    public var pasteLastHotkeyKeycaps: [String]
    public var pasteLastHotkeySide: String?
    public var availableMicrophones: [String]
    public var resolvedMeetingOutputFolder: String
    public var appleIntelligenceAvailable: Bool
    public var appleIntelligenceStatus: String
    public var transcriptionCatalog: HexTranscriptionCatalogSnapshot
    public var opencodeCatalog: HexOpenCodeCatalogSnapshot
    public var commandsSnapshot: HexCommandsSnapshot
    public var meetings: [HexMeetingRowSnapshot]
    public var activity: HexActivitySnapshot
    public var installedApplications: [HexInstalledApplicationSnapshot]

    enum CodingKeys: String, CodingKey {
        case version
        case developerFeaturesEnabled = "developer_features_enabled"
        case pid
        case instanceLockHeld = "instance_lock_held"
        case instanceConflictError = "instance_conflict_error"
        case setupReady = "setup_ready"
        case onboardingCompleted = "onboarding_completed"
        case permissions
        case statusMenu = "status_menu"
        case hudTelemetry = "hud_telemetry"
        case meetingPanel = "meeting_panel"
        case activeModelId = "active_model_id"
        case activeLanguage = "active_language"
        case commandsEnabled = "commands_enabled"
        case voiceActionEnabled = "voice_action_enabled"
        case meetingsEnabled = "meetings_enabled"
        case releaseMicrophoneWhileIdle = "release_microphone_while_idle"
        case showDockIcon = "show_dock_icon"
        case soundVolume = "sound_volume"
        case settings
        case dictationHotkeyKeycaps = "dictation_hotkey_keycaps"
        case dictationHotkeySide = "dictation_hotkey_side"
        case editHotkeyKeycaps = "edit_hotkey_keycaps"
        case editHotkeySide = "edit_hotkey_side"
        case pasteLastHotkeyKeycaps = "paste_last_hotkey_keycaps"
        case pasteLastHotkeySide = "paste_last_hotkey_side"
        case availableMicrophones = "available_microphones"
        case resolvedMeetingOutputFolder = "resolved_meeting_output_folder"
        case appleIntelligenceAvailable = "apple_intelligence_available"
        case appleIntelligenceStatus = "apple_intelligence_status"
        case transcriptionCatalog = "transcription_catalog"
        case opencodeCatalog = "opencode_catalog"
        case commandsSnapshot = "commands_snapshot"
        case meetings
        case activity
        case installedApplications = "installed_applications"
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        self.version = try c.decode(String.self, forKey: .version)
        self.developerFeaturesEnabled = try c.decodeIfPresent(Bool.self, forKey: .developerFeaturesEnabled) ?? false
        self.pid = try c.decodeIfPresent(UInt32.self, forKey: .pid) ?? 0
        self.instanceLockHeld = try c.decodeIfPresent(Bool.self, forKey: .instanceLockHeld) ?? true
        self.instanceConflictError = try c.decodeIfPresent(String.self, forKey: .instanceConflictError)
        self.setupReady = try c.decodeIfPresent(Bool.self, forKey: .setupReady) ?? false
        self.onboardingCompleted = try c.decodeIfPresent(Bool.self, forKey: .onboardingCompleted) ?? false
        self.permissions = try c.decodeIfPresent(HexPermissionSnapshot.self, forKey: .permissions) ?? HexPermissionSnapshot()
        self.statusMenu = try c.decode(HexStatusMenuSnapshot.self, forKey: .statusMenu)
        self.hudTelemetry = try c.decodeIfPresent(HexHudTelemetry.self, forKey: .hudTelemetry) ?? HexHudTelemetry()
        self.meetingPanel = try c.decodeIfPresent(HexMeetingPanelSnapshot.self, forKey: .meetingPanel) ?? .hidden
        self.activeModelId = try c.decodeIfPresent(String.self, forKey: .activeModelId) ?? "parakeet_unified_en"
        self.activeLanguage = try c.decodeIfPresent(String.self, forKey: .activeLanguage) ?? "en"
        self.commandsEnabled = try c.decodeIfPresent(Bool.self, forKey: .commandsEnabled) ?? false
        self.voiceActionEnabled = try c.decodeIfPresent(Bool.self, forKey: .voiceActionEnabled) ?? false
        self.meetingsEnabled = try c.decodeIfPresent(Bool.self, forKey: .meetingsEnabled) ?? false
        self.releaseMicrophoneWhileIdle = try c.decodeIfPresent(Bool.self, forKey: .releaseMicrophoneWhileIdle) ?? false
        self.showDockIcon = try c.decodeIfPresent(Bool.self, forKey: .showDockIcon) ?? true
        self.soundVolume = try c.decodeIfPresent(Float.self, forKey: .soundVolume) ?? 0.5
        self.settings = try c.decodeIfPresent(HexAppSettings.self, forKey: .settings) ?? HexAppSettings()
        self.dictationHotkeyKeycaps = try c.decodeIfPresent([String].self, forKey: .dictationHotkeyKeycaps) ?? ["⌥"]
        self.dictationHotkeySide = try c.decodeIfPresent(String.self, forKey: .dictationHotkeySide)
        self.editHotkeyKeycaps = try c.decodeIfPresent([String].self, forKey: .editHotkeyKeycaps) ?? ["⌥", "⌘"]
        self.editHotkeySide = try c.decodeIfPresent(String.self, forKey: .editHotkeySide)
        self.pasteLastHotkeyKeycaps = try c.decodeIfPresent([String].self, forKey: .pasteLastHotkeyKeycaps) ?? ["⌥", "⇧", "V"]
        self.pasteLastHotkeySide = try c.decodeIfPresent(String.self, forKey: .pasteLastHotkeySide)
        self.availableMicrophones = try c.decodeIfPresent([String].self, forKey: .availableMicrophones) ?? []
        self.resolvedMeetingOutputFolder = try c.decodeIfPresent(String.self, forKey: .resolvedMeetingOutputFolder) ?? ""
        self.appleIntelligenceAvailable = try c.decodeIfPresent(Bool.self, forKey: .appleIntelligenceAvailable) ?? false
        self.appleIntelligenceStatus = try c.decodeIfPresent(String.self, forKey: .appleIntelligenceStatus) ?? ""
        self.transcriptionCatalog = try c.decodeIfPresent(HexTranscriptionCatalogSnapshot.self, forKey: .transcriptionCatalog) ?? HexTranscriptionCatalogSnapshot()
        self.opencodeCatalog = try c.decodeIfPresent(HexOpenCodeCatalogSnapshot.self, forKey: .opencodeCatalog) ?? HexOpenCodeCatalogSnapshot()
        self.commandsSnapshot = try c.decodeIfPresent(HexCommandsSnapshot.self, forKey: .commandsSnapshot) ?? HexCommandsSnapshot()
        self.meetings = try c.decodeIfPresent([HexMeetingRowSnapshot].self, forKey: .meetings) ?? []
        self.activity = try c.decodeIfPresent(HexActivitySnapshot.self, forKey: .activity) ?? HexActivitySnapshot()
        self.installedApplications = try c.decodeIfPresent([HexInstalledApplicationSnapshot].self, forKey: .installedApplications) ?? []
    }
}

public enum HexEngineCommand: Encodable, Sendable, Equatable {
    case refreshSnapshot
    case requestPermission(kind: String)
    case openPermissionSettings(kind: String)
    case recordOnboardingCompletion
    case pasteLastDictation
    case pasteMeetingTurns
    case selectInstalledModel(modelId: String)
    case chooseTranscriptionModel(modelId: String, language: String, recognitionHints: String?)
    case setTranscriptionLanguage(language: String)
    case setRecognitionHints(hints: String)
    case deleteTranscriptionModel(modelId: String)
    case cancelModelPreparation
    case startMeeting
    case stopMeeting
    case acceptMeetingOffer
    case dismissMeetingOffer
    case dismissCallEndedPrompt
    case revealMeetingTranscript(path: String)
    case renameMeeting(id: String, title: String)
    case deleteMeeting(id: String)
    case summarizeMeeting(id: String)
    case refreshMeetings
    case setSoundVolume(volume: Float)
    case setShowDockIcon(visible: Bool)
    case setMeetingsEnabled(enabled: Bool)
    case setMeetingNotesEnabled(enabled: Bool)
    case setMeetingOutputFolder(path: String?)
    case saveSettings(settings: HexAppSettings)
    case setHotkeyCaptureActive(active: Bool)
    case setHotkeyBinding(target: String, binding: HexHotkeyBinding?)
    case captureHotkey(target: String, key: String?, keyCode: UInt16?, modifierFlags: UInt64)
    case setHotkeySide(target: String, side: String)
    case setDoubleTapLock(enabled: Bool)
    case setDoubleTapOnly(enabled: Bool)
    case setIncrementalTranscription(enabled: Bool)
    case setRecordingAudioBehavior(behavior: String)
    case setMicrophone(device: String?)
    case refreshMicrophones
    case setReleaseMicrophoneWhileIdle(enabled: Bool, confirmDisableCommands: Bool = false)
    case setCommandsEnabled(enabled: Bool, confirmKeepMicrophoneReady: Bool = false)
    case retryCommandsModel
    case provisionPersonalCommandsWorkspace
    case openPersonalCommandsConfig
    case copyPersonalCommandsAgentPrompt
    case setVoiceActionEnabled(enabled: Bool)
    case setVoiceActionModel(model: String?, variant: String?, deadlineSeconds: UInt64?)
    case reloadOpenCodeCatalog
    case openOpenCodeSetupDocs
    case discoverInstalledApplications
    case setHistoryRetention(retention: String)
    case deleteHistoryEntry(id: UInt64)
    case clearHistory
    case refreshActivity
    case configureHudTuning(
        style: Float,
        lineCount: Float,
        curvature: Float,
        speed: Float,
        sharpness: Float,
        glow: Float,
        depth: Float,
        lightAngle: Float,
        outline: Float
    )
    case simulateHudState(state: String, rms: Float?, peak: Float?)
    case simulateMeetingPanel(panel: HexMeetingPanelSnapshot)
    case retryInstanceLock
    case quitOtherHexInstancesAndRetry
    case requestCheckForUpdates
    case requestQuit

    private enum CodingKeys: String, CodingKey {
        case type, kind, language, hints, title, path, id, volume, visible, enabled, settings
        case active, target, binding, key, side, behavior, device, model, variant, retention
        case style, curvature, speed, sharpness, glow, depth, outline, state, rms, peak, panel
        case modelId = "model_id"
        case recognitionHints = "recognition_hints"
        case keyCode = "key_code"
        case modifierFlags = "modifier_flags"
        case confirmDisableCommands = "confirm_disable_commands"
        case confirmKeepMicrophoneReady = "confirm_keep_microphone_ready"
        case deadlineSeconds = "deadline_seconds"
        case lineCount = "line_count"
        case lightAngle = "light_angle"
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .refreshSnapshot:
            try container.encode("refresh_snapshot", forKey: .type)
        case let .requestPermission(kind):
            try container.encode("request_permission", forKey: .type)
            try container.encode(kind, forKey: .kind)
        case let .openPermissionSettings(kind):
            try container.encode("open_permission_settings", forKey: .type)
            try container.encode(kind, forKey: .kind)
        case .recordOnboardingCompletion:
            try container.encode("record_onboarding_completion", forKey: .type)
        case .pasteLastDictation:
            try container.encode("paste_last_dictation", forKey: .type)
        case .pasteMeetingTurns:
            try container.encode("paste_meeting_turns", forKey: .type)
        case let .selectInstalledModel(modelId):
            try container.encode("select_installed_model", forKey: .type)
            try container.encode(modelId, forKey: .modelId)
        case let .chooseTranscriptionModel(modelId, language, recognitionHints):
            try container.encode("choose_transcription_model", forKey: .type)
            try container.encode(modelId, forKey: .modelId)
            try container.encode(language, forKey: .language)
            try container.encodeIfPresent(recognitionHints, forKey: .recognitionHints)
        case let .setTranscriptionLanguage(language):
            try container.encode("set_transcription_language", forKey: .type)
            try container.encode(language, forKey: .language)
        case let .setRecognitionHints(hints):
            try container.encode("set_recognition_hints", forKey: .type)
            try container.encode(hints, forKey: .hints)
        case let .deleteTranscriptionModel(modelId):
            try container.encode("delete_transcription_model", forKey: .type)
            try container.encode(modelId, forKey: .modelId)
        case .cancelModelPreparation:
            try container.encode("cancel_model_preparation", forKey: .type)
        case .startMeeting:
            try container.encode("start_meeting", forKey: .type)
        case .stopMeeting:
            try container.encode("stop_meeting", forKey: .type)
        case .acceptMeetingOffer:
            try container.encode("accept_meeting_offer", forKey: .type)
        case .dismissMeetingOffer:
            try container.encode("dismiss_meeting_offer", forKey: .type)
        case .dismissCallEndedPrompt:
            try container.encode("dismiss_call_ended_prompt", forKey: .type)
        case let .revealMeetingTranscript(path):
            try container.encode("reveal_meeting_transcript", forKey: .type)
            try container.encode(path, forKey: .path)
        case let .renameMeeting(id, title):
            try container.encode("rename_meeting", forKey: .type)
            try container.encode(id, forKey: .id)
            try container.encode(title, forKey: .title)
        case let .deleteMeeting(id):
            try container.encode("delete_meeting", forKey: .type)
            try container.encode(id, forKey: .id)
        case let .summarizeMeeting(id):
            try container.encode("summarize_meeting", forKey: .type)
            try container.encode(id, forKey: .id)
        case .refreshMeetings:
            try container.encode("refresh_meetings", forKey: .type)
        case let .setSoundVolume(volume):
            try container.encode("set_sound_volume", forKey: .type)
            try container.encode(volume, forKey: .volume)
        case let .setShowDockIcon(visible):
            try container.encode("set_show_dock_icon", forKey: .type)
            try container.encode(visible, forKey: .visible)
        case let .setMeetingsEnabled(enabled):
            try container.encode("set_meetings_enabled", forKey: .type)
            try container.encode(enabled, forKey: .enabled)
        case let .setMeetingNotesEnabled(enabled):
            try container.encode("set_meeting_notes_enabled", forKey: .type)
            try container.encode(enabled, forKey: .enabled)
        case let .setMeetingOutputFolder(path):
            try container.encode("set_meeting_output_folder", forKey: .type)
            try container.encodeIfPresent(path, forKey: .path)
        case let .saveSettings(settings):
            try container.encode("save_settings", forKey: .type)
            try container.encode(settings, forKey: .settings)
        case let .setHotkeyCaptureActive(active):
            try container.encode("set_hotkey_capture_active", forKey: .type)
            try container.encode(active, forKey: .active)
        case let .setHotkeyBinding(target, binding):
            try container.encode("set_hotkey_binding", forKey: .type)
            try container.encode(target, forKey: .target)
            try container.encodeIfPresent(binding, forKey: .binding)
        case let .captureHotkey(target, key, keyCode, modifierFlags):
            try container.encode("capture_hotkey", forKey: .type)
            try container.encode(target, forKey: .target)
            try container.encodeIfPresent(key, forKey: .key)
            try container.encodeIfPresent(keyCode, forKey: .keyCode)
            try container.encode(modifierFlags, forKey: .modifierFlags)
        case let .setHotkeySide(target, side):
            try container.encode("set_hotkey_side", forKey: .type)
            try container.encode(target, forKey: .target)
            try container.encode(side, forKey: .side)
        case let .setDoubleTapLock(enabled):
            try container.encode("set_double_tap_lock", forKey: .type)
            try container.encode(enabled, forKey: .enabled)
        case let .setDoubleTapOnly(enabled):
            try container.encode("set_double_tap_only", forKey: .type)
            try container.encode(enabled, forKey: .enabled)
        case let .setIncrementalTranscription(enabled):
            try container.encode("set_incremental_transcription", forKey: .type)
            try container.encode(enabled, forKey: .enabled)
        case let .setRecordingAudioBehavior(behavior):
            try container.encode("set_recording_audio_behavior", forKey: .type)
            try container.encode(behavior, forKey: .behavior)
        case let .setMicrophone(device):
            try container.encode("set_microphone", forKey: .type)
            try container.encodeIfPresent(device, forKey: .device)
        case .refreshMicrophones:
            try container.encode("refresh_microphones", forKey: .type)
        case let .setReleaseMicrophoneWhileIdle(enabled, confirmDisableCommands):
            try container.encode("set_release_microphone_while_idle", forKey: .type)
            try container.encode(enabled, forKey: .enabled)
            try container.encode(confirmDisableCommands, forKey: .confirmDisableCommands)
        case let .setCommandsEnabled(enabled, confirmKeepMicrophoneReady):
            try container.encode("set_commands_enabled", forKey: .type)
            try container.encode(enabled, forKey: .enabled)
            try container.encode(confirmKeepMicrophoneReady, forKey: .confirmKeepMicrophoneReady)
        case .retryCommandsModel:
            try container.encode("retry_commands_model", forKey: .type)
        case .provisionPersonalCommandsWorkspace:
            try container.encode("provision_personal_commands_workspace", forKey: .type)
        case .openPersonalCommandsConfig:
            try container.encode("open_personal_commands_config", forKey: .type)
        case .copyPersonalCommandsAgentPrompt:
            try container.encode("copy_personal_commands_agent_prompt", forKey: .type)
        case let .setVoiceActionEnabled(enabled):
            try container.encode("set_voice_action_enabled", forKey: .type)
            try container.encode(enabled, forKey: .enabled)
        case let .setVoiceActionModel(model, variant, deadlineSeconds):
            try container.encode("set_voice_action_model", forKey: .type)
            try container.encodeIfPresent(model, forKey: .model)
            try container.encodeIfPresent(variant, forKey: .variant)
            try container.encodeIfPresent(deadlineSeconds, forKey: .deadlineSeconds)
        case .reloadOpenCodeCatalog:
            try container.encode("reload_open_code_catalog", forKey: .type)
        case .openOpenCodeSetupDocs:
            try container.encode("open_open_code_setup_docs", forKey: .type)
        case .discoverInstalledApplications:
            try container.encode("discover_installed_applications", forKey: .type)
        case let .setHistoryRetention(retention):
            try container.encode("set_history_retention", forKey: .type)
            try container.encode(retention, forKey: .retention)
        case let .deleteHistoryEntry(id):
            try container.encode("delete_history_entry", forKey: .type)
            try container.encode(id, forKey: .id)
        case .clearHistory:
            try container.encode("clear_history", forKey: .type)
        case .refreshActivity:
            try container.encode("refresh_activity", forKey: .type)
        case let .configureHudTuning(style, lineCount, curvature, speed, sharpness, glow, depth, lightAngle, outline):
            try container.encode("configure_hud_tuning", forKey: .type)
            try container.encode(style, forKey: .style)
            try container.encode(lineCount, forKey: .lineCount)
            try container.encode(curvature, forKey: .curvature)
            try container.encode(speed, forKey: .speed)
            try container.encode(sharpness, forKey: .sharpness)
            try container.encode(glow, forKey: .glow)
            try container.encode(depth, forKey: .depth)
            try container.encode(lightAngle, forKey: .lightAngle)
            try container.encode(outline, forKey: .outline)
        case let .simulateHudState(state, rms, peak):
            try container.encode("simulate_hud_state", forKey: .type)
            try container.encode(state, forKey: .state)
            try container.encodeIfPresent(rms, forKey: .rms)
            try container.encodeIfPresent(peak, forKey: .peak)
        case let .simulateMeetingPanel(panel):
            try container.encode("simulate_meeting_panel", forKey: .type)
            try container.encode(panel, forKey: .panel)
        case .retryInstanceLock:
            try container.encode("retry_instance_lock", forKey: .type)
        case .quitOtherHexInstancesAndRetry:
            try container.encode("quit_other_hex_instances_and_retry", forKey: .type)
        case .requestCheckForUpdates:
            try container.encode("request_check_for_updates", forKey: .type)
        case .requestQuit:
            try container.encode("request_quit", forKey: .type)
        }
    }
}

public enum HexEventTag: UInt32, Sendable, Equatable {
    case readinessChanged = 1
    case settingsChanged = 2
    case modelPrepUpdated = 3
    case shortcutCaptureUpdated = 4
    case historyUpdated = 5
    case activityAppended = 6
    case meetingStateChanged = 7
    case meetingTranscriptDelta = 8
    case opencodeCatalogUpdated = 9
    case personalCommandsUpdated = 10
    case devCommandRequested = 11
    case requestCheckForUpdates = 12
    case hudChanged = 13
    case meetingPanelChanged = 14
    case quitRequested = 15
}

public struct HexRuntimeEvent: Sendable, Equatable {
    public var tag: HexEventTag
    public var generation: UInt64
    public var payloadJSON: Data
}
