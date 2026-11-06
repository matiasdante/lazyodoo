//! Selector de idioma del primer arranque.

use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Alignment, Constraint, Layout};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};

use crate::i18n::Lang;
use crate::theme;

pub enum Outcome {
    None,
    Quit,
    Chosen(Lang),
}

pub struct LangSelect {
    selected: Lang,
}

impl LangSelect {
    pub fn new() -> Self {
        Self {
            selected: Lang::default(),
        }
    }

    #[cfg(test)]
    pub fn selected(&self) -> Lang {
        self.selected
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Outcome {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return Outcome::Quit;
        }
        match key.code {
            KeyCode::Char('1') | KeyCode::Char('e') => return Outcome::Chosen(Lang::En),
            KeyCode::Char('2') | KeyCode::Char('s') => return Outcome::Chosen(Lang::Es),
            KeyCode::Char('3') | KeyCode::Char('p') => return Outcome::Chosen(Lang::Pt),
            KeyCode::Char('4') | KeyCode::Char('z') => return Outcome::Chosen(Lang::Zh),
            KeyCode::Up | KeyCode::Char('k') => self.selected = self.selected.prev(),
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab => {
                self.selected = self.selected.next();
            }
            KeyCode::Enter => return Outcome::Chosen(self.selected),
            KeyCode::Esc | KeyCode::Char('q') => return Outcome::Quit,
            _ => {}
        }
        Outcome::None
    }

    pub fn render(&self, frame: &mut Frame) {
        let area = frame.area();
        frame.render_widget(Clear, area);
        let mut text = vec![
            Line::styled(" lazyodoo ", theme::badge()),
            Line::raw(""),
            // sin traducir a propósito: todavía no se sabe qué idioma lee la persona
            Line::styled(
                "Choose your language · Elegí tu idioma · Escolha o idioma · 选择语言",
                theme::heading(),
            ),
            Line::raw(""),
        ];
        let entries: Vec<String> = Lang::ALL
            .into_iter()
            .enumerate()
            .map(|(i, lang)| format!("{}. {}", i + 1, lang.native_name()))
            .collect();
        // todas del mismo ancho: el bloque se centra y las opciones quedan alineadas
        let width = entries.iter().map(|e| Line::from(e.as_str()).width()).max();
        for (lang, entry) in Lang::ALL.into_iter().zip(&entries) {
            let on = lang == self.selected;
            let pad = width.unwrap_or(0) - Line::from(entry.as_str()).width();
            let style = if on { theme::selected() } else { Style::new() };
            text.push(Line::from(Span::styled(
                format!("{} {entry}{}", if on { "▶" } else { " " }, " ".repeat(pad)),
                style,
            )));
        }
        text.push(Line::raw(""));
        text.push(Line::styled("↑/↓ · Enter · 1-4", theme::dim()));
        let height = text.len() as u16;
        let [_, middle, _] = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Length(height.min(area.height)),
            Constraint::Fill(1),
        ])
        .areas(area);
        frame.render_widget(Paragraph::new(text).alignment(Alignment::Center), middle);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::crossterm::event::KeyEventKind;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new_with_kind(code, KeyModifiers::NONE, KeyEventKind::Press)
    }

    #[test]
    fn english_is_preselected_and_enter_confirms_it() {
        let mut s = LangSelect::new();
        assert_eq!(s.selected(), Lang::En);
        assert!(matches!(
            s.handle_key(key(KeyCode::Enter)),
            Outcome::Chosen(Lang::En)
        ));
    }

    #[test]
    fn arrows_move_and_numbers_pick_directly() {
        let mut s = LangSelect::new();
        s.handle_key(key(KeyCode::Down));
        s.handle_key(key(KeyCode::Down));
        assert!(matches!(
            s.handle_key(key(KeyCode::Enter)),
            Outcome::Chosen(Lang::Pt)
        ));
        s.handle_key(key(KeyCode::Up));
        s.handle_key(key(KeyCode::Up));
        s.handle_key(key(KeyCode::Up));
        assert_eq!(s.selected(), Lang::Zh);
        for (c, lang) in [
            ('1', Lang::En),
            ('2', Lang::Es),
            ('3', Lang::Pt),
            ('4', Lang::Zh),
        ] {
            assert!(matches!(s.handle_key(key(KeyCode::Char(c))), Outcome::Chosen(l) if l == lang));
        }
    }

    #[test]
    fn esc_and_ctrl_c_quit() {
        let mut s = LangSelect::new();
        assert!(matches!(s.handle_key(key(KeyCode::Esc)), Outcome::Quit));
        let ctrl_c = KeyEvent::new_with_kind(
            KeyCode::Char('c'),
            KeyModifiers::CONTROL,
            KeyEventKind::Press,
        );
        assert!(matches!(s.handle_key(ctrl_c), Outcome::Quit));
    }

    #[test]
    fn lists_the_four_languages() {
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal.draw(|f| LangSelect::new().render(f)).unwrap();
        let screen = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect::<String>();
        for lang in Lang::ALL {
            // el carácter ancho deja una celda vacía a continuación
            let screen = screen.replace(' ', "");
            assert!(
                screen.contains(lang.native_name()),
                "{}",
                lang.native_name()
            );
        }
    }
}
