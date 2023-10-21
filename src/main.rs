mod clargs;
mod plugin_logs;
mod simulator_app;

use crate::simulator_app::Simulator;
use clap::Parser;
use eframe::{egui, NativeOptions};
use log::LevelFilter;
use simple_logger::SimpleLogger;
use std::path::PathBuf;
use eframe::egui::Visuals;

const VERSION: Option<&str> = option_env!("CARGO_PKG_VERSION");
const DEFAULT_LOG_LEVEL: &str = "simtricks";
const WINDOW_WIDTH_INITIAL: f32 = 500.0;
const WINDOW_HEIGHT_INITIAL: f32 = 550.0;

fn main() {
    // Parse command line arguments
    let args = clargs::SimtricksArgs::parse();

    // Start the logger
    SimpleLogger::new()
        .with_level(LevelFilter::Off)
        .with_module_level(DEFAULT_LOG_LEVEL, LevelFilter::Debug)
        .init()
        .expect("Unable to start logger!");
    log::info!("Starting Simtricks v{}", VERSION.unwrap_or("unknown"));

    // Setup window options
    let options = NativeOptions {
        initial_window_size: Some(egui::Vec2::new(WINDOW_WIDTH_INITIAL, WINDOW_HEIGHT_INITIAL)),
        ..Default::default()
    };

    // Treat command line arguments
    let path = PathBuf::from(args.path);
    let dimensions = (args.width.clone(), args.height.clone());
    let allowed_hosts = args.allow_host.unwrap_or(vec![]);
    let mapped_paths: Vec<(PathBuf, PathBuf)> = args
        .map_path
        .unwrap_or(vec![])
        .iter()
        .map(|map_string| match map_string.split_once('>') {
            None => (PathBuf::from(map_string), PathBuf::from(map_string.clone())),
            Some(res) => (PathBuf::from(res.0), PathBuf::from(res.1)),
        })
        .collect();

    // Create the simulator
    let simulator = match Simulator::new(path, dimensions, args.fps, allowed_hosts, mapped_paths) {
        Ok(sim) => sim,
        Err(e) => {
            log::error!("Failed to create simulator.");
            log::debug!("Recieved the following error while creating the simulator: {e}");
            log::info!("Exiting Simtricks.");
            return;
        }
    };

    // Start the simulator
    match eframe::run_native("Simtricks", options, Box::new(|cc| {
        cc.egui_ctx.style_mut(|style| style.visuals = Visuals::dark());
        Box::new(simulator)
    })) {
        Ok(_) => {}
        Err(e) => {
            log::error!("Failed to start simulator.");
            log::debug!("Simulator failed with the following error: {e}");
            log::info!("Exiting Simtricks.");
        }
    };
}
