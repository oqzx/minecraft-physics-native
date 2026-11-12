'use strict';

// What a piston does to the blocks it is still moving, and what a caller outside the engine reads of them: the fork's
// physics plugin ticks them after the entities, as the client does.

const assert = require('node:assert/strict');
const { EventEmitter } = require('node:events');
const { describe, it } = require('node:test');
const { Vec3 } = require('vec3');
const adapter = require('..');

/** A world of blocks by name, which the tracker can change. */
function worldOf(version, named) {
    const registry = require('prismarine-registry')(version);
    const Block = require('prismarine-block')(registry);
    const air = registry.blocksByName.air.defaultState;
    const ids = new Map(named.map(([x, y, z, id]) => [`${x},${y},${z}`, id]));
    const stateAt = pos => ids.get(`${Math.floor(pos.x)},${Math.floor(pos.y)},${Math.floor(pos.z)}`) ?? air;
    const world = Object.assign(new EventEmitter(), {
        getBlock: pos => Block.fromStateId(stateAt(pos), 0),
        getBlockStateId: stateAt,
        setBlockStateId: (pos, id) => ids.set(`${pos.x},${pos.y},${pos.z}`, id),
    });
    return { registry, Block, world };
}

const EAST = 5;

describe('a sticky piston that retracts while it still pushes', () => {
    const sticky = () => {
        const version = '26.3';
        const { registry, Block } = worldOf(version, []);
        const base = Block.fromProperties('sticky_piston', { facing: 'east', extended: false }, 0).stateId;
        const made = worldOf(version, [[0, 1, 0, base], [1, 1, 0, registry.blocksByName.stone.defaultState]]);
        return { ...made, tracker: new adapter.MovingBlockTracker(made.registry, version, made.world) };
    };
    const cells = tracker => tracker.blocks().map(({ x, y, z }) => `${x},${y},${z}`).sort();

    it('ends the block it was pushing and drags nothing (26.3 PistonBaseBlock.java:173-186)', () => {
        const { registry, world, tracker } = sticky();
        tracker.trigger('sticky_piston', [0, 1, 0], 0, EAST);
        assert.deepEqual(cells(tracker), ['1,1,0', '2,1,0']);
        tracker.tick();
        tracker.trigger('sticky_piston', [0, 1, 0], 1, EAST);
        assert.deepEqual(cells(tracker), ['0,1,0']);
        assert.equal(world.getBlock(new Vec3(2, 1, 0)).name, 'stone');
        // (the head that ended stays: only a piston that takes nothing clears it)
        assert.equal(world.getBlock(new Vec3(1, 1, 0)).name, 'piston_head');
        assert.equal(registry.blocksByName.stone.defaultState, world.getBlockStateId(new Vec3(2, 1, 0)));
    });

    it('drags the block once it has come to rest', () => {
        const { world, tracker } = sticky();
        tracker.trigger('sticky_piston', [0, 1, 0], 0, EAST);
        for (let i = 0; i < 3; i += 1) tracker.tick();
        tracker.trigger('sticky_piston', [0, 1, 0], 1, EAST);
        assert.deepEqual(cells(tracker), ['0,1,0', '1,1,0']);
        assert.equal(world.getBlock(new Vec3(2, 1, 0)).name, 'air');
    });

    it('leaves a block another piston moves the other way', () => {
        const { registry, world, Block, tracker } = sticky();
        const west = Block.fromProperties('sticky_piston', { facing: 'west', extended: false }, 0).stateId;
        world.setBlockStateId(new Vec3(1, 1, 0), registry.blocksByName.air.defaultState);
        world.setBlockStateId(new Vec3(3, 1, 0), registry.blocksByName.stone.defaultState);
        world.setBlockStateId(new Vec3(4, 1, 0), west);
        tracker.trigger('sticky_piston', [4, 1, 0], 0, 4);
        assert.ok(cells(tracker).includes('2,1,0'));
        tracker.trigger('sticky_piston', [0, 1, 0], 1, EAST);
        assert.ok(cells(tracker).includes('2,1,0'), 'the west way, so it goes on');
    });
});

describe('the moving blocks of a world, to a caller outside', () => {
    const version = '1.10.2';
    /** An engine whose world has a bot's tracker, as a tick of a state with a bot makes it. */
    const tracked = () => {
        const registry = require('prismarine-registry')(version);
        const Block = require('prismarine-block')(registry);
        const world = worldOf(version, []).world;
        world.getBlock = world.getBlock.bind(world);
        const physics = new adapter.NativePhysics(registry);
        const bot = Object.assign(new EventEmitter(), { world });
        const sync = physics.syncOf(world);
        return { physics, world, bot, sync, Block, registry };
    };

    it('says nothing of a world with no bot', () => {
        const { physics, world } = tracked();
        assert.equal(physics.movingBlockTrackerOf(world), undefined);
        assert.equal(physics.movingPistonsOf(world), undefined);
        assert.equal(physics.movingBlockTrackerOf({}), undefined, 'and asking makes no link');
    });

    it('gives the tracker a bot started, and its blocks whole, the outline boxes among them', () => {
        const { physics, world, bot, sync, registry } = tracked();
        physics.syncMovingBlocks(sync, world, bot);
        const tracker = physics.movingBlockTrackerOf(world);
        assert.ok(tracker instanceof adapter.MovingBlockTracker);
        assert.deepEqual(physics.movingPistonsOf(world), []);
        const stone = registry.blocksByName.stone.id << 4;
        const piston = registry.blocksByName.piston.id << 4 | EAST;
        world.setBlockStateId(new Vec3(0, 1, 0), piston);
        world.setBlockStateId(new Vec3(1, 1, 0), stone);
        bot.emit('pistonMove', { name: 'piston', position: new Vec3(0, 1, 0) }, 0, EAST);
        const pistons = physics.movingPistonsOf(world);
        const moved = pistons.find(({ movedBlock }) => movedBlock === 'stone');
        assert.deepEqual(moved.pos, [2, 1, 0]);
        assert.deepEqual(moved.direction, [1, 0, 0]);
        assert.equal(moved.extending, true);
        assert.deepEqual(moved.shapes, [[0, 0, 0, 1, 1, 1]]);
        assert.deepEqual(moved.outlineShapes, moved.shapes);
    });
});

describe('several moving blocks of one tick', () => {
    const version = '26.3';
    const head = (facing, x) => {
        const plate = facing === 'east' ? [0.75, 0, 0, 1, 1, 1] : [0, 0, 0, 0.25, 1, 1];
        return { pos: [x, 1, 0], direction: [facing === 'east' ? 1 : -1, 0, 0], extending: true, source: true, progress: 0.5, shapes: [plate] };
    };
    const boat = () => ({
        age: 3, pos: new Vec3(0.9, 1, 0.5), vel: new Vec3(0, 0, 0), onGround: true, isCollidedHorizontally: false, isCollidedVertically: true,
        halfWidth: 0.3, height: 0.5625, lastVerticalVelocity: 0,
    });

    it('pushes a list as its singles when nothing stands between them', () => {
        const { registry, world } = worldOf(version, []);
        const pistons = [head('east', 0)];
        const [one, list] = [boat(), boat()];
        new adapter.NativeBoatPhysics(registry).pushByMovingPiston({ state: one }, world, pistons[0]);
        new adapter.NativeBoatPhysics(registry).pushByMovingPistons({ state: list }, world, pistons);
        assert.equal(list.pos.x, one.pos.x);
    });

    it('counts the limit of a tick over a whole list', () => {
        const { registry, world } = worldOf(version, []);
        const state = boat();
        new adapter.NativeBoatPhysics(registry).pushByMovingPistons({ state }, world, [head('east', 0), head('east', 1)]);
        assert.ok(state.pos.x - 0.9 <= 0.51 + 1e-9);
    });

    it('gives the player the list the engine takes', () => {
        const { registry } = worldOf(version, []);
        const seen = [];
        const physics = new adapter.NativePhysics(registry);
        const state = { pos: new Vec3(0, 1, 0) };
        physics.carries.set(state, {
            position: { x: 0, y: 1, z: 0 },
            player: { pushByMovingPistons: (_, pistons) => { seen.push(...pistons); return {}; } },
        });
        physics.takeOutput = () => {};
        const { world } = worldOf(version, []);
        physics.pushByMovingPistons({ state }, world, [
            head('east', 0), { ...head('west', 1), nextShapes: [[0, 0, 0, 0.25, 1, 1]] },
        ]);
        assert.deepEqual(seen.map(piston => piston.x), [0, 1]);
        assert.equal(seen[0].nextShapes, undefined);
        assert.equal(seen[1].nextShapes.length, 1);
    });
});
