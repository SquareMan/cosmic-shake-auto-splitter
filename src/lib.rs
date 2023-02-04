mod ffi;

use asr::{timer::TimerState, watcher::Watcher, Process};
use widestring::{u16cstr, U16CStr};

const EXE: &str = "CosmicShake-Win64-Shipping.exe";

const MENU: &U16CStr = u16cstr!("/Game/CS/Maps/MainMenu/MainMenu_P");
const HUB: &U16CStr = u16cstr!("/Game/CS/Maps/BikiniBottom/BB_P");

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
                .map(bytemuck::checked::cast::<u8, _>),
            _ => None,
        }
    }
}

fn read_transition(proc: &Process, module: u64) -> Option<[u16; 34]> {
    let mut buf = proc
        .read_pointer_path64::<[u16; 34]>(module, &TRANSITION_DESCRIPTION_PATH)
        .ok()?;

    *buf.last_mut().unwrap() = 0;
    Some(buf)
}

struct Game {
    process: Process,
    module: u64,
    game_flow_state: Watcher<GameFlowState>,
    transition_description: Watcher<[u16; 34]>,
    use_hack: bool,
}

impl Game {
    fn attach() -> Option<Self> {
        let process = Process::attach(EXE)?;
        let module = process.get_module_address(EXE).ok()?.0;
        Some(Self {
            process,
            module,
            game_flow_state: Watcher::new(),
            transition_description: Watcher::new(),
            use_hack: false,
        })
    }
}

#[derive(Debug, Default)]
struct Settings {
    reset: bool,
}

impl Settings {
    const fn new() -> Self {
        Self { reset: true }
    }

    // NOTE: This adds duplicate settings (at least on asr-debugger) revisit this when LiveSplit actually supports adding settings
    // fn update(&mut self) {
    //     // Note: this is unfortunately not implemented in LiveSplit's UI yet, but I think it's a sane default
    //     self.reset = asr::user_settings::add_bool(
    //         "Reset",
    //         "Automatically reset the timer when starting a New Game",
    //         true,
    //     );
    // }
}

struct State {
    game: Option<Game>,
    settings: Settings,
}

impl State {
    const fn new() -> Self {
        Self {
            game: None,
            settings: Settings::new(),
        }
    }

    fn update(&mut self) {
        // TODO: Limit how often this is called. We could lower the splitter update rate while unhooked
        self.ensure_hooked();

        let Some(game) = self.game.as_mut() else {
            return;
        };

        let transition = read_transition(&game.process, game.module)
            .map(|x| game.transition_description.update_infallible(x));
        if let Some(transition) = transition {
            // We force the last byte in the buffer to null after reading these strings, so we know we can unwrap here
            if U16CStr::from_slice_truncate(&transition.old).unwrap() == MENU
                && U16CStr::from_slice_truncate(&transition.current).unwrap() == HUB
            {
                if self.settings.reset {
                    asr::timer::reset();
                }
                if asr::timer::state() == TimerState::NotRunning {
                    asr::timer::start();
                    game.use_hack = true;
                }
            }
        }

        let game_flow_state = game
            .game_flow_state
            .update(GameFlowState::read(&game.process, game.module));

        match game_flow_state.map(|x| x.current) {
            Some(
                GameFlowState::QuickTravelTransitionState | GameFlowState::LoadingTransitionState,
            ) => asr::timer::pause_game_time(),
            // Nasty hack here to pause the timer on new game. The game flow state list is empty in this case so we need a workaround
            // Idea here is that when the load finished we will immediately be in the `CinematicSequenceState` and can simply keep the timer paused while the list remains empty
            None if game.use_hack => asr::timer::pause_game_time(),
            _ => {
                asr::timer::resume_game_time();
                game.use_hack = false;
            }
        };
    }

    fn ensure_hooked(&mut self) {
        if self
            .game
            .as_ref()
            .map(|x| x.process.is_open())
            .unwrap_or(false)
        {
            // We are already hooked
            return;
        }

        self.game = Game::attach();
    }
}
