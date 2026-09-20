//! WCAG 2.x relative luminance and contrast, used to hold the palette to a bar.
//!
//! The formulas are from the W3C definition of relative luminance and of the
//! contrast ratio. They are here rather than in a build script so that the
//! palette test can run them directly.

use crate::palette::Rgb;

/// Minimum ratio for body text (WCAG 2.x AA, success criterion 1.4.3).
pub const TEXT_FLOOR: f64 = 4.5;

/// Minimum ratio for icons, borders, focus rings and other non-text UI
/// (WCAG 2.x AA, success criterion 1.4.11).
pub const UI_FLOOR: f64 = 3.0;

/// Minimum ratio for a structural hairline such as a gridline or a divider.
///
/// Not a WCAG figure. A hairline carries no information and must not compete
/// with the data sitting on it, so the bar is calibrated to what a spreadsheet
/// actually ships: Excel draws `#e0e0e0` on `#ffffff`, which computes to
/// 1.32:1 (measured, see `docs/design/theme.md`). This floor sits just under
/// that, so a hairline may be as quiet as Excel's but no quieter.
pub const STRUCTURE_FLOOR: f64 = 1.30;

/// Undo the sRGB transfer function for one channel.
fn to_linear(channel: u8) -> f64 {
    let c = f64::from(channel) / 255.0;
    if c <= 0.040_45 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Relative luminance of a colour, in the range 0.0 to 1.0.
pub fn relative_luminance(colour: Rgb) -> f64 {
    0.2126 * to_linear(colour.r) + 0.7152 * to_linear(colour.g) + 0.0722 * to_linear(colour.b)
}

/// Hue angle in degrees, 0 to 360, with red at 0 and cyan at 180.
///
/// Contrast ratio answers "can this be read", which is a question about
/// lightness. It says nothing about whether two colours look like different
/// colours: pure red and pure blue can share a luminance exactly. When the
/// question is "would someone mistake one of these for the other", hue is the
/// instrument.
///
/// Grey has no hue, so it reports 0.
pub fn hue_degrees(colour: Rgb) -> f64 {
    let (r, g, b) = (
        f64::from(colour.r) / 255.0,
        f64::from(colour.g) / 255.0,
        f64::from(colour.b) / 255.0,
    );
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let chroma = max - min;
    if chroma <= f64::EPSILON {
        return 0.0;
    }
    let sector = if max == r {
        ((g - b) / chroma).rem_euclid(6.0)
    } else if max == g {
        (b - r) / chroma + 2.0
    } else {
        (r - g) / chroma + 4.0
    };
    (sector * 60.0).rem_euclid(360.0)
}

/// Shortest distance between two hue angles, 0 to 180 degrees.
pub fn hue_separation(a: Rgb, b: Rgb) -> f64 {
    let delta = (hue_degrees(a) - hue_degrees(b)).abs();
    delta.min(360.0 - delta)
}

/// Contrast ratio between two colours, from 1.0 to 21.0. Order does not matter.
pub fn ratio(a: Rgb, b: Rgb) -> f64 {
    let (x, y) = (relative_luminance(a), relative_luminance(b));
    let (lighter, darker) = if x > y { (x, y) } else { (y, x) };
    (lighter + 0.05) / (darker + 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;

    const WHITE: Rgb = Rgb::hex(0xffffff);
    const BLACK: Rgb = Rgb::hex(0x000000);

    #[test]
    fn the_extremes_are_the_known_values() {
        // Black on white is the maximum the formula can produce.
        assert!((ratio(BLACK, WHITE) - 21.0).abs() < 0.01);
        assert!((ratio(WHITE, WHITE) - 1.0).abs() < 0.001);
    }

    #[test]
    fn the_ratio_does_not_depend_on_order() {
        let a = Rgb::hex(0x7c34a8);
        let b = Rgb::hex(0xffffff);
        assert!((ratio(a, b) - ratio(b, a)).abs() < 1e-12);
    }

    #[test]
    fn excel_gridline_matches_the_measured_ratio() {
        // Measured from a live Excel window: #e0e0e0 on #ffffff. The ratio is
        // computed here rather than quoted, because the first figure written
        // down was an estimate and was wrong by half a point.
        let measured = ratio(Rgb::hex(0xe0e0e0), WHITE);
        assert!(
            (measured - 1.3201).abs() < 0.001,
            "expected 1.3201:1, computed {measured:.4}"
        );
        assert!(measured >= STRUCTURE_FLOOR);
    }
}
