import AppKit
import SwiftUI

struct HexM0StatusView: View {
    var runtime: HexEngineRuntime

    var body: some View {
        Form {
            if let snapshot = runtime.snapshot {
                Section("Engine & Readiness") {
                    LabeledContent("Version", value: "Hex \(snapshot.version) (PID \(snapshot.pid))")
                    LabeledContent("Startup Gate", value: snapshot.setupReady ? "Ready" : "Needs Setup")
                    LabeledContent("Active Model", value: "\(snapshot.activeModelId) (\(snapshot.activeLanguage))")
                }

                Section("Permissions") {
                    permissionRow("Microphone", granted: snapshot.permissions.microphoneGranted, kind: "microphone")
                    permissionRow("Input Monitoring", granted: snapshot.permissions.inputMonitoringGranted, kind: "input_monitoring")
                    permissionRow("Accessibility", granted: snapshot.permissions.accessibilityGranted, kind: "accessibility")
                    LabeledContent("Local Model Installed", value: snapshot.permissions.modelInstalled ? "Installed" : "Missing")
                }

                Section("Preferences") {
                    Toggle(
                        "Show Hex in Dock",
                        isOn: Binding(
                            get: { snapshot.showDockIcon },
                            set: { runtime.dispatch(.setShowDockIcon(visible: $0)) }
                        )
                    )
                    Toggle(
                        "Enable Meetings",
                        isOn: Binding(
                            get: { snapshot.meetingsEnabled },
                            set: { runtime.dispatch(.setMeetingsEnabled(enabled: $0)) }
                        )
                    )
                }

                Section("M1 Native Panel Verification") {
                    HStack(spacing: 8) {
                        Button("HUD: Listening") {
                            runtime.dispatch(.simulateHudState(state: "listening", rms: 0.08, peak: 0.45))
                        }
                        Button("HUD: Voice Action") {
                            runtime.dispatch(.simulateHudState(state: "voice_action", rms: 0.09, peak: 0.52))
                        }
                        Button("HUD: Transcribing") {
                            runtime.dispatch(.simulateHudState(state: "transcribing", rms: nil, peak: nil))
                        }
                        Button("HUD: Completed") {
                            runtime.dispatch(.simulateHudState(state: "completed", rms: nil, peak: nil))
                        }
                        Button("HUD: Hide") {
                            runtime.dispatch(.simulateHudState(state: "hidden", rms: nil, peak: nil))
                        }
                    }
                    HStack(spacing: 8) {
                        Button("Meeting Offer") {
                            runtime.dispatch(.simulateMeetingPanel(panel: .offer(title: "Design Sync", sourceLabel: "Zoom")))
                        }
                        Button("Meeting Recording") {
                            runtime.dispatch(.simulateMeetingPanel(panel: .recording(title: "Design Sync", appLabel: "Zoom", elapsedText: "04:12")))
                        }
                        Button("Call Ended") {
                            runtime.dispatch(.simulateMeetingPanel(panel: .callEnded(appLabel: "Zoom", title: "Design Sync", subtitle: "Call ended · 28:40")))
                        }
                        Button("Hide Capsule") {
                            runtime.dispatch(.simulateMeetingPanel(panel: .hidden))
                        }
                    }
                }
            } else {
                Section {
                    ProgressView("Connecting to libhex runtime…")
                }
            }
        }
        .formStyle(.grouped)
        .frame(
            minWidth: HexTuning.StatusWindow.minWidth,
            minHeight: HexTuning.StatusWindow.minHeight
        )
    }

    @ViewBuilder
    private func permissionRow(_ title: String, granted: Bool, kind: String) -> some View {
        HStack {
            Text(title)
            Spacer()
            if granted {
                Label("Granted", systemImage: "checkmark.circle.fill")
                    .foregroundStyle(.green)
            } else {
                Button("Grant…") {
                    runtime.dispatch(.requestPermission(kind: kind))
                }
                .controlSize(.small)
            }
        }
    }
}

@MainActor
public final class HexStatusWindowController: NSWindowController {
    private let runtime: HexEngineRuntime

    public init(runtime: HexEngineRuntime) {
        self.runtime = runtime
        let hostingController = NSHostingController(rootView: HexM0StatusView(runtime: runtime))
        let window = NSWindow(
            contentRect: NSRect(
                x: 0,
                y: 0,
                width: HexTuning.StatusWindow.defaultWidth,
                height: HexTuning.StatusWindow.defaultHeight
            ),
            styleMask: [.titled, .closable, .miniaturizable, .resizable, .fullSizeContentView],
            backing: .buffered,
            defer: false
        )
        window.title = "Hex"
        window.toolbarStyle = .unified
        window.contentViewController = hostingController
        window.setFrameAutosaveName("HexMainStatusWindow")
        window.isReleasedWhenClosed = false
        window.center()

        super.init(window: window)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    public func showAndFocus() {
        NSApp.activate(ignoringOtherApps: true)
        showWindow(nil)
        window?.makeKeyAndOrderFront(nil)
    }
}
