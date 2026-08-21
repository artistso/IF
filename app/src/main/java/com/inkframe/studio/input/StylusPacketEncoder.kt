package com.inkframe.studio.input

import android.view.MotionEvent
import java.nio.ByteBuffer
import java.nio.ByteOrder

internal class StylusPacketEncoder {
    data class Packet(val buffer: ByteBuffer, val sampleCount: Int)

    private var buffer: ByteBuffer = allocate(64)

    fun encode(event: MotionEvent, pointerIndex: Int): Packet {
        val count = event.historySize + 1
        ensureCapacity(count)
        buffer.clear()

        val eraser = event.getToolType(pointerIndex) == MotionEvent.TOOL_TYPE_ERASER
        for (historyIndex in 0 until event.historySize) {
            writeSample(
                x = event.getHistoricalX(pointerIndex, historyIndex),
                y = event.getHistoricalY(pointerIndex, historyIndex),
                pressure = event.getHistoricalPressure(pointerIndex, historyIndex),
                tilt = event.getHistoricalAxisValue(MotionEvent.AXIS_TILT, pointerIndex, historyIndex),
                orientation = event.getHistoricalAxisValue(MotionEvent.AXIS_ORIENTATION, pointerIndex, historyIndex),
                timeNs = event.getHistoricalEventTime(historyIndex) * NANOS_PER_MILLI,
                flags = FLAG_MOVE or if (eraser) FLAG_ERASER else 0,
            )
        }

        writeSample(
            x = event.getX(pointerIndex),
            y = event.getY(pointerIndex),
            pressure = event.getPressure(pointerIndex),
            tilt = event.getAxisValue(MotionEvent.AXIS_TILT, pointerIndex),
            orientation = event.getAxisValue(MotionEvent.AXIS_ORIENTATION, pointerIndex),
            timeNs = event.eventTime * NANOS_PER_MILLI,
            flags = actionFlag(event, pointerIndex) or if (eraser) FLAG_ERASER else 0,
        )
        buffer.position(0)
        return Packet(buffer, count)
    }

    private fun actionFlag(event: MotionEvent, pointerIndex: Int): Int = when (event.actionMasked) {
        MotionEvent.ACTION_DOWN -> FLAG_DOWN
        MotionEvent.ACTION_UP -> FLAG_UP
        MotionEvent.ACTION_CANCEL -> FLAG_CANCEL
        MotionEvent.ACTION_POINTER_DOWN -> if (event.actionIndex == pointerIndex) FLAG_DOWN else FLAG_MOVE
        MotionEvent.ACTION_POINTER_UP -> if (event.actionIndex == pointerIndex) FLAG_UP else FLAG_MOVE
        else -> FLAG_MOVE
    }

    private fun writeSample(
        x: Float,
        y: Float,
        pressure: Float,
        tilt: Float,
        orientation: Float,
        timeNs: Long,
        flags: Int,
    ) {
        buffer.putFloat(x)
        buffer.putFloat(y)
        buffer.putFloat(pressure)
        buffer.putFloat(tilt)
        buffer.putFloat(orientation)
        buffer.putLong(timeNs)
        buffer.putInt(flags)
    }

    private fun ensureCapacity(sampleCount: Int) {
        val required = sampleCount * SAMPLE_STRIDE_BYTES
        if (buffer.capacity() >= required) return
        var sampleCapacity = buffer.capacity() / SAMPLE_STRIDE_BYTES
        while (sampleCapacity < sampleCount) sampleCapacity *= 2
        buffer = allocate(sampleCapacity)
    }

    private fun allocate(sampleCapacity: Int): ByteBuffer =
        ByteBuffer.allocateDirect(sampleCapacity * SAMPLE_STRIDE_BYTES).order(ByteOrder.LITTLE_ENDIAN)

    companion object {
        const val SAMPLE_STRIDE_BYTES = 32
        private const val NANOS_PER_MILLI = 1_000_000L
        private const val FLAG_DOWN = 1 shl 0
        private const val FLAG_MOVE = 1 shl 1
        private const val FLAG_UP = 1 shl 2
        private const val FLAG_CANCEL = 1 shl 3
        private const val FLAG_ERASER = 1 shl 5
    }
}
