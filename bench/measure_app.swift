// Usage: swift measure_app.swift <executable> [idleSeconds] [runs]
// Measures: time from process launch until its first on-screen window (>200x200) appears,
// then idle memory (RSS) of the app process + WebKit helper processes it spawned, after idleSeconds.
import Foundation
import CoreGraphics

func webkitPids() -> Set<Int32> {
    let p = Process(); p.executableURL = URL(fileURLWithPath: "/bin/ps")
    p.arguments = ["-axo", "pid=,comm="]
    let pipe = Pipe(); p.standardOutput = pipe; try! p.run(); p.waitUntilExit()
    let out = String(data: pipe.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8)!
    var s = Set<Int32>()
    for line in out.split(separator: "\n") where line.contains("com.apple.WebKit") {
        if let pid = Int32(line.trimmingCharacters(in: .whitespaces).split(separator: " ")[0]) { s.insert(pid) }
    }
    return s
}

func rssKB(_ pids: [Int32]) -> Int {
    let p = Process(); p.executableURL = URL(fileURLWithPath: "/bin/ps")
    p.arguments = ["-o", "rss=", "-p", pids.map(String.init).joined(separator: ",")]
    let pipe = Pipe(); p.standardOutput = pipe; try! p.run(); p.waitUntilExit()
    let out = String(data: pipe.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8)!
    return out.split(separator: "\n").compactMap { Int($0.trimmingCharacters(in: .whitespaces)) }.reduce(0, +)
}

func hasWindow(_ pid: Int32) -> Bool {
    guard let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as? [[String: Any]] else { return false }
    for w in list where (w[kCGWindowOwnerPID as String] as? Int32) == pid {
        if let b = w[kCGWindowBounds as String] as? [String: CGFloat], (b["Width"] ?? 0) > 200, (b["Height"] ?? 0) > 200 { return true }
    }
    return false
}

setvbuf(stdout, nil, _IONBF, 0)
let args = CommandLine.arguments
let exe = args[1]
let idle = args.count > 2 ? Double(args[2])! : 10
let runs = args.count > 3 ? Int(args[3])! : 5
var starts: [Double] = [], mems: [Int] = []
for i in 1...runs {
    let before = webkitPids()
    let p = Process(); p.executableURL = URL(fileURLWithPath: exe)
    p.standardOutput = FileHandle.nullDevice; p.standardError = FileHandle.nullDevice
    let t0 = Date(); try! p.run()
    var startup = -1.0
    while Date().timeIntervalSince(t0) < 30 {
        if hasWindow(p.processIdentifier) { startup = Date().timeIntervalSince(t0) * 1000; break }
        usleep(5000)
    }
    Thread.sleep(forTimeInterval: idle)
    let helpers = webkitPids().subtracting(before)
    let mem = rssKB([p.processIdentifier] + Array(helpers))
    print(String(format: "run %d: window %.0f ms, idle RSS %.1f MB (main + %d webkit)", i, startup, Double(mem) / 1024, helpers.count))
    starts.append(startup); mems.append(mem)
    p.terminate(); p.waitUntilExit(); Thread.sleep(forTimeInterval: 2)
}
let s = starts.sorted(), m = mems.sorted()
print(String(format: "MEDIAN window %.0f ms, idle RSS %.1f MB", s[s.count / 2], Double(m[m.count / 2]) / 1024))
