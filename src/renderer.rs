// The layered DIB allocation, presentation, and cleanup are adapted from
// Nekodachi's renderer.rs, Copyright (c) 2026 Yannick Herrero, MIT.
use std::{ffi::c_void, io, mem, ptr};

use windows_sys::Win32::{
    Foundation::{HWND, POINT, RECT, SIZE},
    Graphics::Gdi::*,
    UI::WindowsAndMessaging::{ULW_ALPHA, UPDATELAYEREDWINDOWINFO, UpdateLayeredWindowIndirect},
};

pub const DEFAULT_WIDTH: i32 = 480;
pub const DEFAULT_HEIGHT: i32 = 300;
const PIXEL: i32 = 3;
pub const DETAIL_WIDTH: usize = 72;
pub const DETAIL_HEIGHT: usize = 48;
pub const DETAIL_BYTES: usize = DETAIL_WIDTH * DETAIL_HEIGHT * 4;
const DETAIL_SCALE: i32 = 2;

pub fn dimensions(width: i32, height: i32) -> (i32, i32) {
    (width.clamp(240, 3840), height.clamp(180, 2160))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Damage {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Damage {
    pub fn full((width, height): (i32, i32)) -> Self {
        Self {
            left: 0,
            top: 0,
            right: width,
            bottom: height,
        }
    }

    pub fn union(self, other: Self) -> Self {
        Self {
            left: self.left.min(other.left),
            top: self.top.min(other.top),
            right: self.right.max(other.right),
            bottom: self.bottom.max(other.bottom),
        }
    }
}

#[derive(Clone, Copy)]
pub struct DetailSprite {
    pub pixels: &'static [u8],
    left: i32,
    top: i32,
    flip: bool,
}

impl DetailSprite {
    pub fn new(pixels: &'static [u8], (x, y, flip): (i32, i32, bool)) -> Self {
        Self {
            pixels,
            left: x * PIXEL - 36 * DETAIL_SCALE,
            top: y * PIXEL - 44 * DETAIL_SCALE,
            flip,
        }
    }

    pub fn damage(self, (width, height): (i32, i32)) -> Option<Damage> {
        let right = self.left + DETAIL_WIDTH as i32 * DETAIL_SCALE;
        let bottom = self.top + DETAIL_HEIGHT as i32 * DETAIL_SCALE;
        let rect = Damage {
            left: self.left.div_euclid(PIXEL).clamp(0, width),
            top: self.top.div_euclid(PIXEL).clamp(0, height),
            right: (right + PIXEL - 1).div_euclid(PIXEL).clamp(0, width),
            bottom: (bottom + PIXEL - 1).div_euclid(PIXEL).clamp(0, height),
        };
        (rect.left < rect.right && rect.top < rect.bottom).then_some(rect)
    }
}

pub fn composite_detail(
    out: &mut [u8],
    width: i32,
    height: i32,
    area: Damage,
    detail: DetailSprite,
) {
    let start_x = detail.left.max(area.left).max(0);
    let end_x = (detail.left + DETAIL_WIDTH as i32 * DETAIL_SCALE)
        .min(area.right)
        .min(width);
    let start_y = detail.top.max(area.top).max(0);
    let end_y = (detail.top + DETAIL_HEIGHT as i32 * DETAIL_SCALE)
        .min(area.bottom)
        .min(height);
    for y in start_y..end_y {
        let row = ((y - detail.top) / DETAIL_SCALE) as usize * DETAIL_WIDTH;
        let dest_row = y as usize * width as usize * 4;
        for x in start_x..end_x {
            let column = ((x - detail.left) / DETAIL_SCALE) as usize;
            let column = if detail.flip {
                DETAIL_WIDTH - 1 - column
            } else {
                column
            };
            let source = (row + column) * 4;
            let pixel = &detail.pixels[source..source + 4];
            if pixel[3] == 0 {
                continue;
            }
            let offset = dest_row + x as usize * 4;
            let dest = &mut out[offset..offset + 4];
            if pixel[3] == 255 {
                dest.copy_from_slice(pixel);
            } else {
                let remain = 255 - u16::from(pixel[3]);
                for channel in 0..4 {
                    dest[channel] =
                        (u16::from(pixel[channel]) + u16::from(dest[channel]) * remain / 255) as u8;
                }
            }
        }
    }
}

pub struct Renderer {
    screen_dc: HDC,
    memory_dc: HDC,
    bitmap: HBITMAP,
    old_bitmap: HGDIOBJ,
    bits: *mut u8,
    width: i32,
    height: i32,
    presented_at: Option<(i32, i32)>,
}

fn new_bitmap(dc: HDC, width: i32, height: i32) -> (HBITMAP, *mut u8) {
    let mut info: BITMAPINFO = unsafe { mem::zeroed() };
    info.bmiHeader = BITMAPINFOHEADER {
        biSize: mem::size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: width,
        biHeight: -height,
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB,
        ..unsafe { mem::zeroed() }
    };
    let mut bits: *mut c_void = ptr::null_mut();
    let bitmap =
        unsafe { CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, ptr::null_mut(), 0) };
    (bitmap, bits.cast())
}

impl Renderer {
    pub fn new(width: i32, height: i32) -> io::Result<Self> {
        let (width, height) = dimensions(width, height);
        unsafe {
            let screen_dc = GetDC(ptr::null_mut());
            if screen_dc.is_null() {
                return Err(io::Error::last_os_error());
            }
            let memory_dc = CreateCompatibleDC(screen_dc);
            let (bitmap, bits) = new_bitmap(screen_dc, width, height);
            if memory_dc.is_null() || bitmap.is_null() || bits.is_null() {
                let error = io::Error::last_os_error();
                if !memory_dc.is_null() {
                    DeleteDC(memory_dc);
                }
                if !bitmap.is_null() {
                    DeleteObject(bitmap);
                }
                ReleaseDC(ptr::null_mut(), screen_dc);
                return Err(error);
            }
            let old_bitmap = SelectObject(memory_dc, bitmap);
            if old_bitmap.is_null() || old_bitmap == -1isize as HGDIOBJ {
                let error = io::Error::last_os_error();
                DeleteObject(bitmap);
                DeleteDC(memory_dc);
                ReleaseDC(ptr::null_mut(), screen_dc);
                return Err(error);
            }
            Ok(Self {
                screen_dc,
                memory_dc,
                bitmap,
                old_bitmap,
                bits,
                width,
                height,
                presented_at: None,
            })
        }
    }

    pub fn size(&self) -> (i32, i32) {
        (self.width, self.height)
    }

    pub fn logical_size(&self) -> (i32, i32) {
        (
            (self.width + PIXEL - 1) / PIXEL,
            (self.height + PIXEL - 1) / PIXEL,
        )
    }

    pub fn resize(&mut self, width: i32, height: i32) -> io::Result<()> {
        let (width, height) = dimensions(width, height);
        if (width, height) == self.size() {
            return Ok(());
        }
        unsafe {
            let (bitmap, bits) = new_bitmap(self.screen_dc, width, height);
            if bitmap.is_null() || bits.is_null() {
                let error = io::Error::last_os_error();
                if !bitmap.is_null() {
                    DeleteObject(bitmap);
                }
                return Err(error);
            }
            let previous = SelectObject(self.memory_dc, bitmap);
            if previous.is_null() || previous == -1isize as HGDIOBJ {
                let error = io::Error::last_os_error();
                DeleteObject(bitmap);
                return Err(error);
            }
            DeleteObject(self.bitmap);
            self.bitmap = bitmap;
            self.bits = bits;
        }
        self.width = width;
        self.height = height;
        self.presented_at = None;
        Ok(())
    }

    pub fn draw(
        &mut self,
        hwnd: HWND,
        position: POINT,
        pixels: &[u8],
        changed: Option<Damage>,
        details: &[Option<DetailSprite>],
    ) -> io::Result<()> {
        let logical = self.logical_size();
        if pixels.len() != (logical.0 * logical.1 * 4) as usize {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid world frame",
            ));
        }
        let new_surface = self.presented_at.is_none();
        let moved = self.presented_at != Some((position.x, position.y));
        let damage = if new_surface {
            Some(Damage::full(logical))
        } else {
            changed
        };
        if !moved && damage.is_none() {
            return Ok(());
        }
        let physical_damage = damage.map(|region| self.write_pixels(pixels, logical.0, region));
        if let Some(rect) = &physical_damage {
            let area = Damage {
                left: rect.left,
                top: rect.top,
                right: rect.right,
                bottom: rect.bottom,
            };
            let out = unsafe {
                std::slice::from_raw_parts_mut(
                    self.bits,
                    self.width as usize * self.height as usize * 4,
                )
            };
            for detail in details.iter().flatten() {
                composite_detail(out, self.width, self.height, area, *detail);
            }
        }

        let source = POINT { x: 0, y: 0 };
        let size = SIZE {
            cx: self.width,
            cy: self.height,
        };
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        let info = UPDATELAYEREDWINDOWINFO {
            cbSize: mem::size_of::<UPDATELAYEREDWINDOWINFO>() as u32,
            hdcDst: self.screen_dc,
            pptDst: &position,
            psize: &size,
            hdcSrc: self.memory_dc,
            pptSrc: &source,
            crKey: 0,
            pblend: &blend,
            dwFlags: ULW_ALPHA,
            prcDirty: if moved {
                ptr::null()
            } else {
                physical_damage
                    .as_ref()
                    .map_or(ptr::null(), std::ptr::from_ref)
            },
        };
        let result = unsafe { UpdateLayeredWindowIndirect(hwnd, &info) };
        if result == 0 {
            self.presented_at = None;
            Err(io::Error::last_os_error())
        } else {
            self.presented_at = Some((position.x, position.y));
            Ok(())
        }
    }

    fn write_pixels(&mut self, pixels: &[u8], logical_width: i32, region: Damage) -> RECT {
        let left = (region.left * PIXEL) as usize;
        let right = (region.right * PIXEL).min(self.width) as usize;
        let width = self.width as usize;
        let output = unsafe {
            std::slice::from_raw_parts_mut(self.bits.cast::<u32>(), width * self.height as usize)
        };
        for y in region.top..region.bottom {
            let first_row = (y * PIXEL) as usize;
            let end_row = ((y + 1) * PIXEL).min(self.height) as usize;
            let offset = ((y * logical_width + region.left) * 4) as usize;
            let length = ((region.right - region.left) * 4) as usize;
            let start = first_row * width + left;
            let end = first_row * width + right;
            for (copies, pixel) in output[start..end]
                .chunks_mut(PIXEL as usize)
                .zip(pixels[offset..offset + length].chunks_exact(4))
            {
                copies.fill(u32::from_ne_bytes(pixel.try_into().unwrap()));
            }
            for row in first_row + 1..end_row {
                output.copy_within(start..end, row * width + left);
            }
        }
        RECT {
            left: left as i32,
            right: right as i32,
            top: region.top * PIXEL,
            bottom: (region.bottom * PIXEL).min(self.height),
        }
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.memory_dc, self.old_bitmap);
            DeleteObject(self.bitmap);
            DeleteDC(self.memory_dc);
            ReleaseDC(ptr::null_mut(), self.screen_dc);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detailed_sprite_preserves_native_pixels_and_clips_at_surface_edge() {
        static SAMPLE: [u8; DETAIL_BYTES] = {
            let mut pixels = [0; DETAIL_BYTES];
            let index = (44 * DETAIL_WIDTH + 36) * 4;
            pixels[index] = 35;
            pixels[index + 1] = 80;
            pixels[index + 2] = 240;
            pixels[index + 3] = 255;
            pixels
        };
        let mut surface = [0; 4 * 4 * 4];
        let sprite = DetailSprite::new(&SAMPLE, (1, 1, false));
        assert_eq!(sprite.damage((2, 2)), Some(Damage::full((2, 2))));
        composite_detail(&mut surface, 4, 4, Damage::full((4, 4)), sprite);
        assert_eq!(&surface[60..64], &[35, 80, 240, 255]);
        assert_eq!(
            surface
                .iter()
                .skip(3)
                .step_by(4)
                .filter(|&&a| a > 0)
                .count(),
            1
        );
    }
}
