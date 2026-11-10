mod core;
#[cfg(feature = "gui")]
mod gui;

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::atomic::AtomicBool;

use clap::Parser;

use core::{Options, parse_password};

/// rentgeny – extract password-protected zips. Runs as a GUI when started
/// without arguments (or with --gui), otherwise as a command-line tool.
#[derive(Debug, Parser)]
#[command(version, about)]
struct Args {
    /// Open the graphical interface (also the default when no arguments are given).
    #[cfg(feature = "gui")]
    #[arg(long)]
    gui: bool,

    /// Password (UTF-8, at most 64 characters). Can also be set via RENTGENY_PASSWORD.
    #[arg(short, long, env = "RENTGENY_PASSWORD", value_parser = parse_password, hide_env_values = true, required_unless_present = "gui")]
    password: Option<String>,

    /// Input directory.
    #[arg(short, long, default_value = ".")]
    input: PathBuf,

    /// Output directory.
    #[arg(short, long, default_value = ".")]
    output: PathBuf,

    /// Delete each zip after it was extracted successfully (zips are kept by default).
    #[arg(short, long)]
    remove_zip: bool,

    /// Extract each zip into a new folder named after the zip file.
    #[arg(short = 'f', long)]
    folder_per_zip: bool,
}

fn run_cli(args: Args) -> Result<(), String> {
    let opts = Options {
        password: args.password.ok_or("password is required")?,
        input: args.input,
        output: args.output,
        remove_zip: args.remove_zip,
        folder_per_zip: args.folder_per_zip,
    };
    core::run(&opts, &AtomicBool::new(false), &mut |event| {
        if event.is_error() {
            eprintln!("{event}");
        } else {
            println!("{event}");
        }
    })
}

fn main() -> ExitCode {
    #[cfg(feature = "gui")]
    if std::env::args_os().len() == 1 && std::env::var_os("RENTGENY_PASSWORD").is_none() {
        return gui::run();
    }

    let args = Args::parse();
    #[cfg(feature = "gui")]
    if args.gui {
        return gui::run();
    }

    match run_cli(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
