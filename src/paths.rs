use asr::{Address, Process};
use bytemuck::Pod;

use crate::Version;

const GAME_INSTANCE_OFFSET: u64 = 0xD28;
const SUBSYSTEM_MAP_OFFSET: u64 = 0xF0;

const TRANSITION_DESCRIPTION_OFFSET: u64 = 0x8B0;

const WORLD_CONTEXT_OFFSET: u64 = 0x30;
const CURRENT_WORLD_OFFSET: u64 = 0x280;
const STREAMING_LEVELS_OFFSET: u64 = 0x88;
const STREAMING_LEVELS_LEN_OFFSET: u64 = 0x90;
const PERSISTENT_LEVEL_OFFSET: u64 = 0x128;
const ACTORS_OFFSET: u64 = 0x98;
const HEALTH_COMPONENT_OFFSET: u64 = 0x508;
const CURRENT_HEALTH_OFFSET: u64 = 0x264;

pub struct Path<T, const N: usize> {
    path: [u64; N],
    ty: core::marker::PhantomData<T>,
}
pub struct BitPath<const N: usize> {
    path: Path<u8, N>,
    bit_num: u8,
}
pub struct Paths {
    pub game_flow_state: Path<u8, 6>,
    pub game_flow_state_len: Path<u8, 5>,
    pub transition_description: Path<[u16; 34], 3>,
    pub current_world: Path<usize, 4>,
    pub num_streaming_levels_being_loaded: Path<u16, 5>,
    pub begun_play: BitPath<5>,
    pub squidward_boss_health: Path<u32, 11>,
    pub streaming_levels_len: Path<u32, 5>,
}

impl Paths {
    pub fn new(version: Version) -> Self {
        let game_engine_offset; // TODO: Sig scan for this for resilency to game updates.
        let game_flow_manager_offset;

        match version {
            Version::V1_0_2 => {
                game_engine_offset = 0x0575_8730;
                game_flow_manager_offset = 0xC8;
            }
            Version::V1_0_3 => {
                game_engine_offset = 0x0576_1E70;
                game_flow_manager_offset = 0xE0;
            }
        };

        Self {
            // NOTE: We have to check the list len (0x68) and then dereference the data pointer (0x60) because during normal gameplay
            //       it simply sets the len to 0 and keep the stale state around (also it's null on the main menu)
            game_flow_state: Path::new([
                game_engine_offset,
                GAME_INSTANCE_OFFSET,
                SUBSYSTEM_MAP_OFFSET,
                game_flow_manager_offset,
                0x60,
                0x0,
            ]),
            game_flow_state_len: Path::new([
                game_engine_offset,
                GAME_INSTANCE_OFFSET,
                SUBSYSTEM_MAP_OFFSET,
                game_flow_manager_offset,
                0x68,
            ]),
            transition_description: Path::new([
                game_engine_offset,
                TRANSITION_DESCRIPTION_OFFSET,
                0,
            ]),
            current_world: Path::new([
                game_engine_offset,
                GAME_INSTANCE_OFFSET,
                WORLD_CONTEXT_OFFSET,
                CURRENT_WORLD_OFFSET,
            ]),
            num_streaming_levels_being_loaded: Path::new([
                game_engine_offset,
                GAME_INSTANCE_OFFSET,
                WORLD_CONTEXT_OFFSET,
                CURRENT_WORLD_OFFSET,
                0x5EA,
            ]),
            begun_play: BitPath::new(
                [
                    game_engine_offset,
                    GAME_INSTANCE_OFFSET,
                    WORLD_CONTEXT_OFFSET,
                    CURRENT_WORLD_OFFSET,
                    0x10D,
                ],
                0,
            ),
            // TODO: consider avoiding hardcoding these indexes by searching for the correct object within arrays
            squidward_boss_health: Path::new([
                game_engine_offset,
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
            ]),
            streaming_levels_len: Path::new([
                game_engine_offset,
                GAME_INSTANCE_OFFSET,
                WORLD_CONTEXT_OFFSET,
                CURRENT_WORLD_OFFSET,
                STREAMING_LEVELS_LEN_OFFSET,
            ]),
        }
    }
}

impl<T: Pod, const N: usize> Path<T, N> {
    fn new(path: [u64; N]) -> Self {
        Self {
            path,
            ty: core::marker::PhantomData,
        }
    }

    pub fn read(&self, proc: &Process, module: Address) -> Option<T> {
        proc.read_pointer_path64(module, &self.path).ok()
    }
}

impl<const N: usize> BitPath<N> {
    fn new(path: [u64; N], bit_num: u8) -> Self {
        assert!(bit_num < 8, "bit_num must be within range 0..8");
        Self {
            path: Path::new(path),
            bit_num,
        }
    }

    pub fn read(&self, proc: &Process, module: Address) -> Option<bool> {
        let mask = 1 << self.bit_num;
        self.path.read(proc, module).map(|field| field & mask != 0)
    }
}
