//! Installs the OpenComputers host API globals (`component`, `computer`,
//! `system`, `unicode`, `userdata`) into a Lua state.

use super::Host;
use crate::color::wcwidth;
use mlua::{Lua, MultiValue, Value, Variadic};
use std::rc::Rc;

pub fn install(lua: &Lua, host: Rc<Host>) -> anyhow::Result<()> {
    install_component(lua, &host)?;
    install_computer(lua, &host)?;
    install_system(lua, &host)?;
    install_unicode(lua)?;
    install_userdata(lua)?;
    Ok(())
}

fn install_component(lua: &Lua, host: &Rc<Host>) -> mlua::Result<()> {
    let component = lua.create_table()?;

    {
        let host = host.clone();
        component.set(
            "list",
            lua.create_function(
                move |lua, (filter, exact): (Option<String>, Option<bool>)| {
                    host.registry
                        .borrow()
                        .list(lua, filter.as_deref(), exact.unwrap_or(false))
                },
            )?,
        )?;
    }
    {
        let host = host.clone();
        component.set(
            "methods",
            lua.create_function(move |lua, address: String| {
                let table = host.registry.borrow().methods_table(lua, &address)?;
                match table {
                    Some(table) => Ok(Value::Table(table)),
                    None => Ok(Value::Nil),
                }
            })?,
        )?;
    }
    {
        let host = host.clone();
        component.set(
            "invoke",
            lua.create_function(
                move |lua, (address, method, args): (String, String, Variadic<Value>)| {
                    let component = {
                        let registry = host.registry.borrow();
                        match registry.find(&address) {
                            Some(entry) => entry.component.clone(),
                            None => {
                                return Err(mlua::Error::RuntimeError(
                                    "no such component".into(),
                                ))
                            }
                        }
                    };
                    if !component.has_method(&method) {
                        return Err(mlua::Error::RuntimeError("no such method".into()));
                    }
                    // `machine.lua`'s `processResult` treats the first value as
                    // a success flag, so prepend `true`.
                    let result = component.invoke(lua, &method, args.as_ref())?;
                    let mut out = MultiValue::new();
                    out.push_back(Value::Boolean(true));
                    for value in result {
                        out.push_back(value);
                    }
                    Ok(out)
                },
            )?,
        )?;
    }
    {
        let host = host.clone();
        component.set(
            "doc",
            lua.create_function(move |_, (address, method): (String, String)| {
                let doc = host.registry.borrow().doc(&address, &method);
                Ok(doc)
            })?,
        )?;
    }
    {
        let host = host.clone();
        component.set(
            "type",
            lua.create_function(move |_, address: String| {
                let registry = host.registry.borrow();
                Ok(registry.find(&address).map(|e| e.type_name.clone()))
            })?,
        )?;
    }
    {
        let host = host.clone();
        component.set(
            "slot",
            lua.create_function(move |_, address: String| {
                let registry = host.registry.borrow();
                Ok(registry.find(&address).map(|e| e.slot.clone()))
            })?,
        )?;
    }

    lua.globals().set("component", component)?;
    Ok(())
}

fn install_computer(lua: &Lua, host: &Rc<Host>) -> mlua::Result<()> {
    let computer = lua.create_table()?;

    {
        let host = host.clone();
        computer.set(
            "address",
            lua.create_function(move |_, ()| Ok(host.computer_address.clone()))?,
        )?;
    }
    {
        let host = host.clone();
        computer.set(
            "tmpAddress",
            lua.create_function(move |_, ()| Ok(host.tmp_address.borrow().clone()))?,
        )?;
    }
    {
        let host = host.clone();
        computer.set("realTime", lua.create_function(move |_, ()| Ok(host.now()))?)?;
    }
    {
        let host = host.clone();
        computer.set("uptime", lua.create_function(move |_, ()| Ok(host.now()))?)?;
    }
    computer.set(
        "energy",
        lua.create_function(|_, ()| Ok(100000.0f64))?,
    )?;
    computer.set(
        "maxEnergy",
        lua.create_function(|_, ()| Ok(100000.0f64))?,
    )?;
    {
        let host = host.clone();
        computer.set(
            "freeMemory",
            lua.create_function(move |_, ()| Ok((host.memory_bytes() / 2) as i64))?,
        )?;
    }
    {
        let host = host.clone();
        computer.set(
            "totalMemory",
            lua.create_function(move |_, ()| Ok(host.memory_bytes() as i64))?,
        )?;
    }
    {
        let host = host.clone();
        computer.set(
            "getBootAddress",
            lua.create_function(move |_, ()| Ok(host.boot_address.borrow().clone()))?,
        )?;
    }
    {
        let host = host.clone();
        computer.set(
            "setBootAddress",
            lua.create_function(move |_, address: Option<String>| {
                *host.boot_address.borrow_mut() = address;
                Ok(())
            })?,
        )?;
    }
    {
        let host = host.clone();
        computer.set(
            "pushSignal",
            lua.create_function(move |lua, (name, args): (String, Variadic<Value>)| {
                let mut signal = crate::components::Signal::new(name);
                for arg in args.iter() {
                    signal.args.push(value_to_signal_arg(arg)?);
                }
                host.signals.borrow_mut().push_back(signal);
                let _ = lua;
                Ok(())
            })?,
        )?;
    }
    {
        let host = host.clone();
        computer.set(
            "users",
            lua.create_function(move |lua, ()| {
                let table = lua.create_table()?;
                for (i, user) in host.users.borrow().iter().enumerate() {
                    table.set(i + 1, user.as_str())?;
                }
                Ok(table)
            })?,
        )?;
    }
    {
        let host = host.clone();
        computer.set(
            "addUser",
            lua.create_function(move |_, name: String| {
                let mut users = host.users.borrow_mut();
                if users.iter().any(|u| u == &name) {
                    Ok(false)
                } else {
                    users.push(name);
                    Ok(true)
                }
            })?,
        )?;
    }
    {
        let host = host.clone();
        computer.set(
            "removeUser",
            lua.create_function(move |_, name: String| {
                let mut users = host.users.borrow_mut();
                let before = users.len();
                users.retain(|u| u != &name);
                Ok(users.len() != before)
            })?,
        )?;
    }
    computer.set("isRobot", lua.create_function(|_, ()| Ok(false))?)?;
    computer.set(
        "getArchitectures",
        lua.create_function(|lua, ()| {
            let table = lua.create_table()?;
            table.set(1, "Lua 5.4")?;
            Ok(table)
        })?,
    )?;
    computer.set(
        "getArchitecture",
        lua.create_function(|_, ()| Ok("Lua 5.4"))?,
    )?;
    computer.set(
        "setArchitecture",
        lua.create_function(|_, _name: String| Ok(false))?,
    )?;
    // `computer.beep` is provided through the computer component in machine.lua,
    // but define a fallback for direct calls.
    computer.set("beep", lua.create_function(|_, _: Variadic<Value>| Ok(()))?)?;
    computer.set(
        "shutdown",
        lua.create_function(|_, _: Option<bool>| Ok(()))?,
    )?;

    lua.globals().set("computer", computer)?;
    Ok(())
}

fn install_system(lua: &Lua, host: &Rc<Host>) -> mlua::Result<()> {
    let system = lua.create_table()?;
    system.set(
        "allowBytecode",
        lua.create_function(|_, ()| Ok(true))?,
    )?;
    system.set("allowGC", lua.create_function(|_, ()| Ok(false))?)?;
    {
        let host = host.clone();
        system.set(
            "timeout",
            lua.create_function(move |_, ()| Ok(host.timeout))?,
        )?;
    }
    lua.globals().set("system", system)?;
    Ok(())
}

fn install_unicode(lua: &Lua) -> mlua::Result<()> {
    let unicode = lua.create_table()?;

    unicode.set(
        "char",
        lua.create_function(|_, args: Variadic<i64>| {
            let mut s = String::new();
            for code in args.iter() {
                if let Some(ch) = char::from_u32(*code as u32) {
                    s.push(ch);
                }
            }
            Ok(s)
        })?,
    )?;
    unicode.set(
        "len",
        lua.create_function(|_, s: String| Ok(s.chars().count() as i64))?,
    )?;
    unicode.set(
        "lower",
        lua.create_function(|_, s: String| Ok(s.to_lowercase()))?,
    )?;
    unicode.set(
        "upper",
        lua.create_function(|_, s: String| Ok(s.to_uppercase()))?,
    )?;
    unicode.set(
        "reverse",
        lua.create_function(|_, s: String| Ok(s.chars().rev().collect::<String>()))?,
    )?;
    unicode.set(
        "sub",
        lua.create_function(|_, (s, i, j): (String, i64, Option<i64>)| {
            Ok(unicode_sub(&s, i, j))
        })?,
    )?;
    unicode.set(
        "isWide",
        lua.create_function(|_, s: String| {
            Ok(s.chars().next().map(|c| wcwidth(c as u32) > 1).unwrap_or(false))
        })?,
    )?;
    unicode.set(
        "charWidth",
        lua.create_function(|_, s: String| {
            Ok(s.chars().next().map(|c| wcwidth(c as u32)).unwrap_or(1) as i64)
        })?,
    )?;
    unicode.set(
        "wlen",
        lua.create_function(|_, s: String| {
            Ok(s.chars().map(|c| wcwidth(c as u32).max(1) as i64).sum::<i64>())
        })?,
    )?;
    unicode.set(
        "wtrunc",
        lua.create_function(|_, (s, n): (String, i64)| {
            let mut width = 0i64;
            let mut out = String::new();
            for c in s.chars() {
                if width >= n {
                    break;
                }
                out.push(c);
                width += wcwidth(c as u32).max(1) as i64;
            }
            Ok(out)
        })?,
    )?;

    lua.globals().set("unicode", unicode)?;
    Ok(())
}

fn unicode_sub(s: &str, i: i64, j: Option<i64>) -> String {
    let chars: Vec<char> = s.chars().collect();
    let len = chars.len() as i64;
    let start = if i < 0 {
        (len + i).max(0)
    } else if i == 0 {
        0
    } else {
        (i - 1).min(len)
    };
    let end = match j {
        None => len,
        Some(j) if j < 0 => (len + j + 1).max(0),
        Some(j) => j.min(len),
    };
    if end <= start {
        String::new()
    } else {
        chars[start as usize..end as usize].iter().collect()
    }
}

fn install_userdata(lua: &Lua) -> mlua::Result<()> {
    // OpenComputers uses the `userdata` host API for persistence and proxying.
    // The emulator never hands userdata to Lua, so these stubs are inert.
    let userdata = lua.create_table()?;
    let unsupported = lua.create_function(|_, _: Variadic<Value>| {
        Err::<Value, _>(mlua::Error::RuntimeError("userdata is not supported".into()))
    })?;
    for name in [
        "methods", "invoke", "doc", "apply", "unapply", "call", "save", "load", "dispose",
    ] {
        userdata.set(name, unsupported.clone())?;
    }
    lua.globals().set("userdata", userdata)?;
    Ok(())
}

fn value_to_signal_arg(value: &Value) -> mlua::Result<crate::components::SignalArg> {
    use crate::components::SignalArg;
    Ok(match value {
        Value::Nil => SignalArg::Nil,
        Value::Boolean(b) => SignalArg::Bool(*b),
        Value::Integer(i) => SignalArg::Int(*i),
        Value::Number(n) => SignalArg::Float(*n),
        Value::String(s) => SignalArg::Str(s.to_string_lossy().to_string()),
        other => SignalArg::Str(other.type_name().to_string()),
    })
}

/// Helper used by [`install_computer`] to expose a `MultiValue` if needed.
#[allow(dead_code)]
fn multi(values: Vec<Value>) -> MultiValue {
    values.into_iter().collect()
}
