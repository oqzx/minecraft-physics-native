'use strict';

// The fluid cells a NativeWorld stores through its write paths: source and flowing states round-trip
// with their heights, the versioned path checks them against the generated release tables, and an
// empty cell never carries the source or falling bit. Run after `pnpm build` and the native build.

const assert = require('node:assert/strict');
const { describe, it } = require('node:test');
const { LoadNativePhysics } = require('..');

const native = LoadNativePhysics();
const still = { x: 0, y: 0, z: 0 };

describe('native fluid states', () => {
    it('round-trips source and flowing cells with their FlowingFluid heights', () => {
        const world = new native.NativeWorld();
        // getOwnHeight is (float) getAmount() / 9.0F: a source (amount 8) stands 8/9 high, legacy
        // level 3 (amount 5) 5/9, and a falling cell keeps amount 8 (1.20.6 FlowingFluid.java)
        world.applyBlockUpdates([{ x: 0, y: 0, z: 0, fluid: 1 }]);
        assert.equal(world.blockAt(0, 0, 0).fluidSource, true);
        assert.equal(world.blockAt(0, 0, 0).fluidLevel, 0);
        assert.equal(world.fluidHeightAt(0, 0, 0), Math.fround(8 / 9));
        world.applyBlockUpdates([{ x: 0, y: 0, z: 0, fluid: 1, fluidLevel: 3, fluidSource: false }]);
        assert.equal(world.blockAt(0, 0, 0).fluidSource, false);
        assert.equal(world.blockAt(0, 0, 0).fluidLevel, 3);
        assert.equal(world.fluidHeightAt(0, 0, 0), Math.fround(5 / 9));
        world.applyBlockUpdates([{ x: 1, y: 0, z: 0, fluid: 2, fluidLevel: 9, fluidSource: false, fluidFalling: true }]);
        assert.equal(world.blockAt(1, 0, 0).fluidFalling, true);
        assert.equal(world.fluidHeightAt(1, 0, 0), Math.fround(8 / 9));
        // water above makes the height 1 (FlowingFluid.getHeight)
        world.applyBlockUpdates([{ x: 0, y: 1, z: 0, fluid: 1 }]);
        assert.equal(world.fluidHeightAt(0, 0, 0), 1);
        world.setFluidState(2, 0, 0, 1, 0, 6, false, false, still);
        assert.deepEqual([world.blockAt(2, 0, 0).fluidSource, world.blockAt(2, 0, 0).fluidLevel], [false, 6]);
        world.setFluidState(2, 0, 0, 1, 0, 0, true, false, still);
        assert.deepEqual([world.blockAt(2, 0, 0).fluidSource, world.blockAt(2, 0, 0).fluidLevel], [true, 0]);
    });

    it('checks versioned cells against each release table, flattened or not', () => {
        const world = new native.NativeWorld();
        for (const version of ['1.12.2', '1.13.2', '1.20.6', '26.3']) {
            world.setVersionedFluidState(0, 0, 0, version, 1, 0, 2, false, false, still);
            assert.deepEqual([world.blockAt(0, 0, 0).fluidSource, world.blockAt(0, 0, 0).fluidLevel], [false, 2], version);
            world.setVersionedFluidState(0, 0, 0, version, 2, 0, 0, true, false, still);
            assert.deepEqual([world.blockAt(0, 0, 0).fluid, world.blockAt(0, 0, 0).fluidSource], [2, true], version);
        }
        // before 1.13 the fluid is the liquid block's LEVEL 0-15; from 1.13 FlowingFluid's states
        assert.equal(native.generatedFluidStateCount('1.12.2', 'flowing_water'), 16);
        assert.equal(native.generatedFluidStateCount('1.13.2', 'water'), 2);
        assert.throws(() => world.setVersionedFluidState(0, 0, 0, '1.7.10', 1, 0, 0, true, false, still), /not present in the generated source definition/);
    });

    it('rejects source and falling bits on empty cells on every write path', () => {
        const world = new native.NativeWorld();
        assert.throws(() => world.setFluidState(0, 0, 0, 0, 0, 0, true, false, still), /empty fluid states cannot be source or falling fluids/);
        assert.throws(() => world.setFluidState(0, 0, 0, 0, 0, 0, false, true, still), /empty fluid states cannot be source or falling fluids/);
        assert.throws(() => world.setVersionedFluidState(0, 0, 0, '1.20.6', 0, 0, 0, true, false, still), /empty fluid states cannot be source or falling fluids/);
        assert.throws(() => world.applyBlockUpdates([{ x: 0, y: 0, z: 0, fluid: 0, fluidSource: true }]), /empty fluid states cannot be marked as source fluids/);
        assert.throws(() => world.applyBlockUpdates([{ x: 0, y: 0, z: 0, fluid: 0, fluidFalling: true }]), /empty fluid states cannot be marked as falling/);
        assert.equal(world.blockAt(0, 0, 0).present, false);
    });
});
