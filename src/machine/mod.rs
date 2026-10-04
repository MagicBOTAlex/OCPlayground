//! The Lua machine: host state, Lua state setup, and the resume protocol.

pub mod host_api;

use crate::components::keyboard::Keyboard;
use crate::components::screen::{Screen, SharedBuffer};
use crate::components::{Registry, SignalQueue};
use mlua::{Lua, LuaOptions, MultiValue, StdLib, Thread, Value};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

/// Shared state accessible to the host API and the front-end.
pub struct Host {
    pub signals: SignalQueue,
    pub start: Instant,
    pub registry: Rc<RefCell<Registry>>,
    pub computer_address: String,
    pub boot_address: RefCell<Option<String>>,
    pub tmp_address: RefCell<Option<String>>,
    pub timeout: f64,
    pub memory_kib: u32,
    pub screen: SharedBuffer,
    pub screen_component: Rc<Screen>,
    pub keyboard: Rc<Keyboard>,
    pub users: Rc<RefCell<Vec<String>>>,
}

impl Host {
    pub fn now(&self) -> f64 {
        self.start.elapsed().as_secs_f64()
    }

    pub fn memory_bytes(&self) -> u64 {
        self.memory_kib as u64 * 1024
    }
}

/// Result of resuming the machine's Lua thread once.
pub enum Step {
    /// The machine asked to sleep for the given number of seconds.
    Sleep(f64),
    /// The machine shut down (`true` means reboot).
    Shutdown(bool),
    /// The machine yielded without a meaningful scheduling value.
    Yield,
    /// The machine thread finished (`success`, optional message).
    Finished(bool, Option<String>),
}

pub struct Machine {
    pub lua: Lua,
    pub thread: Thread,
    pub host: Rc<Host>,
}

impl Machine {
    pub fn create(host: Rc<Host>, machine_src: &str) -> anyhow::Result<Machine> {
        // OpenComputers' `machine.lua` needs the full `debug` library (hooks,
        // getinfo, ...). User code only ever sees the restricted `debug`
        // proxies exposed by the machine sandbox.
        let lua = unsafe { Lua::unsafe_new_with(StdLib::ALL, LuaOptions::default()) };
        host_api::install(&lua, host.clone())?;
        let func = lua
            .load(machine_src)
            .set_name("=machine")
            .into_function()?;
        let thread = lua.create_thread(func)?;
        Ok(Machine { lua, thread, host })
    }

    /// Resume the machine thread with the given values.
    pub fn step(&mut self, args: MultiValue) -> Step {
        let mut resume = args;
        loop {
            let result: mlua::Result<MultiValue> = self.thread.resume(resume);
            let values = match result {
                Err(error) => return Step::Finished(false, Some(error.to_string())),
                Ok(values) => values,
            };
            if self.thread.is_finished() {
                let success = matches!(values.get(0), Some(Value::Boolean(true)));
                let message = values
                    .get(1)
                    .filter(|v| !v.is_nil())
                    .and_then(|v| v.to_string().ok());
                return Step::Finished(success, message);
            }
            match values.get(0) {
                // `machine.lua` yields a host callback for non-direct component
                // methods; run it and resume the coroutine with its result.
                Some(Value::Function(function)) => {
                    match function.call::<Value>(()) {
                        Ok(value) => {
                            let mut next = MultiValue::new();
                            next.push_back(value);
                            resume = next;
                        }
                        Err(error) => return Step::Finished(false, Some(error.to_string())),
                    }
                }
                Some(Value::Integer(i)) => return Step::Sleep(*i as f64),
                Some(Value::Number(n)) => return Step::Sleep(*n),
                Some(Value::Boolean(b)) => return Step::Shutdown(*b),
                _ => return Step::Yield,
            }
        }
    }

    /// Drain one queued signal into resume arguments, or return empty.
    pub fn take_signal_args(&self) -> mlua::Result<MultiValue> {
        match self.host.signals.borrow_mut().pop_front() {
            Some(signal) => signal.to_multi(&self.lua),
            None => Ok(MultiValue::new()),
        }
    }

    pub fn screen_buffer(&self) -> &SharedBuffer {
        &self.host.screen
    }
}
