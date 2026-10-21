//! Writes the application icon files from `venecita_app::icons::app_icon_rgba`:
//!
//! ```sh
//! cargo run -p venecita-app --example app-icons -- crates/venecita-app/assets/icons
//! ```
//!
//! * `AppIcon.png` (1024 px): the macOS `.icns` is made from it when bundling.
//! * `AppIcon.ico` (16–256 px, PNG-compressed): embedded in the Windows executable.

use std::path::Path;

use venecita_app::icons::app_icon_rgba;

fn png(size: u32) -> Vec<u8> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, size, size);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("png header");
    writer
        .write_image_data(&app_icon_rgba(size))
        .expect("png data");
    writer.finish().expect("png end");
    out
}

/// An .ico whose images are PNGs (supported since Windows Vista).
fn ico(sizes: &[u32]) -> Vec<u8> {
    let images: Vec<Vec<u8>> = sizes.iter().map(|&s| png(s)).collect();
    let mut out = Vec::new();
    out.extend([0, 0, 1, 0]); // reserved, type 1 = icon
    out.extend((sizes.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * sizes.len() as u32;
    for (size, image) in sizes.iter().zip(&images) {
        let side = if *size >= 256 { 0 } else { *size as u8 }; // 0 means 256
        out.extend([side, side, 0, 0]); // width, height, palette, reserved
        out.extend(1u16.to_le_bytes()); // color planes
        out.extend(32u16.to_le_bytes()); // bits per pixel
        out.extend((image.len() as u32).to_le_bytes());
        out.extend(offset.to_le_bytes());
        offset += image.len() as u32;
    }
    for image in images {
        out.extend(image);
    }
    out
}

fn main() {
    let dir = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "crates/venecita-app/assets/icons".into());
    let dir = Path::new(&dir);
    std::fs::create_dir_all(dir).expect("output directory");
    std::fs::write(dir.join("AppIcon.png"), png(1024)).expect("write png");
    std::fs::write(
        dir.join("AppIcon.ico"),
        ico(&[16, 24, 32, 48, 64, 128, 256]),
    )
    .expect("write ico");
    println!("wrote {}", dir.display());
}
