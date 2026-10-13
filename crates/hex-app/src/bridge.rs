//! wasm-bindgen exports behind window.hexWorld. Shared state is written by Bevy systems.
use std::sync::Mutex;

pub static READY: Mutex<bool> = Mutex::new(false);
pub static STATE_JSON: Mutex<String> = Mutex::new(String::new());
pub static DIAG_JSON: Mutex<String> = Mutex::new(String::new());

#[cfg(target_arch = "wasm32")]
mod exports {
    use wasm_bindgen::prelude::*;

    use crate::{capture, control};

    #[wasm_bindgen]
    pub fn hex_world_ready() -> bool {
        *super::READY.lock().unwrap()
    }

    #[wasm_bindgen]
    pub fn hex_world_state_json() -> String {
        let s = super::STATE_JSON.lock().unwrap();
        if s.is_empty() { "{}".into() } else { s.clone() }
    }

    #[wasm_bindgen]
    pub fn hex_world_diag_json() -> String {
        let s = super::DIAG_JSON.lock().unwrap();
        if s.is_empty() { "{}".into() } else { s.clone() }
    }

    /// Enqueues one command object or an array of them. Returns false and logs on a parse error.
    #[wasm_bindgen]
    pub fn hex_world_cmd(json: &str) -> bool {
        let parsed = if json.trim_start().starts_with('{') {
            control::parse_script(&format!("[{json}]"))
        } else {
            control::parse_script(json)
        };
        match parsed {
            Ok(cmds) => {
                control::INBOX.lock().unwrap().extend(cmds);
                true
            }
            Err(e) => {
                web_sys::console::error_1(&format!("hexWorld.cmd: {e}").into());
                false
            }
        }
    }

    /// Queues a screenshot behind any pending commands; poll `hex_world_take_shot` for the result.
    #[wasm_bindgen]
    pub fn hex_world_screenshot(name: String) {
        capture::SHOTS.lock().unwrap().retain(|(n, _)| *n != name);
        control::INBOX
            .lock()
            .unwrap()
            .push(control::Command::Screenshot { name });
    }

    /// Removes and returns the PNG data URL for `name` once the capture has landed.
    #[wasm_bindgen]
    pub fn hex_world_take_shot(name: String) -> Option<String> {
        let mut shots = capture::SHOTS.lock().unwrap();
        let i = shots.iter().position(|(n, _)| *n == name)?;
        Some(shots.remove(i).1)
    }
}
