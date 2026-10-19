// Isolated macOS WKWebView smoke test; loads only the local mock preview, never AxDB.app.
import AppKit
import WebKit

@MainActor final class DriverView: NSObject, WKNavigationDelegate {
    let web: WKWebView
    let window: NSWindow
    var started = false
    override init() {
        let config = WKWebViewConfiguration()
        config.userContentController.addUserScript(WKUserScript(source: "const media=window.matchMedia.bind(window);window.matchMedia=q=>q.includes('prefers-reduced-motion')?({matches:true,media:q,addEventListener(){},removeEventListener(){}}):media(q);", injectionTime: .atDocumentStart, forMainFrameOnly: true))
        web = WKWebView(frame: NSRect(x: 0, y: 0, width: 960, height: 600), configuration: config)
        window = NSWindow(contentRect: NSRect(x: -2200, y: 0, width: 960, height: 600), styleMask: [.borderless], backing: .buffered, defer: false)
        super.init()
        web.navigationDelegate = self
        window.contentView = web
        window.orderBack(nil)
        web.load(URLRequest(url: URL(string: "http://127.0.0.1:5194")!))
    }
    func wait(_ expression: String) async throws {
        for _ in 0..<300 {
            if (try? await web.evaluateJavaScript(expression)) as? Bool == true { return }
            try await Task.sleep(nanoseconds: 100_000_000)
        }
        throw NSError(domain: "WKDriverSmoke", code: 1, userInfo: [NSLocalizedDescriptionKey: "Timed out: \(expression)"])
    }
    func shot(_ name: String) async throws {
        let image = try await web.takeSnapshot(configuration: nil)
        guard let tiff = image.tiffRepresentation, let rep = NSBitmapImageRep(data: tiff), let png = rep.representation(using: .png, properties: [:]) else { throw NSError(domain: "WKDriverSmoke", code: 2) }
        try png.write(to: URL(fileURLWithPath: "docs/design-shots/\(name).png"))
    }
    func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
        if started { return }; started = true
        Task { @MainActor in
            do {
                try await wait("!!document.querySelector('button[aria-label=\"드라이버\"]')")
                _ = try await web.evaluateJavaScript("document.querySelector('button[aria-label=\"드라이버\"]').click()")
                try await wait("document.querySelectorAll('.glass-icon-cell').length===5")
                try await Task.sleep(nanoseconds: 2_000_000_000)
                let gpu = try await web.evaluateJavaScript("document.querySelector('.glass-icons').dataset.gpu || 'unknown'")
                for theme in ["midnight", "daylight"] {
                    _ = try await web.evaluateJavaScript("document.documentElement.dataset.theme='\(theme)';window.dispatchEvent(new Event('axdb-theme-change'))")
                    try await Task.sleep(nanoseconds: 300_000_000)
                    try await shot("drivers-glass-wk-\(theme)")
                }
                _ = try await web.evaluateJavaScript("document.querySelector('.glass-icons canvas').getContext('webgl2')?.getExtension('WEBGL_lose_context')?.loseContext()")
                try await wait("document.querySelector('.glass-icons').dataset.gpu==='false'")
                try await Task.sleep(nanoseconds: 300_000_000)
                try await shot("drivers-glass-wk-fallback")
                print("WKWebView PASS: mock preview, GPU=\(String(describing: gpu)), daylight/midnight snapshots, context-loss DOM fallback; no user app or DB access")
                window.close(); NSApplication.shared.terminate(nil)
            } catch {
                fputs("WKWebView FAIL: \(error)\n", stderr)
                window.close(); exit(1)
            }
        }
    }
}
@main struct DriverSmoke {
    @MainActor static func main() {
        let app = NSApplication.shared
        app.setActivationPolicy(.accessory)
        let driverView = DriverView()
        withExtendedLifetime(driverView) { app.run() }
    }
}
