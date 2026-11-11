package org.fedi3.client

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder

import io.flutter.FlutterInjector
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.embedding.engine.dart.DartExecutor

/**
 * Keeps the app process alive while the UI is in the background so new activity
 * still reaches the user without a push provider.
 *
 * The service hosts its own headless Flutter engine that runs the
 * `backgroundMain` entrypoint. That entrypoint is push-only: it polls the relay
 * spool over HTTPS and starts the local core only when there is something to
 * deliver, stopping it again after a quiet period. No core and no P2P swarm
 * stay alive while the phone is idle. Measured idle cost is about 16 MB of
 * additional PSS and well under 1% of one core.
 */
class KeepAliveService : Service() {
    override fun onBind(intent: Intent?): IBinder? = null

    private var engine: FlutterEngine? = null

    private fun ensureEngine() {
        if (engine != null) return
        val loader = FlutterInjector.instance().flutterLoader()
        loader.startInitialization(applicationContext)
        loader.ensureInitializationComplete(applicationContext, null)
        // FlutterEngine registers generated plugins itself; registering them a
        // second time replaced plugin instances and made flutter_secure_storage
        // lose its Keystore key in the headless engine.
        val flutterEngine = FlutterEngine(applicationContext)
        val entrypoint = DartExecutor.DartEntrypoint(
            loader.findAppBundlePath(),
            "backgroundMain"
        )
        flutterEngine.dartExecutor.executeDartEntrypoint(entrypoint)
        engine = flutterEngine
    }

    override fun onCreate() {
        super.onCreate()
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            val channel = NotificationChannel(
                CHANNEL_ID,
                "Background sync",
                NotificationManager.IMPORTANCE_LOW
            )
            channel.setShowBadge(false)
            channel.description = "Checks for new activity and wakes Fedi3 when needed"
            getSystemService(NotificationManager::class.java).createNotificationChannel(channel)
        }
        // Android gives startForegroundService() a five second window. Enter the
        // foreground from onCreate, before onStartCommand and before the headless
        // Flutter engine is created, so a busy main thread cannot miss the window.
        startForegroundNotification(prefs(this).getString(KEY_TEXT, null))
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val text = intent?.getStringExtra(EXTRA_TEXT) ?: prefs(this).getString(KEY_TEXT, null)
        if (text != null) {
            prefs(this).edit().putString(KEY_TEXT, text).apply()
        }
        startForegroundNotification(text)
        ensureEngine()
        return START_STICKY
    }

    private fun startForegroundNotification(text: String?) {
        val builder = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            Notification.Builder(this, CHANNEL_ID)
        } else {
            Notification.Builder(this)
        }
        val openAppIntent = Intent(this, MainActivity::class.java).apply {
            // singleTask in the manifest plus CLEAR_TOP keeps one Fedi3 task:
            // the notification must not spawn a second activity whose core
            // would start from scratch and appear offline.
            flags = Intent.FLAG_ACTIVITY_NEW_TASK or
                Intent.FLAG_ACTIVITY_CLEAR_TOP or
                Intent.FLAG_ACTIVITY_SINGLE_TOP
        }
        val openApp = PendingIntent.getActivity(
            this,
            0,
            openAppIntent,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
        )
        val notification: Notification = builder
            .setContentTitle("Fedi3")
            .setContentText(text ?: "Checking for new activity")
            .setSmallIcon(android.R.drawable.stat_notify_sync)
            .setOngoing(true)
            .setContentIntent(openApp)
            .build()
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
            // Android 15 caps dataSync foreground services at six hours per day;
            // remoteMessaging has no such limit and matches what this service does.
            startForeground(
                NOTIFICATION_ID,
                notification,
                ServiceInfo.FOREGROUND_SERVICE_TYPE_REMOTE_MESSAGING
            )
        } else if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            startForeground(NOTIFICATION_ID, notification, ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC)
        } else {
            startForeground(NOTIFICATION_ID, notification)
        }
    }

    override fun onDestroy() {
        engine?.destroy()
        engine = null
        super.onDestroy()
    }

    companion object {
        private const val CHANNEL_ID = "fedi3_keepalive"
        private const val NOTIFICATION_ID = 1003
        private const val PREFS = "fedi3_keepalive"
        private const val KEY_ENABLED = "enabled"
        private const val KEY_TEXT = "text"
        const val EXTRA_TEXT = "text"

        private fun prefs(context: Context) =
            context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)

        /** Defaults to enabled: notifications are the point of the service. */
        fun isEnabled(context: Context): Boolean =
            prefs(context).getBoolean(KEY_ENABLED, true)

        fun setEnabled(context: Context, enabled: Boolean, text: String?) {
            prefs(context).edit().putBoolean(KEY_ENABLED, enabled).apply()
            if (text != null) {
                prefs(context).edit().putString(KEY_TEXT, text).apply()
            }
        }

        fun start(context: Context, text: String?) {
            val intent = Intent(context, KeepAliveService::class.java).putExtra(EXTRA_TEXT, text)
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                context.startForegroundService(intent)
            } else {
                context.startService(intent)
            }
        }

        fun stop(context: Context) {
            context.stopService(Intent(context, KeepAliveService::class.java))
        }
    }
}
