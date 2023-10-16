use extism::{CurrentPlugin, Function, Plugin, UserData, Val, ValType};
use serde_json::from_str;
use std::path::PathBuf;
use std::str::from_utf8;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use std::{fs, thread};
use std::collections::BTreeMap;
use crate::matrix_config::MatrixConfiguration;

pub struct PluginThread {
    pub join_handle: JoinHandle<()>,
    pub channels: PluginThreadChannels,
    pub path: PathBuf,
}

pub struct PluginThreadChannels {
    pub update_rx: Receiver<Vec<Vec<[u8; 4]>>>,
    pub log_rx: Receiver<String>,
    pub is_done_rx: Receiver<()>,
}

pub fn start_plugin_thread(path: PathBuf, mat_config: MatrixConfiguration, allowed_hosts: Vec<String>, path_mappings: Vec<(PathBuf, PathBuf)>) -> PluginThread {
    let (update_tx, update_rx) = channel::<Vec<Vec<[u8; 4]>>>();
    let (log_tx, log_rx) = channel::<String>();
    let (is_done_tx, is_done_rx) = channel::<()>();

    PluginThread {
        path: path.clone(),
        join_handle: thread::spawn(|| plugin_thread(path, mat_config, allowed_hosts, path_mappings, update_tx, log_tx, is_done_tx)),
        channels: PluginThreadChannels { update_rx, log_rx, is_done_rx },
    }
}

fn plugin_thread(
    path: PathBuf,
    mat_config: MatrixConfiguration,
    allowed_hosts: Vec<String>,
    path_mappings: Vec<(PathBuf, PathBuf)>,
    update_tx: Sender<Vec<Vec<[u8; 4]>>>,
    log_tx: Sender<String>,
    is_done_tx: Sender<()>,
) {
    // Calculate ms per frame
    let target_frame_time_ms =
        Duration::from_nanos((1_000_000_000.0 / mat_config.target_fps).round() as u64);

    // Create the config
    let mut matricks_config: BTreeMap<String, Option<String>> = BTreeMap::new();
    matricks_config.insert(
        String::from("width"),
        Some(format!("{}", mat_config.width)),
    );
    matricks_config.insert(
        String::from("height"),
        Some(format!("{}", mat_config.height)),
    );
    matricks_config.insert(
        String::from("target_fps"),
        Some(format!("{}", mat_config.target_fps)),
    );
    matricks_config.insert(
        String::from("serpentine"),
        Some(format!("{}", false)),
    );
    matricks_config.insert(
        String::from("brightness"),
        Some(format!("{}", 255u8)),
    );

    let send_log_fn = |
        _plugin: &mut CurrentPlugin, _inputs: &[Val], _outputs: &mut [Val], _user_data: UserData
    | -> Result<(), extism::Error> {
        // Not implemented!
        Ok(())
    };

    // Setup the host functions
    let plugin_debug_log_function = Function::new(
        "matricks_debug",
        [ValType::I64],
        [],
        None,
        send_log_fn.clone(),
    );
    let plugin_info_log_function = Function::new(
        "matricks_info",
        [ValType::I64],
        [],
        None,
        send_log_fn.clone(),
    );
    let plugin_warn_log_function = Function::new(
        "matricks_warn",
        [ValType::I64],
        [],
        None,
        send_log_fn.clone(),
    );
    let plugin_error_log_function = Function::new(
        "matricks_error",
        [ValType::I64],
        [],
        None,
        send_log_fn,
    );
    let plugin_functions = [
        plugin_debug_log_function,
        plugin_info_log_function,
        plugin_warn_log_function,
        plugin_error_log_function,
    ];

    // Get the plugin data
    let wasm = fs::read(path).expect("Unable to load plugin data");

    // Make new context for plugin
    let context = extism::Context::new();

    // Make a new manifest for the plugin
    let manifest = extism::Manifest::new([extism::manifest::Wasm::data(wasm)])
        .with_allowed_hosts(allowed_hosts.into_iter())
        .with_allowed_paths(path_mappings.into_iter());

    // Make a new instance of the plugin
    let plugin = Plugin::new_with_manifest(&context, &manifest, plugin_functions, true).expect("Unable to instantiate plugin!");
    let mut plugin = plugin.with_config(&matricks_config).expect("Unable to apply config to plugin!");

    match plugin.call("setup", "") {
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
            let update = match from_str::<Option<Vec<Vec<[u8; 4]>>>>(update_str) {
                Ok(plugin_update) => plugin_update,
                Err(_) => {
                    log_tx.send("Malformed plugin update! No further updates will be requested from this thread.".to_string()).expect("Could not send log to main thread!");
                    is_done_tx.send(()).expect("Unable send done signal to main thread!");
                    break 'update_loop;
                }
            };

            // Decide what to do with this update
            match update {
                None => {
                    // If the update is None (JSON 'null'), this plugin is done
                    is_done_tx.send(()).expect("Unable to send done signal to main thread!");
                    break 'update_loop;
                }
                Some(update) => {
                    // Send the update to the GUI
                    match update_tx.send(update) {
                        Ok(_) => { /* Update sent without issue, no further action required */ }
                        Err(_) => {
                            /* Assume the main thread has started a new plugin thread and stop this thread */
                            break 'update_loop;
                        }
                    }
                }
            }
        }
    }
}
