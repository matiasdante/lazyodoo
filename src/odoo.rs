//! Sesión JSON-RPC contra Odoo (`/jsonrpc`): arma el envelope, valida la
//! respuesta, autentica una vez (rechazando usuarios portal) y mapea los
//! errores de Odoo.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use reqwest::blocking::Client as Http;
use reqwest::header::CONTENT_TYPE;
use reqwest::redirect::Policy;
use serde_json::{Value, json};

use crate::api::ApiError;
use crate::config::Config;
use crate::models::sanitize;

#[derive(Clone)]
pub struct Session {
    http: Http,
    url: String,
    database: String,
    login: String,
    secret: String,
    timeout: Duration,
    /// Compartido entre clones: se autentica una sola vez por sesión.
    uid: Arc<Mutex<Option<i64>>>,
    next_id: Arc<AtomicU64>,
    /// Campos de cada modelo (de `fields_get`), para detectar capacidades.
    fields: Arc<Mutex<HashMap<String, HashSet<String>>>>,
}

impl fmt::Debug for Session {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Session")
            .field("url", &self.url)
            .field("database", &self.database)
            .field("login", &self.login)
            .finish_non_exhaustive()
    }
}

impl Session {
    pub fn new(config: &Config, timeout: Duration) -> Result<Self, ApiError> {
        let http = Http::builder()
            .timeout(timeout)
            // Odoo no redirige en /jsonrpc: un redirect es otra cosa y no se sigue
            .redirect(Policy::none())
            .build()
            .map_err(|e| ApiError::Network(e.without_url().to_string()))?;
        Ok(Self {
            http,
            url: format!("{}/jsonrpc", config.base_url),
            database: config.database.clone(),
            login: config.login.clone(),
            secret: config.secret.clone(),
            timeout,
            uid: Arc::new(Mutex::new(None)),
            next_id: Arc::new(AtomicU64::new(0)),
            fields: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    /// Un `call` de JSON-RPC. Devuelve `result`; los errores de Odoo se mapean.
    pub fn call(&self, service: &str, method: &str, args: Value) -> Result<Value, ApiError> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst) + 1;
        let body = json!({
            "jsonrpc": "2.0",
            "method": "call",
            "params": {"service": service, "method": method, "args": args},
            "id": id,
        });
        let response = self
            .http
            .post(&self.url)
            .json(&body)
            .send()
            .map_err(|e| self.transport_error(e))?;
        let status = response.status();
        if status.is_redirection() {
            return Err(ApiError::NotApi);
        }
        let is_json = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.contains("json"));
        // Odoo responde siempre 200 (incluso sus errores); un proxy puede cortar
        // con 401/403 o con una página HTML.
        match status.as_u16() {
            200 => {}
            401 | 403 => return Err(ApiError::NotApi),
            404 => return Err(ApiError::NotApi),
            code => {
                return Err(ApiError::Server(
                    code,
                    "respuesta HTTP inesperada de Odoo".into(),
                ));
            }
        }
        let data: Value = if is_json {
            response.json().map_err(|_| ApiError::NotApi)?
        } else {
            return Err(ApiError::NotApi);
        };
        if data.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
            return Err(ApiError::NotApi);
        }
        if data.get("id").and_then(Value::as_u64) != Some(id) {
            return Err(ApiError::BadResponse("respuesta JSON-RPC inválida".into()));
        }
        if let Some(error) = data.get("error") {
            return Err(odoo_error(error));
        }
        data.get("result")
            .cloned()
            .ok_or_else(|| ApiError::BadResponse("respuesta JSON-RPC incompleta".into()))
    }

    /// `uid` autenticado, pidiéndolo a Odoo la primera vez.
    pub fn uid(&self) -> Result<i64, ApiError> {
        let mut cached = self.lock_uid();
        if let Some(uid) = *cached {
            return Ok(uid);
        }
        let uid = self.authenticate()?;
        *cached = Some(uid);
        Ok(uid)
    }

    fn lock_uid(&self) -> MutexGuard<'_, Option<i64>> {
        self.uid.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn authenticate(&self) -> Result<i64, ApiError> {
        let uid = self.call(
            "common",
            "authenticate",
            json!([self.database, self.login, self.secret, {}]),
        )?;
        let uid = uid
            .as_i64()
            .filter(|uid| *uid > 0)
            .ok_or(ApiError::Unauthorized)?;
        let users = self.call(
            "object",
            "execute_kw",
            json!([
                self.database,
                uid,
                self.secret,
                "res.users",
                "read",
                [[uid]],
                {"fields": ["share"]}
            ]),
        )?;
        let share = users
            .get(0)
            .and_then(|u| u.get("share"))
            .and_then(Value::as_bool);
        match share {
            Some(false) => Ok(uid),
            Some(true) => Err(ApiError::Forbidden(
                "Solo los usuarios internos pueden usar lazyodoo.".into(),
            )),
            None => Err(ApiError::Forbidden("No se pudo leer tu usuario.".into())),
        }
    }

    /// Prueba el camino completo (red, base, credenciales y tipo
    /// de usuario). Fuerza una autenticación nueva.
    pub fn validate(&self) -> Result<(), ApiError> {
        *self.lock_uid() = None;
        self.uid().map(|_| ())
    }

    /// `execute_kw` con el usuario autenticado.
    pub fn execute(
        &self,
        model: &str,
        method: &str,
        args: Value,
        kwargs: Value,
    ) -> Result<Value, ApiError> {
        let uid = self.uid()?;
        let result = self.call(
            "object",
            "execute_kw",
            json!([self.database, uid, self.secret, model, method, args, kwargs]),
        );
        if matches!(result, Err(ApiError::Unauthorized)) {
            // la clave pudo revocarse: la próxima vez se autentica de nuevo
            *self.lock_uid() = None;
        }
        result
    }

    /// ¿El modelo tiene ese campo? Se consulta `fields_get` una vez por modelo.
    /// Sirve para no depender de campos que varían entre instalaciones (por
    /// ejemplo `state` en tareas).
    pub fn has_field(&self, model: &str, field: &str) -> bool {
        let mut cache = self.fields.lock().unwrap_or_else(|e| e.into_inner());
        if !cache.contains_key(model) {
            match self.execute(
                model,
                "fields_get",
                json!([]),
                json!({"attributes": ["type"]}),
            ) {
                Ok(Value::Object(fields)) => {
                    cache.insert(model.to_string(), fields.keys().cloned().collect());
                }
                // sin permiso o sin el modelo: se asume que no está (y no se cachea)
                _ => return false,
            }
        }
        cache.get(model).is_some_and(|f| f.contains(field))
    }

    fn transport_error(&self, e: reqwest::Error) -> ApiError {
        if e.is_timeout() {
            ApiError::Timeout(self.timeout.as_secs())
        } else {
            ApiError::Network(e.without_url().to_string())
        }
    }
}

/// Mapeo de `odoo.exceptions.*`, mostrando el mensaje de Odoo: el que lo lee es
/// el dueño de la sesión.
fn odoo_error(error: &Value) -> ApiError {
    let data = error.get("data");
    let name = data
        .and_then(|d| d.get("name"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let message = data
        .and_then(|d| d.get("message"))
        .or_else(|| error.get("message"))
        .and_then(Value::as_str)
        .map(first_line)
        .unwrap_or_default();
    match name {
        "odoo.exceptions.AccessDenied" => ApiError::Unauthorized,
        "odoo.exceptions.AccessError" => ApiError::Forbidden(if message.is_empty() {
            "No tenés permiso para esa operación.".into()
        } else {
            message
        }),
        "odoo.exceptions.MissingError" => ApiError::Server(404, message),
        "odoo.exceptions.ValidationError" | "odoo.exceptions.UserError" => {
            ApiError::Server(400, message)
        }
        _ => ApiError::Server(
            502,
            format!("Odoo no pudo completar la operación. {message}")
                .trim_end()
                .to_string(),
        ),
    }
}

fn first_line(text: &str) -> String {
    let line = sanitize(text)
        .lines()
        .next()
        .unwrap_or("")
        .trim()
        .to_string();
    line.chars().take(300).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{FakeOdoo, ok};
    use mockito::Server;

    fn config(url: &str) -> Config {
        Config::new(url, "odoo", "me@example.com", "key").unwrap()
    }

    fn session(url: &str) -> Session {
        Session::new(&config(url), Duration::from_secs(2)).unwrap()
    }

    fn internal_user(call: &crate::testutil::Call) -> Option<Result<Value, Value>> {
        match (call.service.as_str(), call.method.as_str()) {
            ("common", "authenticate") => Some(ok(json!(7))),
            _ if call.op == "read" && call.model == "res.users" => {
                Some(ok(json!([{"id": 7, "share": false}])))
            }
            _ => None,
        }
    }

    #[test]
    fn authenticates_once_with_internal_check() {
        let fake = FakeOdoo::start(|c| internal_user(c).unwrap_or_else(|| ok(json!([{"id": 1}]))));
        let s = session(&fake.url);
        s.execute("project.task", "search_read", json!([[]]), json!({}))
            .unwrap();
        s.execute("project.task", "search_read", json!([[]]), json!({}))
            .unwrap();
        let calls = fake.calls();
        assert_eq!(
            calls.iter().filter(|c| c.method == "authenticate").count(),
            1
        );
        // authenticate(db, login, secret, {}) y execute_kw(db, uid, secret, ...)
        assert_eq!(calls[0].args, json!(["odoo", "me@example.com", "key", {}]));
        let exec = &calls[2].raw["params"]["args"];
        assert_eq!(
            (&exec[0], &exec[1], &exec[2]),
            (&json!("odoo"), &json!(7), &json!("key"))
        );
    }

    #[test]
    fn false_uid_is_bad_credentials() {
        let fake = FakeOdoo::start(|_| ok(json!(false)));
        assert_eq!(session(&fake.url).validate(), Err(ApiError::Unauthorized));
    }

    #[test]
    fn portal_user_is_rejected() {
        let fake = FakeOdoo::start(|c| {
            if c.method == "authenticate" {
                ok(json!(9))
            } else {
                ok(json!([{"id": 9, "share": true}]))
            }
        });
        let err = session(&fake.url).validate().unwrap_err();
        assert!(
            matches!(&err, ApiError::Forbidden(m) if m.contains("internos")),
            "{err:?}"
        );
    }

    #[test]
    fn odoo_errors_are_mapped_with_their_message() {
        let cases = [
            (
                "odoo.exceptions.AccessDenied",
                "Access Denied",
                ApiError::Unauthorized,
            ),
            (
                "odoo.exceptions.AccessError",
                "No podés ver esto\nTraceback…",
                ApiError::Forbidden("No podés ver esto".into()),
            ),
            (
                "odoo.exceptions.ValidationError",
                "Campo obligatorio",
                ApiError::Server(400, "Campo obligatorio".into()),
            ),
            (
                "odoo.exceptions.UserError",
                "No se puede",
                ApiError::Server(400, "No se puede".into()),
            ),
            (
                "odoo.exceptions.MissingError",
                "Ya no existe",
                ApiError::Server(404, "Ya no existe".into()),
            ),
            (
                "psycopg2.OperationalError",
                "FATAL: database \"x\" does not exist",
                ApiError::Server(
                    502,
                    "Odoo no pudo completar la operación. FATAL: database \"x\" does not exist"
                        .into(),
                ),
            ),
        ];
        for (name, message, expected) in cases {
            let fake = FakeOdoo::start(move |_| {
                Err(json!({"code": 200, "message": "Odoo Server Error",
                           "data": {"name": name, "message": message}}))
            });
            assert_eq!(session(&fake.url).validate(), Err(expected), "{name}");
        }
    }

    #[test]
    fn error_messages_drop_control_characters() {
        let fake = FakeOdoo::start(|_| {
            Err(json!({"data": {"name": "odoo.exceptions.UserError", "message": "a\u{1b}[31mb"}}))
        });
        let err = session(&fake.url).validate().unwrap_err();
        assert!(!err.to_string().contains('\u{1b}'));
    }

    #[test]
    fn access_denied_during_a_call_forces_reauthentication() {
        let denied = Arc::new(Mutex::new(true));
        let flag = denied.clone();
        let fake = FakeOdoo::start(move |c| {
            if let Some(r) = internal_user(c) {
                return r;
            }
            let mut denied = flag.lock().unwrap();
            if *denied {
                *denied = false;
                Err(json!({"data": {"name": "odoo.exceptions.AccessDenied", "message": "x"}}))
            } else {
                ok(json!([]))
            }
        });
        let s = session(&fake.url);
        let call = |s: &Session| s.execute("project.task", "search_read", json!([[]]), json!({}));
        assert_eq!(call(&s), Err(ApiError::Unauthorized));
        assert!(call(&s).is_ok());
        let auths = fake
            .calls()
            .iter()
            .filter(|c| c.method == "authenticate")
            .count();
        assert_eq!(auths, 2);
    }

    #[test]
    fn mismatched_or_incomplete_envelopes_are_rejected() {
        for (body, expected) in [
            (
                r#"{"jsonrpc":"2.0","id":999,"result":1}"#,
                ApiError::BadResponse("respuesta JSON-RPC inválida".into()),
            ),
            (
                r#"{"jsonrpc":"2.0","id":1}"#,
                ApiError::BadResponse("respuesta JSON-RPC incompleta".into()),
            ),
            (r#"{"otra":"cosa"}"#, ApiError::NotApi),
        ] {
            let mut server = Server::new();
            server
                .mock("POST", "/jsonrpc")
                .with_header("content-type", "application/json")
                .with_body(body)
                .create();
            assert_eq!(
                session(&server.url()).call("common", "version", json!([])),
                Err(expected),
                "{body}"
            );
        }
    }

    #[test]
    fn redirects_blocks_and_non_odoo_responses_are_not_odoo() {
        let mut server = Server::new();
        server
            .mock("POST", "/jsonrpc")
            .with_status(302)
            .with_header("location", "https://example.com/login")
            .create();
        assert_eq!(session(&server.url()).validate(), Err(ApiError::NotApi));

        let mut server = Server::new();
        server
            .mock("POST", "/jsonrpc")
            .with_status(403)
            .with_header("content-type", "text/html")
            .with_body("<html>Forbidden</html>")
            .create();
        assert_eq!(session(&server.url()).validate(), Err(ApiError::NotApi));

        let mut server = Server::new();
        server
            .mock("POST", "/jsonrpc")
            .with_header("content-type", "text/html")
            .with_body("<html>hola</html>")
            .create();
        assert_eq!(session(&server.url()).validate(), Err(ApiError::NotApi));

        let mut server = Server::new();
        server.mock("POST", "/jsonrpc").with_status(404).create();
        assert_eq!(session(&server.url()).validate(), Err(ApiError::NotApi));

        let mut server = Server::new();
        server.mock("POST", "/jsonrpc").with_status(500).create();
        assert!(matches!(
            session(&server.url()).validate(),
            Err(ApiError::Server(500, _))
        ));
    }

    #[test]
    fn closed_port_and_silent_server_fail_cleanly() {
        let port = {
            let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            l.local_addr().unwrap().port()
        };
        let err = session(&format!("http://127.0.0.1:{port}"))
            .validate()
            .unwrap_err();
        assert!(matches!(err, ApiError::Network(_)), "{err:?}");

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
        let s = Session::new(&config(&url), Duration::from_millis(300)).unwrap();
        assert_eq!(s.validate(), Err(ApiError::Timeout(0)));
        drop(listener);
    }

    #[test]
    fn errors_and_debug_never_contain_secrets() {
        let fake = FakeOdoo::start(|_| Err(json!({"data": {"name": "x.Y", "message": "boom"}})));
        let s = session(&fake.url);
        let text = format!("{:?} {:?}", s, s.validate());
        assert!(!text.contains("key"), "{text}");
    }
}
