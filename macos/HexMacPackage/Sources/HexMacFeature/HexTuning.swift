import CoreGraphics
import Foundation

/// Centralized tunable constants for the native macOS Hex app.
///
/// Kept in one module without importing `KnobsKit` yet so every spring,
/// dimension, timing threshold, and shader parameter can be wired to live
/// knobs later without touching call sites.
public enum HexTuning {
    public enum HUD {
        public static let windowWidth: CGFloat = 112.0
        public static let windowHeight: CGFloat = 64.0
        public static let capsuleWidth: Float = 56.0
        public static let capsuleHeight: Float = 16.0
        public static let topOffset: CGFloat = 12.0

        public static let entranceAngularFrequency: Float = 24.0
        public static let exitAngularFrequency: Float = 24.0
        public static let geometryAngularFrequency: Float = 24.0
        public static let averageAngularFrequency: Float = 22.0
        public static let peakAngularFrequency: Float = 26.0

        public static let hiddenScale: Float = 0.82
        public static let hiddenSoftness: Float = 4.0

        public static let processingMorphDuration: TimeInterval = 0.250
        public static let completedVisibleDuration: TimeInterval = 0.240
        public static let failedVisibleDuration: TimeInterval = 0.600
        public static let recordingFlashHalfLife: Float = 0.280

        public static let averageGain: Float = 9.0
        public static let peakGain: Float = 3.0
        public static let peakDecayPer60HzFrame: Float = 0.91

        public static let lineStyle: Float = 1.0
        public static let lineCount: Float = 2.0
        public static let lineCurvature: Float = 0.47
        public static let lineSpeed: Float = 17.08
        public static let lineSharpness: Float = 0.29
        public static let lineGlow: Float = 0.77
        public static let sphereDepth: Float = 0.65
        public static let lightAngle: Float = 0.35
        public static let sphereOutline: Float = 1.0
    }

    public enum MeetingCapsule {
        public static let width: CGFloat = 336.0
        public static let compactHeight: CGFloat = 56.0
        public static let expandedHeight: CGFloat = 88.0
        public static let cornerRadius: CGFloat = 14.0
        public static let topMargin: CGFloat = 14.0
        public static let rightMargin: CGFloat = 18.0
        public static let horizontalPadding: CGFloat = 14.0
        public static let verticalPadding: CGFloat = 11.0
        public static let interItemSpacing: CGFloat = 8.0
        public static let statusDotDiameter: CGFloat = 8.0
    }

    public enum StatusWindow {
        public static let defaultWidth: CGFloat = 720.0
        public static let defaultHeight: CGFloat = 520.0
        public static let minWidth: CGFloat = 640.0
        public static let minHeight: CGFloat = 440.0
        public static let contentPadding: CGFloat = 20.0
        public static let cardCornerRadius: CGFloat = 10.0
    }
}
