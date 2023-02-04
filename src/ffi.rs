use std::sync::Mutex;

use crate::State;

// TODO: Mutex shouldn't be necessary as this is all single-threaded (double-check this)
static STATE: Mutex<State> = Mutex::new(State::new());

/// LiveSplit Auto Splitting Runtime expects a function named `update` to be exported in the wasm module.
#[no_mangle]
pub extern "C" fn update() {
    let mut state = STATE.lock().unwrap();
    state.update();
}
