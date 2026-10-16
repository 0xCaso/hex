import AppKit
import Foundation

@MainActor
public final class HexAppDelegate: NSObject, NSApplicationDelegate {
    public private(set) var runtime: HexEngineRuntime?
    public private(set) var statusBarController: StatusBarController?
    public private(set) var dictationHUDController: DictationHUDPanelController?
    public private(set) var meetingCapsuleController: MeetingCapsulePanelController?
    public private(set) var sparkleController: SparkleController?
    public private(set) var statusWindowController: HexStatusWindowController?

    private let previewDictationHUD: Bool
    private let developerFeaturesEnabled: Bool
    private var isSettingsWindowVisible: Bool = false

    #if DEBUG
    nonisolated public static let defaultDeveloperFeaturesEnabled = true
    #else
    nonisolated public static let defaultDeveloperFeaturesEnabled = false
    #endif

    public init(
        previewDictationHUD: Bool = CommandLine.arguments.contains("--preview-dictation"),
        developerFeaturesEnabled: Bool = HexAppDelegate.defaultDeveloperFeaturesEnabled
    ) {
        self.previewDictationHUD = previewDictationHUD
        self.developerFeaturesEnabled = developerFeaturesEnabled
        super.init()
    }

    public func applicationDidFinishLaunching(_ notification: Notification) {
        _ = HexEngineRuntime.initializeKeyboardLayoutOnMainThread()

        let engine = HexEngineRuntime(
            developerFeaturesEnabled: developerFeaturesEnabled,
            startWorkers: true,
            startLocalAPI: true,
            previewDictationHUD: previewDictationHUD
        )
        self.runtime = engine

        let sparkle = SparkleController()
        self.sparkleController = sparkle

        let statusBar = StatusBarController(runtime: engine)
        let hudController = DictationHUDPanelController(runtime: engine)
        let meetingController = MeetingCapsulePanelController(runtime: engine)
        let windowController = HexStatusWindowController(
            runtime: engine,
            onCheckForUpdates: { [weak sparkle] in
                sparkle?.checkForUpdates()
            }
        )

        self.statusBarController = statusBar
        self.dictationHUDController = hudController
        self.meetingCapsuleController = meetingController
        self.statusWindowController = windowController

        windowController.onWindowVisibilityChanged = { [weak self] visible in
            guard let self else { return }
            self.isSettingsWindowVisible = visible
            let showDock = self.runtime?.snapshot?.showDockIcon ?? true
            self.applyDockPolicy(showDockIcon: showDock)
            if !visible && !showDock {
                NSApp.hide(nil)
            }
        }

        statusBar.onOpenSettings = { [weak self] destination in
            self?.statusWindowController?.showAndFocus(destination: destination)
        }
        statusBar.onCheckForUpdates = { [weak self] in
            self?.sparkleController?.checkForUpdates()
        }
        statusBar.onQuitRequested = {
            NSApp.terminate(nil)
        }

        engine.onSnapshotUpdated = { [weak self] snapshot in
            guard let self else { return }
            self.applyDockPolicy(showDockIcon: snapshot.showDockIcon)
            self.statusBarController?.update(with: snapshot.statusMenu)
            self.statusWindowController?.handleSnapshotUpdate(snapshot)
        }
        engine.onHudTelemetryUpdated = { [weak self] telemetry in
            self?.dictationHUDController?.handleTelemetryUpdate(telemetry)
        }
        engine.onMeetingPanelUpdated = { [weak self] panel in
            self?.meetingCapsuleController?.update(with: panel)
        }
        engine.onCheckForUpdatesRequested = { [weak self] in
            self?.sparkleController?.checkForUpdates()
        }
        engine.onDeveloperCommandRequested = { [weak self] payload in
            if let pane = try? JSONDecoder().decode(String.self, from: payload) {
                self?.statusWindowController?.showAndFocus(destination: pane)
            } else {
                self?.statusWindowController?.showAndFocus()
            }
        }
        engine.onQuitRequested = {
            NSApp.terminate(nil)
        }

        if let initialSnapshot = engine.snapshot {
            applyDockPolicy(showDockIcon: initialSnapshot.showDockIcon)
            statusBar.update(with: initialSnapshot.statusMenu)
            hudController.handleTelemetryUpdate(initialSnapshot.hudTelemetry)
            meetingController.update(with: initialSnapshot.meetingPanel)
            if !initialSnapshot.setupReady && !previewDictationHUD {
                windowController.showAndFocus(destination: "onboarding")
            }
        }
    }

    public func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows flag: Bool) -> Bool {
        if !flag {
            statusWindowController?.showAndFocus()
        }
        return true
    }

    public func applicationWillTerminate(_ notification: Notification) {
        dictationHUDController?.stop()
        meetingCapsuleController?.close()
        runtime?.shutdown()
    }

    private func applyDockPolicy(showDockIcon: Bool) {
        let shouldShowInDock = showDockIcon || isSettingsWindowVisible
        let targetPolicy: NSApplication.ActivationPolicy = shouldShowInDock ? .regular : .accessory
        if NSApp.activationPolicy() != targetPolicy {
            NSApp.setActivationPolicy(targetPolicy)
        }
    }
}
