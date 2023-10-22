use extism::Plugin;
use std::ops::DerefMut;
use std::str::from_utf8;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub(crate) fn plugin_thread(
    mut plugin: Plugin,
    fps: f32,
    frame_mutex: Arc<Mutex<Vec<Vec<[u8; 4]>>>>,
    generate_frame_flag: Arc<Mutex<bool>>,
    autoplay_flag: Arc<Mutex<bool>>,
    freeze_flag: Arc<Mutex<bool>>,
    kill_flag: Arc<Mutex<bool>>,
) {
    // Setup frame timing variables
    let mut time_at_last_frame = Instant::now();
    let time_between_frames = Duration::from_secs_f32(1.0 / fps);

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

    'update_loop: loop {
        // Kill the thread if requested
        {
            if *kill_flag.lock().unwrap() {
                log::info!("Received kill signal.");
                break 'update_loop;
            }
        }

        if (
                // Is autoplay on, and has enough time passes for the given FPS?
            *autoplay_flag.lock().unwrap()
            && (Instant::now().duration_since(time_at_last_frame) >= time_between_frames))
                // Or, does the simulator want us to generate a new frame?
            || *generate_frame_flag.lock().unwrap()
        {
            // Reset the frame generate flag
            if *generate_frame_flag.lock().unwrap() {
                *generate_frame_flag.lock().unwrap() = false;
            }

            // Attempt to pull the next frame from the plugin, as a UTF8 JSON string
            let new_state_utf8 = match plugin.call("update", "") {
                Ok(utf8) => utf8,
                Err(e) => {
                    log::error!("Failed to receive update from plugin.");
                    log::debug!(
                        "Received the following error while polling for update from plugin: {e}"
                    );
                    break 'update_loop;
                }
            };

            // Convert the UTF8 to a string
            let new_state_str = match from_utf8(new_state_utf8) {
                Ok(str) => str,
                Err(e) => {
                    log::error!("Failed to convert update from UTF8.");
                    log::debug!("Received the following error while converting from UTF8: {e}");
                    break 'update_loop;
                }
            };

            // Deserialize the new state from a string
            let new_state: Option<Vec<Vec<[u8; 4]>>> =
                match serde_json::from_str::<Option<Vec<Vec<[u8; 4]>>>>(new_state_str) {
                    Ok(update) => update,
                    Err(_) => {
                        log::error!("Invalid update returned from plugin.");
                        break 'update_loop;
                    }
                };

            // If the plugin signalled that it is done, exit this thread
            let new_state: Vec<Vec<[u8; 4]>> = match new_state {
                Some(new_state) => new_state,
                None => {
                    log::info!("Plugin has stopped providing updates.");
                    break 'update_loop;
                }
            };

            // Replace the previous frame with the new frame
            let mut frame = frame_mutex.lock().unwrap();
            let frame = frame.deref_mut();
            *frame = new_state;

            // Mark the time
            time_at_last_frame = Instant::now();
        }
    }

    log::info!("Freezing simulator.");
    *freeze_flag.lock().unwrap() = true;
}
