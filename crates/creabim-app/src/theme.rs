//! The interface themes. The web panes take their colours from app.css by
//! the theme's id; egui (the properties panel and the views' own controls)
//! takes the same colours from here.

use eframe::egui::Color32;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Theme {
    /// The original dark look.
    #[default]
    Dark,
    /// The original light look.
    Light,
    /// Warm dark grey with a steel accent and gilt highlights.
    Graphite,
    /// Warm off-white, like drawing paper.
    Paper,
    /// Deep blue, like a blueprint.
    Blueprint,
    /// Pale green-grey.
    Sage,
    /// Near black with a gold accent, for high contrast.
    Ink,
}

/// The colours egui needs.
pub struct Palette {
    pub dark: bool,
    pub bg: Color32,
    pub panel: Color32,
    pub panel2: Color32,
    pub raise: Color32,
    pub raise2: Color32,
    pub border: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub accent: Color32,
}

const fn hex(v: u32) -> Color32 {
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

impl Theme {
    pub const ALL: [Theme; 7] = [
        Theme::Dark,
        Theme::Light,
        Theme::Graphite,
        Theme::Paper,
        Theme::Blueprint,
        Theme::Sage,
        Theme::Ink,
    ];

    /// The name app.css and the settings file know it by.
    pub fn id(self) -> &'static str {
        match self {
            Theme::Dark => "dark",
            Theme::Light => "light",
            Theme::Graphite => "graphite",
            Theme::Paper => "paper",
            Theme::Blueprint => "blueprint",
            Theme::Sage => "sage",
            Theme::Ink => "ink",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Theme::Dark => "Dark",
            Theme::Light => "Light",
            Theme::Graphite => "Graphite",
            Theme::Paper => "Paper",
            Theme::Blueprint => "Blueprint",
            Theme::Sage => "Sage",
            Theme::Ink => "Ink (high contrast)",
        }
    }

    pub fn from_id(id: &str) -> Option<Theme> {
        Theme::ALL.into_iter().find(|t| t.id() == id)
    }

    /// The next theme, for Ctrl+Shift+T.
    pub fn next(self) -> Theme {
        let i = Theme::ALL.iter().position(|t| *t == self).unwrap_or(0);
        Theme::ALL[(i + 1) % Theme::ALL.len()]
    }

    pub fn palette(self) -> Palette {
        // the template's tokens: bg, panel, panel2, line, line2, ink, ink3, accent
        let t = |dark, bg, panel, panel2, line, line2, ink, ink3, accent| {
            let (raise, raise2) = if dark { (line, line2) } else { (panel2, line) };
            Palette {
                dark,
                bg: hex(bg),
                panel: hex(panel),
                panel2: hex(panel2),
                raise: hex(raise),
                raise2: hex(raise2),
                border: hex(line),
                text: hex(ink),
                muted: hex(ink3),
                accent: hex(accent),
            }
        };
        match self {
            Theme::Dark => Palette {
                dark: true,
                bg: hex(0x121419),
                panel: hex(0x17191f),
                panel2: hex(0x1c1f26),
                raise: hex(0x252a33),
                raise2: hex(0x2d323d),
                border: hex(0x272b34),
                text: hex(0xc3c8d2),
                muted: hex(0x8a93a3),
                accent: hex(0x5b8cff),
            },
            Theme::Light => Palette {
                dark: false,
                bg: hex(0xeef0f3),
                panel: hex(0xffffff),
                panel2: hex(0xf6f7f9),
                raise: hex(0xeceef2),
                raise2: hex(0xe2e5ea),
                border: hex(0xe3e6eb),
                text: hex(0x3a404c),
                muted: hex(0x6b7384),
                accent: hex(0x3d6ff5),
            },
            Theme::Graphite => t(
                true, 0x15181C, 0x1C2127, 0x232930, 0x2E353D, 0x3A434C, 0xECE8DF, 0x8A8F93,
                0x7FA6BD,
            ),
            Theme::Paper => t(
                false, 0xF3F1EC, 0xFBFAF7, 0xEEEBE4, 0xD9D4C9, 0xC9C2B4, 0x1F252B, 0x7C7F80,
                0x3B6177,
            ),
            Theme::Blueprint => t(
                true, 0x0F1E29, 0x14283A, 0x1B3349, 0x24405A, 0x2F5070, 0xE4EEF5, 0x7893A8,
                0x8CC4E6,
            ),
            Theme::Sage => t(
                false, 0xEEF1EA, 0xF7F9F3, 0xE3E8DC, 0xCBD3C2, 0xB5C0AA, 0x1E2A22, 0x7A877C,
                0x4F6E58,
            ),
            Theme::Ink => t(
                true, 0x0B0C0E, 0x141518, 0x1D1F23, 0x2A2D33, 0x3A3E46, 0xF5F3EE, 0x8F9096,
                0xF0C36B,
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_theme_round_trips_by_id_and_cycles() {
        for t in Theme::ALL {
            assert_eq!(Theme::from_id(t.id()), Some(t));
        }
        let mut t = Theme::Dark;
        for _ in 0..Theme::ALL.len() {
            t = t.next();
        }
        assert_eq!(t, Theme::Dark);
        assert!(Theme::from_id("nope").is_none());
    }

    #[test]
    fn every_theme_has_a_rule_in_the_stylesheet() {
        let css = include_str!("../ui/app.css");
        for t in Theme::ALL.into_iter().skip(1) {
            assert!(
                css.contains(&format!("html[data-theme=\"{}\"]", t.id())),
                "{}",
                t.id()
            );
        }
    }
}
