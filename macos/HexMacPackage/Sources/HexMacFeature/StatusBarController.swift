import AppKit
import Foundation

@MainActor
public final class StatusBarController: NSObject, NSMenuDelegate {
    private let statusItem: NSStatusItem
    private let menu: NSMenu
    private let runtime: HexEngineRuntime

    public var onOpenSettings: ((String?) -> Void)?
    public var onCheckForUpdates: (() -> Void)?
    public var onQuitRequested: (() -> Void)?

    private let idleImage: NSImage?
    private let recordingImage: NSImage?

    public init(runtime: HexEngineRuntime) {
        self.runtime = runtime
        self.statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        self.menu = NSMenu(title: "HEX")
        self.menu.autoenablesItems = false

        let idle = NSImage(systemSymbolName: "hexagon", accessibilityDescription: "HEX")
        idle?.isTemplate = true
        self.idleImage = idle

        let rec = NSImage(
            systemSymbolName: "record.circle",
            accessibilityDescription: "HEX is recording a meeting"
        )
        rec?.isTemplate = true
        self.recordingImage = rec

        super.init()

        self.menu.delegate = self
        if let button = statusItem.button {
            button.image = idleImage
            button.toolTip = "Hex"
        }
        statusItem.menu = menu
        rebuildMenu(from: runtime.snapshot?.statusMenu)
    }

    public func update(with snapshot: HexStatusMenuSnapshot) {
        if let button = statusItem.button {
            button.image = snapshot.meeting.recordingIcon ? (recordingImage ?? idleImage) : idleImage
        }
        rebuildMenu(from: snapshot)
    }

    public func menuNeedsUpdate(_ menu: NSMenu) {
        runtime.refreshBootstrapSnapshot()
        rebuildMenu(from: runtime.snapshot?.statusMenu)
    }

    public func rebuildMenu(from statusMenu: HexStatusMenuSnapshot?) {
        menu.removeAllItems()
        let instanceLocked = runtime.snapshot?.instanceLockHeld ?? true

        if !instanceLocked {
            let conflictItem = NSMenuItem(
                title: "Another HEX instance is running",
                action: nil,
                keyEquivalent: ""
            )
            conflictItem.isEnabled = false
            menu.addItem(conflictItem)

            let quitOtherItem = NSMenuItem(
                title: "Quit Other HEX & Retry",
                action: #selector(handleQuitOtherHexAndRetry(_:)),
                keyEquivalent: ""
            )
            quitOtherItem.target = self
            menu.addItem(quitOtherItem)
            menu.addItem(.separator())
        }

        if let meeting = statusMenu?.meeting, meeting.visible {
            if let statusTitle = meeting.statusTitle {
                let statusEntry = NSMenuItem(
                    title: statusTitle,
                    action: #selector(handleOpenMeetings(_:)),
                    keyEquivalent: ""
                )
                statusEntry.target = self
                menu.addItem(statusEntry)
            }
            if meeting.canStop {
                let stopItem = NSMenuItem(
                    title: "Stop & Transcribe",
                    action: #selector(handleStopMeeting(_:)),
                    keyEquivalent: ""
                )
                stopItem.target = self
                menu.addItem(stopItem)
            }
            if meeting.canStart {
                let startTitle = meeting.buttonTitle.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                    ? "Start Recording"
                    : meeting.buttonTitle
                let startItem = NSMenuItem(
                    title: startTitle,
                    action: #selector(handleStartMeeting(_:)),
                    keyEquivalent: ""
                )
                startItem.target = self
                startItem.isEnabled = instanceLocked
                menu.addItem(startItem)
            }
            let meetingsItem = NSMenuItem(
                title: "Meetings…",
                action: #selector(handleOpenMeetings(_:)),
                keyEquivalent: ""
            )
            meetingsItem.target = self
            menu.addItem(meetingsItem)
            menu.addItem(.separator())
        }

        let pasteLastItem = NSMenuItem(
            title: "Paste Last Dictation",
            action: #selector(handlePasteLastDictation(_:)),
            keyEquivalent: ""
        )
        pasteLastItem.target = self
        pasteLastItem.isEnabled = instanceLocked
        menu.addItem(pasteLastItem)

        let settingsItem = NSMenuItem(
            title: "Settings…",
            action: #selector(handleOpenSettings(_:)),
            keyEquivalent: ","
        )
        settingsItem.target = self
        menu.addItem(settingsItem)

        let modelSubmenu = NSMenu(title: "Transcription Model")
        modelSubmenu.autoenablesItems = false
        if let models = statusMenu?.models, !models.isEmpty {
            for entry in models {
                let item = NSMenuItem(
                    title: entry.title,
                    action: #selector(handleSelectModel(_:)),
                    keyEquivalent: ""
                )
                item.target = self
                item.representedObject = entry.modelId
                item.state = entry.selected ? .on : .off
                item.isEnabled = entry.enabled
                modelSubmenu.addItem(item)
            }
        } else {
            let emptyItem = NSMenuItem(
                title: "No downloaded models",
                action: nil,
                keyEquivalent: ""
            )
            emptyItem.isEnabled = false
            modelSubmenu.addItem(emptyItem)
        }

        modelSubmenu.addItem(.separator())

        if let preparingTitle = statusMenu?.preparingModelTitle {
            let preparingItem = NSMenuItem(
                title: preparingTitle,
                action: nil,
                keyEquivalent: ""
            )
            preparingItem.isEnabled = false
            modelSubmenu.addItem(preparingItem)

            let cancelItem = NSMenuItem(
                title: "Cancel Model Switch",
                action: #selector(handleCancelModelPreparation(_:)),
                keyEquivalent: ""
            )
            cancelItem.target = self
            modelSubmenu.addItem(cancelItem)
        }

        if let error = statusMenu?.modelSwitchError, !error.isEmpty {
            let errorItem = NSMenuItem(
                title: "Model switch failed — open Settings…",
                action: #selector(handleOpenModels(_:)),
                keyEquivalent: ""
            )
            errorItem.target = self
            errorItem.toolTip = error
            modelSubmenu.addItem(errorItem)
        }

        let manageModelsItem = NSMenuItem(
            title: "Manage Models…",
            action: #selector(handleOpenModels(_:)),
            keyEquivalent: ""
        )
        manageModelsItem.target = self
        modelSubmenu.addItem(manageModelsItem)

        let modelHeading = NSMenuItem(
            title: "Transcription Model",
            action: nil,
            keyEquivalent: ""
        )
        modelHeading.submenu = modelSubmenu
        menu.addItem(modelHeading)

        menu.addItem(.separator())

        if statusMenu?.updatesManaged == true {
            let managedItem = NSMenuItem(
                title: statusMenu?.managedUpdatesLabel ?? "Updated externally",
                action: nil,
                keyEquivalent: ""
            )
            managedItem.isEnabled = false
            menu.addItem(managedItem)
        } else {
            let checkUpdatesItem = NSMenuItem(
                title: "Check for Updates…",
                action: #selector(handleCheckForUpdates(_:)),
                keyEquivalent: ""
            )
            checkUpdatesItem.target = self
            menu.addItem(checkUpdatesItem)
        }

        menu.addItem(.separator())

        let quitItem = NSMenuItem(
            title: "Quit HEX",
            action: #selector(handleQuit(_:)),
            keyEquivalent: "q"
        )
        quitItem.target = self
        menu.addItem(quitItem)
    }

    @objc private func handlePasteLastDictation(_ sender: Any?) {
        runtime.dispatch(.pasteLastDictation)
    }

    @objc private func handleQuitOtherHexAndRetry(_ sender: Any?) {
        _ = runtime.quitOtherHexInstancesAndRetry()
    }

    @objc private func handleOpenSettings(_ sender: Any?) {
        onOpenSettings?(nil)
    }

    @objc private func handleOpenModels(_ sender: Any?) {
        onOpenSettings?("models")
    }

    @objc private func handleOpenMeetings(_ sender: Any?) {
        onOpenSettings?("meetings")
    }

    @objc private func handleStartMeeting(_ sender: Any?) {
        runtime.dispatch(.startMeeting)
    }

    @objc private func handleStopMeeting(_ sender: Any?) {
        runtime.dispatch(.stopMeeting)
    }

    @objc private func handleSelectModel(_ sender: NSMenuItem) {
        guard let modelId = sender.representedObject as? String else { return }
        runtime.dispatch(.selectInstalledModel(modelId: modelId))
    }

    @objc private func handleCancelModelPreparation(_ sender: Any?) {
        runtime.dispatch(.cancelModelPreparation)
    }

    @objc private func handleCheckForUpdates(_ sender: Any?) {
        if let onCheckForUpdates {
            onCheckForUpdates()
        } else {
            runtime.dispatch(.requestCheckForUpdates)
        }
    }

    @objc private func handleQuit(_ sender: Any?) {
        runtime.dispatch(.requestQuit)
        onQuitRequested?()
    }
}
