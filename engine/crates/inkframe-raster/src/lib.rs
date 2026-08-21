use std::collections::HashMap;

pub const TILE_SIZE: u32 = 256;
pub const BYTES_PER_PIXEL: usize = 4;
pub const TILE_BYTES: usize = TILE_SIZE as usize * TILE_SIZE as usize * BYTES_PER_PIXEL;
const MIN_DAB_DIAMETER_PX: f32 = 0.5;
const MAX_DAB_DIAMETER_PX: f32 = 4096.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TileCoord {
    pub x: i32,
    pub y: i32,
}

impl TileCoord {
    pub fn from_pixel(x: i32, y: i32) -> Self {
        Self {
            x: x.div_euclid(TILE_SIZE as i32),
            y: y.div_euclid(TILE_SIZE as i32),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirtyRect {
    pub min_x: u16,
    pub min_y: u16,
    pub max_x: u16,
    pub max_y: u16,
}

impl DirtyRect {
    fn from_pixel(x: u16, y: u16) -> Self {
        Self {
            min_x: x,
            min_y: y,
            max_x: x + 1,
            max_y: y + 1,
        }
    }

    fn include(&mut self, x: u16, y: u16) {
        self.min_x = self.min_x.min(x);
        self.min_y = self.min_y.min(y);
        self.max_x = self.max_x.max(x + 1);
        self.max_y = self.max_y.max(y + 1);
    }

    pub fn width(self) -> u16 {
        self.max_x - self.min_x
    }

    pub fn height(self) -> u16 {
        self.max_y - self.min_y
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaintMode {
    Paint,
    Erase,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RasterDab {
    pub x: f32,
    pub y: f32,
    pub diameter: f32,
    /// Straight-alpha input color. The tile store converts it to premultiplied RGBA8.
    pub rgba: [u8; 4],
    pub mode: PaintMode,
}

impl RasterDab {
    pub fn ink(x: f32, y: f32, diameter: f32, rgba: [u8; 4]) -> Self {
        Self {
            x,
            y,
            diameter,
            rgba,
            mode: PaintMode::Paint,
        }
    }

    pub fn erase(x: f32, y: f32, diameter: f32) -> Self {
        Self {
            x,
            y,
            diameter,
            rgba: [0, 0, 0, 0],
            mode: PaintMode::Erase,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RasterTile {
    pixels: Vec<u8>,
    dirty: Option<DirtyRect>,
}

impl Default for RasterTile {
    fn default() -> Self {
        Self {
            pixels: vec![0; TILE_BYTES],
            dirty: None,
        }
    }
}

impl RasterTile {
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    pub fn dirty_rect(&self) -> Option<DirtyRect> {
        self.dirty
    }

    fn pixel_index(local_x: u16, local_y: u16) -> usize {
        (local_y as usize * TILE_SIZE as usize + local_x as usize) * BYTES_PER_PIXEL
    }

    fn mark_dirty(&mut self, local_x: u16, local_y: u16) {
        match &mut self.dirty {
            Some(rect) => rect.include(local_x, local_y),
            None => self.dirty = Some(DirtyRect::from_pixel(local_x, local_y)),
        }
    }

    fn clear_dirty(&mut self) {
        self.dirty = None;
    }
}

#[derive(Debug, Default)]
pub struct SparseRaster {
    tiles: HashMap<TileCoord, RasterTile>,
}

impl SparseRaster {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn tile_count(&self) -> usize {
        self.tiles.len()
    }

    pub fn memory_bytes(&self) -> usize {
        self.tiles.len() * TILE_BYTES
    }

    pub fn tile(&self, coord: TileCoord) -> Option<&RasterTile> {
        self.tiles.get(&coord)
    }

    pub fn pixel_rgba(&self, x: i32, y: i32) -> [u8; 4] {
        let coord = TileCoord::from_pixel(x, y);
        let Some(tile) = self.tiles.get(&coord) else {
            return [0; 4];
        };
        let local_x = x.rem_euclid(TILE_SIZE as i32) as u16;
        let local_y = y.rem_euclid(TILE_SIZE as i32) as u16;
        let index = RasterTile::pixel_index(local_x, local_y);
        tile.pixels[index..index + 4].try_into().unwrap()
    }

    pub fn dirty_tiles(&self) -> impl Iterator<Item = (TileCoord, DirtyRect, &[u8])> + '_ {
        self.tiles.iter().filter_map(|(coord, tile)| {
            tile.dirty_rect()
                .map(|dirty| (*coord, dirty, tile.pixels()))
        })
    }

    pub fn clear_dirty(&mut self) {
        for tile in self.tiles.values_mut() {
            tile.clear_dirty();
        }
    }

    pub fn clear(&mut self) {
        self.tiles.clear();
    }

    pub fn apply_dabs<I>(&mut self, dabs: I)
    where
        I: IntoIterator<Item = RasterDab>,
    {
        for dab in dabs {
            self.apply_dab(dab);
        }
    }

    pub fn apply_dab(&mut self, dab: RasterDab) {
        if !dab.x.is_finite() || !dab.y.is_finite() || !dab.diameter.is_finite() {
            return;
        }

        let diameter = dab.diameter.clamp(MIN_DAB_DIAMETER_PX, MAX_DAB_DIAMETER_PX);
        let radius = diameter * 0.5;
        // One pixel of analytical feathering keeps the CPU persistence layer from
        // producing visibly jagged tile-edge circles before the final GPU brush path.
        let outer_radius = radius + 0.5;
        let min_x = (dab.x - outer_radius).floor() as i32;
        let min_y = (dab.y - outer_radius).floor() as i32;
        let max_x = (dab.x + outer_radius).ceil() as i32;
        let max_y = (dab.y + outer_radius).ceil() as i32;

        for pixel_y in min_y..max_y {
            let center_y = pixel_y as f32 + 0.5;
            for pixel_x in min_x..max_x {
                let center_x = pixel_x as f32 + 0.5;
                let distance = (center_x - dab.x).hypot(center_y - dab.y);
                let coverage = (outer_radius - distance).clamp(0.0, 1.0);
                if coverage <= 0.0 {
                    continue;
                }
                self.apply_pixel(pixel_x, pixel_y, dab, coverage);
            }
        }
    }

    fn apply_pixel(&mut self, x: i32, y: i32, dab: RasterDab, coverage: f32) {
        let coord = TileCoord::from_pixel(x, y);
        if matches!(dab.mode, PaintMode::Erase) && !self.tiles.contains_key(&coord) {
            // Erasing untouched space must remain free: no sparse tile allocation.
            return;
        }

        let local_x = x.rem_euclid(TILE_SIZE as i32) as u16;
        let local_y = y.rem_euclid(TILE_SIZE as i32) as u16;
        let tile = self.tiles.entry(coord).or_default();
        let index = RasterTile::pixel_index(local_x, local_y);
        let before: [u8; 4] = tile.pixels[index..index + 4].try_into().unwrap();
        let after = match dab.mode {
            PaintMode::Paint => blend_premultiplied(before, dab.rgba, coverage),
            PaintMode::Erase => erase_premultiplied(before, coverage),
        };
        if before == after {
            return;
        }
        tile.pixels[index..index + 4].copy_from_slice(&after);
        tile.mark_dirty(local_x, local_y);
    }
}

fn byte(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn blend_premultiplied(dst: [u8; 4], src_straight: [u8; 4], coverage: f32) -> [u8; 4] {
    let src_alpha = src_straight[3] as f32 / 255.0 * coverage;
    if src_alpha <= 0.0 {
        return dst;
    }
    let inv_src = 1.0 - src_alpha;
    let dst_alpha = dst[3] as f32 / 255.0;
    let src_r = src_straight[0] as f32 / 255.0 * src_alpha;
    let src_g = src_straight[1] as f32 / 255.0 * src_alpha;
    let src_b = src_straight[2] as f32 / 255.0 * src_alpha;
    let dst_r = dst[0] as f32 / 255.0;
    let dst_g = dst[1] as f32 / 255.0;
    let dst_b = dst[2] as f32 / 255.0;

    [
        byte(src_r + dst_r * inv_src),
        byte(src_g + dst_g * inv_src),
        byte(src_b + dst_b * inv_src),
        byte(src_alpha + dst_alpha * inv_src),
    ]
}

fn erase_premultiplied(dst: [u8; 4], coverage: f32) -> [u8; 4] {
    let keep = 1.0 - coverage.clamp(0.0, 1.0);
    [
        byte(dst[0] as f32 / 255.0 * keep),
        byte(dst[1] as f32 / 255.0 * keep),
        byte(dst[2] as f32 / 255.0 * keep),
        byte(dst[3] as f32 / 255.0 * keep),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    const WHITE: [u8; 4] = [255, 255, 255, 255];

    #[test]
    fn untouched_raster_allocates_nothing() {
        let raster = SparseRaster::new();
        assert_eq!(raster.tile_count(), 0);
        assert_eq!(raster.memory_bytes(), 0);
    }

    #[test]
    fn dab_allocates_only_the_touched_tile() {
        let mut raster = SparseRaster::new();
        raster.apply_dab(RasterDab::ink(128.0, 128.0, 6.0, WHITE));

        assert_eq!(raster.tile_count(), 1);
        assert_eq!(raster.memory_bytes(), TILE_BYTES);
        assert!(raster.pixel_rgba(128, 128)[3] > 0);
    }

    #[test]
    fn dab_crossing_a_tile_corner_allocates_four_tiles() {
        let mut raster = SparseRaster::new();
        raster.apply_dab(RasterDab::ink(256.0, 256.0, 8.0, WHITE));

        assert_eq!(raster.tile_count(), 4);
        for coord in [
            TileCoord { x: 0, y: 0 },
            TileCoord { x: 1, y: 0 },
            TileCoord { x: 0, y: 1 },
            TileCoord { x: 1, y: 1 },
        ] {
            assert!(raster.tile(coord).is_some());
        }
    }

    #[test]
    fn erasing_untouched_space_does_not_allocate_tiles() {
        let mut raster = SparseRaster::new();
        raster.apply_dab(RasterDab::erase(1000.0, 1000.0, 32.0));
        assert_eq!(raster.tile_count(), 0);
    }

    #[test]
    fn eraser_removes_premultiplied_color_and_alpha() {
        let mut raster = SparseRaster::new();
        raster.apply_dab(RasterDab::ink(32.5, 32.5, 8.0, WHITE));
        assert_eq!(raster.pixel_rgba(32, 32), WHITE);

        raster.apply_dab(RasterDab::erase(32.5, 32.5, 8.0));
        assert_eq!(raster.pixel_rgba(32, 32), [0; 4]);
    }

    #[test]
    fn negative_document_coordinates_use_euclidean_tile_addressing() {
        let mut raster = SparseRaster::new();
        raster.apply_dab(RasterDab::ink(-0.5, -0.5, 1.0, WHITE));

        assert!(raster.tile(TileCoord { x: -1, y: -1 }).is_some());
        assert!(raster.pixel_rgba(-1, -1)[3] > 0);
    }

    #[test]
    fn dirty_tiles_can_be_acknowledged_without_dropping_pixels() {
        let mut raster = SparseRaster::new();
        raster.apply_dab(RasterDab::ink(12.5, 20.5, 2.0, WHITE));

        let dirty: Vec<_> = raster.dirty_tiles().collect();
        assert_eq!(dirty.len(), 1);
        assert!(dirty[0].1.width() > 0);
        assert!(dirty[0].1.height() > 0);

        raster.clear_dirty();
        assert_eq!(raster.dirty_tiles().count(), 0);
        assert!(raster.pixel_rgba(12, 20)[3] > 0);
    }

    #[test]
    fn distant_marks_remain_sparse_instead_of_allocating_a_monolithic_canvas() {
        let mut raster = SparseRaster::new();
        raster.apply_dab(RasterDab::ink(10.0, 10.0, 4.0, WHITE));
        raster.apply_dab(RasterDab::ink(20_000.0, 20_000.0, 4.0, WHITE));

        assert_eq!(raster.tile_count(), 2);
        assert_eq!(raster.memory_bytes(), TILE_BYTES * 2);
    }
}
