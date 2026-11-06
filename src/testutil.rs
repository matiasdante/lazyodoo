//! Odoo simulado para tests: un servidor HTTP mínimo que habla JSON-RPC y
//! registra cada llamada.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;

use serde_json::{Value, json};

#[derive(Debug, Clone)]
pub struct Call {
    pub service: String,
    /// `authenticate`, `execute_kw`, …
    pub method: String,
    /// En `execute_kw`: modelo, método ORM, argumentos y kwargs.
    pub model: String,
    pub op: String,
    pub args: Value,
    pub kwargs: Value,
    /// Cuerpo JSON-RPC completo.
    pub raw: Value,
}

impl Call {
    fn parse(raw: Value) -> Self {
        let params = &raw["params"];
        let args = params["args"].clone();
        let service = params["service"].as_str().unwrap_or("").to_string();
        let method = params["method"].as_str().unwrap_or("").to_string();
        let text = |v: &Value| v.as_str().unwrap_or("").to_string();
        if service == "object" {
            Self {
                model: text(&args[3]),
                op: text(&args[4]),
                kwargs: args[6].clone(),
                args: args[5].clone(),
                service,
                method,
                raw,
            }
        } else {
            Self {
                model: String::new(),
                op: String::new(),
                kwargs: Value::Null,
                args,
                service,
                method,
                raw,
            }
        }
    }

    /// Autenticación y chequeo de usuario interno (no son datos de negocio).
    fn is_session_setup(&self) -> bool {
        self.method == "authenticate"
            || (self.op == "read"
                && self.model == "res.users"
                && self.kwargs["fields"] == json!(["share"]))
    }
}

pub fn ok(value: Value) -> Result<Value, Value> {
    Ok(value)
}

pub struct FakeOdoo {
    pub url: String,
    calls: Arc<Mutex<Vec<Call>>>,
}

impl FakeOdoo {
    /// El handler ve todas las llamadas, incluida la autenticación.
    pub fn start(handler: impl Fn(&Call) -> Result<Value, Value> + Send + 'static) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("puerto local");
        let url = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
        let calls = Arc::new(Mutex::new(Vec::new()));
        let log = calls.clone();
        thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                serve(stream, &handler, &log);
            }
        });
        Self { url, calls }
    }

    /// Autentica como el usuario 7 (interno) y el handler solo ve los datos.
    pub fn with_login(handler: impl Fn(&Call) -> Result<Value, Value> + Send + 'static) -> Self {
        Self::start(move |call| {
            if call.method == "authenticate" {
                ok(json!(7))
            } else if call.is_session_setup() {
                ok(json!([{"id": 7, "share": false}]))
            } else {
                handler(call)
            }
        })
    }

    pub fn calls(&self) -> Vec<Call> {
        self.calls.lock().unwrap().clone()
    }

    /// Solo las llamadas de datos (sin autenticación).
    pub fn data_calls(&self) -> Vec<Call> {
        self.calls()
            .into_iter()
            .filter(|c| !c.is_session_setup())
            .collect()
    }
}

fn serve(
    mut stream: TcpStream,
    handler: &impl Fn(&Call) -> Result<Value, Value>,
    log: &Arc<Mutex<Vec<Call>>>,
) {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let header_end = loop {
        let n = stream.read(&mut chunk).unwrap_or(0);
        if n == 0 {
            return;
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break pos + 4;
        }
    };
    let head = String::from_utf8_lossy(&buf[..header_end]).to_string();
    let headers: Vec<(String, String)> = head
        .lines()
        .skip(1)
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect();
    let length = headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, v)| v.parse::<usize>().ok())
        .unwrap_or(0);
    while buf.len() < header_end + length {
        let n = stream.read(&mut chunk).unwrap_or(0);
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    let raw: Value = serde_json::from_slice(&buf[header_end..]).unwrap_or(Value::Null);
    let call = Call::parse(raw.clone());
    log.lock().unwrap().push(call.clone());
    let reply = match handler(&call) {
        Ok(result) => json!({"jsonrpc": "2.0", "id": raw["id"], "result": result}),
        Err(error) => json!({"jsonrpc": "2.0", "id": raw["id"], "error": error}),
    };
    let body = reply.to_string();
    let _ = write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
}
