//! The `keyboard` component. It has no Lua methods; the terminal front-end
//! feeds it key events through [`Keyboard::key_down`] and friends, which push
//! machine signals.

use super::{Component, MethodInfo, Signal, SignalArg, SignalQueue};
use std::cell::RefCell;

pub struct Keyboard {
    pub signals: SignalQueue,
    /// Assigned when the component is registered; OpenComputers key signals
    /// carry their source address as the first argument.
    pub address: RefCell<String>,
}

impl Keyboard {
    pub fn new(signals: SignalQueue) -> Keyboard {
        Keyboard {
            signals,
            address: RefCell::new(String::new()),
        }
    }

    pub fn key_down(&self, character: char, code: i32) {
        self.push("key_down", character, code);
    }

    pub fn key_up(&self, character: char, code: i32) {
        self.push("key_up", character, code);
    }

    pub fn clipboard(&self, value: &str) {
        let signal = Signal::new("clipboard")
            .with(SignalArg::Str(self.address.borrow().clone()))
            .with(SignalArg::Str(value.to_string()));
        self.signals.borrow_mut().push_back(signal);
    }

    fn push(&self, name: &str, character: char, code: i32) {
        // OpenComputers delivers `(name, address, charCode, keyCode)`; the
        // character is a numeric code point for key_down/key_up signals.
        let signal = Signal::new(name)
            .with(SignalArg::Str(self.address.borrow().clone()))
            .with(SignalArg::Int(character as i64))
            .with(SignalArg::Int(code as i64));
        self.signals.borrow_mut().push_back(signal);
    }
}

impl Component for Keyboard {
    fn type_name(&self) -> &'static str {
        "keyboard"
    }

    fn slot(&self) -> &'static str {
        "keyboard"
    }

    fn methods(&self) -> &'static [MethodInfo] {
        &[]
    }

    fn invoke(
        &self,
        _lua: &mlua::Lua,
        _method: &str,
        _args: &[mlua::Value],
    ) -> mlua::Result<mlua::MultiValue> {
        Err(mlua::Error::RuntimeError("no such method".into()))
    }

    fn device_info(&self) -> Vec<(String, String)> {
        vec![
            ("class".into(), "input".into()),
            ("description".into(), "Keyboard".into()),
            ("product".into(), "Fancytyper MX-Stone".into()),
        ]
    }
}
