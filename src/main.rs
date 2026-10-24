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
        headless: false,
        custom_data_dir: false,
    };
    let mut mcp = false;
    let mut updated = false;
    let mut custom_data_dir = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--autostart" => options.autostart = true,
            "--mcp" => mcp = true,
            // Restarted by an update: the previous instance is still releasing the pipe.
            "--updated" => updated = true,
            // Development option for tests: serve the pipe without a window.
            "--headless" => options.headless = true,
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
    if mcp {
        // The bridge has no window; errors go to the agent through stderr.
        let result = pipe::pipe_name(custom_data_dir.then_some(options.data_dir.as_path()))
            .and_then(|name| {
                runbay::mcp::run_bridge(runbay::mcp::BridgeOptions {
                    pipe_name: name,
                    gui_args: runbay::mcp::gui_args(custom_data_dir.then_some(&options.data_dir)),
                })
            });
        if let Err(e) = result {
            eprintln!("RunBay MCP bridge: {e:#}");
            std::process::exit(1);
        }
        return;
    }
    options.custom_data_dir = custom_data_dir;
    runbay::logging::init(&options.data_dir);
    let headless = options.headless;
    if let Ok(exe) = std::env::current_exe() {
        // Left by an update; the previous process may still be exiting.
        std::thread::spawn(move || runbay::update::remove_old(&exe));
    }
    if let Err(e) = run(options, custom_data_dir, updated) {
        log::error!("{e:#}");
        if !headless {
            runbay::platform::error_box("RunBay", &format!("{e:#}"));
        }
        std::process::exit(1);
    }
}

fn run(options: ui::Options, custom_data_dir: bool, updated: bool) -> anyhow::Result<()> {
    let name = pipe::pipe_name(custom_data_dir.then_some(options.data_dir.as_path()))?;
    if updated {
        // Wait for the previous version to exit instead of activating it.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        loop {
            match pipe::PipeServer::bind(&name) {
                Ok(listener) => return ui::run(options, listener),
                Err(e) if std::time::Instant::now() >= deadline => return Err(e),
                Err(_) => std::thread::sleep(std::time::Duration::from_millis(200)),
            }
        }
    }
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
