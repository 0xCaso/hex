import AppKit
import Foundation

@MainActor
public final class MeetingCapsulePanelController: NSObject {
    private let runtime: HexEngineRuntime
    private let panel: NSPanel
    private let effectView: NSVisualEffectView

    private let dotView = NSView()
    private let titleLabel = NSTextField(labelWithString: "")
    private let subtitleLabel = NSTextField(labelWithString: "")
    private let primaryButton = NSButton(title: "", target: nil, action: nil)
    private let secondaryButton = NSButton(title: "", target: nil, action: nil)

    public private(set) var currentState: HexMeetingPanelSnapshot = .hidden
    public private(set) var isVisible: Bool = false

    public init(runtime: HexEngineRuntime) {
        self.runtime = runtime

        let initialRect = NSRect(
            x: 0,
            y: 0,
            width: HexTuning.MeetingCapsule.width,
            height: HexTuning.MeetingCapsule.expandedHeight
        )
        let style: NSWindow.StyleMask = [.borderless, .nonactivatingPanel]
        let createdPanel = NSPanel(
            contentRect: initialRect,
            styleMask: style,
            backing: .buffered,
            defer: false
        )
        createdPanel.isFloatingPanel = true
        createdPanel.becomesKeyOnlyIfNeeded = true
        createdPanel.hidesOnDeactivate = false
        createdPanel.canHide = false
        createdPanel.isOpaque = false
        createdPanel.backgroundColor = .clear
        createdPanel.hasShadow = true
        createdPanel.level = .floating
        createdPanel.collectionBehavior = [
            .canJoinAllSpaces,
            .fullScreenAuxiliary,
            .stationary,
            .ignoresCycle,
        ]
        createdPanel.isReleasedWhenClosed = false

        let visualEffect = NSVisualEffectView(frame: initialRect)
        visualEffect.material = .hudWindow
        visualEffect.blendingMode = .withinWindow
        visualEffect.state = .active
        visualEffect.wantsLayer = true
        visualEffect.layer?.cornerRadius = HexTuning.MeetingCapsule.cornerRadius
        visualEffect.layer?.masksToBounds = true
        visualEffect.layer?.borderWidth = 1.0
        visualEffect.layer?.borderColor = NSColor.separatorColor.withAlphaComponent(0.35).cgColor
        createdPanel.contentView = visualEffect

        self.panel = createdPanel
        self.effectView = visualEffect

        super.init()

        setupSubviews()
    }

    public func update(with state: HexMeetingPanelSnapshot) {
        self.currentState = state
        switch state {
        case .hidden:
            if isVisible {
                panel.orderOut(nil)
                isVisible = false
            }
            return

        case let .offer(_, title, subtitle, sourceLabel, _):
            dotView.layer?.backgroundColor = NSColor.systemRed.cgColor
            titleLabel.stringValue = title.isEmpty ? "Meeting detected" : title
            subtitleLabel.stringValue = subtitle.isEmpty ? "\(sourceLabel) · Record this meeting?" : subtitle
            primaryButton.title = "Record"
            primaryButton.keyEquivalent = ""
            primaryButton.bezelColor = .controlAccentColor
            secondaryButton.title = "Not Now"
            secondaryButton.isHidden = false
            resizeAndPosition(height: HexTuning.MeetingCapsule.expandedHeight)

        case let .recording(title, _, elapsedText):
            dotView.layer?.backgroundColor = NSColor.systemRed.cgColor
            titleLabel.stringValue = title.isEmpty ? "Recording meeting" : title
            subtitleLabel.stringValue = "Recording · \(elapsedText)"
            primaryButton.title = "Stop"
            primaryButton.bezelColor = nil
            secondaryButton.isHidden = true
            resizeAndPosition(height: HexTuning.MeetingCapsule.compactHeight)

        case let .callEnded(_, title, subtitle):
            dotView.layer?.backgroundColor = NSColor.systemOrange.cgColor
            titleLabel.stringValue = title.isEmpty ? "Call ended" : title
            subtitleLabel.stringValue = subtitle.isEmpty ? "Call ended" : subtitle
            primaryButton.title = "Stop & Save"
            primaryButton.bezelColor = .controlAccentColor
            secondaryButton.title = "Keep Recording"
            secondaryButton.isHidden = false
            resizeAndPosition(height: HexTuning.MeetingCapsule.expandedHeight)

        case let .ready(_, title, subtitle, _, summarizing):
            dotView.layer?.backgroundColor = NSColor.systemGreen.cgColor
            titleLabel.stringValue = title.isEmpty ? "Transcript ready" : title
            subtitleLabel.stringValue = summarizing ? "Summarizing · \(subtitle)" : subtitle
            primaryButton.title = "Show in Finder"
            primaryButton.bezelColor = .controlAccentColor
            secondaryButton.title = "Dismiss"
            secondaryButton.isHidden = false
            resizeAndPosition(height: HexTuning.MeetingCapsule.expandedHeight)
        }

        if !isVisible {
            panel.orderFrontRegardless()
            isVisible = true
        }
    }

    public func close() {
        if isVisible {
            panel.orderOut(nil)
            isVisible = false
        }
    }

    private func setupSubviews() {
        dotView.wantsLayer = true
        dotView.layer?.cornerRadius = HexTuning.MeetingCapsule.statusDotDiameter / 2.0
        dotView.layer?.backgroundColor = NSColor.systemRed.cgColor
        dotView.translatesAutoresizingMaskIntoConstraints = false
        NSLayoutConstraint.activate([
            dotView.widthAnchor.constraint(equalToConstant: HexTuning.MeetingCapsule.statusDotDiameter),
            dotView.heightAnchor.constraint(equalToConstant: HexTuning.MeetingCapsule.statusDotDiameter),
        ])

        titleLabel.font = .systemFont(ofSize: 13, weight: .semibold)
        titleLabel.textColor = .labelColor
        titleLabel.lineBreakMode = .byTruncatingTail
        titleLabel.maximumNumberOfLines = 1

        subtitleLabel.font = .monospacedDigitSystemFont(ofSize: 11, weight: .regular)
        subtitleLabel.textColor = .secondaryLabelColor
        subtitleLabel.lineBreakMode = .byTruncatingTail
        subtitleLabel.maximumNumberOfLines = 1

        let textStack = NSStackView(views: [titleLabel, subtitleLabel])
        textStack.orientation = .vertical
        textStack.alignment = .leading
        textStack.spacing = 2.0

        primaryButton.bezelStyle = .rounded
        primaryButton.controlSize = .small
        primaryButton.font = .systemFont(ofSize: 11, weight: .medium)
        primaryButton.target = self
        primaryButton.action = #selector(handlePrimaryAction(_:))

        secondaryButton.bezelStyle = .rounded
        secondaryButton.controlSize = .small
        secondaryButton.font = .systemFont(ofSize: 11, weight: .regular)
        secondaryButton.target = self
        secondaryButton.action = #selector(handleSecondaryAction(_:))

        let buttonStack = NSStackView(views: [secondaryButton, primaryButton])
        buttonStack.orientation = .horizontal
        buttonStack.alignment = .centerY
        buttonStack.spacing = 6.0

        let rootRow = NSStackView(views: [dotView, textStack, NSView(), buttonStack])
        rootRow.orientation = .horizontal
        rootRow.alignment = .centerY
        rootRow.spacing = HexTuning.MeetingCapsule.interItemSpacing
        rootRow.translatesAutoresizingMaskIntoConstraints = false

        effectView.addSubview(rootRow)
        NSLayoutConstraint.activate([
            rootRow.leadingAnchor.constraint(
                equalTo: effectView.leadingAnchor,
                constant: HexTuning.MeetingCapsule.horizontalPadding
            ),
            rootRow.trailingAnchor.constraint(
                equalTo: effectView.trailingAnchor,
                constant: -HexTuning.MeetingCapsule.horizontalPadding
            ),
            rootRow.topAnchor.constraint(
                equalTo: effectView.topAnchor,
                constant: HexTuning.MeetingCapsule.verticalPadding
            ),
            rootRow.bottomAnchor.constraint(
                equalTo: effectView.bottomAnchor,
                constant: -HexTuning.MeetingCapsule.verticalPadding
            ),
        ])
    }

    private func resizeAndPosition(height: CGFloat) {
        let screen = NSScreen.main ?? NSScreen.screens.first
        let visible = screen?.visibleFrame ?? NSRect(x: 0, y: 0, width: 1440, height: 900)
        let width = HexTuning.MeetingCapsule.width
        let origin = NSPoint(
            x: visible.maxX - width - HexTuning.MeetingCapsule.rightMargin,
            y: visible.maxY - height - HexTuning.MeetingCapsule.topMargin
        )
        let frame = NSRect(origin: origin, size: NSSize(width: width, height: height))
        panel.setFrame(frame, display: true)
        effectView.frame = NSRect(origin: .zero, size: frame.size)
    }

    @objc private func handlePrimaryAction(_ sender: Any?) {
        switch currentState {
        case .offer:
            runtime.dispatch(.acceptMeetingOffer)
        case .recording, .callEnded:
            runtime.dispatch(.stopMeeting)
        case let .ready(_, _, _, exportPath, _):
            if let exportPath {
                runtime.dispatch(.revealMeetingTranscript(path: exportPath))
            }
        case .hidden:
            break
        }
    }

    @objc private func handleSecondaryAction(_ sender: Any?) {
        switch currentState {
        case .offer:
            runtime.dispatch(.dismissMeetingOffer)
        case .callEnded:
            runtime.dispatch(.dismissCallEndedPrompt)
        case .ready:
            runtime.dispatch(.simulateMeetingPanel(panel: .hidden))
        case .recording, .hidden:
            break
        }
    }
}
