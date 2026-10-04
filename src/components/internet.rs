//! The `internet` component: HTTP(S) requests performed on a background thread,
//! mirroring the OpenComputers internet card's streaming request handle.

use super::{arg_string, multi1, Component, MethodInfo};
use mlua::{Lua, MultiValue, Value};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// OpenComputers clamps every `read` call to this many bytes.
const MAX_READ_BUFFER: usize = 2048;

#[derive(Default)]
struct HttpState {
    done: bool,
    error: Option<String>,
    response: Option<ResponseInfo>,
    body: Vec<u8>,
    pos: usize,
    closed: bool,
}

struct ResponseInfo {
    code: i32,
    message: String,
    headers: Vec<(String, Vec<String>)>,
}

/// The internet card. HTTP runs asynchronously so the machine keeps rendering
/// while a request is in flight, exactly like the real component.
pub struct Internet {
    pub enabled: bool,
    pub timeout: f64,
    pub user_agent: String,
    pub tcp_enabled: bool,
}

impl Internet {
    pub fn new(enabled: bool, timeout: f64, user_agent: String, tcp_enabled: bool) -> Internet {
        Internet {
            enabled,
            timeout,
            user_agent,
            tcp_enabled,
        }
    }

    fn request(&self, lua: &Lua, args: &[Value]) -> mlua::Result<MultiValue> {
        let url = arg_string(args, 0)?;
        if !self.enabled {
            return err(lua, "internet access is unavailable");
        }
        if let Err(reason) = validate_url(&url) {
            return err(lua, &reason);
        }

        let post = match args.get(1) {
            Some(Value::String(s)) => Some(s.as_bytes().to_vec()),
            _ => None,
        };
        let headers = parse_headers(args.get(2))?;
        let method = match args.get(3) {
            Some(Value::String(s)) => Some(s.to_string_lossy().to_string()),
            _ => None,
        };

        let state = Arc::new(Mutex::new(HttpState::default()));
        let worker = state.clone();
        let timeout = self.timeout;
        let user_agent = self.user_agent.clone();
        let url_owned = url.clone();
        std::thread::spawn(move || {
            let outcome = perform_request(&url_owned, method, post, headers, timeout, &user_agent);
            let mut state = worker.lock().unwrap();
            state.done = true;
            match outcome {
                Ok((info, body)) => {
                    // OpenComputers never exposes the error stream: reading a
                    // response with a >= 400 status fails, but `response()`
                    // still reports the status code and headers.
                    if info.code >= 400 {
                        state.error = Some(format!("{} {}", info.code, info.message));
                    }
                    state.response = Some(info);
                    state.body = body;
                }
                Err(reason) => state.error = Some(reason),
            }
        });

        let handle = lua.create_table()?;

        let read_state = state.clone();
        handle.set(
            "read",
            lua.create_function(move |lua, n: Option<i64>| {
                let mut state = read_state.lock().unwrap();
                if let Some(reason) = &state.error {
                    return err(lua, reason);
                }
                if !state.done {
                    // Not ready yet: an empty string tells the caller to retry.
                    return Ok(multi1(Value::String(lua.create_string("")?)));
                }
                if state.pos >= state.body.len() {
                    return Ok(multi1(Value::Nil));
                }
                let limit = n.unwrap_or(i64::MAX).max(0) as usize;
                let limit = limit.min(MAX_READ_BUFFER);
                let end = (state.pos + limit).min(state.body.len());
                let chunk = state.body[state.pos..end].to_vec();
                state.pos = end;
                Ok(multi1(Value::String(lua.create_string(&chunk)?)))
            })?,
        )?;

        let response_state = state.clone();
        handle.set(
            "response",
            lua.create_function(move |lua, ()| {
                let state = response_state.lock().unwrap();
                let mut out = MultiValue::new();
                match &state.response {
                    Some(info) => {
                        out.push_back(Value::Integer(info.code as i64));
                        out.push_back(Value::String(lua.create_string(&info.message)?));
                        let table = lua.create_table()?;
                        for (name, values) in &info.headers {
                            let list = lua.create_table()?;
                            for (index, value) in values.iter().enumerate() {
                                list.set(index + 1, value.as_str())?;
                            }
                            table.set(name.as_str(), list)?;
                        }
                        out.push_back(Value::Table(table));
                    }
                    None => out.push_back(Value::Nil),
                }
                Ok(out)
            })?,
        )?;

        let finish_state = state.clone();
        handle.set(
            "finishConnect",
            lua.create_function(move |lua, ()| {
                let state = finish_state.lock().unwrap();
                if let Some(reason) = &state.error {
                    return err(lua, reason);
                }
                Ok(multi1(Value::Boolean(state.done)))
            })?,
        )?;

        handle.set(
            "close",
            lua.create_function(move |_, ()| {
                let mut state = state.lock().unwrap();
                state.closed = true;
                state.body.clear();
                Ok(())
            })?,
        )?;

        Ok(multi1(Value::Table(handle)))
    }
}

impl Component for Internet {
    fn type_name(&self) -> &'static str {
        "internet"
    }

    fn slot(&self) -> &'static str {
        "card"
    }

    fn methods(&self) -> &'static [MethodInfo] {
        METHODS
    }

    fn invoke(&self, lua: &Lua, method: &str, args: &[Value]) -> mlua::Result<MultiValue> {
        match method {
            "isHttpEnabled" => Ok(multi1(Value::Boolean(self.enabled))),
            "isTcpEnabled" => Ok(multi1(Value::Boolean(self.tcp_enabled))),
            "request" => self.request(lua, args),
            "connect" => err(lua, "tcp connections are unavailable"),
            _ => Err(mlua::Error::RuntimeError("no such method".into())),
        }
    }

    fn device_info(&self) -> Vec<(String, String)> {
        vec![
            ("class".into(), "card".into()),
            ("description".into(), "Internet Card".into()),
            ("vendor".into(), "MightyPirates GmbH & Co. KG".into()),
            ("product".into(), "Tier 2 Internet Card".into()),
        ]
    }
}

const METHODS: &[MethodInfo] = &[
    MethodInfo { name: "isHttpEnabled", direct: true, doc: "function():boolean -- Returns whether HTTP requests can be made (config setting)." },
    MethodInfo { name: "request", direct: false, doc: "function(url:string[, postData:string[, headers:table[, method:string]]]):userdata -- Starts an HTTP request." },
    MethodInfo { name: "isTcpEnabled", direct: true, doc: "function():boolean -- Returns whether TCP connections can be made (config setting)." },
    MethodInfo { name: "connect", direct: false, doc: "function(address:string[, port:number]):userdata -- Opens a new TCP connection. Returns the handle of the connection." },
];

// ---------------------------------------------------------------------------

fn err<'a>(lua: &'a Lua, reason: &str) -> mlua::Result<MultiValue> {
    let mut out = MultiValue::new();
    out.push_back(Value::Nil);
    out.push_back(Value::String(lua.create_string(reason)?));
    Ok(out)
}

fn validate_url(url: &str) -> Result<(), String> {
    match url.find("://") {
        Some(index) => match url[..index].to_ascii_lowercase().as_str() {
            "http" | "https" => Ok(()),
            _ => Err("unsupported protocol".into()),
        },
        None => Err("invalid address".into()),
    }
}

fn parse_headers(value: Option<&Value>) -> mlua::Result<Vec<(String, String)>> {
    let mut headers = Vec::new();
    if let Some(Value::Table(table)) = value {
        for pair in table.pairs::<Value, Value>() {
            let (key, value) = pair?;
            let key = match key {
                Value::String(s) => s.to_string_lossy().to_string(),
                _ => continue,
            };
            let value = match value {
                Value::String(s) => s.to_string_lossy().to_string(),
                Value::Integer(i) => i.to_string(),
                Value::Number(n) => n.to_string(),
                Value::Boolean(b) => b.to_string(),
                _ => continue,
            };
            headers.push((key, value));
        }
    }
    Ok(headers)
}

fn perform_request(
    url: &str,
    method: Option<String>,
    post: Option<Vec<u8>>,
    headers: Vec<(String, String)>,
    timeout: f64,
    user_agent: &str,
) -> Result<(ResponseInfo, Vec<u8>), String> {
    let mut config = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .user_agent(user_agent);
    if timeout > 0.0 {
        config = config.timeout_global(Some(Duration::from_secs_f64(timeout)));
    }
    let agent = ureq::Agent::new_with_config(config.build());

    let method = method.unwrap_or_else(|| {
        if post.is_some() {
            "POST".to_string()
        } else {
            "GET".to_string()
        }
    });
    let body = post.unwrap_or_default();

    let method =
        ureq::http::Method::from_bytes(method.as_bytes()).map_err(|e| e.to_string())?;
    let mut builder = ureq::http::Request::builder().method(method).uri(url);
    for (name, value) in &headers {
        builder = builder.header(name.as_str(), value.as_str());
    }
    let request = builder.body(body).map_err(|e| e.to_string())?;
    let response = agent.run(request).map_err(|e| e.to_string())?;

    let status = response.status();
    let code = status.as_u16() as i32;
    let message = status.canonical_reason().unwrap_or("").to_string();

    let mut collected: Vec<(String, Vec<String>)> = Vec::new();
    for (name, value) in response.headers().iter() {
        let name = name.as_str().to_string();
        let value = String::from_utf8_lossy(value.as_bytes()).to_string();
        match collected.iter_mut().find(|(existing, _)| *existing == name) {
            Some((_, values)) => values.push(value),
            None => collected.push((name, vec![value])),
        }
    }

    let mut body = response.into_body();
    let bytes = body.read_to_vec().map_err(|e| e.to_string())?;

    Ok((
        ResponseInfo {
            code,
            message,
            headers: collected,
        },
        bytes,
    ))
}
