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
}
