//! Component trait and registry. New component types drop in by implementing
//! [`Component`] and registering an instance.

pub mod computer;
pub mod eeprom;
pub mod filesystem;
pub mod gpu;
pub mod keyboard;
pub mod screen;

use mlua::{Lua, MultiValue, Value};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

/// A machine signal, the OpenComputers equivalent of an event.
#[derive(Clone, Debug)]
pub enum SignalArg {
    Nil,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
}

#[derive(Clone, Debug)]
pub struct Signal {
    pub name: String,
    pub args: Vec<SignalArg>,
}

impl Signal {
    pub fn new(name: impl Into<String>) -> Signal {
        Signal {
            name: name.into(),
            args: Vec::new(),
        }
    }

    pub fn with(mut self, arg: SignalArg) -> Signal {
        self.args.push(arg);
        self
    }

    pub fn to_multi(&self, lua: &Lua) -> mlua::Result<MultiValue> {
        let mut out = MultiValue::new();
        out.push_back(Value::String(lua.create_string(&self.name)?));
        for arg in &self.args {
            out.push_back(match arg {
                SignalArg::Nil => Value::Nil,
                SignalArg::Bool(b) => Value::Boolean(*b),
                SignalArg::Int(i) => Value::Integer(*i),
                SignalArg::Float(f) => Value::Number(*f),
                SignalArg::Str(s) => Value::String(lua.create_string(s)?),
            });
        }
        Ok(out)
    }
}

pub type SignalQueue = Rc<RefCell<VecDeque<Signal>>>;

pub fn new_signal_queue() -> SignalQueue {
    Rc::new(RefCell::new(VecDeque::new()))
}

pub struct MethodInfo {
    pub name: &'static str,
    pub direct: bool,
    pub doc: &'static str,
}

pub trait Component {
    fn type_name(&self) -> &'static str;
    fn slot(&self) -> &'static str;
    fn methods(&self) -> &'static [MethodInfo];
    fn invoke(&self, lua: &Lua, method: &str, args: &[Value]) -> mlua::Result<MultiValue>;

    fn device_info(&self) -> Vec<(String, String)> {
        Vec::new()
    }

    fn has_method(&self, method: &str) -> bool {
        self.methods().iter().any(|m| m.name == method)
    }
}

pub struct RegistryEntry {
    pub address: String,
    pub type_name: String,
    pub slot: String,
    pub component: Rc<dyn Component>,
}

#[derive(Default)]
pub struct Registry {
    entries: Vec<RegistryEntry>,
    next: u64,
}

impl Registry {
    pub fn new() -> Registry {
        Registry {
            entries: Vec::new(),
            next: 1,
        }
    }

    pub fn add(&mut self, component: Rc<dyn Component>) -> String {
        let address = self.next_address();
        self.add_with_address(address.clone(), component);
        address
    }

    pub fn next_address(&mut self) -> String {
        let address = generate_address(self.next);
        self.next += 1;
        address
    }

    pub fn add_with_address(&mut self, address: String, component: Rc<dyn Component>) {
        self.entries.push(RegistryEntry {
            address,
            type_name: component.type_name().to_string(),
            slot: component.slot().to_string(),
            component,
        });
    }

    pub fn entries(&self) -> &[RegistryEntry] {
        &self.entries
    }

    pub fn find(&self, address: &str) -> Option<&RegistryEntry> {
        self.entries.iter().find(|e| e.address == address)
    }

    /// Build the `component.list` result table.
    pub fn list(&self, lua: &Lua, filter: Option<&str>, exact: bool) -> mlua::Result<mlua::Table> {
        let table = lua.create_table()?;
        for entry in &self.entries {
            let matches = match filter {
                None => true,
                Some(f) => {
                    if exact {
                        entry.type_name == f
                    } else {
                        entry.type_name.starts_with(f)
                    }
                }
            };
            if matches {
                table.set(entry.address.as_str(), entry.type_name.as_str())?;
            }
        }
        Ok(table)
    }

    /// Build the `component.methods` result table.
    pub fn methods_table(&self, lua: &Lua, address: &str) -> mlua::Result<Option<mlua::Table>> {
        let entry = match self.find(address) {
            Some(e) => e,
            None => return Ok(None),
        };
        let table = lua.create_table()?;
        for method in entry.component.methods() {
            let info = lua.create_table()?;
            info.set("direct", method.direct)?;
            table.set(method.name, info)?;
        }
        Ok(Some(table))
    }

    pub fn doc(&self, address: &str, method: &str) -> Option<String> {
        self.find(address)
            .and_then(|e| e.component.methods().iter().find(|m| m.name == method))
            .map(|m| m.doc.to_string())
    }
}

fn generate_address(n: u64) -> String {
    // OpenComputers addresses are UUID-like strings; the exact form is opaque,
    // so any unique string works.
    let hex = format!("{:012x}", n);
    format!("00000000-0000-0000-0000-{}", hex)
}

// ---------------------------------------------------------------------------
// Argument helpers

pub fn arg_value(args: &[Value], index: usize) -> Value {
    args.get(index).cloned().unwrap_or(Value::Nil)
}

pub fn arg_string(args: &[Value], index: usize) -> mlua::Result<String> {
    match args.get(index) {
        Some(Value::String(s)) => Ok(s.to_string_lossy().to_string()),
        Some(Value::Integer(i)) => Ok(i.to_string()),
        Some(Value::Number(n)) => Ok(n.to_string()),
        Some(other) => Err(mlua::Error::RuntimeError(format!(
            "bad argument #{} (string expected, got {})",
            index + 1,
            other.type_name()
        ))),
        None => Err(mlua::Error::RuntimeError(format!(
            "bad argument #{} (string expected, got no value)",
            index + 1
        ))),
    }
}

pub fn arg_bytes(args: &[Value], index: usize) -> mlua::Result<Vec<u8>> {
    match args.get(index) {
        Some(Value::String(s)) => Ok(s.as_bytes().to_vec()),
        Some(other) => Err(mlua::Error::RuntimeError(format!(
            "bad argument #{} (string expected, got {})",
            index + 1,
            other.type_name()
        ))),
        None => Err(mlua::Error::RuntimeError(format!(
            "bad argument #{} (string expected, got no value)",
            index + 1
        ))),
    }
}

pub fn arg_int(args: &[Value], index: usize) -> mlua::Result<i64> {
    match args.get(index) {
        Some(Value::Integer(i)) => Ok(*i),
        Some(Value::Number(n)) => Ok(*n as i64),
        Some(other) => Err(mlua::Error::RuntimeError(format!(
            "bad argument #{} (number expected, got {})",
            index + 1,
            other.type_name()
        ))),
        None => Err(mlua::Error::RuntimeError(format!(
            "bad argument #{} (number expected, got no value)",
            index + 1
        ))),
    }
}

pub fn arg_int32(args: &[Value], index: usize) -> mlua::Result<i32> {
    Ok(arg_int(args, index)? as i32)
}

pub fn arg_bool(args: &[Value], index: usize) -> mlua::Result<bool> {
    match args.get(index) {
        Some(Value::Boolean(b)) => Ok(*b),
        Some(other) => Err(mlua::Error::RuntimeError(format!(
            "bad argument #{} (boolean expected, got {})",
            index + 1,
            other.type_name()
        ))),
        None => Err(mlua::Error::RuntimeError(format!(
            "bad argument #{} (boolean expected, got no value)",
            index + 1
        ))),
    }
}

pub fn opt_bool(args: &[Value], index: usize, default: bool) -> bool {
    match args.get(index) {
        Some(Value::Boolean(b)) => *b,
        Some(Value::Nil) | None => default,
        _ => default,
    }
}

pub fn opt_int(args: &[Value], index: usize, default: i64) -> i64 {
    match args.get(index) {
        Some(Value::Integer(i)) => *i,
        Some(Value::Number(n)) => *n as i64,
        _ => default,
    }
}

pub fn opt_string(args: &[Value], index: usize, default: &str) -> String {
    match args.get(index) {
        Some(Value::String(s)) => s.to_string_lossy().to_string(),
        _ => default.to_string(),
    }
}

/// Convenience for returning a single value.
pub fn multi1(value: Value) -> MultiValue {
    let mut m = MultiValue::new();
    m.push_back(value);
    m
}
