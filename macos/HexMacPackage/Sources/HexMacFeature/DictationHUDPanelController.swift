import AppKit
import Foundation
import Metal
import QuartzCore

private struct CriticallyDampedSpring {
    var value: Float
    var velocity: Float = 0.0

    init(_ initial: Float) {
        self.value = initial
    }

    mutating func reset(_ newValue: Float) {
        self.value = newValue
        self.velocity = 0.0
    }

    mutating func step(target: Float, dt: Float, angularFrequency: Float) {
        let displacement = value - target
        let coefficient = velocity + angularFrequency * displacement
        let decay = Foundation.exp(-angularFrequency * dt)
        value = target + (displacement + coefficient * dt) * decay
        velocity = (velocity - angularFrequency * coefficient * dt) * decay
    }

    func isSettled(target: Float, epsilon: Float, angularFrequency: Float) -> Bool {
        abs(value - target) <= epsilon && abs(velocity) <= epsilon * angularFrequency
    }
}

/// 112-byte Metal uniform block matching `Uniforms` in `src/dictation_indicator.metal`.
public struct HUDShaderUniforms: Sendable, Equatable {
    public var resolution: SIMD2<Float>
    public var time: Float
    public var width: Float
    public var height: Float
    public var opacity: Float
    public var scale: Float
    public var softness: Float
    public var average: Float
    public var peak: Float
    public var processing: Float
    public var postProcessing: Float
    public var capturing: Float
    public var editing: Float
    public var queuedCount: Float
    public var lineStyle: Float
    public var lineCount: Float
    public var lineCurvature: Float
    public var lineSpeed: Float
    public var lineSharpness: Float
    public var lineGlow: Float
    public var sphereDepth: Float
    public var lightAngle: Float
    public var sphereOutline: Float
    public var completion: Float
    public var recordingFlash: Float
    public var padding: Float
    public var tailAlignPadding: Float = 0.0
}

@MainActor
public final class DictationHUDPanelController: NSObject {
    public static let expectedUniformsSize = 112

    private let runtime: HexEngineRuntime
    private let panel: NSPanel
    private let hostView: NSView
    private let metalLayer: CAMetalLayer
    private let device: MTLDevice?
    private let commandQueue: MTLCommandQueue?
    private let pipelineState: MTLRenderPipelineState?
    private var displayLink: CADisplayLink?

    public private(set) var isPanelOrderedFront: Bool = false

    private var lastPhase: HexHudPhase = .hidden
    private var phaseStartedAt: TimeInterval = ProcessInfo.processInfo.systemUptime
    private var renderStartedAt: TimeInterval = ProcessInfo.processInfo.systemUptime
    private var lastFrameAt: TimeInterval = ProcessInfo.processInfo.systemUptime
    private var exiting: Bool = true
    private var backingScale: Float = 2.0

    private var targetAverage: Float = 0.0
    private var targetPeak: Float = 0.0

    private var averageSpring = CriticallyDampedSpring(0.0)
    private var peakSpring = CriticallyDampedSpring(0.0)
    private var widthSpring = CriticallyDampedSpring(HexTuning.HUD.capsuleWidth)
    private var opacitySpring = CriticallyDampedSpring(0.0)
    private var visualScaleSpring = CriticallyDampedSpring(HexTuning.HUD.hiddenScale)
    private var softnessSpring = CriticallyDampedSpring(HexTuning.HUD.hiddenSoftness)
    private var processingSpring = CriticallyDampedSpring(0.0)
    private var postProcessingSpring = CriticallyDampedSpring(0.0)

    public init(runtime: HexEngineRuntime) {
        self.runtime = runtime

        let frame = NSRect(
            x: 0,
            y: 0,
            width: HexTuning.HUD.windowWidth,
            height: HexTuning.HUD.windowHeight
        )
        let style: NSWindow.StyleMask = [
            .borderless,
            .utilityWindow,
            .nonactivatingPanel,
            .fullSizeContentView,
        ]
        let createdPanel = NSPanel(
            contentRect: frame,
            styleMask: style,
            backing: .buffered,
            defer: false
        )
        createdPanel.backgroundColor = .clear
        createdPanel.isOpaque = false
        createdPanel.ignoresMouseEvents = true
        createdPanel.hasShadow = false
        createdPanel.hidesOnDeactivate = false
        createdPanel.canHide = false
        createdPanel.level = .statusBar
        createdPanel.collectionBehavior = [
            .canJoinAllSpaces,
            .canJoinAllApplications,
            .fullScreenAuxiliary,
            .stationary,
            .ignoresCycle,
        ]
        createdPanel.isReleasedWhenClosed = false

        let view = NSView(frame: frame)
        view.wantsLayer = true
        let layer = CAMetalLayer()
        layer.pixelFormat = .bgra8Unorm
        layer.isOpaque = false
        layer.presentsWithTransaction = false
        layer.displaySyncEnabled = true
        layer.contentsScale = 2.0
        layer.drawableSize = CGSize(
            width: HexTuning.HUD.windowWidth * 2.0,
            height: HexTuning.HUD.windowHeight * 2.0
        )
        view.layer = layer
        createdPanel.contentView = view

        self.panel = createdPanel
        self.hostView = view
        self.metalLayer = layer

        let mtlDevice = MTLCreateSystemDefaultDevice()
        self.device = mtlDevice
        self.commandQueue = mtlDevice?.makeCommandQueue()
        if let mtlDevice {
            layer.device = mtlDevice
            self.pipelineState = Self.makePipelineState(device: mtlDevice)
        } else {
            self.pipelineState = nil
        }

        super.init()

        positionOnPointerScreen()
    }

    public func handleTelemetryUpdate(_ telemetry: HexHudTelemetry) {
        let phaseChanged = telemetry.phase != lastPhase
        syncPhaseTransitions(with: telemetry, now: ProcessInfo.processInfo.systemUptime)
        let isActiveCaptureOrTranscribe =
            telemetry.phase == .listening
            || telemetry.phase == .locked
            || telemetry.phase == .voiceAction
            || telemetry.phase == .transcribing
        if isActiveCaptureOrTranscribe || (phaseChanged && telemetry.phase != .hidden) {
            ensureActiveDisplayLink()
        }
    }

    public func stop() {
        displayLink?.isPaused = true
        displayLink?.invalidate()
        displayLink = nil
        if isPanelOrderedFront {
            panel.orderOut(nil)
            isPanelOrderedFront = false
        }
    }

    private func ensureActiveDisplayLink() {
        positionOnPointerScreen()
        if !isPanelOrderedFront {
            panel.orderFrontRegardless()
            isPanelOrderedFront = true
        }
        if displayLink == nil {
            let link = hostView.displayLink(target: self, selector: #selector(handleDisplayLinkFrame(_:)))
            link.add(to: .main, forMode: .common)
            self.displayLink = link
        }
        lastFrameAt = ProcessInfo.processInfo.systemUptime
        displayLink?.isPaused = false
    }

    @objc private func handleDisplayLinkFrame(_ link: CADisplayLink) {
        let now = ProcessInfo.processInfo.systemUptime
        let telemetry = runtime.readHudTelemetry()
        syncPhaseTransitions(with: telemetry, now: now)
        let keepRunning = renderFrame(telemetry: telemetry, now: now)
        if keepRunning {
            positionOnPointerScreen()
            if !isPanelOrderedFront {
                panel.orderFrontRegardless()
                isPanelOrderedFront = true
            }
        } else {
            displayLink?.isPaused = true
            if isPanelOrderedFront {
                panel.orderOut(nil)
                isPanelOrderedFront = false
            }
        }
    }

    private func syncPhaseTransitions(with telemetry: HexHudTelemetry, now: TimeInterval) {
        targetAverage = min(max(telemetry.rmsLevel * HexTuning.HUD.averageGain, 0.0), 1.0)
        targetPeak = max(targetPeak, min(max(telemetry.peakLevel * HexTuning.HUD.peakGain, 0.0), 1.0))

        guard telemetry.phase != lastPhase else { return }
        let previous = lastPhase
        lastPhase = telemetry.phase
        phaseStartedAt = now

        switch telemetry.phase {
        case .listening, .locked, .voiceAction:
            if previous == .hidden || previous == .completed || previous == .cancelled || previous == .error {
                renderStartedAt = now
                lastFrameAt = now
                exiting = false
                averageSpring.reset(0.0)
                peakSpring.reset(0.0)
                widthSpring.reset(HexTuning.HUD.capsuleWidth)
                opacitySpring.reset(0.0)
                visualScaleSpring.reset(HexTuning.HUD.hiddenScale)
                softnessSpring.reset(HexTuning.HUD.hiddenSoftness)
                processingSpring.reset(0.0)
                postProcessingSpring.reset(0.0)
            } else {
                exiting = false
            }
        case .transcribing:
            exiting = false
            targetAverage = 0.0
            targetPeak = 0.0
        case .completed:
            exiting = false
            widthSpring.reset(HexTuning.HUD.capsuleHeight)
            processingSpring.reset(1.0)
            freezeVisualVelocities()
        case .cancelled, .error, .hidden:
            freezeVisualVelocities()
        }
    }

    private func freezeVisualVelocities() {
        targetAverage = averageSpring.value
        targetPeak = peakSpring.value
        averageSpring.velocity = 0.0
        peakSpring.velocity = 0.0
        widthSpring.velocity = 0.0
        processingSpring.velocity = 0.0
        postProcessingSpring.velocity = 0.0
    }

    private func renderFrame(telemetry: HexHudTelemetry, now: TimeInterval) -> Bool {
        let dt = Float(min(max(now - lastFrameAt, 1.0 / 240.0), 1.0 / 20.0))
        lastFrameAt = now
        let elapsed = max(now - phaseStartedAt, 0.0)

        let terminalFinished: Bool
        switch telemetry.phase {
        case .completed:
            terminalFinished = elapsed >= HexTuning.HUD.completedVisibleDuration
        case .cancelled:
            terminalFinished = true
        case .error:
            terminalFinished = elapsed >= HexTuning.HUD.failedVisibleDuration
        case .hidden:
            terminalFinished = true
        case .listening, .locked, .voiceAction, .transcribing:
            terminalFinished = false
        }

        let visible = telemetry.phase != .hidden && !terminalFinished
        if !visible && !exiting {
            opacitySpring.velocity = 0.0
            visualScaleSpring.velocity = 0.0
            softnessSpring.velocity = 0.0
            exiting = true
        }

        let targetWidth: Float
        let targetProcessing: Float
        switch telemetry.phase {
        case .listening, .locked, .voiceAction:
            targetWidth = HexTuning.HUD.capsuleWidth
            targetProcessing = 0.0
        case .transcribing:
            targetWidth = HexTuning.HUD.capsuleHeight
            targetProcessing = 1.0
        case .hidden, .completed, .cancelled, .error:
            targetWidth = widthSpring.value
            targetProcessing = processingSpring.value
        }
        let targetPostProcessing: Float = telemetry.isPostProcessing ? 1.0 : 0.0

        averageSpring.step(
            target: targetAverage,
            dt: dt,
            angularFrequency: HexTuning.HUD.averageAngularFrequency
        )
        peakSpring.step(
            target: targetPeak,
            dt: dt,
            angularFrequency: HexTuning.HUD.peakAngularFrequency
        )
        widthSpring.step(
            target: targetWidth,
            dt: dt,
            angularFrequency: HexTuning.HUD.geometryAngularFrequency
        )
        processingSpring.step(
            target: targetProcessing,
            dt: dt,
            angularFrequency: HexTuning.HUD.geometryAngularFrequency
        )
        postProcessingSpring.step(
            target: targetPostProcessing,
            dt: dt,
            angularFrequency: HexTuning.HUD.geometryAngularFrequency
        )

        let visibilityFreq = visible
            ? HexTuning.HUD.entranceAngularFrequency
            : HexTuning.HUD.exitAngularFrequency
        opacitySpring.step(
            target: visible ? 1.0 : 0.0,
            dt: dt,
            angularFrequency: visibilityFreq
        )
        visualScaleSpring.step(
            target: visible ? 1.0 : HexTuning.HUD.hiddenScale,
            dt: dt,
            angularFrequency: visibilityFreq
        )
        softnessSpring.step(
            target: visible ? 0.0 : HexTuning.HUD.hiddenSoftness,
            dt: dt,
            angularFrequency: visibilityFreq
        )
        targetPeak *= Foundation.pow(HexTuning.HUD.peakDecayPer60HzFrame, dt * 60.0)

        if let commandQueue,
           let pipelineState,
           let drawable = metalLayer.nextDrawable()
        {
            let pass = MTLRenderPassDescriptor()
            pass.colorAttachments[0].texture = drawable.texture
            pass.colorAttachments[0].loadAction = .clear
            pass.colorAttachments[0].clearColor = MTLClearColor(red: 0, green: 0, blue: 0, alpha: 0)
            pass.colorAttachments[0].storeAction = .store

            if let commandBuffer = commandQueue.makeCommandBuffer(),
               let encoder = commandBuffer.makeRenderCommandEncoder(descriptor: pass)
            {
                let isRecordingPhase = telemetry.phase == .listening || telemetry.phase == .locked || telemetry.phase == .voiceAction
                let recordingFlash: Float = (isRecordingPhase && !telemetry.isVoiceAction)
                    ? Foundation.pow(2.0, -Float(elapsed) / HexTuning.HUD.recordingFlashHalfLife)
                    : 0.0
                let completionProgress: Float = telemetry.phase == .completed
                    ? min(max(Float(elapsed / HexTuning.HUD.completedVisibleDuration), 0.0), 1.0)
                    : 0.0
                let queuedCount = Float(telemetry.activeQueueCount)

                var uniforms = HUDShaderUniforms(
                    resolution: SIMD2<Float>(
                        Float(HexTuning.HUD.windowWidth) * backingScale,
                        Float(HexTuning.HUD.windowHeight) * backingScale
                    ),
                    time: Float(max(now - renderStartedAt, 0.0)),
                    width: widthSpring.value,
                    height: HexTuning.HUD.capsuleHeight,
                    opacity: min(max(opacitySpring.value, 0.0), 1.0),
                    scale: max(visualScaleSpring.value, 0.01),
                    softness: min(max(softnessSpring.value, 0.0), HexTuning.HUD.hiddenSoftness),
                    average: min(max(averageSpring.value, 0.0), 1.0),
                    peak: min(max(peakSpring.value, 0.0), 1.0),
                    processing: min(max(processingSpring.value, 0.0), 1.0),
                    postProcessing: min(max(postProcessingSpring.value, 0.0), 1.0),
                    capturing: telemetry.isCapturing ? 1.0 : 0.0,
                    editing: telemetry.isVoiceAction ? 1.0 : 0.0,
                    queuedCount: queuedCount,
                    lineStyle: HexTuning.HUD.lineStyle,
                    lineCount: HexTuning.HUD.lineCount,
                    lineCurvature: HexTuning.HUD.lineCurvature,
                    lineSpeed: HexTuning.HUD.lineSpeed,
                    lineSharpness: HexTuning.HUD.lineSharpness,
                    lineGlow: HexTuning.HUD.lineGlow,
                    sphereDepth: HexTuning.HUD.sphereDepth,
                    lightAngle: HexTuning.HUD.lightAngle,
                    sphereOutline: HexTuning.HUD.sphereOutline,
                    completion: completionProgress,
                    recordingFlash: recordingFlash,
                    padding: 0.0
                )

                encoder.setRenderPipelineState(pipelineState)
                encoder.setFragmentBytes(
                    &uniforms,
                    length: MemoryLayout<HUDShaderUniforms>.stride,
                    index: 0
                )
                encoder.drawPrimitives(type: .triangle, vertexStart: 0, vertexCount: 3)
                encoder.endEncoding()
                commandBuffer.present(drawable)
                commandBuffer.commit()
            }
        }

        let exitFreq = HexTuning.HUD.exitAngularFrequency
        return visible
            || !opacitySpring.isSettled(target: 0.0, epsilon: 0.002, angularFrequency: exitFreq)
            || !visualScaleSpring.isSettled(target: HexTuning.HUD.hiddenScale, epsilon: 0.002, angularFrequency: exitFreq)
            || !softnessSpring.isSettled(target: HexTuning.HUD.hiddenSoftness, epsilon: 0.02, angularFrequency: exitFreq)
    }

    private func positionOnPointerScreen() {
        let pointer = NSEvent.mouseLocation
        let screen = NSScreen.screens.first(where: { NSMouseInRect(pointer, $0.frame, false) })
            ?? NSScreen.main
            ?? NSScreen.screens.first
        guard let screen else { return }

        let screenFrame = screen.frame
        let visible = screen.visibleFrame
        let capsuleMargin = (HexTuning.HUD.windowHeight - CGFloat(HexTuning.HUD.capsuleHeight)) / 2.0
        let topLeft = NSPoint(
            x: screenFrame.origin.x + (screenFrame.size.width - HexTuning.HUD.windowWidth) / 2.0,
            y: visible.origin.y + visible.size.height + capsuleMargin - HexTuning.HUD.topOffset
        )
        let currentFrame = panel.frame
        if abs(currentFrame.origin.x - topLeft.x) > 0.5
            || abs(currentFrame.origin.y + currentFrame.size.height - topLeft.y) > 0.5
        {
            panel.setFrameTopLeftPoint(topLeft)
        }
        let scale = Float(screen.backingScaleFactor)
        if abs(scale - backingScale) > .ulpOfOne {
            backingScale = scale
            metalLayer.contentsScale = CGFloat(scale)
            metalLayer.drawableSize = CGSize(
                width: HexTuning.HUD.windowWidth * CGFloat(scale),
                height: HexTuning.HUD.windowHeight * CGFloat(scale)
            )
        }
    }

    private static func makePipelineState(device: MTLDevice) -> MTLRenderPipelineState? {
        let library = device.makeDefaultLibrary()
            ?? (try? device.makeLibrary(source: embeddedIndicatorShaderSource, options: nil))
        guard let library,
              let vertexFn = library.makeFunction(name: "indicator_vertex"),
              let fragmentFn = library.makeFunction(name: "indicator_fragment")
        else {
            return nil
        }
        let descriptor = MTLRenderPipelineDescriptor()
        descriptor.vertexFunction = vertexFn
        descriptor.fragmentFunction = fragmentFn
        descriptor.colorAttachments[0].pixelFormat = .bgra8Unorm
        return try? device.makeRenderPipelineState(descriptor: descriptor)
    }

    private static let embeddedIndicatorShaderSource = #"""
    #include <metal_stdlib>
    using namespace metal;

    struct Uniforms {
        float2 resolution;
        float time;
        float width;
        float height;
        float opacity;
        float scale;
        float softness;
        float average;
        float peak;
        float processing;
        float post_processing;
        float capturing;
        float editing;
        float queued_count;
        float line_style;
        float line_count;
        float line_curvature;
        float line_speed;
        float line_sharpness;
        float line_glow;
        float sphere_depth;
        float light_angle;
        float sphere_outline;
        float completion;
        float recording_flash;
        float _padding;
    };

    struct VertexOutput {
        float4 position [[position]];
        float2 uv;
    };

    vertex VertexOutput indicator_vertex(uint vertex_id [[vertex_id]]) {
        const float2 positions[] = {
            float2(-1.0, -1.0),
            float2( 3.0, -1.0),
            float2(-1.0,  3.0),
        };
        VertexOutput output;
        output.position = float4(positions[vertex_id], 0.0, 1.0);
        output.uv = positions[vertex_id] * float2(0.5, -0.5) + 0.5;
        return output;
    }

    float rounded_box(float2 point, float2 half_size, float radius) {
        float2 q = abs(point) - half_size + radius;
        return length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - radius;
    }

    float coverage(float distance, float softness) {
        return smoothstep(softness, -softness, distance);
    }

    float glow(float distance, float radius) {
        float outside = max(distance, 0.0);
        return exp2(-outside * outside / max(radius * radius, 0.001));
    }

    void composite(thread float4 &destination, float3 color, float alpha) {
        alpha = clamp(alpha, 0.0, 1.0);
        destination.rgb = color * alpha + destination.rgb * (1.0 - alpha);
        destination.a = alpha + destination.a * (1.0 - alpha);
    }

    void screen(thread float4 &destination, float3 color, float amount) {
        amount = clamp(amount, 0.0, 1.0);
        destination.rgb = 1.0 - (1.0 - destination.rgb) * (1.0 - color * amount);
    }

    fragment float4 indicator_fragment(
        VertexOutput input [[stage_in]],
        constant Uniforms &uniforms [[buffer(0)]])
    {
        float backing_scale = uniforms.resolution.x / 112.0;
        float2 point = (input.uv * uniforms.resolution - uniforms.resolution * 0.5)
            / backing_scale / max(uniforms.scale, 0.001);
        float2 half_size = float2(uniforms.width, uniforms.height) * 0.5;
        float radius = uniforms.height * 0.5;
        float distance = rounded_box(point, half_size, radius);
        float lifecycle_softness = max(uniforms.softness, 0.0);
        float edge = (max(fwidth(distance), 0.32) + lifecycle_softness * 0.72)
            / max(uniforms.scale, 0.001);
        float shape = coverage(distance, edge);
        float detail_clarity = exp2(-lifecycle_softness * 0.34);
        float4 result = 0.0;

        float processing = clamp(uniforms.processing, 0.0, 1.0);
        float post_processing = clamp(uniforms.post_processing, 0.0, 1.0);
        float editing = clamp(uniforms.editing, 0.0, 1.0);
        float completion = clamp(uniforms.completion, 0.0, 1.0);
        float recording_flash = clamp(uniforms.recording_flash, 0.0, 1.0)
            * (1.0 - processing);
        float completion_flash = smoothstep(0.0, 0.16, completion)
            * (1.0 - smoothstep(0.32, 1.0, completion));
        float recording = 1.0 - processing;
        float circularity = 1.0 - smoothstep(0.5, 2.25, abs(uniforms.width - uniforms.height));

        float3 red_accent = mix(
            float3(1.0, 0.025, 0.035),
            float3(0.05, 0.85, 0.58),
            editing);
        float3 blue_accent = float3(0.1, 0.34, 1.0);
        float3 violet_accent = float3(0.64, 0.2, 1.0);
        float3 pipeline_accent = mix(blue_accent, violet_accent, post_processing);
        float3 accent = mix(red_accent, pipeline_accent, processing);

        float average_power = clamp(uniforms.average, 0.0, 1.0);
        float peak_power = clamp(uniforms.peak, 0.0, 1.0);
        float average_activation = smoothstep(0.0, 0.1, average_power);
        float outer = recording * (
            glow(distance, 4.0 + lifecycle_softness * 0.5) * average_power * 0.72
            + glow(distance, 8.0 + lifecycle_softness) * average_power * 0.36);
        outer += processing * glow(distance, 2.6 + lifecycle_softness) * 0.045;
        outer += glow(distance, 6.0 + lifecycle_softness * 0.5) * completion_flash * 0.22;
        outer += glow(distance, 7.0 + lifecycle_softness * 0.5) * recording_flash * 0.5;
        composite(result, accent, outer);

        float2 shadow_point = point - float2(0.0, 1.5);
        float shadow_distance = rounded_box(shadow_point, half_size, radius);
        float shadow = glow(shadow_distance, 5.5 + lifecycle_softness * 1.4) * (1.0 - shape) * 0.28;
        composite(result, float3(0.0), shadow);

        float vertical = clamp((point.y + half_size.y) / max(uniforms.height, 1.0), 0.0, 1.0);
        float3 base = mix(float3(0.047, 0.047, 0.055), float3(0.014, 0.014, 0.018), vertical);
        base += accent * (recording * (0.045 + average_power * 0.18) + processing * 0.05);
        composite(result, base, shape * 0.96);

        float2 normalized = point / max(half_size, float2(1.0));
        float radial = length(normalized);
        if (recording > 0.001) {
            float meter = clamp(average_power * 0.65 + peak_power * 0.55, 0.0, 1.0);
            float envelope = smoothstep(1.0, 0.08, abs(normalized.x));
            float wave1 = sin(normalized.x * 4.2 + uniforms.time * 7.5) * 0.35;
            float wave2 = cos(normalized.x * 7.1 - uniforms.time * 10.2) * 0.22;
            float target_y = (wave1 + wave2) * envelope * (0.14 + meter * 0.72);
            float line_dist = abs(normalized.y - target_y) * half_size.y;
            float core = smoothstep(1.35, 0.0, line_dist);
            float halo = exp2(-line_dist * line_dist / 5.5) * (0.25 + meter * 0.65);
            screen(result, mix(accent, float3(1.0), 0.45), (core * 0.85 + halo * 0.55) * shape * recording * detail_clarity);
        }

        if (circularity > 0.001) {
            float r = length(point) / max(radius, 1.0);
            float angle = uniforms.light_angle * 6.2831853;
            float2 light_dir = float2(cos(angle), sin(angle));
            float3 normal = float3(
                point / max(radius, 1.0),
                sqrt(max(1.0 - r * r, 0.0)));
            float4 orb = 0.0;
            for (int idx = 0; idx < 3; idx++) {
                if (float(idx) >= uniforms.line_count) { break; }
                float phase = uniforms.time * (uniforms.line_speed * 0.18) + float(idx) * 2.0943951;
                float2 dir = float2(cos(phase), sin(phase));
                float band = dot(normal.xy, dir) + normal.z * uniforms.line_curvature * sin(phase * 0.7);
                float width_px = mix(2.2, 0.45, clamp(uniforms.line_sharpness, 0.0, 1.0));
                float arc = exp2(-(band * band * 18.0) / max(width_px, 0.15));
                screen(orb, mix(accent, float3(1.0), 0.38), arc * uniforms.line_glow * 0.62);
            }
            float rim = smoothstep(0.55, 1.0, r) * max(dot(normal.xy, light_dir), 0.0);
            screen(orb, mix(accent, float3(1.0), 0.55), rim * uniforms.sphere_outline * 0.45);
            screen(result, orb.rgb, circularity * shape * detail_clarity);
        }

        float border = smoothstep(1.3, 0.0, abs(distance + 0.5)) * shape;
        float3 border_color = mix(float3(1.0), accent, 0.35 + average_activation * 0.35);
        screen(result, border_color, border * 0.24 * detail_clarity);
        screen(result, float3(1.0), completion_flash * shape * 0.55);

        return result * clamp(uniforms.opacity, 0.0, 1.0);
    }
    """#
}
