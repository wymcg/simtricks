mod plugin_thread;
mod simulator_app;
mod matrix_config;

use crate::simulator_app::SimulatorApp;
use eframe::{egui, NativeOptions};

const WINDOW_WIDTH_INITIAL: f32 = 800.0;
const WINDOW_HEIGHT_INITIAL: f32 = 850.0;

fn main() {
    // Setup window options
    let options = NativeOptions {
        initial_window_size: Some(egui::Vec2::new(WINDOW_WIDTH_INITIAL, WINDOW_HEIGHT_INITIAL)),
        ..Default::default()
    };

    // Start the GUI
    eframe::run_native(
        "Simtricks",
        options,
        Box::new(|_cc| Box::<SimulatorApp>::default()),
    )
    .expect("Unable to start egui app!");
}
