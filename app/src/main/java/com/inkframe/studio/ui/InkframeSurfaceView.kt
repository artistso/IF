package com.inkframe.studio.ui

import android.content.Context
import android.view.MotionEvent
import android.view.SurfaceHolder
import android.view.SurfaceView
import com.inkframe.studio.engine.NativeEngine
import com.inkframe.studio.input.StylusPacketEncoder

class InkframeSurfaceView(
    context: Context,
    private val engine: NativeEngine,
) : SurfaceView(context), SurfaceHolder.Callback {
    private val encoder = StylusPacketEncoder()
    private var surfaceAttached = false

    init {
        holder.addCallback(this)
        isFocusable = true
        isFocusableInTouchMode = true
    }

    override fun surfaceCreated(holder: SurfaceHolder) {
        val frame = holder.surfaceFrame
        if (frame.width() > 0 && frame.height() > 0) {
            surfaceAttached = engine.attachSurface(holder.surface, frame.width(), frame.height())
        }
    }

    override fun surfaceChanged(holder: SurfaceHolder, format: Int, width: Int, height: Int) {
        if (width <= 0 || height <= 0) return

        if (!surfaceAttached) {
            surfaceAttached = engine.attachSurface(holder.surface, width, height)
        } else {
            engine.resizeSurface(width, height)
        }
    }

    override fun surfaceDestroyed(holder: SurfaceHolder) {
        if (surfaceAttached) {
            engine.detachSurface()
            surfaceAttached = false
        }
    }

    override fun onTouchEvent(event: MotionEvent): Boolean {
        val pointerIndex = findStylusPointer(event)
        if (pointerIndex < 0) return false

        parent?.requestDisallowInterceptTouchEvent(true)
        val packet = encoder.encode(event, pointerIndex)
        engine.pushInput(packet.buffer, packet.sampleCount)
        return true
    }

    private fun findStylusPointer(event: MotionEvent): Int {
        for (index in 0 until event.pointerCount) {
            when (event.getToolType(index)) {
                MotionEvent.TOOL_TYPE_STYLUS,
                MotionEvent.TOOL_TYPE_ERASER,
                -> return index
            }
        }
        return -1
    }
}
