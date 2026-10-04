//! The `eeprom` component: stores the BIOS code and the volatile boot address.

use super::{arg_bytes, opt_string, Component, MethodInfo};
use mlua::{Lua, MultiValue, Value};
use std::cell::RefCell;

pub struct Eeprom {
    pub code: Vec<u8>,
    pub data: RefCell<Vec<u8>>,
    pub label: RefCell<String>,
    pub readonly: bool,
}

impl Eeprom {
    pub fn new(code: Vec<u8>) -> Eeprom {
        Eeprom {
            code,
            data: RefCell::new(Vec::new()),
            label: RefCell::new("EEPROM".to_string()),
            readonly: false,
        }
    }

    pub fn boot_address(&self) -> Option<String> {
        let data = self.data.borrow();
        if data.is_empty() {
            None
        } else {
            Some(String::from_utf8_lossy(&data).to_string())
        }
    }
}

// --------------------------------------------------------------------------- //

const METHODS: &[MethodInfo] = &[
    MethodInfo { name: "get", direct: true, doc: "function():string -- Get the currently stored byte array." },
    MethodInfo { name: "set", direct: true, doc: "function(data:string) -- Overwrite the currently stored byte array." },
    MethodInfo { name: "getLabel", direct: true, doc: "function():string -- Get the label of the EEPROM." },
    MethodInfo { name: "setLabel", direct: true, doc: "function(data:string):string -- Set the label of the EEPROM." },
    MethodInfo { name: "getSize", direct: true, doc: "function():number -- Get the storage capacity of this EEPROM." },
    MethodInfo { name: "getChecksum", direct: true, doc: "function():string -- Get the checksum of the data on this EEPROM." },
    MethodInfo { name: "getData", direct: true, doc: "function():string -- Get the currently stored boot data." },
    MethodInfo { name: "setData", direct: true, doc: "function(data:string) -- Overwrite the currently stored boot data." },
    MethodInfo { name: "getDataSize", direct: true, doc: "function():number -- Get the storage capacity of the boot data." },
];

impl Component for Eeprom {
    fn type_name(&self) -> &'static str {
        "eeprom"
    }

    fn slot(&self) -> &'static str {
        "eeprom"
    }

    fn methods(&self) -> &'static [MethodInfo] {
        METHODS
    }

    fn invoke(&self, lua: &Lua, method: &str, args: &[Value]) -> mlua::Result<MultiValue> {
        let mut out = MultiValue::new();
        match method {
            "get" => out.push_back(Value::String(lua.create_string(&self.code)?)),
            "set" => {
                let data = arg_bytes(args, 0)?;
                *self.data.borrow_mut() = data;
            }
            "getLabel" => out.push_back(Value::String(lua.create_string(self.label.borrow().as_str())?)),
            "setLabel" => {
                let label = opt_string(args, 0, "EEPROM");
                let label: String = label.trim().chars().take(24).collect();
                let label = if label.is_empty() { "EEPROM".to_string() } else { label };
                *self.label.borrow_mut() = label.clone();
                out.push_back(Value::String(lua.create_string(&label)?));
            }
            "getSize" => out.push_back(Value::Integer(2048)),
            "getChecksum" => {
                let sum = crc32(&self.code);
                out.push_back(Value::String(lua.create_string(format!("{:08x}", sum))?));
            }
            "getData" => {
                let data = self.data.borrow();
                out.push_back(Value::String(lua.create_string(&data[..])?));
            }
            "setData" => {
                let data = arg_bytes(args, 0)?;
                *self.data.borrow_mut() = data;
            }
            "getDataSize" => out.push_back(Value::Integer(256)),
            _ => return Err(mlua::Error::RuntimeError("no such method".into())),
        }
        Ok(out)
    }

    fn device_info(&self) -> Vec<(String, String)> {
        vec![
            ("class".into(), "memory".into()),
            ("description".into(), "EEPROM".into()),
            ("product".into(), "FlashStick2k".into()),
            ("capacity".into(), "2048".into()),
            ("size".into(), "2048".into()),
        ]
    }
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for byte in data {
        crc ^= *byte as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}
