use std::sync::Mutex;

use asr::{
    timer::TimerState,
    watcher::{Pair, Watcher},
    Address, Process,
};
use once_cell::sync::Lazy;

// TODO: Sig scan for this for resilency to game updates.
const GAME_ENGINE_OFFSET: u64 = 0x0575_8730;
const GAME_INSTANCE_OFFSET: u64 = 0xD28;

const TRANSITION_DESCRIPTION_OFFSET: u64 = 0x8B0;

const LOAD_PATH: [u64; 5] = [GAME_ENGINE_OFFSET, GAME_INSTANCE_OFFSET, 0xF0, 0xE0, 0xA0];
const TRANSITION_DESCRIPTION_PATH: [u64; 3] =
    [GAME_ENGINE_OFFSET, TRANSITION_DESCRIPTION_OFFSET, 0];

// TODO: Mutex shouldn't be necessary as this is all single-threaded (double-check this)
static STATE: Lazy<Mutex<State>> = Lazy::new(|| Mutex::new(State { game: None }));

struct State {
    game: Option<Game>,
}

struct Game {
    process: Process,
    module: u64,
    is_loading: Watcher<u8>,
    transition_description: Pair<[u8; 66]>,
}

impl Game {
    fn attach() -> Option<Self> {
        let process = Process::attach("CosmicShake-Win64-Shipping.exe")?;
        let module = process
            .get_module_address("CosmicShake-Win64-Shipping.exe")
            .ok()?
            .0;
        Some(Self {
            process,
            module,
            is_loading: Watcher::new(),
            transition_description: Pair {
                old: [0; 66],
                current: [0; 66],
            },
        })
    }
}

#[no_mangle]
pub extern "C" fn update() {
    let mut state = STATE.lock().unwrap();
    // TODO: Rehooking after game restart
    let Some(game) = &mut state.game else {
        state.game = Game::attach();
        return;
    };

    let is_loading = game
        .process
        .read_pointer_path64(game.module, &LOAD_PATH)
        .ok();
    game.is_loading.update(is_loading);

    let mut buf = [0u8; 66];
    // Need effective address of transition description to read into a buf
    let addr: u64 = game
        .process
        .read_pointer_path64(
            game.module,
            &TRANSITION_DESCRIPTION_PATH[0..TRANSITION_DESCRIPTION_PATH.len() - 1],
        )
        .unwrap();
    game.process.read_into_buf(Address(addr), &mut buf).unwrap();
    asr::print_message(format!("{buf:?}").as_str());
    (
        game.transition_description.old,
        game.transition_description.current,
    ) = (game.transition_description.current, buf);

    if asr::timer::state() == TimerState::NotRunning {
        // TODO: Make this not horrible
        if &game.transition_description.old == MENU && &game.transition_description.current == HUB {
            asr::timer::start();
        }
    }

    match is_loading {
        Some(x) if x != 0 => asr::timer::pause_game_time(),
        _ => asr::timer::resume_game_time(),
    };
}

// Temporary nasty hack, need proper widestring support
const MENU: &'static [u8; 66] = b"/\0G\0a\0m\0e\0/\0C\0S\0/\0M\0a\0p\0s\0/\0M\0a\0i\0n\0M\0e\0n\0u\0/\0M\0a\0i\0n\0M\0e\0n\0u\0_\0P\0";
const HUB: &'static [u8; 66] = b"/\0G\0a\0m\0e\0/\0C\0S\0/\0M\0a\0p\0s\0/\0B\0i\0k\0i\0n\0i\0B\0o\0t\0t\0o\0m\0/\0B\0B\0_\0P\0\0\0P\0";
