import AppKit
import Testing
@testable import HexMacFeature

@Suite("HexEngineClient & M1 Native Panels")
@MainActor
struct HexEngineClientTests {
    @Test("Metal HUD shader uniform layout matches 112-byte Metal struct")
    func shaderUniformsStrideMatchesMetalLayout() {
        #expect(MemoryLayout<HUDShaderUniforms>.size == DictationHUDPanelController.expectedUniformsSize)
        #expect(MemoryLayout<HUDShaderUniforms>.stride == DictationHUDPanelController.expectedUniformsSize)
    }

    @Test("Main-thread TIS keyboard layout initialization succeeds")
    func keyboardLayoutInitializationSucceedsOnMainThread() {
        #expect(HexEngineRuntime.initializeKeyboardLayoutOnMainThread() == true)
    }

    @Test("Engine runtime boots, decodes bootstrap snapshot, and drains HUD & meeting capsule events")
    func runtimeLifecycleAndEventDrain() {
        #expect(HexEngineRuntime.initializeKeyboardLayoutOnMainThread() == true)
        let runtime = HexEngineRuntime(
            developerFeaturesEnabled: true,
            startWorkers: false,
            startLocalAPI: false,
            previewDictationHUD: false
        )
        defer { runtime.shutdown() }

        let initial = runtime.snapshot
        #expect(initial != nil)
        #expect(initial?.pid == UInt32(ProcessInfo.processInfo.processIdentifier))
        #expect(initial?.developerFeaturesEnabled == true)

        let hudController = DictationHUDPanelController(runtime: runtime)
        defer { hudController.stop() }
        runtime.onHudTelemetryUpdated = { telemetry in
            hudController.handleTelemetryUpdate(telemetry)
        }

        #expect(runtime.dispatch(.simulateHudState(state: "listening", rms: 0.08, peak: 0.42)))
        runtime.tick()
        #expect(runtime.hudTelemetry.phase == .listening)
        #expect(runtime.hudTelemetry.isCapturing == true)
        #expect(hudController.isPanelOrderedFront == true)

        #expect(runtime.dispatch(.simulateHudState(state: "voice_action", rms: 0.09, peak: 0.50)))
        runtime.tick()
        #expect(runtime.hudTelemetry.phase == .voiceAction)
        #expect(runtime.hudTelemetry.isVoiceAction == true)

        #expect(runtime.dispatch(.simulateHudState(state: "transcribing", rms: nil, peak: nil)))
        runtime.tick()
        #expect(runtime.hudTelemetry.phase == .transcribing)
        #expect(runtime.hudTelemetry.activeQueueCount == 0)

        #expect(runtime.dispatch(.simulateHudState(state: "completed", rms: nil, peak: nil)))
        runtime.tick()
        #expect(runtime.hudTelemetry.phase == .completed)
        #expect(runtime.hudTelemetry.activeQueueCount == 0)

        let meetingCapsule = MeetingCapsulePanelController(runtime: runtime)
        defer { meetingCapsule.close() }
        runtime.onMeetingPanelUpdated = { panel in
            meetingCapsule.update(with: panel)
        }

        let offer = HexMeetingPanelSnapshot.offer(title: "Architecture Sync", sourceLabel: "Zoom")
        #expect(runtime.dispatch(.simulateMeetingPanel(panel: offer)))
        runtime.tick()
        #expect(runtime.meetingPanel == offer)
        #expect(meetingCapsule.currentState == offer)
        #expect(meetingCapsule.isVisible == true)

        let recording = HexMeetingPanelSnapshot.recording(
            title: "Architecture Sync",
            appLabel: "Zoom",
            elapsedText: "02:15"
        )
        #expect(runtime.dispatch(.simulateMeetingPanel(panel: recording)))
        runtime.tick()
        #expect(meetingCapsule.currentState == recording)
        #expect(meetingCapsule.isVisible == true)

        #expect(runtime.dispatch(.simulateMeetingPanel(panel: .hidden)))
        runtime.tick()
        #expect(meetingCapsule.currentState == .hidden)
        #expect(meetingCapsule.isVisible == false)
    }

    @Test("StatusBarController builds native menu items from engine snapshot")
    func statusBarMenuConstruction() {
        let runtime = HexEngineRuntime(
            developerFeaturesEnabled: true,
            startWorkers: false,
            startLocalAPI: false,
            previewDictationHUD: false
        )
        defer { runtime.shutdown() }

        let statusBar = StatusBarController(runtime: runtime)
        if let menuSnapshot = runtime.snapshot?.statusMenu {
            statusBar.update(with: menuSnapshot)
        }
    }

    @Test("HexSettingsStore drives Settings, Modes, Voice Action, Commands, and History invariants over FFI")
    func settingsStorePanesAndInvariants() {
        #expect(HexEngineRuntime.initializeKeyboardLayoutOnMainThread() == true)
        let runtime = HexEngineRuntime(
            developerFeaturesEnabled: true,
            startWorkers: false,
            startLocalAPI: false,
            previewDictationHUD: false
        )
        defer { runtime.shutdown() }

        let store = HexSettingsStore(runtime: runtime)
        runtime.onSnapshotUpdated = { snapshot in
            store.handleSnapshotUpdate(snapshot)
        }

        // Deep-link navigation
        store.openDeepLink("models")
        #expect(store.selectedSection == .settings)
        #expect(store.isModelPickerPresented == true)
        store.isModelPickerPresented = false

        store.openDeepLink("voice-action")
        #expect(store.selectedSection == .voiceAction)

        // Standalone modifier side change on Dictation shortcut
        store.setShortcutSide(target: "dictation", side: "left")
        runtime.tick()
        #expect(runtime.snapshot?.dictationHotkeySide == "left")
        #expect(runtime.snapshot?.dictationHotkeyKeycaps == ["L⌥"])

        store.resetShortcutToDefault(target: "dictation")
        runtime.tick()
        #expect(runtime.snapshot?.dictationHotkeySide == "either")

        // Commands & Release Microphone mutual-exclusion confirmation flow
        store.setCommandsEnabled(true)
        runtime.tick()
        #expect(runtime.snapshot?.commandsEnabled == true)

        store.setReleaseMicrophoneWhileIdle(true)
        #expect(store.pendingReleaseMicrophoneConfirmation == true)
        #expect(runtime.snapshot?.releaseMicrophoneWhileIdle == false)

        store.confirmDisableCommandsAndReleaseMicrophone()
        runtime.tick()
        #expect(runtime.snapshot?.commandsEnabled == false)
        #expect(runtime.snapshot?.releaseMicrophoneWhileIdle == true)

        store.setCommandsEnabled(true)
        #expect(store.pendingKeepMicrophoneReadyConfirmation == true)
        store.confirmKeepMicrophoneReadyAndEnableCommands()
        runtime.tick()
        #expect(runtime.snapshot?.commandsEnabled == true)
        #expect(runtime.snapshot?.releaseMicrophoneWhileIdle == false)

        // Modes: create custom mode, add browser domain, add replacement, add transformation
        store.selectSection(.modes)
        #expect(store.addMode(named: "Global") == false)
        #expect(store.modeValidationError != nil)
        #expect(store.addMode(named: "Coding") == true)
        runtime.tick()
        #expect(store.selectedMode == .custom(0))
        #expect(store.currentModeValue.name == "Coding")

        #expect(store.addBrowserDomainToCurrentMode("not a domain") == false)
        #expect(store.addBrowserDomainToCurrentMode("https://github.com/anomalyco/hex") == true)
        runtime.tick()
        #expect(store.currentModeValue.browserHosts == ["github.com"])

        store.mutateCurrentMode { mode in
            mode.replacements.append(HexTextReplacement(matchedPhrase: "open code", output: "OpenCode"))
            mode.postProcessing.enabled = true
            mode.postProcessing.model = "anthropic/claude-sonnet-4-5"
        }
        runtime.tick()
        #expect(store.currentModeValue.replacements.count == 1)
        #expect(store.currentModeValue.postProcessing.enabled == true)

        // Delete custom mode
        store.deleteCustomMode(at: 0)
        runtime.tick()
        #expect(store.selectedMode == .global)
        #expect(runtime.snapshot?.settings.dictationProcessing.modes.isEmpty == true)

        // Voice Action toggle & model
        #expect(runtime.dispatch(.setVoiceActionEnabled(enabled: true)))
        #expect(runtime.dispatch(.setVoiceActionModel(model: "openai/gpt-5", variant: "medium", deadlineSeconds: 25)))
        runtime.tick()
        #expect(runtime.snapshot?.voiceActionEnabled == true)
        #expect(runtime.snapshot?.settings.voiceAction.model == "openai/gpt-5")
        #expect(runtime.snapshot?.settings.voiceAction.variant == "medium")

        // Commands catalog filtering
        store.selectSection(.commands)
        let totalCommands = store.filteredCommands.count
        #expect(totalCommands > 0)
        store.commandsSearchQuery = "zzzz_nonexistent_phrase_zzzz"
        #expect(store.filteredCommands.isEmpty)
        store.commandsSearchQuery = ""
        #expect(store.filteredCommands.count == totalCommands)

        // Error routing: non-destructive failure populates bannerError; destructive failure populates alertError
        store.dismissBannerError()
        store.dismissAlertError()
        #expect(runtime.dispatch(.setDoubleTapOnly(enabled: true)) == false)
        #expect(store.bannerError?.contains("Double-tap only") == true)
        #expect(store.alertError == nil)

        store.dismissBannerError()
        let activeModel = runtime.snapshot?.activeModelId ?? "parakeet-tdt-0.6b-v3"
        #expect(runtime.dispatch(.deleteTranscriptionModel(modelId: activeModel)) == false)
        #expect(store.bannerError != nil)
        #expect(store.alertError != nil)
        store.dismissAlertError()
        store.dismissBannerError()
    }
}
