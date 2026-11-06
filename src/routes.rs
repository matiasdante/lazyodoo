//! Rutas, filtros y serializers de la app, resueltos directamente contra
//! Odoo por JSON-RPC. Recibe rutas, filtros y bodies simples y devuelve JSON
//! con la forma que esperan `models` y la UI.
//!
//! Solo existen las rutas y los campos de la lista blanca: no hay un
//! `execute_kw` genérico.

use std::collections::{BTreeMap, HashMap};

use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};
use serde_json::{Map, Value, json};

use crate::api::ApiError;
use crate::odoo::Session;

const DEFAULT_LIMIT: u32 = 80;
const MAX_LIMIT: u32 = 1000;
const MAX_IDS: usize = 100;

/// Nombres de los registros relacionados por campo many2many.
type Related = HashMap<&'static str, HashMap<i64, String>>;

fn bad(message: impl Into<String>) -> ApiError {
    ApiError::BadResponse(message.into())
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Model {
    Task,
    Timesheet,
}

impl Model {
    fn name(self) -> &'static str {
        match self {
            Model::Task => "project.task",
            Model::Timesheet => "account.analytic.line",
        }
    }

    /// Solo los campos que consumen los serializers; no se leen adjuntos.
    fn fields(self) -> &'static [&'static str] {
        match self {
            Model::Task => &[
                "name",
                "description",
                "date_deadline",
                "priority",
                "allocated_hours",
                "effective_hours",
                "remaining_hours",
                "total_hours_spent",
                "project_id",
                "stage_id",
                "partner_id",
                "user_ids",
                "tag_ids",
            ],
            Model::Timesheet => &[
                "name",
                "date",
                "unit_amount",
                "project_id",
                "task_id",
                "employee_id",
            ],
        }
    }

    /// Campos many2many cuyos nombres se resuelven por lote.
    fn many(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Model::Task => &[("user_ids", "res.users"), ("tag_ids", "project.tags")],
            _ => &[],
        }
    }

    fn shape(self, row: &Value, related: &Related) -> Value {
        match self {
            Model::Task => {
                let description = text(row, "description");
                json!({
                    "id": row["id"],
                    "name": text(row, "name"),
                    "description_has_image": has_image(&description),
                    "description": description,
                    "date_deadline": datetime(row, "date_deadline"),
                    "priority": text(row, "priority"),
                    "allocated_hours": number(row, "allocated_hours"),
                    "effective_hours": number(row, "effective_hours"),
                    "remaining_hours": number(row, "remaining_hours"),
                    "total_hours_spent": number(row, "total_hours_spent"),
                    "project": m2o(row, "project_id"),
                    "stage": m2o(row, "stage_id"),
                    "partner": m2o(row, "partner_id"),
                    "users": many(row, "user_ids", related),
                    "tags": many(row, "tag_ids", related),
                })
            }
            Model::Timesheet => json!({
                "id": row["id"],
                "name": text(row, "name"),
                "date": text(row, "date"),
                "unit_amount": number(row, "unit_amount"),
                "date_time": datetime(row, "date_time"),
                "project": m2o(row, "project_id"),
                "task": m2o(row, "task_id"),
                "employee": m2o(row, "employee_id"),
            }),
        }
    }
}

// --- lectura de valores crudos de Odoo (False = vacío) -----------------------

fn text(row: &Value, key: &str) -> String {
    row.get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn number(row: &Value, key: &str) -> f64 {
    row.get(key).and_then(Value::as_f64).unwrap_or(0.0)
}

fn m2o(row: &Value, key: &str) -> Value {
    match row.get(key) {
        Some(Value::Array(pair)) if pair.len() == 2 => json!({"id": pair[0], "name": pair[1]}),
        _ => Value::Null,
    }
}

fn many(row: &Value, key: &str, related: &Related) -> Value {
    let names = related.get(key);
    let list = row
        .get(key)
        .and_then(Value::as_array)
        .map_or(vec![], |ids| {
            ids.iter()
                .filter_map(Value::as_i64)
                .map(|id| {
                    let name = names.and_then(|n| n.get(&id)).cloned().unwrap_or_default();
                    json!({"id": id, "name": name})
                })
                .collect()
        });
    Value::Array(list)
}

/// `"2026-11-20 23:59:00"` (UTC) a `"2026-11-20T23:59:00"`, como lo serializaba la API.
fn datetime(row: &Value, key: &str) -> Value {
    match row.get(key).and_then(Value::as_str) {
        Some(raw) => json!(raw.replacen(' ', "T", 1)),
        None => Value::Null,
    }
}

/// Un `<img src="data:image/...">` embebido: solo se avisa que existe.
fn has_image(html: &str) -> bool {
    html.to_ascii_lowercase().contains("src=\"data:image/")
}

// --- parámetros de búsqueda -----------------------------------------------------

struct Params<'a>(&'a [(&'a str, String)]);

impl<'a> Params<'a> {
    fn new(items: &'a [(&'a str, String)], allowed: &[&str]) -> Result<Self, ApiError> {
        for (key, _) in items {
            if !allowed.contains(key) {
                return Err(bad(format!("Parámetro no permitido: {key}")));
            }
        }
        Ok(Self(items))
    }

    fn get(&self, key: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, v)| v.as_str())
    }

    fn id(&self, key: &str) -> Result<Option<i64>, ApiError> {
        self.get(key)
            .map(|raw| {
                raw.parse::<i64>()
                    .ok()
                    .filter(|id| *id > 0)
                    .ok_or_else(|| bad(format!("{key} inválido")))
            })
            .transpose()
    }

    fn limit(&self) -> Result<u32, ApiError> {
        match self.get("limit") {
            None => Ok(DEFAULT_LIMIT),
            Some(raw) => raw
                .parse::<u32>()
                .ok()
                .filter(|l| (1..=MAX_LIMIT).contains(l))
                .ok_or_else(|| bad(format!("limit tiene que estar entre 1 y {MAX_LIMIT}"))),
        }
    }

    /// `open=true`: solo lo no finalizado.
    fn flag(&self, key: &str) -> Result<bool, ApiError> {
        match self.get(key) {
            None | Some("false") => Ok(false),
            Some("true") => Ok(true),
            Some(_) => Err(bad(format!("{key} inválido"))),
        }
    }

    fn date(&self, key: &str) -> Result<Option<String>, ApiError> {
        self.get(key)
            .map(|raw| {
                NaiveDate::parse_from_str(raw, "%Y-%m-%d")
                    .map(|d| d.format("%Y-%m-%d").to_string())
                    .map_err(|_| bad(format!("{key} inválido")))
            })
            .transpose()
    }

    fn equality(&self, keys: &[&str]) -> Result<Vec<Value>, ApiError> {
        let mut domain = vec![];
        for key in keys {
            if let Some(id) = self.id(key)? {
                domain.push(json!([key, "=", id]));
            }
        }
        Ok(domain)
    }
}

// --- lecturas -------------------------------------------------------------------

pub fn get(session: &Session, path: &str, query: &[(&str, String)]) -> Result<Value, ApiError> {
    let path = path.trim_matches('/');
    if let Some(name) = path.strip_prefix("catalog/") {
        // `project_id` acota los estados a los de ese proyecto
        let p = Params::new(query, &["project_id"])?;
        return catalog(session, name, p.id("project_id")?);
    }
    match path {
        "project-task/search" => {
            let p = Params::new(
                query,
                &["project_id", "user_id", "stage_id", "open", "limit"],
            )?;
            let mut domain = p.equality(&["project_id", "stage_id"])?;
            if let Some(user) = p.id("user_id")? {
                domain.push(json!(["user_ids", "in", [user]]));
            }
            if p.flag("open")? {
                domain.extend(open_domain(session));
            }
            list(session, Model::Task, domain, p.limit()?)
        }
        "timesheet/search" => {
            let p = Params::new(
                query,
                &[
                    "project_id",
                    "task_id",
                    "employee_id",
                    "date_from",
                    "date_to",
                    "limit",
                ],
            )?;
            let mut domain = vec![json!(["project_id", "!=", false])];
            domain.extend(p.equality(&["project_id", "task_id", "employee_id"])?);
            if let Some(from) = p.date("date_from")? {
                domain.push(json!(["date", ">=", from]));
            }
            if let Some(to) = p.date("date_to")? {
                domain.push(json!(["date", "<=", to]));
            }
            list(session, Model::Timesheet, domain, p.limit()?)
        }
        other => Err(bad(format!("Ruta no soportada: {other}"))),
    }
}

/// Filtro de tareas "no finalizadas" según los campos que tenga este Odoo:
/// `state` (Odoo 17) o `is_closed`. Sin ellos no filtra.
fn open_domain(session: &Session) -> Vec<Value> {
    if session.has_field("project.task", "state") {
        vec![json!(["state", "not in", ["1_done", "1_canceled"]])]
    } else if session.has_field("project.task", "is_closed") {
        vec![json!(["is_closed", "=", false])]
    } else {
        vec![]
    }
}

fn list(
    session: &Session,
    model: Model,
    domain: Vec<Value>,
    limit: u32,
) -> Result<Value, ApiError> {
    let rows = search(session, model, domain, Some(limit))?;
    Ok(json!({"count": rows.len(), "rows": rows}))
}

/// `search_read` más la resolución por lote de los many2many (una lectura por
/// campo, no una por registro). Odoo aplica los permisos del usuario.
fn search(
    session: &Session,
    model: Model,
    domain: Vec<Value>,
    limit: Option<u32>,
) -> Result<Vec<Value>, ApiError> {
    let mut fields = model.fields().to_vec();
    // hora de inicio del parte, si este Odoo la guarda
    if model == Model::Timesheet && has_start_time(session) {
        fields.push("date_time");
    }
    let mut kwargs = json!({"fields": fields});
    if let Some(limit) = limit {
        kwargs["limit"] = json!(limit);
    }
    let rows = session.execute(model.name(), "search_read", json!([domain]), kwargs)?;
    let rows = rows
        .as_array()
        .ok_or_else(|| bad("search_read no devolvió una lista"))?;

    let mut related = Related::new();
    for (field, related_model) in model.many() {
        let mut ids: Vec<i64> = rows
            .iter()
            .filter_map(|row| row.get(*field).and_then(Value::as_array))
            .flatten()
            .filter_map(Value::as_i64)
            .collect();
        ids.sort_unstable();
        ids.dedup();
        let names = if ids.is_empty() {
            HashMap::new()
        } else {
            let records = session.execute(
                related_model,
                "read",
                json!([ids]),
                json!({"fields": ["name"]}),
            )?;
            records
                .as_array()
                .map(|records| {
                    records
                        .iter()
                        .filter_map(|r| Some((r.get("id")?.as_i64()?, text(r, "name"))))
                        .collect()
                })
                .unwrap_or_default()
        };
        related.insert(*field, names);
    }
    Ok(rows.iter().map(|row| model.shape(row, &related)).collect())
}

/// Tu usuario (por `uid`, no por email) y tu empleado, si lo hay.
fn me(session: &Session) -> Result<Value, ApiError> {
    let uid = session.uid()?;
    let users = session.execute(
        "res.users",
        "read",
        json!([[uid]]),
        json!({"fields": ["name", "display_name", "email"]}),
    )?;
    let user = users
        .get(0)
        .ok_or_else(|| bad("No se pudo leer tu usuario"))?;
    let label = match text(user, "display_name") {
        label if !label.is_empty() => label,
        _ => text(user, "name"),
    };
    // el empleado (hr) puede no existir o no ser legible: no es motivo de error
    let employee = session
        .execute(
            "hr.employee",
            "search_read",
            json!([[["user_id", "=", uid]]]),
            json!({"fields": ["id"], "limit": 1}),
        )
        .ok()
        .and_then(|rows| rows.get(0)?.get("id")?.as_i64());
    Ok(json!({"count": 1, "rows": [{
        "id": uid, "name": label, "email": text(user, "email"), "employee_id": employee,
        "timesheet_start": has_start_time(session),
    }]}))
}

fn project_pair(value: Option<&Value>) -> Option<(i64, String)> {
    let pair = value?.as_array()?;
    Some((pair.first()?.as_i64()?, pair.get(1)?.as_str()?.to_string()))
}

/// Proyectos donde tenés tareas abiertas asignadas.
fn my_projects(session: &Session) -> Result<Value, ApiError> {
    let uid = session.uid()?;
    let mut projects: BTreeMap<i64, String> = BTreeMap::new();
    let mut domain = vec![json!(["user_ids", "in", [uid]])];
    domain.extend(open_domain(session));
    let rows = session.execute(
        "project.task",
        "search_read",
        json!([domain]),
        json!({"fields": ["project_id"], "limit": MAX_LIMIT}),
    )?;
    for row in rows.as_array().into_iter().flatten() {
        if let Some((id, name)) = project_pair(row.get("project_id")) {
            projects.insert(id, name);
        }
    }
    let mut rows: Vec<Value> = projects
        .into_iter()
        .map(|(id, name)| json!({"id": id, "name": name}))
        .collect();
    rows.sort_by_key(|r| r["name"].as_str().unwrap_or("").to_lowercase());
    Ok(json!({"count": rows.len(), "rows": rows}))
}

/// ¿Las horas de este Odoo guardan hora de inicio (`date_time`)?
fn has_start_time(session: &Session) -> bool {
    session.has_field("account.analytic.line", "date_time")
}

fn catalog(session: &Session, name: &str, project: Option<i64>) -> Result<Value, ApiError> {
    match name {
        "me" => return me(session),
        "my-projects" => return my_projects(session),
        _ => {}
    }
    let model = match name {
        "task-stages" => "project.task.type",
        "task-tags" => "project.tags",
        "task-types" => "project.type",
        "projects" => "project.project",
        "users" => "res.users",
        other => return Err(bad(format!("Catálogo no soportado: {other}"))),
    };
    let users = name == "users";
    // usuarios internos, con el email para ubicarlos sin depender del nombre
    let (domain, fields) = if users {
        (
            json!([["share", "=", false]]),
            json!(["name", "display_name", "email"]),
        )
    } else {
        (json!([]), json!(["name", "display_name"]))
    };
    // los estados dependen del proyecto, si este Odoo lo modela así; si el
    // alcance no devuelve nada, se listan todos
    let scoped = match (name, project) {
        ("task-stages", Some(project)) if session.has_field(model, "project_ids") => {
            Some(json!([["project_ids", "in", [project]]]))
        }
        _ => None,
    };
    let read = |domain: &Value| {
        session.execute(
            model,
            "search_read",
            json!([domain]),
            json!({"fields": fields}),
        )
    };
    let mut records = match &scoped {
        Some(domain) => read(domain)?,
        None => read(&domain)?,
    };
    if scoped.is_some() && records.as_array().is_none_or(Vec::is_empty) {
        records = read(&domain)?;
    }
    let rows: Vec<Value> = records
        .as_array()
        .ok_or_else(|| bad("search_read no devolvió una lista"))?
        .iter()
        .map(|r| {
            let label = match text(r, "display_name") {
                label if !label.is_empty() => label,
                _ => text(r, "name"),
            };
            let mut row = json!({"id": r["id"], "name": label});
            if users {
                row["email"] = json!(text(r, "email"));
            }
            row
        })
        .collect();
    Ok(json!({"count": rows.len(), "rows": rows}))
}

// --- escrituras -----------------------------------------------------------------

/// Campos que acepta cada alta o modificación y los que son obligatorios al crear.
fn write_spec(model: Model) -> (&'static [&'static str], &'static [&'static str]) {
    match model {
        Model::Task => (
            &[
                "name",
                "project_id",
                "partner_id",
                "description",
                "date_deadline",
                "priority",
                "allocated_hours",
                "user_ids",
                "stage_id",
                "tag_ids",
                "state",
            ],
            &["name", "project_id"],
        ),
        Model::Timesheet => (
            &[
                "project_id",
                "unit_amount",
                "task_id",
                "employee_id",
                "name",
                "date",
                "date_time",
            ],
            &["project_id", "unit_amount"],
        ),
    }
}

fn write_model(segment: &str) -> Result<Model, ApiError> {
    match segment {
        "project-task" => Ok(Model::Task),
        "timesheet" => Ok(Model::Timesheet),
        other => Err(bad(format!("Ruta no soportada: {other}"))),
    }
}

pub fn post(session: &Session, path: &str, body: &Value) -> Result<Value, ApiError> {
    let model = write_model(path.trim_matches('/'))?;
    let vals = to_vals(model, body, true)?;
    if model == Model::Timesheet && vals.get("date_time").is_some() && !has_start_time(session) {
        return Err(bad(
            "Este Odoo no tiene el campo «date_time» en las horas: no guarda hora de inicio",
        ));
    }
    let created = session.execute(model.name(), "create", json!([vals]), json!({}))?;
    let id = created
        .as_i64()
        .or_else(|| created.get(0).and_then(Value::as_i64))
        .ok_or_else(|| bad("Odoo no devolvió el id del registro creado"))?;
    reload(session, model, id)
}

pub fn put(session: &Session, path: &str, body: &Value) -> Result<Value, ApiError> {
    let (segment, id) = path
        .trim_matches('/')
        .split_once('/')
        .ok_or_else(|| bad("Falta el id del registro"))?;
    let model = write_model(segment)?;
    let id = id
        .parse::<i64>()
        .ok()
        .filter(|id| *id > 0)
        .ok_or_else(|| bad("Id de registro inválido"))?;
    let vals = to_vals(model, body, false)?;
    if vals.as_object().is_some_and(Map::is_empty) {
        return Err(bad("No hay campos para modificar"));
    }
    if model == Model::Task
        && vals.get("state").is_some()
        && !session.has_field("project.task", "state")
    {
        return Err(bad(
            "Este Odoo no tiene el campo «state» en las tareas: cambiá el estado con «s»",
        ));
    }
    session.execute(model.name(), "write", json!([[id], vals]), json!({}))?;
    reload(session, model, id)
}

/// Relee el registro después de `create` y `write`.
fn reload(session: &Session, model: Model, id: i64) -> Result<Value, ApiError> {
    let mut rows = search(session, model, vec![json!(["id", "=", id])], Some(1))?;
    if rows.is_empty() {
        // 502 = fallo ambiguo: el cambio ya quedó aplicado
        return Err(ApiError::Server(
            502,
            "El cambio se guardó, pero no se pudo leer el registro de vuelta.".into(),
        ));
    }
    Ok(rows.remove(0))
}

/// Valida el body contra la lista blanca y lo traduce a valores de Odoo
/// (`None` = `false`, many2many = comando `(6, 0, ids)`).
fn to_vals(model: Model, body: &Value, creating: bool) -> Result<Value, ApiError> {
    let (allowed, required) = write_spec(model);
    let object = body
        .as_object()
        .ok_or_else(|| bad("El body tiene que ser un objeto"))?;
    if let Some(key) = object.keys().find(|k| !allowed.contains(&k.as_str())) {
        return Err(bad(format!("Campo no permitido: {key}")));
    }
    if creating {
        for key in required {
            if object.get(*key).is_none_or(Value::is_null) {
                return Err(bad(format!("Falta el campo {key}")));
            }
        }
    }
    let mut vals = Map::new();
    for (key, value) in object {
        let (out_key, out_value) = match key.as_str() {
            "user_ids" | "tag_ids" => (key.as_str(), json!([[6, 0, id_list(key, value)?]])),
            "date_deadline" | "date_time" => (key.as_str(), deadline(value)?),
            "unit_amount" => (key.as_str(), hours(value)?),
            "state" => (key.as_str(), task_state(value)?),
            k if k.ends_with("_id") => (k, record_id(k, value)?),
            k => (k, null_to_false(value)),
        };
        vals.insert(out_key.to_string(), out_value);
    }
    Ok(Value::Object(vals))
}

fn null_to_false(value: &Value) -> Value {
    if value.is_null() {
        json!(false)
    } else {
        value.clone()
    }
}

fn record_id(key: &str, value: &Value) -> Result<Value, ApiError> {
    match value {
        Value::Null => Ok(json!(false)),
        Value::Number(n) if n.as_i64().is_some_and(|id| id > 0) => Ok(value.clone()),
        _ => Err(bad(format!("{key} inválido"))),
    }
}

fn id_list(key: &str, value: &Value) -> Result<Vec<i64>, ApiError> {
    match value {
        Value::Null => Ok(vec![]),
        Value::Array(items) if items.len() <= MAX_IDS => items
            .iter()
            .map(|item| {
                item.as_i64()
                    .filter(|id| *id > 0)
                    .ok_or_else(|| bad(format!("{key} inválido")))
            })
            .collect(),
        _ => Err(bad(format!("{key} inválido"))),
    }
}

/// Estados de tarea de Odoo 17 que se pueden pedir.
fn task_state(value: &Value) -> Result<Value, ApiError> {
    const STATES: [&str; 5] = [
        "01_in_progress",
        "02_changes_requested",
        "03_approved",
        "1_done",
        "1_canceled",
    ];
    match value.as_str() {
        Some(state) if STATES.contains(&state) => Ok(value.clone()),
        _ => Err(bad("state inválido")),
    }
}

fn hours(value: &Value) -> Result<Value, ApiError> {
    match value.as_f64() {
        Some(h) if h.is_finite() && (0.0..=24.0).contains(&h) => Ok(value.clone()),
        _ => Err(bad("unit_amount tiene que estar entre 0 y 24")),
    }
}

/// Odoo guarda los datetime como UTC naive (`AAAA-MM-DD HH:MM:SS`).
fn deadline(value: &Value) -> Result<Value, ApiError> {
    let raw = match value {
        Value::Null => return Ok(json!(false)),
        Value::String(raw) => raw,
        _ => return Err(bad("date_deadline inválido")),
    };
    const OUT: &str = "%Y-%m-%d %H:%M:%S";
    let utc = DateTime::parse_from_rfc3339(raw)
        .map(|d| d.with_timezone(&Utc).format(OUT).to_string())
        .or_else(|_| {
            NaiveDateTime::parse_from_str(raw, "%Y-%m-%dT%H:%M:%S")
                .map(|n| n.format(OUT).to_string())
        })
        .or_else(|_| {
            NaiveDate::parse_from_str(raw, "%Y-%m-%d")
                .map(|d| d.format("%Y-%m-%d 00:00:00").to_string())
        })
        .map_err(|_| bad("date_deadline inválido"))?;
    Ok(json!(utc))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::models::{Named, Task, Timesheet, User};
    use crate::testutil::{Call, FakeOdoo, ok};
    use std::time::Duration;

    fn session(fake: &FakeOdoo) -> Session {
        let config = Config::new(&fake.url, "odoo", "me@example.com", "key").unwrap();
        Session::new(&config, Duration::from_secs(2)).unwrap()
    }

    fn q(items: &[(&'static str, &str)]) -> Vec<(&'static str, String)> {
        items.iter().map(|(k, v)| (*k, v.to_string())).collect()
    }

    fn task_row() -> Value {
        json!({
            "id": 11, "name": "Migrar", "description": "<p>Hola</p><img src=\"data:image/png;base64,AAAA\">",
            "date_deadline": "2026-11-20 23:59:00", "priority": "1",
            "allocated_hours": 8.0, "effective_hours": 2.0, "remaining_hours": 6.0,
            "total_hours_spent": 2.0,
            "project_id": [4, "Alfa"], "stage_id": [7, "Nuevo"], "partner_id": false,
            "user_ids": [9, 3], "tag_ids": [1]
        })
    }

    #[test]
    fn task_search_uses_the_domain_fields_and_limit() {
        let fake = FakeOdoo::with_login(|c: &Call| match (c.model.as_str(), c.op.as_str()) {
            ("project.task", "search_read") => ok(json!([task_row()])),
            ("res.users", "read") => {
                ok(json!([{"id": 9, "name": "Yo"}, {"id": 3, "name": "Otro"}]))
            }
            ("project.tags", "read") => ok(json!([{"id": 1, "name": "urgente"}])),
            _ => panic!("llamada inesperada {c:?}"),
        });
        let query = q(&[
            ("project_id", "4"),
            ("stage_id", "7"),
            ("user_id", "9"),
            ("limit", "200"),
        ]);
        let result = get(&session(&fake), "project-task/search", &query).unwrap();

        let calls = fake.data_calls();
        assert_eq!(
            calls[0].args,
            json!([[
                ["project_id", "=", 4],
                ["stage_id", "=", 7],
                ["user_ids", "in", [9]]
            ]])
        );
        assert_eq!(calls[0].kwargs["limit"], 200);
        assert_eq!(calls[0].kwargs["fields"][0], "name");
        // una lectura por campo many2many con ids, no por registro; vacío no se pide
        let reads: Vec<_> = calls
            .iter()
            .filter(|c| c.op == "read")
            .map(|c| c.model.as_str())
            .collect();
        assert_eq!(reads, ["res.users", "project.tags"]);

        let task: Task = serde_json::from_value(result["rows"][0].clone()).unwrap();
        assert_eq!(result["count"], 1);
        assert_eq!(
            task.users
                .iter()
                .map(|u| u.name.as_str())
                .collect::<Vec<_>>(),
            ["Yo", "Otro"]
        );
        assert_eq!(task.project.unwrap().name, "Alfa");
        assert!(task.partner.is_none());
        assert_eq!(task.date_deadline.as_deref(), Some("2026-11-20T23:59:00"));
        assert!(task.description_has_image);
        assert_eq!(task.priority, "1");
    }

    #[test]
    fn default_limit_is_80_and_invalid_params_never_reach_odoo() {
        let fake = FakeOdoo::with_login(|_| ok(json!([])));
        let s = session(&fake);
        get(&s, "project-task/search", &[]).unwrap();
        assert_eq!(fake.data_calls()[0].kwargs["limit"], 80);
        for bad_query in [
            q(&[("limit", "0")]),
            q(&[("limit", "1001")]),
            q(&[("limit", "x")]),
            q(&[("project_id", "-3")]),
            q(&[("project_id", "abc")]),
            q(&[("extra", "1")]),
        ] {
            assert!(
                get(&s, "project-task/search", &bad_query).is_err(),
                "{bad_query:?}"
            );
        }
        assert!(get(&s, "timesheet/search", &q(&[("date_from", "05/11/2026")])).is_err());
        assert!(get(&s, "otra/cosa", &[]).is_err());
        assert!(get(&s, "catalog/modelo-secreto", &[]).is_err());
        assert_eq!(fake.data_calls().len(), 1);
    }

    #[test]
    fn timesheet_search_builds_the_domain() {
        let fake = FakeOdoo::with_login(|_| {
            ok(
                json!([{"id": 1, "name": false, "date": "2026-11-03", "unit_amount": 2.5,
                       "project_id": [4, "Alfa"], "task_id": false, "employee_id": [8, "Yo"]}]),
            )
        });
        let query = q(&[
            ("project_id", "4"),
            ("date_from", "2026-11-02"),
            ("date_to", "2026-11-08"),
        ]);
        let result = get(&session(&fake), "timesheet/search", &query).unwrap();
        let search = fake
            .data_calls()
            .into_iter()
            .find(|c| c.op == "search_read")
            .unwrap();
        assert_eq!(
            search.args,
            json!([[
                ["project_id", "!=", false],
                ["project_id", "=", 4],
                ["date", ">=", "2026-11-02"],
                ["date", "<=", "2026-11-08"]
            ]])
        );
        let line: Timesheet = serde_json::from_value(result["rows"][0].clone()).unwrap();
        assert_eq!(
            (line.unit_amount, line.name.as_str(), line.date.as_str()),
            (2.5, "", "2026-11-03")
        );
        assert_eq!(line.employee.unwrap().name, "Yo");
    }

    #[test]
    fn catalogs_list_display_names_and_internal_users_with_email() {
        let fake = FakeOdoo::with_login(|c: &Call| match c.model.as_str() {
            "res.users" => {
                ok(json!([{"id": 9, "name": "yo", "display_name": "Yo Mismo", "email": false}]))
            }
            "project.project" => ok(json!([{"id": 4, "name": "Alfa", "display_name": "Alfa"}])),
            _ => panic!("{c:?}"),
        });
        let s = session(&fake);
        let users = get(&s, "catalog/users", &[]).unwrap();
        let user: User = serde_json::from_value(users["rows"][0].clone()).unwrap();
        assert_eq!(user.name, "Yo Mismo");
        assert_eq!(users["rows"][0]["email"], "");
        assert_eq!(fake.data_calls()[0].args, json!([[["share", "=", false]]]));
        let projects = get(&s, "catalog/projects", &[]).unwrap();
        assert_eq!(projects["rows"][0], json!({"id": 4, "name": "Alfa"}));
        // sin límite
        assert!(fake.data_calls()[1].kwargs.get("limit").is_none());
        // los catálogos mapean a los modelos de proyecto
        for (name, model) in [
            ("task-stages", "project.task.type"),
            ("task-tags", "project.tags"),
        ] {
            let ok_fake = FakeOdoo::with_login(|_| ok(json!([])));
            get(&session(&ok_fake), &format!("catalog/{name}"), &[]).unwrap();
            assert_eq!(ok_fake.data_calls()[0].model, model);
        }
    }

    #[test]
    fn creating_a_task_sends_odoo_values_and_rereads_the_record() {
        let fake = FakeOdoo::with_login(|c: &Call| match (c.model.as_str(), c.op.as_str()) {
            ("project.task", "create") => ok(json!(321)),
            ("project.task", "search_read") => ok(json!([{
                "id": 321, "name": "Nueva", "project_id": [4, "Alfa"], "user_ids": [9],
                "tag_ids": [], "date_deadline": "2026-11-21 02:59:00"
            }])),
            ("res.users", "read") => ok(json!([{"id": 9, "name": "Yo"}])),
            _ => panic!("{c:?}"),
        });
        let body = json!({
            "name": "Nueva", "project_id": 4, "user_ids": [9], "stage_id": null,
            "date_deadline": "2026-11-20T23:59:00-03:00", "allocated_hours": 4.5
        });
        let created = post(&session(&fake), "project-task", &body).unwrap();
        let calls = fake.data_calls();
        assert_eq!(
            calls[0].args,
            json!([{
                "name": "Nueva", "project_id": 4, "user_ids": [[6, 0, [9]]], "stage_id": false,
                "date_deadline": "2026-11-21 02:59:00", "allocated_hours": 4.5
            }])
        );
        assert_eq!(calls[1].args, json!([[["id", "=", 321]]]));
        let named: Named = serde_json::from_value(created).unwrap();
        assert_eq!((named.id, named.name.as_str()), (321, "Nueva"));
    }

    #[test]
    fn put_writes_only_given_fields_and_clears_with_false_and_empty_commands() {
        let fake = FakeOdoo::with_login(|c: &Call| match c.op.as_str() {
            "write" => ok(json!(true)),
            "search_read" => {
                ok(json!([{"id": 11, "name": "Migrar", "user_ids": [], "tag_ids": []}]))
            }
            _ => panic!("{c:?}"),
        });
        let body = json!({"stage_id": null, "user_ids": null, "priority": "0", "tag_ids": [3]});
        put(&session(&fake), "project-task/11", &body).unwrap();
        let write = &fake.data_calls()[0];
        assert_eq!(
            (write.model.as_str(), write.op.as_str()),
            ("project.task", "write")
        );
        assert_eq!(
            write.args,
            json!([[11], {"stage_id": false, "user_ids": [[6, 0, []]], "priority": "0", "tag_ids": [[6, 0, [3]]]}])
        );
    }

    #[test]
    fn timesheet_create_passes_only_whitelisted_fields() {
        let fake = FakeOdoo::with_login(|c: &Call| match c.op.as_str() {
            "create" => ok(json!(77)),
            _ => ok(
                json!([{"id": 77, "name": "daily", "date": "2026-11-05", "unit_amount": 1.5,
                            "project_id": [4, "Alfa"], "task_id": [11, "Migrar"], "employee_id": [8, "Yo"]}]),
            ),
        });
        let body = json!({"project_id": 4, "task_id": 11, "unit_amount": 1.5, "name": "daily", "date": "2026-11-05"});
        let line = post(&session(&fake), "timesheet", &body).unwrap();
        assert_eq!(fake.data_calls()[0].model, "account.analytic.line");
        assert_eq!(fake.data_calls()[0].args, json!([body]));
        assert_eq!(line["task"]["name"], "Migrar");
    }

    #[test]
    fn invalid_writes_are_rejected_before_odoo() {
        let fake = FakeOdoo::with_login(|_| ok(json!(1)));
        let s = session(&fake);
        let cases: Vec<(&str, Value)> = vec![
            ("project-task", json!({"name": "x"})), // falta project_id
            ("project-task", json!({"name": "x", "project_id": 0})), // id inválido
            (
                "project-task",
                json!({"name": "x", "project_id": 1, "campo": 1}),
            ), // fuera de la lista
            (
                "project-task",
                json!({"name": "x", "project_id": 1, "user_ids": [0]}),
            ),
            (
                "project-task",
                json!({"name": "x", "project_id": 1, "date_deadline": "ayer"}),
            ),
            ("timesheet", json!({"project_id": 1, "unit_amount": 25})),
            ("timesheet", json!({"project_id": 1, "unit_amount": -1})),
            ("modelo.secreto", json!({"name": "x"})),
        ];
        for (path, body) in &cases {
            assert!(post(&s, path, body).is_err(), "{path} {body}");
        }
        assert!(post(&s, "timesheet", &json!(["no objeto"])).is_err());
        assert!(put(&s, "project-task", &json!({"name": "x"})).is_err());
        assert!(put(&s, "project-task/abc", &json!({"name": "x"})).is_err());
        assert!(put(&s, "project-task/11", &json!({})).is_err());
        assert!(put(&s, "project-task/11", &json!({"project_id": "x"})).is_err());
        assert!(fake.calls().is_empty(), "nada llegó a Odoo");
    }

    #[test]
    fn a_vanished_record_after_writing_is_reported_as_ambiguous() {
        let fake = FakeOdoo::with_login(|c: &Call| match c.op.as_str() {
            "create" => ok(json!(5)),
            _ => ok(json!([])),
        });
        let err = post(
            &session(&fake),
            "timesheet",
            &json!({"project_id": 1, "unit_amount": 1}),
        )
        .unwrap_err();
        assert!(matches!(err, ApiError::Server(502, _)), "{err:?}");
    }

    #[test]
    fn deadlines_are_converted_to_utc_naive() {
        for (input, expected) in [
            ("2026-11-20T23:59:00-03:00", "2026-11-21 02:59:00"),
            ("2026-11-20T23:59:00+00:00", "2026-11-20 23:59:00"),
            ("2026-11-20T10:00:00", "2026-11-20 10:00:00"),
            ("2026-11-20", "2026-11-20 00:00:00"),
        ] {
            assert_eq!(deadline(&json!(input)).unwrap(), json!(expected), "{input}");
        }
        assert_eq!(deadline(&Value::Null).unwrap(), json!(false));
    }

    fn fields_get(names: &[&str]) -> Value {
        Value::Object(
            names
                .iter()
                .map(|n| (n.to_string(), json!({"type": "char"})))
                .collect(),
        )
    }

    #[test]
    fn me_uses_the_session_uid_and_looks_up_the_employee() {
        let fake = FakeOdoo::with_login(|c: &Call| match c.model.as_str() {
            "res.users" => ok(
                json!([{"id": 7, "name": "yo", "display_name": "Yo Mismo", "email": "otro@mail.com"}]),
            ),
            "hr.employee" => ok(json!([{"id": 8}])),
            "account.analytic.line" => ok(json!({"name": {}, "date_time": {}})),
            _ => panic!("{c:?}"),
        });
        let me = get(&session(&fake), "catalog/me", &[]).unwrap();
        assert_eq!(
            me["rows"][0],
            json!({"id": 7, "name": "Yo Mismo", "email": "otro@mail.com", "employee_id": 8,
                   "timesheet_start": true})
        );
        let calls = fake.data_calls();
        assert_eq!(calls[0].args, json!([[7]]));
        assert_eq!(calls[1].args, json!([[["user_id", "=", 7]]]));
        let user: User = serde_json::from_value(me["rows"][0].clone()).unwrap();
        assert_eq!(user.employee_id, Some(8));
        assert!(user.timesheet_start);
    }

    #[test]
    fn me_still_works_without_a_readable_employee() {
        let fake = FakeOdoo::with_login(|c: &Call| match c.model.as_str() {
            "res.users" => ok(json!([{"id": 7, "display_name": "Yo", "email": false}])),
            _ => Err(json!({"data": {"name": "odoo.exceptions.AccessError", "message": "no"}})),
        });
        let me = get(&session(&fake), "catalog/me", &[]).unwrap();
        assert_eq!(me["rows"][0]["employee_id"], Value::Null);
        assert_eq!(me["rows"][0]["name"], "Yo");
    }

    #[test]
    fn my_projects_lists_each_project_with_open_tasks_assigned_to_me() {
        let fake = FakeOdoo::with_login(|c: &Call| match (c.model.as_str(), c.op.as_str()) {
            ("project.task", "fields_get") => ok(fields_get(&["state"])),
            ("project.task", "search_read") => ok(json!([
                {"id": 1, "project_id": [4, "Beta"]}, {"id": 2, "project_id": [4, "Beta"]},
                {"id": 3, "project_id": [2, "alfa"]}, {"id": 4, "project_id": false}
            ])),
            _ => panic!("{c:?}"),
        });
        let result = get(&session(&fake), "catalog/my-projects", &[]).unwrap();
        // sin repetidos, sin tareas sueltas y ordenados por nombre
        assert_eq!(
            result["rows"],
            json!([{"id": 2, "name": "alfa"}, {"id": 4, "name": "Beta"}])
        );
        let read = fake
            .data_calls()
            .into_iter()
            .find(|c| c.op == "search_read")
            .unwrap();
        assert_eq!(
            read.args,
            json!([[
                ["user_ids", "in", [7]],
                ["state", "not in", ["1_done", "1_canceled"]]
            ]])
        );
    }

    #[test]
    fn open_filter_adapts_to_the_fields_this_odoo_has() {
        let run = |task_fields: &'static [&'static str]| {
            let fake = FakeOdoo::with_login(move |c: &Call| match c.op.as_str() {
                "fields_get" => ok(fields_get(task_fields)),
                _ => ok(json!([])),
            });
            let s = session(&fake);
            let q = q(&[("open", "true")]);
            get(&s, "project-task/search", &q).unwrap();
            // `fields_get` se pide una sola vez por modelo
            get(&s, "project-task/search", &q).unwrap();
            let calls = fake.data_calls();
            let gets = calls.iter().filter(|c| c.op == "fields_get").count();
            assert_eq!(gets, 1, "una vez por modelo");
            calls
                .into_iter()
                .filter(|c| c.op == "search_read")
                .map(|c| c.args)
                .collect::<Vec<_>>()
        };
        let reads = run(&["state"]);
        assert_eq!(
            reads[0],
            json!([[["state", "not in", ["1_done", "1_canceled"]]]])
        );
        // Odoo sin `state` pero con `is_closed`
        let reads = run(&["is_closed"]);
        assert_eq!(reads[0], json!([[["is_closed", "=", false]]]));
        // sin ninguno: no filtra (y no falla)
        assert_eq!(run(&[])[0], json!([[]]));
        // sin `open`, no se toca el dominio ni se consulta `fields_get`
        let fake = FakeOdoo::with_login(|_| ok(json!([])));
        get(&session(&fake), "project-task/search", &[]).unwrap();
        assert!(fake.data_calls().iter().all(|c| c.op != "fields_get"));
        assert!(
            get(
                &session(&fake),
                "project-task/search",
                &q(&[("open", "quizás")])
            )
            .is_err()
        );
    }

    #[test]
    fn completing_a_task_writes_the_done_state_and_validates_it() {
        let fake = FakeOdoo::with_login(|c: &Call| match c.op.as_str() {
            "fields_get" => ok(fields_get(&["state"])),
            "write" => ok(json!(true)),
            _ => ok(json!([{"id": 11, "name": "Migrar", "user_ids": [], "tag_ids": []}])),
        });
        let s = session(&fake);
        let done = put(&s, "project-task/11", &json!({"state": "1_done"})).unwrap();
        assert_eq!(done["id"], 11);
        let write = fake
            .data_calls()
            .into_iter()
            .find(|c| c.op == "write")
            .unwrap();
        assert_eq!(write.args, json!([[11], {"state": "1_done"}]));
        assert!(put(&s, "project-task/11", &json!({"state": "hecho"})).is_err());
        assert!(put(&s, "project-task/11", &json!({"state": null})).is_err());
        assert!(put(&s, "project-task/11", &json!({})).is_err());
    }

    #[test]
    fn completing_explains_when_this_odoo_has_no_state_field() {
        let fake = FakeOdoo::with_login(|c: &Call| match c.op.as_str() {
            "fields_get" => ok(fields_get(&["name"])),
            _ => panic!("no debía escribir: {c:?}"),
        });
        let err = put(
            &session(&fake),
            "project-task/11",
            &json!({"state": "1_done"}),
        )
        .unwrap_err();
        assert!(err.to_string().contains("state"), "{err}");
    }

    #[test]
    fn task_stages_are_scoped_to_the_project_when_odoo_models_it() {
        let fake = FakeOdoo::with_login(|c: &Call| match c.op.as_str() {
            "fields_get" => ok(json!({"name": {}, "project_ids": {}})),
            _ => ok(json!([{"id": 7, "name": "Nuevo", "display_name": "Nuevo"}])),
        });
        let result = get(
            &session(&fake),
            "catalog/task-stages",
            &q(&[("project_id", "4")]),
        )
        .unwrap();
        assert_eq!(result["rows"], json!([{"id": 7, "name": "Nuevo"}]));
        let read = fake
            .data_calls()
            .into_iter()
            .find(|c| c.op == "search_read")
            .unwrap();
        assert_eq!(read.model, "project.task.type");
        assert_eq!(read.args, json!([[["project_ids", "in", [4]]]]));
    }

    #[test]
    fn task_stages_fall_back_to_all_when_the_project_has_none() {
        let fake = FakeOdoo::with_login(|c: &Call| match c.op.as_str() {
            "fields_get" => ok(json!({"project_ids": {}})),
            // la primera consulta (acotada) no devuelve nada; la segunda (todos), sí
            _ if c.args == json!([[["project_ids", "in", [2]]]]) => ok(json!([])),
            _ => ok(json!([{"id": 1, "name": "Nuevo", "display_name": "Nuevo"}])),
        });
        let result = get(
            &session(&fake),
            "catalog/task-stages",
            &q(&[("project_id", "2")]),
        )
        .unwrap();
        assert_eq!(result["count"], 1);
        let reads: Vec<_> = fake
            .data_calls()
            .into_iter()
            .filter(|c| c.op == "search_read")
            .collect();
        assert_eq!(reads.len(), 2);
        assert_eq!(reads[1].args, json!([[]]));
    }

    #[test]
    fn stage_scope_is_ignored_without_the_relation_field_or_for_other_catalogs() {
        let fake = FakeOdoo::with_login(|c: &Call| match c.op.as_str() {
            "fields_get" => ok(json!({"name": {}})),
            _ => ok(json!([])),
        });
        let s = session(&fake);
        get(&s, "catalog/task-stages", &q(&[("project_id", "4")])).unwrap();
        get(&s, "catalog/projects", &q(&[("project_id", "4")])).unwrap();
        for read in fake.data_calls().iter().filter(|c| c.op == "search_read") {
            assert_eq!(read.args, json!([[]]));
        }
        assert!(get(&s, "catalog/task-stages", &q(&[("project_id", "0")])).is_err());
        assert!(get(&s, "catalog/task-stages", &q(&[("otro", "1")])).is_err());
    }

    #[test]
    fn timesheets_read_and_write_the_start_time_when_odoo_has_date_time() {
        let fake = FakeOdoo::with_login(|c: &Call| match c.op.as_str() {
            "fields_get" => ok(json!({"name": {}, "date": {}, "date_time": {}})),
            "create" => ok(json!(77)),
            _ => ok(
                json!([{"id": 77, "name": "daily", "date": "2026-11-04", "unit_amount": 6.0,
                            "date_time": "2026-11-04 13:00:00", "project_id": [4, "Alfa"],
                            "task_id": false, "employee_id": false}]),
            ),
        });
        let s = session(&fake);
        let body = json!({"project_id": 4, "unit_amount": 6, "name": "daily",
                          "date": "2026-11-04", "date_time": "2026-11-04T10:00:00-03:00"});
        let line = post(&s, "timesheet", &body).unwrap();
        let create = fake
            .data_calls()
            .into_iter()
            .find(|c| c.op == "create")
            .unwrap();
        // se guarda en UTC; la fecha va como la pidió el formulario
        assert_eq!(create.args[0]["date_time"], "2026-11-04 13:00:00");
        assert_eq!(create.args[0]["date"], "2026-11-04");
        // y se lee de vuelta (el campo se pide porque existe)
        let read = fake
            .data_calls()
            .into_iter()
            .rfind(|c| c.op == "search_read")
            .unwrap();
        assert!(
            read.kwargs["fields"]
                .as_array()
                .unwrap()
                .contains(&json!("date_time"))
        );
        assert_eq!(line["date_time"], "2026-11-04T13:00:00");
        let parsed: Timesheet = serde_json::from_value(line).unwrap();
        assert_eq!(parsed.date_time.as_deref(), Some("2026-11-04T13:00:00"));
    }

    #[test]
    fn start_time_is_refused_when_this_odoo_has_no_date_time() {
        let fake = FakeOdoo::with_login(|c: &Call| match c.op.as_str() {
            "fields_get" => ok(json!({"name": {}, "date": {}})),
            _ => panic!("no debía escribir: {c:?}"),
        });
        let body =
            json!({"project_id": 4, "unit_amount": 6, "date_time": "2026-11-04T10:00:00-03:00"});
        let err = post(&session(&fake), "timesheet", &body).unwrap_err();
        assert!(err.to_string().contains("date_time"), "{err}");
        // sin el campo no se pide, ni se rompe la lectura
        let fake = FakeOdoo::with_login(|c: &Call| match c.op.as_str() {
            "fields_get" => ok(json!({"name": {}})),
            _ => ok(json!([])),
        });
        get(&session(&fake), "timesheet/search", &[]).unwrap();
        let read = fake
            .data_calls()
            .into_iter()
            .find(|c| c.op == "search_read")
            .unwrap();
        assert!(
            !read.kwargs["fields"]
                .as_array()
                .unwrap()
                .contains(&json!("date_time"))
        );
    }
}
