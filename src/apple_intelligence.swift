import Foundation
#if canImport(FoundationModels)
import FoundationModels
#endif

private struct BridgeActionItem: Codable {
    let task: String
    let owner: String
}

private struct BridgeNotes: Codable {
    let title: String
    let summary: [String]
    let actionItems: [BridgeActionItem]
    let decisions: [String]
}

private struct BridgeError: LocalizedError {
    let message: String

    init(_ message: String) {
        self.message = message
    }

    var errorDescription: String? { message }
}

/// Distinguishes an oversized prompt so Rust can split the chunk once.
private let contextWindowMarker = "context-window:"

#if canImport(FoundationModels)
@available(macOS 26.0, *)
@Generable
private struct GeneratedActionItem {
    @Guide(description: "One concrete follow-up task as a short imperative, without the owner's name")
    let task: String
    @Guide(description: "The person responsible exactly as stated, You for the recording user, or an empty string when no owner was stated")
    let owner: String
}

@available(macOS 26.0, *)
@Generable
private struct GeneratedNotes {
    @Guide(description: "A short, specific meeting title of at most eight words, without dates or the word meeting")
    let title: String
    @Guide(description: "The main topics discussed and their outcomes, one short sentence each, not repeating action items", .maximumCount(5))
    let summary: [String]
    @Guide(description: "Every follow-up task someone agreed to do, including the recording person (You)", .maximumCount(8))
    let actionItems: [GeneratedActionItem]
    @Guide(description: "Decisions the participants agreed on", .maximumCount(5))
    let decisions: [String]
}

@available(macOS 26.0, *)
private func generate(instructions: String, prompt: String) async throws -> BridgeNotes {
    let model = SystemLanguageModel(
        useCase: .general,
        guardrails: .permissiveContentTransformations
    )
    let session = LanguageModelSession(model: model, instructions: instructions)
    do {
        let response = try await session.respond(
            to: prompt,
            generating: GeneratedNotes.self,
            options: GenerationOptions(temperature: 0.2, maximumResponseTokens: 900)
        )
        let notes = response.content
        return BridgeNotes(
            title: notes.title,
            summary: notes.summary,
            actionItems: notes.actionItems.map {
                BridgeActionItem(task: $0.task, owner: $0.owner)
            },
            decisions: notes.decisions
        )
    } catch LanguageModelSession.GenerationError.exceededContextWindowSize(let context) {
        throw BridgeError("\(contextWindowMarker) \(context.debugDescription)")
    }
}
#endif

private final class BlockingResult<Value: Sendable>: @unchecked Sendable {
    private let lock = NSLock()
    private var value: Result<Value, Error>?

    func store(_ value: Result<Value, Error>) {
        lock.withLock { self.value = value }
    }

    func take() -> Result<Value, Error> {
        lock.withLock { value! }
    }
}

private func blocking<Value: Sendable>(
    timeout: TimeInterval,
    _ operation: @escaping @Sendable () async throws -> Value
) -> Result<Value, Error> {
    let semaphore = DispatchSemaphore(value: 0)
    let result = BlockingResult<Value>()
    let task = Task.detached(priority: .utility) {
        do {
            result.store(.success(try await operation()))
        } catch {
            result.store(.failure(error))
        }
        semaphore.signal()
    }
    guard semaphore.wait(timeout: .now() + timeout) == .success else {
        task.cancel()
        return .failure(BridgeError("Apple Intelligence timed out."))
    }
    return result.take()
}

private func writeError(
    _ error: Error,
    to destination: UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>?
) {
    destination?.pointee = strdup(
        (error as? LocalizedError)?.errorDescription ?? error.localizedDescription
    )
}

/// 0 available, 1 device not eligible, 2 Apple Intelligence off,
/// 3 model not ready, 4 unsupported macOS, 5 unknown reason.
@_cdecl("hex_apple_intelligence_availability")
public func hexAppleIntelligenceAvailability() -> Int32 {
    #if canImport(FoundationModels)
    guard #available(macOS 26.0, *) else { return 4 }
    switch SystemLanguageModel.default.availability {
    case .available:
        return 0
    case .unavailable(.deviceNotEligible):
        return 1
    case .unavailable(.appleIntelligenceNotEnabled):
        return 2
    case .unavailable(.modelNotReady):
        return 3
    case .unavailable:
        return 5
    }
    #else
    return 4
    #endif
}

@_cdecl("hex_apple_intelligence_context_size")
public func hexAppleIntelligenceContextSize() -> Int {
    #if canImport(FoundationModels)
    guard #available(macOS 26.0, *) else { return 0 }
    return SystemLanguageModel.default.contextSize
    #else
    return 0
    #endif
}

@_cdecl("hex_apple_intelligence_notes")
public func hexAppleIntelligenceNotes(
    _ instructions: UnsafePointer<CChar>?,
    _ prompt: UnsafePointer<CChar>?,
    _ timeoutMs: UInt64,
    _ errorOutput: UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>?
) -> UnsafeMutablePointer<CChar>? {
    #if canImport(FoundationModels)
    guard #available(macOS 26.0, *), let instructions, let prompt else {
        writeError(BridgeError("Apple Intelligence requires macOS 26."), to: errorOutput)
        return nil
    }
    let instructionText = String(cString: instructions)
    let promptText = String(cString: prompt)
    let result = blocking(timeout: TimeInterval(timeoutMs) / 1_000) {
        try await generate(instructions: instructionText, prompt: promptText)
    }
    switch result {
    case .success(let notes):
        do {
            let data = try JSONEncoder().encode(notes)
            return strdup(String(decoding: data, as: UTF8.self))
        } catch {
            writeError(error, to: errorOutput)
            return nil
        }
    case .failure(let failure):
        writeError(failure, to: errorOutput)
        return nil
    }
    #else
    writeError(BridgeError("Apple Intelligence requires macOS 26."), to: errorOutput)
    return nil
    #endif
}

@_cdecl("hex_apple_intelligence_free_string")
public func hexAppleIntelligenceFreeString(_ string: UnsafeMutablePointer<CChar>?) {
    free(string)
}
