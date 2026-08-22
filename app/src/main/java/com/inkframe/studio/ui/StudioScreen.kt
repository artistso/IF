package com.inkframe.studio.ui

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.viewinterop.AndroidView
import com.inkframe.studio.engine.NativeEngine
import kotlin.math.sin

private val InkframePink = Color(0xFFD60057)
private val InkframePinkHot = Color(0xFFFF4C91)
private val InkframeRose = Color(0xFF8B123F)
private val InkframeNight = Color(0xFF0E0710)
private val InkframePaper = Color(0xFFFFF2F5)
private val InkframeGlass = Color(0x66FFF5F8)
private val InkframeGlassDark = Color(0x8A2B1723)
private val InkframeLine = Color(0x55FFFFFF)
private val InkframeText = Color(0xFFFFF8FA)
private val InkframeMuted = Color(0xFFD8C2CB)

@Composable
fun StudioScreen(engine: NativeEngine) {
    BoxWithConstraints(
        modifier = Modifier
            .fillMaxSize()
            .background(
                Brush.verticalGradient(
                    colors = listOf(
                        Color(0xFFF29AB7),
                        Color(0xFFB62E63),
                        Color(0xFF4D0B30),
                        InkframeNight,
                    ),
                ),
            )
            .statusBarsPadding()
            .navigationBarsPadding(),
    ) {
        if (maxWidth >= 840.dp) {
            OrbitStudioWide(engine)
        } else {
            OrbitStudioCompact(engine)
        }
    }
}

@Composable
private fun OrbitStudioWide(engine: NativeEngine) {
    var fanExpanded by remember { mutableStateOf(true) }

    Box(Modifier.fillMaxSize()) {
        StudioGlowBackdrop()

        CanvasStage(
            engine = engine,
            modifier = Modifier
                .align(Alignment.Center)
                .fillMaxWidth(0.72f)
                .fillMaxHeight(0.82f),
        )

        TopModeCluster(
            modifier = Modifier
                .align(Alignment.TopCenter)
                .padding(top = 4.dp),
        )

        Column(
            modifier = Modifier
                .align(Alignment.CenterStart)
                .padding(start = 16.dp),
            verticalArrangement = Arrangement.spacedBy(28.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            SideAction("T", "TOOLS", selected = fanExpanded) { fanExpanded = !fanExpanded }
            SideAction("~", "LINE")
        }

        Column(
            modifier = Modifier
                .align(Alignment.CenterEnd)
                .padding(end = 16.dp),
            verticalArrangement = Arrangement.spacedBy(28.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            SideAction("C", "COLOR")
            SideAction("★", "FX")
            SideAction("+", "ACTIONS")
            SideAction("◐", "THEMES")
        }

        if (fanExpanded) {
            ToolFan(
                modifier = Modifier
                    .align(Alignment.CenterStart)
                    .offset(x = 92.dp, y = 28.dp),
            )
        }

        BottomOrbitDock(
            modifier = Modifier
                .align(Alignment.BottomCenter)
                .fillMaxWidth()
                .padding(horizontal = 18.dp, vertical = 4.dp),
        )
    }
}

@Composable
private fun OrbitStudioCompact(engine: NativeEngine) {
    Column(Modifier.fillMaxSize()) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .height(52.dp)
                .background(InkframeGlassDark)
                .padding(horizontal = 12.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text("InkFrame", color = InkframeText, fontWeight = FontWeight.Bold, fontSize = 16.sp)
            Spacer(Modifier.weight(1f))
            GlassPill("ENGINE · V2", selected = true)
            Spacer(Modifier.width(6.dp))
            GlassPill("BRUSH LAB")
        }

        Box(
            modifier = Modifier
                .weight(1f)
                .fillMaxWidth()
                .padding(10.dp),
        ) {
            CanvasPaper(
                engine = engine,
                modifier = Modifier.fillMaxSize(),
            )
            Surface(
                modifier = Modifier
                    .align(Alignment.TopEnd)
                    .padding(10.dp),
                color = InkframeGlassDark,
                shape = RoundedCornerShape(999.dp),
            ) {
                Text(
                    "□ SQUARE",
                    modifier = Modifier.padding(horizontal = 12.dp, vertical = 7.dp),
                    color = InkframeText,
                    fontSize = 9.sp,
                    fontWeight = FontWeight.Bold,
                )
            }
        }

        Row(
            modifier = Modifier
                .fillMaxWidth()
                .height(72.dp)
                .background(InkframeGlassDark)
                .padding(horizontal = 10.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.SpaceEvenly,
        ) {
            CompactDock("TOOLS")
            CompactDock("COLOR")
            CompactDock("FRAMES")
            CompactDock("LAYERS")
            CompactDock("SELECT")
        }
    }
}

@Composable
private fun StudioGlowBackdrop() {
    Canvas(Modifier.fillMaxSize()) {
        drawCircle(
            color = InkframePinkHot.copy(alpha = 0.18f),
            radius = size.minDimension * 0.42f,
            center = Offset(size.width * 0.50f, size.height * 0.10f),
        )
        drawCircle(
            color = InkframePink.copy(alpha = 0.16f),
            radius = size.minDimension * 0.34f,
            center = Offset(size.width * 0.10f, size.height * 0.42f),
        )
        drawCircle(
            color = Color.Black.copy(alpha = 0.22f),
            radius = size.minDimension * 0.52f,
            center = Offset(size.width * 0.72f, size.height * 0.95f),
        )
    }
}

@Composable
private fun TopModeCluster(modifier: Modifier = Modifier) {
    Column(
        modifier = modifier,
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Surface(
            color = InkframeGlassDark,
            shape = RoundedCornerShape(18.dp),
            border = BorderStroke(1.dp, InkframeLine),
            shadowElevation = 10.dp,
        ) {
            Row(Modifier.padding(5.dp), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                ModeTab("Engine · V2", selected = true)
                ModeTab("Brush Lab")
            }
        }
        Spacer(Modifier.height(4.dp))
        Surface(
            color = InkframeGlassDark.copy(alpha = 0.88f),
            shape = RoundedCornerShape(999.dp),
            border = BorderStroke(1.dp, InkframeLine),
        ) {
            Row(Modifier.padding(4.dp), horizontalArrangement = Arrangement.spacedBy(3.dp)) {
                MiniMode("PLAY")
                MiniMode("CENTER")
                MiniMode("ALL RINGS")
                MiniMode("SCRUB")
                MiniMode("TIMING")
            }
        }
    }
}

@Composable
private fun ModeTab(label: String, selected: Boolean = false) {
    Surface(
        color = if (selected) InkframePink else InkframeRose.copy(alpha = 0.78f),
        shape = RoundedCornerShape(12.dp),
        border = BorderStroke(1.dp, Color.White.copy(alpha = 0.28f)),
    ) {
        Text(
            label,
            modifier = Modifier.padding(horizontal = 24.dp, vertical = 12.dp),
            color = Color.White,
            fontSize = 11.sp,
            fontWeight = FontWeight.Bold,
        )
    }
}

@Composable
private fun MiniMode(label: String) {
    Surface(
        color = Color.White.copy(alpha = 0.08f),
        shape = RoundedCornerShape(999.dp),
    ) {
        Text(
            label,
            modifier = Modifier.padding(horizontal = 18.dp, vertical = 8.dp),
            color = InkframeMuted,
            fontSize = 8.sp,
            fontWeight = FontWeight.Bold,
            letterSpacing = 0.5.sp,
        )
    }
}

@Composable
private fun CanvasStage(engine: NativeEngine, modifier: Modifier = Modifier) {
    Box(modifier = modifier) {
        CanvasPaper(
            engine = engine,
            modifier = Modifier
                .align(Alignment.Center)
                .fillMaxWidth(0.88f)
                .fillMaxHeight(0.88f),
        )

        ShapeBadge(
            modifier = Modifier
                .align(Alignment.TopEnd)
                .offset(x = (-42).dp, y = 42.dp),
        )

        OrbitMarkers()
    }
}

@Composable
private fun CanvasPaper(engine: NativeEngine, modifier: Modifier = Modifier) {
    Box(
        modifier = modifier
            .shadow(22.dp, RoundedCornerShape(26.dp))
            .background(InkframeRose.copy(alpha = 0.72f), RoundedCornerShape(26.dp))
            .border(1.dp, Color.White.copy(alpha = 0.38f), RoundedCornerShape(26.dp))
            .padding(10.dp)
            .clip(RoundedCornerShape(20.dp))
            .background(InkframePaper),
    ) {
        AndroidView(
            modifier = Modifier.fillMaxSize(),
            factory = { context -> InkframeSurfaceView(context, engine) },
        )

        Canvas(modifier = Modifier.fillMaxSize()) {
            val c = Color.White.copy(alpha = 0.20f)
            drawLine(c, Offset(0f, 0f), Offset(size.width * 0.03f, 0f), strokeWidth = 2f)
            drawLine(c, Offset(0f, 0f), Offset(0f, size.height * 0.04f), strokeWidth = 2f)
            drawLine(c, Offset(size.width, 0f), Offset(size.width * 0.97f, 0f), strokeWidth = 2f)
            drawLine(c, Offset(size.width, 0f), Offset(size.width, size.height * 0.04f), strokeWidth = 2f)
        }
    }
}

@Composable
private fun ShapeBadge(modifier: Modifier = Modifier) {
    Surface(
        modifier = modifier,
        color = InkframeGlassDark,
        shape = RoundedCornerShape(999.dp),
        border = BorderStroke(1.dp, Color.White.copy(alpha = 0.18f)),
    ) {
        Text(
            "□ SQUARE",
            modifier = Modifier.padding(horizontal = 18.dp, vertical = 9.dp),
            color = InkframeText,
            fontSize = 9.sp,
            fontWeight = FontWeight.Bold,
            letterSpacing = 0.5.sp,
        )
    }
}

@Composable
private fun OrbitMarkers() {
    Box(Modifier.fillMaxSize()) {
        val top = listOf(
            Triple((-110).dp, 22.dp, "12"),
            Triple(52.dp, 42.dp, "11"),
            Triple(172.dp, 88.dp, "10"),
        )
        top.forEach { (x, y, label) ->
            OrbitMarker(
                label,
                modifier = Modifier
                    .align(Alignment.TopCenter)
                    .offset(x = x, y = y),
            )
        }

        val right = listOf(
            Triple((-2).dp, (-158).dp, "9"),
            Triple(14.dp, (-78).dp, "8"),
            Triple(18.dp, 4.dp, "7"),
            Triple(12.dp, 84.dp, "6"),
            Triple((-6).dp, 160.dp, "5"),
        )
        right.forEach { (x, y, label) ->
            OrbitMarker(
                label,
                modifier = Modifier
                    .align(Alignment.CenterEnd)
                    .offset(x = x, y = y),
            )
        }

        listOf(
            Triple((-198).dp, (-6).dp, "4"),
            Triple((-116).dp, (-2).dp, "3"),
            Triple((-34).dp, 10.dp, "2"),
            Triple(68.dp, 14.dp, "1"),
        ).forEach { (x, y, label) ->
            OrbitMarker(
                label,
                modifier = Modifier
                    .align(Alignment.BottomCenter)
                    .offset(x = x, y = y),
            )
        }
    }
}

@Composable
private fun OrbitMarker(label: String, modifier: Modifier = Modifier) {
    Column(
        modifier = modifier,
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Box(
            modifier = Modifier
                .size(34.dp)
                .shadow(8.dp, CircleShape)
                .background(InkframePaper.copy(alpha = 0.94f), CircleShape)
                .border(1.dp, InkframeRose.copy(alpha = 0.28f), CircleShape),
        )
        Box(
            modifier = Modifier
                .offset(y = (-4).dp)
                .size(17.dp)
                .background(InkframeNight.copy(alpha = 0.88f), CircleShape),
            contentAlignment = Alignment.Center,
        ) {
            Text(label, color = Color.White, fontSize = 7.sp, fontWeight = FontWeight.Bold)
        }
    }
}

@Composable
private fun SideAction(
    glyph: String,
    label: String,
    selected: Boolean = false,
    onClick: () -> Unit = {},
) {
    Column(
        horizontalAlignment = Alignment.CenterHorizontally,
        modifier = Modifier.clickable(onClick = onClick),
    ) {
        Box(
            modifier = Modifier
                .size(64.dp)
                .shadow(14.dp, CircleShape)
                .background(
                    if (selected) InkframePinkHot.copy(alpha = 0.74f) else InkframePink.copy(alpha = 0.56f),
                    CircleShape,
                )
                .border(1.dp, Color.White.copy(alpha = 0.32f), CircleShape),
            contentAlignment = Alignment.Center,
        ) {
            Text(glyph, color = Color.White, fontSize = 19.sp, fontWeight = FontWeight.Bold)
        }
        Spacer(Modifier.height(5.dp))
        Text(
            label,
            color = InkframeText,
            fontSize = 8.sp,
            fontWeight = FontWeight.Bold,
            letterSpacing = 1.sp,
        )
    }
}

@Composable
private fun ToolFan(modifier: Modifier = Modifier) {
    val items = listOf(
        "STYLUS",
        "PALM",
        "TEXT",
        "PICK",
        "PEN",
        "FULL",
        "EXPAND",
        "GIF",
        "PNG",
        "COMPARE",
        "PAST",
        "TINT",
        "GHOST",
        "ONION",
        "CLEAR",
        "REDO",
        "UNDO",
    )

    Box(
        modifier = modifier
            .width(300.dp)
            .height(470.dp),
    ) {
        Canvas(Modifier.fillMaxSize()) {
            val anchor = Offset(238.dp.toPx(), 240.dp.toPx())
            items.forEachIndexed { index, _ ->
                val t = index / (items.size - 1f)
                val y = 24.dp.toPx() + t * 410.dp.toPx()
                val wave = sin(t * Math.PI).toFloat()
                val x = (28.dp + (84.dp * wave)).toPx()
                drawLine(
                    color = Color.White.copy(alpha = 0.10f),
                    start = anchor,
                    end = Offset(x + 18.dp.toPx(), y + 18.dp.toPx()),
                    strokeWidth = 1.dp.toPx(),
                    cap = StrokeCap.Round,
                )
            }
        }

        items.forEachIndexed { index, label ->
            val t = index / (items.size - 1f)
            val wave = sin(t * Math.PI).toFloat()
            val x = 10.dp + 78.dp * wave
            val y = 8.dp + 25.dp * index
            FanOrb(label, Modifier.offset(x = x, y = y))
        }

        Box(
            modifier = Modifier
                .align(Alignment.CenterEnd)
                .size(58.dp)
                .background(InkframeGlass.copy(alpha = 0.42f), CircleShape)
                .border(1.dp, Color.White.copy(alpha = 0.30f), CircleShape),
            contentAlignment = Alignment.Center,
        ) {
            Text("…", color = Color.White, fontSize = 22.sp)
        }
    }
}

@Composable
private fun FanOrb(label: String, modifier: Modifier = Modifier) {
    Row(
        modifier = modifier,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(
            modifier = Modifier
                .size(38.dp)
                .shadow(8.dp, CircleShape)
                .background(InkframeRose.copy(alpha = 0.72f), CircleShape)
                .border(1.dp, Color.White.copy(alpha = 0.18f), CircleShape),
            contentAlignment = Alignment.Center,
        ) {
            Text(label.take(1), color = Color.White, fontSize = 9.sp, fontWeight = FontWeight.Bold)
        }
        Spacer(Modifier.width(4.dp))
        Text(
            label,
            color = Color.White.copy(alpha = 0.78f),
            fontSize = 7.sp,
            fontWeight = FontWeight.Bold,
        )
    }
}

@Composable
private fun BottomOrbitDock(modifier: Modifier = Modifier) {
    Box(modifier = modifier.height(88.dp)) {
        Canvas(
            modifier = Modifier
                .align(Alignment.BottomCenter)
                .fillMaxWidth()
                .height(26.dp),
        ) {
            val y = size.height * 0.48f
            drawLine(
                color = Color.White.copy(alpha = 0.22f),
                start = Offset(30.dp.toPx(), y),
                end = Offset(size.width - 30.dp.toPx(), y),
                strokeWidth = 1.dp.toPx(),
            )
            for (i in 0..16) {
                val x = 48.dp.toPx() + (size.width - 96.dp.toPx()) * (i / 16f)
                drawCircle(
                    color = if (i == 9) InkframePinkHot else Color.White.copy(alpha = 0.26f),
                    radius = if (i == 9) 4.dp.toPx() else 2.dp.toPx(),
                    center = Offset(x, y),
                )
            }
        }

        Row(
            modifier = Modifier
                .align(Alignment.BottomCenter)
                .padding(bottom = 10.dp),
            horizontalArrangement = Arrangement.spacedBy(24.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            DockOrb("▦", "FRAMES")
            DockOrb("◇", "LAYERS")
            DockOrb("□", "SELECT")
        }

        Row(
            modifier = Modifier.align(Alignment.BottomStart),
            horizontalArrangement = Arrangement.spacedBy(10.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            SmallDockOrb("◉")
            SmallDockOrb("▣")
        }

        Surface(
            modifier = Modifier.align(Alignment.BottomEnd),
            color = InkframeRose.copy(alpha = 0.82f),
            shape = RoundedCornerShape(999.dp),
            border = BorderStroke(1.dp, Color.White.copy(alpha = 0.24f)),
        ) {
            Text(
                "REPORT",
                modifier = Modifier.padding(horizontal = 18.dp, vertical = 10.dp),
                color = Color.White,
                fontSize = 8.sp,
                fontWeight = FontWeight.Bold,
                letterSpacing = 0.8.sp,
            )
        }
    }
}

@Composable
private fun DockOrb(glyph: String, label: String) {
    Column(horizontalAlignment = Alignment.CenterHorizontally) {
        Box(
            modifier = Modifier
                .size(60.dp)
                .shadow(12.dp, CircleShape)
                .background(InkframeGlassDark.copy(alpha = 0.92f), CircleShape)
                .border(1.dp, Color.White.copy(alpha = 0.24f), CircleShape),
            contentAlignment = Alignment.Center,
        ) {
            Text(glyph, color = Color.White, fontSize = 20.sp)
        }
        Text(label, color = InkframeMuted, fontSize = 7.sp, fontWeight = FontWeight.Bold)
    }
}

@Composable
private fun SmallDockOrb(glyph: String) {
    Box(
        modifier = Modifier
            .size(48.dp)
            .background(InkframeGlassDark.copy(alpha = 0.82f), CircleShape)
            .border(1.dp, Color.White.copy(alpha = 0.18f), CircleShape),
        contentAlignment = Alignment.Center,
    ) {
        Text(glyph, color = Color.White, fontSize = 17.sp)
    }
}

@Composable
private fun GlassPill(label: String, selected: Boolean = false) {
    Surface(
        color = if (selected) InkframePink.copy(alpha = 0.84f) else InkframeGlassDark,
        shape = RoundedCornerShape(999.dp),
        border = BorderStroke(1.dp, Color.White.copy(alpha = 0.16f)),
    ) {
        Text(
            label,
            modifier = Modifier.padding(horizontal = 11.dp, vertical = 7.dp),
            color = Color.White,
            fontSize = 8.sp,
            fontWeight = FontWeight.Bold,
        )
    }
}

@Composable
private fun CompactDock(label: String) {
    Column(horizontalAlignment = Alignment.CenterHorizontally) {
        Box(
            modifier = Modifier
                .size(38.dp)
                .background(InkframePink.copy(alpha = 0.62f), CircleShape)
                .border(1.dp, Color.White.copy(alpha = 0.22f), CircleShape),
        )
        Text(label, color = InkframeMuted, fontSize = 7.sp, fontWeight = FontWeight.Bold)
    }
}
