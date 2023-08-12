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
    pub log_rx: Receiver<String>,
    pub is_done_rx: Receiver<()>,
}

pub fn start_plugin_thread(path: PathBuf, mat_config: MatrixConfiguration) -> PluginThread {
    let (update_tx, update_rx) = channel::<PluginUpdate>();
    let (log_tx, log_rx) = channel::<String>();
    let (is_done_tx, is_done_rx) = channel::<()>();

    PluginThread {
        path: path.clone(),
        join_handle: thread::spawn(|| plugin_thread(path, mat_config, update_tx, log_tx, is_done_tx)),
        channels: PluginThreadChannels { update_rx, log_rx, is_done_rx },
    }
}

fn plugin_thread(
    path: PathBuf,
    mat_config: MatrixConfiguration,
    update_tx: Sender<PluginUpdate>,
    log_tx: Sender<String>,
    is_done_tx: Sender<()>,
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

    match plugin.call("setup", &mat_config_string) {
        Ok(_) => { /* Setup call ran without issue, no further action needed */ }
        Err(_) => {
            log_tx
                .send("Unable to run setup function!".to_string())
                .expect("Could not send log to main thread!");
        }
    };

    let mut last_frame_time = Instant::now();

    'update_loop: loop {
        // Generate a frame if the target frame time has passed
        if (Instant::now() - last_frame_time) >= target_frame_time_ms {
            // Reset the last frame time
            last_frame_time = Instant::now();

            // Get an update from the plugin
            let update_utf8 = match plugin.call("update", "") {
                Ok(utf8) => utf8,
                Err(_) => {
                    log_tx.send("Unable to call update function! No further updates will be requested from this thread.".to_string()).expect("Could not send log to main thread!");
                    is_done_tx.send(()).expect("Unable send done signal to main thread!");
                    break 'update_loop;
                }
            };

            // Convert UTF8 update to a string
            let update_str = match from_utf8(update_utf8) {
                Ok(str) => str,
                Err(_) => {
                    log_tx.send("Unable to convert UTF-8 response from the plugin! No further updates will be requested from this thread.".to_string()).expect("Could not send log to main thread!");
                    is_done_tx.send(()).expect("Unable send done signal to main thread!");
                    break 'update_loop;
                }
            };

            // Convert string update to the plugin update struct
            let update = match from_str::<PluginUpdate>(update_str) {
                Ok(plugin_update) => plugin_update,
                Err(_) => {
                    log_tx.send("Malformed plugin update! No further updates will be requested from this thread.".to_string()).expect("Could not send log to main thread!");
                    is_done_tx.send(()).expect("Unable send done signal to main thread!");
                    break 'update_loop;
                }
            };

            // Check if we should stop after this update
            let should_halt = update.done;

            // Send the update to the GUI
            match update_tx.send(update) {
                Ok(_) => { /* Update sent without issue, no further action required */ }
                Err(_) => {
                    /* Assume the main thread has started a new plugin thread and stop this thread */
                    break 'update_loop;
                }
            }

            // If the plugin requested to stop, then stop
            if should_halt {
                is_done_tx.send(()).expect("Unable send done signal to main thread!");
                break 'update_loop;
            }
        }
    }
}
