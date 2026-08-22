package com.inkframe.studio.engine

import android.view.Surface
import java.io.Closeable
import java.nio.ByteBuffer

class NativeEngine : Closeable {
    @Volatile
    private var engineId: Long = NativeBridge.createEngine()

    init {
        check(engineId != 0L) { "InkFrame Rust/Vulkan engine could not be created" }
    }

    fun attachSurface(surface: Surface, width: Int, height: Int): Boolean {
        val id = engineId
        return id != 0L && NativeBridge.attachSurface(id, surface, width, height)
    }

    fun detachSurface(): Boolean {
        val id = engineId
        return id != 0L && NativeBridge.detachSurface(id)
    }

    fun resizeSurface(width: Int, height: Int): Boolean {
        val id = engineId
        return id != 0L && NativeBridge.resizeSurface(id, width, height)
    }

    fun setBrush(colorRgb: Int, sizePx: Float, opacity: Float, eraser: Boolean): Boolean {
        val id = engineId
        return id != 0L && NativeBridge.setBrush(id, colorRgb, sizePx, opacity, eraser)
    }

    fun pushInput(buffer: ByteBuffer, sampleCount: Int): Boolean {
        val id = engineId
        return id != 0L && NativeBridge.pushInput(id, buffer, sampleCount)
    }

    @Synchronized
    override fun close() {
        val id = engineId
        if (id != 0L) {
            engineId = 0L
            NativeBridge.destroyEngine(id)
        }
    }
}
