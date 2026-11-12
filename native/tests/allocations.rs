//! Heap use of the tick itself, counted by a global allocator: exact numbers, which a loaded machine does not move.
//! `cargo test --release --test allocations -- --nocapture` prints them; the assertions hold the two that bound a long
//! run, that a warm tick keeps nothing and that the count per tick does not rise with the run's length.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};

use minecraft_physics_native::math::Vec3;
use minecraft_physics_native::simulation::{PhysicsConfig, SimulationState};
use minecraft_physics_native::versioned;
use minecraft_physics_native::world::{BlockShape, BlockState, FluidState, WorldSnapshot};

struct Counting;

static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
static BYTES: AtomicU64 = AtomicU64::new(0);
static LIVE: AtomicI64 = AtomicI64::new(0);

// SAFETY: every call goes to the System allocator unchanged; the counters do not touch the memory.
#[allow(unsafe_code)]
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        LIVE.fetch_add(layout.size() as i64, Ordering::Relaxed);
        System.alloc(layout)
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size() as i64, Ordering::Relaxed);
        System.dealloc(pointer, layout)
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        BYTES.fetch_add(size as u64, Ordering::Relaxed);
        LIVE.fetch_add(size as i64 - layout.size() as i64, Ordering::Relaxed);
        System.realloc(pointer, layout, size)
    }
}

#[global_allocator]
static COUNTING: Counting = Counting;

fn full() -> BlockState {
    BlockState {
        id: 1,
        tags: 0,
        shape: Some(BlockShape::full_block()),
        outline_shape: Some(BlockShape::full_block()),
        interaction_shape: Some(BlockShape::full_block()),
        ..BlockState::default()
    }
}

fn floor(fluid: bool) -> WorldSnapshot {
    let mut world = WorldSnapshot::new();
    for x in -8..64 {
        for z in -8..8 {
            world.set_block((x, 0, z), full());
            if fluid {
                for y in 1..5 {
                    let water = BlockState {
                        fluid: FluidState::Water,
                        fluid_level: 8,
                        fluid_source: true,
                        water_block: true,
                        ..BlockState::default()
                    };
                    world.set_block((x, y, z), water);
                }
            }
        }
    }
    world
}

fn walker(fluid: bool) -> SimulationState {
    // (field by field: the state has private fields, which a struct update from outside the crate cannot name)
    let mut state = SimulationState::default();
    state.position = Vec3::new(0.5, 1.0, 0.5);
    state.half_width = 0.3;
    state.height = 1.8;
    state.input = Vec3::new(0.0, 0.0, 1.0);
    state.on_ground = !fluid;
    state.in_water = fluid;
    state.is_player = true;
    state
}

struct Counts {
    allocations: u64,
    bytes: u64,
    live: i64,
}

fn run(version: &str, world: &WorldSnapshot, state: &mut SimulationState, ticks: u32) -> Counts {
    let config = PhysicsConfig::default();
    let (allocations, bytes, live) = (
        ALLOCATIONS.load(Ordering::Relaxed),
        BYTES.load(Ordering::Relaxed),
        LIVE.load(Ordering::Relaxed),
    );
    for _ in 0..ticks {
        versioned::dispatch(version, state, world, config).expect("the tick runs");
    }
    Counts {
        allocations: ALLOCATIONS.load(Ordering::Relaxed) - allocations,
        bytes: BYTES.load(Ordering::Relaxed) - bytes,
        live: LIVE.load(Ordering::Relaxed) - live,
    }
}

#[test]
fn a_warm_tick_keeps_nothing_and_its_count_does_not_rise() {
    for version in ["1.8.8", "1.12.2", "1.16.5", "1.20.4", "1.21.11", "26.3"] {
        for (name, fluid) in [("walk", false), ("swim", true)] {
            let world = floor(fluid);
            let mut state = walker(fluid);
            run(version, &world, &mut state, 100);
            let short = run(version, &world, &mut state, 500);
            let long = run(version, &world, &mut state, 5000);
            println!(
                "{version} {name}: {:.2} allocations and {:.1} bytes a tick, {} bytes kept over 5000 ticks",
                long.allocations as f64 / 5000.0,
                long.bytes as f64 / 5000.0,
                long.live,
            );
            assert_eq!(long.live, 0, "{version} {name}: a tick keeps memory");
            assert!(
                long.allocations / 5000 <= short.allocations / 500 + 1,
                "{version} {name}: the count rises with the run"
            );
        }
    }
}
