package org.fedi3.client

import android.app.Application
import android.content.Context

/** Initialise the native Android context before either Flutter engine starts. */
class Fedi3Application : Application() {
    override fun onCreate() {
        super.onCreate()
        System.loadLibrary("fedi3_core")
        nativeInitialize(this)
    }

    private external fun nativeInitialize(context: Context)
}
