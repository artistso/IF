package com.inkframe.studio.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
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
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.VerticalDivider
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.viewinterop.AndroidView
import com.inkframe.studio.engine.NativeEngine

private val StudioBackground = Color(0xFF17181D)
private val Chrome = Color(0xFF202127)
private val ChromeRaised = Color(0xFF292A31)
private val ChromeBorder = Color(0xFF363740)
private val PrimaryText = Color(0xFFF3F3F6)
private val SecondaryText = Color(0xFFA7A8B0)
private val Accent = Color(0xFF9EACFF)
private val CanvasFrame = Color(0xFF101116)

@Composable
fun StudioScreen(engine: NativeEngine) {
    BoxWithConstraints(
        modifier = Modifier
            .fillMaxSize()
            .background(StudioBackground)
            .statusBarsPadding()
            .navigationBarsPadding(),
    ) {
        val wideLayout = maxWidth >= 840.dp

        Column(Modifier.fillMaxSize()) {
            StudioTopBar(wideLayout = wideLayout)
            HorizontalDivider(color = ChromeBorder)

            if (wideLayout) {
                Row(
                    modifier = Modifier
                        .weight(1f)
                        .fillMaxWidth(),
                ) {
                    ToolRail(
                        modifier = Modifier
                            .width(68.dp)
                            .fillMaxHeight(),
                    )
                    VerticalDivider(color = ChromeBorder)

                    CanvasViewport(
                        engine = engine,
                        modifier = Modifier
                            .weight(1f)
                            .fillMaxHeight(),
                    )

                    VerticalDivider(color = ChromeBorder)
                    LayersPanel(
                        modifier = Modifier
                            .width(224.dp)
                            .fillMaxHeight(),
                    )
                }
            } else {
                Column(
                    modifier = Modifier
                        .weight(1f)
                        .fillMaxWidth(),
                ) {
                    CompactToolStrip()
                    HorizontalDivider(color = ChromeBorder)
                    CanvasViewport(
                        engine = engine,
                        modifier = Modifier
                            .weight(1f)
                            .fillMaxWidth(),
                    )
                }
            }

            HorizontalDivider(color = ChromeBorder)
            TimelinePanel(compact = !wideLayout)
        }
    }
}

@Composable
private fun StudioTopBar(wideLayout: Boolean) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .height(54.dp)
            .background(Chrome)
            .padding(horizontal = 14.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            text = "InkFrame",
            color = PrimaryText,
            fontWeight = FontWeight.Bold,
            fontSize = 18.sp,
        )
        Spacer(Modifier.width(12.dp))
        Surface(
            color = ChromeRaised,
            shape = RoundedCornerShape(8.dp),
        ) {
            Text(
                text = "Untitled",
                modifier = Modifier.padding(horizontal = 10.dp, vertical = 6.dp),
                color = PrimaryText,
                fontSize = 12.sp,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
        Spacer(Modifier.width(8.dp))
        Text(
            text = "LOCAL",
            color = Accent,
            fontWeight = FontWeight.SemiBold,
            fontSize = 10.sp,
            letterSpacing = 0.8.sp,
        )

        Spacer(Modifier.weight(1f))

        if (wideLayout) {
            StatusPill("VULKAN LIVE")
            Spacer(Modifier.width(8.dp))
            StatusPill("100%")
            Spacer(Modifier.width(8.dp))
        }
        DisabledAction("EXPORT")
    }
}

@Composable
private fun StatusPill(label: String) {
    Surface(
        color = ChromeRaised,
        shape = RoundedCornerShape(999.dp),
        border = androidx.compose.foundation.BorderStroke(1.dp, ChromeBorder),
    ) {
        Text(
            text = label,
            modifier = Modifier.padding(horizontal = 10.dp, vertical = 5.dp),
            color = SecondaryText,
            fontSize = 10.sp,
            fontWeight = FontWeight.Medium,
        )
    }
}

@Composable
private fun DisabledAction(label: String) {
    Surface(
        color = ChromeRaised,
        shape = RoundedCornerShape(8.dp),
        border = androidx.compose.foundation.BorderStroke(1.dp, ChromeBorder),
    ) {
        Text(
            text = label,
            modifier = Modifier.padding(horizontal = 12.dp, vertical = 7.dp),
            color = SecondaryText,
            fontSize = 10.sp,
            fontWeight = FontWeight.Bold,
            letterSpacing = 0.6.sp,
        )
    }
}

@Composable
private fun ToolRail(modifier: Modifier = Modifier) {
    Column(
        modifier = modifier
            .background(Chrome)
            .padding(vertical = 10.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        ToolTile("PEN", selected = true)
        ToolTile("ERASE")
        ToolTile("SELECT")
        ToolTile("SHAPE")
        ToolTile("TEXT")
        Spacer(Modifier.weight(1f))
        ToolTile("UNDO")
        ToolTile("REDO")
    }
}

@Composable
private fun CompactToolStrip() {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .height(54.dp)
            .background(Chrome)
            .padding(horizontal = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        ToolChip("PEN", selected = true)
        ToolChip("ERASE")
        ToolChip("SELECT")
        Spacer(Modifier.weight(1f))
        ToolChip("UNDO")
        ToolChip("REDO")
    }
}

@Composable
private fun ToolTile(label: String, selected: Boolean = false) {
    Box(
        modifier = Modifier
            .size(width = 52.dp, height = 44.dp)
            .clip(RoundedCornerShape(10.dp))
            .background(if (selected) Accent.copy(alpha = 0.16f) else Color.Transparent)
            .border(
                width = 1.dp,
                color = if (selected) Accent.copy(alpha = 0.55f) else ChromeBorder,
                shape = RoundedCornerShape(10.dp),
            ),
        contentAlignment = Alignment.Center,
    ) {
        Text(
            text = label,
            color = if (selected) Accent else SecondaryText,
            fontSize = 9.sp,
            fontWeight = FontWeight.Bold,
        )
    }
}

@Composable
private fun ToolChip(label: String, selected: Boolean = false) {
    Surface(
        color = if (selected) Accent.copy(alpha = 0.16f) else ChromeRaised,
        shape = RoundedCornerShape(8.dp),
        border = androidx.compose.foundation.BorderStroke(
            1.dp,
            if (selected) Accent.copy(alpha = 0.55f) else ChromeBorder,
        ),
    ) {
        Text(
            text = label,
            modifier = Modifier.padding(horizontal = 10.dp, vertical = 7.dp),
            color = if (selected) Accent else SecondaryText,
            fontSize = 9.sp,
            fontWeight = FontWeight.Bold,
        )
    }
}

@Composable
private fun CanvasViewport(engine: NativeEngine, modifier: Modifier = Modifier) {
    Box(
        modifier = modifier
            .background(StudioBackground)
            .padding(10.dp),
    ) {
        Box(
            modifier = Modifier
                .fillMaxSize()
                .background(CanvasFrame, RoundedCornerShape(12.dp))
                .border(1.dp, ChromeBorder, RoundedCornerShape(12.dp))
                .clip(RoundedCornerShape(12.dp)),
        ) {
            AndroidView(
                modifier = Modifier.fillMaxSize(),
                factory = { context -> InkframeSurfaceView(context, engine) },
            )

            Surface(
                modifier = Modifier
                    .align(Alignment.TopStart)
                    .padding(12.dp),
                color = Chrome.copy(alpha = 0.92f),
                shape = RoundedCornerShape(8.dp),
            ) {
                Text(
                    text = "Canvas  •  S Pen active",
                    modifier = Modifier.padding(horizontal = 10.dp, vertical = 6.dp),
                    color = SecondaryText,
                    fontSize = 10.sp,
                )
            }
        }
    }
}

@Composable
private fun LayersPanel(modifier: Modifier = Modifier) {
    Column(
        modifier = modifier
            .background(Chrome)
            .padding(12.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(
                text = "LAYERS",
                color = PrimaryText,
                fontSize = 11.sp,
                fontWeight = FontWeight.Bold,
                letterSpacing = 0.8.sp,
            )
            Spacer(Modifier.weight(1f))
            Text("+", color = SecondaryText, fontSize = 18.sp)
        }
        Spacer(Modifier.height(10.dp))

        Surface(
            modifier = Modifier.fillMaxWidth(),
            color = Accent.copy(alpha = 0.12f),
            shape = RoundedCornerShape(10.dp),
            border = androidx.compose.foundation.BorderStroke(1.dp, Accent.copy(alpha = 0.45f)),
        ) {
            Column(Modifier.padding(10.dp)) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Box(
                        Modifier
                            .size(30.dp)
                            .background(Color(0xFF45464D), RoundedCornerShape(6.dp)),
                    )
                    Spacer(Modifier.width(9.dp))
                    Column(Modifier.weight(1f)) {
                        Text("Layer 1", color = PrimaryText, fontSize = 12.sp)
                        Text("100%  •  Normal", color = SecondaryText, fontSize = 9.sp)
                    }
                    Text("●", color = Accent, fontSize = 9.sp)
                }
            }
        }

        Spacer(Modifier.height(12.dp))
        Text(
            text = "Layer controls will activate as document commands are connected to the Rust engine.",
            color = SecondaryText,
            fontSize = 9.sp,
            lineHeight = 13.sp,
        )
    }
}

@Composable
private fun TimelinePanel(compact: Boolean) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .height(if (compact) 92.dp else 116.dp)
            .background(Chrome)
            .padding(horizontal = 12.dp, vertical = 9.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(
                text = "TIMELINE",
                color = PrimaryText,
                fontSize = 10.sp,
                fontWeight = FontWeight.Bold,
                letterSpacing = 0.8.sp,
            )
            Spacer(Modifier.width(10.dp))
            Text("12 fps", color = SecondaryText, fontSize = 9.sp)
            Spacer(Modifier.weight(1f))
            Text("Frame 1 / 8", color = SecondaryText, fontSize = 9.sp)
        }

        Spacer(Modifier.height(8.dp))
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .weight(1f),
            horizontalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            for (frame in 1..8) {
                FrameCell(
                    frame = frame,
                    selected = frame == 1,
                    modifier = Modifier
                        .weight(1f)
                        .widthIn(min = 34.dp),
                )
            }
        }
    }
}

@Composable
private fun FrameCell(frame: Int, selected: Boolean, modifier: Modifier = Modifier) {
    Box(
        modifier = modifier
            .fillMaxHeight()
            .background(
                color = if (selected) Accent.copy(alpha = 0.14f) else ChromeRaised,
                shape = RoundedCornerShape(7.dp),
            )
            .border(
                width = 1.dp,
                color = if (selected) Accent.copy(alpha = 0.65f) else ChromeBorder,
                shape = RoundedCornerShape(7.dp),
            ),
        contentAlignment = Alignment.Center,
    ) {
        Text(
            text = frame.toString(),
            color = if (selected) Accent else SecondaryText,
            fontSize = 10.sp,
            fontWeight = if (selected) FontWeight.Bold else FontWeight.Normal,
        )
    }
}
