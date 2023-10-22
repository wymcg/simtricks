use crate::plugin_logs;
use crate::plugin_thread::plugin_thread;
use eframe::egui::{Context, Key, Modifiers, Pos2, Rect, Rounding, Sense, Vec2};
use eframe::emath::RectTransform;
use eframe::{egui, App, Frame};
use extism::manifest::Wasm;
use extism::{Function, Manifest, Plugin, ValType};
use std::collections::BTreeMap;
use std::error::Error;
use std::fs::read;
use std::ops::DerefMut;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;

/// A simulator for a single Matricks plugin
pub(crate) struct Simulator {
    /// Path to the plugin to simulate
    path: PathBuf,

    /// Network hosts that the plugin may communicate with
    allowed_hosts: Vec<String>,

    /// Map a location on the host filesystem to the plugin filesystem
    path_maps: Vec<(PathBuf, PathBuf)>,

    /// The last frame retrieved from the plugin
    frame: Arc<Mutex<Vec<Vec<[u8; 4]>>>>,

    /// The dimensions of the matrix (width in number of LEDs, height in number of LEDs)
    matrix_dimensions: (usize, usize),

    /// Frames per second
    fps: f32,

    /// If true, a new plugin thread should be created
    create_plugin_thread: bool,

    /// If true, the plugin thread should generate a new frame
    generate_frame: Arc<Mutex<bool>>,

    /// If true, the plugin thread should automatically generate new frames, no matter what `generate_frame` is
    autoplay: Arc<Mutex<bool>>,

    /// If true, do not allow the user to continue to interact with the UI
    freeze: Arc<Mutex<bool>>,

    /// If true, tell the current plugin thread to quit
    stop_plugin_thread: Arc<Mutex<bool>>,
}

/// Utility functions
impl Simulator {
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
        fps: f32,
        allowed_hosts: Vec<String>,
        path_maps: Vec<(PathBuf, PathBuf)>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            path,
            allowed_hosts,
            path_maps,
            frame: Arc::new(Mutex::new(vec![
                vec![[0; 4]; matrix_dimensions.0];
                matrix_dimensions.1
            ])),
            matrix_dimensions,
            fps,
            create_plugin_thread: true,
            generate_frame: Arc::new(Mutex::new(false)),
            autoplay: Arc::new(Mutex::new(false)),
            freeze: Arc::new(Mutex::new(false)),
            stop_plugin_thread: Arc::new(Mutex::new(false)),
        })
    }

    fn spawn_thread(&mut self) -> Result<(), Box<dyn Error>> {
        log::info!("Spawning a new plugin thread.");

        // Reset relevant plugin flags
        self.create_plugin_thread = false;
        {
            *self.stop_plugin_thread.lock().unwrap() = false;
        }
        {
            *self.generate_frame.lock().unwrap() = true;
        }

        // Pull WASM data from the given file
        let wasm_data = read(self.path.clone())?;
        let wasm = Wasm::from(wasm_data);

        // Create a new manifest for the plugin
        let manifest = Manifest::new([wasm])
            .with_allowed_hosts(self.allowed_hosts.clone().into_iter())
            .with_allowed_paths(self.path_maps.clone().into_iter());

        // Create the config
        let mut matricks_config: BTreeMap<String, Option<String>> = BTreeMap::new();
        matricks_config.insert(
            String::from("width"),
            Some(format!("{}", self.matrix_dimensions.0)),
        );
        matricks_config.insert(
            String::from("height"),
            Some(format!("{}", self.matrix_dimensions.1)),
        );
        matricks_config.insert(String::from("target_fps"), Some(format!("{}", self.fps)));
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
        let plugin = Plugin::create_with_manifest(&manifest, plugin_functions.clone(), true)?
            .with_config(&matricks_config)?;

        // Setup and spawn the plugin thread
        {
            let frame = Arc::clone(&self.frame);
            let generate_frame = Arc::clone(&self.generate_frame);
            let autoplay = Arc::clone(&self.autoplay);
            let freeze = Arc::clone(&self.freeze);
            let stop_plugin_thread = Arc::clone(&self.stop_plugin_thread);
            let fps = self.fps.clone();
            thread::spawn(move || {
                plugin_thread(
                    plugin,
                    fps,
                    frame,
                    generate_frame,
                    autoplay,
                    freeze,
                    stop_plugin_thread,
                )
            });
        }

        Ok(())
    }
}

/// Control functions
impl Simulator {
    /// Play/pause the plugin
    fn toggle_autoplay(&mut self) {
        let mut autoplay = self.autoplay.lock().unwrap();
        *autoplay = !*autoplay;
    }

    /// Go to the next frame
    fn step(&mut self) {
        // Tell the plugin update thread to generate a new frame
        let mut generate_frame_flag = self.generate_frame.lock().unwrap();
        *generate_frame_flag = true;
    }

    /// Kill the current plugin thread and create a new one
    fn restart(&mut self) {
        // Clear the current frame
        {
            *self.frame.lock().unwrap() =
                vec![vec![[0; 4]; self.matrix_dimensions.0]; self.matrix_dimensions.1];
        }

        // Signal that the existing plugin thread should be stopped
        {
            *self.stop_plugin_thread.lock().unwrap() = true;
        }

        // Signal that a new plugin thread should be created
        self.create_plugin_thread = true;
    }

    /// Handle any keyboard shortcuts
    fn consume_shortcuts(&mut self, ctx: &Context) {
        ctx.input_mut(|input_state| {
            // If space is pressed, toggle autoplay
            if input_state.consume_key(Modifiers::NONE, Key::Space) {
                self.toggle_autoplay();
            }

            // If 'N' or right arrow is pressed and autoplay is off, step to the next frame
            if (input_state.consume_key(Modifiers::NONE, Key::N)
                || input_state.consume_key(Modifiers::NONE, Key::ArrowRight))
                && !*self.autoplay.lock().unwrap()
            {
                self.step();
            }

            // If 'R' is pressed, restart the plugin
            if input_state.consume_key(Modifiers::NONE, Key::R) {
                self.restart()
            }
        });
    }
}

/// GUI functions
impl Simulator {
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

            // Grab the frame
            let mut frame = self.frame.lock().unwrap();
            let frame = frame.deref_mut();

            for y in 0..self.matrix_dimensions.1 {
                for x in 0..self.matrix_dimensions.0 {
                    // Grab the color of this LED from the last update
                    let led_color = egui::Color32::from_rgba_premultiplied(
                        frame[y][x][2],
                        frame[y][x][1],
                        frame[y][x][0],
                        frame[y][x][3],
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

    fn top_panel(&mut self, ctx: &Context) {
        egui::TopBottomPanel::top("controls").show(ctx, |ui| {
            ui.horizontal(|ui| {
                // Add autoplay toggle button
                if ui
                    .add_enabled(
                        !*self.freeze.lock().unwrap(),
                        egui::ImageButton::new(if *self.autoplay.lock().unwrap() {
                            egui::include_image!("../assets/pause.png")
                        } else {
                            egui::include_image!("../assets/play.png")
                        }),
                    )
                    .on_hover_text("Play/pause plugin (space)")
                    .clicked()
                {
                    self.toggle_autoplay();
                };

                // Add step button
                if ui
                    .add_enabled(
                        !*self.autoplay.lock().unwrap() && !*self.freeze.lock().unwrap(),
                        egui::ImageButton::new(egui::include_image!("../assets/step.png")),
                    )
                    .on_hover_text("Step to next frame (N)")
                    .clicked()
                {
                    self.step();
                }

                // Add plugin restart button
                if ui
                    .add_enabled(
                        true,
                        egui::ImageButton::new(egui::include_image!("../assets/restart.png")),
                    )
                    .on_hover_text("Restart plugin (R)")
                    .clicked()
                {
                    self.restart();
                }
            });
        });
    }
}

impl App for Simulator {
    fn update(&mut self, ctx: &Context, _frame: &mut Frame) {
        // Create a new plugin thread, if there isn't one already
        if self.create_plugin_thread {
            match self.spawn_thread() {
                Ok(_) => {
                    // Unfreeze the simulator
                    *self.freeze.lock().unwrap() = false;
                }
                Err(_) => {
                    log::error!("Failed to create a new plugin thread.");
                    *self.freeze.lock().unwrap() = true;
                }
            };
        }

        // Handle keyboard shortcuts
        self.consume_shortcuts(ctx);

        // Install image loaders, if they aren't already installed
        egui_extras::install_image_loaders(ctx);

        // Force a repaint
        ctx.request_repaint();

        // Draw the GUI
        self.top_panel(ctx);
        self.matrix(ctx);
    }
}
