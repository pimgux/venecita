//! Small colored icons drawn in code, in the style of macOS System Settings: a white symbol
//! on a rounded square of color. Used by the tray menu and the settings window.
//!
//! Shapes are described as signed distance functions (negative inside) in a unit square
//! and rasterized with one pixel of anti-aliasing, so they look clean at any size.

/// The icons Venecita draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IconKind {
    /// A person: profiles.
    Profile,
    /// A 2 × 2 grid: arranging windows, layouts.
    Arrange,
    /// Two bars: pause.
    Pause,
    /// A triangle: resume.
    Resume,
    /// A gear: settings.
    Settings,
    /// The power symbol: quit.
    Quit,
    /// An exclamation mark: something needs attention.
    Warning,
    /// A monitor: displays.
    Display,
    /// A key cap: keyboard shortcuts.
    Keyboard,
    /// A slider: general settings.
    General,
}

impl IconKind {
    /// The tile color (Apple's system colors).
    pub fn color(self) -> [u8; 3] {
        match self {
            IconKind::Profile => [10, 132, 255],   // blue
            IconKind::Arrange => [48, 209, 88],    // green
            IconKind::Pause => [255, 159, 10],     // orange
            IconKind::Resume => [48, 209, 88],     // green
            IconKind::Settings => [142, 142, 147], // gray
            IconKind::Quit => [255, 69, 58],       // red
            IconKind::Warning => [255, 204, 0],    // yellow
            IconKind::Display => [94, 92, 230],    // indigo
            IconKind::Keyboard => [100, 210, 255], // cyan
            IconKind::General => [142, 142, 147],  // gray
        }
    }
}

/// The icon as `size × size` RGBA pixels (straight alpha, row by row).
pub fn icon_rgba(kind: IconKind, size: u32) -> Vec<u8> {
    let n = size as f32;
    let px = 1.0 / n; // one pixel in unit coordinates
    let [r, g, b] = kind.color();
    let mut out = vec![0u8; (size * size * 4) as usize];
    for y in 0..size {
        for x in 0..size {
            let p = ((x as f32 + 0.5) / n, (y as f32 + 0.5) / n);
            let tile = coverage(sd_round_rect(p, (0.5, 0.5), (0.47, 0.47), 0.22), px);
            if tile == 0.0 {
                continue;
            }
            let glyph = coverage(glyph(kind, p), px);
            // White glyph over the tile color (dark glyph on the yellow warning tile).
            let ink = if kind == IconKind::Warning {
                40.0
            } else {
                255.0
            };
            let mix = |c: u8| (f32::from(c) * (1.0 - glyph) + ink * glyph).round() as u8;
            let i = ((y * size + x) * 4) as usize;
            out[i..i + 4].copy_from_slice(&[mix(r), mix(g), mix(b), (tile * 255.0).round() as u8]);
        }
    }
    out
}

/// Pixel coverage from a signed distance (in unit coordinates), with 1 px of smoothing.
fn coverage(distance: f32, px: f32) -> f32 {
    (0.5 - distance / px).clamp(0.0, 1.0)
}

fn glyph(kind: IconKind, p: (f32, f32)) -> f32 {
    match kind {
        IconKind::Profile => {
            let head = sd_circle(p, (0.5, 0.38), 0.13);
            let body = sd_circle(p, (0.5, 0.88), 0.27).max(p.1 - 0.78);
            head.min(body)
        }
        IconKind::Arrange => [(0.36, 0.36), (0.64, 0.36), (0.36, 0.64), (0.64, 0.64)]
            .into_iter()
            .map(|c| sd_round_rect(p, c, (0.115, 0.115), 0.04))
            .fold(f32::MAX, f32::min),
        IconKind::Pause => sd_round_rect(p, (0.40, 0.5), (0.06, 0.19), 0.03).min(sd_round_rect(
            p,
            (0.60, 0.5),
            (0.06, 0.19),
            0.03,
        )),
        IconKind::Resume => sd_triangle(p, (0.38, 0.29), (0.38, 0.71), (0.73, 0.5)) - 0.02,
        IconKind::Settings => {
            let (dx, dy) = (p.0 - 0.5, p.1 - 0.5);
            let r = (dx * dx + dy * dy).sqrt();
            let angle = dy.atan2(dx);
            // Eight teeth: a rounded square wave on the outer radius.
            let wave = ((angle * 8.0).cos() * 3.0).clamp(-1.0, 1.0);
            let outer = r - (0.27 + 0.055 * wave);
            let hole = 0.1 - r;
            outer.max(hole)
        }
        IconKind::Quit => {
            let (dx, dy) = (p.0 - 0.5, p.1 - 0.52);
            let r = (dx * dx + dy * dy).sqrt();
            let ring = (r - 0.22).abs() - 0.045;
            // Open at the top: cut where the angle is near straight up.
            let up = (-dy).atan2(dx.abs()); // π/2 straight up
            let gap = (std::f32::consts::FRAC_PI_2 - up) * r - 0.09;
            let arc = ring.max(-gap);
            let bar = sd_segment(p, (0.5, 0.22), (0.5, 0.5)) - 0.045;
            arc.min(bar)
        }
        IconKind::Warning => {
            let bar = sd_segment(p, (0.5, 0.25), (0.5, 0.58)) - 0.06;
            let dot = sd_circle(p, (0.5, 0.74), 0.065);
            bar.min(dot)
        }
        IconKind::Display => {
            let screen = sd_round_rect(p, (0.5, 0.44), (0.27, 0.18), 0.04);
            let inner = sd_round_rect(p, (0.5, 0.44), (0.21, 0.12), 0.01);
            let stand = sd_segment(p, (0.5, 0.62), (0.5, 0.72)) - 0.03;
            let foot = sd_segment(p, (0.38, 0.74), (0.62, 0.74)) - 0.03;
            screen.max(-inner).min(stand).min(foot)
        }
        IconKind::Keyboard => {
            let body = sd_round_rect(p, (0.5, 0.5), (0.31, 0.2), 0.05);
            let inner = sd_round_rect(p, (0.5, 0.5), (0.26, 0.15), 0.02);
            let keys = [0.34, 0.45, 0.56, 0.67]
                .into_iter()
                .map(|x| sd_round_rect(p, (x, 0.45), (0.035, 0.035), 0.01))
                .fold(f32::MAX, f32::min);
            let space = sd_round_rect(p, (0.5, 0.58), (0.14, 0.03), 0.01);
            body.max(-inner).min(keys).min(space)
        }
        IconKind::General => {
            let line = |y: f32| sd_segment(p, (0.27, y), (0.73, y)) - 0.025;
            let knob = |x: f32, y: f32| sd_circle(p, (x, y), 0.075);
            line(0.36)
                .min(line(0.64))
                .min(knob(0.62, 0.36))
                .min(knob(0.38, 0.64))
        }
    }
}

fn sd_circle(p: (f32, f32), c: (f32, f32), r: f32) -> f32 {
    ((p.0 - c.0).powi(2) + (p.1 - c.1).powi(2)).sqrt() - r
}

fn sd_round_rect(p: (f32, f32), c: (f32, f32), half: (f32, f32), r: f32) -> f32 {
    let qx = (p.0 - c.0).abs() - half.0 + r;
    let qy = (p.1 - c.1).abs() - half.1 + r;
    let outside = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt();
    outside + qx.max(qy).min(0.0) - r
}

fn sd_segment(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let (pax, pay) = (p.0 - a.0, p.1 - a.1);
    let (bax, bay) = (b.0 - a.0, b.1 - a.1);
    let h = ((pax * bax + pay * bay) / (bax * bax + bay * bay)).clamp(0.0, 1.0);
    ((pax - bax * h).powi(2) + (pay - bay * h).powi(2)).sqrt()
}

fn sd_triangle(p: (f32, f32), a: (f32, f32), b: (f32, f32), c: (f32, f32)) -> f32 {
    let d = sd_segment(p, a, b)
        .min(sd_segment(p, b, c))
        .min(sd_segment(p, c, a));
    let side = |(x0, y0): (f32, f32), (x1, y1): (f32, f32)| {
        (x1 - x0) * (p.1 - y0) - (y1 - y0) * (p.0 - x0)
    };
    let (s1, s2, s3) = (side(a, b), side(b, c), side(c, a));
    let inside = (s1 >= 0.0 && s2 >= 0.0 && s3 >= 0.0) || (s1 <= 0.0 && s2 <= 0.0 && s3 <= 0.0);
    if inside {
        -d
    } else {
        d
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [IconKind; 10] = [
        IconKind::Profile,
        IconKind::Arrange,
        IconKind::Pause,
        IconKind::Resume,
        IconKind::Settings,
        IconKind::Quit,
        IconKind::Warning,
        IconKind::Display,
        IconKind::Keyboard,
        IconKind::General,
    ];

    fn pixel(rgba: &[u8], size: u32, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * size + x) * 4) as usize;
        rgba[i..i + 4].try_into().unwrap()
    }

    #[test]
    fn tiles_have_transparent_corners_and_an_opaque_body() {
        for kind in ALL {
            let size = 36;
            let icon = icon_rgba(kind, size);
            assert_eq!(icon.len(), (size * size * 4) as usize);
            assert_eq!(pixel(&icon, size, 0, 0)[3], 0, "{kind:?}: rounded corner");
            assert_eq!(
                pixel(&icon, size, 4, size / 2)[3],
                255,
                "{kind:?}: tile edge"
            );
        }
    }

    #[test]
    fn every_icon_draws_a_glyph() {
        for kind in ALL {
            let size = 48;
            let icon = icon_rgba(kind, size);
            let tile = kind.color();
            let glyph_pixels = icon
                .chunks(4)
                .filter(|p| p[3] == 255 && p[..3] != tile)
                .count();
            assert!(
                glyph_pixels > 40,
                "{kind:?}: only {glyph_pixels} glyph pixels"
            );
        }
    }
}
