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

/**
 * Keeps the app process (and therefore the local core plus its event streams)
 * alive while the UI is in the background, so notifications keep arriving
 * without waiting for a push provider.
 *
 * The service never owns a Flutter engine: the activity's engine keeps running
 * while the activity is only stopped, and the foreground service prevents the
 * system from reclaiming the process.
 */
class KeepAliveService : Service() {
    override fun onBind(intent: Intent?): IBinder? = null

    override fun onCreate() {
        super.onCreate()
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            val channel = NotificationChannel(
                CHANNEL_ID,
                "Background sync",
                NotificationManager.IMPORTANCE_LOW
            )
            channel.setShowBadge(false)
            channel.description = "Keeps Fedi3 connected for notifications"
            getSystemService(NotificationManager::class.java).createNotificationChannel(channel)
        }
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val text = intent?.getStringExtra(EXTRA_TEXT) ?: prefs(this).getString(KEY_TEXT, null)
        if (text != null) {
            prefs(this).edit().putString(KEY_TEXT, text).apply()
        }
        val builder = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            Notification.Builder(this, CHANNEL_ID)
        } else {
            Notification.Builder(this)
        }
        val openApp = PendingIntent.getActivity(
            this,
            0,
            Intent(this, MainActivity::class.java),
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
        )
        val notification: Notification = builder
            .setContentTitle("Fedi3")
            .setContentText(text ?: "Connected for notifications")
            .setSmallIcon(android.R.drawable.stat_notify_sync)
            .setOngoing(true)
            .setContentIntent(openApp)
            .build()
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            startForeground(NOTIFICATION_ID, notification, ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC)
        } else {
            startForeground(NOTIFICATION_ID, notification)
        }
        return START_STICKY
    }

    override fun onDestroy() {
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
