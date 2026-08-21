package com.inkframe.studio

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import com.inkframe.studio.engine.NativeEngine
import com.inkframe.studio.ui.StudioScreen

class MainActivity : ComponentActivity() {
    private lateinit var engine: NativeEngine

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        engine = NativeEngine()
        enableEdgeToEdge()
        setContent {
            MaterialTheme {
                Surface {
                    StudioScreen(engine)
                }
            }
        }
    }

    override fun onDestroy() {
        if (::engine.isInitialized) engine.close()
        super.onDestroy()
    }
}
