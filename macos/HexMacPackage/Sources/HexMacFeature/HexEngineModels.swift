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
    public var accessibilityGranted: Bool
    public var inputMonitoringGranted: Bool
    public var modelInstalled: Bool

    enum CodingKeys: String, CodingKey {
        case microphoneGranted = "microphone_granted"
        case accessibilityGranted = "accessibility_granted"
        case inputMonitoringGranted = "input_monitoring_granted"
        case modelInstalled = "model_installed"
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

    enum CodingKeys: String, CodingKey {
        case version
        case developerFeaturesEnabled = "developer_features_enabled"
        case pid
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
    }
}

public enum HexEngineCommand: Encodable, Sendable, Equatable {
    case refreshStatus
    case requestPermission(kind: String)
    case pasteLastDictation
    case selectInstalledModel(modelId: String)
    case prepareTranscriptionModel(modelId: String, language: String?, recognitionHints: String?, allowDownload: Bool)
    case cancelModelPreparation
    case startMeeting(title: String?)
    case stopMeeting
    case acceptMeetingOffer
    case dismissMeetingOffer
    case dismissCallEndedPrompt
    case revealMeetingTranscript(path: String)
    case setSoundVolume(volume: Float)
    case setShowDockIcon(visible: Bool)
    case setMeetingsEnabled(enabled: Bool)
    case simulateHudState(state: String, rms: Float?, peak: Float?)
    case simulateMeetingPanel(panel: HexMeetingPanelSnapshot)
    case requestCheckForUpdates
    case requestQuit

    private enum CodingKeys: String, CodingKey {
        case type
        case kind
        case modelId = "model_id"
        case language
        case recognitionHints = "recognition_hints"
        case allowDownload = "allow_download"
        case title
        case path
        case volume
        case visible
        case enabled
        case state
        case rms
        case peak
        case panel
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .refreshStatus:
            try container.encode("refresh_status", forKey: .type)
        case let .requestPermission(kind):
            try container.encode("request_permission", forKey: .type)
            try container.encode(kind, forKey: .kind)
        case .pasteLastDictation:
            try container.encode("paste_last_dictation", forKey: .type)
        case let .selectInstalledModel(modelId):
            try container.encode("select_installed_model", forKey: .type)
            try container.encode(modelId, forKey: .modelId)
        case let .prepareTranscriptionModel(modelId, language, recognitionHints, allowDownload):
            try container.encode("prepare_transcription_model", forKey: .type)
            try container.encode(modelId, forKey: .modelId)
            try container.encodeIfPresent(language, forKey: .language)
            try container.encodeIfPresent(recognitionHints, forKey: .recognitionHints)
            try container.encode(allowDownload, forKey: .allowDownload)
        case .cancelModelPreparation:
            try container.encode("cancel_model_preparation", forKey: .type)
        case let .startMeeting(title):
            try container.encode("start_meeting", forKey: .type)
            try container.encodeIfPresent(title, forKey: .title)
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
        case let .setSoundVolume(volume):
            try container.encode("set_sound_volume", forKey: .type)
            try container.encode(volume, forKey: .volume)
        case let .setShowDockIcon(visible):
            try container.encode("set_show_dock_icon", forKey: .type)
            try container.encode(visible, forKey: .visible)
        case let .setMeetingsEnabled(enabled):
            try container.encode("set_meetings_enabled", forKey: .type)
            try container.encode(enabled, forKey: .enabled)
        case let .simulateHudState(state, rms, peak):
            try container.encode("simulate_hud_state", forKey: .type)
            try container.encode(state, forKey: .state)
            try container.encodeIfPresent(rms, forKey: .rms)
            try container.encodeIfPresent(peak, forKey: .peak)
        case let .simulateMeetingPanel(panel):
            try container.encode("simulate_meeting_panel", forKey: .type)
            try container.encode(panel, forKey: .panel)
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
