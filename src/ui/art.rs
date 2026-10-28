//! Game artwork for the GAMES tab: launcher cover images decoded with GDI+
//! (Steam's library cache, Xbox logos) and program icons from the shell.
//! Everything is loaded on the library scan thread; GDI bitmaps and icons are
//! process-wide, so the window thread can draw them.

#![allow(non_snake_case, unsafe_op_in_unsafe_fn)]

use std::ptr;
use std::sync::OnceLock;

use windows_sys::Win32::Graphics::Gdi::{DeleteObject, HBITMAP};
use windows_sys::Win32::Graphics::GdiPlus::{self as gp, GdipCreateBitmapFromFile, GdipCreateBitmapFromScan0, GdipCreateHBITMAPFromBitmap, GdipDeleteGraphics, GdipDisposeImage, GdipDrawImageRectI, GdipGetImageGraphicsContext, GdipGetImageHeight, GdipGetImageWidth, GdipSetInterpolationMode, GdiplusStartup, GdiplusStartupInput, GpBitmap, GpGraphics, GpImage, InterpolationModeHighQualityBicubic};
use windows_sys::Win32::UI::Shell::SHDefExtractIconW;
use windows_sys::Win32::UI::WindowsAndMessaging::{DestroyIcon, HICON};

use super::draw::wide;

/// A decoded cover image, scaled down to the tile size.
pub struct Cover {
    pub hbm: HBITMAP,
    pub w: i32,
    pub h: i32,
}

unsafe impl Send for Cover {}

impl Drop for Cover {
    fn drop(&mut self) {
        unsafe { DeleteObject(self.hbm) };
    }
}

/// A program's icon from the shell.
pub struct ExeIcon {
    pub hicon: HICON,
}

unsafe impl Send for ExeIcon {}

impl Drop for ExeIcon {
    fn drop(&mut self) {
        unsafe { DestroyIcon(self.hicon) };
    }
}

/// GDI+ `PixelFormat32bppARGB` (not in windows-sys).
const PIXEL_FORMAT_32BPP_ARGB: i32 = 0x0026200A;

fn startup() {
    static ONCE: OnceLock<()> = OnceLock::new();
    ONCE.get_or_init(|| unsafe {
        let input = GdiplusStartupInput { GdiplusVersion: 1, DebugEventCallback: 0, SuppressBackgroundThread: 0, SuppressExternalCodecs: 0 };
        let mut token = 0usize;
        GdiplusStartup(&mut token, &input, ptr::null_mut());
    });
}

/// Decode an image file (jpg, png, ...) and scale it to fit `max_w` x `max_h`.
/// Transparent areas are flattened onto `background` (a COLORREF).
pub fn load_cover(path: &str, max_w: i32, max_h: i32, background: u32) -> Option<Cover> {
    startup();
    unsafe {
        let mut img: *mut GpBitmap = ptr::null_mut();
        if GdipCreateBitmapFromFile(wide(path).as_ptr(), &mut img) != gp::Ok || img.is_null() {
            return None;
        }
        let (mut w, mut h) = (0u32, 0u32);
        GdipGetImageWidth(img as *mut GpImage, &mut w);
        GdipGetImageHeight(img as *mut GpImage, &mut h);
        if w == 0 || h == 0 {
            GdipDisposeImage(img as *mut GpImage);
            return None;
        }
        let k = f32::min(max_w as f32 / w as f32, max_h as f32 / h as f32).min(1.0);
        let tw = ((w as f32 * k).round() as i32).max(1);
        let th = ((h as f32 * k).round() as i32).max(1);
        let mut dst: *mut GpBitmap = ptr::null_mut();
        GdipCreateBitmapFromScan0(tw, th, 0, PIXEL_FORMAT_32BPP_ARGB, ptr::null(), &mut dst);
        let mut out = None;
        if !dst.is_null() {
            let mut g: *mut GpGraphics = ptr::null_mut();
            if GdipGetImageGraphicsContext(dst as *mut GpImage, &mut g) == gp::Ok && !g.is_null() {
                GdipSetInterpolationMode(g, InterpolationModeHighQualityBicubic);
                GdipDrawImageRectI(g, img as *mut GpImage, 0, 0, tw, th);
                GdipDeleteGraphics(g);
            }
            // The background is ARGB here, unlike GDI's COLORREF.
            let bg = 0xff00_0000 | ((background & 0xff) << 16) | (background & 0xff00) | ((background >> 16) & 0xff);
            let mut hbm: HBITMAP = 0;
            if GdipCreateHBITMAPFromBitmap(dst, &mut hbm, bg) == gp::Ok && hbm != 0 {
                out = Some(Cover { hbm, w: tw, h: th });
            }
            GdipDisposeImage(dst as *mut GpImage);
        }
        GdipDisposeImage(img as *mut GpImage);
        out
    }
}

/// The program's large icon at `size` pixels.
pub fn exe_icon(path: &str, size: i32) -> Option<ExeIcon> {
    unsafe {
        let mut h: HICON = 0;
        let hr = SHDefExtractIconW(wide(path).as_ptr(), 0, 0, &mut h, ptr::null_mut(), size as u32);
        if hr == 0 && h != 0 { Some(ExeIcon { hicon: h }) } else { None }
    }
}

/// The game's own program in an install folder: the largest .exe in the
/// folder or one level below it, skipping installers, crash handlers and redistributables.
pub fn guess_exe(dir: &str) -> Option<String> {
    const SKIP: [&str; 12] = ["unins", "crash", "redist", "setup", "vcredist", "vc_redist", "dxsetup", "dotnet", "report", "installer", "easyanticheat", "cleanup"];
    let mut best: Option<(bool, u64, String)> = None;
    let mut consider = |p: std::path::PathBuf| {
        let name = p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
        if !name.ends_with(".exe") || SKIP.iter().any(|s| name.contains(s)) {
            return;
        }
        let size = p.metadata().map(|m| m.len()).unwrap_or(0);
        // A launcher counts, but anything else wins over it.
        let launcher = name.contains("launcher");
        let key = (!launcher, size);
        if best.as_ref().is_none_or(|(l, s, _)| key > (*l, *s)) {
            best = Some((!launcher, size, p.to_string_lossy().to_string()));
        }
    };
    let Ok(rd) = std::fs::read_dir(dir) else { return None };
    let mut seen = 0;
    for e in rd.flatten() {
        seen += 1;
        if seen > 300 {
            break;
        }
        let p = e.path();
        if p.is_dir() {
            if let Ok(sub) = std::fs::read_dir(&p) {
                for f in sub.flatten().take(80) {
                    consider(f.path());
                }
            }
        } else {
            consider(p);
        }
    }
    best.map(|(_, _, p)| p)
}
