// The app's icon: assets/icon.png, the black tile with a white hexagon
// (eframe/egui's default window icon, MIT OR Apache-2.0; see
// assets/CREDITS.md). Plain std only, so build.rs can include this file and
// put the same picture in the program file for Explorer, shortcuts and pins.

/// The original sets a 16 × 16 small icon (title bar) and a 32 × 32 big icon
/// (taskbar, Alt+Tab) (IC2).
#[allow(dead_code)]
pub const SMALL_ICON_PX: u32 = 16;
#[allow(dead_code)]
pub const BIG_ICON_PX: u32 = 32;
/// Sizes in the program file's icon, the same set as the original's (IC5:
/// 16, 32, 48, 64, 128, 256; 32-bit colour).
#[allow(dead_code)]
pub const EXE_ICON_SIZES: [u32; 6] = [SMALL_ICON_PX, BIG_ICON_PX, 48, 64, 128, 256];
/// The icon picture, relative to the app folder.
#[allow(dead_code)]
pub const ICON_PNG_PATH: &str = "assets/icon.png";
