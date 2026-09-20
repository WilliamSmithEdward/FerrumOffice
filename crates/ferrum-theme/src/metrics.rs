//! Grid geometry: sizes, pixel alignment and zoom.
//!
//! None of this depends on the theme. Light and dark lay out identically, so
//! switching theme never moves a cell boundary by a pixel and a screenshot
//! taken in one theme lines up with the other.
//!
//! The defaults are measured from a live Excel window rather than recalled,
//! so a Ferrum sheet at 100% zoom puts its gridlines where the muscle memory
//! expects them. The measurement and its method are in
//! `docs/design/theme.md`.

/// Typographic points per inch. Fixed by the definition of a point.
pub const POINTS_PER_INCH: f64 = 72.0;

/// The reference display density. Windows calls this 100% scaling.
pub const DEFAULT_DPI: f64 = 96.0;

// The document's own defaults live in `ferrum-core`, because a column width
// is stored in the file and has nothing to do with how it is drawn. They are
// re-exported here so that a renderer has one place to look.
pub use ferrum_core::defaults::{
    COLUMN_WIDTH_CHARS as DEFAULT_COLUMN_WIDTH_CHARS, COLUMN_WIDTH_PT as DEFAULT_COLUMN_WIDTH_PT,
    FONT_SIZE_PT as DEFAULT_FONT_SIZE_PT, FONT_STACK, ROW_HEIGHT_PT as DEFAULT_ROW_HEIGHT_PT,
};

/// The smallest and largest zoom the UI offers, matching the usual range.
pub const MIN_ZOOM: f64 = 0.10;
pub const MAX_ZOOM: f64 = 4.00;

/// Converts the sheet's logical geometry into device pixels.
///
/// Holds the two factors that are not part of the document: how dense the
/// display is, and how far the user has zoomed.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Metrics {
    dpi: f64,
    zoom: f64,
}

impl Default for Metrics {
    fn default() -> Self {
        Self {
            dpi: DEFAULT_DPI,
            zoom: 1.0,
        }
    }
}

impl Metrics {
    /// Build for a display density and a zoom factor.
    ///
    /// Zoom is clamped to the offered range, so a bad value degrades to the
    /// nearest usable one rather than producing a grid nobody can read.
    pub fn new(dpi: f64, zoom: f64) -> Self {
        Self {
            dpi: if dpi > 0.0 { dpi } else { DEFAULT_DPI },
            zoom: zoom.clamp(MIN_ZOOM, MAX_ZOOM),
        }
    }

    pub const fn dpi(self) -> f64 {
        self.dpi
    }

    pub const fn zoom(self) -> f64 {
        self.zoom
    }

    pub fn with_zoom(self, zoom: f64) -> Self {
        Self {
            zoom: zoom.clamp(MIN_ZOOM, MAX_ZOOM),
            ..self
        }
    }

    /// Scale from points to pixels, before rounding.
    pub fn points_to_pixels(self, points: f64) -> f64 {
        points * self.zoom * self.dpi / POINTS_PER_INCH
    }

    pub fn pixels_to_points(self, pixels: f64) -> f64 {
        pixels * POINTS_PER_INCH / (self.zoom * self.dpi)
    }

    /// Snap a dimension to whole device pixels.
    ///
    /// Cell boundaries land on pixel edges so a one-pixel gridline stays one
    /// pixel and does not blur across two. A dimension never rounds to zero,
    /// because a hidden row is a state rather than a rounding result.
    pub fn snap(self, points: f64) -> i32 {
        let pixels = self.points_to_pixels(points).round() as i32;
        pixels.max(1)
    }

    /// Width of a column, in whole pixels.
    pub fn column_width_px(self, points: f64) -> i32 {
        self.snap(points)
    }

    /// Height of a row, in whole pixels.
    pub fn row_height_px(self, points: f64) -> i32 {
        self.snap(points)
    }

    /// Width of the row-number gutter, in whole pixels.
    ///
    /// Grows with the widest row number on screen, the way a spreadsheet's
    /// does, so the digits never clip and the grid does not shift on every
    /// scroll. `highest_row` is one-based.
    pub fn row_header_width_px(self, highest_row: u32) -> i32 {
        let digits = highest_row.max(1).ilog10() + 1;
        // Roughly half an em per digit for a narrow face, plus padding.
        let points = 9.0 + f64::from(digits) * 4.5;
        self.snap(points)
    }

    /// Height of the column-letter header, in whole pixels.
    pub fn column_header_height_px(self) -> i32 {
        self.snap(DEFAULT_ROW_HEIGHT_PT + 2.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_measured_defaults_reproduce_at_their_measured_density() {
        // 144 DPI is where the reference capture was taken, and both figures
        // there were whole pixels: 96 wide and 29 tall.
        let at_144 = Metrics::new(144.0, 1.0);
        assert_eq!(at_144.column_width_px(DEFAULT_COLUMN_WIDTH_PT), 96);
        assert_eq!(at_144.row_height_px(DEFAULT_ROW_HEIGHT_PT), 29);
    }

    #[test]
    fn the_defaults_hold_at_the_reference_density() {
        let at_96 = Metrics::default();
        assert_eq!(at_96.column_width_px(DEFAULT_COLUMN_WIDTH_PT), 64);
        // 14.5pt is 19.33px at 96 DPI, which snaps down.
        assert_eq!(at_96.row_height_px(DEFAULT_ROW_HEIGHT_PT), 19);
    }

    #[test]
    fn points_and_pixels_round_trip() {
        for dpi in [96.0, 120.0, 144.0, 192.0] {
            for zoom in [0.5, 1.0, 1.5, 2.0] {
                let metrics = Metrics::new(dpi, zoom);
                let there = metrics.points_to_pixels(48.0);
                let back = metrics.pixels_to_points(there);
                assert!((back - 48.0).abs() < 1e-9, "dpi {dpi} zoom {zoom}");
            }
        }
    }

    #[test]
    fn zoom_scales_the_grid() {
        let base = Metrics::default();
        let doubled = base.with_zoom(2.0);
        assert_eq!(base.column_width_px(DEFAULT_COLUMN_WIDTH_PT), 64);
        assert_eq!(doubled.column_width_px(DEFAULT_COLUMN_WIDTH_PT), 128);
    }

    #[test]
    fn zoom_is_clamped_to_the_offered_range() {
        assert_eq!(Metrics::new(96.0, 100.0).zoom(), MAX_ZOOM);
        assert_eq!(Metrics::new(96.0, 0.0).zoom(), MIN_ZOOM);
        assert_eq!(Metrics::new(96.0, -3.0).zoom(), MIN_ZOOM);
    }

    #[test]
    fn a_dimension_never_snaps_away_to_nothing() {
        // A very small row at a very small zoom still occupies a pixel.
        let tiny = Metrics::new(96.0, MIN_ZOOM);
        assert!(tiny.row_height_px(0.1) >= 1);
    }

    #[test]
    fn the_row_gutter_grows_with_the_digit_count() {
        let metrics = Metrics::default();
        let one_digit = metrics.row_header_width_px(9);
        let four_digits = metrics.row_header_width_px(1_000);
        let seven_digits = metrics.row_header_width_px(1_048_576);
        assert!(one_digit < four_digits, "{one_digit} < {four_digits}");
        assert!(four_digits < seven_digits, "{four_digits} < {seven_digits}");
    }

    #[test]
    fn a_bad_density_falls_back_rather_than_dividing_by_zero() {
        let metrics = Metrics::new(0.0, 1.0);
        assert_eq!(metrics.dpi(), DEFAULT_DPI);
        assert!(metrics.points_to_pixels(48.0).is_finite());
    }
}
