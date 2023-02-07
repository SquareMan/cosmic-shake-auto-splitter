mod ffi;

use asr::{timer::TimerState, watcher::Watcher, Process};
use widestring::{u16cstr, U16CStr};

const EXE: &str = "CosmicShake-Win64-Shipping.exe";

const MENU: &U16CStr = u16cstr!("/Game/CS/Maps/MainMenu/MainMenu_P");
const HUB: &U16CStr = u16cstr!("/Game/CS/Maps/BikiniBottom/BB_P");
const OVERWORLD: &U16CStr = u16cstr!("/Game/CS/Maps/StreamingOverworld/Overworld_P");

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

const WORLD_CONTEXT_OFFSET: u64 = 0x30;
const CURRENT_WORLD_OFFSET: u64 = 0x280;
const STREAMING_LEVELS_OFFSET: u64 = 0x88;
const STREAMING_LEVELS_LEN_OFFSET: u64 = 0x90;
const PERSISTENT_LEVEL_OFFSET: u64 = 0x128;
const ACTORS_OFFSET: u64 = 0x98;
const HEALTH_COMPONENT_OFFSET: u64 = 0x508;
const CURRENT_HEALTH_OFFSET: u64 = 0x264;

const CURRENT_WORLD_PATH: [u64; 4] = [
    GAME_ENGINE_OFFSET,
    GAME_INSTANCE_OFFSET,
    WORLD_CONTEXT_OFFSET,
    CURRENT_WORLD_OFFSET,
];

const NUM_STREAMING_LEVELS_BEING_LOADED_PATH: [u64; 5] = [
    GAME_ENGINE_OFFSET,
    GAME_INSTANCE_OFFSET,
    WORLD_CONTEXT_OFFSET,
    CURRENT_WORLD_OFFSET,
    0x5EA,
];
// First bit
const BEGUN_PLAY_PATH: [u64; 5] = [
    GAME_ENGINE_OFFSET,
    GAME_INSTANCE_OFFSET,
    WORLD_CONTEXT_OFFSET,
    CURRENT_WORLD_OFFSET,
    0x10D,
];

// TODO: consider avoiding hardcoding these indexes by searching for the correct object within arrays
const SQUIDWARD_BOSS_HEALTH_PATH: [u64; 11] = [
    GAME_ENGINE_OFFSET,
    GAME_INSTANCE_OFFSET,
    WORLD_CONTEXT_OFFSET,
    CURRENT_WORLD_OFFSET,
    STREAMING_LEVELS_OFFSET,
    0x8, // 2nd element of array
    PERSISTENT_LEVEL_OFFSET,
    ACTORS_OFFSET,
    0x3F0, // 0x7E'th element of array
    HEALTH_COMPONENT_OFFSET,
    CURRENT_HEALTH_OFFSET,
];
const STREAMING_LEVELS_LEN_PATH: [u64; 5] = [
    GAME_ENGINE_OFFSET,
    GAME_INSTANCE_OFFSET,
    WORLD_CONTEXT_OFFSET,
    CURRENT_WORLD_OFFSET,
    STREAMING_LEVELS_LEN_OFFSET,
];

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

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Transition {
    Menu,
    Hub,
    Overworld,
}

impl GameFlowState {
    fn read(proc: &Process, module: u64) -> Option<Self> {
        let last_game_state = proc
            .read_pointer_path64(module, &GAME_FLOW_STATE_PATH)
            .ok()
            .map(bytemuck::checked::cast::<u8, _>)?;
        let game_state_len = proc
            .read_pointer_path64::<u8>(module, &GAME_FLOW_STATE_LEN_PATH)
            .ok()?;
        if game_state_len > 0 {
            Some(last_game_state)
        } else {
            // Treat no active states as undefined, this avoids updating our watcher with None and potentially missing a transition
            Some(GameFlowState::Undefined)
        }
    }
}

fn read_transition(proc: &Process, module: u64) -> Option<Transition> {
    let mut buf = proc
        .read_pointer_path64::<[u16; 34]>(module, &TRANSITION_DESCRIPTION_PATH)
        .ok()?;

    // We force the last byte in the buffer to null after reading these strings, so we know we can unwrap here
    *buf.last_mut().unwrap() = 0;
    let cstr = U16CStr::from_slice_truncate(&buf).unwrap();

    // can't match on U16CStr
    if cstr == MENU {
        Some(Transition::Menu)
    } else if cstr == HUB {
        Some(Transition::Hub)
    } else if cstr == OVERWORLD {
        Some(Transition::Overworld)
    } else {
        None
    }
}

fn read_boss_health(proc: &Process, module: u64) -> Option<u32> {
    proc.read_pointer_path64(module, &SQUIDWARD_BOSS_HEALTH_PATH)
        .ok()
}

fn read_begun_play(proc: &Process, module: u64) -> Option<bool> {
    let bitfield = proc
        .read_pointer_path64::<u8>(module, &BEGUN_PLAY_PATH)
        .ok()?;
    Some(bitfield & 0b1 == 1)
}

struct Game {
    process: Process,
    module: u64,
    game_flow_state: Watcher<GameFlowState>,
    transition_description: Watcher<Transition>,
    squidward_boss_health: Watcher<u32>,
    ready_to_end: bool,
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
            squidward_boss_health: Watcher::new(),
            ready_to_end: false,
        })
    }

    fn is_loading(&self) -> bool {
        let Ok(world_ptr) = self
            .process
            .read_pointer_path64::<usize>(self.module, &CURRENT_WORLD_PATH) else
        {
            return false;
        };

        let num_streaming_levels_loading = self
            .process
            .read_pointer_path64::<u16>(self.module, &NUM_STREAMING_LEVELS_BEING_LOADED_PATH)
            .unwrap_or(0);

        world_ptr == 0
            || !read_begun_play(&self.process, self.module).unwrap_or(false)
            || num_streaming_levels_loading > 0
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
            if transition.old == Transition::Menu && transition.current == Transition::Hub {
                if self.settings.reset {
                    asr::timer::reset();
                }
                if asr::timer::state() == TimerState::NotRunning {
                    asr::timer::start();
                    game.ready_to_end = false;
                }
            }
        }

        let game_flow_state = game
            .game_flow_state
            .update(GameFlowState::read(&game.process, game.module))
            .copied();

        let begun_play = read_begun_play(&game.process, game.module).unwrap_or(true);

        if !begun_play {
            asr::timer::pause_game_time();
        } else if matches!(transition.map(|x| x.current), Some(Transition::Menu)) {
            if game.is_loading() {
                asr::timer::pause_game_time();
            } else {
                asr::timer::resume_game_time();
            }
        } else {
            match game_flow_state.map(|x| x.current) {
                Some(
                    GameFlowState::QuickTravelTransitionState
                    | GameFlowState::LoadingTransitionState,
                ) => asr::timer::pause_game_time(),
                Some(GameFlowState::RescueState) if game.is_loading() => {
                    asr::timer::pause_game_time()
                }
                None if game.is_loading() => asr::timer::pause_game_time(),
                _ => {
                    asr::timer::resume_game_time();
                }
            }
        }

        // Sanity check that we're in the final boss
        // TODO: update this once we're able to tell the name of the active level
        if game
            .process
            .read_pointer_path64::<u32>(game.module, &STREAMING_LEVELS_LEN_PATH)
            .unwrap_or(0)
            == 5
        {
            let boss_health = read_boss_health(&game.process, game.module)
                .map(|x| game.squidward_boss_health.update_infallible(x));
            if let Some(boss_health) = boss_health {
                if boss_health.changed_to(&0) {
                    game.ready_to_end = true;
                }
            }
        }

        // After final boss is defeated, split at the start of the next load
        if game.ready_to_end
            && game_flow_state
                .map(|x| x.current == GameFlowState::LoadingTransitionState)
                .unwrap_or(false)
        {
            game.ready_to_end = false;
            asr::timer::split();
        }
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
