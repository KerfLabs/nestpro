// A replaceable window/taskbar icon, as in the original: Expert loads its
// window icons from a loose icon file by name ("iExpert.ico", read from the
// file at 16 × 16 and again at 32 × 32, IC1), sets them small first, then big
// (IC2), and keeps the default when the file can't be loaded (IC3).
// Here the file is nestpro.ico (or nestpro.png), looked for next to the
// program first and then in the working folder; without it the built-in black icon (assets/icon.png) is used.

use std::path::{Path, PathBuf};

/// File names looked for, in order.
pub const ICON_FILES: [&str; 2] = ["nestpro.ico", "nestpro.png"];
/// Size of an ICO directory entry and of the ICO header (bytes).
const ICO_ENTRY: usize = 16;
const ICO_HEADER: usize = 6;
/// A width/height byte of 0 in an ICO entry means 256.
const ICO_ZERO_MEANS: u32 = 256;

/// RGBA pixels with their size.
pub struct Rgba {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// Where a replacement icon would be found, if one is there.
pub fn find_icon_file() -> Option<PathBuf> {
    // The program's folder first: the original changes into it before
    // loading anything (SU1),
    // so its bare "iExpert.ico" is the one next to the program.
    let mut dirs = Vec::new();
    if let Some(d) = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf)) {
        dirs.push(d);
    }
    dirs.push(PathBuf::from("."));
    dirs.iter().flat_map(|d| ICON_FILES.iter().map(move |f| d.join(f))).find(|p| p.is_file())
}

/// Load an .ico or .png file into RGBA; None when it isn't a usable icon.
pub fn load(path: &Path) -> Option<Rgba> {
    let bytes = std::fs::read(path).ok()?;
    let is_ico = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("ico"));
    if is_ico { parse_ico(&bytes) } else { decode_png(&bytes) }
}

fn decode_png(bytes: &[u8]) -> Option<Rgba> {
    let im = image::load_from_memory(bytes).ok()?.to_rgba8();
    let (width, height) = im.dimensions();
    Some(Rgba { rgba: im.into_raw(), width, height })
}

fn u16_at(b: &[u8], i: usize) -> Option<u32> {
    Some(u16::from_le_bytes(b.get(i..i + 2)?.try_into().ok()?) as u32)
}
fn u32_at(b: &[u8], i: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(i..i + 4)?.try_into().ok()?))
}

/// The biggest image in an .ico that can be read (PNG, or a 32- or 24-bit
/// bitmap). Windows scales it for the small and big icons.
pub fn parse_ico(b: &[u8]) -> Option<Rgba> {
    // Header: reserved 0, type 1 (icon), count.
    if u16_at(b, 0)? != 0 || u16_at(b, 2)? != 1 {
        return None;
    }
    let count = u16_at(b, 4)? as usize;
    let mut entries: Vec<(u32, usize, usize)> = (0..count)
        .filter_map(|k| {
            let e = ICO_HEADER + k * ICO_ENTRY;
            let w = *b.get(e)? as u32;
            let w = if w == 0 { ICO_ZERO_MEANS } else { w };
            let size = u32_at(b, e + 8)? as usize;
            let off = u32_at(b, e + 12)? as usize;
            (off + size <= b.len()).then_some((w, off, size))
        })
        .collect();
    entries.sort_by(|a, b| b.0.cmp(&a.0));
    entries.into_iter().find_map(|(_, off, size)| {
        let d = &b[off..off + size];
        if d.starts_with(b"\x89PNG") { decode_png(d) } else { decode_dib(d) }
    })
}

/// A bitmap icon image: BITMAPINFOHEADER, colour rows bottom-up, then the
/// 1-bit AND mask (used for transparency when there's no alpha).
fn decode_dib(d: &[u8]) -> Option<Rgba> {
    let hdr = u32_at(d, 0)? as usize;
    let w = u32_at(d, 4)?;
    let h = u32_at(d, 8)? / 2; // the height counts colour and mask rows
    let bpp = u16_at(d, 14)?;
    if w == 0 || h == 0 || !(bpp == 32 || bpp == 24) {
        return None;
    }
    let bytes_pp = (bpp / 8) as usize;
    let row = (w as usize * bytes_pp).div_ceil(4) * 4;
    let mask_row = (w as usize).div_ceil(32) * 4;
    let colour = hdr;
    let mask = colour + row * h as usize;
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    let mut any_alpha = false;
    for y in 0..h as usize {
        let src = colour + (h as usize - 1 - y) * row;
        for x in 0..w as usize {
            let p = d.get(src + x * bytes_pp..src + x * bytes_pp + bytes_pp)?;
            let i = (y * w as usize + x) * 4;
            rgba[i] = p[2];
            rgba[i + 1] = p[1];
            rgba[i + 2] = p[0];
            rgba[i + 3] = if bpp == 32 { p[3] } else { 255 };
            any_alpha |= bpp == 32 && p[3] != 0;
        }
    }
    // No alpha in the colour rows: the AND mask says what's transparent.
    if !any_alpha {
        for y in 0..h as usize {
            let src = mask + (h as usize - 1 - y) * mask_row;
            for x in 0..w as usize {
                let bit = d.get(src + x / 8).map_or(0, |m| (m >> (7 - x % 8)) & 1);
                rgba[(y * w as usize + x) * 4 + 3] = if bit == 1 { 0 } else { 255 };
            }
        }
    }
    Some(Rgba { rgba, width: w, height: h })
}

/// The app's own icon (assets/icon.png, built into the program).
pub fn built_in() -> Rgba {
    decode_png(include_bytes!("../assets/icon.png")).expect("built-in icon")
}

/// The window and taskbar icon: the replacement file when there is a usable
/// one, otherwise the app's own drawn icon.
pub fn window_icon() -> (Rgba, Option<PathBuf>) {
    if let Some(p) = find_icon_file() {
        if let Some(im) = load(&p) {
            return (im, Some(p));
        }
    }
    (built_in(), None)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A one-image .ico with a 2×2 32-bit bitmap.
    fn tiny_ico(width_byte: u8, alpha: u8) -> Vec<u8> {
        let mut dib = Vec::new();
        for v in [40u32, 2, 4] {
            dib.extend_from_slice(&v.to_le_bytes());
        }
        dib.extend_from_slice(&1u16.to_le_bytes());
        dib.extend_from_slice(&32u16.to_le_bytes());
        dib.extend_from_slice(&[0u8; 24]);
        // Bottom row: blue, green; top row: red, white (BGRA).
        dib.extend_from_slice(&[255, 0, 0, alpha, 0, 255, 0, alpha]);
        dib.extend_from_slice(&[0, 0, 255, alpha, 255, 255, 255, alpha]);
        // AND mask rows (4 bytes each): bottom row's first pixel transparent.
        dib.extend_from_slice(&[0x80, 0, 0, 0, 0, 0, 0, 0]);
        let mut b = vec![0, 0, 1, 0, 1, 0];
        b.extend_from_slice(&[width_byte, width_byte, 0, 0, 1, 0, 32, 0]);
        b.extend_from_slice(&(dib.len() as u32).to_le_bytes());
        b.extend_from_slice(&22u32.to_le_bytes());
        b.extend(dib);
        b
    }

    #[test]
    fn reads_a_bitmap_icon_top_row_first() {
        let im = parse_ico(&tiny_ico(2, 255)).unwrap();
        assert_eq!((im.width, im.height), (2, 2));
        assert_eq!(&im.rgba[0..4], &[255, 0, 0, 255]); // top-left red
        assert_eq!(&im.rgba[8..12], &[0, 0, 255, 255]); // bottom-left blue
    }

    #[test]
    fn mask_gives_transparency_without_alpha() {
        let im = parse_ico(&tiny_ico(2, 0)).unwrap();
        assert_eq!(im.rgba[11], 0); // bottom-left masked out
        assert_eq!(im.rgba[3], 255);
    }

    #[test]
    fn not_an_icon() {
        assert!(parse_ico(b"hello world, not an icon").is_none());
        assert!(parse_ico(&[0, 0, 2, 0, 0, 0]).is_none()); // a cursor file
    }

    #[test]
    fn built_in_icon_has_the_sizes() {
        let im = built_in();
        assert!(im.width >= 256 && im.width == im.height);
        assert_eq!(im.rgba.len(), (im.width * im.height * 4) as usize);
        // The black tile: the middle of the left edge band is dark and opaque.
        let i = ((im.height / 2 * im.width + im.width / 8) * 4) as usize;
        assert!(im.rgba[i] < 40 && im.rgba[i + 3] > 200);
        assert!(crate::appicon::EXE_ICON_SIZES.contains(&crate::appicon::SMALL_ICON_PX));
        assert!(crate::appicon::EXE_ICON_SIZES.contains(&crate::appicon::BIG_ICON_PX));
    }
}
