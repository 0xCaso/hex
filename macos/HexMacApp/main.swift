import AppKit
import Darwin
import HexMacFeature

let cliExitCode = HexEngineRuntime.maybeRunCLIFromProcessArgs()
if cliExitCode >= 0 {
    Darwin.exit(cliExitCode)
}

guard HexEngineRuntime.initializeKeyboardLayoutOnMainThread() else {
    FileHandle.standardError.write(Data("hex: could not initialize the keyboard layout on the main thread\n".utf8))
    Darwin.exit(1)
}

MainActor.assumeIsolated {
    let app = NSApplication.shared
    let delegate = HexAppDelegate()
    app.delegate = delegate
    withExtendedLifetime(delegate) {
        app.run()
    }
}
