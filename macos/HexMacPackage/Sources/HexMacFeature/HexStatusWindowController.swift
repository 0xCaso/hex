import AppKit
import SwiftUI

struct HexSidebarNavigationView: View {
    var store: HexSettingsStore

    var body: some View {
        let sections = HexWindowSection.visibleSections(for: store.runtime.snapshot)

        List(
            sections,
            selection: Binding(
                get: { store.selectedSection },
                set: { store.selectSection($0) }
            )
        ) { section in
            Label(section.title, systemImage: section.symbolName)
                .tag(section)
        }
        .listStyle(.sidebar)
        .safeAreaInset(edge: .bottom) {
            if let snapshot = store.runtime.snapshot {
                HStack {
                    Circle()
                        .fill(statusColor(for: snapshot))
                        .frame(width: 7, height: 7)
                    Text(statusTitle(for: snapshot))
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    Spacer()
                    Text("v\(snapshot.version)")
                        .font(.caption2.monospacedDigit())
                        .foregroundStyle(.tertiary)
                }
                .padding(.horizontal, 14)
                .padding(.vertical, 10)
            }
        }
    }

    private func statusColor(for snapshot: HexBootstrapSnapshot) -> Color {
        if !snapshot.instanceLockHeld {
            return .red
        }
        return snapshot.setupReady ? .green : .orange
    }

    private func statusTitle(for snapshot: HexBootstrapSnapshot) -> String {
        if !snapshot.instanceLockHeld {
            return "Another HEX Running"
        }
        return snapshot.setupReady ? "Dictation Ready" : "Setup Required"
    }
}

struct HexDetailContentHostView: View {
    var store: HexSettingsStore
    var onCheckForUpdates: (() -> Void)?

    var body: some View {
        let snapshot = store.runtime.snapshot

        VStack(spacing: 0) {
            if let snapshot, !snapshot.instanceLockHeld {
                HStack(spacing: 10) {
                    Image(systemName: "exclamationmark.octagon.fill")
                        .foregroundStyle(.red)
                    VStack(alignment: .leading, spacing: 2) {
                        Text("Another HEX instance is already running")
                            .font(.subheadline.weight(.semibold))
                        Text(snapshot.instanceConflictError ?? "Quit the other HEX process so this app can listen for dictation and voice commands.")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                    Spacer()
                    Button("Retry") {
                        store.retryInstanceLock()
                    }
                    .controlSize(.small)
                    Button("Quit Other HEX & Retry") {
                        store.quitOtherHexAndRetry()
                    }
                    .buttonStyle(.borderedProminent)
                    .controlSize(.small)
                }
                .padding(.horizontal, 16)
                .padding(.vertical, 10)
                .background(Color.red.opacity(0.12))
                Divider()
            }

            if let error = store.bannerError, !error.isEmpty {
                HStack(spacing: 8) {
                    Image(systemName: "exclamationmark.triangle.fill")
                        .foregroundStyle(.orange)
                    Text(error)
                        .font(.caption)
                        .textSelection(.enabled)
                    Spacer()
                    Button {
                        store.dismissBannerError()
                    } label: {
                        Image(systemName: "xmark")
                    }
                    .buttonStyle(.borderless)
                    .controlSize(.small)
                    .help("Dismiss error")
                }
                .padding(.horizontal, 16)
                .padding(.vertical, 8)
                .background(Color.orange.opacity(0.12))
                Divider()
            }

            Group {
                switch store.selectedSection {
                case .onboarding:
                    HexOnboardingView(store: store)
                case .settings:
                    HexGeneralSettingsPane(store: store, onCheckForUpdates: onCheckForUpdates)
                case .modes:
                    HexModesPane(store: store)
                case .voiceAction:
                    HexVoiceActionPane(store: store)
                case .history:
                    HexHistoryPane(store: store)
                case .commands:
                    HexCommandsPane(store: store)
                case .meetings:
                    HexMeetingsPane(store: store)
                case .activity:
                    HexActivityPane(store: store)
                case .hudLab:
                    HexHudLabPane(store: store)
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .alert(
            "HEX Error",
            isPresented: Binding(
                get: { store.alertError != nil },
                set: { if !$0 { store.dismissAlertError() } }
            )
        ) {
            Button("OK", role: .cancel) {
                store.dismissAlertError()
            }
        } message: {
            Text(store.alertError ?? "")
        }
    }
}

@MainActor
public final class HexStatusWindowController: NSWindowController, NSWindowDelegate {
    public let runtime: HexEngineRuntime
    public let store: HexSettingsStore
    public var onWindowVisibilityChanged: ((Bool) -> Void)?
    public var onCheckForUpdates: (() -> Void)?

    private let splitViewController: NSSplitViewController

    public init(runtime: HexEngineRuntime, onCheckForUpdates: (() -> Void)? = nil) {
        self.runtime = runtime
        self.store = HexSettingsStore(runtime: runtime)
        self.onCheckForUpdates = onCheckForUpdates
        self.splitViewController = NSSplitViewController()

        let sidebarVC = NSHostingController(rootView: HexSidebarNavigationView(store: store))
        let sidebarItem = NSSplitViewItem(sidebarWithViewController: sidebarVC)
        sidebarItem.minimumThickness = HexTuning.StatusWindow.sidebarMinWidth
        sidebarItem.maximumThickness = HexTuning.StatusWindow.sidebarMaxWidth
        sidebarItem.canCollapse = false

        let detailVC = NSHostingController(
            rootView: HexDetailContentHostView(
                store: store,
                onCheckForUpdates: onCheckForUpdates
            )
        )
        let detailItem = NSSplitViewItem(viewController: detailVC)
        detailItem.minimumThickness = HexTuning.StatusWindow.minWidth - HexTuning.StatusWindow.sidebarMaxWidth

        splitViewController.addSplitViewItem(sidebarItem)
        splitViewController.addSplitViewItem(detailItem)

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
        window.title = "HEX"
        window.toolbarStyle = .unified
        window.minSize = NSSize(
            width: HexTuning.StatusWindow.minWidth,
            height: HexTuning.StatusWindow.minHeight
        )
        window.contentViewController = splitViewController
        window.setFrameAutosaveName("HexMainStatusWindow")
        window.isReleasedWhenClosed = false
        window.center()

        super.init(window: window)
        window.delegate = self
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    public func handleSnapshotUpdate(_ snapshot: HexBootstrapSnapshot) {
        store.handleSnapshotUpdate(snapshot)
        window?.title = store.selectedSection.title
    }

    public func showAndFocus(destination: String? = nil) {
        store.refreshLaunchAtLogin()
        store.openDeepLink(destination)
        window?.title = store.selectedSection.title
        onWindowVisibilityChanged?(true)
        NSApp.activate(ignoringOtherApps: true)
        showWindow(nil)
        window?.makeKeyAndOrderFront(nil)
    }

    public func windowDidResignKey(_ notification: Notification) {
        store.cancelRecordingShortcut()
    }

    public func windowWillClose(_ notification: Notification) {
        store.cancelRecordingShortcut()
        onWindowVisibilityChanged?(false)
    }
}
