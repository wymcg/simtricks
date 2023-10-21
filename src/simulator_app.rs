use crate::plugin_logs;
use eframe::egui::{Context, Pos2, Rect, Rounding, Sense, Vec2};
use eframe::{egui, App, Frame};
use extism::manifest::Wasm;
use extism::{Function, Manifest, Plugin, ValType};
use std::collections::BTreeMap;
use std::fs::read;
use std::path::PathBuf;
use std::str::from_utf8;
use std::time::{Duration, Instant};
use eframe::emath::RectTransform;

/// A simulator for a single Matricks plugin
pub(crate) struct Simulator<'a> {
    /// An Extism plugin designed to work with Matricks.
    plugin: Plugin<'a>,

    /// The last frame retrieved from the plugin
    frame: Vec<Vec<[u8; 4]>>,

    /// The dimensions of the matrix (width in number of LEDs, height in number of LEDs)
    matrix_dimensions: (usize, usize),

    /// Frames per second
    fps: f64,

    /// The time at which the last frame was retrieved from the plugin
    time_at_last_frame: Instant,

    /// The amount of time to wait before requesting another frame from the plugin
    time_per_frame: Duration,

    /// Request the next frame once the appropriate amount of time (`time_per_frame`) has elapsed
    autoplay: bool,

    /// If true, stop playing the plugin, and do not allow the user to continue using the simulator.
    freeze: bool,
}

/// Utility functions
impl Simulator<'_> {
    /// Create a new simulator for a plugin
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the plugin to simulate
    /// * `matrix_dimensions` - The dimensions of the matrix. Width, then height.
    /// * `fps` - Frames per second
    /// * `allowed_hosts` - Hosts to allow the plugin to communicate with
    /// * `path_maps` - Local paths to map to the plugin filesystem, as two paths separated by a '>'.
    pub(crate) fn new(
        path: PathBuf,
        matrix_dimensions: (usize, usize),
        fps: f64,
        allowed_hosts: Vec<String>,
        path_maps: Vec<(PathBuf, PathBuf)>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        // Determine number of milliseconds between each frame
        let time_per_frame = Duration::from_nanos((1_000_000_000.0 / fps).round() as u64);

        // Pull WASM data from the given file
        let wasm_data = read(path)?;
        let wasm = Wasm::from(wasm_data);

        // Create a new manifest for the plugin
        let manifest = Manifest::new([wasm])
            .with_allowed_hosts(allowed_hosts.into_iter())
            .with_allowed_paths(path_maps.into_iter());

        // Create the config
        let mut matricks_config: BTreeMap<String, Option<String>> = BTreeMap::new();
        matricks_config.insert(
            String::from("width"),
            Some(format!("{}", matrix_dimensions.0)),
        );
        matricks_config.insert(
            String::from("height"),
            Some(format!("{}", matrix_dimensions.1)),
        );
        matricks_config.insert(String::from("target_fps"), Some(format!("{}", fps)));
        matricks_config.insert(String::from("serpentine"), Some(format!("{}", true)));
        matricks_config.insert(String::from("brightness"), Some(format!("{}", 255u8)));

        // Setup the host functions
        let plugin_debug_log_function = Function::new(
            "matricks_debug",
            [ValType::I64],
            [],
            None,
            plugin_logs::plugin_debug_log,
        );
        let plugin_info_log_function = Function::new(
            "matricks_info",
            [ValType::I64],
            [],
            None,
            plugin_logs::plugin_info_log,
        );
        let plugin_warn_log_function = Function::new(
            "matricks_warn",
            [ValType::I64],
            [],
            None,
            plugin_logs::plugin_warn_log,
        );
        let plugin_error_log_function = Function::new(
            "matricks_error",
            [ValType::I64],
            [],
            None,
            plugin_logs::plugin_error_log,
        );
        let plugin_functions = [
            plugin_debug_log_function,
            plugin_info_log_function,
            plugin_warn_log_function,
            plugin_error_log_function,
        ];

        // Create the plugin
        let mut plugin = Plugin::create_with_manifest(&manifest, plugin_functions, true)?
            .with_config(&matricks_config)?;

        // Call setup function of current active plugin
        match plugin.call("setup", "") {
            Ok(_) => {
                log::info!("Successfully set up plugin.");
            }
            Err(e) => {
                log::warn!("Failed to set up plugin.");
                log::debug!("Failed to set up plugin with following error: {e}");
            }
        };

        Ok(Self {
            plugin,
            frame: vec![vec![[0; 4]; matrix_dimensions.0]; matrix_dimensions.1],
            matrix_dimensions,
            fps,
            time_per_frame,
            time_at_last_frame: Instant::now(),
            autoplay: true,
            freeze: false,
        })
    }
}

/// Simulator management functions
impl Simulator<'_> {
    /// Check whether the minimum time between frames has elapsed
    fn frame_time_elapsed(&self) -> bool {
        Instant::now().duration_since(self.time_at_last_frame) >= self.time_per_frame
    }

    /// Get the next matrix state from the plugin.
    fn get_next_state(&mut self) -> Result<Option<Vec<Vec<[u8; 4]>>>, ()> {
        // Attempt to pull the next frame from the plugin, as a UTF8 JSON string
        let new_state_utf8 = match self.plugin.call("update", "") {
            Ok(utf8) => utf8,
            Err(e) => {
                log::warn!("Failed to receive update from plugin.");
                log::debug!(
                    "Received the following error while polling for update from plugin: {e}"
                );
                return Err(());
            }
        };

        // Convert the UTF8 to a string
        let new_state_str = match from_utf8(new_state_utf8) {
            Ok(str) => str,
            Err(e) => {
                log::warn!("Failed to convert update from UTF8.");
                log::debug!("Received the following error while converting from UTF8: {e}");
                return Err(());
            }
        };

        // Return
        match serde_json::from_str::<Option<Vec<Vec<[u8; 4]>>>>(new_state_str) {
            Ok(update) => Ok(update),
            Err(_) => {
                log::warn!("Invalid update returned from plugin.");
                return Err(());
            }
        }
    }

    /// Go to the next frame
    fn next_frame(&mut self) {
        // Pull a frame from the plugin
        let next_state = match self.get_next_state() {
            Ok(next_state) => next_state,
            Err(_) => {
                log::warn!("Unable to retrieve the next frame from the plugin.");
                self.freeze_simulator();
                return;
            }
        };

        // Replace the previous frame with the new frame
        self.frame = match next_state {
            None => {
                log::info!("Plugin is done providing updates.");
                self.freeze_simulator();
                return;
            }
            Some(state) => state,
        };

        // Reset the time at last frame
        self.time_at_last_frame = Instant::now();
    }

    /// Freeze the simulator
    fn freeze_simulator(&mut self) {
        log::info!("Freezing simulator.");
        self.freeze = true;
    }
}

/// GUI functions
impl Simulator<'_> {
    fn matrix(&mut self, ctx: &Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            // Allocate our painter
            let (response, painter) = ui.allocate_painter(ui.available_size(), Sense::click());

            // Get the relative position of the painter
            let to_screen = RectTransform::from_to(
                Rect::from_min_size(Pos2::ZERO, response.rect.size()),
                response.rect,
            );

            // Calculate the LED sidelength for x and y based on the window size and number of pixels, and choose smallest value for LED sidelength
            let sidelength = [
                response.rect.width() / self.matrix_dimensions.0 as f32, // Sidelength from width
                response.rect.height() / self.matrix_dimensions.1 as f32, // Sidelength from height
            ]
                .iter()
                .min_by(|a, b| a.partial_cmp(b).unwrap()) // Pick smaller of the two
                .unwrap()
                .clone(); // It's still a &f32, so clone it

            for y in 0..self.matrix_dimensions.1 {
                for x in 0..self.matrix_dimensions.0 {
                    // Grab the color of this LED from the last update
                    let led_color = egui::Color32::from_rgba_premultiplied(
                        self.frame[y][x][2],
                        self.frame[y][x][1],
                        self.frame[y][x][0],
                        self.frame[y][x][3],
                    );

                    // Draw the LED
                    painter.rect_filled(
                        Rect::from_min_size(
                            to_screen.transform_pos(Pos2::new(
                                x as f32 * sidelength,
                                y as f32 * sidelength,
                            )),
                            Vec2::new(sidelength, sidelength),
                        ),
                        Rounding::ZERO,
                        led_color,
                    );
                }
            }
        });
    }

    fn bottom_panel(&mut self, ctx: &Context) {
        egui::TopBottomPanel::bottom("controls").show(ctx, |ui| {
            ui.horizontal(|ui| {
                // Add autoplay toggle button
                ui.set_enabled(!self.freeze);
                if ui
                    .add(egui::ImageButton::new(if self.autoplay {
                        egui::include_image!("../assets/pause.png")
                    } else {
                        egui::include_image!("../assets/play.png")
                    }))
                    .clicked()
                {
                    // Toggle autoplay if clicked
                    self.autoplay = !self.autoplay;
                };

                // Add step button
                ui.set_enabled(!self.autoplay && !self.freeze);
                if ui
                    .add(egui::ImageButton::new(egui::include_image!(
                        "../assets/step.png"
                    )))
                    .clicked()
                {
                    // Move to the next frame if clicked
                    self.next_frame();
                }
            })
        });
    }
}

impl App for Simulator<'_> {
    fn update(&mut self, ctx: &Context, _frame: &mut Frame) {
        // Install image loaders, if they aren't already installed
        egui_extras::install_image_loaders(ctx);

        // Force a repaint after the frame time has elapsed
        ctx.request_repaint_after(self.time_per_frame);

        // If autoplay is on, the frame time has elapsed, and the sim isn't frozen, go to the next frame
        if self.autoplay && self.frame_time_elapsed() && !self.freeze {
            self.next_frame();
        }

        // Draw the GUI
        self.bottom_panel(ctx);
        self.matrix(ctx);
    }
}
