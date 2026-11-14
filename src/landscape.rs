//! Static, world-anchored ground cover. Never decorates water, roads or crop soil.
use crate::map_data::WILLOW_VILLAGE;

pub fn ground_cover(region: &str, x: i32, y: i32) -> Option<([u8; 3], u8)> {
    let all = WILLOW_VILLAGE.ground_cover;
    let first = all.partition_point(|p| p.region < region);
    let mut result = None;
    let mut texture = None;
    for p in all[first..].iter().take_while(|p| p.region == region) {
        let dx = x - i32::from(p.center.x) * 16;
        let dy = y - i32::from(p.center.y) * 16;
        let rx = i32::from(p.radius.x) * 16;
        let ry = i32::from(p.radius.y) * 16;
        if dx.abs() > rx + 8 || dy.abs() > ry + 8 {
            continue;
        }
        let nx = dx * 64 / rx;
        let ny = dy * 64 / ry;
        // Irregular clusters anchored to the terrain, never a per-frame noise pattern.
        let (relief, grain) = *texture.get_or_insert_with(|| {
            (
                relief(x, y, 28, 0x7a16) + relief(x, y, 12, 0x2fd1) / 3,
                ((x.div_euclid(3) * 17) ^ (y.div_euclid(5) * 31)).rem_euclid(13) - 6,
            )
        });
        let weight = 4096 + relief * 43 + grain * 20 - nx * nx - ny * ny;
        if weight <= 0 {
            continue;
        }
        let alpha = (weight / 12).clamp(0, 218) as u8;
        if result.is_none_or(|(_, a)| alpha > a) {
            // A patch fades through a slightly brighter, drier shoulder.
            // Broad color carries the shape. Small grain only breaks its edge.
            let shoulder = ((1_900 - weight) / 130).clamp(0, 12);
            let shade = relief / 3 + grain / 2;
            let dry_scraps = if grain >= 5 && relief > 10 { 9 } else { 0 };
            let detail = [
                shoulder + shade + dry_scraps,
                shoulder + shade + dry_scraps / 2,
                shoulder / 2 + shade / 2,
            ];
            let color = [0, 1, 2].map(|i| (i32::from(p.color[i]) + detail[i]).clamp(0, 255) as u8);
            result = Some((color, alpha));
        }
    }
    result
}

#[derive(Clone, Copy)]
pub(crate) enum GroundMaterial {
    Grass,
    Dry,
    Stone,
}

/// Authored patch colors stay distinct from the flat lawn. Short dry ground,
/// wet moss and rock fragments pick up a different texture in the painter.
pub(crate) fn meadow_material(
    region: &str,
    x: i32,
    y: i32,
) -> Option<([u8; 3], u8, GroundMaterial)> {
    use GroundMaterial::{Dry, Grass, Stone};
    let (mut color, alpha) = ground_cover(region, x, y)?;
    let weight = i32::from(alpha);
    let (offset, texture) = match region {
        "future" => ([15, -9, -8], Dry),
        "farm" | "south_farm" => ([9, -8, -8], Dry),
        "quarry" => ([13, -8, 2], Stone),
        "wetland" | "creek" => ([-17, -8, 3], Grass),
        "orchard" | "forest" | "east_woods" => ([-15, -13, -1], Grass),
        "market" | "north_homes" | "east_homes" | "commons" | "craft" => ([5, -10, 1], Grass),
        _ => ([-7, -6, -2], Grass),
    };
    for (channel, shift) in color.iter_mut().zip(offset) {
        *channel = (i32::from(*channel) + shift * weight / 218).clamp(0, 255) as u8;
    }
    Some((color, alpha, texture))
}

fn relief(x: i32, y: i32, stride: i32, seed: u32) -> i32 {
    let (tx, ty) = (x.div_euclid(stride), y.div_euclid(stride));
    let (dx, dy) = (x.rem_euclid(stride), y.rem_euclid(stride));
    let sample = |x: i32, y: i32| {
        let seed =
            (x as u32).wrapping_mul(0x8da6_b343) ^ (y as u32).wrapping_mul(0xd816_3841) ^ seed;
        let bits = (seed ^ (seed >> 16)).wrapping_mul(0x85eb_ca6b);
        (bits >> 26) as i32 - 32
    };
    let top = sample(tx, ty) * (stride - dx) + sample(tx + 1, ty) * dx;
    let bottom = sample(tx, ty + 1) * (stride - dx) + sample(tx + 1, ty + 1) * dx;
    (top * (stride - dy) + bottom * dy) / (stride * stride)
}
