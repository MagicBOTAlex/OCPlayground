//! Wires the configuration, components, machine and terminal together and runs
//! the emulator.

use crate::buffer::TextBuffer;
use crate::color::ColorDepth;
use crate::components::computer::Computer;
use crate::components::eeprom::Eeprom;
use crate::components::filesystem::HostFileSystem;
use crate::components::gpu::Gpu;
use crate::components::internet::Internet;
use crate::components::keyboard::Keyboard;
use crate::components::screen::Screen;
use crate::components::{new_signal_queue, Registry};
use crate::config::{self, Config};
use crate::machine::{Host, Machine, Step};
use crate::term::Output;
use anyhow::{Context, Result};
use mlua::MultiValue;
use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

pub struct RunOptions {
    pub interactive: bool,
    pub timeout: Option<f64>,
    pub script: Option<PathBuf>,
}

pub fn run(config_path: &Path, options: RunOptions) -> Result<i32> {
    let config = Config::load(config_path)?;
    let system_dir = config::system_dir()?;

    let machine_src = fs::read_to_string(system_dir.join("machine.lua"))
        .context("failed to read machine.lua")?;
    let bios = read_bios(&config, &system_dir)?;

    let interactive = options.interactive || config.run.interactive;
    let timeout = options
        .timeout
        .or_else(|| (config.run.timeout > 0.0).then_some(config.run.timeout))
        .unwrap_or(0.0);
    let script = options
        .script
        .clone()
        .or_else(|| config.run.script.as_ref().map(PathBuf::from));

    // --- shared resources -------------------------------------------------
    let signals = new_signal_queue();
    let registry = Rc::new(RefCell::new(Registry::new()));
    let max_depth = depth_from_bits(config.gpu.screen.max_depth);
    let buffer = Rc::new(RefCell::new(TextBuffer::new(
        config.gpu.screen.width.max(1),
        config.gpu.screen.height.max(1),
        max_depth,
    )));

    // --- filesystems ------------------------------------------------------
    let mut mount_lines: Vec<String> = Vec::new();
    let mut root_address: Option<String> = None;
    let mut _temp_root: Option<tempfile::TempDir> = None;

    let filesystems = effective_filesystems(&config);
    for fs_config in &filesystems {
        let is_root = fs_config.mount == "/";
        let (dir, readonly) = if fs_config.path == "builtin" {
            if is_root && !fs_config.readonly {
                let temp = tempfile::Builder::new()
                    .prefix("ocplay-")
                    .tempdir()
                    .context("failed to create session directory")?;
                copy_dir(&system_dir.join("loot/openos"), temp.path())?;
                let path = temp.path().to_path_buf();
                _temp_root = Some(temp);
                (path, false)
            } else {
                (system_dir.join("loot/openos"), true)
            }
        } else {
            (PathBuf::from(&fs_config.path), fs_config.readonly)
        };

        let component = Rc::new(HostFileSystem::new(
            dir.clone(),
            fs_config.label.clone(),
            readonly,
        ));
        let address = registry.borrow_mut().add(component);
        if is_root {
            root_address = Some(address.clone());
        } else {
            mount_lines.push(format!(
                "  require(\"filesystem\").mount(\"{}\", \"{}\")",
                address, fs_config.mount
            ));
        }
    }

    let root_address = match root_address {
        Some(address) => address,
        None => anyhow::bail!("no root filesystem configured (need a filesystem mounted at \"/\")"),
    };

    // --- script injection -------------------------------------------------
    let root_dir = root_path(&filesystems, &system_dir, &_temp_root);
    if let (Some(script), Some(root_dir)) = (&script, root_dir.as_ref()) {
        inject_script(root_dir, script, interactive)?;
    }
    let has_script = script.is_some();

    // --- components -------------------------------------------------------
    let screen = Rc::new(Screen::new(
        buffer.clone(),
        (config.gpu.screen.width as f64, config.gpu.screen.height as f64),
    ));
    let screen_address = registry.borrow_mut().add(screen.clone());

    let gpu = Rc::new(Gpu::new(
        buffer.clone(),
        depth_from_tier(config.gpu.tier),
    ));
    let gpu_address = registry.borrow_mut().add(gpu);
    let _ = &gpu_address;

    let keyboard = Rc::new(Keyboard::new(signals.clone()));
    let keyboard_address = registry.borrow_mut().add(keyboard.clone());
    *keyboard.address.borrow_mut() = keyboard_address.clone();
    screen.keyboards.borrow_mut().push(keyboard_address);
    let _ = &screen_address;

    let eeprom = Rc::new(Eeprom::new(bios));
    let eeprom_address = registry.borrow_mut().add(eeprom.clone());

    let internet = Rc::new(Internet::new(
        config.internet.enabled,
        config.internet.timeout,
        config.internet.user_agent.clone(),
        config.internet.tcp,
    ));
    let _internet_address = registry.borrow_mut().add(internet);

    // Set the boot device.
    let boot = if config.eeprom.boot == "auto" {
        Some(root_address.clone())
    } else {
        registry
            .borrow()
            .entries()
            .iter()
            .find(|entry| {
                entry.type_name == "filesystem"
                    && entry.address == config.eeprom.boot
            })
            .map(|entry| entry.address.clone())
            .or_else(|| Some(root_address.clone()))
    };
    if let Some(address) = &boot {
        *eeprom.data.borrow_mut() = address.clone().into_bytes();
    }

    let computer_address = registry.borrow_mut().next_address();
    let users = Rc::new(RefCell::new(vec![crate::components::computer::user_name()]));
    let computer = Rc::new(Computer::new(
        computer_address.clone(),
        registry.clone(),
        users.clone(),
    ));
    registry
        .borrow_mut()
        .add_with_address(computer_address.clone(), computer);
    let _ = &eeprom_address;

    // Write the rc.d autostart now that all component addresses are known.
    if let Some(root_dir) = root_dir.as_ref() {
        inject_rc(root_dir, &mount_lines, has_script, interactive)?;
    }

    // --- machine ----------------------------------------------------------
    let host = Rc::new(Host {
        signals,
        start: Instant::now(),
        registry: registry.clone(),
        computer_address,
        boot_address: RefCell::new(boot),
        tmp_address: RefCell::new(None),
        timeout: 10.0,
        memory_kib: config.memory,
        screen: buffer.clone(),
        keyboard: keyboard.clone(),
        users,
    });

    let mut machine = Machine::create(host.clone(), &machine_src)?;
    let mut term = Output::new(interactive, false)?;

    let result = run_loop(
        &mut machine,
        &mut term,
        interactive,
        timeout,
        host.start,
    );
    let is_tty = term.is_tty();
    // Keep the final screen visible for a moment after the machine stops.
    if config.run.terminate_delay > 0.0 {
        wait_terminate_delay(&mut term, &host.screen, config.run.terminate_delay);
    }
    term.restore();
    if !is_tty {
        crate::term::dump_screen(&host.screen.borrow());
    }
    result
}

/// Hold the final screen for `seconds`, but let the user cut it short with
/// Ctrl+C. In raw mode Ctrl+C arrives as a key event rather than SIGINT, so we
/// poll for input instead of sleeping.
fn wait_terminate_delay(term: &mut Output, screen: &Rc<RefCell<TextBuffer>>, seconds: f64) {
    let _ = term.render(&screen.borrow());
    let deadline = Instant::now() + Duration::from_secs_f64(seconds);
    while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
        let slice = remaining.min(Duration::from_millis(50));
        match term.poll(slice) {
            Ok(inputs) => {
                if inputs.iter().any(is_ctrl_c) {
                    break;
                }
            }
            Err(_) => break,
        }
        let _ = term.render(&screen.borrow());
    }
}

fn is_ctrl_c(input: &crate::term::KeyInput) -> bool {
    input.down && input.character == '\u{3}'
}

fn run_loop(
    machine: &mut Machine,
    term: &mut Output,
    interactive: bool,
    timeout: f64,
    start: Instant,
) -> Result<i32> {
    let mut resume_args = MultiValue::new();
    let mut exit_code = 0;

    loop {
        let step = machine.step(resume_args);
        let _ = term.render(&machine.host.screen.borrow());

        match step {
            Step::Sleep(seconds) => {
                resume_args = wait_for_signal(machine, term, interactive, seconds, timeout, start)?;
            }
            Step::Yield => {
                resume_args = machine.take_signal_args()?;
            }
            Step::Shutdown(reboot) => {
                if reboot {
                    resume_args = MultiValue::new();
                    continue;
                }
                break;
            }
            Step::Finished(success, message) => {
                if !success {
                    exit_code = 1;
                    if let Some(message) = message {
                        if !message.is_empty() {
                            eprintln!("ocplay: {}", message);
                        }
                    }
                }
                break;
            }
        }

        if timeout > 0.0 && start.elapsed().as_secs_f64() > timeout {
            eprintln!("ocplay: timed out after {:.1}s", timeout);
            break;
        }
    }

    Ok(exit_code)
}

fn wait_for_signal(
    machine: &Machine,
    term: &mut Output,
    interactive: bool,
    seconds: f64,
    timeout: f64,
    start: Instant,
) -> Result<MultiValue> {
    if !machine.host.signals.borrow().is_empty() {
        return machine
            .take_signal_args()
            .map_err(|e| anyhow::anyhow!(e.to_string()));
    }
    let deadline = if seconds.is_finite() {
        Some(Instant::now() + Duration::from_secs_f64(seconds.max(0.0)))
    } else {
        None
    };
    loop {
        let slice = match deadline {
            Some(deadline) => deadline
                .saturating_duration_since(Instant::now())
                .min(Duration::from_millis(30))
                .max(Duration::from_millis(2)),
            None => Duration::from_millis(30),
        };
        let inputs = term.poll(slice)?;
        for input in inputs {
            if input.down {
                machine.host.keyboard.key_down(input.character, input.code);
            } else {
                machine.host.keyboard.key_up(input.character, input.code);
            }
        }
        let _ = term.render(&machine.host.screen.borrow());

        if !machine.host.signals.borrow().is_empty() {
            return machine
                .take_signal_args()
                .map_err(|e| anyhow::anyhow!(e.to_string()));
        }
        if let Some(deadline) = deadline {
            if Instant::now() >= deadline {
                return Ok(MultiValue::new());
            }
        }
        if timeout > 0.0 && start.elapsed().as_secs_f64() > timeout {
            return Ok(MultiValue::new());
        }
        let _ = interactive;
    }
}

fn read_bios(config: &Config, system_dir: &Path) -> Result<Vec<u8>> {
    if config.eeprom.bios == "builtin" {
        fs::read(system_dir.join("bios.lua")).context("failed to read bios.lua")
    } else {
        fs::read(&config.eeprom.bios)
            .with_context(|| format!("failed to read bios {}", config.eeprom.bios))
    }
}

fn depth_from_bits(bits: u8) -> ColorDepth {
    match bits {
        1 => ColorDepth::OneBit,
        4 => ColorDepth::FourBit,
        _ => ColorDepth::EightBit,
    }
}

fn depth_from_tier(tier: u8) -> ColorDepth {
    match tier {
        1 => ColorDepth::OneBit,
        2 => ColorDepth::FourBit,
        _ => ColorDepth::EightBit,
    }
}

fn effective_filesystems(config: &Config) -> Vec<config::FilesystemConfig> {
    if config.filesystems.is_empty() {
        vec![config::FilesystemConfig {
            label: "openos".into(),
            path: "builtin".into(),
            mount: "/".into(),
            readonly: false,
        }]
    } else {
        config.filesystems.clone()
    }
}

fn root_path(
    filesystems: &[config::FilesystemConfig],
    system_dir: &Path,
    temp_root: &Option<tempfile::TempDir>,
) -> Option<PathBuf> {
    let root = filesystems.iter().find(|fs| fs.mount == "/")?;
    if root.path == "builtin" {
        if !root.readonly {
            temp_root.as_ref().map(|t| t.path().to_path_buf())
        } else {
            Some(system_dir.join("loot/openos"))
        }
    } else {
        Some(PathBuf::from(&root.path))
    }
}

fn inject_script(root: &Path, script: &Path, _interactive: bool) -> Result<()> {
    let contents = fs::read(script)
        .with_context(|| format!("failed to read script {}", script.display()))?;
    let home = root.join("home");
    fs::create_dir_all(&home).ok();
    fs::write(home.join("ocplay_autorun.lua"), contents)
        .context("failed to copy script into the emulated filesystem")?;
    Ok(())
}

fn inject_rc(
    root: &Path,
    mount_lines: &[String],
    has_script: bool,
    interactive: bool,
) -> Result<()> {
    let rc_d = root.join("etc/rc.d");
    fs::create_dir_all(&rc_d).ok();

    let mut body = String::from("function start()\n");
    for line in mount_lines {
        body.push_str(line);
        body.push('\n');
    }
    if has_script {
        body.push_str("  pcall(dofile, \"/home/ocplay_autorun.lua\")\n");
    }
    if !interactive {
        body.push_str("  require(\"computer\").shutdown()\n");
    }
    body.push_str("end\n");
    fs::write(rc_d.join("ocplay.lua"), body).context("failed to write rc service")?;

    let cfg_path = root.join("etc/rc.cfg");
    let mut cfg = fs::read_to_string(&cfg_path).unwrap_or_default();
    if !cfg.ends_with('\n') {
        cfg.push('\n');
    }
    cfg.push_str("enabled = {\"ocplay\"}\n");
    fs::write(&cfg_path, cfg).context("failed to enable rc service")?;
    Ok(())
}

fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let source = entry.path();
        let target = to.join(entry.file_name());
        if source.is_dir() {
            copy_dir(&source, &target)?;
        } else {
            fs::copy(&source, &target)?;
        }
    }
    Ok(())
}
