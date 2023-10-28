#![no_std]

mod ffi;
mod memory;
mod paths;

use asr::{settings::Gui, timer::TimerState, Address, Process};
use memory::Memory;
use widestring::{u16cstr, U16CStr};

const EXE: &str = "CosmicShake-Win64-Shipping.exe";

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

impl Transition {
    pub const MENU: &U16CStr = u16cstr!("/Game/CS/Maps/MainMenu/MainMenu_P");
    pub const HUB: &U16CStr = u16cstr!("/Game/CS/Maps/BikiniBottom/BB_P");
    pub const OVERWORLD: &U16CStr = u16cstr!("/Game/CS/Maps/StreamingOverworld/Overworld_P");
}

struct Game {
    process: Process,
    module: Address,
    memory: Memory,
    ready_to_end: bool,
}

impl Game {
    fn attach() -> Option<Self> {
        let process = Process::attach(EXE)?;
        let module = process.get_module_address(EXE).ok()?;
        let version = process
            .get_module_size(EXE)
            .ok()
            .and_then(Version::from_module_size)?;

        Some(Self {
            process,
            module,
            memory: Memory::new(version),
            ready_to_end: false,
        })
    }
}

#[derive(Debug, Gui)]
struct Settings {
    /// Reset on New Game
    #[default = true]
    reset: bool,
}

struct State {
    game: Option<Game>,
    settings: Settings,
}

impl State {
    fn new() -> Self {
        Self {
            game: None,
            settings: Settings::register(),
        }
    }

    fn update(&mut self) {
        // TODO: Limit how often this is called. We could lower the splitter update rate while unhooked
        self.ensure_hooked();
        self.settings.update();

        let Some(game) = self.game.as_mut() else {
            return;
        };

        game.memory.update(&game.process, game.module);

        if let Some(transition) = game.memory.transition_description() {
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

        let game_flow_state = game.memory.game_flow_state();
        let is_loading = game.memory.is_loading(&game.process, game.module);

        if !game.memory.begun_play().map(|x| x.current).unwrap_or(true) {
            asr::timer::pause_game_time();
        } else if matches!(
            game.memory.transition_description().map(|x| x.current),
            Some(Transition::Menu)
        ) {
            if is_loading {
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
                Some(GameFlowState::RescueState) if is_loading => asr::timer::pause_game_time(),
                None if is_loading => asr::timer::pause_game_time(),
                _ => {
                    asr::timer::resume_game_time();
                }
            }
        }

        if let Some(boss_health) = game.memory.squidward_boss_health() {
            if boss_health.changed_to(&0) {
                game.ready_to_end = true;
            }
        }

        // After final boss is defeated, split at the start of the next load
        if game.ready_to_end
            && matches!(
                game_flow_state.map(|x| x.current),
                Some(GameFlowState::LoadingTransitionState)
            )
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
            _ => {
                asr::print_message("Unknown module size. Game version not supported.");
                None
            }
        }
    }
}

#[cfg(all(not(test), target_arch = "wasm32"))]
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    core::arch::wasm32::unreachable()
}
