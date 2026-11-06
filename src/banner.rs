//! Banner de inicio: «LAZYODOO» en letras grandes, con el bloque en el acento
//! de Odoo y la sombra en violeta. Si la terminal es angosta, una línea simple.

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::i18n::tr;
use crate::theme;

const L: [&str; 6] = [
    "██╗     ",
    "██║     ",
    "██║     ",
    "██║     ",
    "███████╗",
    "╚══════╝",
];
const A: [&str; 6] = [
    " █████╗ ",
    "██╔══██╗",
    "███████║",
    "██╔══██║",
    "██║  ██║",
    "╚═╝  ╚═╝",
];
const Z: [&str; 6] = [
    "███████╗",
    "╚══███╔╝",
    "  ███╔╝ ",
    " ███╔╝  ",
    "███████╗",
    "╚══════╝",
];
const Y: [&str; 6] = [
    "██╗   ██╗",
    "╚██╗ ██╔╝",
    " ╚████╔╝ ",
    "  ╚██╔╝  ",
    "   ██║   ",
    "   ╚═╝   ",
];
const O: [&str; 6] = [
    " ██████╗ ",
    "██╔═══██╗",
    "██║   ██║",
    "██║   ██║",
    "╚██████╔╝",
    " ╚═════╝ ",
];
const D: [&str; 6] = [
    "██████╗ ",
    "██╔══██╗",
    "██║  ██║",
    "██║  ██║",
    "██████╔╝",
    "╚═════╝ ",
];

/// Ancho en columnas del banner completo.
pub const WIDTH: u16 = 68;

fn rows() -> [String; 6] {
    let word = [&L, &A, &Z, &Y, &O, &D, &O, &O];
    std::array::from_fn(|row| word.iter().map(|glyph| glyph[row]).collect())
}

/// El bloque (`█`) va en el acento y la sombra (`╗╔║═╝╚`) en violeta.
fn styled(row: &str) -> Line<'static> {
    let block = Style::new()
        .fg(theme::accent())
        .add_modifier(Modifier::BOLD);
    let shadow = Style::new().fg(theme::primary());
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut current = String::new();
    let mut current_is_block = true;
    for ch in row.chars() {
        let is_block = matches!(ch, '█' | ' ');
        if is_block != current_is_block && !current.is_empty() {
            let style = if current_is_block { block } else { shadow };
            spans.push(Span::styled(std::mem::take(&mut current), style));
        }
        current_is_block = is_block;
        current.push(ch);
    }
    if !current.is_empty() {
        let style = if current_is_block { block } else { shadow };
        spans.push(Span::styled(current, style));
    }
    Line::from(spans)
}

/// El banner completo, o una línea simple si no entra.
pub fn lines(width: u16) -> Vec<Line<'static>> {
    if width >= WIDTH {
        rows().iter().map(|r| styled(r)).collect()
    } else {
        vec![Line::styled("LAZYODOO", theme::heading())]
    }
}

/// Pantalla de inicio: banner centrado, leyenda y cómo seguir.
pub fn render(frame: &mut Frame) {
    let area = frame.area();
    let mut text = lines(area.width);
    text.push(Line::raw(""));
    text.push(Line::styled(
        tr("Odoo tasks and hours, from the terminal"),
        theme::dim(),
    ));
    text.push(Line::styled(
        concat!("v", env!("CARGO_PKG_VERSION")),
        Style::new().fg(theme::primary()),
    ));
    text.push(Line::raw(""));
    text.push(Line::styled(
        tr("Press any key to continue"),
        theme::heading(),
    ));
    let height = text.len() as u16;
    let [_, middle, _] = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(height.min(area.height)),
        Constraint::Fill(1),
    ])
    .areas(area);
    frame.render_widget(Paragraph::new(text).alignment(Alignment::Center), middle);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    #[test]
    fn rows_have_the_declared_width() {
        for row in rows() {
            assert_eq!(row.chars().count() as u16, WIDTH, "{row}");
        }
    }

    fn screen(width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(render).unwrap();
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect()
    }

    #[test]
    fn wide_terminals_get_the_big_banner_and_narrow_ones_plain_text() {
        let wide = screen(100, 20);
        assert!(wide.contains("██╗") && wide.contains("Press any key to continue"));
        let narrow = screen(40, 12);
        assert!(narrow.contains("LAZYODOO") && !narrow.contains("██╗"));
        screen(5, 2);
    }
}
