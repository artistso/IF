use inkframe_raster::{BYTES_PER_PIXEL, DirtyRect, TILE_BYTES, TILE_SIZE, TileCoord};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PixelOrder {
    Rgba,
    Bgra,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TransferEncoding {
    Linear,
    Srgb,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PixelEncoding {
    pub order: PixelOrder,
    pub transfer: TransferEncoding,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ClippedTileRegion {
    pub local_x: u16,
    pub local_y: u16,
    pub width: u16,
    pub height: u16,
    pub image_x: u32,
    pub image_y: u32,
}

pub(crate) fn full_tile_rect() -> DirtyRect {
    DirtyRect {
        min_x: 0,
        min_y: 0,
        max_x: TILE_SIZE as u16,
        max_y: TILE_SIZE as u16,
    }
}

/// Clips a tile-local dirty rectangle to the current screen-sized GPU cache.
/// Coordinates are evaluated in i64 so intentionally sparse document tiles at
/// very large positive/negative indices cannot overflow intermediate math.
pub(crate) fn clip_tile_region(
    coord: TileCoord,
    dirty: DirtyRect,
    viewport_width: u32,
    viewport_height: u32,
) -> Option<ClippedTileRegion> {
    if dirty.min_x >= dirty.max_x
        || dirty.min_y >= dirty.max_y
        || dirty.max_x > TILE_SIZE as u16
        || dirty.max_y > TILE_SIZE as u16
        || viewport_width == 0
        || viewport_height == 0
    {
        return None;
    }

    let tile_size = TILE_SIZE as i64;
    let origin_x = coord.x as i64 * tile_size;
    let origin_y = coord.y as i64 * tile_size;
    let doc_min_x = origin_x + dirty.min_x as i64;
    let doc_min_y = origin_y + dirty.min_y as i64;
    let doc_max_x = origin_x + dirty.max_x as i64;
    let doc_max_y = origin_y + dirty.max_y as i64;

    let clipped_min_x = doc_min_x.max(0);
    let clipped_min_y = doc_min_y.max(0);
    let clipped_max_x = doc_max_x.min(viewport_width as i64);
    let clipped_max_y = doc_max_y.min(viewport_height as i64);
    if clipped_min_x >= clipped_max_x || clipped_min_y >= clipped_max_y {
        return None;
    }

    Some(ClippedTileRegion {
        local_x: (clipped_min_x - origin_x) as u16,
        local_y: (clipped_min_y - origin_y) as u16,
        width: (clipped_max_x - clipped_min_x) as u16,
        height: (clipped_max_y - clipped_min_y) as u16,
        image_x: clipped_min_x as u32,
        image_y: clipped_min_y as u32,
    })
}

fn linear_to_srgb(value: f32) -> f32 {
    let linear = value.clamp(0.0, 1.0);
    if linear <= 0.003_130_8 {
        12.92 * linear
    } else {
        1.055 * linear.powf(1.0 / 2.4) - 0.055
    }
}

fn encode_linear_channel(value: f32, transfer: TransferEncoding) -> u8 {
    let encoded = match transfer {
        TransferEncoding::Linear => value.clamp(0.0, 1.0),
        TransferEncoding::Srgb => linear_to_srgb(value),
    };
    (encoded * 255.0).round() as u8
}

/// Converts one premultiplied linear RGBA8 document tile into an opaque display
/// tile. Composition happens in linear space; only the final RGB is converted
/// to the transfer function required by the Vulkan image format. This keeps a
/// stroke visually stable when it moves from the active render-pass overlay
/// into the persistent cache on UP.
pub(crate) fn encode_opaque_tile(
    source: &[u8],
    encoding: PixelEncoding,
    background_rgb: [u8; 3],
) -> Result<Vec<u8>, &'static str> {
    if source.len() != TILE_BYTES {
        return Err("raster tile byte length does not match TILE_BYTES");
    }

    let background = [
        background_rgb[0] as f32 / 255.0,
        background_rgb[1] as f32 / 255.0,
        background_rgb[2] as f32 / 255.0,
    ];
    let mut output = vec![0_u8; TILE_BYTES];
    for (src, dst) in source
        .chunks_exact(BYTES_PER_PIXEL)
        .zip(output.chunks_exact_mut(BYTES_PER_PIXEL))
    {
        let alpha = src[3] as f32 / 255.0;
        let inverse_alpha = 1.0 - alpha;
        // Source RGB is already premultiplied in linear space.
        let r = encode_linear_channel(
            src[0] as f32 / 255.0 + background[0] * inverse_alpha,
            encoding.transfer,
        );
        let g = encode_linear_channel(
            src[1] as f32 / 255.0 + background[1] * inverse_alpha,
            encoding.transfer,
        );
        let b = encode_linear_channel(
            src[2] as f32 / 255.0 + background[2] * inverse_alpha,
            encoding.transfer,
        );
        match encoding.order {
            PixelOrder::Rgba => dst.copy_from_slice(&[r, g, b, 255]),
            PixelOrder::Bgra => dst.copy_from_slice(&[b, g, r, 255]),
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    const RGBA_LINEAR: PixelEncoding = PixelEncoding {
        order: PixelOrder::Rgba,
        transfer: TransferEncoding::Linear,
    };
    const BGRA_LINEAR: PixelEncoding = PixelEncoding {
        order: PixelOrder::Bgra,
        transfer: TransferEncoding::Linear,
    };
    const RGBA_SRGB: PixelEncoding = PixelEncoding {
        order: PixelOrder::Rgba,
        transfer: TransferEncoding::Srgb,
    };

    #[test]
    fn negative_tiles_are_offscreen_without_overflow() {
        assert!(clip_tile_region(TileCoord { x: -1, y: 0 }, full_tile_rect(), 100, 100,).is_none());
        assert!(
            clip_tile_region(
                TileCoord {
                    x: i32::MIN,
                    y: i32::MIN,
                },
                full_tile_rect(),
                100,
                100,
            )
            .is_none()
        );
    }

    #[test]
    fn positive_edge_tile_clips_to_screen_bounds() {
        let region =
            clip_tile_region(TileCoord { x: 1, y: 0 }, full_tile_rect(), 300, 200).unwrap();
        assert_eq!(region.local_x, 0);
        assert_eq!(region.image_x, 256);
        assert_eq!(region.width, 44);
        assert_eq!(region.height, 200);
    }

    #[test]
    fn transparent_pixel_becomes_linear_workspace_background() {
        let tile = vec![0_u8; TILE_BYTES];
        let encoded = encode_opaque_tile(&tile, RGBA_LINEAR, [10, 20, 30]).unwrap();
        assert_eq!(&encoded[0..4], &[10, 20, 30, 255]);
    }

    #[test]
    fn premultiplied_pixel_composites_once_in_linear_space() {
        let mut tile = vec![0_u8; TILE_BYTES];
        // 50% opaque red in premultiplied linear representation.
        tile[0..4].copy_from_slice(&[128, 0, 0, 128]);
        let encoded = encode_opaque_tile(&tile, RGBA_LINEAR, [20, 40, 60]).unwrap();
        assert_eq!(&encoded[0..4], &[138, 20, 30, 255]);
    }

    #[test]
    fn srgb_target_encodes_linear_color_after_compositing() {
        let mut tile = vec![0_u8; TILE_BYTES];
        tile[0..4].copy_from_slice(&[128, 0, 0, 255]);
        let encoded = encode_opaque_tile(&tile, RGBA_SRGB, [0, 0, 0]).unwrap();
        // Linear 0.502 maps to approximately sRGB 0.737.
        assert_eq!(&encoded[0..4], &[188, 0, 0, 255]);
    }

    #[test]
    fn srgb_transparent_pixel_encodes_linear_background() {
        let tile = vec![0_u8; TILE_BYTES];
        let encoded = encode_opaque_tile(&tile, RGBA_SRGB, [14, 14, 17]).unwrap();
        assert_eq!(&encoded[0..4], &[66, 66, 73, 255]);
    }

    #[test]
    fn bgra_output_swizzles_only_color_channels() {
        let mut tile = vec![0_u8; TILE_BYTES];
        tile[0..4].copy_from_slice(&[10, 20, 30, 255]);
        let encoded = encode_opaque_tile(&tile, BGRA_LINEAR, [1, 2, 3]).unwrap();
        assert_eq!(&encoded[0..4], &[30, 20, 10, 255]);
    }

    #[test]
    fn rejects_malformed_tile_storage() {
        assert!(encode_opaque_tile(&[0; 4], RGBA_LINEAR, [0; 3]).is_err());
    }
}
