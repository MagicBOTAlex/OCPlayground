//! The `computer` component exposed to Lua for the machine's own device.

use super::{arg_string, Component, MethodInfo, Registry};
use mlua::{Lua, MultiValue, Value};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

pub type SharedRegistry = Rc<RefCell<Registry>>;

pub struct Computer {
    pub address: String,
    pub registry: SharedRegistry,
    pub users: Rc<RefCell<Vec<String>>>,
}

impl Computer {
    pub fn new(address: String, registry: SharedRegistry, users: Rc<RefCell<Vec<String>>>) -> Computer {
        Computer {
            address,
            registry,
            users,
        }
    }
}

const METHODS: &[MethodInfo] = &[
    MethodInfo { name: "beep", direct: true, doc: "function([frequency:string or number[, duration:number]]) -- Plays a tone." },
    MethodInfo { name: "getDeviceInfo", direct: true, doc: "function():table -- Collect information on all connected devices." },
    MethodInfo { name: "getProgramLocations", direct: true, doc: "function():table -- Returns a map of program name to disk label." },
    MethodInfo { name: "users", direct: true, doc: "function():table -- Returns a list of users." },
    MethodInfo { name: "addUser", direct: true, doc: "function(name:string) -- Adds a user." },
    MethodInfo { name: "removeUser", direct: true, doc: "function(name:string) -- Removes a user." },
    MethodInfo { name: "isRunning", direct: true, doc: "function():boolean -- Returns whether the computer is running." },
];

impl Component for Computer {
    fn type_name(&self) -> &'static str {
        "computer"
    }

    fn slot(&self) -> &'static str {
        "computer"
    }

    fn methods(&self) -> &'static [MethodInfo] {
        METHODS
    }

    fn invoke(&self, lua: &Lua, method: &str, args: &[Value]) -> mlua::Result<MultiValue> {
        let mut out = MultiValue::new();
        match method {
            "beep" => {
                // Audio is not emitted; the call is accepted for compatibility.
                let _ = args;
            }
            "getDeviceInfo" => {
                let table = lua.create_table()?;
                let registry = self.registry.borrow();
                for entry in registry.entries() {
                    let info = lua.create_table()?;
                    for (key, value) in entry.component.device_info() {
                        info.set(key, value)?;
                    }
                    table.set(entry.address.as_str(), info)?;
                }
                out.push_back(Value::Table(table));
            }
            "getProgramLocations" => {
                let table = lua.create_table()?;
                out.push_back(Value::Table(table));
            }
            "users" => {
                let table = lua.create_table()?;
                for (i, user) in self.users.borrow().iter().enumerate() {
                    table.set(i + 1, user.as_str())?;
                }
                out.push_back(Value::Table(table));
            }
            "addUser" => {
                let name = arg_string(args, 0)?;
                let mut users = self.users.borrow_mut();
                if users.iter().any(|u| u == &name) {
                    out.push_back(Value::Boolean(false));
                } else {
                    users.push(name);
                    out.push_back(Value::Boolean(true));
                }
            }
            "removeUser" => {
                let name = arg_string(args, 0)?;
                let mut users = self.users.borrow_mut();
                let before = users.len();
                users.retain(|u| u != &name);
                out.push_back(Value::Boolean(users.len() != before));
            }
            "isRunning" => out.push_back(Value::Boolean(true)),
            _ => return Err(mlua::Error::RuntimeError("no such method".into())),
        }
        Ok(out)
    }

    fn device_info(&self) -> Vec<(String, String)> {
        let mut info = BTreeMap::new();
        info.insert("class".to_string(), "computer".to_string());
        info.insert("description".to_string(), "Computer".to_string());
        info.into_iter().collect()
    }
}

pub fn user_name() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "user".to_string())
}
