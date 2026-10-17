import AppKit
import Foundation
import HexEngineFFI
import Observation

private final class EngineCallbackBridge: @unchecked Sendable {
    @MainActor weak var runtime: HexEngineRuntime?

    init() {}

    func scheduleWakeupOnMainActor() {
        DispatchQueue.main.async { [weak self] in
            MainActor.assumeIsolated {
                self?.runtime?.tick()
            }
        }
    }

    @MainActor
    func handleEvent(rawTag: UInt32, generation: UInt64, data: Data) {
        runtime?.consumeRawEvent(rawTag: rawTag, generation: generation, data: data)
    }
}

private let cWakeupTrampoline: @convention(c) (UnsafeMutableRawPointer?) -> Void = { userdata in
    guard let userdata else { return }
    let bridge = Unmanaged<EngineCallbackBridge>.fromOpaque(userdata).takeUnretainedValue()
    bridge.scheduleWakeupOnMainActor()
}

private let cEventTrampoline: @convention(c) (UnsafeMutableRawPointer?, UnsafePointer<hex_event_s>?) -> Void = { userdata, eventPtr in
    guard let userdata, let eventPtr else { return }
    let bridge = Unmanaged<EngineCallbackBridge>.fromOpaque(userdata).takeUnretainedValue()
    let rawEvent = eventPtr.pointee
    let rawTag = rawEvent.tag.rawValue
    let generation = rawEvent.generation
    let data: Data
    if let ptr = rawEvent.json_payload.ptr, rawEvent.json_payload.len > 0 {
        data = Data(bytes: ptr, count: rawEvent.json_payload.len)
    } else {
        data = Data()
    }
    MainActor.assumeIsolated {
        bridge.handleEvent(rawTag: rawTag, generation: generation, data: data)
    }
}

@MainActor
@Observable
public final class HexEngineRuntime {
    public private(set) var snapshot: HexBootstrapSnapshot?
    public private(set) var hudTelemetry: HexHudTelemetry = HexHudTelemetry()
    public private(set) var meetingPanel: HexMeetingPanelSnapshot = .hidden
    public private(set) var lastGeneration: UInt64 = 0
    public private(set) var lastError: String?
    public private(set) var receivedEvents: [HexRuntimeEvent] = []

    @ObservationIgnored
    public var onSnapshotUpdated: ((HexBootstrapSnapshot) -> Void)?
    @ObservationIgnored
    public var onHudTelemetryUpdated: ((HexHudTelemetry) -> Void)?
    @ObservationIgnored
    public var onMeetingPanelUpdated: ((HexMeetingPanelSnapshot) -> Void)?
    @ObservationIgnored
    public var onCheckForUpdatesRequested: (() -> Void)?
    @ObservationIgnored
    public var onQuitRequested: (() -> Void)?
    @ObservationIgnored
    public var onDeveloperCommandRequested: ((Data) -> Void)?
    @ObservationIgnored
    public var onCommandFailed: ((HexEngineCommand, String) -> Void)?
    @ObservationIgnored
    public var onEventReceived: ((HexEventTag) -> Void)?

    @ObservationIgnored
    private var engineHandle: hex_engine_t?
    @ObservationIgnored
    private let bridge: EngineCallbackBridge
    @ObservationIgnored
    private var bridgeRetained: Unmanaged<EngineCallbackBridge>?
    @ObservationIgnored
    private var maintenanceTimer: Timer?
    @ObservationIgnored
    private let decoder = JSONDecoder()
    @ObservationIgnored
    private let encoder = JSONEncoder()

    public static func initializeKeyboardLayoutOnMainThread() -> Bool {
        hex_keyboard_initialize_layout_on_main_thread()
    }

    public static func maybeRunCLIFromProcessArgs() -> Int32 {
        hex_cli_maybe_run_from_process_args()
    }

    public init(
        developerFeaturesEnabled: Bool = false,
        startWorkers: Bool = true,
        previewDictationHUD: Bool = false
    ) {
        let bridge = EngineCallbackBridge()
        self.bridge = bridge
        let unmanaged = Unmanaged.passRetained(bridge)
        self.bridgeRetained = unmanaged
        bridge.runtime = self

        var config = hex_runtime_config_s(
            userdata: unmanaged.toOpaque(),
            wakeup_cb: cWakeupTrampoline,
            event_cb: cEventTrampoline,
            developer_features_enabled: developerFeaturesEnabled,
            start_workers: startWorkers,
            preview_dictation_hud: previewDictationHUD
        )

        self.engineHandle = withUnsafePointer(to: &config) { ptr in
            hex_engine_new(ptr)
        }

        refreshBootstrapSnapshot()
        if let initial = snapshot {
            hudTelemetry = initial.hudTelemetry
            meetingPanel = initial.meetingPanel
        }
        startMaintenanceTimer()
    }

    public func shutdown() {
        maintenanceTimer?.invalidate()
        maintenanceTimer = nil
        if let handle = engineHandle {
            hex_engine_free(handle)
            engineHandle = nil
        }
        if let retained = bridgeRetained {
            retained.release()
            bridgeRetained = nil
        }
    }

    public func tick() {
        guard let handle = engineHandle else { return }
        hex_engine_tick(handle)
        let currentHud = readHudTelemetry()
        if currentHud != hudTelemetry {
            hudTelemetry = currentHud
            onHudTelemetryUpdated?(currentHud)
        }
    }

    public func readHudTelemetry() -> HexHudTelemetry {
        guard let handle = engineHandle else { return HexHudTelemetry() }
        let raw = hex_engine_read_hud_telemetry(handle)
        return HexHudTelemetry(cStruct: raw)
    }

    @discardableResult
    public func refreshBootstrapSnapshot() -> HexBootstrapSnapshot? {
        guard let handle = engineHandle else { return nil }
        let slice = hex_engine_copy_bootstrap_snapshot_json(handle)
        defer { hex_string_free(slice) }
        guard let data = Self.data(from: slice) else { return nil }
        do {
            let decoded = try decoder.decode(HexBootstrapSnapshot.self, from: data)
            self.snapshot = decoded
            self.hudTelemetry = decoded.hudTelemetry
            self.meetingPanel = decoded.meetingPanel
            onSnapshotUpdated?(decoded)
            return decoded
        } catch {
            self.lastError = "Failed to decode bootstrap snapshot: \(error.localizedDescription)"
            return nil
        }
    }

    @discardableResult
    public func dispatch(_ command: HexEngineCommand) -> Bool {
        guard let handle = engineHandle else { return false }
        guard let data = try? encoder.encode(command) else { return false }
        var errorSlice = hex_slice_s(ptr: nil, len: 0)
        let succeeded = data.withUnsafeBytes { rawBuffer -> Bool in
            let base = rawBuffer.bindMemory(to: CChar.self).baseAddress
            let cmdSlice = hex_slice_s(ptr: base, len: rawBuffer.count)
            return hex_engine_dispatch_json(handle, cmdSlice, &errorSlice)
        }
        defer { hex_string_free(errorSlice) }
        if !succeeded {
            let resolvedMessage: String
            if let errorData = Self.data(from: errorSlice) {
                if let obj = try? JSONSerialization.jsonObject(with: errorData) as? [String: Any],
                   let msg = obj["error"] as? String
                {
                    resolvedMessage = msg
                } else if let message = String(data: errorData, encoding: .utf8) {
                    resolvedMessage = message
                } else {
                    resolvedMessage = "Engine command failed"
                }
            } else {
                resolvedMessage = "Engine command failed"
            }
            self.lastError = resolvedMessage
            onCommandFailed?(command, resolvedMessage)
        } else {
            switch command {
            case .refreshSnapshot, .refreshMeetings, .refreshActivity,
                 .discoverInstalledApplications, .reloadOpenCodeCatalog, .setHotkeyCaptureActive:
                break
            default:
                self.lastError = nil
            }
        }
        return succeeded
    }

    @discardableResult
    public func quitOtherHexInstancesAndRetry() -> Bool {
        let myPid = ProcessInfo.processInfo.processIdentifier
        for app in NSRunningApplication.runningApplications(withBundleIdentifier: "com.kitlangton.hex2")
        where app.processIdentifier != myPid {
            app.terminate()
        }
        return dispatch(.quitOtherHexInstancesAndRetry)
    }

    public func fetchHistoryEntries(query: String = "", limit: UInt32 = 200) -> [HexHistoryEntry] {
        guard let data = copyHistoryPageJSON(query: query, limit: limit) else { return [] }
        return (try? decoder.decode([HexHistoryEntry].self, from: data)) ?? []
    }

    public func fetchMeetingDetail(meetingId: String) -> HexMeetingDetailSnapshot? {
        guard let data = copyMeetingDetailJSON(meetingId: meetingId) else { return nil }
        return try? decoder.decode(HexMeetingDetailSnapshot.self, from: data)
    }

    public func fetchMeetingDetailAsync(meetingId: String) async -> HexMeetingDetailSnapshot? {
        guard let handle = engineHandle else { return nil }
        let rawHandle = Int(bitPattern: handle)
        return await Task.detached(priority: .userInitiated) {
            guard let ptr = hex_engine_t(bitPattern: rawHandle) else { return nil }
            let idBytes = Array(meetingId.utf8CString.dropLast())
            let slice = idBytes.withUnsafeBufferPointer { buffer in
                hex_engine_copy_meeting_detail_json(
                    ptr,
                    hex_slice_s(ptr: buffer.baseAddress, len: buffer.count)
                )
            }
            defer { hex_string_free(slice) }
            guard let slicePtr = slice.ptr, slice.len > 0 else { return nil }
            let data = Data(bytes: slicePtr, count: slice.len)
            return try? JSONDecoder().decode(HexMeetingDetailSnapshot.self, from: data)
        }.value
    }

    public func copyHistoryPageJSON(query: String = "", limit: UInt32 = 100) -> Data? {
        guard let handle = engineHandle else { return nil }
        let queryBytes = Array(query.utf8CString.dropLast())
        let slice = queryBytes.withUnsafeBufferPointer { buffer in
            hex_engine_copy_history_page_json(
                handle,
                hex_slice_s(ptr: buffer.baseAddress, len: buffer.count),
                limit
            )
        }
        defer { hex_string_free(slice) }
        return Self.data(from: slice)
    }

    public func copyMeetingDetailJSON(meetingId: String) -> Data? {
        guard let handle = engineHandle else { return nil }
        let idBytes = Array(meetingId.utf8CString.dropLast())
        let slice = idBytes.withUnsafeBufferPointer { buffer in
            hex_engine_copy_meeting_detail_json(
                handle,
                hex_slice_s(ptr: buffer.baseAddress, len: buffer.count)
            )
        }
        defer { hex_string_free(slice) }
        return Self.data(from: slice)
    }

    fileprivate func consumeRawEvent(rawTag: UInt32, generation: UInt64, data: Data) {
        guard let tag = HexEventTag(rawValue: rawTag) else { return }
        self.lastGeneration = generation
        let event = HexRuntimeEvent(tag: tag, generation: generation, payloadJSON: data)
        if receivedEvents.count >= 128 {
            receivedEvents.removeFirst(receivedEvents.count - 127)
        }
        receivedEvents.append(event)

        switch tag {
        case .readinessChanged, .settingsChanged, .modelPrepUpdated, .meetingStateChanged,
             .meetingTranscriptDelta, .opencodeCatalogUpdated, .personalCommandsUpdated,
             .historyUpdated, .activityAppended:
            if let decoded = try? decoder.decode(HexBootstrapSnapshot.self, from: data) {
                self.snapshot = decoded
                self.meetingPanel = decoded.meetingPanel
                onSnapshotUpdated?(decoded)
                onMeetingPanelUpdated?(decoded.meetingPanel)
            }
            onEventReceived?(tag)
        case .hudChanged:
            if let decoded = try? decoder.decode(HexHudTelemetry.self, from: data) {
                self.hudTelemetry = decoded
                onHudTelemetryUpdated?(decoded)
            }
        case .meetingPanelChanged:
            if let decoded = try? decoder.decode(HexMeetingPanelSnapshot.self, from: data) {
                self.meetingPanel = decoded
                if var current = self.snapshot {
                    current.meetingPanel = decoded
                    self.snapshot = current
                }
                onMeetingPanelUpdated?(decoded)
            }
        case .requestCheckForUpdates:
            onCheckForUpdatesRequested?()
        case .quitRequested:
            onQuitRequested?()
        case .devCommandRequested:
            onDeveloperCommandRequested?(data)
        default:
            break
        }
    }

    private func startMaintenanceTimer() {
        maintenanceTimer = Timer.scheduledTimer(withTimeInterval: 0.25, repeats: true) { [weak self] _ in
            MainActor.assumeIsolated {
                guard let self else { return }
                if self.shouldPollPeriodicTick {
                    self.tick()
                }
                self.maintenanceTicks &+= 1
                if self.maintenanceTicks % 4 == 0, self.shouldPollReadiness {
                    self.refreshBootstrapSnapshot()
                }
            }
        }
    }

    @ObservationIgnored
    private var maintenanceTicks: UInt = 0

    /// Permission grants, download progress, and background command-model
    /// loading change outside any engine event, so poll until they settle.
    private var shouldPollReadiness: Bool {
        guard let snapshot else { return false }
        return !snapshot.setupReady
            || snapshot.commandsSnapshot.commandModelStatus == "loading"
            || snapshot.transcriptionCatalog.models.contains(where: \.preparing)
    }

    private var shouldPollPeriodicTick: Bool {
        if meetingPanel != .hidden {
            return true
        }
        if let state = snapshot?.statusMenu.meeting.state, state != "idle" {
            return true
        }
        if snapshot?.statusMenu.preparingModelTitle != nil {
            return true
        }
        if snapshot?.opencodeCatalog.state == "loading" || snapshot?.opencodeCatalog.state == "missing" {
            return true
        }
        if snapshot?.commandsSnapshot.workspaceProvisioning == true {
            return true
        }
        if snapshot?.transcriptionCatalog.models.contains(where: \.preparing) == true {
            return true
        }
        return false
    }

    private static func data(from slice: hex_slice_s) -> Data? {
        guard let ptr = slice.ptr, slice.len > 0 else { return nil }
        return Data(bytes: ptr, count: slice.len)
    }
}
