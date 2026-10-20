import Foundation
import AppKit
import ApplicationServices
let pid=pid_t(CommandLine.arguments[1])!,mode=CommandLine.arguments[2]
precondition(AXIsProcessTrusted(),"Native input requires Accessibility access")
let app=AXUIElementCreateApplication(pid)
func attribute<T>(_ element:AXUIElement,_ name:String)->T? {var value:CFTypeRef?;guard AXUIElementCopyAttributeValue(element,name as CFString,&value) == .success else{return nil};return value as? T}
func nodes(_ element:AXUIElement,_ web:Bool=false)->[AXUIElement] {let role:String=attribute(element,kAXRoleAttribute) ?? "";if role=="AXWebArea" && !web{return []};return [element]+(attribute(element,kAXChildrenAttribute) as [AXUIElement]? ?? []).flatMap{nodes($0,web)}}
func wait<T>(_ label:String,_ find:()->T?)->T {let end=Date().addingTimeInterval(8);repeat {if let found=find(){return found};RunLoop.current.run(until:Date().addingTimeInterval(0.03))}while Date()<end;fatalError("Native UI did not settle: \(label)")}
func window()->AXUIElement {wait("owned window"){(attribute(app,kAXWindowsAttribute) as [AXUIElement]?)?.first}}
func key(_ code:CGKeyCode,_ flags:CGEventFlags=[]) {for down in [true,false]{let event=CGEvent(keyboardEventSource:nil,virtualKey:code,keyDown:down)!;event.flags=flags;event.post(tap:.cghidEventTap)}}
func press(_ element:AXUIElement){precondition(AXUIElementPerformAction(element,kAXPressAction as CFString) == .success,"Native AX press failed")}
let _=window()
let running=NSRunningApplication(processIdentifier:pid)!
precondition(running.activate(options:[.activateAllWindows]),"Cannot activate owned app")
AXUIElementSetAttributeValue(app,kAXFrontmostAttribute as CFString,kCFBooleanTrue)
let _:Bool=wait("owned app focus"){NSWorkspace.shared.frontmostApplication?.processIdentifier==pid ? true:nil}
if mode=="save" {
 let sheet=wait("save sheet"){nodes(window()).first{(attribute($0,kAXRoleAttribute) as String?)==kAXSheetRole}}
 func fields()->[AXUIElement]{nodes(sheet).filter{[kAXTextFieldRole,kAXComboBoxRole].contains(attribute($0,kAXRoleAttribute) as String? ?? "")}}
 let name=wait("Save As field"){fields().first}
 precondition(AXUIElementSetAttributeValue(name,kAXValueAttribute as CFString,CommandLine.arguments[4] as CFString) == .success)
 let folderName=URL(fileURLWithPath:CommandLine.arguments[3]).lastPathComponent
 precondition(nodes(sheet).contains{(attribute($0,kAXRoleAttribute) as String?)==kAXPopUpButtonRole && (attribute($0,kAXValueAttribute) as String?)==folderName},"Save dialog escaped the isolated fixture folder")
 let save:AXUIElement=wait("Save button after folder selection"){return nodes(sheet).first{(attribute($0,kAXRoleAttribute) as String?)==kAXButtonRole && ["Save","저장"].contains(attribute($0,kAXTitleAttribute) as String? ?? "") && (attribute($0,kAXEnabledAttribute) as Bool? ?? false)}}
 press(save)
} else if mode=="undo" {
 let editor=wait("Query editor"){nodes(window(),true).first{(attribute($0,kAXRoleAttribute) as String?)==kAXTextAreaRole && (attribute($0,kAXTitleAttribute) as String? ?? attribute($0,kAXDescriptionAttribute) as String? ?? "")=="Query editor"}}
 AXUIElementSetAttributeValue(editor,kAXFocusedAttribute as CFString,kCFBooleanTrue)
 let script=Process();script.executableURL=URL(fileURLWithPath:"/usr/bin/osascript");script.arguments=["-e","tell application \"System Events\" to key code 6 using command down"]
 try! script.run();script.waitUntilExit();precondition(script.terminationStatus==0,"Native Cmd+Z delivery failed")
} else if mode=="fuse-cancel" {
 let script=Process();script.executableURL=URL(fileURLWithPath:"/usr/bin/osascript");script.arguments=["-e","tell application \"System Events\" to key code 53"]
 try! script.run();script.waitUntilExit();precondition(script.terminationStatus==0,"Native Escape delivery failed")
} else if mode=="fuse-arm" || mode=="fuse-run" {
 let windows=CGWindowListCopyWindowInfo([.optionOnScreenOnly],kCGNullWindowID) as! [[String:Any]]
 guard let owned=windows.first(where:{$0[kCGWindowOwnerPID as String] as? Int == Int(pid) && (($0[kCGWindowBounds as String] as? [String:Double])?["Width"] ?? 0)>=960}),let bounds=owned[kCGWindowBounds as String] as? [String:Double] else {fatalError("Owned debug window missing")}
 let button=wait("destructive fuse allow button"){nodes(window(),true).first{element in
  guard (attribute(element,kAXRoleAttribute) as String?)==kAXButtonRole,(attribute(element,kAXEnabledAttribute) as Bool? ?? false) else{return false}
  return [attribute(element,kAXTitleAttribute) as String?,attribute(element,kAXDescriptionAttribute) as String?].compactMap{$0}.contains{$0=="허용"}
 }}
 let position:AXValue=attribute(button,kAXPositionAttribute)!,size:AXValue=attribute(button,kAXSizeAttribute)!
 var origin=CGPoint.zero,dimensions=CGSize.zero
 precondition(AXValueGetValue(position,.cgPoint,&origin)&&AXValueGetValue(size,.cgSize,&dimensions))
 let point=CGPoint(x:origin.x+dimensions.width/2,y:origin.y+dimensions.height/2)
 precondition(CGRect(x:bounds["X"]!,y:bounds["Y"]!,width:bounds["Width"]!,height:bounds["Height"]!).contains(point),"Fuse target escaped owned window")
 func mouse(_ type:CGEventType,_ at:CGPoint){let event=CGEvent(mouseEventSource:nil,mouseType:type,mouseCursorPosition:at,mouseButton:.left)!;event.flags=[];event.post(tap:.cghidEventTap)}
 mouse(.mouseMoved,point);press(button)
 let _:AXUIElement=wait("armed fuse cancel face"){nodes(window(),true).first{element in
  (attribute(element,kAXRoleAttribute) as String?)==kAXButtonRole && [attribute(element,kAXTitleAttribute) as String?,attribute(element,kAXDescriptionAttribute) as String?].compactMap{$0}.contains{$0.contains("취소")}
 }}
 mouse(.mouseMoved,CGPoint(x:bounds["X"]!+50,y:bounds["Y"]!+80))
 if mode=="fuse-arm" {Thread.sleep(forTimeInterval:0.1);mouse(.mouseMoved,point)}
 print("{\"x\":\(point.x),\"y\":\(point.y),\"coordinates\":\"screen\",\"action\":\"\(mode)\",\"activation\":\"AXPress\"}")
}
