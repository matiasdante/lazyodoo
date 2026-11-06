use ratatui::text::Line;
use serde::Deserialize;

use crate::i18n::{Lang, current, tr, tr_in, trf};

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Named {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct Paged<T> {
    pub rows: Vec<T>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct User {
    pub id: i64,
    pub name: String,
    /// Tu empleado (hr), solo en la fila «yo»: permite filtrar horas en Odoo.
    #[serde(default)]
    pub employee_id: Option<i64>,
    /// ¿Este Odoo guarda la hora de inicio de las horas (`date_time`)?
    #[serde(default)]
    pub timesheet_start: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Task {
    pub id: i64,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub description_has_image: bool,
    pub date_deadline: Option<String>,
    #[serde(default)]
    pub priority: String,
    #[serde(default)]
    pub allocated_hours: f64,
    #[serde(default)]
    pub effective_hours: f64,
    #[serde(default)]
    pub remaining_hours: f64,
    /// Incluye las horas de las subtareas.
    #[serde(default)]
    pub total_hours_spent: f64,
    pub project: Option<Named>,
    pub stage: Option<Named>,
    pub partner: Option<Named>,
    #[serde(default)]
    pub users: Vec<Named>,
    #[serde(default)]
    pub tags: Vec<Named>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Timesheet {
    pub id: i64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub date: String,
    #[serde(default)]
    pub unit_amount: f64,
    /// Hora de inicio (UTC, `AAAA-MM-DDTHH:MM:SS`), si este Odoo la guarda.
    #[serde(default)]
    pub date_time: Option<String>,
    pub project: Option<Named>,
    pub task: Option<Named>,
    pub employee: Option<Named>,
}

/// Cómo se muestra un registro: una línea en la lista y un texto de detalle.
/// Todo lo que sale de acá ya está saneado (ver `sanitize`).
pub trait Row {
    fn id(&self) -> i64;
    fn label(&self) -> String;
    fn detail(&self) -> String;
}

/// El texto viene de Odoo (cargado por cualquiera): se descartan los
/// caracteres de control para que no puedan mandar secuencias de escape a
/// la terminal. Solo sobreviven los saltos de línea.
pub fn sanitize(text: &str) -> String {
    text.chars()
        .map(|c| if c == '\t' { ' ' } else { c })
        .filter(|c| *c == '\n' || !c.is_control())
        .collect()
}

/// La hora de inicio llega en UTC; se muestra en hora local.
fn local_start(raw: Option<&str>) -> Option<chrono::DateTime<chrono::Local>> {
    use chrono::{Local, NaiveDateTime, TimeZone};
    let utc = NaiveDateTime::parse_from_str(raw?.get(..19)?, "%Y-%m-%dT%H:%M:%S").ok()?;
    Some(chrono::Utc.from_utc_datetime(&utc).with_timezone(&Local))
}

/// HTML de Odoo a texto plano: suficiente para leer, no para fidelidad.
pub fn html_to_text(html: &str) -> String {
    let mut out = String::new();
    let mut tag = String::new();
    let mut in_tag = false;
    for c in html.chars() {
        match (in_tag, c) {
            (false, '<') => {
                in_tag = true;
                tag.clear();
            }
            (true, '>') => {
                in_tag = false;
                let closing = tag.starts_with('/');
                let name: String = tag
                    .trim_start_matches('/')
                    .chars()
                    .take_while(char::is_ascii_alphanumeric)
                    .collect::<String>()
                    .to_ascii_lowercase();
                match (name.as_str(), closing) {
                    ("br", _) | ("p" | "div" | "tr" | "li" | "h1" | "h2" | "h3", true) => {
                        out.push('\n')
                    }
                    ("li", false) => out.push_str("• "),
                    _ => {}
                }
            }
            (true, _) => tag.push(c),
            (false, _) => out.push(c),
        }
    }
    let decoded = out
        .replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&");
    let mut result = String::new();
    let mut blank = 0;
    for line in decoded.lines().map(str::trim_end) {
        blank = if line.is_empty() { blank + 1 } else { 0 };
        if blank <= 1 {
            result.push_str(line);
            result.push('\n');
        }
    }
    sanitize(result.trim())
}

/// «Etiqueta: valor» por línea, con los valores alineados según el ancho en pantalla
/// de las etiquetas traducidas (el chino ocupa dos columnas por carácter).
fn table(rows: &[(&'static str, String)]) -> String {
    table_in(current(), rows)
}

fn table_in(lang: Lang, rows: &[(&'static str, String)]) -> String {
    let labels: Vec<String> = rows
        .iter()
        .map(|(l, _)| format!("{}:", tr_in(lang, l)))
        .collect();
    let width = |text: &str| Line::from(text).width();
    let longest = labels.iter().map(|l| width(l)).max().unwrap_or(0);
    labels
        .iter()
        .zip(rows)
        .map(|(label, (_, value))| format!("{label}{} {value}", " ".repeat(longest - width(label))))
        .collect::<Vec<_>>()
        .join("\n")
}

fn name(record: &Option<Named>) -> &str {
    record.as_ref().map_or("-", |r| r.name.as_str())
}

fn names(records: &[Named]) -> String {
    if records.is_empty() {
        "-".into()
    } else {
        records
            .iter()
            .map(|r| r.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

fn body(html: &str, has_image: bool) -> String {
    let text = html_to_text(html);
    match (text.is_empty(), has_image) {
        (true, false) => tr("(no description)").into(),
        (true, true) => tr("(image only)").into(),
        (false, true) => format!("{text}\n{}", tr("(the description has images)")),
        (false, false) => text,
    }
}

impl Row for Named {
    fn id(&self) -> i64 {
        self.id
    }
    fn label(&self) -> String {
        sanitize(&self.name)
    }
    fn detail(&self) -> String {
        sanitize(&format!("{}\nid: {}", self.name, self.id))
    }
}

impl Row for Task {
    fn id(&self) -> i64 {
        self.id
    }
    fn label(&self) -> String {
        sanitize(&format!("#{} {}", self.id, self.name))
    }
    fn detail(&self) -> String {
        let hours = trf(
            "{allocated} allocated · {logged} logged · {remaining} remaining",
            &[
                ("allocated", &format!("{:.1}", self.allocated_hours)),
                ("logged", &format!("{:.1}", self.effective_hours)),
                ("remaining", &format!("{:.1}", self.remaining_hours)),
            ],
        );
        let rows = table(&[
            ("Project", name(&self.project).to_string()),
            ("Stage", name(&self.stage).to_string()),
            ("Assignees", names(&self.users)),
            ("Customer", name(&self.partner).to_string()),
            ("Tags", names(&self.tags)),
            (
                "Deadline",
                self.date_deadline.as_deref().unwrap_or("-").to_string(),
            ),
            ("Priority", self.priority.to_string()),
            ("Hours", hours),
        ]);
        sanitize(&format!(
            "#{} {}\n\n{rows}\n\n{}",
            self.id,
            self.name,
            body(&self.description, self.description_has_image),
        ))
    }
}

impl Row for Timesheet {
    fn id(&self) -> i64 {
        self.id
    }
    fn label(&self) -> String {
        let day = self.date.get(5..).unwrap_or(&self.date);
        let target = self.task.as_ref().or(self.project.as_ref());
        // con hora de inicio: «11-05 10:00  6.0h tarea»
        let start = local_start(self.date_time.as_deref())
            .map(|t| format!(" {}", t.format("%H:%M")))
            .unwrap_or_default();
        sanitize(&format!(
            "{day}{start} {:>4.1}h {}",
            self.unit_amount,
            target.map_or("-", |t| t.name.as_str())
        ))
    }
    fn detail(&self) -> String {
        let rows = table(&[
            (
                "Start",
                local_start(self.date_time.as_deref())
                    .map_or("-".to_string(), |t| t.format("%d-%m-%y %H:%M").to_string()),
            ),
            ("Employee", name(&self.employee).to_string()),
            ("Project", name(&self.project).to_string()),
            ("Task", name(&self.task).to_string()),
        ]);
        sanitize(&format!(
            "#{} · {} · {:.2} h\n\n{rows}\n\n{}",
            self.id, self.date, self.unit_amount, self.name,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_becomes_readable_text() {
        let html = "<p>Hola&nbsp;<b>mundo</b></p><ul><li>uno</li><li>a &amp; b</li></ul><br><p></p><p></p><p>fin</p>";
        assert_eq!(html_to_text(html), "Hola mundo\n• uno\n• a & b\n\nfin");
    }

    #[test]
    fn sanitize_removes_terminal_escapes() {
        assert_eq!(sanitize("a\x1b[31mb\x07\tc\nd"), "a[31mb c\nd");
        let task: Task = serde_json::from_value(serde_json::json!({
            "id": 1, "name": "x\x1b]0;pwn\x07y", "date_deadline": null
        }))
        .unwrap();
        assert!(!task.label().contains('\x1b') && !task.detail().contains('\x1b'));
    }

    #[test]
    fn task_parses_api_shape_and_renders() {
        let task: Task = serde_json::from_value(serde_json::json!({
            "id": 7, "name": "Migrar", "description": "<p>Detalle</p>",
            "description_has_image": true, "date_deadline": null, "priority": "1",
            "allocated_hours": 8.0, "effective_hours": 2.5, "remaining_hours": 5.5,
            "total_hours_spent": 2.5,
            "project": {"id": 1, "name": "Proy"}, "stage": {"id": 2, "name": "Nuevo"},
            "partner": null, "users": [{"id": 3, "name": "Yo"}], "tags": []
        }))
        .unwrap();
        let detail = task.detail();
        assert!(detail.contains("Project:   Proy") && detail.contains("Assignees: Yo"));
        assert!(detail.contains("Detalle") && detail.contains("images"));
        assert_eq!(task.label(), "#7 Migrar");
    }

    #[test]
    fn timesheet_label_is_compact() {
        let line: Timesheet = serde_json::from_value(serde_json::json!({
            "id": 1, "name": "x", "date": "2026-11-05", "unit_amount": 2.0,
            "project": {"id": 1, "name": "P"}, "task": null, "employee": null
        }))
        .unwrap();
        assert_eq!(line.label(), "11-05  2.0h P");
    }

    #[test]
    fn timesheet_shows_the_local_start_time_when_it_has_one() {
        let line: Timesheet = serde_json::from_value(serde_json::json!({
            "id": 1, "name": "x", "date": "2026-11-05", "unit_amount": 6.0,
            "date_time": "2026-11-05T13:00:00",
            "project": {"id": 1, "name": "P"}, "task": null, "employee": null
        }))
        .unwrap();
        let label = line.label();
        // «11-05 HH:MM  6.0h P»: la hora depende de la zona local
        assert!(label.starts_with("11-0"), "{label}");
        assert!(
            label.contains(" 6.0h P") && label.matches(':').count() == 1,
            "{label}"
        );
        assert!(line.detail().contains("Start:"));
        let without: Timesheet = serde_json::from_value(serde_json::json!({
            "id": 1, "date": "2026-11-05", "project": null, "task": null, "employee": null
        }))
        .unwrap();
        assert!(without.detail().contains("Start:    -"));
    }

    #[test]
    fn detail_values_line_up_in_every_language() {
        let rows = [
            ("Project", "p".to_string()),
            ("Assignees", "a".to_string()),
            ("Hours", "h".to_string()),
        ];
        for lang in Lang::ALL {
            let text = table_in(lang, &rows);
            let columns: Vec<usize> = text
                .lines()
                .map(|line| {
                    let (label, value) = line.split_once(':').unwrap();
                    Line::from(format!("{label}:{}", &value[..value.len() - 1])).width()
                })
                .collect();
            assert!(columns.windows(2).all(|w| w[0] == w[1]), "{lang:?}: {text}");
            for line in text.lines() {
                let label = line.split_once(':').unwrap().0;
                // styled_detail solo reconoce etiquetas cortas y de puras letras
                assert!(label.chars().count() <= 14 && label.chars().all(char::is_alphabetic));
            }
        }
    }
}
