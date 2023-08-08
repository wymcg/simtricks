use matricks_plugin::{MatrixConfiguration, PluginUpdate};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::{fs, thread};
use std::str::from_utf8;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use extism::Plugin;
use serde_json::from_str;

pub struct PluginThread {
    pub join_handle: JoinHandle<()>,
    pub channels: PluginThreadChannels,
}

pub struct PluginThreadChannels {
    pub update_rx: Receiver<PluginUpdate>,
    pub plugin_info_tx: Sender<(PathBuf, MatrixConfiguration)>,
    pub next_plugin_tx: Sender<()>,
}

pub fn start_plugin_thread() -> PluginThread {
    let (update_tx, update_rx) = channel::<PluginUpdate>();
    let (plugin_info_tx, plugin_info_rx) = channel::<(PathBuf, MatrixConfiguration)>();
    let (next_plugin_tx, next_plugin_rx) = channel::<()>();

    PluginThread {
        join_handle: thread::spawn(|| plugin_thread(update_tx, plugin_info_rx, next_plugin_rx)),
        channels: PluginThreadChannels {
            update_rx,
            plugin_info_tx,
            next_plugin_tx,
        },
    }
}

fn plugin_thread(
    update_tx: Sender<PluginUpdate>,
    plugin_info_rx: Receiver<(PathBuf, MatrixConfiguration)>,
    next_plugin_rx: Receiver<()>,
) {
    // Wait for the user to choose a file
    for _ in &next_plugin_rx { break; }

    for (path, mat_config) in &plugin_info_rx {
        // Calculate ms per frame
        let target_frame_time_ms = Duration::from_nanos((1_000_000_000.0 / mat_config.target_fps).round() as u64);

        // Prepare the matrix config string
        let mat_config_string = serde_json::to_string(&mat_config).expect("Unable to make matrix config string");

        // Get the plugin data
        let wasm = fs::read(path).expect("Unable to load plugin data");

        // Make new context for plugin
        let context = extism::Context::new();

        // Make a new instance of the plugin
        let mut plugin = Plugin::new(&context, wasm, [], true).expect("Unable to instantiate plugin");

        let _setup_result = plugin.call("setup", &mat_config_string).expect("Unable to call setup!");

        let mut last_frame_time = Instant::now();

        'update_loop: loop {
            // Generate a frame if the target frame time has passed
            if (Instant::now() - last_frame_time) >= target_frame_time_ms {
                // Reset the last frame time
                last_frame_time = Instant::now();

                // Generate a frame
                let update_utf8 = plugin.call("update", "").expect("Unable to call update function");
                let update_str = from_utf8(update_utf8).expect("Unable to convert to str from utf8!");
                let update = from_str::<PluginUpdate>(update_str).expect("Unable to deserialize update!");

                // Send the update to the GUI
                update_tx.send(update.clone()).expect("Unable to send update!");

                // Go to the next plugin if the plugin indicates it is done providing updates
                if update.done { break 'update_loop; }

                // Go to the next plugin if requested by the GUI
                match next_plugin_rx.try_recv() {
                    Ok(()) => {break 'update_loop}
                    Err(_) => {/* do nothing */}
                }
            }
        }
    }
}
