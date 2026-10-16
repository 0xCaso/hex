import AppKit
import Darwin
import HexMacFeature

let cliExitCode = HexEngineRuntime.maybeRunCLIFromProcessArgs()
if cliExitCode >= 0 {
    Darwin.exit(cliExitCode)
}

_ = HexEngineRuntime.initializeKeyboardLayoutOnMainThread()

MainActor.assumeIsolated {
    let app = NSApplication.shared
    let delegate = HexAppDelegate()
    app.delegate = delegate
    withExtendedLifetime(delegate) {
        app.run()
    }
}
