use chrono::{Local, NaiveDateTime, SecondsFormat, TimeZone};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph, Wrap};
use serde_json::{Value, json};

use crate::i18n::{tr, trf};
use crate::inputs;
use crate::models::{Task, sanitize};
use crate::theme;

const MAX_DESCRIPTION: usize = 256;

// Posición de cada campo en `values`.
const DATE: usize = 0;
const TIME: usize = 1;
const HOURS: usize = 2;
const DESCRIPTION: usize = 3;
const FINISH: usize = 4;

pub enum Outcome {
    None,
    Cancel,
    Submit(Value),
}

/// Formulario «cargar horas» sobre una tarea, a pantalla completa. No habla
/// con la red: arma el body y `App` decide el envío.
pub struct HoursForm {
    pub task_id: i64,
    project_id: i64,
    pub title: String,
    /// Fecha (`DD-MM-AA`), hora de inicio (`HH:MM`), horas y descripción.
    values: [String; 4],
    cursors: [usize; 5],
    /// Ahora (hora local): tope de la fecha y de la hora de inicio.
    now: NaiveDateTime,
    /// ¿Este Odoo guarda la hora de inicio? Si no, el campo no se muestra.
    has_time: bool,
    /// «Finalizar tarea?»: después de cargar las horas, la tarea pasa a Hecha.
    pub finish: bool,
    focus: usize,
    pub error: Option<String>,
    pub sending: bool,
    /// Pedido de alta en vuelo; sirve para ignorar respuestas ajenas.
    pub pending_seq: u64,
}

impl HoursForm {
    /// `None` si la tarea no tiene proyecto (Odoo lo exige).
    pub fn new(task: &Task, now: NaiveDateTime, has_time: bool) -> Option<Self> {
        let date = inputs::mask_show(now.date());
        let time = inputs::time_show(now.time());
        let cursors = [date.chars().count(), time.chars().count(), 0, 0, 0];
        Some(Self {
            task_id: task.id,
            project_id: task.project.as_ref()?.id,
            title: sanitize(&format!("#{} {}", task.id, task.name)),
            values: [date, time, String::new(), String::new()],
            cursors,
            now,
            has_time,
            finish: false,
            focus: HOURS,
            error: None,
            sending: false,
            pending_seq: 0,
        })
    }

    /// Siguiente campo con foco (salteando la hora si este Odoo no la guarda).
    fn move_focus(&mut self, forward: bool) {
        loop {
            self.focus = if forward {
                (self.focus + 1) % 5
            } else {
                (self.focus + 4) % 5
            };
            if self.has_time || self.focus != TIME {
                break;
            }
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Outcome {
        if self.sending {
            // un alta en vuelo no se cancela: el resultado hay que verlo
            return Outcome::None;
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Esc => return Outcome::Cancel,
            KeyCode::Char('s') if ctrl => return self.submit(),
            KeyCode::Tab | KeyCode::Down => self.move_focus(true),
            KeyCode::BackTab | KeyCode::Up => self.move_focus(false),
            KeyCode::Enter if self.focus < FINISH => self.move_focus(true),
            KeyCode::Enter => return self.submit(),
            _ => self.edit_field(&key),
        }
        Outcome::None
    }

    fn edit_field(&mut self, key: &KeyEvent) {
        let i = self.focus;
        if i == FINISH {
            // checkbox: Espacio marca y desmarca
            if key.code == KeyCode::Char(' ') {
                self.finish = !self.finish;
            }
            return;
        }
        let typed = match key.code {
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => Some(c),
            _ => None,
        };
        let erase = matches!(key.code, KeyCode::Backspace | KeyCode::Delete);
        // fecha y hora se escriben con máscara: solo dígitos, separadores automáticos
        let masked = match i {
            DATE => {
                let text = &self.values[DATE];
                typed
                    .map(|c| inputs::mask_type(text, c, Some(self.now.date())))
                    .unwrap_or_else(|| erase.then(|| inputs::mask_backspace(text)))
            }
            TIME => {
                let text = &self.values[TIME];
                typed
                    .map(|c| inputs::time_type(text, c))
                    .unwrap_or_else(|| erase.then(|| inputs::time_backspace(text)))
            }
            _ => None,
        };
        match i {
            DATE | TIME => {
                if let Some(next) = masked {
                    self.cursors[i] = next.chars().count();
                    self.values[i] = next;
                }
            }
            HOURS => {
                if !matches!(typed, Some(c) if !"0123456789.,".contains(c)) {
                    inputs::edit(&mut self.values[i], &mut self.cursors[i], key);
                }
            }
            _ => {
                inputs::edit(&mut self.values[i], &mut self.cursors[i], key);
            }
        }
    }

    pub fn handle_paste(&mut self, text: &str) {
        if self.sending || self.focus == FINISH {
            return;
        }
        let i = self.focus;
        match i {
            DATE | TIME => {
                let pasted = if i == DATE {
                    inputs::mask_paste(text, Some(self.now.date()))
                } else {
                    inputs::time_paste(text)
                };
                if !pasted.is_empty() {
                    self.cursors[i] = pasted.chars().count();
                    self.values[i] = pasted;
                }
            }
            HOURS => {
                let clean: String = text
                    .chars()
                    .filter(|c| "0123456789.,".contains(*c))
                    .collect();
                inputs::insert_str(&mut self.values[i], &mut self.cursors[i], &clean);
            }
            _ => inputs::insert_str(&mut self.values[i], &mut self.cursors[i], text),
        }
    }

    fn submit(&mut self) -> Outcome {
        match self.body() {
            Ok(body) => {
                self.error = None;
                self.sending = true;
                Outcome::Submit(body)
            }
            Err(message) => {
                self.error = Some(message);
                Outcome::None
            }
        }
    }

    /// Mismos límites que `routes`: horas de 0 a 24, descripción de hasta 256,
    /// y ni la fecha ni la hora de inicio son futuras.
    pub fn body(&self) -> Result<Value, String> {
        let date = inputs::mask_parse(&self.values[DATE])
            .ok_or_else(|| tr("Invalid date: use DD-MM-YY (e.g. 05-11-26).").to_string())?;
        if date > self.now.date() {
            return Err(tr("The date cannot be in the future.").into());
        }
        let start = if self.has_time {
            let time = inputs::time_parse(&self.values[TIME])
                .ok_or_else(|| tr("Invalid start time: use HH:MM (e.g. 10:30).").to_string())?;
            let start = date.and_time(time);
            if start > self.now {
                return Err(tr("The start time cannot be in the future.").into());
            }
            Some(start)
        } else {
            None
        };
        let hours: f64 = self.values[HOURS]
            .trim()
            .replace(',', ".")
            .parse()
            .map_err(|_| tr("Invalid hours: e.g. 1.5").to_string())?;
        if !hours.is_finite() || hours <= 0.0 || hours > 24.0 {
            return Err(tr("Hours must be greater than 0 and at most 24.").into());
        }
        let description = self.values[DESCRIPTION].trim();
        if description.is_empty() {
            return Err(tr("Write a description.").into());
        }
        if description.chars().count() > MAX_DESCRIPTION {
            return Err(trf(
                "The description accepts up to {max} characters.",
                &[("max", &MAX_DESCRIPTION.to_string())],
            ));
        }
        let mut body = json!({
            "project_id": self.project_id,
            "task_id": self.task_id,
            "unit_amount": hours,
            "name": description,
            "date": date.format("%Y-%m-%d").to_string(),
        });
        if let Some(start) = start {
            // hora local con su offset: `routes` la guarda en UTC
            let local = Local.from_local_datetime(&start).single().ok_or_else(|| {
                tr("The start time is ambiguous because of a daylight-saving change.").to_string()
            })?;
            body["date_time"] = json!(local.to_rfc3339_opts(SecondsFormat::Secs, false));
        }
        Ok(body)
    }

    pub fn render(&self, frame: &mut Frame, spinner: char) {
        let area = frame.area();
        frame.render_widget(Clear, area);
        let [
            top,
            _,
            labels,
            values,
            hints_row,
            _,
            description,
            finish,
            status,
            hints,
        ] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(4),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .areas(area);

        frame.render_widget(
            Paragraph::new(inputs::top_bar(tr("Log hours ·"), &self.title)),
            top,
        );

        // Fecha · Hora de inicio · Horas, una al lado de la otra
        let fields: Vec<(usize, &str, &str)> = [
            (DATE, "Date", "DD-MM-YY, up to today"),
            (TIME, "Start time", "HH:MM"),
            (HOURS, "Hours", "0 to 24, e.g. 1.5"),
        ]
        .into_iter()
        .filter(|(i, _, _)| self.has_time || *i != TIME)
        .collect();
        let widths: Vec<Constraint> = fields
            .iter()
            .map(|(i, _, _)| Constraint::Length(if *i == DATE { 24 } else { 18 }))
            .chain([Constraint::Min(0)])
            .collect();
        let split = |row: Rect| Layout::horizontal(widths.clone()).split(row);
        let (label_cells, value_cells, hint_cells) =
            (split(labels), split(values), split(hints_row));
        let editing = !self.sending;
        for (n, (i, label, hint)) in fields.iter().enumerate() {
            let focused = self.focus == *i;
            frame.render_widget(
                Paragraph::new(Line::styled(
                    format!(" {}", tr(label)),
                    if focused {
                        theme::heading()
                    } else {
                        theme::dim()
                    },
                )),
                label_cells[n],
            );
            let cell = value_cells[n];
            let value = Rect::new(cell.x + 1, cell.y, cell.width.saturating_sub(2), 1);
            inputs::draw_line(
                frame,
                value,
                &self.values[*i],
                self.cursors[*i],
                focused && editing,
                false,
                "",
                Style::new().add_modifier(Modifier::BOLD),
            );
            frame.render_widget(
                Paragraph::new(Span::styled(format!(" ({})", tr(hint)), theme::dim())),
                hint_cells[n],
            );
        }

        inputs::draw_text_box(
            frame,
            description,
            tr("Description"),
            &self.values[DESCRIPTION],
            self.cursors[DESCRIPTION],
            self.focus == DESCRIPTION,
            self.focus == DESCRIPTION && editing,
            MAX_DESCRIPTION,
        );

        // «Finalizar tarea? [x]»: se marca con Espacio
        let on_finish = self.focus == FINISH;
        let mark = if self.finish { "[x]" } else { "[ ]" };
        let finish_style = if on_finish {
            theme::heading()
        } else {
            theme::dim()
        };
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(if on_finish { "▸ " } else { "  " }, finish_style),
                Span::styled(format!("{} ", tr("Finish task?")), finish_style),
                Span::styled(
                    mark,
                    if self.finish {
                        Style::new()
                            .fg(theme::accent())
                            .add_modifier(Modifier::BOLD)
                    } else {
                        finish_style
                    },
                ),
                Span::styled(
                    format!("  {}", tr("Space toggles: moves it to Done")),
                    theme::dim(),
                ),
            ])),
            finish,
        );

        let line = if self.sending {
            Line::styled(format!(" {spinner} {}", tr("Sending…")), theme::heading())
        } else if let Some(error) = &self.error {
            Line::styled(
                format!(" {}", sanitize(error)),
                Style::new().fg(Color::Red).add_modifier(Modifier::BOLD),
            )
        } else if !self.has_time {
            Line::styled(
                format!(
                    " {}",
                    tr("This Odoo does not store a start time (it has no date_time field).")
                ),
                theme::dim(),
            )
        } else {
            Line::raw("")
        };
        frame.render_widget(Paragraph::new(line).wrap(Wrap { trim: true }), status);
        frame.render_widget(
            Paragraph::new(format!(
                " {}",
                tr("Tab field · Enter next/send · Ctrl+S send · Esc cancel")
            ))
            .style(theme::dim()),
            hints,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::crossterm::event::KeyEventKind;

    /// Jueves 5 de noviembre de 2026, 11:15.
    fn now() -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 11, 5)
            .unwrap()
            .and_hms_opt(11, 15, 0)
            .unwrap()
    }

    fn task(with_project: bool) -> Task {
        serde_json::from_value(json!({
            "id": 11, "name": "Migrar", "date_deadline": null,
            "project": if with_project { json!({"id": 4, "name": "Alfa"}) } else { Value::Null }
        }))
        .unwrap()
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new_with_kind(code, KeyModifiers::NONE, KeyEventKind::Press)
    }

    fn ctrl_s() -> KeyEvent {
        KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL)
    }

    fn type_text(form: &mut HoursForm, text: &str) {
        for c in text.chars() {
            form.handle_key(key(KeyCode::Char(c)));
        }
    }

    fn form_with(has_time: bool) -> HoursForm {
        HoursForm::new(&task(true), now(), has_time).unwrap()
    }

    fn form() -> HoursForm {
        form_with(true)
    }

    fn clear(f: &mut HoursForm, field: usize) {
        f.focus = field;
        for _ in 0..8 {
            f.handle_key(key(KeyCode::Backspace));
        }
    }

    /// Horas y descripción escritas, con el foco al final.
    fn filled(hours: &str, description: &str) -> HoursForm {
        let mut f = form();
        type_text(&mut f, hours);
        f.handle_key(key(KeyCode::Enter));
        type_text(&mut f, description);
        f
    }

    #[test]
    fn needs_a_project() {
        assert!(HoursForm::new(&task(false), now(), true).is_none());
    }

    #[test]
    fn starts_with_now_in_the_mask_formats_and_focus_on_hours() {
        let f = form();
        assert_eq!(f.values[DATE], "05-11-26");
        assert_eq!(f.values[TIME], "11:15");
        assert_eq!(f.focus, HOURS);
    }

    #[test]
    fn submit_sends_the_start_time_in_local_time_with_its_offset() {
        let mut f = filled("1,5", "  reunión  ");
        clear(&mut f, TIME);
        type_text(&mut f, "1000");
        f.focus = DESCRIPTION;
        let Outcome::Submit(body) = f.handle_key(ctrl_s()) else {
            panic!("debía enviar");
        };
        assert_eq!(body["unit_amount"], 1.5);
        assert_eq!(body["name"], "reunión");
        assert_eq!(body["date"], "2026-11-05");
        let start = body["date_time"].as_str().unwrap();
        assert!(start.starts_with("2026-11-05T10:00:00"), "{start}");
        assert!(start.len() > "2026-11-05T10:00:00".len(), "lleva offset");
        assert!(f.sending);
    }

    #[test]
    fn without_start_time_support_the_field_is_skipped_and_not_sent() {
        let mut f = form_with(false);
        f.focus = DATE;
        f.handle_key(key(KeyCode::Tab));
        assert_eq!(f.focus, HOURS, "se saltea la hora");
        type_text(&mut f, "2");
        f.handle_key(key(KeyCode::Enter));
        type_text(&mut f, "reunión");
        let body = f.body().unwrap();
        assert!(body.get("date_time").is_none());
        f.focus = HOURS;
        f.handle_key(key(KeyCode::BackTab));
        assert_eq!(f.focus, DATE);
        assert!(screen(&f, 100, 24).contains("does not store a start time"));
        assert!(!screen(&f, 100, 24).contains("Start time"));
    }

    #[test]
    fn a_task_started_yesterday_keeps_yesterdays_date_and_time() {
        // el caso real: «lo hice ayer a las 10:00»
        let mut f = filled("6", "Cambios en los tableros");
        clear(&mut f, DATE);
        type_text(&mut f, "041126");
        clear(&mut f, TIME);
        type_text(&mut f, "1000");
        let body = f.body().unwrap();
        assert_eq!(body["date"], "2026-11-04");
        assert!(
            body["date_time"]
                .as_str()
                .unwrap()
                .starts_with("2026-11-04T10:00:00")
        );
    }

    #[test]
    fn time_field_only_takes_digits_and_fills_the_colon() {
        let mut f = form();
        clear(&mut f, TIME);
        assert_eq!(f.values[TIME], "");
        type_text(&mut f, "x9a");
        assert_eq!(f.values[TIME], "09:");
        type_text(&mut f, "45");
        assert_eq!(f.values[TIME], "09:45");
        type_text(&mut f, "9");
        assert_eq!(f.values[TIME], "09:45");
        clear(&mut f, TIME);
        type_text(&mut f, "2570"); // 25 y 27 no son horas; entra el 0
        assert_eq!(f.values[TIME], "20:");
        f.handle_paste("8:30");
        assert_eq!(f.values[TIME], "08:30");
    }

    #[test]
    fn a_future_start_time_is_refused_on_send() {
        let mut f = filled("1", "ok");
        clear(&mut f, TIME);
        type_text(&mut f, "1130"); // hoy, pero 11:30 > 11:15
        assert_eq!(
            f.body().unwrap_err(),
            "The start time cannot be in the future."
        );
        clear(&mut f, TIME);
        type_text(&mut f, "1115");
        assert!(f.body().is_ok());
        // ayer, 23:59 está bien
        clear(&mut f, DATE);
        type_text(&mut f, "041126");
        clear(&mut f, TIME);
        type_text(&mut f, "2359");
        assert!(f.body().is_ok());
        clear(&mut f, TIME);
        assert!(f.body().unwrap_err().contains("HH:MM"));
    }

    #[test]
    fn date_field_only_takes_digits_and_fills_in_the_dashes() {
        let mut f = form();
        clear(&mut f, DATE);
        assert_eq!(f.values[DATE], "");
        type_text(&mut f, "x3/");
        assert_eq!(f.values[DATE], "3");
        type_text(&mut f, "0");
        assert_eq!(f.values[DATE], "30-");
        type_text(&mut f, "10");
        assert_eq!(f.values[DATE], "30-10-");
        type_text(&mut f, "25");
        assert_eq!(f.values[DATE], "30-10-25");
        // ya está completa: no entran más dígitos
        type_text(&mut f, "9");
        assert_eq!(f.values[DATE], "30-10-25");
        assert_eq!(f.body().unwrap_err(), "Invalid hours: e.g. 1.5");
    }

    #[test]
    fn future_and_impossible_dates_cannot_be_typed() {
        let mut f = form();
        clear(&mut f, DATE);
        type_text(&mut f, "061126"); // mañana
        assert_eq!(f.values[DATE], "06-11-2");
        clear(&mut f, DATE);
        type_text(&mut f, "3102"); // 31 de febrero
        assert_eq!(f.values[DATE], "31-0");
        type_text(&mut f, "1");
        type_text(&mut f, "3"); // mes 13
        assert_eq!(f.values[DATE], "31-01-");
    }

    #[test]
    fn pasting_a_date_normalizes_it() {
        let mut f = form();
        f.focus = DATE;
        f.handle_paste("03/11/2026");
        assert_eq!(f.values[DATE], "03-11-26");
        f.handle_paste("20/11/2026"); // futura: se ignora lo que no entra
        assert_eq!(f.values[DATE], "20-11-2");
    }

    #[test]
    fn a_stale_date_value_is_still_refused_when_sending() {
        let mut f = filled("2", "ok");
        f.values[DATE] = "06-11-26".into();
        assert_eq!(f.body().unwrap_err(), "The date cannot be in the future.");
        f.values[DATE] = "31-02-26".into();
        assert!(f.body().unwrap_err().contains("Invalid date"));
        f.values[DATE] = "05-11-".into();
        assert!(f.body().unwrap_err().contains("DD-MM-YY"));
    }

    #[test]
    fn hours_field_only_takes_numbers() {
        let mut f = form();
        type_text(&mut f, "1a,5b");
        assert_eq!(f.values[HOURS], "1,5");
    }

    #[test]
    fn rejects_invalid_values_without_sending() {
        for (hours, description) in [
            ("0", "x"),
            ("24.5", "x"),
            ("2", "  "),
            ("", "x"),
            (".", "x"),
        ] {
            let mut f = filled(hours, description);
            assert!(
                matches!(f.handle_key(ctrl_s()), Outcome::None),
                "{hours}/{description}"
            );
            assert!(f.error.is_some() && !f.sending);
        }
        let mut f = filled("2", "ok");
        f.values[DESCRIPTION] = "x".repeat(257);
        assert!(f.body().is_err());
        f.values[DESCRIPTION] = "x".repeat(256);
        assert!(f.body().is_ok());
        assert!(filled("24", "jornada").body().is_ok());
    }

    #[test]
    fn long_descriptions_are_edited_in_the_middle_with_the_cursor() {
        let mut f = form();
        f.focus = DESCRIPTION;
        type_text(&mut f, "Realicé los tableros de NOC y de IT");
        for _ in 0..5 {
            f.handle_key(key(KeyCode::Left));
        }
        type_text(&mut f, "XX");
        assert_eq!(
            f.values[DESCRIPTION],
            "Realicé los tableros de NOC y XXde IT"
        );
        f.handle_key(key(KeyCode::Home));
        f.handle_key(key(KeyCode::Delete));
        assert!(f.values[DESCRIPTION].starts_with("ealicé"));
        f.handle_key(key(KeyCode::End));
        assert_eq!(
            f.cursors[DESCRIPTION],
            f.values[DESCRIPTION].chars().count()
        );
    }

    #[test]
    fn keys_are_ignored_while_sending_and_esc_cancels_otherwise() {
        let mut f = filled("2", "ok");
        f.handle_key(ctrl_s());
        assert!(matches!(f.handle_key(key(KeyCode::Esc)), Outcome::None));
        assert!(matches!(f.handle_key(key(KeyCode::Enter)), Outcome::None));
        f.sending = false;
        assert!(matches!(f.handle_key(key(KeyCode::Esc)), Outcome::Cancel));
    }

    #[test]
    fn ctrl_s_sends_from_any_field() {
        let mut f = filled("2", "ok");
        f.focus = DATE;
        let ctrl_s = KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL);
        assert!(matches!(f.handle_key(ctrl_s), Outcome::Submit(_)));
    }

    fn screen(f: &HoursForm, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| f.render(frame, '⠋')).unwrap();
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect()
    }

    #[test]
    fn full_screen_form_shows_date_time_hours_and_the_whole_wrapped_description() {
        let mut f = form();
        f.focus = DESCRIPTION;
        let text = "Realicé las modificaciones en los tableros de NOC y de IT de Grafana: \
                    unifiqué los paneles de disponibilidad y revisé los umbrales";
        f.handle_paste(text);
        let shown = screen(&f, 80, 24);
        assert!(shown.contains("Log hours") && shown.contains("#11 Migrar"));
        assert!(shown.contains("Date") && shown.contains("05-11-26"));
        assert!(shown.contains("Start time") && shown.contains("11:15"));
        assert!(shown.contains("Hours") && shown.contains("HH:MM"));
        assert!(shown.contains("umbrales"), "{shown}");
        assert!(shown.contains("/256"));
        screen(&f, 20, 8);
    }

    #[test]
    fn finish_checkbox_is_off_by_default_and_only_space_toggles_it() {
        let mut f = filled("2", "ok");
        assert!(!f.finish);
        // Enter en la descripción pasa al checkbox (no envía)
        assert!(matches!(f.handle_key(key(KeyCode::Enter)), Outcome::None));
        assert_eq!(f.focus, FINISH);
        type_text(&mut f, "xq1"); // otras teclas no hacen nada
        assert!(!f.finish);
        f.handle_key(key(KeyCode::Char(' ')));
        assert!(f.finish);
        f.handle_key(key(KeyCode::Char(' ')));
        assert!(!f.finish);
        // y el pegado no lo toca
        f.handle_paste("texto");
        assert_eq!(f.values[DESCRIPTION], "ok");
    }

    #[test]
    fn enter_on_the_checkbox_sends_and_the_flag_survives_the_send() {
        let mut f = filled("2", "ok");
        f.handle_key(key(KeyCode::Enter));
        f.handle_key(key(KeyCode::Char(' ')));
        assert!(matches!(
            f.handle_key(key(KeyCode::Enter)),
            Outcome::Submit(_)
        ));
        assert!(f.finish && f.sending);
    }

    #[test]
    fn the_space_bar_in_other_fields_is_still_text() {
        let mut f = form();
        f.focus = DESCRIPTION;
        type_text(&mut f, "una frase");
        assert_eq!(f.values[DESCRIPTION], "una frase");
        assert!(!f.finish);
    }

    #[test]
    fn the_checkbox_is_drawn_with_its_state_and_meaning() {
        let mut f = form();
        let shown = screen(&f, 100, 24);
        assert!(shown.contains("Finish task?") && shown.contains("[ ]"));
        assert!(shown.contains("moves it to Done"));
        f.finish = true;
        assert!(screen(&f, 100, 24).contains("[x]"));
        // en una terminal de 80 × 24 entra todo
        let shown = screen(&f, 80, 24);
        assert!(shown.contains("Finish task?") && shown.contains("Esc cancel"));
    }
}
