// The layered DIB allocation, presentation, and cleanup are adapted from
// Nekodachi's renderer.rs, Copyright (c) 2026 Yannick Herrero, MIT.
use std::{ffi::c_void, io, mem, ptr};

use windows_sys::Win32::{
    Foundation::{HWND, POINT, SIZE},
    Graphics::Gdi::*,
    UI::WindowsAndMessaging::{ULW_ALPHA, UpdateLayeredWindow},
};

pub const ART_WIDTH: usize = 160;
pub const ART_HEIGHT: usize = 100;
pub const DEFAULT_WIDTH: i32 = 480;
pub const FRAME_BYTES: usize = ART_WIDTH * ART_HEIGHT * 4;

pub fn dimensions(requested_width: i32) -> (i32, i32) {
    let width = requested_width.clamp(240, 960) / 8 * 8;
    (width, width * 5 / 8)
}

pub struct Renderer {
    screen_dc: HDC,
    memory_dc: HDC,
    bitmap: HBITMAP,
    old_bitmap: HGDIOBJ,
    bits: *mut u8,
    width: i32,
    height: i32,
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
    pub fn new(width: i32) -> io::Result<Self> {
        let (width, height) = dimensions(width);
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
            })
        }
    }

    pub fn size(&self) -> (i32, i32) {
        (self.width, self.height)
    }

    pub fn resize(&mut self, width: i32) -> io::Result<()> {
        let (width, height) = dimensions(width);
        if width == self.width {
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
        Ok(())
    }

    pub fn draw(&mut self, hwnd: HWND, position: POINT, pixels: &[u8]) -> io::Result<()> {
        if pixels.len() != FRAME_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid world frame",
            ));
        }
        let width = self.width as usize;
        let height = self.height as usize;
        let stride = width * 4;
        let output = unsafe { std::slice::from_raw_parts_mut(self.bits, stride * height) };

        // Write the first scaled row, then duplicate it for nearest-neighbor scaling.
        for (y, source) in pixels.chunks_exact(ART_WIDTH * 4).enumerate() {
            let first_row = y * height / ART_HEIGHT;
            let end_row = (y + 1) * height / ART_HEIGHT;
            let row = first_row * stride;
            for (x, pixel) in source.chunks_exact(4).enumerate() {
                let start = row + x * width / ART_WIDTH * 4;
                let end = row + (x + 1) * width / ART_WIDTH * 4;
                for copy in output[start..end].chunks_exact_mut(4) {
                    copy.copy_from_slice(pixel);
                }
            }
            for next_row in first_row + 1..end_row {
                output.copy_within(row..row + stride, next_row * stride);
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
        let result = unsafe {
            UpdateLayeredWindow(
                hwnd,
                self.screen_dc,
                &position,
                &size,
                self.memory_dc,
                &source,
                0,
                &blend,
                ULW_ALPHA,
            )
        };
        if result == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
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
