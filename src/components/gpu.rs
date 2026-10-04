//! The `gpu` component, drawing into the bound screen's [`TextBuffer`].

use super::screen::SharedBuffer;
use super::{arg_int32, arg_string, opt_bool, Component, MethodInfo};
use crate::color::{ColorDepth, Format};
use mlua::{Lua, MultiValue, Value};
use std::cell::RefCell;

pub struct Gpu {
    pub buffer: SharedBuffer,
    pub max_depth: ColorDepth,
    pub screen_address: RefCell<Option<String>>,
}

impl Gpu {
    pub fn new(buffer: SharedBuffer, max_depth: ColorDepth) -> Gpu {
        Gpu {
            buffer,
            max_depth,
            screen_address: RefCell::new(None),
        }
    }

    fn effective_max_depth(&self) -> u8 {
        self.max_depth
            .ordinal()
            .min(self.buffer.borrow().max_depth.ordinal())
    }
}

const METHODS: &[MethodInfo] = &[
    MethodInfo { name: "bind", direct: true, doc: "function(address:string[, reset:boolean=true]):boolean -- Binds the GPU to a screen." },
    MethodInfo { name: "getScreen", direct: true, doc: "function():string -- Get the address of the screen the GPU is currently bound to." },
    MethodInfo { name: "getBackground", direct: true, doc: "function():number, boolean -- Get the current background color." },
    MethodInfo { name: "setBackground", direct: true, doc: "function(value:number[, palette:boolean]) -- Sets the background color." },
    MethodInfo { name: "getForeground", direct: true, doc: "function():number, boolean -- Get the current foreground color." },
    MethodInfo { name: "setForeground", direct: true, doc: "function(value:number[, palette:boolean]) -- Sets the foreground color." },
    MethodInfo { name: "getPaletteColor", direct: true, doc: "function(index:number):number -- Get the palette color at the specified index." },
    MethodInfo { name: "setPaletteColor", direct: true, doc: "function(index:number, color:number):number -- Set the palette color at the specified index." },
    MethodInfo { name: "getDepth", direct: true, doc: "function():number -- Returns the currently set color depth." },
    MethodInfo { name: "setDepth", direct: true, doc: "function(depth:number):number -- Set the color depth." },
    MethodInfo { name: "maxDepth", direct: true, doc: "function():number -- Get the maximum supported color depth." },
    MethodInfo { name: "getResolution", direct: true, doc: "function():number, number -- Get the current screen resolution." },
    MethodInfo { name: "setResolution", direct: true, doc: "function(width:number, height:number):boolean -- Set the screen resolution." },
    MethodInfo { name: "maxResolution", direct: true, doc: "function():number, number -- Get the maximum screen resolution." },
    MethodInfo { name: "getViewport", direct: true, doc: "function():number, number -- Get the current viewport resolution." },
    MethodInfo { name: "setViewport", direct: true, doc: "function(width:number, height:number):boolean -- Set the viewport resolution." },
    MethodInfo { name: "get", direct: true, doc: "function(x:number, y:number):string, number, number, number or nil, number or nil -- Get the value at the specified index." },
    MethodInfo { name: "set", direct: true, doc: "function(x:number, y:number, value:string[, vertical:boolean]):boolean -- Plots a string value." },
    MethodInfo { name: "copy", direct: true, doc: "function(x:number, y:number, width:number, height:number, tx:number, ty:number):boolean -- Copies a portion of the screen." },
    MethodInfo { name: "fill", direct: true, doc: "function(x:number, y:number, width:number, height:number, char:string):boolean -- Fills a portion of the screen." },
];

impl Component for Gpu {
    fn type_name(&self) -> &'static str {
        "gpu"
    }

    fn slot(&self) -> &'static str {
        "gpu"
    }

    fn methods(&self) -> &'static [MethodInfo] {
        METHODS
    }

    fn invoke(&self, lua: &Lua, method: &str, args: &[Value]) -> mlua::Result<MultiValue> {
        let mut out = MultiValue::new();
        match method {
            "bind" => {
                let address = arg_string(args, 0)?;
                let reset = opt_bool(args, 1, true);
                *self.screen_address.borrow_mut() = Some(address);
                if reset {
                    let mut buffer = self.buffer.borrow_mut();
                    let (mw, mh) = (buffer.max_width, buffer.max_height);
                    let depth = ColorDepth::from_ordinal(
                        self.max_depth.ordinal().min(buffer.max_depth.ordinal()),
                    );
                    buffer.format = Format::new(depth);
                    buffer.set_resolution(mw, mh).map_err(mlua::Error::RuntimeError)?;
                    buffer
                        .set_foreground(0xFFFFFF, false)
                        .map_err(mlua::Error::RuntimeError)?;
                    buffer
                        .set_background(0x000000, false)
                        .map_err(mlua::Error::RuntimeError)?;
                    buffer.viewport_width = mw;
                    buffer.viewport_height = mh;
                    buffer.revision += 1;
                }
                out.push_back(Value::Boolean(true));
            }
            "getScreen" => match self.screen_address.borrow().clone() {
                Some(address) => out.push_back(Value::String(lua.create_string(&address)?)),
                None => out.push_back(Value::Nil),
            },
            "getBackground" => {
                let buffer = self.buffer.borrow();
                out.push_back(Value::Integer(buffer.background.value as i64));
                out.push_back(Value::Boolean(buffer.background.is_palette));
            }
            "setBackground" => {
                let value = arg_int32(args, 0)?;
                let palette = opt_bool(args, 1, false);
                let mut buffer = self.buffer.borrow_mut();
                let old = buffer.background;
                let (old_color, old_index) = resolve_old(&buffer.format, old);
                buffer
                    .set_background(value, palette)
                    .map_err(mlua::Error::RuntimeError)?;
                out.push_back(Value::Integer(old_color as i64));
                push_opt_int(&mut out, old_index);
            }
            "getForeground" => {
                let buffer = self.buffer.borrow();
                out.push_back(Value::Integer(buffer.foreground.value as i64));
                out.push_back(Value::Boolean(buffer.foreground.is_palette));
            }
            "setForeground" => {
                let value = arg_int32(args, 0)?;
                let palette = opt_bool(args, 1, false);
                let mut buffer = self.buffer.borrow_mut();
                let old = buffer.foreground;
                let (old_color, old_index) = resolve_old(&buffer.format, old);
                buffer
                    .set_foreground(value, palette)
                    .map_err(mlua::Error::RuntimeError)?;
                out.push_back(Value::Integer(old_color as i64));
                push_opt_int(&mut out, old_index);
            }
            "getPaletteColor" => {
                let index = arg_int32(args, 0)?;
                let color = self
                    .buffer
                    .borrow()
                    .get_palette_color(index)
                    .map_err(mlua::Error::RuntimeError)?;
                out.push_back(Value::Integer(color as i64));
            }
            "setPaletteColor" => {
                let index = arg_int32(args, 0)?;
                let color = arg_int32(args, 1)? as u32;
                let old = self
                    .buffer
                    .borrow_mut()
                    .set_palette_color(index, color)
                    .map_err(mlua::Error::RuntimeError)?;
                out.push_back(Value::Integer(old as i64));
            }
            "getDepth" => {
                out.push_back(Value::Integer(self.buffer.borrow().format.depth.bits() as i64));
            }
            "setDepth" => {
                let depth = arg_int32(args, 0)?;
                let new_depth = match depth {
                    1 => ColorDepth::OneBit,
                    4 if self.effective_max_depth() >= ColorDepth::FourBit.ordinal() => {
                        ColorDepth::FourBit
                    }
                    8 if self.effective_max_depth() >= ColorDepth::EightBit.ordinal() => {
                        ColorDepth::EightBit
                    }
                    _ => return Err(mlua::Error::RuntimeError("unsupported depth".into())),
                };
                let mut buffer = self.buffer.borrow_mut();
                let old = buffer.format.depth.bits() as i64;
                buffer.set_depth(new_depth);
                out.push_back(Value::Integer(old));
            }
            "maxDepth" => {
                let bits = match self.effective_max_depth() {
                    0 => 1,
                    1 => 4,
                    _ => 8,
                };
                out.push_back(Value::Integer(bits));
            }
            "getResolution" => {
                let buffer = self.buffer.borrow();
                out.push_back(Value::Integer(buffer.width as i64));
                out.push_back(Value::Integer(buffer.height as i64));
            }
            "setResolution" => {
                let w = arg_int32(args, 0)?;
                let h = arg_int32(args, 1)?;
                let changed = self
                    .buffer
                    .borrow_mut()
                    .set_resolution(w, h)
                    .map_err(mlua::Error::RuntimeError)?;
                out.push_back(Value::Boolean(changed));
            }
            "maxResolution" => {
                let buffer = self.buffer.borrow();
                out.push_back(Value::Integer(buffer.max_width as i64));
                out.push_back(Value::Integer(buffer.max_height as i64));
            }
            "getViewport" => {
                let buffer = self.buffer.borrow();
                out.push_back(Value::Integer(buffer.viewport_width as i64));
                out.push_back(Value::Integer(buffer.viewport_height as i64));
            }
            "setViewport" => {
                let w = arg_int32(args, 0)?;
                let h = arg_int32(args, 1)?;
                let mut buffer = self.buffer.borrow_mut();
                if w < 1 || h < 1 || w > buffer.width || h > buffer.height {
                    return Err(mlua::Error::RuntimeError(
                        "unsupported viewport resolution".into(),
                    ));
                }
                let changed = buffer.set_viewport(w, h);
                out.push_back(Value::Boolean(changed));
            }
            "get" => {
                let x = arg_int32(args, 0)? - 1;
                let y = arg_int32(args, 1)? - 1;
                let buffer = self.buffer.borrow();
                let cp = buffer
                    .get(x, y)
                    .ok_or_else(|| mlua::Error::RuntimeError("index out of bounds".into()))?;
                let (fg, bg, fg_idx, bg_idx) = buffer.cell_colors_full(x, y);
                let ch = char::from_u32(cp).unwrap_or(' ');
                out.push_back(Value::String(lua.create_string(ch.to_string())?));
                out.push_back(Value::Integer(fg as i64));
                out.push_back(Value::Integer(bg as i64));
                push_opt_int(&mut out, fg_idx);
                push_opt_int(&mut out, bg_idx);
            }
            "set" => {
                let x = arg_int32(args, 0)? - 1;
                let y = arg_int32(args, 1)? - 1;
                let value = arg_string(args, 2)?;
                let vertical = opt_bool(args, 3, false);
                let changed = self.buffer.borrow_mut().set(x, y, &value, vertical);
                out.push_back(Value::Boolean(changed));
            }
            "copy" => {
                let x = arg_int32(args, 0)? - 1;
                let y = arg_int32(args, 1)? - 1;
                let w = arg_int32(args, 2)?.max(0);
                let h = arg_int32(args, 3)?.max(0);
                let tx = arg_int32(args, 4)?;
                let ty = arg_int32(args, 5)?;
                self.buffer.borrow_mut().copy(x, y, w, h, tx, ty);
                out.push_back(Value::Boolean(true));
            }
            "fill" => {
                let x = arg_int32(args, 0)? - 1;
                let y = arg_int32(args, 1)? - 1;
                let w = arg_int32(args, 2)?.max(0);
                let h = arg_int32(args, 3)?.max(0);
                let value = arg_string(args, 4)?;
                let c = value
                    .chars()
                    .next()
                    .ok_or_else(|| mlua::Error::RuntimeError("invalid fill value".into()))?
                    as u32;
                self.buffer.borrow_mut().fill(x, y, w, h, c);
                out.push_back(Value::Boolean(true));
            }
            _ => return Err(mlua::Error::RuntimeError("no such method".into())),
        }
        Ok(out)
    }

    fn device_info(&self) -> Vec<(String, String)> {
        vec![
            ("class".into(), "display".into()),
            ("description".into(), "Graphics Card".into()),
            ("product".into(), "GPU".into()),
        ]
    }
}

fn resolve_old(format: &Format, color: crate::color::Color) -> (i32, Option<i32>) {
    if color.is_palette {
        let index = color.value;
        let rgb = format
            .palette
            .get(index.max(0) as usize)
            .copied()
            .unwrap_or(0) as i32;
        (rgb, Some(index))
    } else {
        (color.value, None)
    }
}

fn push_opt_int(out: &mut MultiValue, value: Option<i32>) {
    match value {
        Some(v) => out.push_back(Value::Integer(v as i64)),
        None => out.push_back(Value::Nil),
    }
}
