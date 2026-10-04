//! The `filesystem` component, backed by a host directory.

use super::{arg_bytes, arg_int32, arg_string, opt_string, Component, MethodInfo};
use mlua::{Lua, MultiValue, Value};
use std::cell::RefCell;
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

struct OpenFile {
    file: fs::File,
}

pub struct HostFileSystem {
    pub root: PathBuf,
    pub label: RefCell<String>,
    pub readonly: bool,
    handles: RefCell<HashMap<i64, OpenFile>>,
    next_handle: RefCell<i64>,
}

impl HostFileSystem {
    pub fn new(root: PathBuf, label: String, readonly: bool) -> HostFileSystem {
        HostFileSystem {
            root,
            label: RefCell::new(label),
            readonly,
            handles: RefCell::new(HashMap::new()),
            next_handle: RefCell::new(1),
        }
    }

    fn resolve(&self, path: &str) -> Result<PathBuf, String> {
        let cleaned = clean(path)?;
        let mut result = self.root.clone();
        for segment in cleaned.split('/') {
            if !segment.is_empty() {
                result.push(segment);
            }
        }
        Ok(result)
    }

    fn check_writable(&self) -> Result<(), String> {
        if self.readonly {
            Err("filesystem is readonly".into())
        } else {
            Ok(())
        }
    }
}

fn clean(path: &str) -> Result<String, String> {
    let mut parts: Vec<&str> = Vec::new();
    for segment in path.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                if parts.pop().is_none() {
                    return Err("no such file or directory".into());
                }
            }
            other => parts.push(other),
        }
    }
    Ok(parts.join("/"))
}

const METHODS: &[MethodInfo] = &[
    MethodInfo { name: "getLabel", direct: true, doc: "function():string -- Get the current label of the drive." },
    MethodInfo { name: "setLabel", direct: true, doc: "function(value:string):string -- Sets the label of the drive." },
    MethodInfo { name: "isReadOnly", direct: true, doc: "function():boolean -- Returns whether the file system is read-only." },
    MethodInfo { name: "spaceTotal", direct: true, doc: "function():number -- The overall capacity of the file system, in bytes." },
    MethodInfo { name: "spaceUsed", direct: true, doc: "function():number -- The currently used capacity of the file system, in bytes." },
    MethodInfo { name: "exists", direct: true, doc: "function(path:string):boolean -- Returns whether an object exists at the specified absolute path." },
    MethodInfo { name: "size", direct: true, doc: "function(path:string):number -- Returns the size of the object at the specified absolute path." },
    MethodInfo { name: "isDirectory", direct: true, doc: "function(path:string):boolean -- Returns whether the object at the specified path is a directory." },
    MethodInfo { name: "lastModified", direct: true, doc: "function(path:string):number -- Returns the timestamp of when the object was modified." },
    MethodInfo { name: "list", direct: true, doc: "function(path:string):table -- Returns a list of names of objects in the directory." },
    MethodInfo { name: "makeDirectory", direct: true, doc: "function(path:string):boolean -- Creates a directory at the specified path." },
    MethodInfo { name: "remove", direct: true, doc: "function(path:string):boolean -- Removes the object at the specified path." },
    MethodInfo { name: "rename", direct: true, doc: "function(from:string, to:string):boolean -- Renames an object." },
    MethodInfo { name: "open", direct: true, doc: "function(path:string[, mode:string='r']):number -- Opens a file descriptor." },
    MethodInfo { name: "read", direct: true, doc: "function(handle:number, count:number):string or nil -- Reads from an open file descriptor." },
    MethodInfo { name: "seek", direct: true, doc: "function(handle:number, whence:string, offset:number):number -- Seeks in an open file descriptor." },
    MethodInfo { name: "write", direct: true, doc: "function(handle:number, value:string):boolean -- Writes to an open file descriptor." },
    MethodInfo { name: "close", direct: true, doc: "function(handle:number) -- Closes an open file descriptor." },
];

impl Component for HostFileSystem {
    fn type_name(&self) -> &'static str {
        "filesystem"
    }

    fn slot(&self) -> &'static str {
        "filesystem"
    }

    fn methods(&self) -> &'static [MethodInfo] {
        METHODS
    }

    fn invoke(&self, lua: &Lua, method: &str, args: &[Value]) -> mlua::Result<MultiValue> {
        let mut out = MultiValue::new();
        match method {
            "getLabel" => out.push_back(Value::String(
                lua.create_string(self.label.borrow().as_str())?,
            )),
            "setLabel" => {
                self.check_writable().map_err(mlua::Error::RuntimeError)?;
                let label: String = opt_string(args, 0, "").trim().chars().take(16).collect();
                *self.label.borrow_mut() = label.clone();
                out.push_back(Value::String(lua.create_string(&label)?));
            }
            "isReadOnly" => out.push_back(Value::Boolean(self.readonly)),
            "spaceTotal" => out.push_back(Value::Integer(i64::MAX)),
            "spaceUsed" => {
                let used = dir_size(&self.root);
                out.push_back(Value::Integer(used as i64));
            }
            "exists" => {
                let path = self
                    .resolve(&arg_string(args, 0)?)
                    .map_err(mlua::Error::RuntimeError)?;
                out.push_back(Value::Boolean(path.exists()));
            }
            "size" => {
                let path = self
                    .resolve(&arg_string(args, 0)?)
                    .map_err(mlua::Error::RuntimeError)?;
                let size = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                out.push_back(Value::Integer(size as i64));
            }
            "isDirectory" => {
                let path = self
                    .resolve(&arg_string(args, 0)?)
                    .map_err(mlua::Error::RuntimeError)?;
                out.push_back(Value::Boolean(path.is_dir()));
            }
            "lastModified" => {
                let path = self
                    .resolve(&arg_string(args, 0)?)
                    .map_err(mlua::Error::RuntimeError)?;
                let modified = fs::metadata(&path)
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0);
                out.push_back(Value::Integer(modified));
            }
            "list" => {
                let path = self
                    .resolve(&arg_string(args, 0)?)
                    .map_err(mlua::Error::RuntimeError)?;
                let table = lua.create_table()?;
                if let Ok(entries) = fs::read_dir(&path) {
                    let mut names: Vec<String> = entries
                        .filter_map(|e| e.ok())
                        .map(|e| e.file_name().to_string_lossy().to_string())
                        .collect();
                    names.sort();
                    for (i, name) in names.iter().enumerate() {
                        table.set(i + 1, name.as_str())?;
                    }
                }
                out.push_back(Value::Table(table));
            }
            "makeDirectory" => {
                self.check_writable().map_err(mlua::Error::RuntimeError)?;
                let path = self
                    .resolve(&arg_string(args, 0)?)
                    .map_err(mlua::Error::RuntimeError)?;
                let success = if path.exists() {
                    false
                } else {
                    fs::create_dir_all(&path).is_ok()
                };
                out.push_back(Value::Boolean(success));
            }
            "remove" => {
                self.check_writable().map_err(mlua::Error::RuntimeError)?;
                let path = self
                    .resolve(&arg_string(args, 0)?)
                    .map_err(mlua::Error::RuntimeError)?;
                let success = if path.is_dir() {
                    fs::remove_dir_all(&path).is_ok()
                } else {
                    fs::remove_file(&path).is_ok()
                };
                out.push_back(Value::Boolean(success));
            }
            "rename" => {
                self.check_writable().map_err(mlua::Error::RuntimeError)?;
                let from = self
                    .resolve(&arg_string(args, 0)?)
                    .map_err(mlua::Error::RuntimeError)?;
                let to = self
                    .resolve(&arg_string(args, 1)?)
                    .map_err(mlua::Error::RuntimeError)?;
                out.push_back(Value::Boolean(fs::rename(&from, &to).is_ok()));
            }
            "open" => {
                let path = self
                    .resolve(&arg_string(args, 0)?)
                    .map_err(mlua::Error::RuntimeError)?;
                let mode = opt_string(args, 1, "r");
                let file = open_file(&path, &mode, self.readonly)?;
                let mut handles = self.handles.borrow_mut();
                let mut next = self.next_handle.borrow_mut();
                let handle = *next;
                *next += 1;
                handles.insert(handle, OpenFile { file });
                out.push_back(Value::Integer(handle));
            }
            "read" => {
                let handle = arg_int32(args, 0)? as i64;
                let count = arg_int32(args, 1)? as usize;
                let count = count.min(1024 * 1024);
                let mut handles = self.handles.borrow_mut();
                let file = handles
                    .get_mut(&handle)
                    .ok_or_else(|| mlua::Error::RuntimeError("bad file descriptor".into()))?;
                let mut buffer = vec![0u8; count];
                let read = file
                    .file
                    .read(&mut buffer)
                    .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
                if read == 0 {
                    out.push_back(Value::Nil);
                } else {
                    buffer.truncate(read);
                    out.push_back(Value::String(lua.create_string(&buffer)?));
                }
            }
            "seek" => {
                let handle = arg_int32(args, 0)? as i64;
                let whence = arg_string(args, 1)?;
                let offset = arg_int32(args, 2)? as i64;
                let mut handles = self.handles.borrow_mut();
                let file = handles
                    .get_mut(&handle)
                    .ok_or_else(|| mlua::Error::RuntimeError("bad file descriptor".into()))?;
                let pos = match whence.as_str() {
                    "cur" => file
                        .file
                        .seek(SeekFrom::Current(offset))
                        .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?,
                    "set" => file
                        .file
                        .seek(SeekFrom::Start(offset.max(0) as u64))
                        .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?,
                    "end" => file
                        .file
                        .seek(SeekFrom::End(offset))
                        .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?,
                    _ => return Err(mlua::Error::RuntimeError("invalid mode".into())),
                };
                out.push_back(Value::Integer(pos as i64));
            }
            "write" => {
                self.check_writable().map_err(mlua::Error::RuntimeError)?;
                let handle = arg_int32(args, 0)? as i64;
                let data = arg_bytes(args, 1)?;
                let mut handles = self.handles.borrow_mut();
                let file = handles
                    .get_mut(&handle)
                    .ok_or_else(|| mlua::Error::RuntimeError("bad file descriptor".into()))?;
                file.file
                    .write_all(&data)
                    .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
                file.file
                    .flush()
                    .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
                out.push_back(Value::Boolean(true));
            }
            "close" => {
                let handle = arg_int32(args, 0)? as i64;
                self.handles.borrow_mut().remove(&handle);
            }
            _ => return Err(mlua::Error::RuntimeError("no such method".into())),
        }
        Ok(out)
    }

    fn device_info(&self) -> Vec<(String, String)> {
        vec![
            ("class".into(), "volume".into()),
            ("description".into(), "Filesystem".into()),
            ("product".into(), "MPFS.21.6".into()),
        ]
    }
}

fn open_file(path: &Path, mode: &str, readonly: bool) -> mlua::Result<fs::File> {
    let read = matches!(mode, "r" | "rb");
    let write = matches!(mode, "w" | "wb");
    let append = matches!(mode, "a" | "ab");
    if !read && !write && !append {
        return Err(mlua::Error::RuntimeError("unsupported mode".into()));
    }
    if !read && readonly {
        return Err(mlua::Error::RuntimeError("filesystem is readonly".into()));
    }
    if read {
        return fs::File::open(path).map_err(|_| mlua::Error::RuntimeError("file not found".into()));
    }
    let mut options = fs::OpenOptions::new();
    options.create(true).write(true);
    if write {
        options.truncate(true);
    } else {
        options.append(true);
    }
    options
        .open(path)
        .map_err(|e| mlua::Error::RuntimeError(e.to_string()))
}

fn dir_size(path: &Path) -> u64 {
    let mut total = 0;
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                total += dir_size(&p);
            } else if let Ok(meta) = entry.metadata() {
                total += meta.len();
            }
        }
    }
    total
}
