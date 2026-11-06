use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap,
};

use crate::app::{App, Confirm, ListPanel, Load, Panel};
use crate::i18n::{tr, trf};
use crate::models::{Row, sanitize};
use crate::theme;

pub fn render(frame: &mut Frame, app: &App, spinner: char) {
    let [top, body, bottom] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .areas(frame.area());

    frame.render_widget(Paragraph::new(top_line(app)), top);
    frame.render_widget(Paragraph::new(hint_line(&hints(app))), bottom);

    let left_width = (body.width.saturating_mul(35) / 100)
        .clamp(28, 60)
        .min(body.width);
    let [left, right] =
        Layout::horizontal([Constraint::Length(left_width), Constraint::Min(0)]).areas(body);
    let [p1, p2, p3] = Layout::vertical([
        Constraint::Percentage(25),
        Constraint::Percentage(45),
        Constraint::Percentage(30),
    ])
    .areas(left);

    let applied = app.project.as_ref().map_or(0, |p| p.id);
    let project_labels = app
        .projects
        .visible()
        .iter()
        .map(|p| {
            let mark = if p.id == applied { "● " } else { "  " };
            format!("{mark}{}", p.label())
        })
        .collect();
    let hours_total: f64 = app.hours.items.iter().map(|h| h.unit_amount).sum();

    let focus = app.focus;
    list(
        frame,
        p1,
        &Spec::new(
            &format!("[1] {}", tr("Projects")),
            focus == Panel::Projects,
            spinner,
            app.filtering,
        ),
        &app.projects,
        project_labels,
    );
    list(
        frame,
        p2,
        &Spec::new(
            &format!("[2] {}", tr("Tasks")),
            focus == Panel::Tasks,
            spinner,
            app.filtering,
        ),
        &app.tasks,
        labels(&app.tasks),
    );
    list(
        frame,
        p3,
        &Spec::new(
            &format!("[3] {} · {hours_total:.1}h", tr("Hours this week")),
            focus == Panel::Hours,
            spinner,
            app.filtering,
        ),
        &app.hours,
        labels(&app.hours),
    );

    let detail = Paragraph::new(styled_detail(&app.detail()))
        .wrap(Wrap { trim: false })
        .scroll((app.detail_scroll, 0))
        .block(
            Block::default()
                .title(Line::styled(format!(" {} ", tr("Detail")), theme::dim()))
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(theme::border(false)),
        );
    frame.render_widget(detail, right);

    if let Some(form) = &app.form {
        form.render(frame, spinner);
    }
    if let Some(form) = &app.create {
        form.render(frame, spinner);
    }
    if let Some(confirm) = &app.confirm {
        render_confirm(frame, confirm, spinner);
    }
}

fn labels<T: Row>(panel: &ListPanel<T>) -> Vec<String> {
    panel.visible().iter().map(|i| i.label()).collect()
}

struct Spec {
    title: String,
    focused: bool,
    spinner: char,
    filtering: bool,
}

impl Spec {
    fn new(title: &str, focused: bool, spinner: char, filtering: bool) -> Self {
        Self {
            title: title.to_string(),
            focused,
            spinner,
            filtering: filtering && focused,
        }
    }
}

fn list<T: Row>(
    frame: &mut Frame,
    area: Rect,
    spec: &Spec,
    panel: &ListPanel<T>,
    labels: Vec<String>,
) {
    let status = match &panel.load {
        Load::Loading => format!(" {}", spec.spinner),
        _ => format!(" ({})", labels.len()),
    };
    let title_style = if spec.focused {
        theme::heading()
    } else {
        theme::dim()
    };
    let mut block = Block::default()
        .title(Line::styled(
            format!(" {}{status} ", spec.title),
            title_style,
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::border(spec.focused));
    if spec.filtering || !panel.filter.is_empty() {
        let cursor = if spec.filtering { "▏" } else { "" };
        block = block.title_bottom(Line::styled(
            format!(" /{}{cursor} ", sanitize(&panel.filter)),
            Style::new().fg(theme::accent()),
        ));
    }

    if let Load::Failed(message) = &panel.load {
        let text = Paragraph::new(sanitize(message))
            .style(Style::new().fg(Color::Red))
            .wrap(Wrap { trim: true })
            .block(block);
        frame.render_widget(text, area);
        return;
    }
    if labels.is_empty() {
        let text = if panel.load == Load::Loading {
            tr("loading…")
        } else {
            tr("(empty)")
        };
        frame.render_widget(
            Paragraph::new(Span::styled(text, theme::dim())).block(block),
            area,
        );
        return;
    }

    let highlight = if spec.focused {
        theme::selected()
    } else {
        Style::new().add_modifier(Modifier::BOLD)
    };
    let items: Vec<ListItem> = labels.into_iter().map(ListItem::new).collect();
    let mut state = ListState::default().with_selected(Some(panel.selected));
    frame.render_stateful_widget(
        List::new(items).block(block).highlight_style(highlight),
        area,
        &mut state,
    );
}

/// Detalle con el título resaltado y las etiquetas («Proyecto:», «Estado:»…) en gris.
fn styled_detail(text: &str) -> Vec<Line<'static>> {
    text.lines()
        .enumerate()
        .map(|(i, line)| {
            if i == 0 {
                return Line::styled(line.to_string(), theme::heading());
            }
            match line.split_once(':') {
                Some((label, value))
                    if !label.is_empty()
                        && label.chars().count() <= 14
                        && label.chars().all(char::is_alphabetic) =>
                {
                    Line::from(vec![
                        Span::styled(format!("{label}:"), theme::dim()),
                        Span::raw(value.to_string()),
                    ])
                }
                _ => Line::raw(line.to_string()),
            }
        })
        .collect()
}

fn top_line(app: &App) -> Line<'static> {
    let scope = if app.mine { tr("mine") } else { tr("all") };
    let project = app
        .project
        .as_ref()
        .map_or(tr("all projects"), |p| p.name.as_str());
    let mut spans = vec![
        Span::styled(" lazyodoo ", theme::badge()),
        Span::styled(
            format!(" {} @ {} · ", sanitize(&app.login), sanitize(&app.host)),
            theme::dim(),
        ),
        Span::styled(scope, theme::heading()),
        Span::styled(" (m)", theme::dim()),
        Span::styled(format!(" · {}: ", tr("project")), theme::dim()),
        Span::styled(sanitize(project), Style::new().add_modifier(Modifier::BOLD)),
    ];
    if let Some(notice) = &app.notice {
        spans.push(Span::styled(
            format!("  ▸ {}", sanitize(notice)),
            Style::new().fg(theme::accent()),
        ));
    }
    Line::from(spans)
}

fn hints(app: &App) -> String {
    let text = if app.filtering {
        tr("type to filter · Enter apply · Esc clear")
    } else if app.focus == Panel::Projects {
        tr(
            "1-3 panel · j/k move · Enter apply project · m mine/all · / filter · r refresh · l language · q quit",
        )
    } else if app.focus == Panel::Tasks {
        tr(
            "j/k move · t hours · n new · e edit · c complete · a assign · s stage · p prio · l language · q quit",
        )
    } else {
        tr(
            "1-3 panel · j/k move · J/K detail · m mine/all · / filter · r refresh · l language · q quit",
        )
    };
    format!(" {text}")
}

/// «tecla descripción · tecla descripción»: la tecla en acento y el resto en gris.
fn hint_line(text: &str) -> Line<'static> {
    let mut spans = vec![Span::raw(" ")];
    for (i, item) in text.trim().split(" · ").enumerate() {
        if i > 0 {
            spans.push(Span::styled(" · ", theme::dim()));
        }
        match item.split_once(' ') {
            Some((key, desc)) => {
                spans.push(Span::styled(key.to_string(), theme::heading()));
                spans.push(Span::styled(format!(" {desc}"), theme::dim()));
            }
            None => spans.push(Span::styled(item.to_string(), theme::heading())),
        }
    }
    Line::from(spans)
}

fn render_confirm(frame: &mut Frame, confirm: &Confirm, spinner: char) {
    let area = frame.area();
    let (width, height) = (60.min(area.width), 8.min(area.height));
    let rect = Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    );
    frame.render_widget(Clear, rect);
    let block = Block::default()
        .title(Line::styled(
            format!(" {} ", tr("Complete task")),
            theme::heading(),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::border(true));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    let status = if confirm.sending {
        Line::styled(format!("{spinner} {}", tr("Saving…")), theme::heading())
    } else if let Some(error) = &confirm.error {
        Line::styled(
            sanitize(error),
            Style::new().fg(Color::Red).add_modifier(Modifier::BOLD),
        )
    } else {
        Line::raw("")
    };
    let label = Line::styled(
        sanitize(&confirm.label),
        Style::new().add_modifier(Modifier::BOLD),
    );
    let text = vec![
        if confirm.after_hours {
            Line::styled(tr("Hours logged. Finishing the task…"), theme::heading())
        } else {
            Line::raw(tr("Mark as completed?"))
        },
        label,
        Line::styled(
            trf(
                "Hours logged: {hours} h",
                &[("hours", &format!("{:.1}", confirm.hours))],
            ),
            theme::dim(),
        ),
        status,
        Line::raw(""),
        hint_line(if confirm.after_hours {
            tr("Enter retry · Esc close")
        } else {
            tr("Enter confirm · Esc cancel")
        }),
    ];
    frame.render_widget(Paragraph::new(text).wrap(Wrap { trim: true }), inner);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{Payload, Response, Source};
    use crate::config::Config;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use serde_json::json;

    fn loaded_app() -> App {
        let config =
            Config::new("https://odoo.example.com", "odoo", "me@example.com", "k").unwrap();
        let mut app = App::new(&config, "2026-11-02".into());
        let start = app.start();
        let data = app.on_response(Response {
            source: Source::Users,
            seq: start[0].seq,
            result: Ok(Payload::Users(
                serde_json::from_value(json!([{"id": 9, "name": "Yo", "email": "me@example.com"}]))
                    .unwrap(),
            )),
        });
        app.on_response(Response {
            source: Source::Projects,
            seq: data[2].seq,
            result: Ok(Payload::Projects(
                serde_json::from_value(json!([{"id": 1, "name": "Alfa"}])).unwrap(),
            )),
        });
        app.on_response(Response {
            source: Source::Tasks,
            seq: data[0].seq,
            result: Ok(Payload::Tasks(
                serde_json::from_value(json!([
                    {"id": 5, "name": "Hacer \u{1b}[31mcosa", "date_deadline": null,
                     "effective_hours": 1.5, "description": "<p>Detalle largo</p>", "project": {"id": 1, "name": "Alfa"}, "stage": {"id": 1, "name": "Nuevo"}}
                ]))
                .unwrap(),
            )),
        });
        app.on_response(Response {
            source: Source::Hours,
            seq: data[1].seq,
            result: Err("502 de prueba".into()),
        });
        app
    }

    fn draw(app: &App, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|f| render(f, app, '⠋')).unwrap();
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect()
    }

    #[test]
    fn renders_panels_detail_and_errors() {
        let screen = draw(&loaded_app(), 120, 40);
        for expected in [
            "[1] Projects",
            "[2] Tasks",
            "[3] Hours this week",
            "● All",
            "#5 Hacer ",
            "Detalle largo",
            "502 de prueba",
            "me@example.com @ odoo.example.com · mine",
        ] {
            assert!(screen.contains(expected), "falta {expected:?}");
        }
        assert!(!screen.contains('\u{1b}'));
    }

    #[test]
    fn tiny_terminals_do_not_panic() {
        let app = loaded_app();
        for (w, h) in [(1, 1), (10, 3), (30, 8), (200, 5)] {
            draw(&app, w, h);
        }
    }

    #[test]
    fn renders_hours_form_over_the_panels() {
        let mut app = loaded_app();
        app.on_key(ratatui::crossterm::event::KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('t'),
            ratatui::crossterm::event::KeyModifiers::NONE,
        ));
        let screen = draw(&app, 120, 40);
        assert!(screen.contains("Log hours · #5 Hacer "));
        assert!(screen.contains("0 to 24"));
        draw(&app, 20, 6);
    }

    #[test]
    fn confirm_box_names_the_task_and_shows_errors() {
        let mut app = loaded_app();
        app.on_key(ratatui::crossterm::event::KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('c'),
            ratatui::crossterm::event::KeyModifiers::NONE,
        ));
        let screen = draw(&app, 120, 40);
        assert!(screen.contains("Complete task") && screen.contains("Mark as completed?"));
        assert!(screen.contains("#5 Hacer ") && screen.contains("Enter confirm"));
        app.confirm.as_mut().unwrap().error = Some("Access denied: x".into());
        assert!(draw(&app, 120, 40).contains("Access denied: x"));
        draw(&app, 12, 5);
    }

    #[test]
    fn detail_highlights_title_and_labels_only() {
        let lines = styled_detail(
            "#5 Hacer\n\nProyecto:  Alfa\nuna frase: normal con dos palabras\nTexto sin etiqueta",
        );
        assert_eq!(lines[0].style, theme::heading());
        assert_eq!(lines[2].spans[0].content, "Proyecto:");
        assert_eq!(lines[2].spans[0].style, theme::dim());
        // «una frase» tiene espacio: no es una etiqueta
        assert_eq!(lines[3].spans.len(), 1);
        assert_eq!(lines[4].spans.len(), 1);
    }

    #[test]
    fn hints_split_into_key_and_description() {
        let line = hint_line(" j/k mover · Enter aplicar proyecto · q");
        let keys: Vec<_> = line
            .spans
            .iter()
            .filter(|s| s.style == theme::heading())
            .map(|s| s.content.to_string())
            .collect();
        assert_eq!(keys, ["j/k", "Enter", "q"]);
    }

    #[test]
    fn tasks_panel_advertises_completion() {
        let app = loaded_app();
        assert!(hints(&app).contains("c complete"));
    }

    #[test]
    fn confirm_box_shows_the_loaded_hours() {
        let mut app = loaded_app();
        press_c(&mut app);
        let screen = draw(&app, 120, 40);
        assert!(screen.contains("Hours logged: 1.5 h"), "{screen}");
        assert!(screen.contains("Mark as completed?"));
    }

    fn press_c(app: &mut App) {
        app.on_key(ratatui::crossterm::event::KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('c'),
            ratatui::crossterm::event::KeyModifiers::NONE,
        ));
    }
}
