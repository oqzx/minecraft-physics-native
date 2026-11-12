'use strict';

// What changes between ticks, through the object and packed paths alike, and the other families' checkpoints:
// docs/state-contract.md, docs/field-maps.md, docs/checkpoints.md.

const assert = require('node:assert/strict');
const { it } = require('node:test');
const { readFileSync } = require('node:fs');
const { join } = require('node:path');
const {
    LoadNativePhysics, NativePlayerInput: In, NativePlayerInputFlag: InFlag, NativePlayerOutput: Out,
    SUPPORTED_VERSIONS, encodeCheckpoint, decodeCheckpoint,
} = require('..');

const native = LoadNativePhysics();
const RELEASES = ['1.12.2', '1.16.5', '1.20.4', '1.21.5', '26.3'];
const NONE = new Float64Array(0);

function floor(slab = true) {
    const world = new native.NativeWorld();
    const updates = [];
    for (let x = -6; x < 24; x++) {
        for (let z = -6; z < 8; z++) {
            updates.push({ x, y: 0, z, id: 1, shape: { minX: 0, minY: 0, minZ: 0, maxX: 1, maxY: 1, maxZ: 1, slipperiness: 0.6 } });
        }
    }
    if (slab) updates.push({ x: 4, y: 1, z: 0, id: 2, shape: { minX: 0, minY: 0, minZ: 0, maxX: 1, maxY: 0.5, maxZ: 1, slipperiness: 0.6 } });
    world.applyBlockUpdates(updates);
    return world;
}

const walker = (fields = {}) => ({
    position: { x: 0.5, y: 1, z: 0.5 }, velocity: { x: 0, y: 0, z: 0 }, halfWidth: 0.3, height: 1.8, eyeHeight: 1.62,
    onGround: true, collidedHorizontally: false, collidedVertically: true, verticalCollisionBelow: true, isPlayer: true,
    controlledByLocalInstance: true, clientSide: true, input: { x: 0, y: 0, z: 1 }, yaw: -Math.PI / 2, yawDegrees: -90,
    look: { x: 1, y: 0, z: 0 }, sprintModifier: false, movingSlowly: false, movementSpeed: 0.10000000149011612,
    flyingSpeed: 0.05, safeFallDistance: 3, itemUseSpeedMultiplier: 0.2, ...fields,
});

function packed(state, unmoved) {
    const values = new Float64Array(In.LENGTH);
    values.set([state.position.x, state.position.y, state.position.z], In.POSITION);
    values.set([state.velocity.x, state.velocity.y, state.velocity.z], In.VELOCITY);
    values.set([state.look.x, state.look.y, state.look.z], In.LOOK);
    values[In.YAW] = state.yaw;
    values[In.YAW_DEGREES] = state.yawDegrees ?? -90;
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
    values[In.ITEM_USE_SPEED_MULTIPLIER] = state.itemUseSpeedMultiplier;
    const bit = (set, flag) => (set ? flag : 0);
    values[In.FLAGS] = bit(state.sprinting, InFlag.SPRINTING) | bit(state.wasSprinting, InFlag.WAS_SPRINTING)
        | bit(state.onGround, InFlag.ON_GROUND) | bit(state.collidedHorizontally, InFlag.COLLIDED_HORIZONTALLY)
        | bit(state.minorHorizontalCollision, InFlag.MINOR_HORIZONTAL_COLLISION)
        | bit(state.collidedVertically, InFlag.COLLIDED_VERTICALLY)
        | bit(state.verticalCollisionBelow, InFlag.VERTICAL_COLLISION_BELOW) | bit(state.inWater, InFlag.IN_WATER)
        | bit(state.inLava, InFlag.IN_LAVA) | bit(state.jumpRequested, InFlag.JUMP_REQUESTED)
        | bit(unmoved, InFlag.UNMOVED) | bit(state.sprintModifier, InFlag.SPRINT_MODIFIER) | bit(state.removed, InFlag.REMOVED)
        | bit(state.dead, InFlag.DEAD) | bit(state.noPhysics, InFlag.NO_PHYSICS) | bit(state.movingSlowly, InFlag.MOVING_SLOWLY);
    return values;
}

const outcome = (version, world, state) => native.simulateVersionedTickOutcome(version, world, state, {}).state;
const text = value => JSON.stringify(value, (key, entry) => (Object.is(entry, -0) ? '-0' : Number.isNaN(entry) ? 'NaN' : entry));

/** One run through both paths, `change(tick)` giving what the caller sets before that tick. */
function bothPaths(version, world, start, ticks, change) {
    let object = start;
    const player = new native.NativePlayer(version, start, {});
    for (let tick = 0; tick < ticks; tick++) {
        const set = change(tick);
        object = outcome(version, world, { ...object, ...set });
        // packed: what is no array field goes through setState, as the adapter's rare inputs do
        const kept = player.state();
        if (Object.keys(set).length) player.setState({ ...kept, ...set });
        const out = player.tick(world, packed({ ...kept, ...set }, true), NONE);
        const where = `${version} tick ${tick}`;
        assert.equal(text([...out.subarray(Out.POSITION, Out.POSITION + 6)]),
            text([object.position.x, object.position.y, object.position.z, object.velocity.x, object.velocity.y, object.velocity.z]), where);
        assert.equal(out[Out.FALL_DISTANCE], object.fallDistance ?? 0, where);
        assert.equal(text(player.state().supportingBlock ?? null), text(object.supportingBlock ?? null), `${where} support`);
    }
    return object;
}

it('an effect that ends ends the same on both paths, and its fallback goes with it', () => {
    for (const version of RELEASES) {
        const world = floor();
        const on = { levitation: true, levitationAmplifier: 1, slowFalling: true };
        const off = { levitation: false, levitationAmplifier: undefined, slowFalling: false };
        bothPaths(version, world, walker({ input: { x: 0, y: 0, z: 0 } }), 16, tick => (tick === 3 ? on : tick === 9 ? off : {}));
    }
});

it('getting on and off a seat goes the same on both paths', () => {
    for (const version of RELEASES) {
        const world = floor();
        const seat = { x: 1.5, y: 1.5, z: 0.5 };
        const last = bothPaths(version, world, walker(), 18, (tick) => {
            if (tick >= 5 && tick < 11) return { passenger: true, position: seat };
            return tick === 11 ? { passenger: false } : {};
        });
        assert.equal(last.passenger ?? false, false, version);
    }
});

it('removed, dead, passenger, noPhysics and authority combine the same on both paths, and each exit holds', () => {
    for (const version of RELEASES) {
        for (let bits = 0; bits < 32; bits++) {
            const flags = {
                removed: !!(bits & 1), dead: !!(bits & 2), passenger: !!(bits & 4), noPhysics: !!(bits & 8), clientSide: !!(bits & 16),
            };
            const world = floor(false);
            const start = walker({ ...flags, position: { x: 0.5, y: 3, z: 0.5 }, onGround: false, verticalCollisionBelow: false, collidedVertically: false, velocity: { x: 0.1, y: 0, z: 0 } });
            const last = bothPaths(version, world, start, 3, () => ({}));
            const where = `${version} ${JSON.stringify(flags)}`;
            // a removed entity does not tick from 1.19.1 (ClientLevel's tickNonPassenger never reaches it)
            if (flags.removed && !['1.12.2', '1.16.5'].includes(version)) assert.deepEqual(last.position, start.position, where);
            // a seat is where the caller says, whatever the tick did
            if (flags.passenger) assert.deepEqual(last.position, start.position, where);
            assert.equal(last.tickCount, 3, `${where}: each admitted tick counts once`);
        }
    }
});

it('every movement flag comes back from setState and stays through an ordinary tick', () => {
    const flags = ['sneaking', 'swimming', 'flying', 'climbing', 'noGravity', 'noPhysics', 'sleeping', 'dead', 'invulnerable',
        'fireImmune', 'fireResistance', 'onFireFlag', 'inRain', 'fallDamageImmune', 'frostWalker', 'powderSnowImmune',
        'powderSnowWalkable', 'sweetBerryBushImmune', 'nonLiving', 'weaving', 'ultraWarm', 'clientSide', 'isPlayer'];
    const world = floor(false);
    for (const flag of flags) {
        const player = new native.NativePlayer('26.3', walker({ [flag]: true }), {});
        assert.equal(player.state()[flag], true, flag);
        player.tick(world, packed(player.state(), true), NONE);
        const after = player.state();
        // the packed input says these itself, and this array leaves them off
        if (!['sneaking', 'swimming', 'flying', 'sleeping', 'dead', 'noPhysics'].includes(flag)) assert.equal(after[flag], true, `${flag} after a tick`);
    }
});

it('a turn or a resize leaves motion, support and the counters alone', () => {
    const world = floor(false);
    for (const version of RELEASES) {
        const moving = outcome(version, world, walker({ tickCount: 9, remainingFireTicks: 30 }));
        const unrelated = state => text([state.velocity, state.supportingBlock, state.tickCount, state.fallDistance, state.onGround, state.jumpTicks, state.remainingFireTicks, state.position]);
        const player = new native.NativePlayer(version, moving, {});
        const before = unrelated(player.state());
        for (const change of [{ yaw: 1.25, pitch: -0.5, yawDegrees: 71.6 }, { halfWidth: 0.25, height: 1.5, eyeHeight: 1.27 }]) {
            player.setState({ ...player.state(), ...change });
            assert.equal(unrelated(player.state()), before, version);
        }
    }
});

it('death clears the input and lets the counters run: the next tick has no jump and no walk', () => {
    const world = floor(false);
    for (const version of RELEASES) {
        const alive = outcome(version, world, walker({ jumpRequested: true }));
        assert.ok(alive.velocity.y > 0, `${version} jumps`);
        const dead = outcome(version, world, walker({ jumpRequested: true, dead: true, tickCount: 4 }));
        assert.equal(dead.tickCount, 5, version);
        assert.ok(dead.velocity.y <= 0, `${version}: LivingEntity.aiStep clears jumping of an immobile entity`);
        assert.equal(dead.position.x, 0.5, `${version}: and xxa, zza`);
    }
});

it('a boat, a ridden mob and a minecart count once a tick: a batch is its ticks', () => {
    const world = floor(false);
    const boat = { position: { x: 0.5, y: 1, z: 0.5 }, velocity: { x: 0, y: 0, z: 0 }, yaw: 0, forward: true, left: true, right: false, inWater: false };
    let stepped = boat;
    for (let tick = 0; tick < 12; tick++) stepped = native.simulateVersionedBoatTick('1.21.5', world, stepped);
    assert.equal(text(native.simulateVersionedBoatTicks('1.21.5', world, [boat], 12)[0]), text(stepped));
    const horse = { position: { x: 0.5, y: 1, z: 0.5 }, velocity: { x: 0, y: 0, z: 0 }, yaw: 0, forward: 1, strafe: 0, jumpRequested: false, onGround: true, inWater: false };
    let ridden = horse;
    for (let tick = 0; tick < 12; tick++) ridden = native.simulateVersionedHorseTick('1.21.5', world, ridden);
    assert.equal(text(native.simulateVersionedHorseTicks('1.21.5', world, [horse], 12)[0]), text(ridden));
});

it('attributes reach what they drive, at zero and past it, on both paths', () => {
    const world = floor();
    const up = (fields, ticks = 6) => bothPaths('26.3', world, walker({ input: { x: 0, y: 0, z: 0 }, position: { x: 0.5, y: 3, z: 0.5 }, onGround: false, collidedVertically: false, verticalCollisionBelow: false, ...fields }), ticks, () => ({}));
    assert.equal(up({ gravity: 0 }).position.y, 3, 'zero gravity holds it');
    assert.ok(up({ gravity: -0.08 }).position.y > 3, 'a negative gravity lifts it');
    assert.ok(up({ gravity: 0.08 }).position.y < 3, 'a positive one drops it');
    // (settled: a zero-y move on the first tick leaves the ground and the air's own 0.02 would carry it, oracle_move_tests.rs)
    const settled = { x: 0, y: -0.0784000015258789, z: 0 };
    const walk = fields => bothPaths('26.3', world, walker({ velocity: settled, ...fields }), 40, () => ({}));
    assert.equal(walk({ movementSpeedValue: 0 }).position.x, 0.5, 'zero speed stays');
    assert.ok(walk({ stepHeight: 0 }).position.y === 1, 'zero step stops at the slab');
    assert.ok(walk({ stepHeight: 0 }).position.x < 3.8, 'and against it');
    let highest = 0;
    let climbing = walker({ velocity: settled, stepHeight: 0.6 });
    for (let tick = 0; tick < 60; tick++) {
        climbing = outcome('26.3', world, climbing);
        highest = Math.max(highest, climbing.position.y);
    }
    assert.equal(highest, 1.5, 'the slab is taken at 0.6');
    const jump = strength => outcome('26.3', world, walker({ jumpStrength: strength, jumpRequested: true, input: { x: 0, y: 0, z: 0 } })).velocity.y;
    assert.ok(jump(0) <= 0, 'no jump at zero');
    assert.ok(jump(1) > jump(0.42), 'stronger is higher');
    const fall = (safe) => {
        const landing = { input: { x: 0, y: 0, z: 0 }, position: { x: 0.5, y: 1.01, z: 0.5 }, velocity: { x: 0, y: -1, z: 0 }, onGround: false, collidedVertically: false, verticalCollisionBelow: false, fallDistance: 6, safeFallDistance: safe };
        return outcome('26.3', world, walker(landing)).fallDamage ?? 0;
    };
    assert.ok(fall(-2) > fall(3), 'a negative safe fall distance hurts sooner');
    assert.equal(fall(100), 0, 'a large one does not');
});

it('landings and contact damage read the same on the object and persistent paths, once each', () => {
    const world = floor(false);
    world.applyBlockUpdates([{ x: 2, y: 1, z: 2, id: 9, shape: { minX: 0.0625, minY: 0, minZ: 0.0625, maxX: 0.9375, maxY: 1, maxZ: 0.9375, slipperiness: 0.6, effect: 12 } }]);
    const start = walker({ clientSide: false, input: { x: 0, y: 0, z: 0 }, position: { x: 2.5, y: 2.5, z: 1.32 }, onGround: false, collidedVertically: false, verticalCollisionBelow: false, fallDistance: 9, velocity: { x: 0, y: -1, z: 0 } });
    let object = start;
    const player = new native.NativePlayer('26.3', start, {});
    let landed = 0;
    for (let tick = 0; tick < 6; tick++) {
        object = outcome('26.3', world, object);
        const kept = player.state();
        const out = player.tick(world, packed(kept, true), NONE);
        assert.equal(out[Out.FALL_DAMAGE], object.fallDamage ?? 0, `tick ${tick} fall`);
        assert.equal(out[Out.CONTACT_DAMAGE], object.contactDamage ?? 0, `tick ${tick} contact`);
        landed += out[Out.FALL_DAMAGE] > 0 ? 1 : 0;
        assert.equal(text(player.state()), text(player.state()), 'a read replays nothing');
    }
    assert.equal(landed, 1, 'one landing, one event');
});

const FAMILIES = [
    ['boat', 'simulateVersionedBoatTick', undefined, { position: { x: 0.5, y: 1, z: 0.5 }, velocity: { x: 0.1, y: 0, z: -0 }, yaw: 0, forward: true, left: false, right: false, inWater: false }],
    ['horse', 'simulateVersionedHorseTick', undefined, { position: { x: 0.5, y: 1, z: 0.5 }, velocity: { x: 0, y: 0, z: 0 }, yaw: 0, forward: 1, strafe: 0, jumpRequested: false, onGround: true, inWater: false }],
    ['minecart', 'simulateVersionedMinecartTick', { improvements: false }, { variant: 'minecart', position: { x: 0.5, y: 1, z: 0.5 }, velocity: { x: 0.2, y: 0, z: 0 }, yRot: 0, flipped: false, onGround: true, inWater: false }],
];

it('a boat, a ridden mob and a cart go on from a checkpoint by their bits, and a mixed-up kind is refused', () => {
    for (const version of SUPPORTED_VERSIONS) {
        for (const [kind, call, rules, start] of FAMILIES) {
            const world = floor(false);
            const history = [];
            let state = start;
            const step = (from, target) => {
                const after = rules ? native[call](version, target, from, rules) : native[call](version, target, from);
                return after.state ?? after;
            };
            for (let tick = 0; tick < 16; tick++) {
                state = step(state, world);
                history.push(state);
            }
            const text1 = encodeCheckpoint(native, { version, kind, state: history[7], world });
            const restored = decodeCheckpoint(native, text1, version, kind);
            let resumed = restored.state;
            for (let tick = 8; tick < 16; tick++) {
                resumed = step(resumed, restored.world);
                assert.equal(text(resumed), text(history[tick]), `${version} ${kind} tick ${tick}`);
            }
            assert.throws(() => decodeCheckpoint(native, text1, version, 'player'), new RegExp(`a ${kind} checkpoint`));
        }
    }
});

const BATCHED = [
    ['projectile', 'simulateVersionedProjectileTicks', { kind: 'snowball', position: { x: 0.5, y: 40, z: 0.5 }, velocity: { x: 0.3, y: 0.2, z: -0.1 } }],
    ['arrow', 'simulateVersionedArrowTicks', { position: { x: 0.5, y: 40, z: 0.5 }, velocity: { x: 0.3, y: 0.2, z: -0.1 } }],
    ['trident', 'simulateVersionedTridentTicks', { position: { x: 0.5, y: 40, z: 0.5 }, velocity: { x: 0.3, y: 0.2, z: -0.1 } }],
];

it('a projectile, an arrow and a trident go on from a checkpoint by their bits', () => {
    for (const release of SUPPORTED_VERSIONS) {
        for (const [kind, call, start] of BATCHED) {
            const world = floor(false);
            // (a trident is of 1.13: a release before refuses it by name)
            if (kind === 'trident' && /^1\.([89]|1[0-2])(\.|$)/.test(release)) {
                assert.throws(() => native[call](release, world, [start], 1), /not supported before 1\.13/);
                continue;
            }
            const step = (from, target) => ({ ...from, ...native[call](release, target, [from], 1)[0] });
            const history = [];
            let state = start;
            for (let tick = 0; tick < 24; tick++) {
                state = step(state, world);
                history.push(state);
            }
            const restored = decodeCheckpoint(native, encodeCheckpoint(native, { version: release, kind, state: history[9], world }), release, kind);
            assert.equal(text(restored.state), text(history[9]), `${kind}: the state itself`);
            let resumed = restored.state;
            for (let tick = 10; tick < 24; tick++) {
                resumed = step(resumed, restored.world);
                assert.equal(text(resumed), text(history[tick]), `${release} ${kind} tick ${tick}`);
            }
            assert.notEqual(text(history[23].position), text(history[9].position), `${kind} moved`);
        }
    }
});

it('a moving entity goes on from a checkpoint by its bits, its random draws with it', () => {
    const world = floor(false);
    for (const version of SUPPORTED_VERSIONS) for (const [kind, extra] of [
        ['item', { age: 3, pickupDelay: 10 }], ['tnt', { fuse: 80 }], ['experience_orb', { age: 3 }],
    ]) {
        const start = { kind, position: { x: 0.5, y: 6, z: 0.5 }, velocity: { x: 0.05, y: 0.1, z: -0.0 }, onGround: false, inWater: false, ...extra };
        const inputs = { random: [0.25, 0.75, 0.5, 0.125] };
        const step = (from, target) => native.simulateVersionedMovingEntityTick(version, target, from, inputs).state;
        const history = [];
        let state = start;
        for (let tick = 0; tick < 20; tick++) {
            state = step(state, world);
            history.push(state);
        }
        const restored = decodeCheckpoint(native, encodeCheckpoint(native, { version, kind: 'moving_entity', state: history[8], world }), version, 'moving_entity');
        let resumed = restored.state;
        for (let tick = 9; tick < 20; tick++) {
            resumed = step(resumed, restored.world);
            assert.equal(text(resumed), text(history[tick]), `${version} ${kind} tick ${tick}`);
        }
    }
});

it('a family keeps a zero, a fraction and an absence, and refuses what it cannot carry', () => {
    const world = floor(false);
    const horse = { position: { x: 0.5, y: 1, z: 0.5 }, velocity: { x: 0, y: 0, z: 0 }, yaw: 0, forward: 1, strafe: 0, jumpRequested: false, onGround: true, inWater: false };
    const zero = native.simulateVersionedHorseTick('1.21.5', world, { ...horse, movementSpeed: 0, jumpStrength: 0, halfWidth: Math.fround(1 / 3), height: 1.6000000238418579 });
    assert.strictEqual(zero.movementSpeed, 0);
    assert.strictEqual(zero.jumpStrength, 0);
    assert.strictEqual(zero.halfWidth, Math.fround(1 / 3), 'a size is a float in vanilla');
    assert.strictEqual(zero.height, 1.6000000238418579);
    assert.equal(zero.position.z, 0.5, 'a zero speed does not walk');
    assert.equal(native.simulateVersionedHorseTick('1.21.5', world, horse).jumpStrength, Math.fround(0.42), 'absent is the mob base, never a zero');
    const boat = { position: { x: 0.5, y: 1, z: 0.5 }, velocity: { x: 0, y: 0, z: 0 }, yaw: 0, forward: false, left: false, right: false, inWater: false };
    assert.throws(() => native.simulateVersionedBoatTick('1.21.5', world, { ...boat, previousStatus: 'on_the_moon' }));
    assert.throws(() => native.simulateVersionedHorseTick('1.21.5', world, { ...horse, entityType: 'cow' }));
    assert.throws(() => native.simulateVersionedMovingEntityTick('1.21.5', world, { kind: 'cow', position: { x: 0, y: 0, z: 0 }, velocity: { x: 0, y: 0, z: 0 } }, {}));
    assert.throws(() => native.simulateVersionedMinecartTick('1.21.5', world, { variant: 'nonsense', position: { x: 0, y: 0, z: 0 }, velocity: { x: 0, y: 0, z: 0 }, yRot: 0, flipped: false, onGround: false, inWater: false }, { improvements: false }));
});

it('the adapter sizes a pose as the jar does, by the bits', () => {
    const adapter = require('..');
    const ordinal = { STANDING: 0, FALL_FLYING: 1, SLEEPING: 2, SWIMMING: 3, SPIN_ATTACK: 4, CROUCHING: 5, LONG_JUMPING: 6, DYING: 7 };
    let seen = 0;
    for (const release of ['1.18.2', '1.20.4', '1.21.5', '26.3']) {
        const physics = new adapter.NativePhysics(require('prismarine-registry')(release));
        const fixture = readFileSync(join(__dirname, `../native/fixtures/java-oracle-living-${release}.txt`), 'utf8');
        for (const line of fixture.split(/\r?\n/).filter(entry => entry.startsWith('pose '))) {
            const [, name, width, height] = line.split(' ');
            if (!(name in ordinal)) continue;
            assert.deepEqual(physics.poseSize(ordinal[name], 1), { width: Number(width), height: Number(height) }, `${release} ${name}`);
            seen += 1;
        }
    }
    assert.ok(seen >= 24, `${seen} poses`);
});

it('the field maps say what a real tick hands back, both ways, as docs/field-maps.md renders it', () => {
    const { TICKS, measureKept } = require('../scripts/field-kept.cjs');
    const source = readFileSync(join(__dirname, '../src/native.ts'), 'utf8');
    const members = {};
    for (const name of Object.keys(TICKS)) {
        const body = source.slice(source.indexOf(`export interface ${name} `));
        members[name] = body.slice(0, body.indexOf('\n}')).match(/^ {4}[a-zA-Z]+(?=\??:)/gm).map(field => field.trim());
    }
    const notKept = measureKept(native, members);
    const doc = readFileSync(join(__dirname, '../docs/field-maps.md'), 'utf8').replace(/\r\n/g, '\n');
    for (const [name, { seed, tick }] of Object.entries(TICKS)) {
        const output = tick(native, seed);
        const section = doc.slice(doc.indexOf(`## ${name}\n`));
        for (const field of members[name]) {
            const back = output[field] !== undefined && output[field] !== null;
            assert.equal(notKept[name].includes(field), !back, `${name}.${field}`);
            const row = section.split('\n').find(line => line.startsWith(`| \`${field}\` |`));
            assert.ok(row?.includes(back ? '| yes |' : '| no |'), `${name}.${field} says ${back ? 'yes' : 'no'}: ${row}`);
        }
        // a state's place and motion always come back
        assert.ok(!notKept[name].includes('position') && !notKept[name].includes('velocity'), name);
    }
    assert.ok(notKept.NativeBoatState.includes('nearbyEntities'));
});

it('the field maps name every field of every public state', () => {
    const source = readFileSync(join(__dirname, '../src/native.ts'), 'utf8');
    const doc = readFileSync(join(__dirname, '../docs/field-maps.md'), 'utf8').replace(/\r\n/g, '\n');
    for (const name of ['NativeBoatState', 'NativeHorseState', 'NativeMinecartState', 'NativeMovingEntityState', 'NativeProjectileState',
        'NativeArrowState', 'NativeTridentState', 'NativeBlockUpdate', 'NativeBlockShape', 'NativeMovingPiston', 'NativeNearbyEntity']) {
        const body = source.slice(source.indexOf(`export interface ${name} `));
        const fields = body.slice(0, body.indexOf('\n}')).match(/^ {4}[a-zA-Z]+(?=\??:)/gm).map(field => field.trim());
        const section = doc.slice(doc.indexOf(`## ${name}\n`));
        assert.ok(doc.includes(`## ${name}\n`), name);
        const table = section.slice(0, section.indexOf('\n## ', 4) === -1 ? undefined : section.indexOf('\n## ', 4));
        assert.deepEqual([...table.matchAll(/^\| `([a-zA-Z]+)`/gm)].map(match => match[1]).sort(), [...fields].sort(), name);
    }
});
