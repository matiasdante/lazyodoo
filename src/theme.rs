//! Paleta de Odoo sobre el fondo de la terminal (negro):
//! primario #714B67 (violeta), secundario #017E84 (verde azulado) y gris #8F8F8F.
//! Con truecolor se usan los RGB exactos; sin él, el índice más cercano de la
//! paleta de 256 colores.

use std::sync::OnceLock;

use ratatui::style::{Color, Modifier, Style};

const PRIMARY: (u8, u8, u8) = (0x71, 0x4B, 0x67);
const ACCENT: (u8, u8, u8) = (0x01, 0x7E, 0x84);
const GRAY: (u8, u8, u8) = (0x8F, 0x8F, 0x8F);

/// Windows Terminal no define `COLORTERM`, pero sí `WT_SESSION`.
fn detect_truecolor(colorterm: Option<&str>, windows_terminal: bool) -> bool {
    windows_terminal || colorterm.is_some_and(|v| v.contains("truecolor") || v.contains("24bit"))
}

fn truecolor() -> bool {
    static TRUECOLOR: OnceLock<bool> = OnceLock::new();
    *TRUECOLOR.get_or_init(|| {
        detect_truecolor(
            std::env::var("COLORTERM").ok().as_deref(),
            std::env::var_os("WT_SESSION").is_some(),
        )
    })
}

fn pick(rgb: (u8, u8, u8), indexed: u8, truecolor: bool) -> Color {
    if truecolor {
        Color::Rgb(rgb.0, rgb.1, rgb.2)
    } else {
        Color::Indexed(indexed)
    }
}

/// Violeta de Odoo: fondos y bordes de lo seleccionado.
pub fn primary() -> Color {
    pick(PRIMARY, 96, truecolor())
}

/// Verde azulado de Odoo: acento, foco y títulos.
pub fn accent() -> Color {
    pick(ACCENT, 30, truecolor())
}

pub fn gray() -> Color {
    pick(GRAY, 245, truecolor())
}

pub fn dim() -> Style {
    Style::new().fg(gray())
}

pub fn heading() -> Style {
    Style::new().fg(accent()).add_modifier(Modifier::BOLD)
}

/// Borde de un panel: acento si tiene el foco, gris si no.
pub fn border(focused: bool) -> Style {
    Style::new().fg(if focused { accent() } else { gray() })
}

/// Fila seleccionada en el panel con foco.
pub fn selected() -> Style {
    Style::new()
        .bg(primary())
        .fg(Color::White)
        .add_modifier(Modifier::BOLD)
}

/// Marca « lazyodoo » de la barra superior.
pub fn badge() -> Style {
    selected()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truecolor_detection() {
        assert!(detect_truecolor(Some("truecolor"), false));
        assert!(detect_truecolor(Some("24bit"), false));
        assert!(detect_truecolor(None, true));
        assert!(!detect_truecolor(Some("256color"), false));
        assert!(!detect_truecolor(None, false));
    }

    #[test]
    fn exact_rgb_with_truecolor_and_close_index_without() {
        assert_eq!(pick(PRIMARY, 96, true), Color::Rgb(0x71, 0x4B, 0x67));
        assert_eq!(pick(ACCENT, 30, true), Color::Rgb(0x01, 0x7E, 0x84));
        assert_eq!(pick(GRAY, 245, false), Color::Indexed(245));
    }
}
