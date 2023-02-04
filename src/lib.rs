mod ffi;

use asr::{time::Duration, timer::TimerState, watcher::Watcher, Process};

const EXE: &str = "CosmicShake-Win64-Shipping.exe";

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
                .map(|x| bytemuck::checked::cast::<u8, _>(x)),
            _ => None,
        }
    }
}

fn read_transition(proc: &Process, module: u64) -> Option<[u8; 66]> {
    proc.read_pointer_path64(module, &TRANSITION_DESCRIPTION_PATH)
        .ok()
}

struct Game {
    process: Process,
    module: u64,
    game_flow_state: Watcher<GameFlowState>,
    transition_description: Watcher<[u8; 66]>,
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
            // TODO: Make this not horrible
            // Temporary nasty hack, need proper widestring support
            const MENU: &'static [u8; 66] = b"/\0G\0a\0m\0e\0/\0C\0S\0/\0M\0a\0p\0s\0/\0M\0a\0i\0n\0M\0e\0n\0u\0/\0M\0a\0i\0n\0M\0e\0n\0u\0_\0P\0";
            const HUB: &'static [u8; 66] = b"/\0G\0a\0m\0e\0/\0C\0S\0/\0M\0a\0p\0s\0/\0B\0i\0k\0i\0n\0i\0B\0o\0t\0t\0o\0m\0/\0B\0B\0_\0P\0\0\0P\0";
            if &transition.old == MENU && &transition.current == HUB {
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
            None if game.use_hack == true => asr::timer::pause_game_time(),
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
