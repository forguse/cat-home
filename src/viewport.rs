//! One fixed magnification for native presentation, camera and pointer conversion.
//! Resizing reveals more terrain; it never rescales or destroys the world.
pub const WORLD_PIXEL: i32 = 6;
pub const ART_PIXEL: i32 = WORLD_PIXEL / 2;

pub fn logical_size((width, height): (i32, i32)) -> (i32, i32) {
    (
        (width.clamp(1, 3840) + WORLD_PIXEL - 1) / WORLD_PIXEL,
        (height.clamp(1, 2160) + WORLD_PIXEL - 1) / WORLD_PIXEL,
    )
}

pub fn source_size((width, height): (i32, i32)) -> (i32, i32) {
    (
        (width.clamp(1, 3840) + ART_PIXEL - 1) / ART_PIXEL,
        (height.clamp(1, 2160) + ART_PIXEL - 1) / ART_PIXEL,
    )
}

pub fn source_coordinate(physical: i32) -> i32 {
    physical.div_euclid(ART_PIXEL)
}

/// The native camera uses whole screen pixels while the painter uses whole
/// world pixels. Render the last strip needed for the presentation offset.
pub fn overscan_source_size(physical: (i32, i32)) -> (i32, i32) {
    let size = |pixels: i32| (pixels + WORLD_PIXEL - 1 + ART_PIXEL - 1) / ART_PIXEL;
    (
        size(physical.0.clamp(1, 3840)),
        size(physical.1.clamp(1, 2160)),
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PixelOrigin {
    pub world: (i32, i32),
    pub phase: (i32, i32),
}

impl PixelOrigin {
    pub fn new(origin: (f32, f32), physical: (i32, i32), map_world: (i32, i32)) -> Self {
        let axis = |world: f32, viewport: i32, map: i32| {
            let pixels = (world * WORLD_PIXEL as f32).round() as i32;
            let pixels = pixels.clamp(0, (map * WORLD_PIXEL - viewport).max(0));
            (pixels / WORLD_PIXEL, pixels % WORLD_PIXEL)
        };
        let x = axis(origin.0, physical.0.clamp(1, 3840), map_world.0);
        let y = axis(origin.1, physical.1.clamp(1, 2160), map_world.1);
        Self {
            world: (x.0, y.0),
            phase: (x.1, y.1),
        }
    }

    pub fn physical(self) -> (i32, i32) {
        (
            self.world.0 * WORLD_PIXEL + self.phase.0,
            self.world.1 * WORLD_PIXEL + self.phase.1,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn doubled_view_covers_half_as_many_tiles_without_fractional_art_pixels() {
        assert_eq!(logical_size((720, 480)), (120, 80));
        assert_eq!(source_size((720, 480)), (240, 160));
        for width in 1..=3840 {
            let (source, _) = source_size((width, 1));
            assert_eq!(source_coordinate(width - 1), source - 1);
        }
        for x in 0..100 {
            assert_eq!(
                (0..ART_PIXEL)
                    .map(|i| source_coordinate(x * ART_PIXEL + i))
                    .collect::<Vec<_>>(),
                vec![x; ART_PIXEL as usize]
            );
        }
    }

    #[test]
    fn pixel_phase_crosses_world_pixels_without_a_jump_and_clamps_map_edges() {
        for width in [1, 5, 6, 7, 251, 479, 3840] {
            let (source, _) = overscan_source_size((width, 1));
            for pixels in 0..=8320 - width {
                let origin = PixelOrigin::new((pixels as f32 / 6.0, 0.0), (width, 1), (1387, 1080));
                assert_eq!(origin.world.0 * WORLD_PIXEL + origin.phase.0, pixels);
                assert!(source_coordinate(width - 1 + origin.phase.0) < source);
            }
            let origin = PixelOrigin::new((2000.0, -3.0), (width, 1), (1387, 1080));
            assert_eq!(origin.world.0 * WORLD_PIXEL + origin.phase.0, 8322 - width);
            assert_eq!((origin.world.1, origin.phase.1), (0, 0));
        }
    }
}
