//! RunBay entry point: argument parsing, single instance check, and startup.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;

use runbay::data::Store;
use runbay::ipc::{self, pipe};
use runbay::ui;

fn main() {
    let mut options = ui::Options {
        data_dir: Store::default_root(),
        autostart: false,
    };
    let mut custom_data_dir = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--autostart" => options.autostart = true,
            // Development option: use another data folder (tests, side-by-side debugging).
            "--data-dir" => {
                if let Some(dir) = args.next() {
                    options.data_dir = std::path::absolute(PathBuf::from(dir))
                        .unwrap_or_else(|_| PathBuf::from(&options.data_dir));
                    custom_data_dir = true;
                }
            }
            _ => {}
        }
    }
    runbay::logging::init(&options.data_dir);
    if let Err(e) = run(options, custom_data_dir) {
        log::error!("{e:#}");
        runbay::platform::error_box("RunBay", &format!("{e:#}"));
        std::process::exit(1);
    }
}

fn run(options: ui::Options, custom_data_dir: bool) -> anyhow::Result<()> {
    let name = pipe::pipe_name(custom_data_dir.then_some(options.data_dir.as_path()))?;
    match pipe::PipeServer::bind(&name) {
        Ok(listener) => ui::run(options, listener),
        Err(bind_error) => {
            if ipc::activate_running_instance(&name)? {
                Ok(())
            } else {
                Err(bind_error)
            }
        }
    }
}
