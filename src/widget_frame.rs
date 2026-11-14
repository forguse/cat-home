//! A narrow, wallpaper-independent rim. No bitmap, blur surface or animation.
//! Input pixels are fresh premultiplied BGRA; only the dirty perimeter is touched.

const BAND: i32 = 24;

fn distance(x: f32, y: f32, rect: [f32; 4], radius: f32) -> f32 {
    let [left, top, right, bottom] = rect;
    let dx = (x - (left + right) * 0.5).abs() - ((right - left) * 0.5 - radius);
    let dy = (y - (top + bottom) * 0.5).abs() - ((bottom - top) * 0.5 - radius);
    dx.max(0.0).hypot(dy.max(0.0)) + dx.max(dy).min(0.0) - radius
}
fn coverage(d: f32) -> u16 {
    ((0.5 - d).clamp(0.0, 1.0) * 255.0).round() as u16
}
fn solid(rgb: [u8; 3], alpha: u16) -> [u8; 4] {
    [
        (u16::from(rgb[2]) * alpha / 255) as u8,
        (u16::from(rgb[1]) * alpha / 255) as u8,
        (u16::from(rgb[0]) * alpha / 255) as u8,
        alpha as u8,
    ]
}
fn over(base: &mut [u8; 4], source: [u8; 4]) {
    let remain = 255 - u16::from(source[3]);
    for c in 0..4 {
        base[c] = (u16::from(source[c]) + u16::from(base[c]) * remain / 255).min(255) as u8;
    }
}

/// Apply after copying the source pixels for `dirty`, never on an old decorated strip.
pub fn decorate(pixels: &mut [u32], size: (i32, i32), dirty: [i32; 4]) {
    let (w, h) = size;
    if w < 48 || h < 48 || pixels.len() != (w as usize) * (h as usize) {
        return;
    }
    let [left, top, right, bottom] = dirty;
    let outer = [5.0, 3.0, w as f32 - 5.0, h as f32 - 7.0];
    let inner = [7.0, 5.0, w as f32 - 7.0, h as f32 - 9.0];
    let shadow = [5.0, 5.5, w as f32 - 5.0, h as f32 - 4.5];
    for y in top.max(0)..bottom.min(h) {
        let begin = left.max(0);
        let end = right.min(w);
        if y < BAND || y >= h - BAND {
            for x in begin..end {
                pixel(pixels, w, h, x, y, outer, inner, shadow);
            }
        } else {
            for x in begin..end.min(BAND) {
                pixel(pixels, w, h, x, y, outer, inner, shadow);
            }
            for x in begin.max(w - BAND)..end {
                pixel(pixels, w, h, x, y, outer, inner, shadow);
            }
        }
    }
}
#[allow(clippy::too_many_arguments)]
fn pixel(
    pixels: &mut [u32],
    w: i32,
    h: i32,
    x: i32,
    y: i32,
    outer: [f32; 4],
    inner: [f32; 4],
    shadow: [f32; 4],
) {
    let offset = (y * w + x) as usize;
    let original = pixels[offset].to_le_bytes();
    let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
    let outer_d = distance(px, py, outer, 12.0);
    let inner_d = distance(px, py, inner, 10.0);
    if inner_d < -1.5 {
        return;
    }
    let shadow_d = distance(px, py, shadow, 12.0).max(0.0);
    let shadow_alpha = ((1.0 - shadow_d / 6.0).clamp(0.0, 1.0).powi(2) * 38.0) as u16;
    let mut result = solid([24, 31, 27], shadow_alpha);
    let edge = coverage(outer_d);
    // Warm stone/wood tone stays quiet against either a dark or pale wallpaper.
    let rim = if py < 7.0 {
        [222, 218, 190]
    } else {
        [174, 174, 148]
    };
    over(&mut result, solid(rim, edge * 238 / 255));
    let mask = coverage(inner_d);
    let mut content = original;
    for c in &mut content {
        *c = (u16::from(*c) * mask / 255) as u8;
    }
    over(&mut result, content);
    if inner_d >= -1.0 && inner_d < 0.0 {
        over(
            &mut result,
            solid([42, 53, 40], ((inner_d + 1.0) * 46.0) as u16),
        );
    }
    // A tiny grip in the rim communicates resizing without covering the map.
    if x >= w - 19 && y >= h - 21 && x + y == w + h - 29 && (x % 3 == 0) {
        over(&mut result, solid([244, 235, 204], 100));
    }
    pixels[offset] = u32::from_le_bytes(result);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn interior_pixels_are_unchanged_and_corners_remain_transparent() {
        let (w, h) = (481, 301);
        let original = u32::from_le_bytes([85, 125, 95, 255]);
        let mut pixels = vec![original; (w * h) as usize];
        decorate(&mut pixels, (w, h), [0, 0, w, h]);
        assert_eq!(pixels[(h / 2 * w + w / 2) as usize], original);
        assert!(pixels[0].to_le_bytes()[3] < 32);
        for p in pixels {
            let b = p.to_le_bytes();
            assert!(b[..3].iter().all(|c| *c <= b[3]));
        }
    }
    #[test]
    fn dirty_rectangles_match_a_single_full_composite_without_darkening() {
        let (w, h) = (241, 181);
        let source = vec![u32::from_le_bytes([110, 145, 90, 255]); (w * h) as usize];
        let mut full = source.clone();
        decorate(&mut full, (w, h), [0, 0, w, h]);
        let mut split = vec![0; source.len()];
        for top in (0..h).step_by(17) {
            for left in (0..w).step_by(19) {
                let bottom = (top + 17).min(h);
                let right = (left + 19).min(w);
                for y in top..bottom {
                    for x in left..right {
                        split[(y * w + x) as usize] = source[(y * w + x) as usize];
                    }
                }
                decorate(&mut split, (w, h), [left, top, right, bottom]);
            }
        }
        assert_eq!(split, full);
        for _ in 0..3 {
            split.clone_from(&source);
            decorate(&mut split, (w, h), [0, 0, w, h]);
            assert_eq!(split, full);
        }
    }
}
