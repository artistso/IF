package com.inkframe.studio.engine

import android.view.Surface
import java.nio.ByteBuffer

internal object NativeBridge {
    init {
        System.loadLibrary("inkframe_android")
    }

    @JvmStatic external fun createEngine(): Long
    @JvmStatic external fun destroyEngine(engineId: Long)
    @JvmStatic external fun attachSurface(engineId: Long, surface: Surface, width: Int, height: Int): Boolean
    @JvmStatic external fun detachSurface(engineId: Long): Boolean
    @JvmStatic external fun resizeSurface(engineId: Long, width: Int, height: Int): Boolean
    @JvmStatic external fun setBrush(
        engineId: Long,
        colorRgb: Int,
        sizePx: Float,
        opacity: Float,
        eraser: Boolean,
    ): Boolean
    @JvmStatic external fun pushInput(engineId: Long, buffer: ByteBuffer, sampleCount: Int): Boolean
    @JvmStatic external fun performanceStats(engineId: Long): String
}
