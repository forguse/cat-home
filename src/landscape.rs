//! Static, world-anchored ground cover. Never decorates water, roads or crop soil.
use crate::map_data::WILLOW_VILLAGE;

pub fn ground_cover(region: &str, x: i32, y: i32) -> Option<([u8; 3], u8)> {
    let all = WILLOW_VILLAGE.ground_cover;
    let first = all.partition_point(|p| p.region < region);
    let mut result = None;
    for p in all[first..].iter().take_while(|p| p.region == region) {
        let dx = x - i32::from(p.center.x) * 16;
        let dy = y - i32::from(p.center.y) * 16;
        let rx = i32::from(p.radius.x) * 16;
        let ry = i32::from(p.radius.y) * 16;
        if dx.abs() > rx + 4 || dy.abs() > ry + 4 { continue; }
        let nx = dx * 64 / rx;
        let ny = dy * 64 / ry;
        // Irregular clusters anchored to the terrain, never a per-frame noise pattern.
        let grain = ((x.div_euclid(3) * 17) ^ (y.div_euclid(5) * 31)).rem_euclid(13);
        let weight = 4096 + (grain - 6) * 54 - nx * nx - ny * ny;
        if weight <= 0 { continue; }
        let alpha = (weight / 20).clamp(0, 155) as u8;
        if result.is_none_or(|(_, a)| alpha > a) {
            let color = p.color.map(|v| (i32::from(v) + grain - 6).clamp(0, 255) as u8);
            result = Some((color, alpha));
        }
    }
    result
}
