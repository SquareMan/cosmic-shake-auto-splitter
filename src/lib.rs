mod ffi;
mod paths;

use asr::{timer::TimerState, watcher::Watcher, Process};
use paths::Paths;
use widestring::{u16cstr, U16CStr};

const EXE: &str = "CosmicShake-Win64-Shipping.exe";

const MENU: &U16CStr = u16cstr!("/Game/CS/Maps/MainMenu/MainMenu_P");
const HUB: &U16CStr = u16cstr!("/Game/CS/Maps/BikiniBottom/BB_P");
const OVERWORLD: &U16CStr = u16cstr!("/Game/CS/Maps/StreamingOverworld/Overworld_P");

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
    fn read(paths: &Paths, proc: &Process, module: u64) -> Option<Self> {
        let last_game_state = paths
            .game_flow_state
            .read(proc, module)
            .map(bytemuck::checked::cast::<u8, _>)?;
        let game_state_len = paths.game_flow_state_len.read(proc, module)?;
        if game_state_len > 0 {
            Some(last_game_state)
        } else {
            // Treat no active states as undefined, this avoids updating our watcher with None and potentially missing a transition
            Some(GameFlowState::Undefined)
        }
    }
}

fn read_transition(paths: &Paths, proc: &Process, module: u64) -> Option<Transition> {
    let mut buf = paths.transition_description.read(proc, module)?;

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

struct Game {
    process: Process,
    paths: Paths,
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
        let version = process
            .get_module_size(EXE)
            .ok()
            .and_then(Version::from_module_size)?;
        let paths = Paths::new(version);

        Some(Self {
            process,
            paths,
            module,
            game_flow_state: Watcher::new(),
            transition_description: Watcher::new(),
            squidward_boss_health: Watcher::new(),
            ready_to_end: false,
        })
    }

    fn is_loading(&self) -> bool {
        let Some(world_ptr) = self
            .paths.current_world
            .read(&self.process, self.module) else
        {
            return false;
        };

        let num_streaming_levels_loading = self
            .paths
            .num_streaming_levels_being_loaded
            .read(&self.process, self.module)
            .unwrap_or(0);

        world_ptr == 0
            || !self
                .paths
                .begun_play
                .read(&self.process, self.module)
                .unwrap_or(false)
            || num_streaming_levels_loading > 0
    }
}

#[derive(Debug, Default)]
struct Settings {
    reset: bool,
}

impl Settings {
    fn new() -> Self {
        let reset = asr::user_settings::add_bool("Reset", "Reset on New Game", true);
        Self { reset }
    }
}

struct State {
    game: Option<Game>,
    settings: Settings,
}

impl State {
    fn new() -> Self {
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

        let transition = read_transition(&game.paths, &game.process, game.module)
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
            .update(GameFlowState::read(&game.paths, &game.process, game.module))
            .copied();

        let begun_play = game
            .paths
            .begun_play
            .read(&game.process, game.module)
            .unwrap_or(true);

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
            .paths
            .streaming_levels_len
            .read(&game.process, game.module)
            .unwrap_or(0)
            == 5
        {
            let boss_health = game
                .paths
                .squidward_boss_health
                .read(&game.process, game.module)
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

pub enum Version {
    V1_0_2, // Revision 684088
    V1_0_3, // Revision 687718
}

impl Version {
    fn from_module_size(size: u64) -> Option<Self> {
        match size {
            0x5D7_3000 => Some(Self::V1_0_2),
            0x5D4_B000 => Some(Self::V1_0_3),
            x => {
                asr::print_message(format!("Unknown module size: {x:#X}").as_str());
                None
            }
        }
    }
}
