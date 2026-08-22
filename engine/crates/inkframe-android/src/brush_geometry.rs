#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct BrushInstance {
    /// Vulkan clip-space center. With a positive viewport height, -1 is the top
    /// edge and +1 is the bottom edge, matching Android pixel coordinates.
    pub center: [f32; 2],
    /// Half-width/height in clip-space units.
    pub half_size: [f32; 2],
    /// Linear, unpremultiplied color. Alpha is blended by the Vulkan pipeline.
    pub color: [f32; 4],
    /// Normalized radial antialias width used by the fragment shader.
    pub edge: f32,
}

impl BrushInstance {
    pub(crate) fn from_pixels(
        x: f32,
        y: f32,
        diameter: f32,
        mut color: [f32; 4],
        viewport_width: u32,
        viewport_height: u32,
        alpha_scale: f32,
    ) -> Option<Self> {
        if viewport_width == 0 || viewport_height == 0 {
            return None;
        }
        let diameter = diameter.max(1.0);
        let width = viewport_width as f32;
        let height = viewport_height as f32;
        color[3] = (color[3] * alpha_scale).clamp(0.0, 1.0);
        Some(Self {
            center: [2.0 * x / width - 1.0, 2.0 * y / height - 1.0],
            half_size: [diameter / width, diameter / height],
            color,
            // Approximately a one-pixel radial transition for normal brush sizes,
            // with a wider normalized edge for tiny dabs where subpixel coverage
            // would otherwise shimmer.
            edge: (2.0 / diameter).clamp(0.02, 0.75),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_android_pixels_to_vulkan_clip_space() {
        let top_left = BrushInstance::from_pixels(
            0.0,
            0.0,
            20.0,
            [1.0, 0.0, 0.0, 1.0],
            100,
            200,
            1.0,
        )
        .unwrap();
        assert_eq!(top_left.center, [-1.0, -1.0]);
        assert_eq!(top_left.half_size, [0.2, 0.1]);

        let center = BrushInstance::from_pixels(
            50.0,
            100.0,
            20.0,
            [1.0, 0.0, 0.0, 1.0],
            100,
            200,
            1.0,
        )
        .unwrap();
        assert!(center.center[0].abs() < f32::EPSILON);
        assert!(center.center[1].abs() < f32::EPSILON);
    }

    #[test]
    fn scales_prediction_alpha_without_changing_rgb() {
        let instance = BrushInstance::from_pixels(
            10.0,
            10.0,
            8.0,
            [0.2, 0.4, 0.8, 0.75],
            100,
            100,
            0.5,
        )
        .unwrap();
        assert_eq!(&instance.color[..3], &[0.2, 0.4, 0.8]);
        assert!((instance.color[3] - 0.375).abs() < f32::EPSILON);
    }

    #[test]
    fn rejects_zero_sized_viewports() {
        assert!(
            BrushInstance::from_pixels(
                1.0,
                1.0,
                4.0,
                [1.0, 1.0, 1.0, 1.0],
                0,
                100,
                1.0,
            )
            .is_none()
        );
    }
}
