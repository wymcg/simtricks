mod simulator_app;
mod plugin_thread;

use eframe::{egui, NativeOptions};
use crate::simulator_app::SimulatorApp;

const WINDOW_WIDTH_INITIAL: f32 = 600.0;
const WINDOW_HEIGHT_INITIAL: f32 = 300.0;

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
        Box::new(|_cc| Box::<SimulatorApp>::default())
    ).expect("Unable to start egui app!");
}
