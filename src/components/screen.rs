//! The `screen` component: a display surface backed by a shared [`TextBuffer`].

use super::{Component, MethodInfo, Signal, SignalArg, SignalQueue};
use crate::buffer::TextBuffer;
use crate::term::MouseAction;
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
    pub signals: SignalQueue,
    /// Assigned when the component is registered; touch signals carry their
    /// source address as the first argument.
    pub address: RefCell<String>,
}

impl Screen {
    pub fn new(buffer: SharedBuffer, aspect: (f64, f64), signals: SignalQueue) -> Screen {
        Screen {
            buffer,
            aspect_ratio: RefCell::new(aspect),
            precise: RefCell::new(false),
            touch_mode_inverted: RefCell::new(false),
            keyboards: RefCell::new(Vec::new()),
            signals,
            address: RefCell::new(String::new()),
        }
    }

    /// Deliver a pointer action to the machine as an OpenComputers screen
    /// signal (`touch`, `drag`, `drop` or `scroll`). Positions are 1-based
    /// integers normally, or 0-based numbers in high-precision mode; events
    /// outside the screen are dropped.
    pub fn touch(&self, action: MouseAction, x: i32, y: i32, data: i32) {
        let (width, height) = {
            let buffer = self.buffer.borrow();
            (buffer.width, buffer.height)
        };
        if x < 0 || y < 0 || x >= width || y >= height {
            return;
        }
        let name = match action {
            MouseAction::Press => "touch",
            MouseAction::Drag => "drag",
            MouseAction::Release => "drop",
            MouseAction::Scroll => "scroll",
        };
        let mut signal = Signal::new(name).with(SignalArg::Str(self.address.borrow().clone()));
        if *self.precise.borrow() {
            signal = signal
                .with(SignalArg::Float(x as f64))
                .with(SignalArg::Float(y as f64));
        } else {
            signal = signal
                .with(SignalArg::Int(x as i64 + 1))
                .with(SignalArg::Int(y as i64 + 1));
        }
        signal = signal.with(SignalArg::Int(data as i64));
        self.signals.borrow_mut().push_back(signal);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::{new_signal_queue, SignalQueue};
    use crate::color::ColorDepth;
    use crate::term::MouseAction;

    fn screen() -> (Screen, SignalQueue) {
        let buffer = Rc::new(RefCell::new(TextBuffer::new(80, 25, ColorDepth::EightBit)));
        let signals = new_signal_queue();
        let screen = Screen::new(buffer, (80.0, 25.0), signals.clone());
        *screen.address.borrow_mut() = "screen-addr".into();
        (screen, signals)
    }

    fn last(signals: &SignalQueue) -> Signal {
        signals.borrow().back().cloned().unwrap()
    }

    #[test]
    fn press_uses_one_based_coordinates() {
        let (screen, signals) = screen();
        screen.touch(MouseAction::Press, 9, 4, 0);

        let signal = last(&signals);
        assert_eq!(signal.name, "touch");
        assert_eq!(
            signal.args,
            vec![
                SignalArg::Str("screen-addr".into()),
                SignalArg::Int(10),
                SignalArg::Int(5),
                SignalArg::Int(0),
            ]
        );
    }

    #[test]
    fn precise_mode_uses_zero_based_floats() {
        let (screen, signals) = screen();
        *screen.precise.borrow_mut() = true;
        screen.touch(MouseAction::Press, 9, 4, 0);

        let signal = last(&signals);
        assert_eq!(
            signal.args,
            vec![
                SignalArg::Str("screen-addr".into()),
                SignalArg::Float(9.0),
                SignalArg::Float(4.0),
                SignalArg::Int(0),
            ]
        );
    }

    #[test]
    fn drag_drop_and_scroll_map_to_their_signals() {
        let (screen, signals) = screen();
        screen.touch(MouseAction::Drag, 0, 0, 0);
        assert_eq!(last(&signals).name, "drag");
        screen.touch(MouseAction::Release, 0, 0, 1);
        assert_eq!(last(&signals).name, "drop");
        screen.touch(MouseAction::Scroll, 2, 3, -1);
        let signal = last(&signals);
        assert_eq!(signal.name, "scroll");
        assert_eq!(signal.args[3], SignalArg::Int(-1));
    }

    #[test]
    fn out_of_bounds_is_ignored() {
        let (screen, signals) = screen();
        screen.touch(MouseAction::Press, -1, 0, 0);
        screen.touch(MouseAction::Press, 80, 0, 0);
        screen.touch(MouseAction::Press, 0, 25, 0);
        assert!(signals.borrow().is_empty());
    }
}
