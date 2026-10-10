// Offscreen WKWebView QA; no foreground window or user session changes.
import AppKit
import WebKit
import Foundation
let args=CommandLine.arguments
guard args.count==3,let url=URL(string:args[1]) else {fatalError("url results.json")}
let visible=ProcessInfo.processInfo.environment["SQUEEZE_QA_VISIBLE"] == "1"
let app=NSApplication.shared
app.setActivationPolicy(visible ? .accessory : .prohibited)
final class Observer: NSObject,WKScriptMessageHandler,WKNavigationDelegate {
 func userContentController(_ controller: WKUserContentController,didReceive message: WKScriptMessage){
  do {let data=try JSONSerialization.data(withJSONObject:message.body,options:[.prettyPrinted,.sortedKeys]);try data.write(to:URL(fileURLWithPath:args[2]));print("WKWebView result saved");exit(0)}catch{fputs("Result error: \(error)\n",stderr);exit(1)}
 }
 func webView(_ webView: WKWebView,didFailProvisionalNavigation navigation: WKNavigation!,withError error: Error){fputs("Navigation error: \(error)\n",stderr);exit(1)}
 func webViewWebContentProcessDidTerminate(_ webView: WKWebView){fputs("Web content process terminated\n",stderr);exit(1)}
}
let observer=Observer()
let config=WKWebViewConfiguration()
config.userContentController.add(observer,name:"qaResult")
let view=WKWebView(frame:NSRect(x:0,y:0,width:1200,height:900),configuration:config)
let window=NSWindow(contentRect:NSRect(x:visible ? 80 : -10000,y:visible ? 80 : -10000,width:1200,height:900),styleMask:.borderless,backing:.buffered,defer:false)
window.contentView=view
if visible {window.title="Squeeze PDF — Local renderer QA";window.makeKeyAndOrderFront(nil);app.activate(ignoringOtherApps:true)} else {window.orderBack(nil)}
view.navigationDelegate=observer
view.load(URLRequest(url:url))
DispatchQueue.main.asyncAfter(deadline:.now()+240){fputs("WKWebView QA timed out\n",stderr);exit(1)}
app.run()
