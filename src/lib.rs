use std::sync::Mutex;

use asr::{watcher::Watcher, Process};
use once_cell::sync::Lazy;

// TODO: Sig scan for this for resilency to game updates.
const GAME_ENGINE_OFFSET: u64 = 0x0575_8730;
const GAME_INSTANCE_OFFSET: u64 = 0xD28;

const LOAD_PATH: [u64; 5] = [GAME_ENGINE_OFFSET, GAME_INSTANCE_OFFSET, 0xF0, 0xE0, 0xA0];

// TODO: Mutex shouldn't be necessary as this is all single-threaded (double-check this)
static STATE: Lazy<Mutex<State>> = Lazy::new(|| {
    Mutex::new(State {
        // TODO: do this in update, deal with fallability
        process: Process::attach("CosmicShake-Win64-Shipping.exe")
            .expect("Could not attach to process"),
        is_loading: Watcher::new(),
    })
});

struct State {
    process: Process,
    is_loading: Watcher<u8>,
}

#[no_mangle]
pub extern "C" fn update() {
    let mut state = STATE.lock().unwrap();
    let is_loading = dbg!(state.process.read_pointer_path64(
        state
            .process
            .get_module_address("CosmicShake-Win64-Shipping.exe")
            .unwrap()
            .0,
        &LOAD_PATH,
    ))
    .ok();
    state.is_loading.update(is_loading);

    match is_loading {
        Some(x) if x != 0 => asr::timer::pause_game_time(),
        _ => asr::timer::resume_game_time(),
    };
}
