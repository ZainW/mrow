//! Mrow's colours, type and measurements: warm paper in light mode, neutral ink in dark mode,
//! with one clay accent. The same values as the Swift app's `Palette` and `Theme`.

use rune::Color;
use rune::color::hex;
use rune::tok::{Family, TextStyle};
use rune_kit::{Kit, PaletteSeed};

/// The strip across the top of the window with the window buttons and actions.
pub const HEADER_HEIGHT: f32 = 40.0;
/// The margin between the card and the window's edges.
pub const GUTTER: f32 = 8.0;
pub const CARD_RADIUS: f32 = 12.0;
/// Room for the close, minimise and zoom buttons at the left of the header. Elsewhere the
/// system draws its own title bar, so the header starts at the gutter.
pub const TRAFFIC_LIGHTS_WIDTH: f32 = if cfg!(target_os = "macos") { 76.0 } else { 8.0 };
/// The widest the merge request's text column gets.
pub const READING_WIDTH: f32 = 780.0;

#[derive(Clone, Copy, Debug)]
pub struct Pal {
    pub dark: bool,
    /// The window itself: sidebar, header and the margin around the card.
    pub chrome: Color,
    /// The card the merge request sits on.
    pub paper: Color,
    /// Things that float: the active tab, menus, sheets.
    pub elevated: Color,
    /// Recessed areas inside the paper, like code and diffs.
    pub well: Color,
    pub text: Color,
    pub secondary: Color,
    pub tertiary: Color,
    /// Hairlines between surfaces.
    pub line: Color,
    /// Lines that need to read as structure, like table rules.
    pub strong_line: Color,
    pub hover: Color,
    pub selected: Color,
    pub accent: Color,
    pub accent_soft: Color,
    pub green: Color,
    pub red: Color,
    pub amber: Color,
    pub blue: Color,
    pub added_line: Color,
    pub removed_line: Color,
    pub added_word: Color,
    pub removed_word: Color,
}

fn c(s: &str) -> Color {
    hex(s).expect("palette colour")
}

/// The light theme's warm near-black at an opacity.
fn ink(a: f32) -> Color {
    c("#3C2D1E").alpha(a)
}

fn glow(a: f32) -> Color {
    c("#FFFFFF").alpha(a)
}

impl Pal {
    pub fn new(dark: bool) -> Pal {
        let pick = |l: &str, d: &str| if dark { c(d) } else { c(l) };
        Pal {
            dark,
            chrome: pick("#F3F1EB", "#131313"),
            paper: pick("#FDFCF9", "#1B1B1B"),
            elevated: pick("#FFFFFF", "#242424"),
            well: pick("#F5F3ED", "#161616"),
            text: pick("#1E1C19", "#ECECEC"),
            secondary: pick("#6B665E", "#A3A3A3"),
            tertiary: pick("#9E998F", "#6E6E6E"),
            line: if dark { glow(0.08) } else { ink(0.1) },
            strong_line: if dark { glow(0.14) } else { ink(0.16) },
            hover: if dark { glow(0.05) } else { ink(0.05) },
            selected: if dark { glow(0.085) } else { ink(0.085) },
            accent: pick("#B95A33", "#E08A66"),
            accent_soft: if dark { c("#E08A66").alpha(0.16) } else { c("#B95A33").alpha(0.12) },
            green: pick("#3F7D4E", "#7BC48D"),
            red: pick("#B4443A", "#E5847A"),
            amber: pick("#9A6B12", "#D9AE5C"),
            blue: pick("#3C6A9E", "#86AEDB"),
            added_line: if dark { c("#5FBF77").alpha(0.11) } else { c("#3F9A55").alpha(0.11) },
            removed_line: if dark { c("#E5655A").alpha(0.11) } else { c("#C9483C").alpha(0.09) },
            added_word: if dark { c("#5FBF77").alpha(0.22) } else { c("#3F9A55").alpha(0.22) },
            removed_word: if dark { c("#E5655A").alpha(0.22) } else { c("#C9483C").alpha(0.2) },
        }
    }

    /// rune-kit's components (menus, inputs, switches) in Mrow's colours.
    pub fn kit(&self) -> Kit {
        let mut k = Kit::from_palette(PaletteSeed { bg: self.paper, fg: self.text, accent: self.accent });
        k.c.page = self.chrome;
        k.c.bg_base = self.paper;
        k.c.surface_float = self.elevated;
        k.c.text_base = self.text;
        k.c.text_muted = self.secondary;
        k.c.text_faint = self.tertiary;
        k.c.icon_base = self.secondary;
        k.c.icon_muted = self.tertiary;
        k.c.border_base = self.line;
        k.c.border_strong = self.strong_line;
        k.c.hover = self.hover;
        k.c.text_accent = self.accent;
        k.c.icon_accent = self.accent;
        k.c.bg_accent = self.accent;
        k.c.success.fg = self.green;
        k.c.danger.fg = self.red;
        k.c.warning.fg = self.amber;
        k.c.info.fg = self.blue;
        k.c.border_focus = self.accent.alpha(0.55);
        k.ty.ui = ui(12.5, 400.0);
        k.ty.ui_medium = ui(12.5, 500.0);
        k.ty.body = ui(13.0, 400.0);
        k.ty.mono = mono(12.0, 400.0);
        k
    }

    /// Text colour for a label background ("#RRGGBB").
    pub fn label_colors(&self, bg: &str, fg: &str) -> (Color, Color) {
        (hex(bg).unwrap_or(self.hover), hex(fg).unwrap_or(self.secondary))
    }
}

/// Inter, the interface face.
pub const fn ui(size: f32, weight: f32) -> TextStyle {
    TextStyle::new(size, weight)
}

/// Geist Mono, for paths, branches, key hints and diffs.
pub const fn mono(size: f32, weight: f32) -> TextStyle {
    TextStyle::new(size, weight).mono()
}

/// Charter, the reading face, for titles and descriptions. macOS ships it; elsewhere it falls
/// back to a serif, then the interface face.
pub const fn serif(size: f32, weight: f32) -> TextStyle {
    TextStyle::new(size, weight).family(Family::Named("Charter, Georgia, serif"))
}

pub const MEDIUM: f32 = 500.0;
pub const SEMIBOLD: f32 = 600.0;
pub const BOLD: f32 = 700.0;

/// Fonts bundled into the binary: Inter for the interface, Geist Mono for code (both OFL).
pub const FONTS: &[&[u8]] = &[include_bytes!("../../assets/fonts/InterVariable.ttf"), include_bytes!("../../assets/fonts/GeistMono-Variable.ttf")];

pub const FAMILIES: (&str, &str) = ("Inter", "Geist Mono");

/// Black at an opacity, for shadows and scrims.
pub fn shade(a: f32) -> Color {
    c("#000000").alpha(a)
}

/// Line height as a multiple of the size; Rune's `lh` takes points.
pub trait LineHeight {
    fn lines(self, multiple: f32) -> Self;
}

impl LineHeight for TextStyle {
    fn lines(self, multiple: f32) -> Self {
        let size = self.size;
        self.lh((size * multiple * 2.0).round() / 2.0)
    }
}
