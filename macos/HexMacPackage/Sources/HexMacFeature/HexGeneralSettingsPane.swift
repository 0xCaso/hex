import AppKit
import SwiftUI

public struct HexShortcutRecorderControl: View {
    public var store: HexSettingsStore
    public let target: String
    public let keycaps: [String]
    public let side: String?
    public let isOptional: Bool
    public let isDefault: Bool

    @State private var eventMonitor: Any?
    @State private var pendingModifierFlags: UInt64 = 0

    private var isRecording: Bool {
        store.activeShortcutTarget == target
    }

    public init(
        store: HexSettingsStore,
        target: String,
        keycaps: [String],
        side: String?,
        isOptional: Bool = false,
        isDefault: Bool = true
    ) {
        self.store = store
        self.target = target
        self.keycaps = keycaps
        self.side = side
        self.isOptional = isOptional
        self.isDefault = isDefault
    }

    public var body: some View {
        VStack(alignment: .trailing, spacing: 4) {
            HStack(spacing: 8) {
                if let side {
                    Picker(
                        "",
                        selection: Binding(
                            get: { side },
                            set: { store.setShortcutSide(target: target, side: $0) }
                        )
                    ) {
                        Text("Left").tag("left")
                        Text("Either").tag("either")
                        Text("Right").tag("right")
                    }
                    .pickerStyle(.segmented)
                    .labelsHidden()
                    .frame(width: 150)
                    .controlSize(.small)
                }

                Button {
                    if isRecording {
                        stopRecording()
                    } else {
                        startRecording()
                    }
                } label: {
                    HStack(spacing: 4) {
                        if isRecording {
                            Image(systemName: "record.circle")
                                .foregroundStyle(Color.accentColor)
                            Text("Press shortcut… (Esc to cancel)")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        } else if keycaps.isEmpty {
                            Text("Not set")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        } else {
                            ForEach(Array(keycaps.enumerated()), id: \.offset) { _, cap in
                                Text(cap)
                                    .font(.system(size: 12, weight: .semibold, design: .rounded))
                                    .padding(.horizontal, 6)
                                    .padding(.vertical, 2)
                                    .background(
                                        RoundedRectangle(cornerRadius: 4, style: .continuous)
                                            .fill(Color.primary.opacity(0.10))
                                    )
                            }
                        }
                    }
                    .padding(.horizontal, 8)
                    .padding(.vertical, 4)
                }
                .buttonStyle(.bordered)
                .tint(isRecording ? .accentColor : nil)

                if isOptional && !keycaps.isEmpty {
                    Button("Clear") {
                        stopRecording()
                        store.clearOptionalShortcut(target: target)
                    }
                    .controlSize(.small)
                }

                if !isDefault {
                    Button("Default") {
                        stopRecording()
                        store.resetShortcutToDefault(target: target)
                    }
                    .controlSize(.small)
                }
            }

            if isRecording, let error = store.shortcutCaptureError {
                Text(error)
                    .font(.caption2)
                    .foregroundStyle(.red)
            }
        }
        .onDisappear {
            if isRecording {
                stopRecording()
            }
        }
    }

    private func startRecording() {
        pendingModifierFlags = 0
        store.startRecordingShortcut(target: target)
        removeMonitor()
        eventMonitor = NSEvent.addLocalMonitorForEvents(matching: [.keyDown, .flagsChanged]) { event in
            handleEvent(event)
            return nil
        }
    }

    private func stopRecording() {
        removeMonitor()
        pendingModifierFlags = 0
        store.cancelRecordingShortcut()
    }

    private func removeMonitor() {
        if let monitor = eventMonitor {
            NSEvent.removeMonitor(monitor)
            eventMonitor = nil
        }
    }

    private func handleEvent(_ event: NSEvent) {
        let relevantMask: UInt64 = (1 << 17) | (1 << 18) | (1 << 19) | (1 << 20) | (1 << 23)
        let rawFlags = UInt64(event.modifierFlags.rawValue)

        if event.type == .flagsChanged {
            let activeMods = rawFlags & relevantMask
            if activeMods != 0 {
                pendingModifierFlags = rawFlags
            } else if pendingModifierFlags != 0 {
                let captured = pendingModifierFlags
                pendingModifierFlags = 0
                store.captureShortcut(target: target, key: nil, keyCode: nil, modifierFlags: captured)
                if store.activeShortcutTarget == nil {
                    removeMonitor()
                }
            }
            return
        }

        if event.type == .keyDown {
            if event.keyCode == 53 {
                stopRecording()
                return
            }
            if isOptional && (event.keyCode == 51 || event.keyCode == 117) && (rawFlags & relevantMask) == 0 {
                stopRecording()
                store.clearOptionalShortcut(target: target)
                return
            }
            pendingModifierFlags = 0
            let keyName = Self.keyString(for: event)
            store.captureShortcut(
                target: target,
                key: keyName,
                keyCode: event.keyCode,
                modifierFlags: rawFlags
            )
            if store.activeShortcutTarget == nil {
                removeMonitor()
            }
        }
    }

    private static func keyString(for event: NSEvent) -> String {
        switch event.keyCode {
        case 49: return "space"
        case 48: return "tab"
        case 36, 76: return "enter"
        case 51: return "backspace"
        case 117: return "delete"
        case 126: return "up"
        case 125: return "down"
        case 123: return "left"
        case 124: return "right"
        case 116: return "pageup"
        case 121: return "pagedown"
        case 115: return "home"
        case 119: return "end"
        case 114: return "help"
        case 122: return "f1"
        case 120: return "f2"
        case 99: return "f3"
        case 118: return "f4"
        case 96: return "f5"
        case 97: return "f6"
        case 98: return "f7"
        case 100: return "f8"
        case 101: return "f9"
        case 109: return "f10"
        case 103: return "f11"
        case 111: return "f12"
        case 105: return "f13"
        case 107: return "f14"
        case 113: return "f15"
        case 106: return "f16"
        case 64: return "f17"
        case 79: return "f18"
        case 80: return "f19"
        case 90: return "f20"
        default:
            return event.charactersIgnoringModifiers ?? ""
        }
    }
}

public struct HexTranscriptionModelPickerSheet: View {
    public var store: HexSettingsStore
    @Environment(\.dismiss) private var dismiss

    public init(store: HexSettingsStore) {
        self.store = store
    }

    public var body: some View {
        let catalog = store.runtime.snapshot?.transcriptionCatalog ?? HexTranscriptionCatalogSnapshot()
        let selectedLanguage = catalog.selectedLanguage

        VStack(alignment: .leading, spacing: 0) {
            HStack {
                VStack(alignment: .leading, spacing: 2) {
                    Text("Language & Transcription Model")
                        .font(.headline)
                    Text("Models are verified by SHA-256 and loaded on Metal before switching.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer()
                Picker(
                    "Language",
                    selection: Binding(
                        get: { selectedLanguage },
                        set: { store.runtime.dispatch(.setTranscriptionLanguage(language: $0)) }
                    )
                ) {
                    ForEach(catalog.languages) { lang in
                        Text(lang.name).tag(lang.code)
                    }
                }
                .frame(width: 220)

                Button("Done") {
                    dismiss()
                }
                .keyboardShortcut(.cancelAction)
            }
            .padding(16)

            Divider()

            if let error = catalog.preparationError ?? catalog.deletionError {
                HStack(spacing: 8) {
                    Image(systemName: "exclamationmark.triangle.fill")
                        .foregroundStyle(.orange)
                    Text(error)
                        .font(.caption)
                    Spacer()
                }
                .padding(.horizontal, 16)
                .padding(.vertical, 8)
                .background(Color.orange.opacity(0.12))
            }

            ScrollView {
                VStack(alignment: .leading, spacing: 10) {
                    ForEach(catalog.models) { model in
                        modelRowCard(model, selectedLanguage: selectedLanguage)
                    }
                }
                .padding(16)
            }
        }
        .frame(width: 620, height: 480)
    }

    @ViewBuilder
    private func modelRowCard(_ model: HexTranscriptionModelRow, selectedLanguage: String) -> some View {
        let supportsLanguage = model.languages.contains(selectedLanguage) || selectedLanguage == "auto"
        VStack(alignment: .leading, spacing: 8) {
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                Text(model.name)
                    .font(.system(size: 14, weight: .semibold))
                if let rec = model.recommendationLabel {
                    Text(rec)
                        .font(.caption2.weight(.semibold))
                        .padding(.horizontal, 6)
                        .padding(.vertical, 2)
                        .background(Capsule().fill(Color.accentColor.opacity(0.18)))
                        .foregroundStyle(Color.accentColor)
                }
                if model.active {
                    Label("Active", systemImage: "checkmark.circle.fill")
                        .font(.caption.weight(.medium))
                        .foregroundStyle(.green)
                } else if model.installed {
                    Text("Downloaded")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer()
                Text("\(model.realtime) · \(model.sizeLabel)")
                    .font(.caption.monospacedDigit())
                    .foregroundStyle(.secondary)
            }

            Text(model.summary)
                .font(.caption)
                .foregroundStyle(.secondary)

            if model.id == "parakeet-tdt-0.6b-v3" && selectedLanguage != "en" && selectedLanguage != "auto" {
                Text("Parakeet v3 detects language automatically from speech.")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            }

            if model.preparing {
                HStack(spacing: 10) {
                    if let fraction = model.progressFraction {
                        ProgressView(value: Double(fraction))
                            .frame(maxWidth: 260)
                    } else {
                        ProgressView()
                            .controlSize(.small)
                    }
                    Text(model.statusLabel)
                        .font(.caption.monospacedDigit())
                        .foregroundStyle(.secondary)
                    Spacer()
                    Button("Cancel") {
                        store.runtime.dispatch(.cancelModelPreparation)
                    }
                    .controlSize(.small)
                }
            } else {
                HStack(spacing: 8) {
                    if !model.active {
                        Button(model.installed ? "Use Model" : "Download & Use") {
                            store.runtime.dispatch(
                                .chooseTranscriptionModel(
                                    modelId: model.id,
                                    language: selectedLanguage,
                                    recognitionHints: nil
                                )
                            )
                        }
                        .buttonStyle(.borderedProminent)
                        .controlSize(.small)
                        .disabled(!supportsLanguage)
                    }
                    if model.canDelete {
                        Button(role: .destructive) {
                            store.runtime.dispatch(.deleteTranscriptionModel(modelId: model.id))
                        } label: {
                            Label("Delete", systemImage: "trash")
                        }
                        .controlSize(.small)
                    }
                    Spacer()
                }
            }
        }
        .padding(12)
        .background(
            RoundedRectangle(cornerRadius: 8, style: .continuous)
                .fill(Color(nsColor: .controlBackgroundColor))
        )
        .overlay(
            RoundedRectangle(cornerRadius: 8, style: .continuous)
                .strokeBorder(model.active ? Color.accentColor.opacity(0.6) : Color.primary.opacity(0.08), lineWidth: 1)
        )
    }
}

public struct HexOnboardingView: View {
    public var store: HexSettingsStore

    public init(store: HexSettingsStore) {
        self.store = store
    }

    public var body: some View {
        let snapshot = store.runtime.snapshot
        let perms = snapshot?.permissions ?? HexPermissionSnapshot()
        let activeModelRow = snapshot?.transcriptionCatalog.models.first(where: {
            $0.id == snapshot?.activeModelId
        }) ?? snapshot?.transcriptionCatalog.models.first

        Form {
            Section {
                VStack(alignment: .leading, spacing: 6) {
                    Label("Set up HEX for local voice dictation", systemImage: "waveform.badge.mic")
                        .font(.headline)
                    Text("HEX runs speech recognition locally on your Mac. Grant the three macOS permissions and confirm your local speech model to start dictating.")
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                }
                .padding(.vertical, 4)
            }

            Section("Required macOS Permissions") {
                permissionItem(
                    title: "Microphone",
                    subtitle: "Capture your voice for dictation and optional commands.",
                    granted: perms.microphoneGranted,
                    buttonTitle: perms.microphoneState == "needs_request" ? "Allow" : "Open Settings",
                    kind: "microphone"
                )
                permissionItem(
                    title: "Input Monitoring",
                    subtitle: "Detect your dictation shortcut globally across apps.",
                    granted: perms.inputMonitoringGranted,
                    buttonTitle: "Open Settings",
                    kind: "input_monitoring"
                )
                permissionItem(
                    title: "Accessibility",
                    subtitle: "Insert transcribed text and suppress shortcut chord leakage.",
                    granted: perms.accessibilityGranted,
                    buttonTitle: "Open Settings",
                    kind: "accessibility"
                )
            }

            Section("Local Speech Model") {
                HStack {
                    VStack(alignment: .leading, spacing: 3) {
                        Text(activeModelRow?.name ?? (snapshot?.activeModelId ?? "Parakeet TDT 0.6B v3"))
                            .font(.body.weight(.medium))
                        Text(perms.modelInstalled ? "Installed and ready on Metal." : "Download required before dictation can start.")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                    Spacer()
                    if let activeModelRow, activeModelRow.preparing {
                        ProgressView()
                            .controlSize(.small)
                        Text(activeModelRow.statusLabel)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                        Button("Cancel") {
                            store.runtime.dispatch(.cancelModelPreparation)
                        }
                        .controlSize(.small)
                    } else if perms.modelInstalled {
                        Label("Ready", systemImage: "checkmark.circle.fill")
                            .foregroundStyle(.green)
                    } else {
                        Button("Download Model") {
                            let modelId = snapshot?.activeModelId ?? "parakeet-tdt-0.6b-v3"
                            let lang = snapshot?.activeLanguage ?? "en"
                            store.runtime.dispatch(
                                .chooseTranscriptionModel(
                                    modelId: modelId,
                                    language: lang,
                                    recognitionHints: nil
                                )
                            )
                        }
                        .buttonStyle(.borderedProminent)
                    }
                    Button("Choose Model…") {
                        store.isModelPickerPresented = true
                    }
                }
            }

            if snapshot?.setupReady == true {
                Section {
                    HStack {
                        Label("All permissions and local speech model are ready.", systemImage: "checkmark.seal.fill")
                            .foregroundStyle(.green)
                        Spacer()
                        Button("Continue to Settings") {
                            store.runtime.dispatch(.recordOnboardingCompletion)
                            store.selectSection(.settings)
                        }
                        .buttonStyle(.borderedProminent)
                    }
                }
            }
        }
        .formStyle(.grouped)
        .sheet(isPresented: Bindable(store).isModelPickerPresented) {
            HexTranscriptionModelPickerSheet(store: store)
        }
    }

    @ViewBuilder
    private func permissionItem(
        title: String,
        subtitle: String,
        granted: Bool,
        buttonTitle: String,
        kind: String
    ) -> some View {
        HStack(alignment: .center, spacing: 12) {
            VStack(alignment: .leading, spacing: 2) {
                Text(title)
                    .font(.body.weight(.medium))
                Text(subtitle)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            Spacer()
            if granted {
                Label("Ready", systemImage: "checkmark.circle.fill")
                    .foregroundStyle(.green)
                    .font(.callout.weight(.medium))
            } else {
                Button(buttonTitle) {
                    store.runtime.dispatch(.requestPermission(kind: kind))
                }
                .controlSize(.small)
            }
        }
    }
}

public struct HexGeneralSettingsPane: View {
    public var store: HexSettingsStore
    public var onCheckForUpdates: (() -> Void)?

    public init(store: HexSettingsStore, onCheckForUpdates: (() -> Void)? = nil) {
        self.store = store
        self.onCheckForUpdates = onCheckForUpdates
    }

    public var body: some View {
        if let snapshot = store.runtime.snapshot {
            let settings = snapshot.settings
            Form {
                topNoticeSections(snapshot: snapshot)
                shortcutsSection(snapshot: snapshot, settings: settings)
                microphoneSection(snapshot: snapshot, settings: settings)
                transcriptionSection(snapshot: snapshot, settings: settings)
                appAndFeedbackSection(snapshot: snapshot, settings: settings)
                meetingsBetaSection(snapshot: snapshot, settings: settings)
            }
            .formStyle(.grouped)
            .sheet(isPresented: Bindable(store).isModelPickerPresented) {
                HexTranscriptionModelPickerSheet(store: store)
            }
            .alert(
                "Disable Voice Commands?",
                isPresented: Bindable(store).pendingReleaseMicrophoneConfirmation
            ) {
                Button("Cancel", role: .cancel) {}
                Button("Disable Commands & Release Microphone", role: .destructive) {
                    store.confirmDisableCommandsAndReleaseMicrophone()
                }
            } message: {
                Text("Releasing the microphone while idle turns off always-listening Voice Commands.")
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
        } else {
            ProgressView("Loading settings…")
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
    }

    @ViewBuilder
    private func topNoticeSections(snapshot: HexBootstrapSnapshot) -> some View {
        let perms = snapshot.permissions
        if !perms.microphoneGranted || !perms.inputMonitoringGranted || !perms.accessibilityGranted {
            Section {
                if !perms.microphoneGranted {
                    HStack {
                        Label("Microphone access is required for dictation.", systemImage: "mic.slash.fill")
                            .foregroundStyle(.orange)
                        Spacer()
                        Button(perms.microphoneState == "needs_request" ? "Allow" : "Open Settings") {
                            store.runtime.dispatch(.requestPermission(kind: "microphone"))
                        }
                        .controlSize(.small)
                    }
                }
                if !perms.inputMonitoringGranted {
                    HStack {
                        Label("Input Monitoring is required to detect shortcuts.", systemImage: "keyboard.badge.ellipsis")
                            .foregroundStyle(.orange)
                        Spacer()
                        Button("Open Settings") {
                            store.runtime.dispatch(.requestPermission(kind: "input_monitoring"))
                        }
                        .controlSize(.small)
                    }
                }
                if !perms.accessibilityGranted {
                    HStack {
                        Label("Accessibility is required to paste transcriptions.", systemImage: "hand.raised.fill")
                            .foregroundStyle(.orange)
                        Spacer()
                        Button("Open Settings") {
                            store.runtime.dispatch(.requestPermission(kind: "accessibility"))
                        }
                        .controlSize(.small)
                    }
                }
            }
        }

        if let prepTitle = snapshot.statusMenu.preparingModelTitle {
            Section {
                HStack {
                    ProgressView()
                        .controlSize(.small)
                    Text(prepTitle)
                    Spacer()
                    Button("Cancel") {
                        store.runtime.dispatch(.cancelModelPreparation)
                    }
                    .controlSize(.small)
                }
            }
        } else if !perms.modelInstalled {
            Section {
                HStack {
                    Label("Selected transcription model is not downloaded.", systemImage: "arrow.down.circle")
                    Spacer()
                    Button("Download") {
                        store.runtime.dispatch(
                            .chooseTranscriptionModel(
                                modelId: snapshot.activeModelId,
                                language: snapshot.activeLanguage,
                                recognitionHints: nil
                            )
                        )
                    }
                    .buttonStyle(.borderedProminent)
                    .controlSize(.small)
                    Button("Choose Model…") {
                        store.isModelPickerPresented = true
                    }
                    .controlSize(.small)
                }
            }
        }
    }

    @ViewBuilder
    private func shortcutsSection(snapshot: HexBootstrapSnapshot, settings: HexAppSettings) -> some View {
        Section("Shortcuts") {
            HStack {
                VStack(alignment: .leading, spacing: 2) {
                    Text("Dictation shortcut")
                    Text("Hold to dictate and release to transcribe.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer()
                HexShortcutRecorderControl(
                    store: store,
                    target: "dictation",
                    keycaps: snapshot.dictationHotkeyKeycaps,
                    side: snapshot.dictationHotkeySide,
                    isOptional: false,
                    isDefault: settings.dictationHotkey == HexHotkeyBinding()
                )
            }

            Toggle(
                isOn: Binding(
                    get: { settings.doubleTapLock },
                    set: { store.runtime.dispatch(.setDoubleTapLock(enabled: $0)) }
                )
            ) {
                VStack(alignment: .leading, spacing: 2) {
                    Text("Double-tap to lock")
                    Text("Tap the shortcut twice within 300 ms for hands-free dictation.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }

            if settings.dictationHotkey.key != nil {
                Toggle(
                    isOn: Binding(
                        get: { settings.doubleTapOnly },
                        set: { store.runtime.dispatch(.setDoubleTapOnly(enabled: $0)) }
                    )
                ) {
                    VStack(alignment: .leading, spacing: 2) {
                        Text("Require double-tap to start")
                        Text("Ignore single presses so the key combination stays available in other apps.")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                }
            }

            HStack {
                VStack(alignment: .leading, spacing: 2) {
                    Text("Paste last dictation")
                    Text("Re-insert the most recent completed transcript.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer()
                HexShortcutRecorderControl(
                    store: store,
                    target: "paste_last",
                    keycaps: snapshot.pasteLastHotkeyKeycaps,
                    side: snapshot.pasteLastHotkeySide,
                    isOptional: true,
                    isDefault: settings.pasteLastHotkey == .pasteLastDefault
                )
            }
        }
    }

    @ViewBuilder
    private func microphoneSection(snapshot: HexBootstrapSnapshot, settings: HexAppSettings) -> some View {
        Section("Microphone & Recording") {
            HStack {
                Picker(
                    "Input device",
                    selection: Binding(
                        get: { settings.microphone ?? "" },
                        set: { value in
                            store.runtime.dispatch(.setMicrophone(device: value.isEmpty ? nil : value))
                        }
                    )
                ) {
                    Text("Automatic (Preferred Order)").tag("")
                    ForEach(snapshot.availableMicrophones, id: \.self) { device in
                        Text(device).tag(device)
                    }
                    if let saved = settings.microphone,
                       !saved.isEmpty,
                       !snapshot.availableMicrophones.contains(saved)
                    {
                        Text("\(saved) (Unavailable)").tag(saved)
                    }
                }

                Button {
                    store.runtime.dispatch(.refreshMicrophones)
                } label: {
                    Image(systemName: "arrow.clockwise")
                }
                .help("Refresh input devices")
            }

            Picker(
                "Microphone access",
                selection: Binding(
                    get: { snapshot.releaseMicrophoneWhileIdle },
                    set: { store.setReleaseMicrophoneWhileIdle($0) }
                )
            ) {
                Text("Keep ready for instant dictation").tag(false)
                Text("Release when idle").tag(true)
            }

            Picker(
                "While recording",
                selection: Binding(
                    get: { settings.recordingAudioBehavior },
                    set: { store.runtime.dispatch(.setRecordingAudioBehavior(behavior: $0)) }
                )
            ) {
                Text("Do nothing").tag("do_nothing")
                Text("Mute system audio").tag("mute")
                Text("Pause media playback").tag("pause_media")
            }

            Toggle(
                isOn: Binding(
                    get: { settings.incrementalTranscription },
                    set: { store.runtime.dispatch(.setIncrementalTranscription(enabled: $0)) }
                )
            ) {
                VStack(alignment: .leading, spacing: 2) {
                    Text("Transcribe during pauses")
                    Text("Transcribe completed phrases while you hold the shortcut so long dictations finish faster.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
        }
    }

    @ViewBuilder
    private func transcriptionSection(snapshot: HexBootstrapSnapshot, settings: HexAppSettings) -> some View {
        let catalog = snapshot.transcriptionCatalog
        let activeRow = catalog.models.first(where: { $0.id == snapshot.activeModelId })
        let langName = catalog.languages.first(where: { $0.code == snapshot.activeLanguage })?.name ?? snapshot.activeLanguage

        Section("Transcription") {
            HStack {
                VStack(alignment: .leading, spacing: 2) {
                    Text("Language & Model")
                    Text("\(langName) · \(activeRow?.name ?? snapshot.activeModelId)")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer()
                if let activeRow {
                    Text(activeRow.statusLabel)
                        .font(.caption)
                        .foregroundStyle(activeRow.active ? .green : .secondary)
                }
                Button("Change…") {
                    store.isModelPickerPresented = true
                }
            }

            if activeRow?.supportsRecognitionHints == true {
                TextField(
                    "Vocabulary & recognition hints",
                    text: Binding(
                        get: { settings.transcription.recognitionHints },
                        set: { store.runtime.dispatch(.setRecognitionHints(hints: $0)) }
                    ),
                    prompt: Text("Custom names, acronyms, or domain terms")
                )
            }
        }
    }

    @ViewBuilder
    private func appAndFeedbackSection(snapshot: HexBootstrapSnapshot, settings: HexAppSettings) -> some View {
        Section("App & Feedback") {
            HStack(spacing: 12) {
                Text("Feedback sound volume")
                Spacer()
                Slider(
                    value: Binding(
                        get: { Double(snapshot.soundVolume) },
                        set: { store.runtime.dispatch(.setSoundVolume(volume: Float($0))) }
                    ),
                    in: 0...1
                )
                .frame(width: 180)
                Text(snapshot.soundVolume <= 0.001 ? "Off" : "\(Int((snapshot.soundVolume * 100).rounded()))%")
                    .font(.caption.monospacedDigit())
                    .foregroundStyle(.secondary)
                    .frame(width: 40, alignment: .trailing)
            }

            Toggle(
                "Show HEX in Dock",
                isOn: Binding(
                    get: { snapshot.showDockIcon },
                    set: { store.runtime.dispatch(.setShowDockIcon(visible: $0)) }
                )
            )

            HStack {
                VStack(alignment: .leading, spacing: 2) {
                    Text("Launch at login")
                    if store.launchAtLoginState == .requiresApproval {
                        Text("Requires approval in System Settings > General > Login Items.")
                            .font(.caption)
                            .foregroundStyle(.orange)
                    } else if let error = store.launchAtLoginError {
                        Text(error)
                            .font(.caption)
                            .foregroundStyle(.red)
                    }
                }
                Spacer()
                if store.launchAtLoginState == .requiresApproval {
                    Button("Open Login Items…") {
                        LaunchAtLoginController.openSystemSettings()
                    }
                    .controlSize(.small)
                }
                Toggle(
                    "",
                    isOn: Binding(
                        get: { store.launchAtLoginState == .enabled },
                        set: { store.setLaunchAtLogin($0) }
                    )
                )
                .labelsHidden()
            }

            HStack {
                VStack(alignment: .leading, spacing: 2) {
                    Text("Software updates")
                    Text("Version \(snapshot.version)")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer()
                if snapshot.statusMenu.updatesManaged {
                    Text(snapshot.statusMenu.managedUpdatesLabel)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                } else {
                    Button("Check for Updates…") {
                        if let onCheckForUpdates {
                            onCheckForUpdates()
                        } else {
                            store.runtime.dispatch(.requestCheckForUpdates)
                        }
                    }
                }
            }
        }
    }

    @ViewBuilder
    private func meetingsBetaSection(snapshot: HexBootstrapSnapshot, settings: HexAppSettings) -> some View {
        Section("Meetings (Beta)") {
            Toggle(
                isOn: Binding(
                    get: { snapshot.meetingsEnabled },
                    set: { store.runtime.dispatch(.setMeetingsEnabled(enabled: $0)) }
                )
            ) {
                VStack(alignment: .leading, spacing: 2) {
                    Text("Enable Meetings")
                    Text("Offer local dual-track meeting capture and speaker-labeled transcripts.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }

            if snapshot.meetingsEnabled {
                HStack {
                    VStack(alignment: .leading, spacing: 2) {
                        Text("Screen & System Audio Recording")
                        Text("Required to capture remote participants via ScreenCaptureKit.")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                    Spacer()
                    if snapshot.permissions.screenRecordingGranted {
                        Label("Ready", systemImage: "checkmark.circle.fill")
                            .foregroundStyle(.green)
                            .font(.callout)
                    } else {
                        Button("Allow") {
                            store.runtime.dispatch(.requestPermission(kind: "screen_recording"))
                        }
                        .controlSize(.small)
                    }
                }

                HStack {
                    VStack(alignment: .leading, spacing: 2) {
                        Text("Transcript export folder")
                        Text(snapshot.resolvedMeetingOutputFolder)
                            .font(.caption.monospaced())
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                            .truncationMode(.middle)
                    }
                    Spacer()
                    Button("Reveal") {
                        store.runtime.dispatch(.revealMeetingTranscript(path: snapshot.resolvedMeetingOutputFolder))
                    }
                    .controlSize(.small)
                    Button("Choose…") {
                        chooseMeetingFolder()
                    }
                    .controlSize(.small)
                    if settings.meetings.outputFolder != nil {
                        Button("Default") {
                            store.runtime.dispatch(.setMeetingOutputFolder(path: nil))
                        }
                        .controlSize(.small)
                    }
                }

                Toggle(
                    isOn: Binding(
                        get: { settings.meetings.summarizeWithAppleIntelligence },
                        set: { store.runtime.dispatch(.setMeetingNotesEnabled(enabled: $0)) }
                    )
                ) {
                    VStack(alignment: .leading, spacing: 2) {
                        Text("Summarize with Apple Intelligence")
                        Text(snapshot.appleIntelligenceStatus)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                }
                .disabled(!snapshot.appleIntelligenceAvailable)
            }
        }
    }

    private func chooseMeetingFolder() {
        let panel = NSOpenPanel()
        panel.canChooseFiles = false
        panel.canChooseDirectories = true
        panel.canCreateDirectories = true
        panel.allowsMultipleSelection = false
        panel.prompt = "Select Folder"
        if panel.runModal() == .OK, let url = panel.url {
            store.runtime.dispatch(.setMeetingOutputFolder(path: url.path))
        }
    }
}
