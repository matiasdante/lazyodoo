use std::collections::HashMap;

use chrono::{Local, NaiveDate, NaiveDateTime, SecondsFormat, TimeZone, Utc};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap,
};
use serde_json::{Map, Value, json};

use crate::inputs;
use crate::models::{Named, Task, sanitize};
use crate::theme;

/// Catálogos de Odoo que alimentan los pickers.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Cat {
    Projects,
    Users,
    TaskStages,
    TaskTags,
}

impl Cat {
    pub fn path(self) -> &'static str {
        match self {
            Cat::Projects => "catalog/projects",
            Cat::Users => "catalog/users",
            Cat::TaskStages => "catalog/task-stages",
            Cat::TaskTags => "catalog/task-tags",
        }
    }
}

/// Un catálogo y su alcance: los estados dependen del proyecto, así que
/// `(TaskStages, Some(proyecto))` es otra lista que `(TaskStages, None)`.
pub type CatKey = (Cat, Option<i64>);
pub type Catalogs = HashMap<CatKey, Vec<Named>>;

type Options = &'static [(i64, &'static str)];

const TASK_PRIORITY: Options = &[(0, "Normal"), (1, "Alta")];
pub const NO_CHANGES: &str = "No hay cambios para guardar.";

/// Ruta de alta y modificación de tareas.
pub const PATH: &str = "project-task";

#[derive(Clone, Copy)]
enum Kind {
    Text {
        max: usize,
    },
    /// Fecha opcional; se envía como fin del día en hora local.
    Deadline,
    Hours,
    Pick(Cat),
    PickMany(Cat),
    /// Opciones fijas (prioridad); `routes` las espera como string.
    Choice(Options),
}

struct Def {
    key: &'static str,
    label: &'static str,
    kind: Kind,
    required: bool,
}

const fn def(key: &'static str, label: &'static str, kind: Kind, required: bool) -> Def {
    Def {
        key,
        label,
        kind,
        required,
    }
}

#[derive(Clone)]
enum Val {
    Text(String),
    Picks(Vec<Named>),
}

struct Picker {
    field: usize,
    key: Option<CatKey>,
    multi: bool,
    query: String,
    rows: Vec<Named>,
    cursor: usize,
    loading: bool,
    error: Option<String>,
}

impl Picker {
    fn visible(&self) -> Vec<&Named> {
        let needle = self.query.to_lowercase();
        self.rows
            .iter()
            .filter(|r| r.name.to_lowercase().contains(&needle))
            .collect()
    }
}

pub enum Event {
    None,
    Cancel,
    Submit(Value),
    LoadCatalog(CatKey),
}

pub struct CreateForm {
    defs: Vec<Def>,
    vals: Vec<Val>,
    focus: usize,
    picker: Option<Picker>,
    pub error: Option<String>,
    pub sending: bool,
    /// Alta en vuelo (descarta lo ajeno).
    pub pending_seq: u64,
    /// `Some(id)` = edición de un registro existente (se envía solo lo que cambió).
    pub edit_id: Option<i64>,
    original: Option<Vec<Val>>,
    /// Edición de un solo campo: al cerrar el picker se guarda sin pasar por el formulario.
    quick: bool,
    /// Posición del cursor de cada campo de texto (en caracteres).
    cursors: Vec<usize>,
    /// Proyecto del registro que se edita: acota los estados.
    scope: Option<i64>,
}

impl CreateForm {
    pub fn new(project: Option<Named>, me: Option<Named>) -> Self {
        let project = project.into_iter().collect();
        let me: Vec<Named> = me.into_iter().collect();
        let defs = vec![
            def("name", "Nombre", Kind::Text { max: 256 }, true),
            def("project_id", "Proyecto", Kind::Pick(Cat::Projects), true),
            def("user_ids", "Asignados", Kind::PickMany(Cat::Users), false),
            def("stage_id", "Estado", Kind::Pick(Cat::TaskStages), false),
            def("date_deadline", "Deadline", Kind::Deadline, false),
            def("allocated_hours", "Horas asignadas", Kind::Hours, false),
        ];
        let vals = vec![
            Val::Text(String::new()),
            Val::Picks(project),
            Val::Picks(me),
            Val::Picks(vec![]),
            Val::Text(String::new()),
            Val::Text(String::new()),
        ];
        Self {
            defs,
            vals,
            focus: 0,
            picker: None,
            error: None,
            sending: false,
            pending_seq: 0,
            edit_id: None,
            original: None,
            quick: false,
            cursors: vec![],
            scope: None,
        }
        .with_cursors_at_end()
    }

    /// El cursor de cada campo de texto arranca al final de lo precargado.
    fn with_cursors_at_end(mut self) -> Self {
        self.cursors = self
            .vals
            .iter()
            .map(|v| match v {
                Val::Text(text) => text.chars().count(),
                Val::Picks(_) => 0,
            })
            .collect();
        self
    }

    pub fn edit_task(task: &Task) -> Self {
        let defs = vec![
            def("name", "Nombre", Kind::Text { max: 256 }, true),
            def("user_ids", "Asignados", Kind::PickMany(Cat::Users), false),
            def("stage_id", "Estado", Kind::Pick(Cat::TaskStages), false),
            def("priority", "Prioridad", Kind::Choice(TASK_PRIORITY), false),
            def("date_deadline", "Deadline", Kind::Deadline, false),
            def("allocated_hours", "Horas asignadas", Kind::Hours, false),
            def("tag_ids", "Tags", Kind::PickMany(Cat::TaskTags), false),
        ];
        let vals = vec![
            Val::Text(task.name.clone()),
            Val::Picks(task.users.clone()),
            Val::Picks(task.stage.clone().into_iter().collect()),
            Val::Picks(choice(TASK_PRIORITY, &task.priority)),
            Val::Text(deadline_text(task.date_deadline.as_deref())),
            Val::Text(hours_text(task.allocated_hours)),
            Val::Picks(task.tags.clone()),
        ];
        let mut form = Self::edited(task.id, defs, vals);
        form.scope = task.project.as_ref().map(|p| p.id);
        form
    }

    fn edited(id: i64, defs: Vec<Def>, vals: Vec<Val>) -> Self {
        let mut form = Self::new(None, None);
        form.defs = defs;
        form.original = Some(vals.clone());
        form.vals = vals;
        form.edit_id = Some(id);
        form.with_cursors_at_end()
    }

    /// Edición de un campo (por su clave de API): abre su picker y, al cerrarlo,
    /// guarda si hubo cambios o cancela si no.
    pub fn start_quick(&mut self, key: &str, catalogs: &Catalogs) -> Event {
        let Some(index) = self.defs.iter().position(|d| d.key == key) else {
            return Event::None;
        };
        self.focus = index;
        self.quick = true;
        self.open_picker(catalogs)
    }

    pub fn title(&self) -> String {
        match self.edit_id {
            Some(id) => format!("Editar tarea #{id}"),
            None => "Nueva tarea".into(),
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent, catalogs: &Catalogs) -> Event {
        if self.sending {
            // un alta en vuelo no se cancela: el resultado hay que verlo
            return Event::None;
        }
        if self.picker.is_some() {
            return self.picker_key(key);
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let last = self.defs.len() - 1;
        match key.code {
            KeyCode::Esc => return Event::Cancel,
            KeyCode::Char('s') if ctrl => return self.submit(),
            KeyCode::Tab | KeyCode::Down => self.focus = (self.focus + 1) % self.defs.len(),
            KeyCode::BackTab | KeyCode::Up => {
                self.focus = (self.focus + self.defs.len() - 1) % self.defs.len();
            }
            KeyCode::Enter => match self.defs[self.focus].kind {
                Kind::Pick(_) | Kind::PickMany(_) | Kind::Choice(_) => {
                    return self.open_picker(catalogs);
                }
                _ if self.focus == last => return self.submit(),
                _ => self.focus += 1,
            },
            _ => self.edit_field(&key),
        }
        Event::None
    }

    /// Escritura en el campo con foco: texto con cursor, fecha con máscara
    /// (`DD-MM-AA`), horas solo con números, y borrar la elección de un selector.
    fn edit_field(&mut self, key: &KeyEvent) {
        let i = self.focus;
        let kind = self.defs[i].kind;
        self.cursors.resize(self.vals.len(), 0);
        match &mut self.vals[i] {
            Val::Picks(picks) => {
                if matches!(key.code, KeyCode::Backspace | KeyCode::Delete) {
                    picks.clear();
                }
            }
            Val::Text(text) => match kind {
                Kind::Deadline => {
                    let next = match key.code {
                        KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                            inputs::mask_type(text, c, None)
                        }
                        KeyCode::Backspace | KeyCode::Delete => Some(inputs::mask_backspace(text)),
                        _ => None,
                    };
                    if let Some(next) = next {
                        self.cursors[i] = next.chars().count();
                        *text = next;
                    }
                }
                Kind::Hours => {
                    if !matches!(key.code, KeyCode::Char(c) if !"0123456789.,".contains(c)) {
                        inputs::edit(text, &mut self.cursors[i], key);
                    }
                }
                _ => {
                    inputs::edit(text, &mut self.cursors[i], key);
                }
            },
        }
    }

    pub fn handle_paste(&mut self, text: &str) {
        if self.sending {
            return;
        }
        let clean: String = text.chars().filter(|c| !c.is_control()).collect();
        if let Some(picker) = &mut self.picker {
            picker.query.push_str(&clean);
            picker.cursor = 0;
            return;
        }
        let i = self.focus;
        self.cursors.resize(self.vals.len(), 0);
        let kind = self.defs[i].kind;
        if let Val::Text(value) = &mut self.vals[i] {
            match kind {
                Kind::Deadline => {
                    let pasted = inputs::mask_paste(text, None);
                    if !pasted.is_empty() {
                        self.cursors[i] = pasted.chars().count();
                        *value = pasted;
                    }
                }
                Kind::Hours => {
                    let numeric: String = text
                        .chars()
                        .filter(|c| "0123456789.,".contains(*c))
                        .collect();
                    inputs::insert_str(value, &mut self.cursors[i], &numeric);
                }
                _ => inputs::insert_str(value, &mut self.cursors[i], text),
            }
        }
    }

    fn submit(&mut self) -> Event {
        match self.body() {
            Ok(body) => {
                self.error = None;
                self.sending = true;
                Event::Submit(body)
            }
            Err(message) => {
                self.error = Some(message);
                Event::None
            }
        }
    }

    fn open_picker(&mut self, catalogs: &Catalogs) -> Event {
        let kind = self.defs[self.focus].kind;
        let (key, multi) = match kind {
            Kind::Pick(cat) => (Some((cat, self.scope_for(cat))), false),
            Kind::PickMany(cat) => (Some((cat, None)), true),
            _ => (None, false),
        };
        let mut picker = Picker {
            field: self.focus,
            key,
            multi,
            query: String::new(),
            rows: vec![],
            cursor: 0,
            loading: false,
            error: None,
        };
        if let Kind::Choice(options) = kind {
            picker.rows = options
                .iter()
                .map(|(id, name)| Named {
                    id: *id,
                    name: (*name).to_string(),
                })
                .collect();
        }
        let mut event = Event::None;
        if let Some(key) = key {
            match catalogs.get(&key) {
                Some(rows) => picker.rows = rows.clone(),
                None => {
                    picker.loading = true;
                    event = Event::LoadCatalog(key);
                }
            }
        }
        self.picker = Some(picker);
        event
    }

    /// Id elegido en el campo de esa clave de API (si el formulario lo tiene).
    fn picked(&self, api_key: &str) -> Option<i64> {
        let index = self.defs.iter().position(|d| d.key == api_key)?;
        match &self.vals[index] {
            Val::Picks(picks) => picks.first().map(|p| p.id),
            Val::Text(_) => None,
        }
    }

    /// Los estados se acotan al proyecto de la tarea.
    fn scope_for(&self, cat: Cat) -> Option<i64> {
        match cat {
            Cat::TaskStages => self.picked("project_id").or(self.scope),
            _ => None,
        }
    }

    fn picker_key(&mut self, key: KeyEvent) -> Event {
        let Some(picker) = self.picker.as_mut() else {
            return Event::None;
        };
        let len = picker.visible().len();
        match key.code {
            KeyCode::Esc => self.picker = None,
            KeyCode::Up => picker.cursor = picker.cursor.saturating_sub(1),
            KeyCode::Down => picker.cursor = (picker.cursor + 1).min(len.saturating_sub(1)),
            KeyCode::Backspace => {
                picker.query.pop();
                picker.cursor = 0;
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                picker.query.push(c);
                picker.cursor = 0;
            }
            KeyCode::Enter => {
                let Some(row) = picker.visible().get(picker.cursor).map(|r| (*r).clone()) else {
                    return Event::None;
                };
                let (field, multi) = (picker.field, picker.multi);
                if let Val::Picks(picks) = &mut self.vals[field] {
                    if multi {
                        match picks.iter().position(|p| p.id == row.id) {
                            Some(i) => {
                                picks.remove(i);
                            }
                            None => picks.push(row),
                        }
                    } else {
                        *picks = vec![row];
                    }
                }
                if !multi {
                    self.picker = None;
                }
            }
            _ => {}
        }
        if self.quick && self.picker.is_none() {
            return self.finish_quick();
        }
        Event::None
    }

    fn finish_quick(&mut self) -> Event {
        self.quick = false;
        match self.body() {
            Ok(body) => {
                self.error = None;
                self.sending = true;
                Event::Submit(body)
            }
            Err(message) if message == NO_CHANGES => Event::Cancel,
            // algo no valida: el formulario queda abierto para corregirlo
            Err(message) => {
                self.error = Some(message);
                Event::None
            }
        }
    }

    pub fn catalog_loaded(&mut self, key: CatKey, result: Result<Vec<Named>, String>) {
        let Some(picker) = self.picker.as_mut().filter(|p| p.key == Some(key)) else {
            return;
        };
        picker.loading = false;
        match result {
            Ok(rows) => picker.rows = rows,
            Err(message) => picker.error = Some(message),
        }
    }

    /// Mismos límites que `routes`; lo opcional vacío no se envía. En edición
    /// devuelve solo los campos que cambiaron (`null` = limpiar).
    pub fn body(&self) -> Result<Value, String> {
        let full = collect(&self.defs, &self.vals)?;
        let Some(original) = &self.original else {
            return Ok(Value::Object(full));
        };
        let before = collect(&self.defs, original).unwrap_or_default();
        let mut changes = Map::new();
        for def in &self.defs {
            let (new, old) = (full.get(def.key), before.get(def.key));
            if new != old {
                changes.insert(def.key.into(), new.cloned().unwrap_or(Value::Null));
            }
        }
        if changes.is_empty() {
            Err(NO_CHANGES.into())
        } else {
            Ok(Value::Object(changes))
        }
    }

    pub fn render(&self, frame: &mut Frame, spinner: char) {
        let area = frame.area();
        frame.render_widget(Clear, area);
        let mut constraints = vec![Constraint::Length(1), Constraint::Length(1)];
        constraints.extend(self.defs.iter().map(|_| Constraint::Length(1)));
        constraints.extend([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
            Constraint::Length(1),
        ]);
        let cells = Layout::vertical(constraints).split(area);

        frame.render_widget(Paragraph::new(inputs::top_bar("", &self.title())), cells[0]);

        const LABEL_WIDTH: u16 = 18;
        let editing = !self.sending && self.picker.is_none();
        for (i, (def, val)) in self.defs.iter().zip(&self.vals).enumerate() {
            let focused = i == self.focus;
            let cell = cells[2 + i];
            let star = if def.required { "*" } else { " " };
            let label_style = if focused {
                theme::heading()
            } else {
                theme::dim()
            };
            let marker = if focused { "▸ " } else { "  " };
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled(marker, label_style),
                    Span::styled(format!("{}{star}", def.label), label_style),
                ])),
                cell,
            );
            let x = cell.x + 2 + LABEL_WIDTH;
            let value_area = Rect::new(x, cell.y, cell.width.saturating_sub(x - cell.x + 1), 1);
            let value_style = if focused {
                Style::new().add_modifier(Modifier::BOLD)
            } else {
                Style::new()
            };
            match val {
                Val::Text(text) => {
                    let cursor = self.cursors.get(i).copied().unwrap_or(0);
                    inputs::draw_line(
                        frame,
                        value_area,
                        text,
                        cursor,
                        focused && editing,
                        false,
                        self.hint(def.kind),
                        value_style,
                    );
                }
                Val::Picks(picks) => {
                    // los campos selector llevan ▾: se eligen con Enter
                    let chosen = !picks.is_empty();
                    let shown = if chosen {
                        picks
                            .iter()
                            .map(|p| p.name.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    } else {
                        self.hint(def.kind).to_string()
                    };
                    frame.render_widget(
                        Paragraph::new(Line::from(vec![
                            Span::styled(
                                sanitize(&shown),
                                if chosen { value_style } else { theme::dim() },
                            ),
                            Span::styled("  ▾", Style::new().fg(theme::accent())),
                        ])),
                        value_area,
                    );
                }
            }
        }
        let status = if self.sending {
            Line::styled(format!(" {spinner} Enviando…"), theme::heading())
        } else if let Some(error) = &self.error {
            Line::styled(
                format!(" {}", sanitize(error)),
                Style::new().fg(Color::Red).add_modifier(Modifier::BOLD),
            )
        } else {
            Line::raw("")
        };
        let last = cells.len();
        frame.render_widget(
            Paragraph::new(status).wrap(Wrap { trim: true }),
            cells[last - 2],
        );
        frame.render_widget(
            Paragraph::new(" Tab/↑↓ campo · Enter elegir/seguir · Ctrl+S enviar · Esc cancelar")
                .style(theme::dim()),
            cells[last - 1],
        );

        if let Some(picker) = &self.picker {
            self.render_picker(frame, picker, spinner);
        }
    }

    fn hint(&self, kind: Kind) -> &'static str {
        match kind {
            Kind::Deadline => "DD-MM-AA (opcional)",
            Kind::Hours => "ej: 4 (opcional)",
            Kind::Text { .. } => "",
            Kind::Pick(_) | Kind::PickMany(_) | Kind::Choice(_) => "Enter para elegir",
        }
    }

    fn render_picker(&self, frame: &mut Frame, picker: &Picker, spinner: char) {
        let visible = picker.visible();
        let height = (visible.len() as u16 + 5).clamp(7, 18);
        let area = centered(frame.area(), 56, height);
        frame.render_widget(Clear, area);
        let selected = match &self.vals[picker.field] {
            Val::Picks(picks) => picks.iter().map(|p| p.id).collect::<Vec<_>>(),
            Val::Text(_) => vec![],
        };
        let help = if picker.multi {
            "Enter marca/desmarca · Esc listo"
        } else {
            "Enter elige · Esc cierra"
        };
        let block = Block::default()
            .title(format!(" {} ", self.defs[picker.field].label))
            .title_bottom(Line::styled(
                format!(" {help} "),
                Style::new().fg(theme::gray()),
            ))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::new().fg(theme::primary()));
        let inner = block.inner(area);
        frame.render_widget(block, area);
        let [query_area, list_area] =
            Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(inner);
        frame.render_widget(
            Paragraph::new(format!("/{}▏", sanitize(&picker.query)))
                .style(Style::new().fg(theme::accent())),
            query_area,
        );

        if let Some(error) = &picker.error {
            frame.render_widget(
                Paragraph::new(sanitize(error))
                    .style(Style::new().fg(Color::Red))
                    .wrap(Wrap { trim: true }),
                list_area,
            );
        } else if picker.loading {
            frame.render_widget(
                Paragraph::new(format!("{spinner} cargando…"))
                    .style(Style::new().fg(theme::gray())),
                list_area,
            );
        } else if visible.is_empty() {
            frame.render_widget(
                Paragraph::new("(sin resultados)").style(Style::new().fg(theme::gray())),
                list_area,
            );
        } else {
            let items: Vec<ListItem> = visible
                .iter()
                .map(|row| {
                    let mark = if picker.multi {
                        if selected.contains(&row.id) {
                            "[x] "
                        } else {
                            "[ ] "
                        }
                    } else {
                        ""
                    };
                    ListItem::new(sanitize(&format!("{mark}{}", row.name)))
                })
                .collect();
            let mut state = ListState::default().with_selected(Some(picker.cursor));
            frame.render_stateful_widget(
                List::new(items)
                    .highlight_style(theme::selected())
                    .highlight_symbol("▶ "),
                list_area,
                &mut state,
            );
        }
    }
}

/// Valida y arma los campos presentes (lo opcional vacío se omite).
fn collect(defs: &[Def], vals: &[Val]) -> Result<Map<String, Value>, String> {
    let mut body = Map::new();
    for (def, val) in defs.iter().zip(vals) {
        let missing = || format!("Completá «{}».", def.label);
        match (def.kind, val) {
            (Kind::Text { max }, Val::Text(text)) => {
                let text = text.trim();
                if text.is_empty() {
                    if def.required {
                        return Err(missing());
                    }
                    continue;
                }
                if text.chars().count() > max {
                    return Err(format!("«{}» admite hasta {max} caracteres.", def.label));
                }
                body.insert(def.key.into(), json!(text));
            }
            (Kind::Deadline, Val::Text(text)) => {
                if text.trim().is_empty() {
                    continue;
                }
                body.insert(def.key.into(), json!(end_of_day(text.trim())?));
            }
            (Kind::Hours, Val::Text(text)) => {
                if text.trim().is_empty() {
                    continue;
                }
                let hours: f64 = text
                    .trim()
                    .replace(',', ".")
                    .parse()
                    .map_err(|_| format!("«{}» tiene que ser un número.", def.label))?;
                if !hours.is_finite() || hours < 0.0 {
                    return Err(format!("«{}» no puede ser negativo.", def.label));
                }
                body.insert(def.key.into(), json!(hours));
            }
            (Kind::PickMany(_), Val::Picks(picks)) => {
                if picks.is_empty() {
                    if def.required {
                        return Err(missing());
                    }
                    continue;
                }
                let ids: Vec<i64> = picks.iter().map(|p| p.id).collect();
                body.insert(def.key.into(), json!(ids));
            }
            (Kind::Choice(_), Val::Picks(picks)) => {
                if let Some(pick) = picks.first() {
                    body.insert(def.key.into(), json!(pick.id.to_string()));
                }
            }
            (Kind::Pick(_), Val::Picks(picks)) => match picks.first() {
                Some(pick) => {
                    body.insert(def.key.into(), json!(pick.id));
                }
                None if def.required => return Err(missing()),
                None => {}
            },
            _ => {}
        }
    }
    Ok(body)
}

fn choice(options: Options, raw: &str) -> Vec<Named> {
    raw.parse::<i64>()
        .ok()
        .and_then(|id| options.iter().find(|(i, _)| *i == id))
        .map(|(id, label)| Named {
            id: *id,
            name: (*label).to_string(),
        })
        .into_iter()
        .collect()
}

/// La API entrega el deadline en UTC; se muestra la fecha en hora local.
fn deadline_text(raw: Option<&str>) -> String {
    let Some(raw) = raw else {
        return String::new();
    };
    match raw
        .get(..19)
        .and_then(|s| NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S").ok())
    {
        Some(utc) => Utc
            .from_utc_datetime(&utc)
            .with_timezone(&Local)
            .format("%d-%m-%y")
            .to_string(),
        None => NaiveDate::parse_from_str(raw.get(..10).unwrap_or_default(), "%Y-%m-%d")
            .map(inputs::mask_show)
            .unwrap_or_default(),
    }
}

fn hours_text(hours: f64) -> String {
    if hours == 0.0 {
        String::new()
    } else {
        hours.to_string()
    }
}

/// Fin del día (23:59) en hora local, con offset, para que `routes` lo pase a UTC.
fn end_of_day(text: &str) -> Result<String, String> {
    let date = inputs::mask_parse(text)
        .ok_or_else(|| "Deadline inválido: usá DD-MM-AA (ej: 20-11-26).".to_string())?;
    let naive = date
        .and_hms_opt(23, 59, 0)
        .ok_or_else(|| "Deadline inválido.".to_string())?;
    Local
        .from_local_datetime(&naive)
        .single()
        .map(|d| d.to_rfc3339_opts(SecondsFormat::Secs, false))
        .ok_or_else(|| "Deadline ambiguo por cambio de horario.".to_string())
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::crossterm::event::KeyEventKind;

    fn named(id: i64, name: &str) -> Named {
        Named {
            id,
            name: name.into(),
        }
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new_with_kind(code, KeyModifiers::NONE, KeyEventKind::Press)
    }

    fn type_text(form: &mut CreateForm, text: &str, catalogs: &Catalogs) {
        for c in text.chars() {
            form.handle_key(key(KeyCode::Char(c)), catalogs);
        }
    }

    fn catalogs() -> Catalogs {
        let mut c = Catalogs::new();
        c.insert(
            (Cat::Projects, None),
            vec![named(1, "Alfa"), named(2, "Beta")],
        );
        c.insert(
            (Cat::Users, None),
            vec![named(9, "Yo Mismo"), named(3, "Otro"), named(4, "Tercero")],
        );
        c
    }

    fn task_form() -> CreateForm {
        CreateForm::new(Some(named(1, "Alfa")), Some(named(9, "Yo Mismo")))
    }

    #[test]
    fn task_defaults_and_minimal_body() {
        let mut form = task_form();
        type_text(&mut form, "  Migrar  ", &catalogs());
        assert_eq!(
            form.body().unwrap(),
            json!({"name": "Migrar", "project_id": 1, "user_ids": [9]})
        );
    }

    #[test]
    fn required_fields_are_enforced_before_any_request() {
        let mut form = CreateForm::new(None, None);
        assert!(form.body().unwrap_err().contains("Nombre"));
        type_text(&mut form, "x", &catalogs());
        assert!(form.body().unwrap_err().contains("Proyecto"));
    }

    #[test]
    fn full_task_body_with_deadline_and_hours() {
        let mut form = task_form();
        type_text(&mut form, "Migrar", &catalogs());
        form.vals[4] = Val::Text("20-11-26".into());
        form.vals[5] = Val::Text("4,5".into());
        form.vals[3] = Val::Picks(vec![named(7, "Nuevo")]);
        let body = form.body().unwrap();
        assert_eq!(body["stage_id"], 7);
        assert_eq!(body["allocated_hours"], 4.5);
        let deadline = body["date_deadline"].as_str().unwrap();
        assert!(deadline.starts_with("2026-11-20T23:59:00"), "{deadline}");
        assert!(deadline.len() > "2026-11-20T23:59:00".len(), "lleva offset");
    }

    #[test]
    fn rejects_bad_optional_values() {
        for (field, text) in [
            (4, "20/11/2026"),
            (4, "2026-13-40"),
            (5, "abc"),
            (5, "-1"),
            (5, "inf"),
        ] {
            let mut form = task_form();
            type_text(&mut form, "ok", &catalogs());
            form.vals[field] = Val::Text(text.into());
            assert!(form.body().is_err(), "{field}/{text}");
        }
        let mut form = task_form();
        type_text(&mut form, &"x".repeat(257), &catalogs());
        assert!(form.body().unwrap_err().contains("256"));
    }

    #[test]
    fn enter_on_last_text_field_submits_and_ctrl_s_submits_anywhere() {
        let c = catalogs();
        let mut form = task_form();
        type_text(&mut form, "Migrar", &c);
        form.focus = 5;
        assert!(matches!(
            form.handle_key(key(KeyCode::Enter), &c),
            Event::Submit(_)
        ));
        assert!(form.sending);

        let mut form = task_form();
        type_text(&mut form, "Migrar", &c);
        let ctrl_s = KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL);
        assert!(matches!(form.handle_key(ctrl_s, &c), Event::Submit(_)));
        // enviando: se ignora todo, incluso Esc
        assert!(matches!(
            form.handle_key(key(KeyCode::Esc), &c),
            Event::None
        ));
    }

    #[test]
    fn invalid_submit_shows_error_and_does_not_send() {
        let c = catalogs();
        let mut form = task_form();
        form.focus = 5;
        assert!(matches!(
            form.handle_key(key(KeyCode::Enter), &c),
            Event::None
        ));
        assert!(form.error.is_some() && !form.sending);
    }

    #[test]
    fn single_picker_filters_selects_and_closes() {
        let c = catalogs();
        let mut form = task_form();
        form.focus = 1;
        assert!(matches!(
            form.handle_key(key(KeyCode::Enter), &c),
            Event::None
        ));
        type_text(&mut form, "bet", &c);
        assert_eq!(form.picker.as_ref().unwrap().visible().len(), 1);
        form.handle_key(key(KeyCode::Enter), &c);
        assert!(form.picker.is_none());
        assert_eq!(form.body_project(), Some(2));
    }

    impl CreateForm {
        fn body_project(&self) -> Option<i64> {
            let Val::Picks(p) = &self.vals[1] else {
                return None;
            };
            p.first().map(|n| n.id)
        }
    }

    #[test]
    fn multi_picker_toggles_and_stays_open() {
        let c = catalogs();
        let mut form = task_form();
        form.focus = 2;
        form.handle_key(key(KeyCode::Enter), &c);
        // el cursor está en "Yo Mismo" (ya marcado): lo desmarca
        form.handle_key(key(KeyCode::Enter), &c);
        form.handle_key(key(KeyCode::Down), &c);
        form.handle_key(key(KeyCode::Enter), &c);
        form.handle_key(key(KeyCode::Down), &c);
        form.handle_key(key(KeyCode::Enter), &c);
        assert!(form.picker.is_some());
        form.handle_key(key(KeyCode::Esc), &c);
        assert!(form.picker.is_none());
        let Val::Picks(picks) = &form.vals[2] else {
            panic!()
        };
        assert_eq!(picks.iter().map(|p| p.id).collect::<Vec<_>>(), [3, 4]);
    }

    #[test]
    fn esc_closes_picker_first_then_form() {
        let c = catalogs();
        let mut form = task_form();
        form.focus = 1;
        form.handle_key(key(KeyCode::Enter), &c);
        assert!(matches!(
            form.handle_key(key(KeyCode::Esc), &c),
            Event::None
        ));
        assert!(matches!(
            form.handle_key(key(KeyCode::Esc), &c),
            Event::Cancel
        ));
    }

    #[test]
    fn missing_catalog_is_requested_and_filled_when_it_arrives() {
        let empty = Catalogs::new();
        let mut form = task_form();
        form.focus = 3;
        let event = form.handle_key(key(KeyCode::Enter), &empty);
        assert!(matches!(
            event,
            Event::LoadCatalog((Cat::TaskStages, Some(1)))
        ));
        assert!(form.picker.as_ref().unwrap().loading);
        form.catalog_loaded((Cat::Users, None), Ok(vec![])); // otro catálogo: se ignora
        assert!(form.picker.as_ref().unwrap().loading);
        form.catalog_loaded((Cat::TaskStages, Some(1)), Ok(vec![named(7, "Nuevo")]));
        assert_eq!(form.picker.as_ref().unwrap().visible().len(), 1);
        form.handle_key(key(KeyCode::Enter), &empty);
        assert!(form.body().is_err()); // falta el nombre, pero el estado quedó elegido
        let Val::Picks(p) = &form.vals[3] else {
            panic!()
        };
        assert_eq!(p[0].id, 7);
    }

    #[test]
    fn catalog_error_is_shown_in_picker() {
        let mut form = task_form();
        form.focus = 3;
        form.handle_key(key(KeyCode::Enter), &Catalogs::new());
        form.catalog_loaded((Cat::TaskStages, Some(1)), Err("502".into()));
        let picker = form.picker.as_ref().unwrap();
        assert!(!picker.loading && picker.error.as_deref() == Some("502"));
    }

    #[test]
    fn backspace_clears_a_pick() {
        let c = catalogs();
        let mut form = task_form();
        form.focus = 1;
        form.handle_key(key(KeyCode::Backspace), &c);
        assert!(form.body().is_err());
    }

    #[test]
    fn renders_form_and_picker_without_leaking_escapes() {
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;
        let c = catalogs();
        let mut form = CreateForm::new(Some(named(1, "Al\u{1b}fa")), None);
        let draw = |form: &CreateForm, w, h| {
            let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
            terminal.draw(|f| form.render(f, '⠋')).unwrap();
            terminal
                .backend()
                .buffer()
                .content()
                .iter()
                .map(|c| c.symbol())
                .collect::<String>()
        };
        let screen = draw(&form, 120, 40);
        assert!(screen.contains("Nueva tarea") && screen.contains("Nombre*"));
        assert!(screen.contains("Alfa") && !screen.contains('\u{1b}'));
        form.focus = 2;
        form.handle_key(key(KeyCode::Enter), &c);
        let screen = draw(&form, 120, 40);
        assert!(screen.contains("Asignados") && screen.contains("Tercero"));
        draw(&form, 20, 6);
    }

    fn task() -> Task {
        serde_json::from_value(json!({
            "id": 11, "name": "Migrar", "priority": "1", "allocated_hours": 8.0,
            "date_deadline": "2026-11-20T12:00:00",
            "stage": {"id": 7, "name": "Nuevo"},
            "users": [{"id": 9, "name": "Yo Mismo"}],
            "tags": [{"id": 1, "name": "urgente"}]
        }))
        .unwrap()
    }

    #[test]
    fn edit_without_changes_has_nothing_to_send() {
        let form = CreateForm::edit_task(&task());
        assert_eq!(form.body().unwrap_err(), NO_CHANGES);
        assert_eq!(form.title(), "Editar tarea #11");
        assert_eq!(form.edit_id, Some(11));
    }

    #[test]
    fn edit_sends_only_what_changed() {
        let c = catalogs();
        let mut form = CreateForm::edit_task(&task());
        type_text(&mut form, "!", &c); // nombre: "Migrar!"
        form.vals[5] = Val::Text("10,5".into());
        assert_eq!(
            form.body().unwrap(),
            json!({"name": "Migrar!", "allocated_hours": 10.5})
        );
    }

    #[test]
    fn edit_clears_optional_fields_with_null_and_resends_full_lists() {
        let mut form = CreateForm::edit_task(&task());
        form.vals[4] = Val::Text(String::new()); // deadline
        form.vals[2] = Val::Picks(vec![]); // estado
        form.vals[1] = Val::Picks(vec![named(9, "Yo Mismo"), named(3, "Otro")]);
        assert_eq!(
            form.body().unwrap(),
            json!({"date_deadline": null, "stage_id": null, "user_ids": [9, 3]})
        );
    }

    #[test]
    fn edit_keeps_validating_and_never_blanks_required_fields() {
        let mut form = CreateForm::edit_task(&task());
        form.vals[0] = Val::Text("  ".into());
        assert!(form.body().unwrap_err().contains("Nombre"));
        let mut form = CreateForm::edit_task(&task());
        form.vals[5] = Val::Text("-3".into());
        assert!(form.body().is_err());
    }

    #[test]
    fn unchanged_deadline_is_not_resent_and_shows_local_date() {
        let form = CreateForm::edit_task(&task());
        let Val::Text(text) = &form.vals[4] else {
            panic!()
        };
        // mediodía UTC cae el mismo día o el contiguo según la zona local
        assert!(["20-11-26", "19-11-26", "21-11-26"].contains(&text.as_str()));
        assert!(form.body().is_err());
        assert_eq!(deadline_text(None), "");
        assert_eq!(deadline_text(Some("2026-11-20")), "20-11-26");
    }

    #[test]
    fn quick_choice_saves_as_soon_as_it_is_picked() {
        let c = Catalogs::new();
        let mut form = CreateForm::edit_task(&task());
        assert!(matches!(form.start_quick("priority", &c), Event::None));
        form.handle_key(key(KeyCode::Up), &c); // ya en "Normal" (0)
        let Event::Submit(body) = form.handle_key(key(KeyCode::Enter), &c) else {
            panic!("debía guardar");
        };
        assert_eq!(body, json!({"priority": "0"}));
        assert!(form.sending);
    }

    #[test]
    fn quick_without_a_change_cancels_the_whole_form() {
        let c = catalogs();
        let mut form = CreateForm::edit_task(&task());
        form.start_quick("priority", &c);
        // elegir lo mismo que ya tiene ("Alta") no cambia nada
        form.handle_key(key(KeyCode::Down), &c);
        assert!(matches!(
            form.handle_key(key(KeyCode::Enter), &c),
            Event::Cancel
        ));
        let mut form = CreateForm::edit_task(&task());
        form.start_quick("priority", &c);
        assert!(matches!(
            form.handle_key(key(KeyCode::Esc), &c),
            Event::Cancel
        ));
    }

    #[test]
    fn quick_assignees_save_when_the_picker_is_closed() {
        let c = catalogs();
        let mut form = CreateForm::edit_task(&task());
        form.start_quick("user_ids", &c);
        form.handle_key(key(KeyCode::Down), &c); // Otro
        assert!(matches!(
            form.handle_key(key(KeyCode::Enter), &c),
            Event::None
        ));
        let Event::Submit(body) = form.handle_key(key(KeyCode::Esc), &c) else {
            panic!("debía guardar");
        };
        assert_eq!(body, json!({"user_ids": [9, 3]}));
    }

    #[test]
    fn quick_waits_for_the_catalog_then_saves() {
        let c = Catalogs::new();
        let mut form = CreateForm::edit_task(&task_in_project(1));
        let event = form.start_quick("stage_id", &c);
        assert!(matches!(
            event,
            Event::LoadCatalog((Cat::TaskStages, Some(1)))
        ));
        form.catalog_loaded(
            (Cat::TaskStages, Some(1)),
            Ok(vec![named(1, "Nuevo"), named(2, "Cerrado")]),
        );
        form.handle_key(key(KeyCode::Down), &c);
        let Event::Submit(body) = form.handle_key(key(KeyCode::Enter), &c) else {
            panic!("debía guardar");
        };
        assert_eq!(body, json!({"stage_id": 2}));
    }

    #[test]
    fn unknown_quick_key_is_ignored() {
        let mut form = CreateForm::edit_task(&task());
        assert!(matches!(
            form.start_quick("nope", &Catalogs::new()),
            Event::None
        ));
        assert!(form.picker.is_none());
    }

    // --- pantalla completa, texto largo, fecha con máscara, selectores y estados ---

    fn screen(form: &CreateForm, width: u16, height: u16) -> String {
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|f| form.render(f, '⠋')).unwrap();
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect()
    }

    #[test]
    fn deadline_is_typed_with_the_dd_mm_aa_mask_and_has_no_upper_limit() {
        let c = catalogs();
        let mut form = task_form();
        form.focus = 4;
        type_text(&mut form, "ab5", &c);
        assert_eq!(form.vals[4].text(), "05-");
        type_text(&mut form, "1130", &c);
        assert_eq!(form.vals[4].text(), "05-11-30"); // 2030: sin tope
        type_text(&mut form, "7", &c);
        assert_eq!(form.vals[4].text(), "05-11-30");
        form.handle_key(key(KeyCode::Backspace), &c);
        assert_eq!(form.vals[4].text(), "05-11-3");
        // pegar una fecha completa la normaliza
        form.handle_paste("20/11/2026");
        assert_eq!(form.vals[4].text(), "20-11-26");
        type_text(&mut form, "Nombre", &c);
    }

    impl Val {
        fn text(&self) -> &str {
            match self {
                Val::Text(text) => text,
                Val::Picks(_) => "",
            }
        }
    }

    #[test]
    fn hours_field_takes_only_numbers() {
        let c = catalogs();
        let mut form = task_form();
        form.focus = 5;
        type_text(&mut form, "4x,5y", &c);
        assert_eq!(form.vals[5].text(), "4,5");
        form.handle_paste("1a2");
        assert_eq!(form.vals[5].text(), "4,512");
    }

    #[test]
    fn selector_fields_show_the_arrow_and_the_picker_highlights_the_cursor() {
        let c = catalogs();
        let mut form = task_form();
        let shown = screen(&form, 100, 24);
        assert!(shown.matches('▾').count() >= 3, "{shown}");
        form.focus = 1;
        form.handle_key(key(KeyCode::Enter), &c);
        form.handle_key(key(KeyCode::Down), &c);
        // el cursor del selector se ve: símbolo y fondo violeta sobre «Beta»
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;
        let mut terminal = Terminal::new(TestBackend::new(100, 24)).unwrap();
        terminal.draw(|f| form.render(f, '⠋')).unwrap();
        let buffer = terminal.backend().buffer().clone();
        let mut found = false;
        for y in 0..buffer.area.height {
            let line: String = (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect();
            if let Some(x) = line.find("▶ Beta") {
                let col = line[..x].chars().count() as u16;
                found = buffer[(col + 3, y)].bg == theme::primary();
            }
        }
        assert!(found, "la fila elegida no está resaltada");
    }

    #[test]
    fn new_task_stages_are_scoped_to_the_chosen_project() {
        let empty = Catalogs::new();
        let mut form = task_form(); // proyecto Alfa (1)
        form.focus = 3;
        let event = form.handle_key(key(KeyCode::Enter), &empty);
        assert!(matches!(
            event,
            Event::LoadCatalog((Cat::TaskStages, Some(1)))
        ));
        // cambiar de proyecto cambia el alcance de los estados
        form.handle_key(key(KeyCode::Esc), &empty);
        form.vals[1] = Val::Picks(vec![named(2, "Beta")]);
        let event = form.handle_key(key(KeyCode::Enter), &empty);
        assert!(matches!(
            event,
            Event::LoadCatalog((Cat::TaskStages, Some(2)))
        ));
        // y sin proyecto, todos
        form.handle_key(key(KeyCode::Esc), &empty);
        form.vals[1] = Val::Picks(vec![]);
        let event = form.handle_key(key(KeyCode::Enter), &empty);
        assert!(matches!(event, Event::LoadCatalog((Cat::TaskStages, None))));
    }

    #[test]
    fn editing_a_task_scopes_its_stages_to_the_tasks_project() {
        let mut form = CreateForm::edit_task(&task_in_project(4));
        let event = form.start_quick("stage_id", &Catalogs::new());
        assert!(matches!(
            event,
            Event::LoadCatalog((Cat::TaskStages, Some(4)))
        ));
        // el cache es por alcance: otra lista no se reutiliza
        let mut c = Catalogs::new();
        c.insert((Cat::TaskStages, None), vec![named(1, "de otro proyecto")]);
        let mut form = CreateForm::edit_task(&task_in_project(4));
        assert!(matches!(
            form.start_quick("stage_id", &c),
            Event::LoadCatalog((Cat::TaskStages, Some(4)))
        ));
    }

    fn task_in_project(project: i64) -> Task {
        serde_json::from_value(json!({
            "id": 11, "name": "Migrar", "date_deadline": null,
            "project": {"id": project, "name": "P"}
        }))
        .unwrap()
    }

    #[test]
    fn every_form_fits_a_24_row_terminal() {
        for form in [CreateForm::new(None, None), CreateForm::edit_task(&task())] {
            let shown = screen(&form, 80, 24);
            assert!(
                shown.contains("lazyodoo") && shown.contains("Esc cancelar"),
                "{shown}"
            );
            screen(&form, 30, 8);
        }
    }
}
