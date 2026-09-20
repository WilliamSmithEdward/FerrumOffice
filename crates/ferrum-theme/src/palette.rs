//! The colour tokens, and the test that keeps them legible.
//!
//! Light is the default theme; dark is an option. The two differ only in
//! colour. Nothing dimensional lives here.
//!
//! The hues are purple and rust: purple drives interaction (selection, focus,
//! the active tab) because it collides with none of the semantic colours, and
//! rust is the brand mark, which suits a product named after iron.
//!
//! Rust and semantic red are neighbours on the wheel, which is the one awkward
//! consequence of that choice. They are separated by hue rather than by
//! lightness, so the brand reads as burnt orange and an error reads as
//! crimson; a test holds them apart, and an error is never carried by colour
//! alone in any case.
//!
//! The neutrals are warm rather than blue-grey, because a cold grey beside a
//! rust accent reads as a mistake.

/// An opaque 24-bit colour.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    /// Build from `0xRRGGBB`, so the source reads like the hex a designer quotes.
    pub const fn hex(value: u32) -> Self {
        Self {
            r: ((value >> 16) & 0xFF) as u8,
            g: ((value >> 8) & 0xFF) as u8,
            b: (value & 0xFF) as u8,
        }
    }

    pub const fn to_u32(self) -> u32 {
        ((self.r as u32) << 16) | ((self.g as u32) << 8) | (self.b as u32)
    }

    pub fn to_hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }
}

/// Which of the two themes a palette is.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Theme {
    #[default]
    Light,
    Dark,
}

impl Theme {
    pub const fn palette(self) -> &'static Palette {
        match self {
            Self::Light => &LIGHT,
            Self::Dark => &DARK,
        }
    }
}

/// Every colour one theme defines.
#[derive(Clone, Copy, Debug)]
pub struct Palette {
    // Surfaces, darkest-to-lightest in dark and the reverse in light.
    /// The cell area and the code editor background.
    pub canvas: Rgb,
    /// Tool panes and the status bar.
    pub surface: Rgb,
    /// Toolbar, tab strip, formula bar.
    pub chrome: Rgb,
    /// Pane titles, and the row and column headers.
    pub header: Rgb,
    /// Menus, popovers and the active tab.
    pub elevated: Rgb,
    /// The wash over a selected range or a selected line.
    pub selection: Rgb,
    /// The tint on the statement the debugger is stopped at.
    pub run_line: Rgb,

    // Text.
    pub text_primary: Rgb,
    pub text_secondary: Rgb,
    pub text_muted: Rgb,
    pub danger: Rgb,
    pub warning: Rgb,
    pub success: Rgb,
    /// The accent at a weight safe for text.
    pub accent_text: Rgb,
    /// The brand at a weight safe for text.
    pub brand_text: Rgb,

    // Code.
    pub syntax_comment: Rgb,
    pub syntax_keyword: Rgb,
    pub syntax_type: Rgb,
    pub syntax_identifier: Rgb,
    pub syntax_string: Rgb,
    pub syntax_number: Rgb,

    // Non-text UI.
    /// Selection borders, the active cell outline, the active tab marker.
    pub accent: Rgb,
    /// The Ferrum mark. Used sparingly.
    pub brand: Rgb,
    pub focus_ring: Rgb,
    /// A border that has to be seen, as opposed to a hairline.
    pub border_strong: Rgb,

    // Structure.
    pub gridline: Rgb,
    pub divider: Rgb,
}

/// The default theme.
///
/// The cell area and the gridline are Excel's own, measured from a live
/// window rather than recalled, so a sheet looks the way the muscle memory
/// expects. Everything else is Ferrum's.
pub static LIGHT: Palette = Palette {
    canvas: Rgb::hex(0xffffff),
    surface: Rgb::hex(0xfaf8f7),
    chrome: Rgb::hex(0xf4f1f0),
    header: Rgb::hex(0xebe7e5),
    elevated: Rgb::hex(0xffffff),
    selection: Rgb::hex(0xece1f7),
    run_line: Rgb::hex(0xfbefd8),

    text_primary: Rgb::hex(0x1c1a19),
    text_secondary: Rgb::hex(0x494442),
    text_muted: Rgb::hex(0x5e5855),
    danger: Rgb::hex(0xc01c30),
    warning: Rgb::hex(0x7a5000),
    success: Rgb::hex(0x166630),
    accent_text: Rgb::hex(0x6d2b96),
    brand_text: Rgb::hex(0x8f4a16),

    syntax_comment: Rgb::hex(0x2d6324),
    syntax_keyword: Rgb::hex(0x7b2d96),
    syntax_type: Rgb::hex(0x0f5f56),
    syntax_identifier: Rgb::hex(0x0b4f9e),
    syntax_string: Rgb::hex(0x9c3a1c),
    syntax_number: Rgb::hex(0x2d6a2d),

    accent: Rgb::hex(0x7c34a8),
    brand: Rgb::hex(0xa24d1f),
    focus_ring: Rgb::hex(0x7c34a8),
    border_strong: Rgb::hex(0x8a817d),

    // Measured from Excel: #e0e0e0 on #ffffff.
    gridline: Rgb::hex(0xe0e0e0),
    divider: Rgb::hex(0xd4cecb),
};

/// The dark option. Same geometry, same roles, different colours.
pub static DARK: Palette = Palette {
    canvas: Rgb::hex(0x1a1817),
    surface: Rgb::hex(0x201d1c),
    chrome: Rgb::hex(0x252221),
    header: Rgb::hex(0x2b2726),
    elevated: Rgb::hex(0x332e2c),
    selection: Rgb::hex(0x3a2b4a),
    run_line: Rgb::hex(0x352a1f),

    text_primary: Rgb::hex(0xe6e1df),
    text_secondary: Rgb::hex(0xb9b2af),
    text_muted: Rgb::hex(0xa8a19e),
    danger: Rgb::hex(0xff7b8e),
    warning: Rgb::hex(0xe8ab3a),
    success: Rgb::hex(0x5fd184),
    accent_text: Rgb::hex(0xc79ae0),
    brand_text: Rgb::hex(0xe89554),

    syntax_comment: Rgb::hex(0x8fb573),
    syntax_keyword: Rgb::hex(0xc79ae0),
    syntax_type: Rgb::hex(0x7bc7b8),
    syntax_identifier: Rgb::hex(0xafdafc),
    syntax_string: Rgb::hex(0xe09b76),
    syntax_number: Rgb::hex(0xb5cea8),

    accent: Rgb::hex(0xb98ee0),
    brand: Rgb::hex(0xe07840),
    focus_ring: Rgb::hex(0xb98ee0),
    border_strong: Rgb::hex(0x857c78),

    gridline: Rgb::hex(0x332e2c),
    divider: Rgb::hex(0x423b38),
};

impl Palette {
    /// Every background that text or a control can be drawn on.
    pub fn surfaces(&self) -> [(&'static str, Rgb); 7] {
        [
            ("canvas", self.canvas),
            ("surface", self.surface),
            ("chrome", self.chrome),
            ("header", self.header),
            ("elevated", self.elevated),
            ("selection", self.selection),
            ("run_line", self.run_line),
        ]
    }

    /// Every colour used for text, which must clear the body-text floor.
    pub fn text_tokens(&self) -> [(&'static str, Rgb); 14] {
        [
            ("text_primary", self.text_primary),
            ("text_secondary", self.text_secondary),
            ("text_muted", self.text_muted),
            ("danger", self.danger),
            ("warning", self.warning),
            ("success", self.success),
            ("accent_text", self.accent_text),
            ("brand_text", self.brand_text),
            ("syntax_comment", self.syntax_comment),
            ("syntax_keyword", self.syntax_keyword),
            ("syntax_type", self.syntax_type),
            ("syntax_identifier", self.syntax_identifier),
            ("syntax_string", self.syntax_string),
            ("syntax_number", self.syntax_number),
        ]
    }

    /// Every colour used for a non-text control, which must clear the UI floor.
    pub fn ui_tokens(&self) -> [(&'static str, Rgb); 4] {
        [
            ("accent", self.accent),
            ("brand", self.brand),
            ("focus_ring", self.focus_ring),
            ("border_strong", self.border_strong),
        ]
    }

    /// The structural hairlines, held to their own low bar.
    pub fn structure_tokens(&self) -> [(&'static str, Rgb); 2] {
        [("gridline", self.gridline), ("divider", self.divider)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contrast::{STRUCTURE_FLOOR, TEXT_FLOOR, UI_FLOOR, hue_separation, ratio};

    /// The check that makes the palette's legibility a fact rather than a claim.
    ///
    /// Every text colour is held to 4.5:1 and every control colour to 3:1
    /// against the worst surface it can land on. A token that fails names
    /// itself, the surface, and the ratio it managed.
    fn assert_palette_is_legible(name: &str, palette: &Palette) {
        let mut failures = Vec::new();

        for (group, floor) in [
            (palette.text_tokens().to_vec(), TEXT_FLOOR),
            (palette.ui_tokens().to_vec(), UI_FLOOR),
        ] {
            for (token, colour) in group {
                for (surface_name, surface) in palette.surfaces() {
                    let measured = ratio(colour, surface);
                    if measured < floor {
                        failures.push(format!(
                            "{name}: {token} ({}) on {surface_name} ({}) is {measured:.2}:1, \
                             below the {floor}:1 floor",
                            colour.to_hex(),
                            surface.to_hex(),
                        ));
                    }
                }
            }
        }

        for (token, colour) in palette.structure_tokens() {
            let measured = ratio(colour, palette.canvas);
            if measured < STRUCTURE_FLOOR {
                failures.push(format!(
                    "{name}: {token} ({}) on canvas is {measured:.2}:1, below the \
                     {STRUCTURE_FLOOR}:1 floor",
                    colour.to_hex(),
                ));
            }
        }

        assert!(failures.is_empty(), "\n{}", failures.join("\n"));
    }

    #[test]
    fn light_palette_is_legible() {
        assert_palette_is_legible("light", &LIGHT);
    }

    #[test]
    fn dark_palette_is_legible() {
        assert_palette_is_legible("dark", &DARK);
    }

    #[test]
    fn light_is_the_default_theme() {
        assert_eq!(Theme::default(), Theme::Light);
    }

    #[test]
    fn the_light_cell_surface_matches_excel() {
        // Both measured from a live Excel window at 100% zoom.
        assert_eq!(LIGHT.canvas.to_hex(), "#ffffff");
        assert_eq!(LIGHT.gridline.to_hex(), "#e0e0e0");
    }

    #[test]
    fn hex_round_trips() {
        let colour = Rgb::hex(0x7c34a8);
        assert_eq!(colour.to_hex(), "#7c34a8");
        assert_eq!(colour.to_u32(), 0x7c34a8);
        assert_eq!((colour.r, colour.g, colour.b), (0x7c, 0x34, 0xa8));
    }

    /// Rust and red are neighbouring hues, which is the cost of naming a
    /// product after iron. They are pushed far enough apart that the brand
    /// reads as burnt orange and the error reads as crimson.
    ///
    /// The 20 degree bar is this project's own choice, not a borrowed
    /// standard: no specification says how far apart two hues must sit. It is
    /// here to stop the two drifting together over time, and it is a backstop
    /// rather than the real defence, which is that an error is always an icon
    /// and a message as well as a colour.
    const MIN_BRAND_ERROR_HUE_SEPARATION: f64 = 20.0;

    #[test]
    fn the_rust_brand_does_not_read_as_an_error() {
        for (name, palette) in [("light", &LIGHT), ("dark", &DARK)] {
            for (label, brand) in [("brand", palette.brand), ("brand_text", palette.brand_text)] {
                let separation = hue_separation(palette.danger, brand);
                assert!(
                    separation >= MIN_BRAND_ERROR_HUE_SEPARATION,
                    "{name}: {label} {} and danger {} are only {separation:.1} degrees apart",
                    brand.to_hex(),
                    palette.danger.to_hex(),
                );
            }
        }
    }
}
