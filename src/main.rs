#![allow(dead_code)]

mod buffer;
mod color;
mod components;
mod config;
mod llm;
mod machine;
mod run;
mod term;
mod upgrade;

use clap::Parser;
use std::path::PathBuf;

/// Console emulator for OpenComputers.
#[derive(Parser, Debug)]
#[command(name = "ocplay", version, about = "Run OpenComputers in the terminal")]
struct Cli {
    /// Attach the terminal as a keyboard and drop into the OpenOS shell after
    /// running the script (Ctrl+C force-quits ocplay; Ctrl+Alt+C interrupts the
    /// guest). Without this flag the emulator runs non-interactively.
    #[arg(long)]
    interactive: bool,

    /// Stop after the given number of seconds (0 means no limit).
    #[arg(long)]
    timeout: Option<f64>,

    /// Update ocplay to the latest release and exit. Not available for
    /// Nix-managed installs (use `nix profile upgrade`).
    #[arg(long)]
    upgrade: bool,

    /// Print an extremely detailed description of how to use ocplay and exit.
    #[arg(long)]
    llm: bool,

    /// The computer configuration file (computer.yaml).
    #[arg(required_unless_present_any = ["upgrade", "llm"])]
    config: Option<PathBuf>,

    /// Lua script to run once OpenOS has booted.
    script: Option<PathBuf>,

    /// Arguments passed to the script (available inside it as `...`).
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

fn main() {
    let cli = Cli::parse();

    if cli.llm {
        print!("{}", llm::describe());
        std::process::exit(0);
    }

    if cli.upgrade {
        let code = match upgrade::upgrade() {
            Ok(code) => code,
            Err(error) => {
                eprintln!("ocplay: {:#}", error);
                1
            }
        };
        std::process::exit(code);
    }

    let config = match cli.config {
        Some(config) => config,
        None => {
            eprintln!("ocplay: a configuration file is required (see --help)");
            std::process::exit(2);
        }
    };
    let options = run::RunOptions {
        interactive: cli.interactive,
        timeout: cli.timeout,
        script: cli.script,
        args: cli.args,
    };
    match run::run(&config, options) {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("ocplay: {:#}", error);
            std::process::exit(1);
        }
    }
}
