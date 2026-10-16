import AppKit
import Foundation

/// Safe wrapper around `Sparkle.framework`'s `SPUStandardUpdaterController`.
///
/// Loads `Sparkle.framework` from `Bundle.main.privateFrameworksPath` when
/// running inside a packaged `.app` bundle (matching `src/sparkle.rs`), and
/// degrades cleanly in `swift test` or unpackaged development builds.
@MainActor
public final class SparkleController: NSObject {
    private var updaterController: NSObject?
    public private(set) var isAvailable: Bool = false

    public override init() {
        super.init()
        configureIfPackagedApp()
    }

    private func configureIfPackagedApp() {
        guard Bundle.main.bundleURL.pathExtension == "app" else {
            return
        }
        if let privateFrameworks = Bundle.main.privateFrameworksPath {
            let sparklePath = (privateFrameworks as NSString).appendingPathComponent("Sparkle.framework")
            if FileManager.default.fileExists(atPath: sparklePath) {
                Bundle(path: sparklePath)?.load()
            }
        }
        guard let controllerClass = NSClassFromString("SPUStandardUpdaterController") as? NSObject.Type else {
            return
        }
        // SPUStandardUpdaterController.alloc().initWithStartingUpdater(true, updaterDelegate: nil, userDriverDelegate: nil)
        let initSel = NSSelectorFromString("initWithStartingUpdater:updaterDelegate:userDriverDelegate:")
        guard controllerClass.instancesRespond(to: initSel) else {
            return
        }
        typealias InitIMP = @convention(c) (AnyObject, Selector, ObjCBool, AnyObject?, AnyObject?) -> AnyObject?
        let allocated = controllerClass.perform(NSSelectorFromString("alloc"))?.takeUnretainedValue() as AnyObject?
        guard let allocated,
              let method = class_getInstanceMethod(controllerClass, initSel)
        else {
            return
        }
        let imp = unsafeBitCast(method_getImplementation(method), to: InitIMP.self)
        if let instance = imp(allocated, initSel, ObjCBool(true), nil, nil) as? NSObject {
            self.updaterController = instance
            self.isAvailable = true
        }
    }

    public func checkForUpdates() {
        guard let controller = updaterController else {
            return
        }
        let sel = NSSelectorFromString("checkForUpdates:")
        if controller.responds(to: sel) {
            controller.perform(sel, with: nil)
        }
    }
}
