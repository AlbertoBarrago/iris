import AppKit
import SwiftUI

/// The night-violet band at the top of Add Account and of the welcome
/// page: the winged envelope in one pose per step, and what moves on it.
/// The core picks the pose and the stamp (`AccountSetup.band`); this
/// draws them as the GTK app's `ui::post_band` does, from the same SVGs
/// and the same geometry, so the two stay alike.
struct PostBand: View {
    let band: BandView
    /// The lower lookup line asks too, once MX has answered.
    var lowerAsking = false
    /// Letters glide into the open envelope while the first sync runs.
    var letters = false
    /// How much larger than the art's own size the band draws it.
    var scale: CGFloat = 1

    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    /// When the stamp last changed, for its drop onto the corner.
    @State private var stampedAt = Date.distantPast
    /// When the badge last appeared, for its pop.
    @State private var warnedAt = Date.distantPast
    /// When the wing last glinted, on the idle pose.
    @State private var glintAt = Date.distantPast

    static let artSize = CGSize(width: 440, height: 196)
    private static let poses = ["idle", "stamped", "browser", "lookup", "error", "unreachable", "success"]

    var body: some View {
        GeometryReader { proxy in
            let fitted = min(scale, proxy.size.width / Self.artSize.width)
            ZStack {
                ForEach(Self.poses, id: \.self) { pose in
                    if let image = Self.image(pose) {
                        Image(nsImage: image)
                            .resizable()
                            .opacity(band.pose == pose ? 1 : 0)
                    }
                }
                .animation(.easeInOut(duration: band.pose == "success" ? 0.25 : 0.2), value: band.pose)
                TimelineView(.animation(paused: reduceMotion)) { timeline in
                    Canvas { context, _ in
                        context.scaleBy(x: fitted, y: fitted)
                        draw(in: &context, at: timeline.date)
                    }
                }
            }
            .frame(width: Self.artSize.width * fitted, height: Self.artSize.height * fitted)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .frame(height: Self.artSize.height * scale)
        .background(Self.background)
        .accessibilityElement()
        .accessibilityAddTraits(.isImage)
        .accessibilityLabel(band.described)
        .onChange(of: band.stamp) { stampedAt = .now }
        .onChange(of: warns) { if warns { warnedAt = .now } }
        .task(id: band.pose) { await glint() }
    }

    /// The GTK band's two gradients: violet from top to bottom, lit a
    /// little at the top edge and shaded at the bottom.
    private static let background = ZStack {
        LinearGradient(colors: [Color(hex: "#4a36b0")!, Color(hex: "#2c2070")!], startPoint: .top, endPoint: .bottom)
        LinearGradient(stops: [
            .init(color: Color(hex: "#fbf7f0")!.opacity(0.14), location: 0),
            .init(color: .clear, location: 0.1),
            .init(color: .clear, location: 0.85),
            .init(color: .black.opacity(0.16), location: 1),
        ], startPoint: .top, endPoint: .bottom)
    }

    private var warns: Bool { band.pose == "error" || band.pose == "unreachable" }

    /// Glints once in 6 to 9 seconds while the envelope rests.
    private func glint() async {
        while band.pose == "idle", !reduceMotion {
            try? await Task.sleep(for: .milliseconds(Int.random(in: 6000...9000)))
            if Task.isCancelled { return }
            glintAt = .now
        }
    }

    // MARK: Drawing

    private func draw(in context: inout GraphicsContext, at now: Date) {
        let env = Envelope(pose: band.pose)
        // The loops run 1.2 s, the browser's 1.6 s; without motion the
        // marks rest where the mockup draws them.
        let moving = !reduceMotion
        let time = now.timeIntervalSinceReferenceDate
        switch band.pose {
        case "idle":
            let t = now.timeIntervalSince(glintAt) / 0.7
            if moving, t < 1 { drawGlint(&context, at: env.wingTip, t: CGFloat(Self.easeInOutSine(t))) }
        case "browser":
            let phase = moving ? CGFloat(time.truncatingRemainder(dividingBy: 1.6) / 1.6) : 0.3
            for (index, strength) in [1.0, 0.75, 0.5].enumerated() {
                let t = (phase + 1 - CGFloat(index) / 3).truncatingRemainder(dividingBy: 1)
                drawDot(&context, Self.toBrowser.along(t), alpha: strength * Self.fade(t))
            }
        case "lookup":
            let phase = moving ? CGFloat(time.truncatingRemainder(dividingBy: 1.2) / 1.2) : 0.3
            for line in Self.toServers.prefix(lowerAsking ? 2 : 1) {
                for offset in [0.0, 0.5] {
                    let t = Self.easeInOut((phase + offset).truncatingRemainder(dividingBy: 1))
                    drawDot(&context, line.along(t), alpha: Self.fade(t))
                }
            }
        case "success":
            if moving, letters {
                let phase = CGFloat(time.truncatingRemainder(dividingBy: 1.2) / 1.2)
                for offset in [0.0, 0.5] {
                    let t = (phase + offset).truncatingRemainder(dividingBy: 1)
                    drawLetter(&context, Self.letterArc.along(t), angle: Self.letterAngle(t), alpha: Self.fade(t))
                }
            } else {
                // At rest, two letters wait on the arc.
                for t in [0.3, 0.66] {
                    drawLetter(&context, Self.letterArc.along(t), angle: Self.letterAngle(t), alpha: 1)
                }
            }
        default:
            break
        }
        if band.pose != "idle", let stamp = band.stamp {
            let t = moving ? min(1, now.timeIntervalSince(stampedAt) / 0.35) : 1
            drawStamp(&context, env: env, stamp: stamp, landed: CGFloat(Self.easeOutBack(t)), alpha: CGFloat(min(1, t * 2)))
        }
        if warns {
            let t = moving ? min(1, now.timeIntervalSince(warnedAt) / 0.2) : 1
            drawBadge(&context, env: env, grown: CGFloat(Self.easeOutCubic(t)))
        }
    }

    private func drawDot(_ context: inout GraphicsContext, _ point: CGPoint, alpha: CGFloat) {
        let r = 3.4
        context.fill(Path(ellipseIn: CGRect(x: point.x - r, y: point.y - r, width: 2 * r, height: 2 * r)),
                     with: .color(Self.paper.opacity(alpha)))
    }

    /// A four-pointed sparkle that swells and fades on the wing's tip,
    /// turning a little as it does.
    private func drawGlint(_ context: inout GraphicsContext, at tip: CGPoint, t: CGFloat) {
        let strength = sin(t * .pi)
        guard strength > 0 else { return }
        var star = context
        star.translateBy(x: tip.x, y: tip.y)
        star.rotate(by: .degrees(45 * t))
        let (long, short) = (9 * strength, 2.2 * strength * 0.4)
        var path = Path()
        path.move(to: CGPoint(x: 0, y: -long))
        path.addQuadCurve(to: CGPoint(x: long, y: 0), control: CGPoint(x: short, y: -short))
        path.addQuadCurve(to: CGPoint(x: 0, y: long), control: CGPoint(x: short, y: short))
        path.addQuadCurve(to: CGPoint(x: -long, y: 0), control: CGPoint(x: -short, y: short))
        path.addQuadCurve(to: CGPoint(x: 0, y: -long), control: CGPoint(x: -short, y: -short))
        star.fill(path, with: .color(Self.paper.opacity(strength)))
    }

    /// The provider's stamp on the envelope's top-right corner, turned
    /// 7°. A new stamp drops in from a little larger, as if pressed on.
    private func drawStamp(_ context: inout GraphicsContext, env: Envelope, stamp: SetupStamp, landed: CGFloat, alpha: CGFloat) {
        let (center, size) = env.stamp
        var layer = context
        layer.translateBy(x: center.x, y: center.y)
        layer.rotate(by: .degrees(7 * landed))
        let grow = 1 + 0.5 * (1 - landed)
        layer.scaleBy(x: grow, y: grow)
        layer.opacity = alpha
        let paper = CGRect(x: -size.width / 2, y: -size.height / 2, width: size.width, height: size.height)
        let body = Path(roundedRect: paper, cornerRadius: 2)
        layer.drawLayer { shadowed in
            shadowed.addFilter(.shadow(color: Color(red: 0.11, green: 0.098, blue: 0.09).opacity(0.28), radius: 4, y: 3))
            shadowed.fill(body, with: .color(Self.paper))
        }
        // A dashed stroke in the paper's colour over the edge is the
        // perforation: the dashes reach past the edge and the gaps bite in.
        layer.stroke(body, with: .color(Self.paper), style: StrokeStyle(lineWidth: 3, dash: [2.4, 2.2]))
        let inset = size.width * 0.14
        layer.fill(Path(roundedRect: paper.insetBy(dx: inset, dy: inset), cornerRadius: 2),
                   with: .color(Color(hex: stamp.colour) ?? PostBand.ink))
        let letter = Text(stamp.letter)
            .font(.system(size: size.width * 0.5, weight: .heavy))
            .foregroundStyle(Color(hex: "#fbf1c7")!)
        layer.draw(letter, at: .zero, anchor: .center)
    }

    /// The warning badge on the bottom-right corner: an ink ring, an
    /// amber disc and an ink mark, popping in from 0.8 of its size.
    private func drawBadge(_ context: inout GraphicsContext, env: Envelope, grown: CGFloat) {
        guard grown > 0 else { return }
        let r = env.width * 0.1
        var badge = context
        badge.translateBy(x: env.x + env.width - r * 0.5, y: env.top + env.height - r * 0.3)
        let size = 0.8 + 0.2 * grown
        badge.scaleBy(x: size, y: size)
        badge.opacity = min(1, grown)
        func circle(_ radius: CGFloat) -> Path {
            Path(ellipseIn: CGRect(x: -radius, y: -radius, width: 2 * radius, height: 2 * radius))
        }
        badge.fill(circle(r + 2.6), with: .color(Self.ink))
        badge.fill(circle(r), with: .color(Self.amber))
        badge.fill(Path(roundedRect: CGRect(x: -r * 0.13, y: -r * 0.58, width: r * 0.26, height: r * 0.72), cornerRadius: r * 0.13),
                   with: .color(Self.ink))
        badge.fill(Path(ellipseIn: CGRect(x: -r * 0.15, y: r * 0.32, width: r * 0.3, height: r * 0.3)), with: .color(Self.ink))
    }

    /// A mini letter, 26 by 18, turned `angle` degrees.
    private func drawLetter(_ context: inout GraphicsContext, _ point: CGPoint, angle: CGFloat, alpha: CGFloat) {
        guard alpha > 0 else { return }
        let (w, h): (CGFloat, CGFloat) = (26, 18)
        var letter = context
        letter.translateBy(x: point.x, y: point.y)
        letter.rotate(by: .degrees(angle))
        letter.opacity = alpha
        let paper = Path(roundedRect: CGRect(x: -w / 2, y: -h / 2, width: w, height: h), cornerRadius: 3)
        letter.drawLayer { shadowed in
            shadowed.addFilter(.shadow(color: Color(red: 0.11, green: 0.098, blue: 0.09).opacity(0.28), radius: 4, y: 3))
            shadowed.fill(paper, with: .color(Self.paper))
        }
        var flap = Path()
        flap.move(to: CGPoint(x: -w / 2 + 3, y: -h / 2 + 3))
        flap.addLine(to: CGPoint(x: 0, y: 1))
        flap.addLine(to: CGPoint(x: w / 2 - 3, y: -h / 2 + 3))
        letter.stroke(flap, with: .color(Self.ink), style: StrokeStyle(lineWidth: 2, lineCap: .round, lineJoin: .round))
    }

    // MARK: Geometry, as scripts/iris-art.py draws the art

    private static let paper = Color(hex: "#fbf7f0")!
    static let ink = Color(hex: "#1d1547")!
    private static let amber = Color(hex: "#f5c211")!

    /// The envelope in one pose: its width, top-left at (`x`, `top`).
    struct Envelope {
        let x: CGFloat
        let top: CGFloat
        let width: CGFloat

        init(pose: String) {
            switch pose {
            case "idle": (x, top, width) = (60, 66, 124)
            case "stamped", "error": (x, top, width) = (150, 62, 140)
            case "success": (x, top, width) = (154, 92, 132)
            default: (x, top, width) = (52, 66, 124)
            }
        }

        init(x: CGFloat, top: CGFloat, width: CGFloat) {
            (self.x, self.top, self.width) = (x, top, width)
        }

        var height: CGFloat { width * 78 / 108 }

        /// The stamp's middle and size.
        var stamp: (CGPoint, CGSize) {
            let sw = width * 0.27
            let sh = sw * 1.16
            return (CGPoint(x: x + width - sw * 0.42, y: top + sh * 0.12), CGSize(width: sw, height: sh))
        }

        /// The tip of the wing's longest feather at rest, where the glint
        /// shows.
        var wingTip: CGPoint {
            let root = CGPoint(x: x + 0.12 * width, y: top + 0.18 * height)
            let angle = (14.0 + 4 * 9.0) * .pi / 180
            let length = 0.76 * width
            return CGPoint(x: root.x - length * cos(angle), y: root.y - length * sin(angle))
        }
    }

    /// A quadratic curve.
    struct Curve {
        let p0: CGPoint, p1: CGPoint, p2: CGPoint

        func along(_ t: CGFloat) -> CGPoint {
            let u = 1 - t
            return CGPoint(x: u * u * p0.x + 2 * u * t * p1.x + t * t * p2.x,
                           y: u * u * p0.y + 2 * u * t * p1.y + t * t * p2.y)
        }
    }

    /// The dashed path to the browser window, drawn in the art.
    private static let toBrowser: Curve = {
        let e = Envelope(pose: "browser")
        return Curve(p0: CGPoint(x: e.x + e.width + 12, y: e.top + 50),
                     p1: CGPoint(x: e.x + e.width + 64, y: e.top + 2),
                     p2: CGPoint(x: 286, y: 104))
    }()

    /// The two lookup lines: to the upper server, and to the lower one
    /// that waits for MX.
    private static let toServers: [Curve] = {
        let e = Envelope(pose: "lookup")
        return [
            Curve(p0: CGPoint(x: e.x + e.width + 12, y: e.top + 44), p1: CGPoint(x: e.x + e.width + 70, y: e.top + 4),
                  p2: CGPoint(x: 300, y: 70)),
            Curve(p0: CGPoint(x: e.x + e.width + 12, y: e.top + 64), p1: CGPoint(x: e.x + e.width + 70, y: e.top + 96),
                  p2: CGPoint(x: 300, y: 136)),
        ]
    }()

    /// The arc the letters glide along into the open envelope.
    private static let letterArc: Curve = {
        let e = Envelope(pose: "success")
        return Curve(p0: CGPoint(x: 430, y: 150), p1: CGPoint(x: 420, y: 50), p2: CGPoint(x: e.x + e.width / 2 + 44, y: e.top - 30))
    }()

    /// A travelling mark fades in as it leaves and out as it arrives.
    private static func fade(_ t: CGFloat) -> CGFloat { min(t / 0.15, (1 - t) / 0.15).clamped(0, 1) }

    private static func letterAngle(_ t: CGFloat) -> CGFloat { -18 + (t - 0.3) * 10 / 0.36 }

    private static func easeInOut(_ t: CGFloat) -> CGFloat {
        t < 0.5 ? 4 * t * t * t : 1 - pow(-2 * t + 2, 3) / 2
    }

    private static func easeInOutSine(_ t: Double) -> Double { -(cos(.pi * t) - 1) / 2 }
    private static func easeOutCubic(_ t: Double) -> Double { 1 - pow(1 - t, 3) }

    /// Overshoots a little before it settles, as a stamp pressed on does.
    private static func easeOutBack(_ t: Double) -> Double {
        let c1 = 1.4, c3 = c1 + 1
        return 1 + c3 * pow(t - 1, 3) + c1 * pow(t - 1, 2)
    }

    // MARK: Pictures

    private static var images: [String: NSImage] = [:]

    /// A pose's SVG from the bundle, read once. NSImage keeps the SVG as
    /// vectors, so it stays sharp at any scale.
    @MainActor static func image(_ pose: String) -> NSImage? {
        if let image = images[pose] { return image }
        let image = Bundle.main.url(forResource: pose, withExtension: "svg", subdirectory: "band").flatMap(NSImage.init(contentsOf:))
        images[pose] = image
        return image
    }
}

/// A provider's mark, as the tiles show it: the logo on white for Google
/// and Microsoft, the cloud on its colour for iCloud, else the initial on
/// a tile of the provider's colour.
struct ProviderMark: View {
    let stamp: SetupStamp
    var size: CGFloat = 38

    var body: some View {
        let shape = RoundedRectangle(cornerRadius: size * 0.29, style: .continuous)
        ZStack {
            shape.fill(onWhite ? Color.white : Color(hex: stamp.colour) ?? PostBand.ink)
            // The lit top half the GTK tile has.
            shape.fill(LinearGradient(stops: [.init(color: .white.opacity(0.07), location: 0.5),
                                              .init(color: .clear, location: 0.5)],
                                      startPoint: .top, endPoint: .bottom))
            if let logo = stamp.logo, let image = Self.logo(logo) {
                Image(nsImage: image)
                    .resizable()
                    .aspectRatio(contentMode: .fit)
                    .frame(width: size * logoShare, height: size * logoShare)
            } else {
                Text(stamp.letter)
                    .font(.system(size: size * 0.5, weight: .heavy))
                    .foregroundStyle(Color(hex: "#fbf1c7")!)
            }
        }
        .frame(width: size, height: size)
        .overlay {
            if onWhite { shape.strokeBorder(.black.opacity(0.14), lineWidth: 1) }
        }
        .accessibilityHidden(true)
    }

    private var onWhite: Bool { stamp.logo == "google" || stamp.logo == "microsoft" }

    private var logoShare: CGFloat {
        switch stamp.logo {
        case "google": 0.6
        case "microsoft": 0.58
        default: 0.74
        }
    }

    private static func logo(_ name: String) -> NSImage? {
        ["png", "svg"].lazy
            .compactMap { Bundle.main.url(forResource: name, withExtension: $0, subdirectory: "logos") }
            .compactMap(NSImage.init(contentsOf:))
            .first
    }
}

