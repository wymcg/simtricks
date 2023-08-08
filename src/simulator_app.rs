use crate::plugin_thread::{start_plugin_thread, PluginThread};
use eframe::egui::{Context, Pos2, Rect, Rounding, Sense, Vec2};
use eframe::{egui, App, Frame};
use eframe::emath::RectTransform;
use matricks_plugin::{MatrixConfiguration, PluginUpdate};

const VERSION: Option<&str> = option_env!("CARGO_PKG_VERSION");

pub struct DisplaySettings {
    pub round_leds: bool
}

pub struct SimulatorApp {
    plugin_thread: PluginThread,
    matrix_config: MatrixConfiguration,
    current_matrix_config: MatrixConfiguration,
    status_msg: String,
    last_update: PluginUpdate,
    display_settings: DisplaySettings,
}

impl Default for SimulatorApp {
    fn default() -> Self {
        Self {
            plugin_thread: start_plugin_thread(),
            matrix_config: MatrixConfiguration {
                width: 5,
                height: 5,
                target_fps: 60.0,
                serpentine: false,
                magnification: 1.0,
                brightness: u8::MAX,
            },
            current_matrix_config: MatrixConfiguration::default(),
            status_msg: format!("Welcome to Simtricks v{}", VERSION.unwrap_or("unknown")),
            last_update: PluginUpdate::default(),
            display_settings: DisplaySettings {
                round_leds: false,
            }
        }
    }
}

impl App for SimulatorApp {
    fn update(&mut self, ctx: &Context, _frame: &mut Frame) {
        // Attempt to get an update from the plugin thread
        match self.plugin_thread.channels.update_rx.try_recv() {
            Ok(update) => {
                // Save this update
                self.last_update = update;

                // Request a repaint now that we have a new update
                ctx.request_repaint();
            }
            Err(_) => {/* No update was provided, so do nothing */}
        }

        // Render the menu bar
        egui::TopBottomPanel::top("menu bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                // Plugin open functions
                ui.menu_button("File", |ui| {
                    if ui.button("Open").clicked() {
                        // Stop the current plugin
                        self.plugin_thread
                            .channels
                            .next_plugin_tx
                            .send(())
                            .expect("Unable to stop active plugin!");

                        // Have the user pick a plugin
                        let path = rfd::FileDialog::new()
                            .set_title("Choose a plugin")
                            .add_filter("Matricks Plugin", &["wasm", "plug"])
                            .pick_file();

                        // Get a snapshot of the current matrix config
                        self.current_matrix_config = self.matrix_config.clone();

                        // Tell the plugin thread to start the new plugin
                        match path {
                            None => {}
                            Some(path) => {

                                self.set_status_msg(format!("Starting plugin at path {}", path.to_str().unwrap()));

                                // Send the path of the new plugin to the plugin thread
                                self.plugin_thread
                                    .channels
                                    .plugin_info_tx
                                    .send((path, self.current_matrix_config.clone()))
                                    .expect("Unable to send path to plugin thread!");
                            }
                        }
                    }
                });

                // Matrix configuration settings
                ui.menu_button("Matrix", |ui| {
                    ui.add(egui::Slider::new(&mut self.matrix_config.target_fps, 1.0..=144.0).text("FPS"));
                    ui.add(egui::Slider::new(&mut self.matrix_config.width, 1..=500).text("Width"));
                    ui.add(egui::Slider::new(&mut self.matrix_config.height, 1..=500).text("Height"));
                    ui.label("Note: any changes made here will not be reflected until a new plugin is started!");
                });

                // Display settings
                ui.menu_button("Display", |ui| {
                    ui.checkbox(&mut self.display_settings.round_leds, "Round LEDs");
                });
            });
        });

        // Render the simulated matrix
        egui::CentralPanel::default().show(ctx, |ui| {
            // Allocate our painter
            let (response, painter) = ui.allocate_painter(
                ui.available_size(),
                Sense::click()
            );

            // Get the relative position of the painter
            let to_screen = RectTransform::from_to(
                Rect::from_min_size(Pos2::ZERO, response.rect.size()),
                response.rect
            );

            // Calculate the LED sidelength for x and y based on the window size and number of pixels, and choose smallest value for LED sidelength
            let sidelength= [
                response.rect.width() / self.current_matrix_config.width as f32,    // Sidelength from width
                response.rect.height() / self.current_matrix_config.height as f32,  // Sidelength from height
            ].iter().min_by(|a, b| a.partial_cmp(b).unwrap()) // Pick smaller of the two
                .unwrap().clone(); // It's still a &f32, so clone it

            // Setup the LED roundness parameter
            let rounding = if self.display_settings.round_leds {
                Rounding::same(sidelength)
            } else {
                Rounding::none()
            };

            // Draw the LEDs if the plugin update state is consistent with the current matrix config
            if self.last_update.state.len() > 0 && self.last_update.state.len() == self.current_matrix_config.height && self.last_update.state[0].len() == self.current_matrix_config.width {
                for y in 0..self.current_matrix_config.height {
                    for x in 0..self.current_matrix_config.width {
                        // Grab the color of this LED from the last update
                        let led_color = egui::Color32::from_rgba_premultiplied(
                            self.last_update.state[y][x][2],
                            self.last_update.state[y][x][1],
                            self.last_update.state[y][x][0],
                            self.last_update.state[y][x][3]
                        );

                        // Draw the LED
                        painter.rect_filled(
                            Rect::from_min_size(
                                to_screen.transform_pos(Pos2::new(x as f32 * sidelength, y as f32 * sidelength)),
                                Vec2::new(sidelength, sidelength)
                            ),
                            rounding,
                            led_color
                        );
                    }
                }
            }
        });

        // Render the status message at the bottom of the screen
        egui::TopBottomPanel::bottom("status_msg").show(ctx, |ui| {
            ui.label(self.status_msg.clone());
        });
    }
}

impl SimulatorApp {
    fn set_status_msg(&mut self, msg: String) {
        self.status_msg = format!(">> {msg}");
    }
}
