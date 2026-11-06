use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::collections::HashSet;

use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::api::{ApiError, Client};
use crate::config::Config;
use crate::create_form::{Cat, CatKey, Catalogs, CreateForm, Event as CreateEvent, PATH};
use crate::hours_form::{HoursForm, Outcome as FormOutcome};
use crate::i18n::{tr, trf};
use crate::models::{Named, Paged, Row, Task, Timesheet, User};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Source {
    Users,
    Projects,
    Tasks,
    Hours,
    CreateHours,
    Catalog(CatKey),
    CreateTask,
    EditTask,
    /// Marcar una tarea como completada (`state = 1_done`).
    CloseTask,
}

/// Un pedido pendiente (GET, o POST si trae `body`). `seq` permite descartar respuestas viejas.
#[derive(Debug, Clone)]
pub struct Request {
    pub source: Source,
    pub seq: u64,
    pub path: String,
    pub query: Vec<(&'static str, String)>,
    pub body: Option<Value>,
}

pub enum Payload {
    Users(Vec<User>),
    Projects(Vec<Named>),
    Tasks(Vec<Task>),
    Hours(Vec<Timesheet>),
    Created(Timesheet),
    Catalog(Vec<Named>),
    /// Tarea recién creada o editada: alcanza con id y nombre.
    Saved(Named),
}

pub struct Response {
    pub source: Source,
    pub seq: u64,
    pub result: Result<Payload, String>,
}

/// Corre el pedido (bloqueante: va en un thread aparte).
pub fn execute(client: &Client, request: Request) -> Response {
    let reply = match (&request.body, request.source) {
        (Some(body), Source::EditTask | Source::CloseTask) => client.put_json(&request.path, body),
        (Some(body), _) => client.post_json(&request.path, body),
        (None, _) => client.get_json(&request.path, &request.query),
    };
    let result = reply
        .and_then(|json| match request.source {
            Source::Users => rows(json).map(Payload::Users),
            Source::Projects => rows(json).map(Payload::Projects),
            Source::Tasks => rows(json).map(Payload::Tasks),
            Source::Hours => rows(json).map(Payload::Hours),
            Source::CreateHours => serde_json::from_value(json)
                .map(Payload::Created)
                .map_err(|e| ApiError::BadResponse(e.to_string())),
            Source::Catalog(_) => rows(json).map(Payload::Catalog),
            Source::CreateTask | Source::EditTask | Source::CloseTask => {
                serde_json::from_value(json)
                    .map(Payload::Saved)
                    .map_err(|e| ApiError::BadResponse(e.to_string()))
            }
        })
        .map_err(|e| describe(&request, &e));
    Response {
        source: request.source,
        seq: request.seq,
        result,
    }
}

/// En una escritura, ciertos fallos no prueban que no se haya guardado.
fn describe(request: &Request, error: &ApiError) -> String {
    let maybe_applied = matches!(
        error,
        ApiError::Timeout(_) | ApiError::Network(_) | ApiError::BadResponse(_)
    ) || matches!(error, ApiError::Server(code, _) if *code >= 500);
    if request.body.is_some() && maybe_applied {
        let panel = match request.source {
            Source::CreateTask | Source::EditTask | Source::CloseTask => tr("Tasks"),
            _ => tr("Hours"),
        };
        trf(
            "{error} It may have been saved: check the {panel} panel before retrying.",
            &[("error", &error.to_string()), ("panel", panel)],
        )
    } else {
        error.to_string()
    }
}

fn rows<T: DeserializeOwned>(json: serde_json::Value) -> Result<Vec<T>, ApiError> {
    serde_json::from_value::<Paged<T>>(json)
        .map(|p| p.rows)
        .map_err(|e| ApiError::BadResponse(e.to_string()))
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Panel {
    Projects,
    Tasks,
    Hours,
}

impl Panel {
    const ALL: [Panel; 3] = [Panel::Projects, Panel::Tasks, Panel::Hours];

    fn index(self) -> usize {
        Self::ALL.iter().position(|p| *p == self).unwrap_or(0)
    }
}

#[derive(Debug, PartialEq)]
pub enum Load {
    Loading,
    Ready,
    Failed(String),
}

pub struct ListPanel<T> {
    pub items: Vec<T>,
    pub load: Load,
    pub selected: usize,
    pub filter: String,
    expected: u64,
    /// Id a re-seleccionar cuando llegue la recarga (se conserva el cursor).
    keep: Option<i64>,
}

impl<T> Default for ListPanel<T> {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            load: Load::Loading,
            selected: 0,
            filter: String::new(),
            expected: 0,
            keep: None,
        }
    }
}

impl<T: Row> ListPanel<T> {
    /// Elementos que pasan el filtro de texto (sobre la etiqueta).
    pub fn visible(&self) -> Vec<&T> {
        let needle = self.filter.to_lowercase();
        self.items
            .iter()
            .filter(|i| needle.is_empty() || i.label().to_lowercase().contains(&needle))
            .collect()
    }

    pub fn selected_item(&self) -> Option<&T> {
        self.visible().get(self.selected).copied()
    }

    fn begin(&mut self, seq: u64) {
        self.expected = seq;
        self.load = Load::Loading;
        self.keep = None;
    }

    /// Como `begin`, pero el cursor sigue sobre el mismo registro al recargar.
    fn begin_keep(&mut self, seq: u64) {
        let id = self.selected_item().map(Row::id);
        self.begin(seq);
        self.keep = id;
    }

    /// `false` si la respuesta es de un pedido que ya quedó viejo.
    fn finish(&mut self, seq: u64, result: Result<Vec<T>, String>) -> bool {
        if seq != self.expected {
            return false;
        }
        let keep = self.keep.take();
        match result {
            Ok(items) => {
                self.items = items;
                self.load = Load::Ready;
                let position = keep.and_then(|id| self.visible().iter().position(|i| i.id() == id));
                if let Some(position) = position {
                    self.selected = position;
                }
            }
            Err(message) => {
                self.items.clear();
                self.load = Load::Failed(message);
            }
        }
        self.select(self.selected as isize);
        true
    }
}

/// Operaciones comunes de los paneles, para no repetir código por tipo.
pub trait PanelOps {
    fn len(&self) -> usize;
    fn selected(&self) -> usize;
    fn select(&mut self, index: isize);
    fn filter_mut(&mut self) -> &mut String;
    fn detail(&self) -> String;
}

impl<T: Row> PanelOps for ListPanel<T> {
    fn len(&self) -> usize {
        self.visible().len()
    }
    fn selected(&self) -> usize {
        self.selected
    }
    fn select(&mut self, index: isize) {
        let len = self.len();
        self.selected = if len == 0 {
            0
        } else {
            index.clamp(0, len as isize - 1) as usize
        };
    }
    fn filter_mut(&mut self) -> &mut String {
        &mut self.filter
    }
    fn detail(&self) -> String {
        self.selected_item().map(Row::detail).unwrap_or_default()
    }
}

/// Qué se le escribe a una tarea para finalizarla: estado Hecha.
fn complete_body() -> Value {
    json!({"state": "1_done"})
}

/// Confirmación para completar una tarea.
pub struct Confirm {
    pub task_id: i64,
    pub label: String,
    /// Horas cargadas en la tarea (y sus subtareas).
    pub hours: f64,
    pub sending: bool,
    pub error: Option<String>,
    pub pending_seq: u64,
    /// Viene de «Finalizar tarea?» al cargar horas: las horas ya quedaron guardadas.
    pub after_hours: bool,
}

#[derive(Default)]
pub struct Step {
    pub requests: Vec<Request>,
    pub quit: bool,
    /// Pidió pasar al idioma siguiente (lo aplica y lo guarda `main`).
    pub cycle_lang: bool,
}

pub struct App {
    pub login: String,
    pub host: String,
    pub me: Option<User>,
    pub mine: bool,
    /// Proyecto aplicado como filtro (`None` = todos).
    pub project: Option<Named>,
    pub projects: ListPanel<Named>,
    pub tasks: ListPanel<Task>,
    pub hours: ListPanel<Timesheet>,
    pub focus: Panel,
    pub filtering: bool,
    pub detail_scroll: u16,
    pub form: Option<HoursForm>,
    pub create: Option<CreateForm>,
    pub confirm: Option<Confirm>,
    catalogs: Catalogs,
    loading_cats: HashSet<CatKey>,
    pub notice: Option<String>,
    week_start: String,
    next_seq: u64,
    users_expected: u64,
}

impl App {
    pub fn new(config: &Config, week_start: String) -> Self {
        let host = config
            .base_url
            .split_once("://")
            .map_or(config.base_url.as_str(), |(_, rest)| rest);
        Self {
            login: config.login.clone(),
            host: host.to_string(),
            me: None,
            mine: false,
            project: None,
            projects: ListPanel::default(),
            tasks: ListPanel::default(),
            hours: ListPanel::default(),
            focus: Panel::Tasks,
            filtering: false,
            detail_scroll: 0,
            form: None,
            create: None,
            confirm: None,
            catalogs: Catalogs::new(),
            loading_cats: HashSet::new(),
            notice: None,
            week_start,
            next_seq: 0,
            users_expected: 0,
        }
    }

    /// Primeros pedidos: quién soy y los proyectos. Los datos filtrados por
    /// usuario salen cuando se sabe quién es.
    pub fn start(&mut self) -> Vec<Request> {
        self.users_expected = self.alloc();
        let users = Request {
            source: Source::Users,
            seq: self.users_expected,
            path: "catalog/me".into(),
            query: vec![],
            body: None,
        };
        // los proyectos dependen de «mías»: se piden cuando se sabe quién sos
        vec![users]
    }

    pub fn panel(&self) -> &dyn PanelOps {
        match self.focus {
            Panel::Projects => &self.projects,
            Panel::Tasks => &self.tasks,
            Panel::Hours => &self.hours,
        }
    }

    fn panel_mut(&mut self) -> &mut dyn PanelOps {
        match self.focus {
            Panel::Projects => &mut self.projects,
            Panel::Tasks => &mut self.tasks,
            Panel::Hours => &mut self.hours,
        }
    }

    pub fn set_notice(&mut self, message: String) {
        self.notice = Some(message);
    }

    pub fn detail(&self) -> String {
        self.panel().detail()
    }

    fn alloc(&mut self) -> u64 {
        self.next_seq += 1;
        self.next_seq
    }

    fn projects_request(&mut self) -> Request {
        let seq = self.alloc();
        self.projects.begin(seq);
        Request {
            source: Source::Projects,
            seq,
            path: if self.mine {
                "catalog/my-projects"
            } else {
                "catalog/projects"
            }
            .into(),
            query: vec![],
            body: None,
        }
    }

    /// Tareas y horas según proyecto aplicado y "mías".
    fn data_requests(&mut self, keep: bool) -> Vec<Request> {
        let user = self.me.as_ref().filter(|_| self.mine).map(|u| u.id);
        let base = |limit: &str, project: &Option<Named>| {
            let mut query = vec![("limit", limit.to_string())];
            if let Some(p) = project {
                query.push(("project_id", p.id.to_string()));
            }
            query
        };
        let mut tasks = base("200", &self.project);
        // solo lo que sigue abierto: al completar algo, la lista baja
        tasks.push(("open", "true".into()));
        if let Some(id) = user {
            tasks.push(("user_id", id.to_string()));
        }
        let mut hours = base("1000", &self.project);
        hours.push(("date_from", self.week_start.clone()));
        // mis horas: por empleado, filtrado en Odoo
        if let Some(employee) = self
            .me
            .as_ref()
            .filter(|_| self.mine)
            .and_then(|u| u.employee_id)
        {
            hours.push(("employee_id", employee.to_string()));
        }

        let (t, h) = (self.alloc(), self.alloc());
        if keep {
            self.tasks.begin_keep(t);
            self.hours.begin_keep(h);
        } else {
            self.tasks.begin(t);
            self.hours.begin(h);
        }
        vec![
            Request {
                source: Source::Tasks,
                seq: t,
                path: "project-task/search".into(),
                query: tasks,
                body: None,
            },
            Request {
                source: Source::Hours,
                seq: h,
                path: "timesheet/search".into(),
                query: hours,
                body: None,
            },
        ]
    }

    pub fn on_response(&mut self, response: Response) -> Vec<Request> {
        let Response {
            source,
            seq,
            result,
        } = response;
        match source {
            Source::Users => {
                if seq != self.users_expected {
                    return vec![];
                }
                self.locate_me(result);
                let mut requests = self.data_requests(false);
                requests.push(self.projects_request());
                return requests;
            }
            Source::Projects => {
                let result = result.and_then(|p| match p {
                    Payload::Projects(rows) => Ok(rows),
                    _ => Err(tr("unexpected response").to_string()),
                });
                let result = result.map(|mut rows| {
                    rows.insert(
                        0,
                        Named {
                            id: 0,
                            name: tr("All").into(),
                        },
                    );
                    rows
                });
                if self.projects.finish(seq, result) {
                    // el cursor arranca sobre el proyecto aplicado
                    let applied = self.project.as_ref().map_or(0, |p| p.id);
                    let index = self.projects.items.iter().position(|n| n.id == applied);
                    self.projects.select(index.unwrap_or(0) as isize);
                }
            }
            Source::Tasks => {
                let result = result.and_then(|p| match p {
                    Payload::Tasks(rows) => Ok(rows),
                    _ => Err(tr("unexpected response").to_string()),
                });
                self.tasks.finish(seq, result);
            }
            Source::Hours => {
                let result = result.and_then(|p| match p {
                    Payload::Hours(rows) => Ok(rows),
                    _ => Err(tr("unexpected response").to_string()),
                });
                self.finish_hours(seq, result);
            }
            Source::CreateHours => return self.finish_create(seq, result),
            Source::Catalog(key) => {
                self.loading_cats.remove(&key);
                let rows = Self::catalog_rows(result);
                if let Ok(rows) = &rows {
                    self.catalogs.insert(key, rows.clone());
                }
                if let Some(form) = &mut self.create {
                    form.catalog_loaded(key, rows);
                }
            }
            Source::CreateTask | Source::EditTask => return self.finish_save(seq, result),
            Source::CloseTask => return self.finish_close(seq, result),
        }
        vec![]
    }

    fn catalog_rows(result: Result<Payload, String>) -> Result<Vec<Named>, String> {
        result.and_then(|p| match p {
            Payload::Catalog(rows) => Ok(rows),
            _ => Err(tr("unexpected response").to_string()),
        })
    }

    fn finish_save(&mut self, seq: u64, result: Result<Payload, String>) -> Vec<Request> {
        let Some(form) = self.create.as_mut().filter(|f| f.pending_seq == seq) else {
            return vec![];
        };
        match result {
            Ok(Payload::Saved(record)) => {
                let what = if form.edit_id.is_some() {
                    tr("Task updated")
                } else {
                    tr("Task created")
                };
                self.notice = Some(format!("{what} #{}: {}", record.id, record.name));
                self.create = None;
                return self.refresh_after_write();
            }
            Ok(_) => form.error = Some(tr("Unexpected response from Odoo.").into()),
            Err(message) => form.error = Some(message),
        }
        form.sending = false;
        vec![]
    }

    fn finish_create(&mut self, seq: u64, result: Result<Payload, String>) -> Vec<Request> {
        let Some(form) = self.form.as_mut().filter(|f| f.pending_seq == seq) else {
            return vec![];
        };
        match result {
            Ok(Payload::Created(line)) => {
                self.notice = Some(trf(
                    "Logged {hours}h on {task}",
                    &[
                        ("hours", &format!("{:.1}", line.unit_amount)),
                        ("task", &form.title),
                    ],
                ));
                let (finish, task_id, label) = (form.finish, form.task_id, form.title.clone());
                self.form = None;
                if finish {
                    // las horas ya están: ahora se finaliza la tarea (Hecha)
                    let seq = self.alloc();
                    self.confirm = Some(Confirm {
                        task_id,
                        label,
                        hours: line.unit_amount,
                        sending: true,
                        error: None,
                        pending_seq: seq,
                        after_hours: true,
                    });
                    return vec![Request {
                        source: Source::CloseTask,
                        seq,
                        path: format!("project-task/{task_id}"),
                        query: vec![],
                        body: Some(complete_body()),
                    }];
                }
                // las horas cargadas cambian también los totales de la tarea
                return self.data_requests(true);
            }
            Ok(_) => form.error = Some(tr("Unexpected response from Odoo.").into()),
            Err(message) => form.error = Some(message),
        }
        form.sending = false;
        vec![]
    }

    /// La API no tiene "mi empleado": con "mías" se filtra por nombre de
    /// empleado igual al de mi usuario.
    fn finish_hours(&mut self, seq: u64, result: Result<Vec<Timesheet>, String>) {
        // con empleado conocido, Odoo ya filtró; si no, se filtra por nombre
        let me = self
            .me
            .as_ref()
            .filter(|u| self.mine && u.employee_id.is_none());
        let result = result.map(|rows| match me {
            Some(me) => {
                let total = rows.len();
                let mine: Vec<_> = rows
                    .into_iter()
                    .filter(|r| {
                        r.employee
                            .as_ref()
                            .is_some_and(|e| e.name.eq_ignore_ascii_case(&me.name))
                    })
                    .collect();
                if mine.is_empty() && total > 0 {
                    self.notice = Some(
                        tr("Hours: no employee has your user's name; press 'm' to see all.").into(),
                    );
                }
                mine
            }
            None => rows,
        });
        self.hours.finish(seq, result);
    }

    /// «Yo» viene del `uid` de la sesión (no de comparar emails).
    fn locate_me(&mut self, result: Result<Payload, String>) {
        self.me = None;
        self.mine = false;
        match result {
            Ok(Payload::Users(users)) => {
                self.me = users.into_iter().next();
                self.mine = self.me.is_some();
                if self.me.is_none() {
                    self.notice =
                        Some(tr("Could not read your user: showing every record.").into());
                }
            }
            Ok(_) => {}
            Err(message) => {
                self.notice = Some(trf(
                    "Could not locate your user: {message}",
                    &[("message", &message)],
                ));
            }
        }
    }

    pub fn on_paste(&mut self, text: &str) {
        if let Some(form) = &mut self.form {
            form.handle_paste(text);
        } else if let Some(form) = &mut self.create {
            form.handle_paste(text);
        } else if self.filtering {
            let clean: String = text.chars().filter(|c| !c.is_control()).collect();
            self.panel_mut().filter_mut().push_str(&clean);
            self.panel_mut().select(0);
        }
    }

    pub fn on_key(&mut self, key: KeyEvent) -> Step {
        let mut step = Step::default();
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            step.quit = true;
            return step;
        }
        if self.confirm.is_some() {
            return self.confirm_key(key);
        }
        if self.form.is_some() {
            return self.form_key(key);
        }
        if self.create.is_some() {
            return self.create_key(key);
        }
        if self.filtering {
            self.filter_key(key.code);
            return step;
        }
        self.notice = None;
        match key.code {
            KeyCode::Char('q') => step.quit = true,
            KeyCode::Char(c @ '1'..='3') => {
                self.set_focus(Panel::ALL[c as usize - '1' as usize]);
            }
            KeyCode::Tab => self.set_focus(Panel::ALL[(self.focus.index() + 1) % 3]),
            KeyCode::BackTab => self.set_focus(Panel::ALL[(self.focus.index() + 2) % 3]),
            KeyCode::Char('j') | KeyCode::Down => self.move_selection(1),
            KeyCode::Char('k') | KeyCode::Up => self.move_selection(-1),
            KeyCode::Char('g') | KeyCode::Home => self.move_selection(isize::MIN / 2),
            KeyCode::Char('G') | KeyCode::End => self.move_selection(isize::MAX / 2),
            KeyCode::Char('J') | KeyCode::PageDown => self.scroll_detail(5),
            KeyCode::Char('K') | KeyCode::PageUp => self.scroll_detail(-5),
            KeyCode::Enter if self.focus == Panel::Projects => {
                step.requests = self.apply_project();
            }
            KeyCode::Char('t') if self.focus == Panel::Tasks => self.open_hours_form(),
            KeyCode::Char('n') if self.focus == Panel::Tasks => self.open_create(),
            KeyCode::Char('c') if self.focus == Panel::Tasks => self.open_confirm(),
            KeyCode::Char('e') if self.editable() => step.requests = self.open_edit(None),
            KeyCode::Char('a') if self.editable() => {
                step.requests = self.open_edit(Some("user_ids"));
            }
            KeyCode::Char('s') if self.editable() => {
                step.requests = self.open_edit(Some("stage_id"))
            }
            KeyCode::Char('p') if self.editable() => {
                step.requests = self.open_edit(Some("priority"))
            }
            KeyCode::Char('m') => step.requests = self.toggle_mine(),
            KeyCode::Char('r') => {
                self.catalogs.clear();
                step.requests = self.data_requests(true);
                step.requests.push(self.projects_request());
            }
            KeyCode::Char('/') => self.filtering = true,
            KeyCode::Char('l') => step.cycle_lang = true,
            KeyCode::Esc => {
                self.panel_mut().filter_mut().clear();
                self.panel_mut().select(0);
            }
            _ => {}
        }
        step
    }

    fn open_hours_form(&mut self) {
        let Some(task) = self.tasks.selected_item() else {
            return;
        };
        // la hora de inicio solo se pide si este Odoo la guarda
        let has_time = self.me.as_ref().is_some_and(|u| u.timesheet_start);
        match HoursForm::new(task, now(), has_time) {
            Some(form) => self.form = Some(form),
            None => {
                self.notice = Some(tr("The task has no project: hours cannot be logged.").into());
            }
        }
    }

    fn open_create(&mut self) {
        let me = self.me.as_ref().map(|u| Named {
            id: u.id,
            name: u.name.clone(),
        });
        self.create = Some(CreateForm::new(self.project.clone(), me));
    }

    fn editable(&self) -> bool {
        self.focus == Panel::Tasks
    }

    /// Edita el registro seleccionado. Con `quick` (clave de API) solo se abre
    /// el picker de ese campo y se guarda al elegir.
    fn open_edit(&mut self, quick: Option<&str>) -> Vec<Request> {
        let mut form = match self.focus {
            Panel::Tasks => match self.tasks.selected_item() {
                Some(task) => CreateForm::edit_task(task),
                None => return vec![],
            },
            _ => return vec![],
        };
        let event = quick.map(|key| form.start_quick(key, &self.catalogs));
        self.create = Some(form);
        match event {
            Some(event) => self.apply_create_event(event),
            None => vec![],
        }
    }

    fn create_key(&mut self, key: KeyEvent) -> Step {
        let mut step = Step::default();
        let Some(form) = self.create.as_mut() else {
            return step;
        };
        let event = form.handle_key(key, &self.catalogs);
        step.requests = self.apply_create_event(event);
        step
    }

    fn apply_create_event(&mut self, event: CreateEvent) -> Vec<Request> {
        let mut requests = vec![];
        match event {
            CreateEvent::None => {}
            CreateEvent::Cancel => self.create = None,
            CreateEvent::Submit(body) => {
                let seq = self.alloc();
                let edit_id = self.create.as_ref().and_then(|f| f.edit_id);
                if let Some(form) = self.create.as_mut() {
                    form.pending_seq = seq;
                }
                let (source, path) = match edit_id {
                    None => (Source::CreateTask, PATH.to_string()),
                    Some(id) => (Source::EditTask, format!("{PATH}/{id}")),
                };
                requests.push(Request {
                    source,
                    seq,
                    path,
                    query: vec![],
                    body: Some(body),
                });
            }
            CreateEvent::LoadCatalog(key) => {
                // si ya hay un pedido en vuelo, su respuesta llena el picker
                if self.loading_cats.insert(key) {
                    let (cat, scope) = key;
                    // los estados se piden solo los del proyecto
                    let query = match (cat, scope) {
                        (Cat::TaskStages, Some(project)) => {
                            vec![("project_id", project.to_string())]
                        }
                        _ => vec![],
                    };
                    requests.push(Request {
                        source: Source::Catalog(key),
                        seq: self.alloc(),
                        path: cat.path().into(),
                        query,
                        body: None,
                    });
                }
            }
        }
        requests
    }

    fn form_key(&mut self, key: KeyEvent) -> Step {
        let mut step = Step::default();
        let Some(form) = self.form.as_mut() else {
            return step;
        };
        match form.handle_key(key) {
            FormOutcome::None => {}
            FormOutcome::Cancel => self.form = None,
            FormOutcome::Submit(body) => {
                let seq = self.alloc();
                if let Some(form) = self.form.as_mut() {
                    form.pending_seq = seq;
                }
                step.requests.push(Request {
                    source: Source::CreateHours,
                    seq,
                    path: "timesheet".into(),
                    query: vec![],
                    body: Some(body),
                });
            }
        }
        step
    }

    fn filter_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Enter => self.filtering = false,
            KeyCode::Esc => {
                self.filtering = false;
                self.panel_mut().filter_mut().clear();
            }
            KeyCode::Backspace => {
                self.panel_mut().filter_mut().pop();
            }
            KeyCode::Char(c) => self.panel_mut().filter_mut().push(c),
            _ => return,
        }
        self.panel_mut().select(0);
        self.detail_scroll = 0;
    }

    fn set_focus(&mut self, panel: Panel) {
        self.focus = panel;
        self.detail_scroll = 0;
    }

    fn move_selection(&mut self, delta: isize) {
        let current = self.panel().selected() as isize;
        self.panel_mut().select(current.saturating_add(delta));
        self.detail_scroll = 0;
    }

    fn scroll_detail(&mut self, delta: i32) {
        let lines = self.detail().lines().count() as i32;
        let next = (self.detail_scroll as i32 + delta).clamp(0, lines.max(1) - 1);
        self.detail_scroll = next as u16;
    }

    fn apply_project(&mut self) -> Vec<Request> {
        let Some(item) = self.projects.selected_item() else {
            return vec![];
        };
        self.project = (item.id != 0).then(|| item.clone());
        self.reset_data_selection();
        self.set_focus(Panel::Tasks);
        self.data_requests(false)
    }

    fn toggle_mine(&mut self) -> Vec<Request> {
        if self.me.is_none() {
            self.notice = Some(tr("Your user was not found: 'mine' is not available.").into());
            return vec![];
        }
        self.mine = !self.mine;
        self.reset_data_selection();
        let mut requests = self.data_requests(false);
        requests.push(self.projects_request());
        requests
    }

    /// Después de crear, editar o completar: recarga todo y la lista de proyectos.
    fn refresh_after_write(&mut self) -> Vec<Request> {
        let mut requests = self.data_requests(true);
        requests.push(self.projects_request());
        requests
    }

    fn open_confirm(&mut self) {
        let Some(task) = self.tasks.selected_item() else {
            return;
        };
        self.confirm = Some(Confirm {
            task_id: task.id,
            label: format!("#{} {}", task.id, task.name),
            hours: task.effective_hours.max(task.total_hours_spent),
            sending: false,
            error: None,
            pending_seq: 0,
            after_hours: false,
        });
    }

    fn confirm_key(&mut self, key: KeyEvent) -> Step {
        let mut step = Step::default();
        let Some(confirm) = self.confirm.as_mut() else {
            return step;
        };
        if confirm.sending {
            // un cambio en vuelo no se cancela: hay que ver el resultado
            return step;
        }
        match key.code {
            KeyCode::Esc | KeyCode::Char('n') => self.confirm = None,
            KeyCode::Enter | KeyCode::Char('y') => {
                let task_id = confirm.task_id;
                confirm.sending = true;
                confirm.error = None;
                let seq = self.alloc();
                if let Some(confirm) = self.confirm.as_mut() {
                    confirm.pending_seq = seq;
                }
                step.requests.push(Request {
                    source: Source::CloseTask,
                    seq,
                    path: format!("project-task/{task_id}"),
                    query: vec![],
                    body: Some(complete_body()),
                });
            }
            _ => {}
        }
        step
    }

    fn finish_close(&mut self, seq: u64, result: Result<Payload, String>) -> Vec<Request> {
        let Some(confirm) = self.confirm.as_mut().filter(|c| c.pending_seq == seq) else {
            return vec![];
        };
        match result {
            Ok(Payload::Saved(record)) => {
                self.notice = Some(format!(
                    "{} #{}: {}",
                    tr("Task completed"),
                    record.id,
                    record.name
                ));
                self.confirm = None;
                // sale de la lista de abiertas: la carga baja
                return self.refresh_after_write();
            }
            Ok(_) => confirm.error = Some(tr("Unexpected response from Odoo.").into()),
            Err(message) => {
                let after_hours = confirm.after_hours;
                confirm.error = Some(if after_hours {
                    trf(
                        "The hours were already logged, but the task could not be finished: {message}",
                        &[("message", &message)],
                    )
                } else {
                    message
                });
                confirm.sending = false;
                // las horas sí se guardaron: que se vean en el panel
                return if after_hours {
                    self.data_requests(true)
                } else {
                    vec![]
                };
            }
        }
        confirm.sending = false;
        vec![]
    }

    fn reset_data_selection(&mut self) {
        self.tasks.selected = 0;
        self.hours.selected = 0;
        self.detail_scroll = 0;
    }
}

/// Ahora, en hora local.
pub fn now() -> chrono::NaiveDateTime {
    chrono::Local::now().naive_local()
}

/// Lunes de la semana actual (hora local), `AAAA-MM-DD`.
pub fn week_start_today() -> String {
    use chrono::{Datelike, Duration, Local};
    let today = Local::now().date_naive();
    let monday = today - Duration::days(today.weekday().num_days_from_monday() as i64);
    monday.format("%Y-%m-%d").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::crossterm::event::KeyEventKind;
    use serde_json::json;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new_with_kind(code, KeyModifiers::NONE, KeyEventKind::Press)
    }

    fn press(app: &mut App, c: char) -> Step {
        app.on_key(key(KeyCode::Char(c)))
    }

    fn app() -> App {
        let config =
            Config::new("https://odoo.example.com", "odoo", "me@example.com", "k").unwrap();
        App::new(&config, "2026-11-02".into())
    }

    fn users(rows: serde_json::Value) -> Payload {
        Payload::Users(serde_json::from_value(rows).unwrap())
    }

    fn query<'a>(req: &'a Request, key: &str) -> Option<&'a str> {
        req.query
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, v)| v.as_str())
    }

    /// App que ya ubicó al usuario y recibió proyectos.
    fn ready() -> (App, Vec<Request>) {
        let mut app = app();
        let start = app.start();
        let user_response = Response {
            source: Source::Users,
            seq: start[0].seq,
            result: Ok(users(json!([
                {"id": 9, "name": "Yo Mismo", "email": "ME@example.com"},
                {"id": 3, "name": "Otro", "email": "otro@example.com"}
            ]))),
        };
        let data = app.on_response(user_response);
        let projects = Response {
            source: Source::Projects,
            seq: data[2].seq,
            result: Ok(Payload::Projects(
                serde_json::from_value(
                    json!([{"id": 1, "name": "Alfa"}, {"id": 2, "name": "Beta"}]),
                )
                .unwrap(),
            )),
        };
        app.on_response(projects);
        (app, data)
    }

    #[test]
    fn start_only_asks_who_you_are() {
        let mut app = app();
        let start = app.start();
        let sources: Vec<_> = start.iter().map(|r| r.source).collect();
        assert_eq!(sources, [Source::Users]);
        assert_eq!(start[0].path, "catalog/me");
    }

    #[test]
    fn finding_me_enables_mine_and_filters_requests() {
        let (app, data) = ready();
        assert!(app.mine && app.me.as_ref().unwrap().id == 9);
        let sources: Vec<_> = data.iter().map(|r| r.source).collect();
        assert_eq!(sources, [Source::Tasks, Source::Hours, Source::Projects]);
        // solo lo asignado a mí y solo lo que sigue abierto
        for request in &data[..1] {
            assert_eq!(query(request, "user_id"), Some("9"));
            assert_eq!(query(request, "open"), Some("true"));
        }
        assert_eq!(query(&data[1], "date_from"), Some("2026-11-02"));
        assert_eq!(query(&data[1], "user_id"), None);
        // y los proyectos son los míos, no todos los de Odoo
        assert_eq!(data[2].path, "catalog/my-projects");
    }

    #[test]
    fn me_comes_from_the_session_not_from_matching_emails() {
        let mut app = app(); // login me@example.com
        let start = app.start();
        let data = app.on_response(Response {
            source: Source::Users,
            seq: start[0].seq,
            result: Ok(users(
                json!([{"id": 42, "name": "Yo", "email": "otra-direccion@example.org"}]),
            )),
        });
        assert!(app.mine);
        assert_eq!(query(&data[0], "user_id"), Some("42"));
    }

    #[test]
    fn hours_use_my_employee_when_known() {
        let mut app = app();
        let start = app.start();
        let data = app.on_response(Response {
            source: Source::Users,
            seq: start[0].seq,
            result: Ok(users(json!([{"id": 9, "name": "Yo", "employee_id": 8}]))),
        });
        assert_eq!(query(&data[1], "employee_id"), Some("8"));
        // Odoo ya filtró: no se vuelve a filtrar por nombre
        let lines = json!([{"id": 1, "date": "2026-11-03", "unit_amount": 2.0, "project": null,
                            "task": null, "employee": {"id": 8, "name": "Otro Nombre"}}]);
        app.on_response(Response {
            source: Source::Hours,
            seq: data[1].seq,
            result: Ok(Payload::Hours(serde_json::from_value(lines).unwrap())),
        });
        assert_eq!(app.hours.items.len(), 1);
    }

    #[test]
    fn empty_user_answer_falls_back_to_everything() {
        let mut app = app();
        let start = app.start();
        let data = app.on_response(Response {
            source: Source::Users,
            seq: start[0].seq,
            result: Ok(users(json!([]))),
        });
        assert!(!app.mine && app.notice.is_some());
        assert_eq!(query(&data[0], "user_id"), None);
        assert_eq!(data[2].path, "catalog/projects");
        assert!(press(&mut app, 'm').requests.is_empty());
        assert!(!app.mine);
    }

    #[test]
    fn users_failure_still_loads_data() {
        let mut app = app();
        let start = app.start();
        let data = app.on_response(Response {
            source: Source::Users,
            seq: start[0].seq,
            result: Err("boom".into()),
        });
        assert_eq!(data.len(), 3);
        assert!(app.notice.as_deref().unwrap().contains("boom"));
    }

    #[test]
    fn project_list_follows_the_mine_toggle() {
        let (mut app, _) = ready();
        let step = press(&mut app, 'm'); // a «todas»
        let projects = step
            .requests
            .iter()
            .find(|r| r.source == Source::Projects)
            .unwrap();
        assert_eq!(projects.path, "catalog/projects");
        assert!(step.requests.iter().all(|r| query(r, "user_id").is_none()));
        let step = press(&mut app, 'm'); // de vuelta a «mías»
        let projects = step
            .requests
            .iter()
            .find(|r| r.source == Source::Projects)
            .unwrap();
        assert_eq!(projects.path, "catalog/my-projects");
        // «todas» sigue mostrando solo lo abierto
        assert!(
            step.requests
                .iter()
                .filter(|r| r.source == Source::Tasks)
                .all(|r| query(r, "open") == Some("true"))
        );
    }

    #[test]
    fn enter_on_project_filters_data_and_focuses_tasks() {
        let (mut app, _) = ready();
        press(&mut app, '1');
        press(&mut app, 'j'); // "Todos" -> Alfa
        let step = app.on_key(key(KeyCode::Enter));
        assert_eq!(app.focus, Panel::Tasks);
        assert_eq!(app.project.as_ref().unwrap().name, "Alfa");
        assert_eq!(step.requests.len(), 2);
        assert!(
            step.requests
                .iter()
                .all(|r| query(r, "project_id") == Some("1"))
        );
        // volver a "Todos" saca el filtro
        press(&mut app, '1');
        press(&mut app, 'g');
        let step = app.on_key(key(KeyCode::Enter));
        assert!(app.project.is_none());
        assert!(
            step.requests
                .iter()
                .all(|r| query(r, "project_id").is_none())
        );
    }

    #[test]
    fn toggling_mine_refetches() {
        let (mut app, _) = ready();
        let step = press(&mut app, 'm');
        assert!(!app.mine);
        assert!(step.requests.iter().all(|r| query(r, "user_id").is_none()));
        let step = press(&mut app, 'm');
        assert!(app.mine);
        assert_eq!(query(&step.requests[0], "user_id"), Some("9"));
    }

    #[test]
    fn stale_responses_are_ignored() {
        let (mut app, first) = ready();
        let second = press(&mut app, 'r').requests;
        let rows = |name: &str| {
            Payload::Tasks(
                serde_json::from_value(json!([{"id": 1, "name": name, "date_deadline": null}]))
                    .unwrap(),
            )
        };
        // llega primero la nueva y después la vieja
        app.on_response(Response {
            source: Source::Tasks,
            seq: second[0].seq,
            result: Ok(rows("nueva")),
        });
        app.on_response(Response {
            source: Source::Tasks,
            seq: first[0].seq,
            result: Ok(rows("vieja")),
        });
        assert_eq!(app.tasks.items[0].name, "nueva");
    }

    #[test]
    fn hours_are_filtered_by_my_name_when_mine() {
        let (mut app, data) = ready();
        let lines = json!([
            {"id": 1, "date": "2026-11-03", "unit_amount": 2.0, "project": null, "task": null,
             "employee": {"id": 1, "name": "yo mismo"}},
            {"id": 2, "date": "2026-11-03", "unit_amount": 4.0, "project": null, "task": null,
             "employee": {"id": 2, "name": "Otro"}}
        ]);
        app.on_response(Response {
            source: Source::Hours,
            seq: data[1].seq,
            result: Ok(Payload::Hours(serde_json::from_value(lines).unwrap())),
        });
        assert_eq!(app.hours.items.len(), 1);
        assert_eq!(app.hours.items[0].id, 1);
    }

    #[test]
    fn navigation_clamps_and_cycles_focus() {
        let (mut app, _) = ready();
        press(&mut app, '1');
        press(&mut app, 'G');
        assert_eq!(app.projects.selected, 2);
        press(&mut app, 'j');
        assert_eq!(app.projects.selected, 2);
        press(&mut app, 'g');
        assert_eq!(app.projects.selected, 0);
        app.on_key(key(KeyCode::BackTab));
        assert_eq!(app.focus, Panel::Hours);
        app.on_key(key(KeyCode::Tab));
        assert_eq!(app.focus, Panel::Projects);
    }

    #[test]
    fn text_filter_narrows_and_esc_clears() {
        let (mut app, _) = ready();
        press(&mut app, '1');
        press(&mut app, '/');
        for c in "bet".chars() {
            press(&mut app, c);
        }
        assert_eq!(app.projects.visible().len(), 1);
        app.on_key(key(KeyCode::Enter));
        assert!(!app.filtering);
        assert_eq!(app.projects.selected_item().unwrap().name, "Beta");
        app.on_key(key(KeyCode::Esc));
        assert_eq!(app.projects.visible().len(), 3);
    }

    #[test]
    fn quit_keys() {
        let (mut app, _) = ready();
        assert!(press(&mut app, 'q').quit);
        let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert!(app.on_key(ctrl_c).quit);
        // 'q' dentro del filtro es texto, no salir
        press(&mut app, '/');
        assert!(!press(&mut app, 'q').quit);
    }

    #[test]
    fn l_asks_to_change_the_language_except_while_typing() {
        let (mut app, _) = ready();
        assert!(press(&mut app, 'l').cycle_lang);
        // en el filtro la «l» es texto
        press(&mut app, '/');
        assert!(!press(&mut app, 'l').cycle_lang);
        assert_eq!(app.tasks.filter, "l");
    }

    #[test]
    fn week_start_is_a_monday() {
        use chrono::{Datelike, NaiveDate};
        let day = NaiveDate::parse_from_str(&week_start_today(), "%Y-%m-%d").unwrap();
        assert_eq!(day.weekday().num_days_from_monday(), 0);
    }

    /// App lista con una tarea cargada y enfocada en Tareas.
    fn with_task(project: bool) -> App {
        let (mut app, data) = ready();
        let task = json!({
            "id": 11, "name": "Migrar", "date_deadline": null,
            "project": if project { json!({"id": 4, "name": "Alfa"}) } else { serde_json::Value::Null }
        });
        app.on_response(Response {
            source: Source::Tasks,
            seq: data[0].seq,
            result: Ok(Payload::Tasks(
                serde_json::from_value(json!([task])).unwrap(),
            )),
        });
        press(&mut app, '2');
        app
    }

    fn type_in(app: &mut App, text: &str) {
        for c in text.chars() {
            press(app, c);
        }
    }

    /// Abre el formulario, completa horas y descripción y envía.
    fn submit_hours(app: &mut App) -> Request {
        press(app, 't');
        type_in(app, "2");
        app.on_key(key(KeyCode::Enter));
        type_in(app, "reunión");
        app.on_key(key(KeyCode::Enter)); // pasa a «Finalizar tarea?»
        let mut step = app.on_key(key(KeyCode::Enter));
        assert_eq!(step.requests.len(), 1);
        step.requests.remove(0)
    }

    fn created(seq: u64) -> Response {
        Response {
            source: Source::CreateHours,
            seq,
            result: Ok(Payload::Created(
                serde_json::from_value(json!({
                    "id": 50, "name": "reunión", "date": "2026-11-05", "unit_amount": 2.0,
                    "project": null, "task": null, "employee": null
                }))
                .unwrap(),
            )),
        }
    }

    #[test]
    fn t_opens_form_and_submit_posts_to_timesheet() {
        let mut app = with_task(true);
        let request = submit_hours(&mut app);
        assert_eq!(request.source, Source::CreateHours);
        assert_eq!(request.path, "timesheet");
        let body = request.body.unwrap();
        assert_eq!(body["task_id"], 11);
        assert_eq!(body["project_id"], 4);
        assert_eq!(body["unit_amount"], 2.0);
        assert_eq!(body["name"], "reunión");
        assert!(app.form.as_ref().unwrap().sending);
    }

    #[test]
    fn success_closes_form_and_refreshes_data() {
        let mut app = with_task(true);
        let request = submit_hours(&mut app);
        let refresh = app.on_response(created(request.seq));
        assert!(app.form.is_none());
        assert!(app.notice.as_deref().unwrap().contains("2.0h"));
        let sources: Vec<_> = refresh.iter().map(|r| r.source).collect();
        assert_eq!(sources, [Source::Tasks, Source::Hours]);
    }

    #[test]
    fn failure_keeps_form_open_with_message_and_allows_retry() {
        let mut app = with_task(true);
        let request = submit_hours(&mut app);
        let refresh = app.on_response(Response {
            source: Source::CreateHours,
            seq: request.seq,
            result: Err("Acceso denegado: Escrituras deshabilitadas.".into()),
        });
        assert!(refresh.is_empty());
        let form = app.form.as_ref().unwrap();
        assert!(!form.sending && form.error.as_deref().unwrap().contains("Escrituras"));
        // reintento explícito
        assert_eq!(app.on_key(key(KeyCode::Enter)).requests.len(), 1);
    }

    #[test]
    fn form_cannot_be_cancelled_while_sending_and_other_responses_are_ignored() {
        let mut app = with_task(true);
        let request = submit_hours(&mut app);
        app.on_key(key(KeyCode::Esc));
        assert!(app.form.is_some());
        assert!(app.on_response(created(request.seq + 100)).is_empty());
        assert!(app.form.is_some());
    }

    #[test]
    fn esc_cancels_idle_form_and_keys_do_not_leak_to_panels() {
        let mut app = with_task(true);
        press(&mut app, 't');
        assert!(!press(&mut app, 'q').quit); // 'q' es texto del formulario
        app.on_key(key(KeyCode::Esc));
        assert!(app.form.is_none());
    }

    #[test]
    fn task_without_project_or_empty_list_does_not_open_form() {
        let mut app = with_task(false);
        press(&mut app, 't');
        assert!(app.form.is_none() && app.notice.is_some());
        let (mut empty, _) = ready();
        press(&mut empty, '2');
        press(&mut empty, 't');
        assert!(empty.form.is_none());
        // 't' solo aplica en Tareas
        let mut other = with_task(true);
        press(&mut other, '3');
        press(&mut other, 't');
        assert!(other.form.is_none());
    }

    #[test]
    fn write_errors_that_may_have_applied_warn_before_retrying() {
        let request = Request {
            source: Source::CreateHours,
            seq: 1,
            path: "timesheet".into(),
            query: vec![],
            body: Some(json!({})),
        };
        assert!(describe(&request, &ApiError::Timeout(15)).contains("check the Hours panel"));
        assert!(describe(&request, &ApiError::Server(502, "x".into())).contains("check the"));
        assert!(!describe(&request, &ApiError::Server(400, "x".into())).contains("check the"));
        assert!(!describe(&request, &ApiError::Forbidden("x".into())).contains("check the"));
        let get = Request {
            body: None,
            ..request
        };
        assert!(!describe(&get, &ApiError::Timeout(15)).contains("check the"));
    }

    /// `with_task` + proyecto "Alfa" aplicado, para que el alta de tarea sea válida.
    fn with_project_applied() -> App {
        let mut app = with_task(true);
        press(&mut app, '1');
        press(&mut app, 'j');
        app.on_key(key(KeyCode::Enter));
        app
    }

    fn catalog_response(key: CatKey, seq: u64, rows: serde_json::Value) -> Response {
        Response {
            source: Source::Catalog(key),
            seq,
            result: Ok(Payload::Catalog(serde_json::from_value(rows).unwrap())),
        }
    }

    fn saved(source: Source, seq: u64, id: i64, name: &str) -> Response {
        Response {
            source,
            seq,
            result: Ok(Payload::Saved(Named {
                id,
                name: name.into(),
            })),
        }
    }

    #[test]
    fn n_opens_task_form_with_applied_project_and_me_and_posts() {
        let mut app = with_task(true);
        press(&mut app, '1');
        press(&mut app, 'j');
        app.on_key(key(KeyCode::Enter)); // aplica "Alfa" y enfoca Tareas
        press(&mut app, 'n');
        assert_eq!(app.create.as_ref().unwrap().title(), "New task");
        type_in(&mut app, "Nueva cosa");
        let step = app.on_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
        let request = &step.requests[0];
        assert_eq!(request.source, Source::CreateTask);
        assert_eq!(request.path, "project-task");
        assert_eq!(
            request.body.as_ref().unwrap(),
            &json!({"name": "Nueva cosa", "project_id": 1, "user_ids": [9]})
        );
    }

    #[test]
    fn n_outside_tasks_does_nothing() {
        let mut app = with_task(true);
        for panel in ['1', '3'] {
            press(&mut app, panel);
            press(&mut app, 'n');
            assert!(app.create.is_none());
        }
    }

    #[test]
    fn invalid_create_never_reaches_the_network() {
        let mut app = with_task(true);
        press(&mut app, 'n');
        let step = app.on_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
        assert!(step.requests.is_empty());
        assert!(app.create.as_ref().unwrap().error.is_some());
    }

    #[test]
    fn create_success_closes_form_and_refreshes() {
        let mut app = with_project_applied();
        press(&mut app, 'n');
        type_in(&mut app, "X");
        let step = app.on_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
        let seq = step.requests[0].seq;
        let refresh = app.on_response(saved(Source::CreateTask, seq, 321, "X"));
        assert!(app.create.is_none());
        assert!(app.notice.as_deref().unwrap().contains("Task created #321"));
        assert_eq!(refresh.len(), 3);
    }

    #[test]
    fn create_failure_keeps_form_for_retry_and_ignores_foreign_responses() {
        let mut app = with_project_applied();
        press(&mut app, 'n');
        type_in(&mut app, "X");
        let step = app.on_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
        let seq = step.requests[0].seq;
        assert!(
            app.on_response(saved(Source::CreateTask, seq + 50, 1, "ajena"))
                .is_empty()
        );
        assert!(app.create.is_some());
        app.on_response(Response {
            source: Source::CreateTask,
            seq,
            result: Err("Acceso denegado: Escrituras deshabilitadas.".into()),
        });
        let form = app.create.as_ref().unwrap();
        assert!(!form.sending && form.error.as_deref().unwrap().contains("Escrituras"));
        let retry = app.on_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
        assert_eq!(retry.requests.len(), 1);
    }

    #[test]
    fn catalogs_are_fetched_once_cached_and_dropped_on_refresh() {
        let mut app = with_task(true);
        press(&mut app, 'n');
        // Estado: 4ª fila (índice 3)
        for _ in 0..3 {
            app.on_key(key(KeyCode::Tab));
        }
        let step = app.on_key(key(KeyCode::Enter));
        assert_eq!(step.requests.len(), 1);
        assert_eq!(
            step.requests[0].source,
            Source::Catalog((Cat::TaskStages, None))
        );
        assert_eq!(step.requests[0].path, "catalog/task-stages");
        // cerrar y reabrir mientras viaja: no duplica el pedido
        app.on_key(key(KeyCode::Esc));
        assert!(app.on_key(key(KeyCode::Enter)).requests.is_empty());
        app.on_response(catalog_response(
            (Cat::TaskStages, None),
            step.requests[0].seq,
            json!([{"id": 7, "name": "Nuevo"}]),
        ));
        // elige el estado
        app.on_key(key(KeyCode::Enter));
        // ya en caché: reabrir no pide nada
        assert!(app.on_key(key(KeyCode::Enter)).requests.is_empty());
        app.on_key(key(KeyCode::Esc));
        app.on_key(key(KeyCode::Esc));
        // refrescar descarta la caché
        press(&mut app, 'r');
        press(&mut app, 'n');
        for _ in 0..3 {
            app.on_key(key(KeyCode::Tab));
        }
        assert_eq!(app.on_key(key(KeyCode::Enter)).requests.len(), 1);
    }

    #[test]
    fn ambiguous_write_errors_name_the_right_panel() {
        let request = |source| Request {
            source,
            seq: 1,
            path: "x".into(),
            query: vec![],
            body: Some(json!({})),
        };
        let err = ApiError::Timeout(15);
        assert!(describe(&request(Source::CreateTask), &err).contains("Tasks panel"));
        assert!(describe(&request(Source::EditTask), &err).contains("Tasks panel"));
        assert!(describe(&request(Source::CreateHours), &err).contains("Hours panel"));
    }

    /// App con estas tareas cargadas (enfocada en Tareas) y mías activo.
    fn app_with_tasks(rows: serde_json::Value) -> App {
        let (mut app, data) = ready();
        app.on_response(Response {
            source: Source::Tasks,
            seq: data[0].seq,
            result: Ok(Payload::Tasks(serde_json::from_value(rows).unwrap())),
        });
        press(&mut app, '2');
        app
    }

    fn two_tasks() -> serde_json::Value {
        json!([
            {"id": 11, "name": "Primera", "priority": "1", "date_deadline": null, "effective_hours": 2.0,
             "project": {"id": 4, "name": "Alfa"}, "users": [{"id": 9, "name": "Yo Mismo"}]},
            {"id": 12, "name": "Segunda", "priority": "0", "date_deadline": null, "effective_hours": 1.0,
             "project": {"id": 4, "name": "Alfa"}, "users": []}
        ])
    }

    fn ctrl_s() -> KeyEvent {
        KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL)
    }

    #[test]
    fn e_opens_edit_form_only_on_tasks_with_a_selection() {
        let mut app = app_with_tasks(two_tasks());
        press(&mut app, 'e');
        assert_eq!(app.create.as_ref().unwrap().edit_id, Some(11));
        app.on_key(key(KeyCode::Esc));
        press(&mut app, '1');
        press(&mut app, 'e');
        assert!(app.create.is_none());
        press(&mut app, '3'); // horas
        press(&mut app, 'e');
        assert!(app.create.is_none());
    }

    #[test]
    fn edit_submit_puts_only_changes_to_the_record() {
        let mut app = app_with_tasks(two_tasks());
        press(&mut app, 'j'); // "Segunda"
        press(&mut app, 'e');
        // sin cambios no sale ningún pedido
        let step = app.on_key(ctrl_s());
        assert!(step.requests.is_empty());
        assert!(app.create.as_ref().unwrap().error.is_some());
        type_in(&mut app, "!");
        let step = app.on_key(ctrl_s());
        let request = &step.requests[0];
        assert_eq!(request.source, Source::EditTask);
        assert_eq!(request.path, "project-task/12");
        assert_eq!(request.body.as_ref().unwrap(), &json!({"name": "Segunda!"}));
    }

    #[test]
    fn quick_priority_puts_immediately_and_success_refreshes() {
        let mut app = app_with_tasks(two_tasks());
        press(&mut app, 'p'); // "Primera" tiene "Alta"
        app.on_key(key(KeyCode::Up)); // Normal
        let step = app.on_key(key(KeyCode::Enter));
        let request = &step.requests[0];
        assert_eq!(request.path, "project-task/11");
        assert_eq!(request.body.as_ref().unwrap(), &json!({"priority": "0"}));
        let refresh = app.on_response(saved(Source::EditTask, request.seq, 11, "Primera"));
        assert!(app.create.is_none());
        assert!(app.notice.as_deref().unwrap().contains("Task updated #11"));
        assert_eq!(refresh.len(), 3);
    }

    #[test]
    fn quick_stage_requests_the_catalog_of_the_task_project() {
        let mut app = app_with_tasks(two_tasks());
        let step = app.on_key(key(KeyCode::Char('s')));
        assert_eq!(step.requests[0].path, "catalog/task-stages");
        app.on_key(key(KeyCode::Esc));
        app.on_key(key(KeyCode::Esc));
        assert!(app.create.is_none());
    }

    #[test]
    fn edit_failure_keeps_the_form_open_with_the_message() {
        let mut app = app_with_tasks(two_tasks());
        press(&mut app, 'e');
        type_in(&mut app, "!");
        let request = app.on_key(ctrl_s()).requests.remove(0);
        app.on_response(Response {
            source: Source::EditTask,
            seq: request.seq,
            result: Err("El servidor respondió 400: Valor inválido".into()),
        });
        let form = app.create.as_ref().unwrap();
        assert!(!form.sending && form.error.as_deref().unwrap().contains("400"));
    }

    #[test]
    fn refresh_keeps_the_cursor_on_the_same_record() {
        let mut app = app_with_tasks(two_tasks());
        press(&mut app, 'j'); // "Segunda" (id 12)
        let step = press(&mut app, 'r');
        let reordered = json!([
            {"id": 13, "name": "Nueva", "date_deadline": null},
            {"id": 12, "name": "Segunda", "date_deadline": null},
            {"id": 11, "name": "Primera", "date_deadline": null}
        ]);
        let tasks = step
            .requests
            .iter()
            .find(|r| r.source == Source::Tasks)
            .unwrap();
        app.on_response(Response {
            source: Source::Tasks,
            seq: tasks.seq,
            result: Ok(Payload::Tasks(serde_json::from_value(reordered).unwrap())),
        });
        assert_eq!(app.tasks.selected_item().unwrap().id, 12);
        // cambiar el filtro, en cambio, vuelve al principio
        press(&mut app, '1');
        press(&mut app, 'j');
        let step = app.on_key(key(KeyCode::Enter));
        let tasks = step
            .requests
            .iter()
            .find(|r| r.source == Source::Tasks)
            .unwrap();
        app.on_response(Response {
            source: Source::Tasks,
            seq: tasks.seq,
            result: Ok(Payload::Tasks(
                serde_json::from_value(json!([
                    {"id": 20, "name": "A", "date_deadline": null},
                    {"id": 12, "name": "Segunda", "date_deadline": null}
                ]))
                .unwrap(),
            )),
        });
        assert_eq!(app.tasks.selected, 0);
    }

    fn close_request(app: &mut App) -> Request {
        press(app, 'c');
        let mut step = app.on_key(key(KeyCode::Enter));
        assert_eq!(step.requests.len(), 1);
        step.requests.remove(0)
    }

    #[test]
    fn c_asks_for_confirmation_before_completing() {
        let mut app = app_with_tasks(two_tasks());
        press(&mut app, 'c');
        assert_eq!(app.confirm.as_ref().unwrap().task_id, 11);
        // Esc y n cancelan sin pedir nada
        assert!(app.on_key(key(KeyCode::Esc)).requests.is_empty());
        assert!(app.confirm.is_none());
        press(&mut app, 'c');
        assert!(press(&mut app, 'n').requests.is_empty());
        assert!(app.confirm.is_none());
        // cualquier otra tecla no confirma ni se filtra a los paneles
        press(&mut app, 'c');
        assert!(press(&mut app, 'q').requests.is_empty() && app.confirm.is_some());
    }

    #[test]
    fn confirming_puts_done_state_on_the_selected_task() {
        let mut app = app_with_tasks(two_tasks());
        press(&mut app, 'j'); // "Segunda"
        let request = close_request(&mut app);
        assert_eq!(request.source, Source::CloseTask);
        assert_eq!(request.path, "project-task/12");
        assert_eq!(request.body.unwrap(), json!({"state": "1_done"}));
        assert!(app.confirm.as_ref().unwrap().sending);
    }

    #[test]
    fn completing_refreshes_lists_and_projects() {
        let mut app = app_with_tasks(two_tasks());
        let request = close_request(&mut app);
        let refresh = app.on_response(saved(Source::CloseTask, request.seq, 11, "Primera"));
        assert!(app.confirm.is_none());
        assert!(
            app.notice
                .as_deref()
                .unwrap()
                .contains("Task completed #11")
        );
        let sources: Vec<_> = refresh.iter().map(|r| r.source).collect();
        assert_eq!(sources, [Source::Tasks, Source::Hours, Source::Projects]);
        assert!(
            refresh
                .iter()
                .filter(|r| r.source == Source::Tasks)
                .all(|r| query(r, "open") == Some("true"))
        );
    }

    #[test]
    fn completion_failure_stays_open_and_can_be_retried_but_not_cancelled_in_flight() {
        let mut app = app_with_tasks(two_tasks());
        let request = close_request(&mut app);
        // en vuelo: no se cancela ni se duplica
        assert!(app.on_key(key(KeyCode::Esc)).requests.is_empty());
        assert!(app.on_key(key(KeyCode::Enter)).requests.is_empty());
        assert!(app.confirm.is_some());
        // una respuesta ajena no cierra nada
        assert!(
            app.on_response(saved(Source::CloseTask, request.seq + 9, 11, "x"))
                .is_empty()
        );
        assert!(app.confirm.is_some());
        app.on_response(Response {
            source: Source::CloseTask,
            seq: request.seq,
            result: Err("Acceso denegado: No podés modificar esta tarea".into()),
        });
        let confirm = app.confirm.as_ref().unwrap();
        assert!(!confirm.sending && confirm.error.as_deref().unwrap().contains("Acceso"));
        assert_eq!(app.on_key(key(KeyCode::Enter)).requests.len(), 1);
    }

    #[test]
    fn c_only_works_on_a_selected_task() {
        let mut app = app_with_tasks(two_tasks());
        for panel in ['1', '3'] {
            press(&mut app, panel);
            press(&mut app, 'c');
            assert!(app.confirm.is_none());
        }
        let (mut empty, _) = ready();
        press(&mut empty, '2');
        press(&mut empty, 'c');
        assert!(empty.confirm.is_none());
    }

    #[test]
    fn completing_a_task_may_have_applied_on_ambiguous_errors() {
        let request = Request {
            source: Source::CloseTask,
            seq: 1,
            path: "project-task/1".into(),
            query: vec![],
            body: Some(json!({"state": "1_done"})),
        };
        assert!(describe(&request, &ApiError::Timeout(15)).contains("Tasks panel"));
    }

    #[test]
    fn hours_from_subtasks_show_up_in_the_confirmation() {
        let mut app = app_with_tasks(json!([
            {"id": 22, "name": "Solo subtareas", "date_deadline": null,
             "project": {"id": 4, "name": "Alfa"}, "effective_hours": 0.0, "total_hours_spent": 3.0}
        ]));
        press(&mut app, 'c');
        let confirm = app.confirm.as_ref().unwrap();
        assert!(confirm.hours == 3.0);
        assert_eq!(app.on_key(key(KeyCode::Enter)).requests.len(), 1);
    }

    #[test]
    fn stage_catalogs_are_requested_for_the_tasks_project_and_cached_per_scope() {
        // «Primera» y «Segunda» son del proyecto 4 en el fixture
        let mut app = app_with_tasks(two_tasks());
        let step = app.on_key(key(KeyCode::Char('s'))); // estado de la tarea #11
        let request = &step.requests[0];
        assert_eq!(request.path, "catalog/task-stages");
        assert_eq!(query(request, "project_id"), Some("4"));
        assert_eq!(request.source, Source::Catalog((Cat::TaskStages, Some(4))));
        app.on_response(catalog_response(
            (Cat::TaskStages, Some(4)),
            request.seq,
            json!([{"id": 7, "name": "Nuevo"}, {"id": 8, "name": "En ejecución"}]),
        ));
        // elegir «Nuevo» guarda enseguida
        let step = app.on_key(key(KeyCode::Enter));
        assert_eq!(
            step.requests[0].body.as_ref().unwrap(),
            &json!({"stage_id": 7})
        );
    }

    /// Carga horas y deja marcado «Finalizar tarea?».
    fn submit_hours_and_finish(app: &mut App) -> Request {
        press(app, 't');
        type_in(app, "2");
        app.on_key(key(KeyCode::Enter));
        type_in(app, "reunión");
        app.on_key(key(KeyCode::Enter)); // checkbox
        press(app, ' ');
        let mut step = app.on_key(key(KeyCode::Enter));
        assert_eq!(step.requests.len(), 1);
        step.requests.remove(0)
    }

    fn finish_request_after_hours(app: &mut App, hours: &Request) -> Request {
        let mut next = app.on_response(created(hours.seq));
        assert_eq!(next.len(), 1, "solo el pedido de finalizar");
        next.remove(0)
    }

    #[test]
    fn finish_checked_loads_the_hours_first_and_then_completes_the_task() {
        let mut app = with_task(true);
        let hours = submit_hours_and_finish(&mut app);
        assert_eq!(hours.source, Source::CreateHours);
        let close = finish_request_after_hours(&mut app, &hours);
        assert_eq!(close.source, Source::CloseTask);
        assert_eq!(close.path, "project-task/11");
        assert_eq!(close.body.unwrap(), json!({"state": "1_done"}));
        assert!(app.form.is_none());
        let confirm = app.confirm.as_ref().unwrap();
        assert!(confirm.sending && confirm.after_hours);
        assert!(app.notice.as_deref().unwrap().contains("Logged 2.0h"));
    }

    #[test]
    fn finishing_after_hours_refreshes_everything_on_success() {
        let mut app = with_task(true);
        let hours = submit_hours_and_finish(&mut app);
        let close = finish_request_after_hours(&mut app, &hours);
        let refresh = app.on_response(saved(Source::CloseTask, close.seq, 11, "Migrar"));
        assert!(app.confirm.is_none());
        assert!(
            app.notice
                .as_deref()
                .unwrap()
                .contains("Task completed #11")
        );
        let sources: Vec<_> = refresh.iter().map(|r| r.source).collect();
        assert_eq!(sources, [Source::Tasks, Source::Hours, Source::Projects]);
    }

    #[test]
    fn if_finishing_fails_the_hours_are_kept_and_the_error_says_so() {
        let mut app = with_task(true);
        let hours = submit_hours_and_finish(&mut app);
        let close = finish_request_after_hours(&mut app, &hours);
        let refresh = app.on_response(Response {
            source: Source::CloseTask,
            seq: close.seq,
            result: Err("Odoo respondió 400: No se pudo cambiar el estado".into()),
        });
        // las horas ya están guardadas: se recargan los paneles para verlas
        assert!(refresh.iter().any(|r| r.source == Source::Hours));
        let confirm = app.confirm.as_ref().unwrap();
        let error = confirm.error.as_deref().unwrap();
        assert!(!confirm.sending);
        assert!(error.contains("hours were already logged") && error.contains("estado"));
        // Enter reintenta solo el cierre (no vuelve a cargar horas)
        let retry = app.on_key(key(KeyCode::Enter));
        assert_eq!(retry.requests.len(), 1);
        assert_eq!(retry.requests[0].source, Source::CloseTask);
        // Esc deja todo como está
        app.on_response(Response {
            source: Source::CloseTask,
            seq: retry.requests[0].seq,
            result: Err("otra vez".into()),
        });
        app.on_key(key(KeyCode::Esc));
        assert!(app.confirm.is_none());
    }

    #[test]
    fn without_the_checkbox_the_task_is_left_alone() {
        let mut app = with_task(true);
        let hours = submit_hours(&mut app);
        let next = app.on_response(created(hours.seq));
        assert!(next.iter().all(|r| r.source != Source::CloseTask));
        assert!(app.confirm.is_none());
    }
}
