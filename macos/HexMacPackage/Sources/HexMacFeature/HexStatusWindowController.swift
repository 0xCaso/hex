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
                        .fill(snapshot.setupReady ? Color.green : Color.orange)
                        .frame(width: 7, height: 7)
                    Text(snapshot.setupReady ? "Dictation Ready" : "Setup Required")
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
}

struct HexDetailContentHostView: View {
    var store: HexSettingsStore
    var onCheckForUpdates: (() -> Void)?

    var body: some View {
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

    public func windowWillClose(_ notification: Notification) {
        store.cancelRecordingShortcut()
        onWindowVisibilityChanged?(false)
    }
}
