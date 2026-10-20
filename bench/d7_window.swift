import Foundation
import CoreGraphics
let pid = Int(CommandLine.arguments[1])!
let windows = CGWindowListCopyWindowInfo([.optionAll], kCGNullWindowID) as! [[String:Any]]
let matching = windows.filter { w in
 guard w[kCGWindowOwnerPID as String] as? Int == pid, w[kCGWindowLayer as String] as? Int == 0, let bounds=w[kCGWindowBounds as String] as? [String:Any], let width=bounds["Width"] as? Double, let height=bounds["Height"] as? Double else { return false }
 return width >= 960 && height >= 600
}
if let window=matching.first(where:{$0[kCGWindowIsOnscreen as String] as? Bool == true}) ?? matching.first, let id=window[kCGWindowNumber as String] as? Int {print(id)}
