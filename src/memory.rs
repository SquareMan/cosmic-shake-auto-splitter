use asr::{
    watcher::{Pair, Watcher},
    Process,
};
use widestring::U16CStr;

use crate::{paths::Paths, GameFlowState, Transition, Version};

pub struct Memory {
    paths: Paths,
    /// Path for the currently loaded level. Limited usefulness as this game relies heavily on level streaming
    transition_description: Watcher<Transition>,
    begun_play: Watcher<bool>,
    game_flow_state: Watcher<GameFlowState>,
    squidward_boss_health: Watcher<u32>,
}

impl Memory {
    pub fn new(version: Version) -> Self {
        Self {
            paths: Paths::new(version),
            transition_description: Watcher::default(),
            begun_play: Watcher::default(),
            game_flow_state: Watcher::default(),
            squidward_boss_health: Watcher::default(),
        }
    }

    pub fn update(&mut self, proc: &Process, module: u64) {
        self.read_transition(proc, module)
            .map(|x| self.transition_description.update_infallible(x));
        self.read_game_flow_state(proc, module)
            .map(|x| self.game_flow_state.update_infallible(x));
        self.begun_play
            .update(self.paths.begun_play.read(proc, module));
        self.squidward_boss_health
            .update(self.read_boss_health(proc, module));
    }

    pub fn is_loading(&self, proc: &Process, module: u64) -> bool {
        let Some(world_ptr) = self
            .paths.current_world
            .read(proc, module) else
        {
            return false;
        };

        let num_streaming_levels_loading = self
            .paths
            .num_streaming_levels_being_loaded
            .read(proc, module)
            .unwrap_or(0);

        world_ptr == 0
            || !self.paths.begun_play.read(proc, module).unwrap_or(false)
            || num_streaming_levels_loading > 0
    }

    pub fn transition_description(&self) -> Option<&Pair<Transition>> {
        self.transition_description.pair.as_ref()
    }

    pub fn begun_play(&self) -> Option<&Pair<bool>> {
        self.begun_play.pair.as_ref()
    }

    pub fn game_flow_state(&self) -> Option<&Pair<GameFlowState>> {
        self.game_flow_state.pair.as_ref()
    }

    pub fn squidward_boss_health(&self) -> Option<&Pair<u32>> {
        self.squidward_boss_health.pair.as_ref()
    }
}

impl Memory {
    fn read_transition(&self, proc: &Process, module: u64) -> Option<Transition> {
        let mut buf = self.paths.transition_description.read(proc, module)?;

        // We force the last byte in the buffer to null after reading these strings, so we know we can unwrap here
        *buf.last_mut().unwrap() = 0;
        let cstr = U16CStr::from_slice_truncate(&buf).unwrap();

        // can't match on U16CStr
        if cstr == Transition::MENU {
            Some(Transition::Menu)
        } else if cstr == Transition::HUB {
            Some(Transition::Hub)
        } else if cstr == Transition::OVERWORLD {
            Some(Transition::Overworld)
        } else {
            None
        }
    }

    fn read_game_flow_state(&self, proc: &Process, module: u64) -> Option<GameFlowState> {
        let last_game_state = self
            .paths
            .game_flow_state
            .read(proc, module)
            .map(bytemuck::checked::cast::<u8, _>)?;
        let game_state_len = self.paths.game_flow_state_len.read(proc, module)?;
        if game_state_len > 0 {
            Some(last_game_state)
        } else {
            // Treat no active states as undefined, this avoids updating our watcher with None and potentially missing a transition
            Some(GameFlowState::Undefined)
        }
    }

    fn read_boss_health(&self, proc: &Process, module: u64) -> Option<u32> {
        // Sanity check that we're in the final boss
        // TODO: update this once we're able to tell the name of the active level
        let streaming_levels_len = self.paths.streaming_levels_len.read(proc, module)?;
        if streaming_levels_len != 5 {
            return None;
        }
        self.paths.squidward_boss_health.read(proc, module)
    }
}
