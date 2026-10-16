import AppKit
import SwiftUI

public struct HexModesPane: View {
    public var store: HexSettingsStore

    public init(store: HexSettingsStore) {
        self.store = store
    }

    public var body: some View {
        let snapshot = store.runtime.snapshot
        let modes = snapshot?.settings.dictationProcessing.modes ?? []

        HSplitView {
            modesListColumn(modes: modes)
                .frame(
                    minWidth: 210,
                    idealWidth: HexTuning.StatusWindow.listColumnWidth,
                    maxWidth: 280,
                    maxHeight: .infinity
                )

            modeDetailColumn(snapshot: snapshot)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
    }

    @ViewBuilder
    private func modesListColumn(modes: [HexDictationMode]) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                Text("Modes")
                    .font(.headline)
                Spacer()
                Button {
                    store.isAddingMode.toggle()
                    store.modeValidationError = nil
                } label: {
                    Image(systemName: store.isAddingMode ? "xmark" : "plus")
                }
                .buttonStyle(.borderless)
                .help("Add Custom Mode")
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 10)

            if store.isAddingMode {
                VStack(alignment: .leading, spacing: 6) {
                    HStack(spacing: 6) {
                        TextField("Mode name (e.g. Coding)", text: Bindable(store).newModeNameDraft)
                            .textFieldStyle(.roundedBorder)
                            .onSubmit {
                                _ = store.addMode(named: store.newModeNameDraft)
                            }
                        Button("Add") {
                            _ = store.addMode(named: store.newModeNameDraft)
                        }
                        .controlSize(.small)
                        .buttonStyle(.borderedProminent)
                    }
                    if let error = store.modeValidationError {
                        Text(error)
                            .font(.caption2)
                            .foregroundStyle(.red)
                    }
                }
                .padding(.horizontal, 12)
                .padding(.bottom, 8)
            }

            Divider()

            List(selection: Bindable(store).selectedMode) {
                modeListRow(
                    title: "Global",
                    subtitle: "Default fallback for all apps",
                    icon: "globe"
                )
                .tag(HexModeSelection.global)

                if !modes.isEmpty {
                    Section("Custom Modes") {
                        ForEach(Array(modes.enumerated()), id: \.offset) { index, mode in
                            modeListRow(
                                title: mode.name,
                                subtitle: activationSummary(for: mode),
                                icon: "app.badge.checkmark"
                            )
                            .tag(HexModeSelection.custom(index))
                        }
                    }
                }
            }
            .listStyle(.sidebar)
        }
    }

    @ViewBuilder
    private func modeListRow(title: String, subtitle: String, icon: String) -> some View {
        HStack(spacing: 8) {
            Image(systemName: icon)
                .foregroundStyle(.secondary)
            VStack(alignment: .leading, spacing: 1) {
                Text(title)
                    .font(.body.weight(.medium))
                    .lineLimit(1)
                Text(subtitle)
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
        }
        .padding(.vertical, 2)
    }

    private func activationSummary(for mode: HexDictationMode) -> String {
        let combined = mode.applications + mode.browserHosts
        if combined.isEmpty {
            return "No apps or domains assigned"
        }
        return combined.joined(separator: ", ")
    }

    @ViewBuilder
    private func modeDetailColumn(snapshot: HexBootstrapSnapshot?) -> some View {
        let mode = store.currentModeValue
        let isGlobal = (store.selectedMode == .global)

        Form {
            if isGlobal {
                Section("Global Mode") {
                    Text("Global applies whenever no custom mode matches the active application or browser domain.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            } else if case let .custom(index) = store.selectedMode {
                Section("Mode Identity") {
                    TextField(
                        "Mode Name",
                        text: Binding(
                            get: { mode.name },
                            set: { newValue in
                                let trimmed = newValue.trimmingCharacters(in: .whitespacesAndNewlines)
                                guard !trimmed.isEmpty, trimmed.lowercased() != "global" else { return }
                                store.mutateCurrentMode { $0.name = trimmed }
                            }
                        )
                    )
                    HStack {
                        Spacer()
                        if store.confirmingDeleteModeIndex == index {
                            Button("Cancel") {
                                store.confirmingDeleteModeIndex = nil
                            }
                            .controlSize(.small)
                            Button("Confirm Delete Mode", role: .destructive) {
                                store.deleteCustomMode(at: index)
                            }
                            .buttonStyle(.borderedProminent)
                            .tint(.red)
                            .controlSize(.small)
                        } else {
                            Button("Delete Mode…", role: .destructive) {
                                store.confirmingDeleteModeIndex = index
                            }
                            .controlSize(.small)
                        }
                    }
                }

                activationSection(mode: mode, installedApps: snapshot?.installedApplications ?? [])
            }

            correctionsSection(mode: mode)
            rewriteSection(mode: mode, catalog: snapshot?.opencodeCatalog ?? HexOpenCodeCatalogSnapshot())
            transformationsSection(
                mode: mode,
                availableTransformations: snapshot?.commandsSnapshot.transformations ?? []
            )
        }
        .formStyle(.grouped)
    }

    @ViewBuilder
    private func activationSection(
        mode: HexDictationMode,
        installedApps: [HexInstalledApplicationSnapshot]
    ) -> some View {
        Section("Activation Rules") {
            VStack(alignment: .leading, spacing: 8) {
                HStack {
                    Text("Applications")
                        .font(.subheadline.weight(.medium))
                    Spacer()
                    Menu("Add Application…") {
                        ForEach(installedApps) { app in
                            Button(app.name) {
                                store.mutateCurrentMode { current in
                                    if !current.applications.contains(app.name) {
                                        current.applications.append(app.name)
                                        current.applications.sort()
                                    }
                                }
                            }
                        }
                        Divider()
                        Button("Choose Application from Finder…") {
                            chooseApplicationFromPanel()
                        }
                    }
                    .controlSize(.small)
                }

                if mode.applications.isEmpty {
                    Text("No applications selected.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                } else {
                    ForEach(mode.applications, id: \.self) { appName in
                        HStack {
                            Image(systemName: "app")
                                .foregroundStyle(.secondary)
                            Text(appName)
                            Spacer()
                            Button(role: .destructive) {
                                store.mutateCurrentMode { current in
                                    current.applications.removeAll { $0 == appName }
                                }
                            } label: {
                                Image(systemName: "minus.circle.fill")
                            }
                            .buttonStyle(.borderless)
                        }
                    }
                }
            }

            VStack(alignment: .leading, spacing: 8) {
                Text("Browser Domains (Brave)")
                    .font(.subheadline.weight(.medium))
                HStack(spacing: 6) {
                    TextField(
                        "Domain (e.g. github.com or linear.app)",
                        text: Bindable(store).newBrowserDomainDraft
                    )
                    .textFieldStyle(.roundedBorder)
                    .onSubmit {
                        _ = store.addBrowserDomainToCurrentMode(store.newBrowserDomainDraft)
                    }
                    Button("Add Domain") {
                        _ = store.addBrowserDomainToCurrentMode(store.newBrowserDomainDraft)
                    }
                    .controlSize(.small)
                }
                if let error = store.browserDomainError {
                    Text(error)
                        .font(.caption2)
                        .foregroundStyle(.red)
                }
                if !mode.browserHosts.isEmpty {
                    ForEach(mode.browserHosts, id: \.self) { host in
                        HStack {
                            Image(systemName: "globe")
                                .foregroundStyle(.secondary)
                            Text(host)
                                .font(.body.monospaced())
                            Spacer()
                            Button(role: .destructive) {
                                store.mutateCurrentMode { current in
                                    current.browserHosts.removeAll { $0 == host }
                                }
                            } label: {
                                Image(systemName: "minus.circle.fill")
                            }
                            .buttonStyle(.borderless)
                        }
                    }
                }
            }
        }
    }

    @ViewBuilder
    private func correctionsSection(mode: HexDictationMode) -> some View {
        Section("Corrections (Phrase Replacements)") {
            HStack {
                Text("Applied longest-match-first at phrase boundaries before AI rewriting.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                Spacer()
                Button {
                    store.mutateCurrentMode { current in
                        current.replacements.append(HexTextReplacement(matchedPhrase: "", output: ""))
                    }
                } label: {
                    Label("Add Correction", systemImage: "plus")
                }
                .controlSize(.small)
            }

            if mode.replacements.isEmpty {
                Text("No phrase replacements configured for this mode.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            } else {
                ForEach(Array(mode.replacements.enumerated()), id: \.offset) { idx, item in
                    HStack(spacing: 8) {
                        TextField(
                            "Heard phrase",
                            text: Binding(
                                get: { item.matchedPhrase },
                                set: { newVal in
                                    store.mutateCurrentMode { current in
                                        if current.replacements.indices.contains(idx) {
                                            current.replacements[idx].matchedPhrase = newVal
                                        }
                                    }
                                }
                            )
                        )
                        .textFieldStyle(.roundedBorder)

                        Image(systemName: "arrow.right")
                            .foregroundStyle(.secondary)

                        TextField(
                            "Replacement text",
                            text: Binding(
                                get: { item.output },
                                set: { newVal in
                                    store.mutateCurrentMode { current in
                                        if current.replacements.indices.contains(idx) {
                                            current.replacements[idx].output = newVal
                                        }
                                    }
                                }
                            )
                        )
                        .textFieldStyle(.roundedBorder)

                        Button(role: .destructive) {
                            store.mutateCurrentMode { current in
                                if current.replacements.indices.contains(idx) {
                                    current.replacements.remove(at: idx)
                                }
                            }
                        } label: {
                            Image(systemName: "trash")
                        }
                        .buttonStyle(.borderless)
                    }
                }
            }
        }
    }

    @ViewBuilder
    private func rewriteSection(
        mode: HexDictationMode,
        catalog: HexOpenCodeCatalogSnapshot
    ) -> some View {
        Section("OpenCode AI Rewriting") {
            Toggle(
                "Rewrite transcripts with OpenCode",
                isOn: Binding(
                    get: { mode.postProcessing.enabled },
                    set: { enabled in
                        store.mutateCurrentMode { $0.postProcessing.enabled = enabled }
                    }
                )
            )

            if catalog.state == "missing" || catalog.state == "failed" {
                HStack(spacing: 8) {
                    Image(systemName: "exclamationmark.triangle.fill")
                        .foregroundStyle(.orange)
                    Text(catalog.error ?? "OpenCode v2 is unavailable.")
                        .font(.caption)
                    Spacer()
                    Button("Retry") {
                        store.runtime.dispatch(.reloadOpenCodeCatalog)
                    }
                    .controlSize(.small)
                    Button("Install OpenCode…") {
                        store.runtime.dispatch(.openOpenCodeSetupDocs)
                    }
                    .controlSize(.small)
                }
            }

            if mode.postProcessing.enabled {
                HStack {
                    Picker(
                        "Model",
                        selection: Binding(
                            get: { mode.postProcessing.model ?? "" },
                            set: { newModel in
                                store.mutateCurrentMode { current in
                                    current.postProcessing.model = newModel.isEmpty ? nil : newModel
                                }
                            }
                        )
                    ) {
                        Text("Default (\(catalog.defaultName ?? "OpenCode Default"))").tag("")
                        ForEach(catalog.models) { choice in
                            Text("\(choice.name) (\(choice.provider))").tag(choice.key)
                        }
                    }

                    Button {
                        store.runtime.dispatch(.reloadOpenCodeCatalog)
                    } label: {
                        Image(systemName: "arrow.clockwise")
                    }
                    .help("Refresh OpenCode model catalog")
                }

                let selectedChoice = catalog.models.first(where: { $0.key == mode.postProcessing.model })
                if let variants = selectedChoice?.variants, !variants.isEmpty {
                    Picker(
                        "Thinking / Variant",
                        selection: Binding(
                            get: { mode.postProcessing.variant ?? "" },
                            set: { newVar in
                                store.mutateCurrentMode { current in
                                    current.postProcessing.variant = newVar.isEmpty ? nil : newVar
                                }
                            }
                        )
                    ) {
                        Text("Default").tag("")
                        ForEach(variants, id: \.self) { variant in
                            Text(variant).tag(variant)
                        }
                    }
                }

                VStack(alignment: .leading, spacing: 4) {
                    Text("Rewrite Instructions")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    TextField(
                        "Instructions for rewriting dictated text",
                        text: Binding(
                            get: { mode.postProcessing.prompt },
                            set: { newPrompt in
                                store.mutateCurrentMode { $0.postProcessing.prompt = newPrompt }
                            }
                        ),
                        axis: .vertical
                    )
                    .lineLimit(3...6)
                    .textFieldStyle(.roundedBorder)
                }
            }
        }
    }

    @ViewBuilder
    private func transformationsSection(
        mode: HexDictationMode,
        availableTransformations: [HexTransformationCatalogItem]
    ) -> some View {
        Section("TypeScript Transformations (@hex/commands)") {
            HStack {
                Text("Run deterministic transformations in order after corrections and AI rewrite.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                Spacer()
                Menu("Add Transformation…") {
                    ForEach(availableTransformations) { item in
                        Button(item.name) {
                            store.mutateCurrentMode { current in
                                if !current.transformations.contains(item.id) {
                                    current.transformations.append(item.id)
                                }
                            }
                        }
                    }
                }
                .controlSize(.small)
                .disabled(availableTransformations.isEmpty)
            }

            if mode.transformations.isEmpty {
                Text("No transformations selected for this mode.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            } else {
                ForEach(Array(mode.transformations.enumerated()), id: \.offset) { idx, transId in
                    let meta = availableTransformations.first(where: { $0.id == transId })
                    HStack {
                        VStack(alignment: .leading, spacing: 2) {
                            Text(meta?.name ?? transId)
                                .font(.body.weight(.medium))
                            if let desc = meta?.description, !desc.isEmpty {
                                Text(desc)
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                            }
                        }
                        Spacer()
                        Button {
                            store.mutateCurrentMode { current in
                                guard idx > 0 else { return }
                                current.transformations.swapAt(idx, idx - 1)
                            }
                        } label: {
                            Image(systemName: "arrow.up")
                        }
                        .buttonStyle(.borderless)
                        .disabled(idx == 0)

                        Button {
                            store.mutateCurrentMode { current in
                                guard idx + 1 < current.transformations.count else { return }
                                current.transformations.swapAt(idx, idx + 1)
                            }
                        } label: {
                            Image(systemName: "arrow.down")
                        }
                        .buttonStyle(.borderless)
                        .disabled(idx + 1 >= mode.transformations.count)

                        Button(role: .destructive) {
                            store.mutateCurrentMode { current in
                                if current.transformations.indices.contains(idx) {
                                    current.transformations.remove(at: idx)
                                }
                            }
                        } label: {
                            Image(systemName: "trash")
                        }
                        .buttonStyle(.borderless)
                    }
                }
            }
        }
    }

    private func chooseApplicationFromPanel() {
        let panel = NSOpenPanel()
        panel.canChooseFiles = true
        panel.canChooseDirectories = false
        panel.allowsMultipleSelection = false
        panel.directoryURL = URL(fileURLWithPath: "/Applications")
        if panel.runModal() == .OK, let url = panel.url {
            let name = url.deletingPathExtension().lastPathComponent
            guard !name.isEmpty else { return }
            store.mutateCurrentMode { current in
                if !current.applications.contains(name) {
                    current.applications.append(name)
                    current.applications.sort()
                }
            }
        }
    }
}

public struct HexVoiceActionPane: View {
    public var store: HexSettingsStore

    public init(store: HexSettingsStore) {
        self.store = store
    }

    public var body: some View {
        if let snapshot = store.runtime.snapshot {
            let settings = snapshot.settings
            let catalog = snapshot.opencodeCatalog

            Form {
                Section("Voice Action") {
                    Toggle(
                        isOn: Binding(
                            get: { snapshot.voiceActionEnabled },
                            set: { store.runtime.dispatch(.setVoiceActionEnabled(enabled: $0)) }
                        )
                    ) {
                        VStack(alignment: .leading, spacing: 2) {
                            Text("Enable Voice Action")
                            Text(
                                snapshot.voiceActionEnabled
                                    ? "Use the shortcut below with OpenCode to run spoken instructions on selected text or at the cursor."
                                    : "Off by default. Its saved shortcut is not reserved until you enable Voice Action."
                            )
                            .font(.caption)
                            .foregroundStyle(.secondary)
                        }
                    }

                    HStack {
                        VStack(alignment: .leading, spacing: 2) {
                            Text("Voice Action shortcut")
                            Text("Hold to speak an instruction; release to run and paste. Pressing Command while holding Option promotes an active dictation capture.")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                        Spacer()
                        HexShortcutRecorderControl(
                            store: store,
                            target: "edit",
                            keycaps: snapshot.editHotkeyKeycaps,
                            side: snapshot.editHotkeySide,
                            isOptional: false,
                            isDefault: settings.editHotkey == .editDefault
                        )
                    }
                }

                Section("OpenCode Processing") {
                    if catalog.state == "missing" || catalog.state == "failed" {
                        HStack(spacing: 8) {
                            Image(systemName: "exclamationmark.triangle.fill")
                                .foregroundStyle(.orange)
                            Text(catalog.error ?? "OpenCode v2 is required for Voice Action.")
                                .font(.caption)
                            Spacer()
                            Button("Retry") {
                                store.runtime.dispatch(.reloadOpenCodeCatalog)
                            }
                            .controlSize(.small)
                            Button("OpenCode Setup…") {
                                store.runtime.dispatch(.openOpenCodeSetupDocs)
                            }
                            .controlSize(.small)
                        }
                    }

                    HStack {
                        Picker(
                            "Model",
                            selection: Binding(
                                get: { settings.voiceAction.model ?? "" },
                                set: { newModel in
                                    store.runtime.dispatch(
                                        .setVoiceActionModel(
                                            model: newModel.isEmpty ? nil : newModel,
                                            variant: settings.voiceAction.variant,
                                            deadlineSeconds: settings.voiceAction.deadlineSeconds
                                        )
                                    )
                                }
                            )
                        ) {
                            Text("Default (\(catalog.defaultName ?? "OpenCode Default"))").tag("")
                            ForEach(catalog.models) { choice in
                                Text("\(choice.name) (\(choice.provider))").tag(choice.key)
                            }
                        }

                        Button {
                            store.runtime.dispatch(.reloadOpenCodeCatalog)
                        } label: {
                            Image(systemName: "arrow.clockwise")
                        }
                        .help("Refresh OpenCode models")
                    }

                    let selectedChoice = catalog.models.first(where: { $0.key == settings.voiceAction.model })
                    if let variants = selectedChoice?.variants, !variants.isEmpty {
                        Picker(
                            "Thinking / Variant",
                            selection: Binding(
                                get: { settings.voiceAction.variant ?? "" },
                                set: { newVariant in
                                    store.runtime.dispatch(
                                        .setVoiceActionModel(
                                            model: settings.voiceAction.model,
                                            variant: newVariant.isEmpty ? nil : newVariant,
                                            deadlineSeconds: settings.voiceAction.deadlineSeconds
                                        )
                                    )
                                }
                            )
                        ) {
                            Text("Default").tag("")
                            ForEach(variants, id: \.self) { variant in
                                Text(variant).tag(variant)
                            }
                        }
                    }
                }
            }
            .formStyle(.grouped)
        } else {
            ProgressView("Loading Voice Action…")
        }
    }
}
