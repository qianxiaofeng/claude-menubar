import AppKit

// MARK: - Data Model (mirrors Rust SessionInfo)

struct SessionInfo: Decodable {
    let tty: String
    let pid: UInt32
    let cwd: String
    let terminal: String
    let transcript: String?
    let status: String
}

enum SessionStatus: String {
    case active
    case pending
    case idle

    var sfSymbol: String { "cpu.fill" }

    var color: NSColor {
        switch self {
        case .active:  return NSColor(srgbRed: 0x32/255, green: 0xD7/255, blue: 0x4B/255, alpha: 1)
        case .pending: return NSColor(srgbRed: 0xFF/255, green: 0x9F/255, blue: 0x0A/255, alpha: 1)
        case .idle:    return NSColor(srgbRed: 0x8E/255, green: 0x8E/255, blue: 0x93/255, alpha: 1)
        }
    }

    var label: String {
        switch self {
        case .active:  return "Running"
        case .pending: return "Needs input"
        case .idle:    return "Idle"
        }
    }
}

// MARK: - App Delegate

class AppDelegate: NSObject, NSApplicationDelegate {
    var statusItem: NSStatusItem!
    var timer: Timer!
    var animationTimer: Timer?
    var currentSessions: [SessionInfo] = []
    let binaryPath: String

    init(binaryPath: String) {
        self.binaryPath = binaryPath
        super.init()
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        statusItem.isVisible = false

        pollAndUpdate()

        timer = Timer(timeInterval: 2.0, repeats: true) { [weak self] _ in
            self?.pollAndUpdate()
        }
        RunLoop.main.add(timer, forMode: .common)
    }

    func pollAndUpdate() {
        let sessions = pollSessions()
        currentSessions = sessions

        if sessions.isEmpty {
            statusItem.isVisible = false
            statusItem.menu = nil
            stopAnimation()
            return
        }

        statusItem.isVisible = true
        statusItem.menu = buildMenu(sessions: sessions)

        let needsAnimation = sessions.contains {
            let s = SessionStatus(rawValue: $0.status) ?? .idle
            return s == .pending || s == .idle
        }
        if needsAnimation { startAnimation() } else { stopAnimation() }
        updateIcon()
    }

    func updateIcon() {
        guard let button = statusItem.button else { return }
        let icon = composeIcon(sessions: currentSessions)
        icon.isTemplate = false
        button.image = icon
    }

    func startAnimation() {
        guard animationTimer == nil else { return }
        animationTimer = Timer(timeInterval: 1.0 / 30.0, repeats: true) { [weak self] _ in
            self?.updateIcon()
        }
        RunLoop.main.add(animationTimer!, forMode: .common)
    }

    func stopAnimation() {
        animationTimer?.invalidate()
        animationTimer = nil
    }

    /// Sine-wave pulse: opacity oscillates 0.3 … 1.0 over a 1.5 s cycle
    func pulseAlpha() -> CGFloat {
        let period: Double = 4.0
        let t = Date.timeIntervalSinceReferenceDate.truncatingRemainder(dividingBy: period) / period
        return CGFloat(0.3 + 0.7 * (0.5 + 0.5 * sin(t * 2 * .pi)))
    }

    func alphaForStatus(_ status: SessionStatus) -> CGFloat {
        guard status != .active, animationTimer != nil else { return 1.0 }
        return pulseAlpha()
    }

    // MARK: - Poll

    func pollSessions() -> [SessionInfo] {
        let proc = Process()
        proc.executableURL = URL(fileURLWithPath: binaryPath)
        proc.arguments = ["poll"]

        let pipe = Pipe()
        proc.standardOutput = pipe
        proc.standardError = FileHandle.nullDevice

        do {
            try proc.run()
            proc.waitUntilExit()
        } catch {
            return []
        }

        guard proc.terminationStatus == 0 else { return [] }

        let data = pipe.fileHandleForReading.readDataToEndOfFile()
        guard !data.isEmpty else { return [] }

        do {
            return try JSONDecoder().decode([SessionInfo].self, from: data)
        } catch {
            return []
        }
    }

    // MARK: - Icon Composition

    func composeIcon(sessions: [SessionInfo]) -> NSImage {
        let symbolSize: CGFloat = 18
        let count = sessions.count

        if count == 1 {
            let status = SessionStatus(rawValue: sessions[0].status) ?? .idle
            let sym = makeSymbol(for: sessions[0], pointSize: symbolSize)
            let alpha = alphaForStatus(status)
            if alpha >= 1.0 { return sym }
            let size = sym.size
            return NSImage(size: size, flipped: false) { rect in
                sym.draw(in: rect, from: .zero, operation: .sourceOver, fraction: alpha)
                return true
            }
        }

        let smallSize: CGFloat = 14
        let step = smallSize * 0.5  // 50% overlap
        let totalWidth = smallSize + CGFloat(count - 1) * step
        let height = smallSize + 2

        let composed = NSImage(size: NSSize(width: totalWidth, height: height), flipped: false) { rect in
            for (i, session) in sessions.enumerated() {
                let status = SessionStatus(rawValue: session.status) ?? .idle
                let alpha = self.alphaForStatus(status)
                let sym = self.makeSymbol(for: session, pointSize: smallSize)
                let x = CGFloat(i) * step
                sym.draw(in: NSRect(x: x, y: 0, width: smallSize, height: height),
                         from: .zero, operation: .sourceOver, fraction: alpha)
            }
            return true
        }
        return composed
    }

    func makeSymbol(for session: SessionInfo, pointSize: CGFloat) -> NSImage {
        let status = SessionStatus(rawValue: session.status) ?? .idle
        let config = NSImage.SymbolConfiguration(pointSize: pointSize, weight: .medium)
            .applying(NSImage.SymbolConfiguration(paletteColors: [status.color]))

        guard let img = NSImage(systemSymbolName: status.sfSymbol, accessibilityDescription: status.label)?
            .withSymbolConfiguration(config) else {
            return NSImage()
        }
        return img
    }

    // MARK: - Menu

    func buildMenu(sessions: [SessionInfo]) -> NSMenu {
        let menu = NSMenu()

        for (index, session) in sessions.enumerated() {
            let status = SessionStatus(rawValue: session.status) ?? .idle
            let project = URL(fileURLWithPath: session.cwd).lastPathComponent

            // Project name row with status icon
            let item = NSMenuItem(title: project, action: #selector(focusSession(_:)), keyEquivalent: "")
            item.target = self
            item.tag = index
            item.image = makeSmallSymbol(for: session)
            menu.addItem(item)

            // Status sub-row (indented, disabled)
            let statusItem = NSMenuItem(title: "  \(status.label)", action: nil, keyEquivalent: "")
            statusItem.isEnabled = false
            if let font = NSFont.systemFont(ofSize: 11, weight: .regular) as NSFont? {
                statusItem.attributedTitle = NSAttributedString(
                    string: "  \(status.label)",
                    attributes: [
                        .font: font,
                        .foregroundColor: NSColor.secondaryLabelColor
                    ]
                )
            }
            menu.addItem(statusItem)
        }

        menu.addItem(.separator())

        let quitItem = NSMenuItem(title: "Quit Claude Bar", action: #selector(quitApp(_:)), keyEquivalent: "q")
        quitItem.target = self
        menu.addItem(quitItem)

        return menu
    }

    func makeSmallSymbol(for session: SessionInfo) -> NSImage? {
        let status = SessionStatus(rawValue: session.status) ?? .idle
        let config = NSImage.SymbolConfiguration(pointSize: 12, weight: .medium)
            .applying(NSImage.SymbolConfiguration(paletteColors: [status.color]))
        return NSImage(systemSymbolName: status.sfSymbol, accessibilityDescription: status.label)?
            .withSymbolConfiguration(config)
    }

    // MARK: - Actions

    @objc func focusSession(_ sender: NSMenuItem) {
        let sessions = pollSessions()
        guard sender.tag < sessions.count else { return }
        let session = sessions[sender.tag]

        let proc = Process()
        proc.executableURL = URL(fileURLWithPath: binaryPath)
        proc.arguments = ["focus", "--terminal", session.terminal, "--tty", session.tty, "--cwd", session.cwd]
        proc.standardOutput = FileHandle.nullDevice
        proc.standardError = FileHandle.nullDevice
        try? proc.run()
    }

    @objc func quitApp(_ sender: NSMenuItem) {
        NSApplication.shared.terminate(nil)
    }
}

// MARK: - Main

let binaryPath: String = {
    let execURL = URL(fileURLWithPath: CommandLine.arguments[0])
    let dir = execURL.deletingLastPathComponent()
    return dir.appendingPathComponent("claude-bar").path
}()

let app = NSApplication.shared
app.setActivationPolicy(.accessory)

let delegate = AppDelegate(binaryPath: binaryPath)
app.delegate = delegate
app.run()
