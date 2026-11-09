package org.fedi3.client

import android.os.Build
import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodChannel

class MainActivity : FlutterActivity() {
    private val channelName = "org.fedi3.client/keepalive"

    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)
        MethodChannel(flutterEngine.dartExecutor.binaryMessenger, channelName)
            .setMethodCallHandler { call, result ->
                when (call.method) {
                    "setEnabled" -> {
                        val enabled = call.argument<Boolean>("enabled") ?: false
                        val text = call.argument<String>("text")
                        KeepAliveService.setEnabled(this, enabled, text)
                        if (enabled && !hasWindowFocus()) {
                            KeepAliveService.start(this, text)
                        }
                        if (!enabled) KeepAliveService.stop(this)
                        result.success(enabled)
                    }
                    "isEnabled" -> result.success(KeepAliveService.isEnabled(this))
                    "isRunning" -> result.success(isServiceRunning())
                    "startScreenShare" -> {
                        // The projection can only be created while a
                        // mediaProjection foreground service is running.
                        ScreenShareService.start(this)
                        result.success(true)
                    }
                    "stopScreenShare" -> {
                        ScreenShareService.stop(this)
                        result.success(true)
                    }
                    else -> result.notImplemented()
                }
            }
    }

    override fun onStart() {
        super.onStart()
        // The UI is visible again: the activity's engine already delivers
        // notifications, so the keep-alive service is not needed.
        KeepAliveService.stop(this)
    }

    override fun onPause() {
        // Start the foreground service while the app is still foreground, so
        // the five second startForeground window cannot be lost behind a slow
        // Flutter engine teardown in onStop.
        if (KeepAliveService.isEnabled(this)) {
            KeepAliveService.start(this, null)
        }
        super.onPause()
    }

    override fun onStop() {
        // Safety net when onPause was skipped (for example during a fast task
        // switch); starting an already running service is harmless.
        if (KeepAliveService.isEnabled(this)) {
            KeepAliveService.start(this, null)
        }
        super.onStop()
    }

    private fun isServiceRunning(): Boolean {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            val manager = getSystemService(android.app.ActivityManager::class.java)
            manager.getRunningServices(Int.MAX_VALUE).forEach { service ->
                if (service.service.className == KeepAliveService::class.java.name) return true
            }
        }
        return false
    }
}
