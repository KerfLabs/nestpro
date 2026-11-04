// Puts the app's icon (assets/icon.png) into the Windows program file, so
// Explorer, shortcuts and pinned taskbar buttons show it too. The running
// window gets the same picture from main.rs (ViewportBuilder::with_icon).

include!("src/appicon.rs");

/// A .ico file with 32-bit images at the usual Windows sizes.
fn ico_bytes(sizes: &[u32]) -> Vec<u8> {
    let mut images: Vec<Vec<u8>> = Vec::new();
    let src = image::open(ICON_PNG_PATH).expect("assets/icon.png").to_rgba8();
    for &s in sizes {
        let rgba = image::imageops::resize(&src, s, s, image::imageops::FilterType::Lanczos3).into_raw();
        let mut b = Vec::new();
        // BITMAPINFOHEADER; height counts the colour rows and the mask rows.
        let mask_row = ((s + 31) / 32 * 4) as usize;
        let img_size = (s * s * 4) as usize + mask_row * s as usize;
        for v in [40u32, s, s * 2] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&32u16.to_le_bytes());
        for v in [0u32, img_size as u32, 0, 0, 0, 0] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        // Colour rows bottom-up, BGRA.
        for y in (0..s).rev() {
            for x in 0..s {
                let i = ((y * s + x) * 4) as usize;
                b.extend_from_slice(&[rgba[i + 2], rgba[i + 1], rgba[i], rgba[i + 3]]);
            }
        }
        // Mask: all zero (alpha does the work).
        b.extend(std::iter::repeat_n(0u8, mask_row * s as usize));
        images.push(b);
    }
    let mut out = Vec::new();
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&(sizes.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * sizes.len();
    for (k, &s) in sizes.iter().enumerate() {
        let d = if s >= 256 { 0 } else { s as u8 };
        out.extend_from_slice(&[d, d, 0, 0]);
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&32u16.to_le_bytes());
        out.extend_from_slice(&(images[k].len() as u32).to_le_bytes());
        out.extend_from_slice(&(offset as u32).to_le_bytes());
        offset += images[k].len();
    }
    for im in images {
        out.extend_from_slice(&im);
    }
    out
}

fn main() {
    println!("cargo:rerun-if-changed=src/appicon.rs");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={ICON_PNG_PATH}");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let ico = out.join("nestpro.ico");
    std::fs::write(&ico, ico_bytes(&EXE_ICON_SIZES)).unwrap();
    let rc = out.join("nestpro.rc");
    let path = ico.display().to_string().replace('\\', "/");
    std::fs::write(&rc, format!("1 ICON \"{path}\"\n")).unwrap();
    // A missing resource compiler only costs the program file its icon; the window
    // still shows it.
    if let Err(e) = embed_resource::compile(&rc, embed_resource::NONE).manifest_optional() {
        println!("cargo:warning=exe icon not embedded: {e}");
    }
}
