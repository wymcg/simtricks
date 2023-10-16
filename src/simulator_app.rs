use crate::plugin_thread::{start_plugin_thread, PluginThread};
use chrono::prelude::*;
use eframe::egui::{Context, Pos2, Rect, Rounding, Sense, Vec2};
use eframe::emath::RectTransform;
use eframe::{egui, App, Frame};
use std::path::PathBuf;
use crate::matrix_config::MatrixConfiguration;

const VERSION: Option<&str> = option_env!("CARGO_PKG_VERSION");

pub struct DisplaySettings {
    pub round_leds: bool,
}

pub struct PluginSettings {
    pub new_allowed_host: String,
    pub new_path_mapping: (String, String),
    pub allowed_hosts: Vec<String>,
    pub path_mappings: Vec<(PathBuf, PathBuf)>,
}

pub struct SimulatorApp {
    plugin_thread: Option<PluginThread>,
    matrix_config: MatrixConfiguration,
    current_matrix_config: MatrixConfiguration,
    status_msg: String,
    last_update: Option<Vec<Vec<[u8; 4]>>>,
    display_settings: DisplaySettings,
    plugin_settings: PluginSettings,
}

impl Default for SimulatorApp {
    fn default() -> Self {
        Self {
            plugin_thread: None,
            matrix_config: MatrixConfiguration::default(),
            current_matrix_config: MatrixConfiguration::default(),
            status_msg: format!("Welcome to Simtricks v{}!", VERSION.unwrap_or("unknown")),
            last_update: None,
            display_settings: DisplaySettings { round_leds: false },
            plugin_settings: PluginSettings {
                new_allowed_host: String::new(),
                new_path_mapping: (String::new(), String::new()),
                allowed_hosts: vec![],
                path_mappings: vec![],
            }
        }
    }
}

impl App for SimulatorApp {
    fn update(&mut self, ctx: &Context, frame: &mut Frame) {
        // Non-gui tasks
        self.check_for_update(ctx);
        self.check_for_log();
        self.check_for_halt();

        // Render the GUI
        self.render_menu_bar(ctx, frame);
        self.render_matrix(ctx, frame);
        self.render_status_bar(ctx, frame);
    }
}

impl SimulatorApp {
    /// Render the menu bar at the top of the screen
    fn render_menu_bar(&mut self, ctx: &Context, _frame: &mut Frame) {
        egui::TopBottomPanel::top("menu bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                // Plugin open functions
                ui.menu_button("File", |ui| {
                    if ui.button("Open").clicked() {
                        // Have the user pick a plugin
                        match rfd::FileDialog::new()
                            .set_title("Choose a plugin")
                            .add_filter("Matricks Plugin", &["wasm", "mtx"])
                            .pick_file()
                        {
                            None => { /* No file picked, so do nothing */ }
                            Some(path) => {
                                self.start_plugin(path.clone());
                            }
                        }
                    }
                });

                // Matrix configuration settings
                ui.menu_button("Matrix", |ui| {
                    ui.add(
                        egui::Slider::new(&mut self.matrix_config.target_fps, 1.0..=60.0)
                            .text("FPS"),
                    );
                    ui.add(egui::Slider::new(&mut self.matrix_config.width, 1..=128).text("Width"));
                    ui.add(
                        egui::Slider::new(&mut self.matrix_config.height, 1..=128).text("Height"),
                    );
                    if ui.button("Reload Matrix").clicked() {
                        // Attempt to pull the path from the current plugin thread struct
                        let path: Option<PathBuf> = match &self.plugin_thread {
                            None => None,
                            Some(plugin_thread) => Some(plugin_thread.path.clone()),
                        };

                        // If we were able to get a path, launch a new plugin with it
                        match path {
                            None => self.set_status_msg(
                                "No active plugin, reload will be ignored".to_string(),
                            ),
                            Some(path) => {
                                self.start_plugin(path);
                                self.set_status_msg("Plugin reloaded.".to_string());
                            }
                        }
                    }
                });

                // Display settings
                ui.menu_button("Display", |ui| {
                    ui.checkbox(&mut self.display_settings.round_leds, "Round LEDs");
                });

                // Plugin settings
                ui.menu_button("Plugin", |ui| {
                    ui.menu_button("Allowed Hosts",  |ui| {
                        // Add a button for each allowed host
                        if self.plugin_settings.allowed_hosts.is_empty() {
                            ui.label("No allowed hosts");
                        } else {
                            // For every allowed host, add a remove button
                            for (allowed_host_index, allowed_host) in self.plugin_settings.allowed_hosts.clone().iter().enumerate() {
                                ui.menu_button(allowed_host, |ui| {
                                    if ui.button("Remove").clicked() {
                                        self.plugin_settings.allowed_hosts.remove(allowed_host_index);
                                    }
                                });
                            }
                        }
                        ui.separator();
                        ui.menu_button("Add Allowed Host", |ui| {
                            ui.horizontal(|ui| {
                                ui.label("Host:");
                                ui.text_edit_singleline(&mut self.plugin_settings.new_allowed_host);
                            });
                            if ui.button("Save host").clicked() {
                                // Don't add the mapping if it already exists in the list
                                if !self.plugin_settings.allowed_hosts.contains(&self.plugin_settings.new_allowed_host) {
                                    self.plugin_settings.allowed_hosts.push(self.plugin_settings.new_allowed_host.clone());
                                }
                                self.plugin_settings.new_allowed_host = String::new();
                            }
                        });
                    });
                    ui.menu_button("Mapped Paths",  |ui| {
                        // Add a button for each allowed host
                        if self.plugin_settings.path_mappings.is_empty() {
                            ui.label("No path mappings");
                        } else {
                            // For every path mapping, add a remove button
                            for (path_mapping_index, path_mapping) in self.plugin_settings.path_mappings.clone().iter().enumerate() {
                                let mapping_string = format!("{} > {}", path_mapping.0.to_str().unwrap(), path_mapping.1.to_str().unwrap());
                                ui.menu_button(mapping_string, |ui| {
                                    if ui.button("Remove").clicked() {
                                        self.plugin_settings.path_mappings.remove(path_mapping_index);
                                    }
                                });
                            }
                        }
                        ui.separator();
                        ui.menu_button("Add Path Mapping", |ui| {
                            ui.horizontal(|ui| {
                                ui.label("Local Path:");
                                ui.text_edit_singleline(&mut self.plugin_settings.new_path_mapping.0);
                            });
                            ui.horizontal(|ui| {
                                ui.label("Plugin Path:");
                                ui.text_edit_singleline(&mut self.plugin_settings.new_path_mapping.1);
                            });
                            if ui.button("Save mapping").clicked() {
                                // Make path buffers from the strings
                                let new_mapping = (PathBuf::from(&self.plugin_settings.new_path_mapping.0), PathBuf::from(&self.plugin_settings.new_path_mapping.1));

                                // Only add the mapping if it doesn't already exist
                                if !self.plugin_settings.path_mappings.contains(&new_mapping) {
                                    self.plugin_settings.path_mappings.push(new_mapping);
                                }

                                self.plugin_settings.new_path_mapping = (String::new(), String::new());
                            }
                        });
                    });
                })
            });
        });
    }

    /// Render the simulated LED matrix
    fn render_matrix(&mut self, ctx: &Context, _frame: &mut Frame) {
        // Get the last update, if there was one
        let last_update = match &self.last_update {
            None => {
                // No last update, so return
                return;
            }
            Some(update) => update
        };

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
                response.rect.width() / self.current_matrix_config.width as f32, // Sidelength from width
                response.rect.height() / self.current_matrix_config.height as f32, // Sidelength from height
            ]
            .iter()
            .min_by(|a, b| a.partial_cmp(b).unwrap()) // Pick smaller of the two
            .unwrap()
            .clone(); // It's still a &f32, so clone it

            // Setup the LED roundness parameter
            let rounding = if self.display_settings.round_leds {
                Rounding::same(sidelength)
            } else {
                Rounding::ZERO
            };

            // Draw the LEDs if the plugin update state is consistent with the current matrix config
            if last_update.len() > 0
                && last_update.len() == self.current_matrix_config.height
                && last_update[0].len() == self.current_matrix_config.width
            {
                for y in 0..self.current_matrix_config.height {
                    for x in 0..self.current_matrix_config.width {
                        // Grab the color of this LED from the last update
                        let led_color = egui::Color32::from_rgba_premultiplied(
                            last_update[y][x][2],
                            last_update[y][x][1],
                            last_update[y][x][0],
                            last_update[y][x][3],
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
                            rounding,
                            led_color,
                        );
                    }
                }
            }
        });
    }

    /// Render the status message at the bottom of the screen
    fn render_status_bar(&mut self, ctx: &Context, _frame: &mut Frame) {
        egui::TopBottomPanel::bottom("status_msg").show(ctx, |ui| {
            ui.horizontal(|ui| {
                // Display the current status message
                ui.label(self.status_msg.clone());

                // Show a loading spinner if there is an active plugin, but not a plugin update
                if self.last_update.is_none() && self.plugin_thread.is_some() {
                    ui.add(egui::Spinner::new());
                }
            });
        });
    }

    /// Set the message in the status bar
    fn set_status_msg(&mut self, msg: String) {
        self.status_msg = format!("[{}]> {msg}", Utc::now().format("%H:%M:%S"));
    }

    /// Start a new plugin thread from a path to a plugin
    fn start_plugin(&mut self, path: PathBuf) {
        // Get a snapshot of the current matrix config
        self.current_matrix_config = self.matrix_config.clone();

        // Clear the last plugin update
        self.last_update = None;

        // Start a new plugin thread
        self.plugin_thread = Some(start_plugin_thread(
            path.clone(),
            self.current_matrix_config.clone(),
            self.plugin_settings.allowed_hosts.clone(),
            self.plugin_settings.path_mappings.clone()
        ));

        // Tell user that a new plugin was started
        self.set_status_msg(format!(
            "Plugin started with path {}",
            path.to_str().unwrap()
        ));
    }

    /// Attempt to receive and handle an update from the currently active plugin, if there is one
    fn check_for_update(&mut self, ctx: &Context) {
        // Grab the plugin thread if there is one, otherwise return without doing anything
        let plugin_thread = match &self.plugin_thread {
            Some(plugin_thread) => plugin_thread,
            None => return
        };

        // Request a repaint
        ctx.request_repaint();

        // Attempt to pull an update from the thread. If there was none, return without doing anything
        let update = match plugin_thread.channels.update_rx.try_recv() {
            Ok(update) => update,
            Err(_) => return
        };

        // Save this update
        self.last_update = Some(update);
    }

    /// Check for logs from the plugin thread
    fn check_for_log(&mut self) {
        let plugin_thread = match &self.plugin_thread {
            None => {
                // No active plugin, so return
                return;
            }
            Some(pt) => pt
        };

        let log = match plugin_thread.channels.log_rx.try_recv() {
            Ok(log) => log,
            Err(_) => {
                // No log, so return
                return;
            }
        };

        self.set_status_msg(log);

    }

    /// Check if the plugin thread has halted or not
    fn check_for_halt(&mut self) {
        let plugin_thread = match &self.plugin_thread {
            None => {
                // No active thread, so return
                return;
            }
            Some(pt) => pt
        };

        // If we receive anything over the halt channel, the thread has stopped (or is stopping)
        if plugin_thread.channels.is_done_rx.try_recv().is_ok() {
            // Clear the current plugin
            self.plugin_thread = None;
        }
    }
}
