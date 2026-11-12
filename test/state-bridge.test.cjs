'use strict';

// B015: every field of the object state has a stated way through a NativePlayer, and a packed tick and an object tick of
// the same state agree on nondefault values, with an absent attribute and an explicit zero kept apart.

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { describe, it } = require('node:test');
const physics = require('..');
const { FLAGS, KEPT, OUTPUTS, OUTPUT_FLAGS, PACKED, SCENE } = require('../scripts/state-bridge.cjs');

const { LoadNativePhysics, NativePlayerInput: IN, NativePlayerInputFlag: FLAG, NativePlayerOutput: OUT, NativePlayerOutputFlag: OUT_FLAG } = physics;
const native = LoadNativePhysics();
const declarations = fs.readFileSync(path.join(__dirname, '..', 'native', 'artifacts', 'native.d.ts'), 'utf8');
const stateFields = [.../export interface NativeSimulationState \{([\s\S]*?)\n\}/.exec(declarations)[1].matchAll(/^ {2}(\w+)\??:/gm)].map(match => match[1]);

const base = {
    position: { x: 0.5, y: 1, z: 0.5 }, velocity: { x: 0.03, y: -0.08, z: -0.02 }, yaw: 0.7, pitch: 0.2,
    look: { x: -0.6313, y: -0.1987, z: 0.7496 },
    halfWidth: 0.3, height: 1.8, eyeHeight: 1.62,
    onGround: true, collidedHorizontally: false, collidedVertically: true,
    isPlayer: true, clientSide: true, input: { x: 0.5, y: 0, z: 0.98 },
};

function floorWorld() {
    const world = new native.NativeWorld();
    const updates = [];
    for (let x = -3; x <= 3; x += 1) {
        for (let z = -3; z <= 3; z += 1) {
            updates.push({ x, y: 0, z, shape: { minX: 0, minY: 0, minZ: 0, maxX: 1, maxY: 1, maxZ: 1, slipperiness: 0.6, effect: 0 } });
        }
    }
    world.applyBlockUpdates(updates);
    return world;
}

/** The packed input of a state, as src/mineflayer/player.ts lays it out. */
function packedInput(state) {
    const input = new Float64Array(IN.LENGTH);
    const vector = (at, value) => { input[at] = value?.x ?? 0; input[at + 1] = value?.y ?? 0; input[at + 2] = value?.z ?? 0; };
    vector(IN.POSITION, state.position);
    vector(IN.VELOCITY, state.velocity);
    input[IN.YAW] = state.yaw ?? 0;
    input[IN.PITCH] = state.pitch ?? 0;
    vector(IN.LOOK, state.look);
    input[IN.STRAFE] = state.input?.x ?? 0;
    input[IN.FORWARD] = state.input?.z ?? 0;
    input[IN.HALF_WIDTH] = state.halfWidth;
    input[IN.HEIGHT] = state.height;
    input[IN.EYE_HEIGHT] = state.eyeHeight;
    input[IN.FALL_DISTANCE] = state.fallDistance ?? 0;
    input[IN.JUMP_TICKS] = state.jumpTicks ?? 0;
    input[IN.AUTO_JUMP_TIME] = state.autoJumpTime ?? 0;
    input[IN.FLYING_SPEED] = state.flyingSpeed ?? Math.fround(0.05);
    input[IN.ITEM_USE_SPEED_MULTIPLIER] = state.itemUseSpeedMultiplier ?? 0.2;
    input[IN.YAW_DEGREES] = (state.yaw ?? 0) * 180 / Math.PI;
    let flags = FLAG.UNMOVED;
    for (const [field, name] of Object.entries(FLAGS)) if (field !== 'previousPosition' && state[field]) flags |= FLAG[name];
    input[IN.FLAGS] = flags;
    return input;
}

describe('the state bridge', () => {
    it('states the way of every field of the object state, once', () => {
        const stated = [...Object.keys(PACKED), ...Object.keys(FLAGS), ...KEPT, ...Object.keys(SCENE)];
        assert.deepEqual(stateFields.filter(field => !stated.includes(field)), []);
        assert.deepEqual(stated.filter(field => !stateFields.includes(field)), []);
        assert.equal(new Set(stated).size, stated.length);
    });

    it('names only channels of the packed layout', () => {
        for (const channel of Object.values(PACKED)) assert.notEqual(IN[channel], undefined, channel);
        for (const channel of Object.values(FLAGS)) assert.notEqual(FLAG[channel], undefined, channel);
        for (const channel of Object.values(OUTPUTS)) assert.notEqual(OUT[channel], undefined, channel);
        for (const channel of Object.values(OUTPUT_FLAGS)) assert.notEqual(OUT_FLAG[channel], undefined, channel);
    });

    it('round-trips kept state through setState, an absent value staying absent and a zero staying zero', () => {
        const kept = {
            ...base, movementSpeedValue: 0, jumpStrength: 0, gravity: 0, stepHeight: 0, fallDamageMultiplier: 0, safeFallDistance: 0,
            depthStrider: 0, soulSpeed: 0, waterMovementEfficiency: 0, movementEfficiency: 0,
        };
        const player = new native.NativePlayer('26.3', kept, {});
        const state = player.state();
        for (const field of ['movementSpeedValue', 'jumpStrength', 'gravity', 'stepHeight', 'fallDamageMultiplier', 'safeFallDistance']) {
            assert.equal(state[field], 0, field);
        }
        const absent = new native.NativePlayer('26.3', base, {}).state();
        for (const field of ['movementSpeedValue', 'jumpStrength', 'gravity', 'stepHeight']) {
            assert.ok(absent[field] === undefined || absent[field] === null, `${field}: ${absent[field]}`);
        }
    });

    it('does not read an explicit zero as the default, on either path', () => {
        const world = floorWorld();
        const walk = extra => native.simulateVersionedTick('26.3', world, { ...base, ...extra }, {});
        assert.notDeepEqual(walk({ movementSpeedValue: 0 }).velocity, walk({}).velocity);
        assert.notEqual(walk({ gravity: 0, onGround: false }).velocity.y, walk({ onGround: false }).velocity.y);
        const packed = extra => {
            const state = { ...base, ...extra };
            return new native.NativePlayer('26.3', state, {}).tick(world, packedInput(state), new Float64Array(0));
        };
        assert.equal(packed({ movementSpeedValue: 0 })[OUT.VELOCITY], walk({ movementSpeedValue: 0 }).velocity.x);
    });
});

describe('every scalar field of the state', () => {
    const fieldList = /export interface NativeSimulationState \{([\s\S]*?)\n\}/.exec(declarations)[1];
    const typed = kind => [...fieldList.matchAll(new RegExp('^ {2}(\\w+)\\??: ' + kind + '\\r?$', 'gm'))].map(match => match[1]);
    const round = extra => new native.NativePlayer('26.3', { ...base, ...extra }, {}).state();

    it('keeps false, true and absent apart', () => {
        const required = ['onGround', 'collidedHorizontally', 'collidedVertically'];
        assert.ok(typed('boolean').length > 30);
        for (const field of typed('boolean')) {
            assert.equal(round({ [field]: true })[field], true, `${field} true`);
            assert.equal(round({ [field]: false })[field], false, `${field} false`);
            if (required.includes(field)) continue;
            const { [field]: unset, ...without } = base;
            assert.notEqual(new native.NativePlayer('26.3', without, {}).state()[field], true, `${field} absent`);
        }
    });

    it('keeps zero and a nondefault value', () => {
        // (what a single tick reads and does not outlive it: packed slots of that tick, or its scene)
        const perTick = ['heightAfterTick', 'fireworkRockets', 'fireworkRocketsBeforePlayer'];
        assert.ok(typed('number').length > 30);
        for (const field of typed('number')) {
            if (['halfWidth', 'height', ...perTick].includes(field)) continue;
            assert.equal(round({ [field]: 0 })[field] ?? 0, 0, `${field} 0`);
            assert.equal(round({ [field]: 1 })[field], 1, `${field} 1`);
        }
    });
});

describe('a packed tick and an object tick', () => {
    const fixtures = {
        walking: {},
        sprinting: { sprinting: true, wasSprinting: true, sprintModifier: true },
        sneaking: { sneaking: true, movingSlowly: true, sneakingSpeed: 0.45 },
        slowed: { movementSpeedValue: 0.03, stepHeight: 1 },
        falling: { onGround: false, collidedVertically: false, fallDistance: 3.5, velocity: { x: 0, y: -0.6, z: 0 } },
        'at the ground with a fall': { position: { x: 0.5, y: 1.2, z: 0.5 }, onGround: false, collidedVertically: false, fallDistance: 10, safeFallDistance: 3, fallDamageMultiplier: 1, velocity: { x: 0, y: -0.6, z: 0 }, clientSide: false, controlledByLocalInstance: true },
        'without gravity': { onGround: false, collidedVertically: false, noGravity: true },
        'custom gravity': { onGround: false, collidedVertically: false, gravity: 0.02 },
        'item use': { usingItem: true, itemUseSpeedMultiplier: 0.2 },
        jumping: { jumpRequested: true, jumpStrength: 0.6 },
    };

    for (const version of ['1.8.8', '1.12.2', '1.20.4', '1.21.5', '26.3']) {
        it(`agree on ${version}`, () => {
            const world = floorWorld();
            for (const [name, extra] of Object.entries(fixtures)) {
                const state = { ...base, ...extra };
                const object = native.simulateVersionedTick(version, world, state, {});
                const player = new native.NativePlayer(version, state, {});
                const output = player.tick(world, packedInput(state), new Float64Array(0));
                const key = `${version} ${name}`;
                assert.deepEqual([output[OUT.POSITION], output[OUT.POSITION + 1], output[OUT.POSITION + 2]], [object.position.x, object.position.y, object.position.z], key);
                assert.deepEqual([output[OUT.VELOCITY], output[OUT.VELOCITY + 1], output[OUT.VELOCITY + 2]], [object.velocity.x, object.velocity.y, object.velocity.z], key);
                assert.equal(output[OUT.FALL_DISTANCE], object.fallDistance ?? 0, key);
                assert.equal(output[OUT.FALL_DAMAGE], object.fallDamage ?? 0, key);
                assert.equal(!!(output[OUT.FLAGS] & OUT_FLAG.ON_GROUND), object.onGround, key);
                assert.equal(!!(output[OUT.FLAGS] & OUT_FLAG.COLLIDED_VERTICALLY), object.collidedVertically, key);
                assert.equal(!!(output[OUT.FLAGS] & OUT_FLAG.COLLIDED_HORIZONTALLY), object.collidedHorizontally, key);
            }
        });
    }

    it('report a landing\'s damage only on the tick of the landing', () => {
        const world = floorWorld();
        const state = { ...base, position: { x: 0.5, y: 1.2, z: 0.5 }, onGround: false, collidedVertically: false, fallDistance: 10, safeFallDistance: 3, fallDamageMultiplier: 1, velocity: { x: 0, y: -0.6, z: 0 }, clientSide: true, controlledByLocalInstance: true };
        const object = native.simulateVersionedTick('1.21.5', world, state, {});
        assert.ok(object.fallDamage > 0);
        const player = new native.NativePlayer('1.21.5', state, {});
        assert.equal(player.tick(world, packedInput(state), new Float64Array(0))[OUT.FALL_DAMAGE], object.fallDamage);
        const next = { ...object, input: state.input, look: state.look, eyeHeight: state.eyeHeight };
        assert.equal(native.simulateVersionedTick('1.21.5', world, next, {}).fallDamage ?? 0, 0);
        assert.equal(player.tick(world, packedInput(next), new Float64Array(0))[OUT.FALL_DAMAGE], 0);
    });

    it('agree on the damage a server state is handed, and a client state is not', () => {
        const world = new native.NativeWorld();
        world.applyBlockUpdates([{ x: 0, y: 1, z: 0, shape: { minX: 0, minY: 0, minZ: 0, maxX: 0, maxY: 0, maxZ: 0, slipperiness: 0.6, effect: 8 } }]);
        for (const clientSide of [false, true]) {
            const state = { ...base, onGround: false, collidedVertically: false, noGravity: true, velocity: { x: 0, y: 0, z: 0 }, input: undefined, ticksFrozen: 140, tickCount: 39, clientSide };
            const object = native.simulateVersionedTick('26.3', world, state, {});
            const player = new native.NativePlayer('26.3', state, {});
            const output = player.tick(world, packedInput(state), new Float64Array(0));
            assert.equal(output[OUT.FREEZE_DAMAGE], object.freezeDamage ?? 0);
            assert.equal(output[OUT.FREEZE_DAMAGE], clientSide ? 0 : 1);
        }
    });
});
