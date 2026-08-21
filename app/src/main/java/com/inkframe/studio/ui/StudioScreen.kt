package com.inkframe.studio.ui

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.viewinterop.AndroidView
import com.inkframe.studio.engine.NativeEngine

@Composable
fun StudioScreen(engine: NativeEngine) {
    Box(Modifier.fillMaxSize()) {
        AndroidView(
            modifier = Modifier.fillMaxSize(),
            factory = { context -> InkframeSurfaceView(context, engine) },
        )
    }
}
