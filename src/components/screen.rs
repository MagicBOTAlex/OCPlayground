//! The `screen` component: a display surface backed by a shared [`TextBuffer`].

use super::{Component, MethodInfo};
use crate::buffer::TextBuffer;
use mlua::{Lua, MultiValue, Value};
use std::cell::RefCell;
use std::rc::Rc;

pub type SharedBuffer = Rc<RefCell<TextBuffer>>;

pub struct Screen {
    pub buffer: SharedBuffer,
    pub aspect_ratio: RefCell<(f64, f64)>,
    pub precise: RefCell<bool>,
    pub touch_mode_inverted: RefCell<bool>,
    pub keyboards: RefCell<Vec<String>>,
}

impl Screen {
    pub fn new(buffer: SharedBuffer, aspect: (f64, f64)) -> Screen {
        Screen {
            buffer,
            aspect_ratio: RefCell::new(aspect),
            precise: RefCell::new(false),
            touch_mode_inverted: RefCell::new(false),
            keyboards: RefCell::new(Vec::new()),
        }
    }
}

const METHODS: &[MethodInfo] = &[
    MethodInfo { name: "isOn", direct: true, doc: "function():boolean -- Returns whether the screen is currently on." },
    MethodInfo { name: "turnOn", direct: true, doc: "function():boolean -- Turns the screen on." },
    MethodInfo { name: "turnOff", direct: true, doc: "function():boolean -- Turns the screen off." },
    MethodInfo { name: "getAspectRatio", direct: true, doc: "function():number, number -- The aspect ratio of the screen." },
    MethodInfo { name: "getKeyboards", direct: true, doc: "function():table -- The list of keyboards attached to the screen." },
    MethodInfo { name: "isPrecise", direct: true, doc: "function():boolean -- Whether the screen is in high precision mode." },
    MethodInfo { name: "setPrecise", direct: true, doc: "function(enabled:boolean):boolean -- Set whether to use high precision mode." },
    MethodInfo { name: "isTouchModeInverted", direct: true, doc: "function():boolean -- Whether touch mode is inverted." },
    MethodInfo { name: "setTouchModeInverted", direct: true, doc: "function(value:boolean):boolean -- Sets whether to invert touch mode." },
];

impl Component for Screen {
    fn type_name(&self) -> &'static str {
        "screen"
    }

    fn slot(&self) -> &'static str {
        "screen"
    }

    fn methods(&self) -> &'static [MethodInfo] {
        METHODS
    }

    fn invoke(&self, lua: &Lua, method: &str, args: &[Value]) -> mlua::Result<MultiValue> {
        let mut out = MultiValue::new();
        match method {
            "isOn" => out.push_back(Value::Boolean(self.buffer.borrow().on)),
            "turnOn" => {
                let old = self.buffer.borrow().on;
                self.buffer.borrow_mut().on = true;
                out.push_back(Value::Boolean(!old));
                out.push_back(Value::Boolean(true));
            }
            "turnOff" => {
                let old = self.buffer.borrow().on;
                self.buffer.borrow_mut().on = false;
                out.push_back(Value::Boolean(old));
                out.push_back(Value::Boolean(false));
            }
            "getAspectRatio" => {
                let (w, h) = *self.aspect_ratio.borrow();
                out.push_back(Value::Number(w));
                out.push_back(Value::Number(h));
            }
            "getKeyboards" => {
                let table = lua.create_table()?;
                for (i, address) in self.keyboards.borrow().iter().enumerate() {
                    table.set(i + 1, address.as_str())?;
                }
                out.push_back(Value::Table(table));
            }
            "isPrecise" => out.push_back(Value::Boolean(*self.precise.borrow())),
            "setPrecise" => {
                let value = super::arg_bool(args, 0)?;
                let old = *self.precise.borrow();
                *self.precise.borrow_mut() = value;
                out.push_back(Value::Boolean(old));
            }
            "isTouchModeInverted" => {
                out.push_back(Value::Boolean(*self.touch_mode_inverted.borrow()))
            }
            "setTouchModeInverted" => {
                let value = super::arg_bool(args, 0)?;
                let old = *self.touch_mode_inverted.borrow();
                *self.touch_mode_inverted.borrow_mut() = value;
                out.push_back(Value::Boolean(old));
            }
            _ => return Err(mlua::Error::RuntimeError("no such method".into())),
        }
        Ok(out)
    }

    fn device_info(&self) -> Vec<(String, String)> {
        vec![
            ("class".into(), "display".into()),
            ("description".into(), "Screen".into()),
            ("product".into(), "Display".into()),
        ]
    }
}
