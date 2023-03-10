use asr::Process;
use bytemuck::Pod;

use crate::Version;

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

pub struct Path<T> {
    path: &'static [u64],
    ty: std::marker::PhantomData<T>,
}
pub struct BitPath {
    path: Path<u8>,
    bit_num: u8,
}
pub struct Paths {
    pub game_flow_state: Path<u8>,
    pub game_flow_state_len: Path<u8>,
    pub transition_description: Path<[u16; 34]>,
    pub current_world: Path<usize>,
    pub num_streaming_levels_being_loaded: Path<u16>,
    pub begun_play: BitPath,
    pub squidward_boss_health: Path<u32>,
    pub streaming_levels_len: Path<u32>,
}

impl Paths {
    pub fn new(_version: Version) -> Self {
        Self {
            game_flow_state: Path::new(&GAME_FLOW_STATE_PATH),
            game_flow_state_len: Path::new(&GAME_FLOW_STATE_LEN_PATH),
            transition_description: Path::new(&TRANSITION_DESCRIPTION_PATH),
            current_world: Path::new(&CURRENT_WORLD_PATH),
            num_streaming_levels_being_loaded: Path::new(&NUM_STREAMING_LEVELS_BEING_LOADED_PATH),
            begun_play: BitPath::new(&BEGUN_PLAY_PATH, 0),
            squidward_boss_health: Path::new(&SQUIDWARD_BOSS_HEALTH_PATH),
            streaming_levels_len: Path::new(&STREAMING_LEVELS_LEN_PATH),
        }
    }
}

impl<T: Pod> Path<T> {
    fn new(path: &'static [u64]) -> Self {
        Self {
            path,
            ty: std::marker::PhantomData,
        }
    }

    pub fn read(&self, proc: &Process, module: u64) -> Option<T> {
        proc.read_pointer_path64(module, self.path).ok()
    }
}

impl BitPath {
    fn new(path: &'static [u64], bit_num: u8) -> Self {
        assert!(bit_num < 8, "bit_num must be within range 0..8");
        Self {
            path: Path::new(path),
            bit_num,
        }
    }

    pub fn read(&self, proc: &Process, module: u64) -> Option<bool> {
        let mask = 1 << self.bit_num;
        self.path.read(proc, module).map(|field| field & mask != 0)
    }
}
