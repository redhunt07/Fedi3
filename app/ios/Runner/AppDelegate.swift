import Flutter
import UIKit
import UserNotifications

@main
@objc class AppDelegate: FlutterAppDelegate {
  private var pushChannel: FlutterMethodChannel?
  private var pushReady = false
  private var pendingToken: String?
  private var wakeups: [String: (UIBackgroundFetchResult) -> Void] = [:]

  override func application(_ application: UIApplication,
    didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]?) -> Bool {
    GeneratedPluginRegistrant.register(with: self)
    if let controller = window?.rootViewController as? FlutterViewController {
      let channel = FlutterMethodChannel(name: "org.fedi3.client/push", binaryMessenger: controller.binaryMessenger)
      pushChannel = channel
      channel.setMethodCallHandler { [weak self] call, result in
        guard let self = self else { result(FlutterError(code: "unavailable", message: nil, details: nil)); return }
        switch call.method {
        case "initialize":
          self.pushReady = true
          result(UIApplication.shared.applicationState != .active)
          if let token = self.pendingToken { channel.invokeMethod("token", arguments: token) }
          for id in Array(self.wakeups.keys) { channel.invokeMethod("wake", arguments: id) }
        case "enable":
          UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .badge, .sound]) { granted, _ in
            DispatchQueue.main.async {
              if granted { application.registerForRemoteNotifications(); result(nil) }
              else { result(FlutterError(code: "permission_denied", message: "Notifications permission denied", details: nil)) }
            }
          }
        case "register":
          application.registerForRemoteNotifications(); result(nil)
        case "disable":
          application.unregisterForRemoteNotifications(); self.pendingToken = nil
          for id in Array(self.wakeups.keys) { self.completeWake(id, .noData) }
          result(nil)
        case "completeWake":
          if let args = call.arguments as? [String: Any], let id = args["id"] as? String {
            self.completeWake(id, (args["updated"] as? Bool == true) ? .newData : .noData)
          }
          result(nil)
        default: result(FlutterMethodNotImplemented)
        }
      }
    }
    return super.application(application, didFinishLaunchingWithOptions: launchOptions)
  }

  override func application(_ application: UIApplication, didRegisterForRemoteNotificationsWithDeviceToken deviceToken: Data) {
    let token = deviceToken.map { String(format: "%02x", $0) }.joined()
    pendingToken = token
    if pushReady { pushChannel?.invokeMethod("token", arguments: token) }
    super.application(application, didRegisterForRemoteNotificationsWithDeviceToken: deviceToken)
  }

  override func application(_ application: UIApplication, didFailToRegisterForRemoteNotificationsWithError error: Error) {
    // Never forward provider errors containing device metadata to telemetry.
    if pushReady { pushChannel?.invokeMethod("registrationFailed", arguments: nil) }
    super.application(application, didFailToRegisterForRemoteNotificationsWithError: error)
  }

  override func application(_ application: UIApplication, didReceiveRemoteNotification userInfo: [AnyHashable: Any],
    fetchCompletionHandler completionHandler: @escaping (UIBackgroundFetchResult) -> Void) {
    guard let payload = userInfo["fedi3"] as? [String: Any], payload["event"] as? String == "sync" else {
      super.application(application, didReceiveRemoteNotification: userInfo, fetchCompletionHandler: completionHandler)
      return
    }
    // Bound memory and OS execution time even when Dart cannot unlock its config.
    guard wakeups.count < 8 else { completionHandler(.noData); return }
    let id = UUID().uuidString
    wakeups[id] = completionHandler
    if pushReady { pushChannel?.invokeMethod("wake", arguments: id) }
    DispatchQueue.main.asyncAfter(deadline: .now() + 25) { [weak self] in self?.completeWake(id, .failed) }
  }

  private func completeWake(_ id: String, _ result: UIBackgroundFetchResult) {
    // Removal makes duplicate/late callbacks safe and guarantees completion once.
    wakeups.removeValue(forKey: id)?(result)
  }
}
