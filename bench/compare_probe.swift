// Compile: swiftc bench/compare_probe.swift -o /tmp/compare-probe
// launch EXE MARKER OUTPUT: window, AX marker, 10/60s RSS, 30–60s CPU.
// query PID FIRST_REGEX COUNT_REGEX OUTPUT: Cmd+Return then AX polling.
import Foundation
import CoreGraphics
import ApplicationServices
func command(_ exe: String, _ args: [String]) -> String {
 let p=Process(); p.executableURL=URL(fileURLWithPath:exe); p.arguments=args
 let pipe=Pipe(); p.standardOutput=pipe; p.standardError=FileHandle.nullDevice
 do { try p.run() } catch { return "ERROR: \(error)" }
 let d=pipe.fileHandleForReading.readDataToEndOfFile(); p.waitUntilExit()
 return String(data:d,encoding:.utf8) ?? ""
}
func processes() -> [Int32:(Int,Double,String)] {
 var result:[Int32:(Int,Double,String)]=[:]
 for line in command("/bin/ps",["-axo","pid=,rss=,time=,comm="]).split(separator:"\n") {
  let f=line.split(separator:" ",maxSplits:3,omittingEmptySubsequences:true)
  if f.count==4, let pid=Int32(f[0]),let rss=Int(f[1]) {
   let t=f[2].split(separator:":").compactMap{Double($0)}
   result[pid]=(rss,t.reduce(0){$0*60+$1},String(f[3]))
  }
 }
 return result
}
func texts(_ root:AXUIElement, _ depth:Int=0) -> [String] {
 if depth>45 { return [] }; var out:[String]=[]
 for key in [kAXValueAttribute,kAXTitleAttribute,kAXDescriptionAttribute] {
  var v:CFTypeRef?; if AXUIElementCopyAttributeValue(root,key as CFString,&v) == .success,let s=v as? String,!s.isEmpty {out.append(s)}
 }
 var c:CFTypeRef?; AXUIElementCopyAttributeValue(root,kAXChildrenAttribute as CFString,&c)
 for child in (c as? [AXUIElement] ?? []) {out += texts(child,depth+1)}
 return out
}
func windowTexts(_ app:AXUIElement) -> [String] {
 var windows:CFTypeRef?;AXUIElementCopyAttributeValue(app,kAXWindowsAttribute as CFString,&windows)
 return (windows as? [AXUIElement] ?? []).flatMap{texts($0)}
}
func window(_ pid:Int32)->Bool {
 let list=CGWindowListCopyWindowInfo(.optionOnScreenOnly,kCGNullWindowID) as? [[String:Any]] ?? []
 return list.contains { w in
  guard (w[kCGWindowOwnerPID as String] as? Int32)==pid,let b=w[kCGWindowBounds as String] as? [String:CGFloat] else{return false}
  return (b["Width"] ?? 0)>200 && (b["Height"] ?? 0)>200
 }
}
func write(_ value:[String:Any],_ path:String) {
 let d=try! JSONSerialization.data(withJSONObject:value,options:[.prettyPrinted,.sortedKeys]);try! d.write(to:URL(fileURLWithPath:path));print(String(data:d,encoding:.utf8)!)
}
let a=CommandLine.arguments
setvbuf(stdout,nil,_IONBF,0)
if a[1]=="selfcheck" {
 assert(processes()[ProcessInfo.processInfo.processIdentifier] != nil)
 assert("row 1".range(of:"row \\d+",options:.regularExpression) != nil)
 print("selfcheck passed; AX trusted: \(AXIsProcessTrusted())")
} else if a[1]=="launch" {
 let before=Set(processes().keys);let p=Process();p.executableURL=URL(fileURLWithPath:a[2]);p.standardOutput=FileHandle.nullDevice;p.standardError=FileHandle.nullDevice
 let t=ProcessInfo.processInfo.systemUptime;try! p.run();let pid=p.processIdentifier;let ax=AXUIElementCreateApplication(pid)
 AXUIElementSetMessagingTimeout(ax,0.2)
 final class Paint { let lock=NSLock(); var ms:Double? }
 let paint=Paint()
 DispatchQueue.global().async {
  while ProcessInfo.processInfo.systemUptime-t<30 {
   if windowTexts(ax).contains(where:{$0==a[3]}) {paint.lock.lock();paint.ms=(ProcessInfo.processInfo.systemUptime-t)*1000;paint.lock.unlock();break}
   usleep(20000)
  }
 }
 var win:Double?;var r:[String:Any]=["pid":pid,"exe":a[2],"marker":a[3],"ax_trusted":AXIsProcessTrusted(),"startup_timeout_seconds":30]
 func sample()->[String:Any] {
  let ps=processes();let ids=ps.keys.filter{$0==pid || (!before.contains($0) && ps[$0]!.2.contains("com.apple.WebKit"))}
  return ["pids":ids.sorted(),"rss_kib":ids.reduce(0){$0+ps[$1]!.0},"footprint":command("/usr/bin/footprint",ids.flatMap{["-p",String($0)]}+["--noCategories"]),"cpu_seconds":ids.reduce(0.0){$0+ps[$1]!.1},"processes":ids.map{["pid":$0,"rss_kib":ps[$0]!.0,"comm":ps[$0]!.2] as [String:Any]}]
 }
 var at10:[String:Any]?;var c0:[String:Any]?;var t0=0.0
 while ProcessInfo.processInfo.systemUptime-t<60 {
  let elapsed=ProcessInfo.processInfo.systemUptime-t
  if win==nil && elapsed<30 && window(pid) {win=(ProcessInfo.processInfo.systemUptime-t)*1000;let bundle=URL(fileURLWithPath:a[2]).deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent().path;r["activation_output"]=command("/usr/bin/open",["-a",bundle])}
  if elapsed>=10 && at10==nil {at10=sample();r["at10_actual_seconds"]=elapsed}
  if elapsed>=30 && c0==nil {c0=sample();t0=ProcessInfo.processInfo.systemUptime}
  usleep(20000)
 }
 let c1=sample();r["at10"]=at10!;r["at60"]=c1;r["at60_actual_seconds"]=ProcessInfo.processInfo.systemUptime-t
 r["window_ms"]=win ?? NSNull();paint.lock.lock();r["ax_marker_ms"]=paint.ms ?? NSNull();paint.lock.unlock()
 r["cpu_interval_seconds"]=ProcessInfo.processInfo.systemUptime-t0
 r["cpu_30_60_percent"]=((c1["cpu_seconds"] as! Double)-(c0!["cpu_seconds"] as! Double))/(ProcessInfo.processInfo.systemUptime-t0)*100
 r["uptime"]=command("/usr/bin/uptime",[]);write(r,a[4]);p.terminate();p.waitUntilExit()
} else if a[1]=="query" {
 let pid=Int32(a[2])!;let ax=AXUIElementCreateApplication(pid)
 let initial=windowTexts(ax);let t=ProcessInfo.processInfo.systemUptime
 let down=CGEvent(keyboardEventSource:nil,virtualKey:36,keyDown:true)!;down.flags = .maskCommand
 let up=CGEvent(keyboardEventSource:nil,virtualKey:36,keyDown:false)!;up.flags = .maskCommand
 down.postToPid(pid);up.postToPid(pid)
 var first:Double?;var count:Double?;var observed:[String]=[]
 while ProcessInfo.processInfo.systemUptime-t<60 {
  observed=windowTexts(ax)
  if first==nil && observed.contains(where:{$0.range(of:a[3],options:.regularExpression) != nil}) {first=(ProcessInfo.processInfo.systemUptime-t)*1000}
  if count==nil && observed.contains(where:{$0.range(of:a[4],options:.regularExpression) != nil}) {count=(ProcessInfo.processInfo.systemUptime-t)*1000}
  if first != nil && count != nil {break};usleep(20000)
 }
 let ps=processes();write(["pid":pid,"first_ms":first ?? NSNull(),"count_ms":count ?? NSNull(),"initial_texts":initial,"final_texts":observed,"main_rss_kib":ps[pid]?.0 ?? 0],a[5])
}
