use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Clear, Paragraph, Wrap};

use crate::config::Config;
use crate::inputs;
use crate::models::sanitize;
use crate::theme;

const LABEL_WIDTH: u16 = 20;

// Posiciones del foco (y de los campos): URL, base, usuario y API key.
const URL: usize = 0;
const KEY: usize = 3;

struct Field {
    label: &'static str,
    hint: &'static str,
    masked: bool,
    value: String,
    /// Posición del cursor, en caracteres.
    cursor: usize,
}

pub enum Outcome {
    None,
    Quit,
    Submit(Config),
}

/// Formulario de primer arranque, a pantalla completa. No guarda nada:
/// devuelve la `Config` y el que lo usa decide cuándo persistirla (después de validar).
pub struct Wizard {
    /// URL, base, usuario y API key (en ese orden).
    fields: [Field; 4],
    focus: usize,
    error: Option<String>,
    validating: bool,
}

impl Wizard {
    pub fn new() -> Self {
        let field = |label, hint, masked| Field {
            label,
            hint,
            masked,
            value: String::new(),
            cursor: 0,
        };
        Self {
            fields: [
                field("URL de Odoo", "https://odoo.example.com", false),
                field("Base de datos", "odoo-db", false),
                field("Usuario de Odoo", "nombre@example.com", false),
                field("API key de Odoo", "APIKEY", true),
            ],
            focus: URL,
            error: None,
            validating: false,
        }
    }

    pub fn set_validating(&mut self) {
        self.validating = true;
        self.error = None;
    }

    pub fn set_error(&mut self, message: String) {
        self.validating = false;
        self.error = Some(message);
    }

    fn move_focus(&mut self, forward: bool) {
        let n = self.fields.len();
        self.focus = if forward {
            (self.focus + 1) % n
        } else {
            (self.focus + n - 1) % n
        };
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Outcome {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if ctrl && key.code == KeyCode::Char('c') {
            return Outcome::Quit;
        }
        if self.validating {
            return Outcome::None;
        }
        match key.code {
            KeyCode::Esc => return Outcome::Quit,
            KeyCode::Tab | KeyCode::Down => self.move_focus(true),
            KeyCode::BackTab | KeyCode::Up => self.move_focus(false),
            KeyCode::Enter => return self.advance_or_submit(),
            _ => {
                let field = &mut self.fields[self.focus];
                inputs::edit(&mut field.value, &mut field.cursor, &key);
            }
        }
        Outcome::None
    }

    /// Pegado (bracketed paste): no pasa por teclas, así que no dispara atajos.
    pub fn handle_paste(&mut self, text: &str) {
        if self.validating {
            return;
        }
        let field = &mut self.fields[self.focus];
        inputs::insert_str(&mut field.value, &mut field.cursor, text);
    }

    fn advance_or_submit(&mut self) -> Outcome {
        if self.focus < KEY {
            self.move_focus(true);
            return Outcome::None;
        }
        let v = |i: usize| self.fields[i].value.as_str();
        match Config::new(v(0), v(1), v(2), v(3)) {
            Ok(config) => {
                self.set_validating();
                Outcome::Submit(config)
            }
            Err(message) => {
                self.error = Some(message);
                Outcome::None
            }
        }
    }

    pub fn render(&self, frame: &mut Frame, spinner: char) {
        let area = frame.area();
        frame.render_widget(Clear, area);
        let mut rows = vec![Constraint::Length(1), Constraint::Length(2)];
        rows.extend(self.fields.iter().map(|_| Constraint::Length(1)));
        rows.extend([
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(1),
        ]);
        let cells = Layout::vertical(rows).split(area);

        frame.render_widget(
            Paragraph::new(inputs::top_bar("primer arranque", "")),
            cells[0],
        );
        frame.render_widget(
            Paragraph::new(
                " Cargá los datos de acceso. Se validan contra Odoo antes de guardarlos.",
            )
            .style(theme::dim()),
            cells[1],
        );
        for (i, field) in self.fields.iter().enumerate() {
            let cell = cells[2 + i];
            let focused = i == self.focus;
            let label_style = if focused {
                theme::heading()
            } else {
                theme::dim()
            };
            let marker =
                ratatui::text::Span::styled(if focused { "▸ " } else { "  " }, label_style);
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    marker,
                    ratatui::text::Span::styled(field.label, label_style),
                ])),
                cell,
            );
            let x = cell.x + 2 + LABEL_WIDTH;
            let value_area = Rect::new(x, cell.y, cell.width.saturating_sub(x - cell.x + 1), 1);
            inputs::draw_line(
                frame,
                value_area,
                &field.value,
                field.cursor,
                focused && !self.validating,
                field.masked,
                field.hint,
                if focused {
                    Style::new().add_modifier(Modifier::BOLD)
                } else {
                    Style::new()
                },
            );
        }

        let status = if self.validating {
            Line::styled(
                format!(" {spinner} Validando contra Odoo…"),
                theme::heading(),
            )
        } else if let Some(error) = &self.error {
            Line::styled(
                format!(" {}", sanitize(error)),
                Style::new().fg(Color::Red).add_modifier(Modifier::BOLD),
            )
        } else {
            Line::raw("")
        };
        frame.render_widget(
            Paragraph::new(status).wrap(Wrap { trim: true }),
            cells[2 + self.fields.len() + 1],
        );
        frame.render_widget(
            Paragraph::new(" Tab/↑↓ campo · Enter seguir/confirmar · Esc salir")
                .style(theme::dim()),
            cells[cells.len() - 1],
        );
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

    fn type_text(w: &mut Wizard, text: &str) {
        for c in text.chars() {
            w.handle_key(key(KeyCode::Char(c)));
        }
    }

    /// URL, base, usuario y API key cargados; el foco queda en la API key.
    fn filled() -> Wizard {
        let mut w = Wizard::new();
        for (i, value) in [
            "https://odoo.example.com",
            "odoo",
            "me@example.com",
            "odoo-key",
        ]
        .into_iter()
        .enumerate()
        {
            type_text(&mut w, value);
            if i < KEY {
                w.handle_key(key(KeyCode::Enter));
            }
        }
        assert_eq!(w.focus, KEY);
        w
    }

    #[test]
    fn enter_on_the_api_key_sends() {
        let mut w = filled();
        match w.handle_key(key(KeyCode::Enter)) {
            Outcome::Submit(c) => {
                assert_eq!(c.base_url, "https://odoo.example.com");
                assert_eq!(c.database, "odoo");
                assert_eq!(c.login, "me@example.com");
                assert_eq!(c.secret, "odoo-key");
            }
            _ => panic!("debía enviar"),
        }
        assert!(w.validating);
    }

    #[test]
    fn enter_before_the_api_key_moves_on_instead_of_sending() {
        let mut w = Wizard::new();
        assert!(matches!(w.handle_key(key(KeyCode::Enter)), Outcome::None));
        assert_eq!(w.focus, 1);
        assert!(!w.validating);
    }

    #[test]
    fn tab_wraps_around_both_ways() {
        let mut w = Wizard::new();
        w.handle_key(key(KeyCode::BackTab));
        assert_eq!(w.focus, KEY);
        w.handle_key(key(KeyCode::Tab));
        assert_eq!(w.focus, URL);
    }

    #[test]
    fn space_is_text_in_fields() {
        let mut w = Wizard::new();
        type_text(&mut w, "a b");
        assert_eq!(w.fields[0].value, "a b");
    }

    #[test]
    fn invalid_input_shows_error_and_does_not_submit() {
        let mut w = Wizard::new();
        w.focus = KEY;
        assert!(matches!(w.handle_key(key(KeyCode::Enter)), Outcome::None));
        assert!(w.error.is_some() && !w.validating);
    }

    #[test]
    fn validation_error_returns_control_to_user() {
        let mut w = filled();
        w.handle_key(key(KeyCode::Enter));
        w.set_error("nope".into());
        assert!(!w.validating);
        w.focus = KEY;
        type_text(&mut w, "x");
        assert!(w.fields[3].value.ends_with('x'));
    }

    #[test]
    fn keys_are_ignored_while_validating_except_ctrl_c() {
        let mut w = filled();
        w.handle_key(key(KeyCode::Enter));
        w.focus = KEY;
        type_text(&mut w, "zzz");
        assert_eq!(w.fields[3].value, "odoo-key");
        let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert!(matches!(w.handle_key(ctrl_c), Outcome::Quit));
    }

    #[test]
    fn paste_strips_control_chars_and_backspace_works() {
        let mut w = Wizard::new();
        w.handle_paste("https://a.example.com\n");
        assert_eq!(w.fields[0].value, "https://a.example.com");
        w.handle_key(key(KeyCode::Backspace));
        assert_eq!(w.fields[0].value, "https://a.example.co");
    }

    fn screen(w: &Wizard, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|f| w.render(f, '⠋')).unwrap();
        let buffer = terminal.backend().buffer().clone();
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn secrets_are_masked_on_screen() {
        let mut w = filled();
        w.focus = URL;
        let text = screen(&w, 100, 40);
        assert!(text.contains("https://odoo.example.com") && text.contains("Base de datos"));
        assert!(!text.contains("odoo-key"));
        assert!(text.contains("••••••••"));
    }

    #[test]
    fn fits_a_24_row_terminal_with_the_error_visible() {
        let mut w = filled();
        w.set_error("Usuario, clave o base de datos de Odoo inválidos.".into());
        let text = screen(&w, 80, 24);
        assert!(text.contains("API key de Odoo") && text.contains("base de datos de Odoo inv"));
        assert!(text.contains("Esc salir"));
        screen(&w, 10, 4);
    }
}
