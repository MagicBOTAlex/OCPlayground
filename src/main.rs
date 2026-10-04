#![allow(dead_code)]

mod buffer;
mod color;
mod components;
mod config;
mod machine;
mod run;
mod term;

use clap::Parser;
use std::path::PathBuf;

/// Console emulator for OpenComputers.
#[derive(Parser, Debug)]
#[command(name = "ocplay", version, about = "Run OpenComputers in the terminal")]
struct Cli {
    /// Attach the terminal as a keyboard and drop into the OpenOS shell after
    /// running the script. Without this flag the emulator runs non-interactively.
    #[arg(long)]
    interactive: bool,

    /// Stop after the given number of seconds (0 means no limit).
    #[arg(long)]
    timeout: Option<f64>,

    /// The computer configuration file (computer.yaml).
    config: PathBuf,

    /// Lua script to run once OpenOS has booted.
    script: Option<PathBuf>,
}

fn main() {
    let cli = Cli::parse();
    let options = run::RunOptions {
        interactive: cli.interactive,
        timeout: cli.timeout,
        script: cli.script,
    };
    match run::run(&cli.config, options) {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("ocplay: {:#}", error);
            std::process::exit(1);
        }
    }
}
