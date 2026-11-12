'use strict';

// The contract of docs/state-contract.md and docs/checkpoints.md through the public entry points:
// the object tick, the packed NativePlayer and the batch must agree, by the bits, and a checkpoint
// must go on exactly where the run would have.

const assert = require('node:assert/strict');
const { it } = require('node:test');
const { readFileSync } = require('node:fs');
const { join } = require('node:path');
const {
    LoadNativePhysics, NativePlayerInput: In, NativePlayerInputFlag: InFlag, NativePlayerOutput: Out,
    NativePlayerOutputFlag: OutFlag, NATIVE_PLAYER_LAYOUT, SUPPORTED_VERSIONS, encodeCheckpoint, decodeCheckpoint,
} = require('..');

const native = LoadNativePhysics();
const RELEASES = ['1.8.8', '1.12.2', '1.16.5', '1.20.4', '1.21.5', '1.21.11', '26.3'];

function floorWorld() {
    const world = new native.NativeWorld();
    const updates = [];
    for (let x = -4; x < 24; x++) {
        for (let z = -4; z < 8; z++) {
            updates.push({
                x, y: 0, z, id: 1,
                shape: { minX: 0, minY: 0, minZ: 0, maxX: 1, maxY: 1, maxZ: 1, slipperiness: x > 10 ? 0.98 : 0.6 },
            });
        }
    }
    updates.push({ x: 9, y: 1, z: 2, id: 2, shape: { minX: 0, minY: 0, minZ: 0, maxX: 1, maxY: 0.5, maxZ: 1, slipperiness: 0.6 } });
    world.applyBlockUpdates(updates);
    return world;
}

function walker(overrides = {}) {
    return {
        position: { x: 0.5, y: 1, z: 2.5 }, velocity: { x: 0, y: 0, z: 0 }, halfWidth: 0.3, height: 1.8, eyeHeight: 1.62,
        onGround: true, collidedHorizontally: false, collidedVertically: false, isPlayer: true,
        controlledByLocalInstance: true, input: { x: 0, y: 0, z: 1 }, yaw: -Math.PI / 2,
        look: { x: 1, y: 0, z: 0 }, sprinting: true, movementSpeed: 0.10000000149011612, flyingSpeed: 0.05,
        safeFallDistance: 3, ...overrides,
    };
}

/** A NativePlayer's input for what `state` holds, as an adapter sends it. */
function packed(state, unmoved) {
    const values = new Float64Array(In.LENGTH);
    values.set([state.position.x, state.position.y, state.position.z], In.POSITION);
    values.set([state.velocity.x, state.velocity.y, state.velocity.z], In.VELOCITY);
    values.set([state.look.x, state.look.y, state.look.z], In.LOOK);
    values[In.YAW] = state.yaw;
    values[In.YAW_DEGREES] = state.yawDegrees ?? (state.yaw * 180) / Math.PI;
    values[In.STRAFE] = state.input.x;
    values[In.FORWARD] = state.input.z;
    values[In.HALF_WIDTH] = state.halfWidth;
    values[In.HEIGHT] = state.height;
    values[In.EYE_HEIGHT] = state.eyeHeight;
    values[In.FALL_DISTANCE] = state.fallDistance ?? 0;
    values[In.JUMP_TICKS] = state.jumpTicks ?? 0;
    values[In.AUTO_JUMP_TIME] = state.autoJumpTime ?? 0;
    const stuck = state.stuckSpeedMultiplier ?? { x: 0, y: 0, z: 0 };
    values.set([stuck.x, stuck.y, stuck.z], In.STUCK_SPEED_MULTIPLIER);
    values[In.FLYING_SPEED] = state.flyingSpeed;
    values[In.ITEM_USE_SPEED_MULTIPLIER] = 0.2;
    const bit = (set, flag) => (set ? flag : 0);
    values[In.FLAGS] = bit(state.sprinting, InFlag.SPRINTING) | bit(state.wasSprinting, InFlag.WAS_SPRINTING)
        | bit(state.onGround, InFlag.ON_GROUND) | bit(state.collidedHorizontally, InFlag.COLLIDED_HORIZONTALLY)
        | bit(state.minorHorizontalCollision, InFlag.MINOR_HORIZONTAL_COLLISION)
        | bit(state.collidedVertically, InFlag.COLLIDED_VERTICALLY)
        | bit(state.verticalCollisionBelow, InFlag.VERTICAL_COLLISION_BELOW) | bit(state.inWater, InFlag.IN_WATER)
        | bit(state.inLava, InFlag.IN_LAVA) | bit(state.jumpRequested, InFlag.JUMP_REQUESTED)
        | bit(unmoved, InFlag.UNMOVED) | bit(state.sprintModifier, InFlag.SPRINT_MODIFIER);
    return values;
}

const NO_ENTITIES = new Float64Array(0);

/** The ticks' keys, a script of what the caller changes each tick. */
function keysAt(tick) {
    return { jumpRequested: tick % 7 === 3, input: { x: tick % 11 === 5 ? 1 : 0, y: 0, z: 1 } };
}

function sameBits(left, right, where) {
    const a = JSON.stringify(left, (key, value) => (Object.is(value, -0) ? '-0' : Number.isNaN(value) ? 'NaN' : value));
    const b = JSON.stringify(right, (key, value) => (Object.is(value, -0) ? '-0' : Number.isNaN(value) ? 'NaN' : value));
    assert.equal(a, b, where);
}

it('the object, packed and batch ticks agree through jumps, a slab, ice and a teleport', () => {
    for (const version of RELEASES) {
        const world = floorWorld();
        const start = walker({ yawDegrees: -90, sprintModifier: true, verticalCollisionBelow: true, collidedVertically: true });
        let object = start;
        const player = new native.NativePlayer(version, start, {});
        let last = player.state();
        for (let tick = 0; tick < 50; tick++) {
            const keys = keysAt(tick);
            // Teleport: a position assignment drops the previous position (no UNMOVED on the packed path).
            const teleported = tick === 30;
            const place = teleported ? { position: { x: 12.5, y: 3, z: 2.5 }, previousPosition: undefined } : {};
            object = native.simulateVersionedTickOutcome(version, world, { ...object, ...keys, ...place }, {}).state;
            const out = player.tick(world, packed({ ...last, ...keys, ...place }, !teleported), NO_ENTITIES);
            last = player.state();
            const where = `${version} tick ${tick}`;
            sameBits([out[Out.POSITION], out[Out.POSITION + 1], out[Out.POSITION + 2]],
                [object.position.x, object.position.y, object.position.z], `${where} position`);
            sameBits([out[Out.VELOCITY], out[Out.VELOCITY + 1], out[Out.VELOCITY + 2]],
                [object.velocity.x, object.velocity.y, object.velocity.z], `${where} velocity`);
            const flags = out[Out.FLAGS];
            assert.equal((flags & OutFlag.ON_GROUND) !== 0, object.onGround, `${where} onGround`);
            assert.equal((flags & OutFlag.VERTICAL_COLLISION_BELOW) !== 0, !!object.verticalCollisionBelow, `${where} below`);
            assert.equal(out[Out.FALL_DISTANCE], object.fallDistance ?? 0, `${where} fall`);
            assert.equal(out[Out.JUMP_TICKS], object.jumpTicks ?? 0, `${where} jump ticks`);
            sameBits(last.supportingBlock, object.supportingBlock, `${where} support`);
        }
        // the batch of the same keys: a constant input, so the script's first tick for all
        const keys = { input: { x: 0, y: 0, z: 1 } };
        let stepped = { ...walker(), ...keys };
        for (let tick = 0; tick < 20; tick++) stepped = native.simulateVersionedTickOutcome(version, world, stepped, {}).state;
        const [batched] = native.simulateVersionedTicks(version, world, [{ ...walker(), ...keys }], 20, {});
        sameBits(batched.position, stepped.position, `${version} batch position`);
        sameBits(batched.velocity, stepped.velocity, `${version} batch velocity`);
        assert.equal(batched.tickCount, stepped.tickCount, `${version} batch tick count`);
    }
});

it('a clone of a NativePlayer goes on by itself and takes no event of the original', () => {
    const world = floorWorld();
    const original = new native.NativePlayer('26.3', walker({ position: { x: 0.5, y: 6, z: 2.5 }, onGround: false, input: { x: 0, y: 0, z: 0 } }), {});
    let state = original.state();
    let landed = null;
    for (let tick = 0; tick < 40 && landed === null; tick++) {
        const out = original.tick(world, packed(state, true), NO_ENTITIES);
        state = original.state();
        if (out[Out.FALL_DAMAGE] > 0 || state.onGround) landed = out;
    }
    assert.ok(landed, 'it lands');
    const fork = new native.NativePlayer('26.3', original.state(), {});
    const forkedAt = fork.state();
    sameBits(forkedAt, original.state(), 'a clone is the state');
    // what a tick reports is that tick's: the next one starts at nothing
    const nextOut = fork.tick(world, packed(forkedAt, true), NO_ENTITIES);
    assert.equal(nextOut[Out.FALL_DAMAGE], 0);
    assert.equal(original.state().tickCount, forkedAt.tickCount, 'the clone ticks alone');
});

it('reading a state twice replays nothing, and setState keeps every persistent field', () => {
    const world = floorWorld();
    for (const version of RELEASES) {
        const player = new native.NativePlayer(version, walker({ tickCount: 41, remainingFireTicks: 30, ticksFrozen: 12 }), {});
        player.tick(world, packed(player.state(), true), NO_ENTITIES);
        const once = player.state();
        sameBits(player.state(), once, `${version}: a read changes nothing`);
        const copy = new native.NativePlayer(version, walker(), {});
        copy.setState(once);
        sameBits(copy.state(), once, `${version}: setState round trip`);
        assert.equal(once.tickCount, 42, `${version}: one tick is one count`);
    }
});

it('a legal zero stays a zero and an absent optional stays absent', () => {
    const world = floorWorld();
    const zeroes = walker({ gravity: 0, stepHeight: 0, jumpStrength: 0, movementSpeedValue: 0, safeFallDistance: 0,
        fallDamageMultiplier: 0, airDragModifier: 0, frictionModifier: 0, burningTime: 0 });
    const player = new native.NativePlayer('26.3', zeroes, {});
    const state = player.state();
    for (const key of ['gravity', 'stepHeight', 'jumpStrength', 'movementSpeedValue', 'safeFallDistance',
        'fallDamageMultiplier', 'airDragModifier', 'frictionModifier', 'burningTime']) {
        assert.strictEqual(state[key], 0, key);
    }
    const plain = new native.NativePlayer('26.3', walker(), {}).state();
    for (const key of ['gravity', 'stepHeight', 'jumpStrength', 'movementSpeedValue', 'burningTime']) {
        assert.equal(plain[key] ?? null, null, `${key} absent`);
    }
    // a zero jump strength does not jump from 1.20.5, whatever the default would have done
    const after = native.simulateVersionedTick('26.3', world, { ...zeroes, input: { x: 0, y: 0, z: 0 }, jumpRequested: true }, {});
    assert.ok(after.velocity.y <= 0, 'no jump at strength 0');
});

it('the packed layout is versioned: a stale array or a stale flag is refused and changes nothing', () => {
    assert.deepEqual(native.nativePlayerLayout(), NATIVE_PLAYER_LAYOUT);
    assert.equal(NATIVE_PLAYER_LAYOUT.inputLength, In.LENGTH);
    assert.equal(NATIVE_PLAYER_LAYOUT.outputLength, Out.LENGTH);
    const world = floorWorld();
    const player = new native.NativePlayer('26.3', walker(), {});
    const before = player.state();
    assert.throws(() => player.tick(world, new Float64Array(In.LENGTH - 1), NO_ENTITIES), /input array holds/);
    const unknown = packed(before, true);
    unknown[In.FLAGS] = 2 ** 30;
    assert.throws(() => player.tick(world, unknown, NO_ENTITIES), /does not know/);
    assert.throws(() => player.tick(world, packed(before, true), new Float64Array(5)), /whole number of entities/);
    sameBits(player.state(), before, 'a refused tick changes nothing');
});

it('a checkpoint midway reproduces the rest of the run on the object, persistent and batch paths', () => {
    for (const version of SUPPORTED_VERSIONS) {
        const world = floorWorld();
        let object = walker({ remainingFireTicks: 40, velocity: { x: -0, y: 0, z: 0 }, nearbyEntities: [], yawDegrees: -90, sprintModifier: true });
        const history = [];
        for (let tick = 0; tick < 40; tick++) {
            object = native.simulateVersionedTickOutcome(version, world, { ...object, ...keysAt(tick) }, {}).state;
            history.push(object);
        }
        const middle = history[19];
        const text = encodeCheckpoint(native, { version, state: middle, world, config: {} });
        const restored = decodeCheckpoint(native, JSON.parse(JSON.stringify(text)), version);
        assert.equal(restored.world.contentDigest(), world.contentDigest(), version);
        let resumed = restored.state;
        for (let tick = 20; tick < 40; tick++) {
            resumed = native.simulateVersionedTickOutcome(version, restored.world, { ...resumed, ...keysAt(tick) }, restored.config).state;
            sameBits(resumed, history[tick], `${version} resumed tick ${tick}`);
        }
        // persistent: from the same checkpoint, a NativePlayer in the restored world
        const again = decodeCheckpoint(native, text, version);
        const player = new native.NativePlayer(version, again.state, {});
        let state = player.state();
        for (let tick = 20; tick < 40; tick++) {
            const out = player.tick(again.world, packed({ ...state, ...keysAt(tick) }, true), NO_ENTITIES);
            state = player.state();
            sameBits([out[Out.POSITION], out[Out.POSITION + 1], out[Out.POSITION + 2]],
                [history[tick].position.x, history[tick].position.y, history[tick].position.z], `${version} persistent ${tick}`);
        }
        // batch: constant keys from the checkpoint against the same keys stepped
        const third = decodeCheckpoint(native, text, version);
        const held = { ...third.state, input: { x: 0, y: 0, z: 1 }, jumpRequested: false };
        const [batched] = native.simulateVersionedTicks(version, third.world, [held], 10, {});
        let stepped = held;
        for (let tick = 0; tick < 10; tick++) stepped = native.simulateVersionedTickOutcome(version, third.world, stepped, {}).state;
        sameBits(batched.position, stepped.position, `${version} batch`);
    }
});

it('a checkpoint keeps what JSON would lose and is refused when it does not fit', () => {
    const world = floorWorld();
    const state = walker({ velocity: { x: -0, y: 0, z: 0 }, gravity: -0, fallDistance: 0.1 + 0.2 });
    const text = encodeCheckpoint(native, { version: '26.3', state, world });
    const restored = decodeCheckpoint(native, text, '26.3');
    assert.ok(Object.is(restored.state.velocity.x, -0));
    assert.ok(Object.is(restored.state.gravity, -0));
    assert.equal(restored.state.fallDistance, 0.1 + 0.2);
    assert.ok(!('stepHeight' in restored.state), 'an absent optional stays absent');
    assert.throws(() => decodeCheckpoint(native, text, '26.2'), /of release 26.3, not 26.2/);
    const data = JSON.parse(text);
    assert.throws(() => decodeCheckpoint(native, JSON.stringify({ ...data, schema: 99 }), '26.3'), /schema 99/);
    assert.throws(() => decodeCheckpoint(native, JSON.stringify({ ...data, world: undefined }), '26.3'), /no world/);
    assert.throws(() => decodeCheckpoint(native, JSON.stringify({ ...data, state: undefined }), '26.3'), /no state/);
    assert.throws(() => decodeCheckpoint(native, JSON.stringify({ ...data, worldDigest: '0000000000000000' }), '26.3'), /not the one/);
    assert.throws(() => decodeCheckpoint(native, JSON.stringify({ ...data, layout: { version: 1 } }), '26.3'), /layout/);
    const broken = Buffer.from(data.world, 'base64');
    assert.throws(() => native.nativeWorldFromCheckpoint(broken.subarray(0, broken.length - 3)), /ends early/);
    assert.throws(() => native.nativeWorldFromCheckpoint(Buffer.from('nonsense!')), /not a world checkpoint/);
});

it('a world through its checkpoint answers every channel, and equal worlds give equal bytes', () => {
    const world = new native.NativeWorld();
    world.applyBlockUpdates([
        {
            x: -3, y: -60, z: 1e6, id: 7, stateVersion: '26.3', blockEntity: 'minecraft:chest',
            properties: [{ name: 'facing', value: 'north' }], tags: 3, fluid: 1, fluidLevel: 3, fluidSource: false,
            fluidFlow: { x: 0.5, y: 0, z: -0.5 }, fluidFalling: true, fluidTags: 1, waterBlock: true,
            shape: { minX: 0.0625, minY: 0, minZ: 0.0625, maxX: 0.9375, maxY: 0.875, maxZ: 0.9375, slipperiness: 0.6, effect: 5 },
            outlineShape: { minX: 0, minY: 0, minZ: 0, maxX: 1, maxY: 1, maxZ: 1, slipperiness: 0.6 },
            interactionShape: { minX: 0.25, minY: 0, minZ: 0.25, maxX: 0.75, maxY: 0.5, maxZ: 0.75, slipperiness: 0.6 },
            bounceRestitution: 0.75, suppressesBounce: true, fallDamageResetting: true, suffocating: false, inWall: true, bottomFaceSturdy: false,
        },
    ]);
    world.setWorldBounds(-64, 320, 8, -8, 1000);
    world.setChunkLoaded(5, 5, false);
    const copy = native.nativeWorldFromCheckpoint(world.checkpoint());
    assert.ok(world.checkpoint().equals(copy.checkpoint()), 'the bytes come back');
    assert.equal(copy.contentDigest(), world.contentDigest());
    assert.deepEqual(copy.worldBounds(), world.worldBounds());
    assert.equal(copy.isChunkLoaded(5, 5), false);
    assert.deepEqual(copy.blockStateAt?.(-3, -60, 1e6), world.blockStateAt?.(-3, -60, 1e6));
    world.setChunkLoaded(5, 5, true);
    assert.notEqual(copy.contentDigest(), world.contentDigest(), 'a change is another digest');
});

it('the field matrix names every public state field and no other', () => {
    const source = readFileSync(join(__dirname, '../src/native.ts'), 'utf8');
    const body = source.slice(source.indexOf('export interface NativeSimulationState'));
    const fields = body.slice(0, body.indexOf('\n}')).match(/^ {4}[a-zA-Z]+(?=\??:)/gm).map(name => name.trim());
    const doc = readFileSync(join(__dirname, '../docs/state-contract.md'), 'utf8');
    const named = [...doc.matchAll(/^\| `([a-zA-Z]+)`/gm)].map(match => match[1]);
    assert.deepEqual([...new Set(named)].sort(), [...new Set(fields)].sort());
    assert.equal(new Set(named).size, named.length, 'each field has one row');
});
