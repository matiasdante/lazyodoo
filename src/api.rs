//! Fachada que usa el resto de la app: rutas, filtros, bodies y JSON simples.
//! Cada pedido se resuelve contra Odoo en `routes` por medio de `odoo::Session`.

use std::fmt;
use std::time::Duration;

use serde_json::Value;

use crate::config::Config;
use crate::i18n::{tr, trf};
use crate::odoo::Session;
use crate::routes;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, PartialEq)]
pub enum ApiError {
    /// Odoo rechazó usuario, clave o base de datos.
    Unauthorized,
    /// Usuario portal o sin permisos sobre la operación.
    Forbidden(String),
    Timeout(u64),
    Network(String),
    /// Respondió algo, pero no es Odoo.
    NotApi,
    Server(u16, String),
    /// Respuesta o pedido con un formato que no esperamos.
    BadResponse(String),
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unauthorized => f.write_str(tr("Invalid Odoo user, key or database.")),
            Self::Forbidden(detail) => {
                write!(
                    f,
                    "{}",
                    trf("Access denied: {detail}", &[("detail", detail)])
                )
            }
            Self::Timeout(secs) => write!(
                f,
                "{}",
                trf(
                    "No response in {secs}s. Check the URL and your connection.",
                    &[("secs", &secs.to_string())]
                )
            ),
            Self::Network(msg) => {
                write!(f, "{}", trf("Could not connect: {msg}", &[("msg", msg)]))
            }
            Self::NotApi => f.write_str(tr(
                "The URL answered, but it does not look like Odoo. Check the domain.",
            )),
            Self::Server(code, detail) => write!(
                f,
                "{}",
                trf(
                    "Odoo answered {code}: {detail}",
                    &[("code", &code.to_string()), ("detail", detail)]
                )
            ),
            Self::BadResponse(msg) => write!(
                f,
                "{}",
                trf("Unexpected response from Odoo: {msg}", &[("msg", msg)])
            ),
        }
    }
}

#[derive(Clone)]
pub struct Client {
    session: Session,
}

impl Client {
    pub fn new(config: &Config) -> Result<Self, ApiError> {
        Self::with_timeout(config, DEFAULT_TIMEOUT)
    }

    pub fn with_timeout(config: &Config, timeout: Duration) -> Result<Self, ApiError> {
        Ok(Self {
            session: Session::new(config, timeout)?,
        })
    }

    /// Prueba el camino completo: red, base, credenciales y que
    /// el usuario sea interno.
    pub fn validate(&self) -> Result<(), ApiError> {
        self.session.validate()
    }

    pub fn get_json(&self, path: &str, query: &[(&str, String)]) -> Result<Value, ApiError> {
        routes::get(&self.session, path, query)
    }

    /// Alta. No se reintenta nunca: si falla la red después de enviar, el
    /// cambio puede haber quedado aplicado.
    pub fn post_json(&self, path: &str, body: &Value) -> Result<Value, ApiError> {
        routes::post(&self.session, path, body)
    }

    /// Modificación. Tampoco se reintenta.
    pub fn put_json(&self, path: &str, body: &Value) -> Result<Value, ApiError> {
        routes::put(&self.session, path, body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{FakeOdoo, ok};
    use serde_json::json;

    fn client(fake: &FakeOdoo) -> Client {
        let config = Config::new(&fake.url, "odoo", "me@example.com", "key").unwrap();
        Client::with_timeout(&config, Duration::from_secs(2)).unwrap()
    }

    #[test]
    fn validate_authenticates_against_odoo() {
        let fake = FakeOdoo::with_login(|_| ok(json!([])));
        assert_eq!(client(&fake).validate(), Ok(()));
    }

    #[test]
    fn facade_resolves_requests_against_odoo() {
        let fake = FakeOdoo::with_login(|c| match c.op.as_str() {
            "create" => ok(json!(77)),
            "write" => ok(json!(true)),
            _ => ok(
                json!([{"id": 77, "name": "x", "project_id": [1, "Alfa"], "user_ids": [],
                            "tag_ids": []}]),
            ),
        });
        let client = client(&fake);
        let list = client
            .get_json("project-task/search", &[("limit", "5".into())])
            .unwrap();
        assert_eq!(list["count"], 1);
        let created = client
            .post_json("project-task", &json!({"name": "x", "project_id": 1}))
            .unwrap();
        assert_eq!(created["id"], 77);
        let updated = client
            .put_json("project-task/77", &json!({"name": "y"}))
            .unwrap();
        assert_eq!(updated["project"]["name"], "Alfa");
    }

    #[test]
    fn error_messages_name_the_problem() {
        assert!(ApiError::Unauthorized.to_string().contains("database"));
        assert!(ApiError::NotApi.to_string().contains("Odoo"));
        assert!(ApiError::Timeout(5).to_string().contains("5s"));
    }
}
