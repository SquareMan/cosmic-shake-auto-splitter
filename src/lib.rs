use std::sync::Mutex;

use asr::{
    timer::TimerState,
    watcher::{Pair, Watcher},
    Address, Process,
};

// TODO: Sig scan for this for resilency to game updates.
const GAME_ENGINE_OFFSET: u64 = 0x0575_8730;
const GAME_INSTANCE_OFFSET: u64 = 0xD28;
const UNKNOWN_OBJ_OFFSET: u64 = 0xF0;
const GAME_FLOW_MANAGER_OFFSET: u64 = 0xC8;

const TRANSITION_DESCRIPTION_OFFSET: u64 = 0x8B0;

// NOTE: We have to check the list len (0x68) and then dereference the data pointer (0x60) because during normal gameplay
//       it simply sets the len to 0 and keep the stale state around (also it's null on the main menu)
const GAME_FLOW_STATE_PATH: [u64; 6] = [
    GAME_ENGINE_OFFSET,
    GAME_INSTANCE_OFFSET,
    UNKNOWN_OBJ_OFFSET,
    GAME_FLOW_MANAGER_OFFSET,
    0x60,
    0x0,
];
const GAME_FLOW_STATE_LEN_PATH: [u64; 5] = [
    GAME_ENGINE_OFFSET,
    GAME_INSTANCE_OFFSET,
    UNKNOWN_OBJ_OFFSET,
    GAME_FLOW_MANAGER_OFFSET,
    0x68,
];

const TRANSITION_DESCRIPTION_PATH: [u64; 3] =
    [GAME_ENGINE_OFFSET, TRANSITION_DESCRIPTION_OFFSET, 0];

// TODO: Mutex shouldn't be necessary as this is all single-threaded (double-check this)
static STATE: Mutex<State> = Mutex::new(State { game: None });

#[repr(u8)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, bytemuck::CheckedBitPattern)]
pub enum GameFlowState {
    Undefined = 0x0,
    BossBattleState,
    GameplaySequenceState,
    CinematicSequenceState,
    LoadingTransitionState,
    NPCDialogueState,
    QuickTravelTransitionState,
    VideoPlayerState,
    MountState,
    RescueState,
    ChallengeState,
    MinigameState,
    TutorialState,
    SlingshotState,
}

impl GameFlowState {
    fn read(proc: &Process, module: u64) -> Option<Self> {
        let game_state_len = proc
            .read_pointer_path64::<u8>(module, &GAME_FLOW_STATE_LEN_PATH)
            .ok();
        match game_state_len {
            Some(x) if x > 0 => proc
                .read_pointer_path64(module, &GAME_FLOW_STATE_PATH)
                .ok()
                .map(|x| bytemuck::checked::cast::<u8, _>(x)),
            _ => None,
        }
    }
}

struct State {
    game: Option<Game>,
}

struct Game {
    process: Process,
    module: u64,
    game_flow_state: Watcher<GameFlowState>,
    transition_description: Pair<[u8; 66]>,
    use_hack: bool,
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
            game_flow_state: Watcher::new(),
            transition_description: Pair {
                old: [0; 66],
                current: [0; 66],
            },
            use_hack: false,
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

    let game_flow_state = GameFlowState::read(&game.process, game.module);
    game.game_flow_state.update(game_flow_state);

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
    (
        game.transition_description.old,
        game.transition_description.current,
    ) = (game.transition_description.current, buf);

    if asr::timer::state() == TimerState::NotRunning {
        // TODO: Make this not horrible
        if &game.transition_description.old == MENU && &game.transition_description.current == HUB {
            asr::timer::start();
            game.use_hack = true;
        }
    }

    match game_flow_state {
        Some(GameFlowState::QuickTravelTransitionState | GameFlowState::LoadingTransitionState) => {
            asr::timer::pause_game_time()
        }
        // Nasty hack here to pause the timer on new game. The game flow state list is empty in this case so we need a workaround
        // Idea here is that when the load finished we will immediately be in the `CinematicSequenceState` and can simply keep the timer paused while the list remains empty
        None if game.use_hack == true => asr::timer::pause_game_time(),
        _ => {
            asr::timer::resume_game_time();
            game.use_hack = false;
        }
    };
}

// Temporary nasty hack, need proper widestring support
const MENU: &'static [u8; 66] = b"/\0G\0a\0m\0e\0/\0C\0S\0/\0M\0a\0p\0s\0/\0M\0a\0i\0n\0M\0e\0n\0u\0/\0M\0a\0i\0n\0M\0e\0n\0u\0_\0P\0";
const HUB: &'static [u8; 66] = b"/\0G\0a\0m\0e\0/\0C\0S\0/\0M\0a\0p\0s\0/\0B\0i\0k\0i\0n\0i\0B\0o\0t\0t\0o\0m\0/\0B\0B\0_\0P\0\0\0P\0";
