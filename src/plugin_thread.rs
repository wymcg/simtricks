use extism::Plugin;
use matricks_plugin::{MatrixConfiguration, PluginUpdate};
use serde_json::from_str;
use std::path::PathBuf;
use std::str::from_utf8;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use std::{fs, thread};

pub struct PluginThread {
    pub join_handle: JoinHandle<()>,
    pub channels: PluginThreadChannels,
    pub path: PathBuf,
}

pub struct PluginThreadChannels {
    pub update_rx: Receiver<PluginUpdate>,
}

pub fn start_plugin_thread(path: PathBuf, mat_config: MatrixConfiguration) -> PluginThread {
    let (update_tx, update_rx) = channel::<PluginUpdate>();

    PluginThread {
        path: path.clone(),
        join_handle: thread::spawn(|| plugin_thread(path, mat_config, update_tx)),
        channels: PluginThreadChannels {
            update_rx,
        },
    }
}

fn plugin_thread(
    path: PathBuf,
    mat_config: MatrixConfiguration,
    update_tx: Sender<PluginUpdate>,
) {

    // Calculate ms per frame
    let target_frame_time_ms =
        Duration::from_nanos((1_000_000_000.0 / mat_config.target_fps).round() as u64);

    // Prepare the matrix config string
    let mat_config_string =
        serde_json::to_string(&mat_config).expect("Unable to make matrix config string");

    // Get the plugin data
    let wasm = fs::read(path).expect("Unable to load plugin data");

    // Make new context for plugin
    let context = extism::Context::new();

    // Make a new instance of the plugin
    let mut plugin = Plugin::new(&context, wasm, [], true).expect("Unable to instantiate plugin");

    let _setup_result = plugin
        .call("setup", &mat_config_string)
        .expect("Unable to call setup!");

    let mut last_frame_time = Instant::now();

    'update_loop: loop {
        // Generate a frame if the target frame time has passed
        if (Instant::now() - last_frame_time) >= target_frame_time_ms {
            // Reset the last frame time
            last_frame_time = Instant::now();

            // Generate a frame
            let update_utf8 = plugin
                .call("update", "")
                .expect("Unable to call update function");
            let update_str = from_utf8(update_utf8).expect("Unable to convert to str from utf8!");
            let update =
                from_str::<PluginUpdate>(update_str).expect("Unable to deserialize update!");

            // Check if we should stop after this update
            let should_halt = update.done;

            // Send the update to the GUI
            match update_tx.send(update) {
                Ok(_) => {/* Update sent without issue, no further action required */}
                Err(_) => {
                    /* Assume the main thread has started a new plugin thread and stop this thread */
                    break 'update_loop;
                }
            }

            // If the plugin requested to stop, then stop
            if should_halt {
                break 'update_loop;
            }
        }
    }
}