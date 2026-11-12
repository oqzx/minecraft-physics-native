'use strict';

// B209: every public input keeps what the caller sent, at the edges of its type. A whole number past what N-API would carry is
// refused by name, a double comes back as the double or as the float the binding says it narrows to, legal zero and absence
// stay apart as the binding's readers say, and the same complete input gives the same result through the object, packed and
// adapter paths of each family.

const assert = require('node:assert/strict');
const { Vec3 } = require('vec3');
const { describe, it } = require('node:test');
const physics = require('..');
const { botOf } = require('./bot-fixture.cjs');
const { readers, rustTypes } = require('../scripts/field-facts.cjs');
const { STATES } = require('../scripts/state-roundtrip.cjs');
const { compareReleases, registeredReleases } = require('../scripts/registered-releases.cjs');
const { render: renderWhole } = require('../scripts/generate-whole-fields.cjs');
const { WHOLE_FUNCTIONS, WHOLE_INTERFACES, WHOLE_METHODS } = require('../dist/whole-fields.js');
const { guardWholeNumbers } = require('../dist/whole-numbers.js');
const fs = require('node:fs');
const path = require('node:path');

const native = physics.LoadNativePhysics();
const RELEASES = registeredReleases().map(({ version }) => version);
const adapter = require('../dist/mineflayer');
const types = rustTypes();
const read = readers(types);
const world = () => new native.NativeWorld();
/** A world whose chunk at the origin is loaded, as a synced one is: before 1.14 a client's tick in an unloaded chunk does not fall. */
const loaded = () => {
    const made = world();
    made.setChunkLoaded(0, 0, true);
    return made;
};
/** The adapter takes the registry of every release, each its own from minecraft-data (1.19.1, 1.21.2, 1.21.7, 1.21.10,
 * 26.1.1 and 26.1.2 included, B302): a release it refused would fail its tests below, not drop out of them. */
const ADAPTED = RELEASES;
const bare = type => type.replace(/^Option<(.*)>$/, '$1');
const LIMITS = { u8: [0, 255], u16: [0, 65535], u32: [0, 4294967295], i32: [-2147483648, 2147483647], i64: [-(2 ** 53) + 1, 2 ** 53 - 1] };
/** Angles a tick wraps into -180..180: Mth.wrapDegrees of AbstractMinecart's move. */
const WRAPPED = { NativeMinecartState: ['yRot'] };
/** Attributes and sizes a state refuses below 0: their range is in the contract. */
const NOT_NEGATIVE = ['movementSpeed', 'jumpStrength', 'flyingSpeed', 'riddenSpeed'];
const DOUBLES = [0, -0, 5e-324, 0.1, 1 / 3, -1234.5678901234567, 1e300];

/** Calls stubbed in place of the addon, to reach every field without running a tick. */
const stubbed = () => {
    const calls = [];
    const binding = {};
    const record = name => (...args) => { calls.push(name); return args; };
    for (const name of Object.keys(WHOLE_FUNCTIONS)) binding[name] = record(name);
    for (const [owner, methods] of Object.entries(WHOLE_METHODS)) {
        binding[owner] = class { };
        for (const name of Object.keys(methods)) if (name !== 'constructor') binding[owner].prototype[name] = record(`${owner}.${name}`);
    }
    guardWholeNumbers(binding);
    return { binding, calls };
};

/** One value with `bad` at a path through a kind, and that path. */
function* placements(kind, bad, seen = new Set()) {
    if (typeof kind === 'string') {
        yield { value: bad, path: '' };
    } else if (kind.list) {
        for (const { value, path: at } of placements(kind.list, bad, seen)) yield { value: [value], path: `[0]${at}` };
    } else if (!seen.has(kind.of)) {
        const inner = new Set(seen).add(kind.of);
        for (const [field, fieldKind] of Object.entries(WHOLE_INTERFACES[kind.of])) {
            for (const { value, path: at } of placements(fieldKind, bad, inner)) yield { value: { [field]: value }, path: `.${field}${at}` };
        }
    }
}

const leafOf = kind => (typeof kind === 'string' ? kind : kind.list ? leafOf(kind.list) : null);

describe('whole numbers at the boundary', () => {
    it('is the table the binding says', () => {
        assert.equal(fs.readFileSync(path.join(__dirname, '../src/whole-fields.ts'), 'utf8').replace(/\r\n/g, '\n'), renderWhole());
    });

    it('holds every integer field of every struct', () => {
        for (const [name, fields] of Object.entries(types)) {
            for (const [field, type] of Object.entries(fields)) {
                if (LIMITS[bare(type)]) assert.equal(WHOLE_INTERFACES[name]?.[field], bare(type), `${name}.${field}`);
            }
        }
    });

    it('refuses a fraction, NaN, an infinity and what is past its type, by the field, before the addon is called', () => {
        const { binding, calls } = stubbed();
        let checked = 0;
        const refuse = (call, parameters, label) => {
            parameters.forEach((parameter, index) => {
                if (!parameter) return;
                const args = (bad) => [...placements(parameter.kind, bad)].map(({ value, path: at }) => {
                    const list = new Array(parameters.length).fill(undefined);
                    list[index] = value;
                    return { list, at };
                });
                for (const bad of [1.5, NaN, Infinity, -Infinity, 2 ** 64]) {
                    for (const { list, at } of args(bad)) {
                        assert.throws(() => call(...list), error => error instanceof RangeError && error.message.includes(`${parameter.name}${at} is `), `${label} ${parameter.name}${at} ${bad}`);
                        checked += 1;
                    }
                }
                for (const { list, at } of args(0)) {
                    const before = calls.length;
                    call(...list);
                    assert.equal(calls.length, before + 1, `${label} ${parameter.name}${at} takes 0`);
                }
            });
        };
        for (const [name, parameters] of Object.entries(WHOLE_FUNCTIONS)) refuse(binding[name], parameters, name);
        for (const [owner, methods] of Object.entries(WHOLE_METHODS)) {
            const instance = owner === 'NativePlayer' ? Object.create(binding[owner].prototype) : new binding[owner]();
            for (const [name, parameters] of Object.entries(methods)) {
                if (name !== 'constructor') refuse((...args) => instance[name](...args), parameters, `${owner}.${name}`);
            }
        }
        assert.ok(checked > 1000, `${checked} refusals`);
    });

    it('refuses each type at one past its ends and takes its ends', () => {
        for (const [kind, [low, high]] of Object.entries(LIMITS)) {
            const parameter = { name: 'value', kind };
            const binding = { call: value => value };
            const named = `whole_${kind}`;
            binding[named] = binding.call;
            const { WHOLE_FUNCTIONS: table } = require('../dist/whole-fields.js');
            const saved = table[named];
            table[named] = [parameter];
            try {
                guardWholeNumbers(binding);
                assert.equal(binding[named](low), low);
                assert.equal(binding[named](high), high);
                if (kind !== 'i64') {
                    assert.throws(() => binding[named](low - 1), RangeError, `${kind} under`);
                    assert.throws(() => binding[named](high + 1), RangeError, `${kind} over`);
                }
            } finally {
                if (saved === undefined) delete table[named];
                else table[named] = saved;
            }
        }
    });

    it('stops the real addon too: nothing changes and the message names the field', () => {
        const arrow = { position: { x: 0.5, y: 100, z: 0.5 }, velocity: { x: 0, y: 0, z: 0 } };
        assert.throws(() => native.simulateVersionedArrowTicks('26.3', world(), [{ ...arrow, shakeTime: 300 }], 1), /states\[0\]\.shakeTime is 300/);
        assert.throws(() => native.simulateVersionedArrowTicks('26.3', world(), [arrow], -1), /ticks is -1/);
        assert.throws(() => native.simulateVersionedArrowTicks('26.3', world(), [arrow], 1.5), /ticks is 1\.5/);
        const player = { position: arrow.position, velocity: arrow.velocity, halfWidth: 0.3, height: 1.8, onGround: false, collidedHorizontally: false, collidedVertically: false };
        assert.throws(() => native.simulateVersionedTick('26.3', world(), { ...player, tickCount: 1.5 }, {}), /state\.tickCount is 1\.5/);
        assert.throws(() => new native.NativePlayer('26.3', { ...player, ticksFrozen: -1 }, {}), /state\.ticksFrozen is -1/);
        assert.ok(new native.NativePlayer('26.3', player, {}) instanceof native.NativePlayer);
    });

    it('guards an addon the caller gives the adapter, as the one LoadNativePhysics loads', () => {
        const { binding, calls } = stubbed();
        binding.isSupportedVersion = () => true;
        physics.createNativePhysics(require('prismarine-registry')('26.3'), { binding });
        assert.throws(() => binding.simulateVersionedArrowTicks('26.3', {}, [], 1.5), /ticks is 1\.5/);
        const other = stubbed();
        new adapter.ShulkerBoxTracker(other.binding, '26.3');
        assert.throws(() => other.binding.simulateVersionedArrowTicks('26.3', {}, [], -1), /ticks is -1/);
        assert.equal(calls.length + other.calls.length, 0, 'the addon is never called');
    });

    it('refuses a seed that is no whole number within 2^53, which a double cannot hold', () => {
        const cart = { variant: 'tnt_minecart', position: { x: 0.5, y: 100, z: 0.5 }, velocity: { x: 0, y: 0, z: 0 }, yRot: 0, flipped: false, onGround: false, inWater: false };
        for (const seed of [1.5, -1, 2 ** 53, NaN]) {
            assert.throws(() => native.simulateVersionedMinecartTick('26.3', world(), { ...cart, randomSeed: seed }, { improvements: false }), /randomSeed/, `${seed}`);
        }
        native.simulateVersionedMinecartTick('26.3', world(), { ...cart, randomSeed: 2 ** 53 - 1 }, { improvements: false });
    });
});

describe('the values that come back', () => {
    for (const [kind, spec] of Object.entries(STATES)) {
        describe(kind, () => {
            const ticker = version => extra => {
                const made = spec.tick(native, world(), { ...spec.base, ...extra }, version);
                return made.state ?? made;
            };
            /** The releases the family runs on: a release without it refuses the base state. */
            const running = RELEASES.filter(version => {
                try {
                    ticker(version)({});
                    return true;
                } catch {
                    return false;
                }
            });
            const special = new Set([...spec.evolved, ...spec.config, ...spec.refused, ...spec.structured]);
            const fields = Object.keys(types[kind]).filter(field => !special.has(field));
            const recomputed = (version, field) => spec.evolvedUntil?.[field] !== undefined && compareReleases(version, spec.evolvedUntil[field]) <= 0;
            const narrowed = field => read[kind][field]?.narrowed;

            it('keeps each scalar exact on each release, or as the float the binding says it narrows to', () => {
                assert.ok(running.length > 0);
                for (const version of running) for (const field of fields) {
                    if (recomputed(version, field)) continue;
                    const tick = ticker(version);
                    const type = bare(types[kind][field]);
                    if (type === 'bool') {
                        for (const value of [true, false]) assert.equal(tick({ [field]: value })[field], value, `${version} ${field} ${value}`);
                    } else if (type === 'f64') {
                        for (const value of DOUBLES) {
                            const expected = narrowed(field) ? Math.fround(value) : value;
                            // what a state refuses whole is not lost: only a value that comes back differently would be
                            let output;
                            try {
                                output = tick({ [field]: value })[field];
                            } catch (error) {
                                assert.match(error.message, /invalid values|the tick's move/, `${version} ${field} ${value}`);
                                assert.ok(Math.abs(value) > 1e30 || (value < 0 && NOT_NEGATIVE.includes(field)), `${version} ${field} ${value} is an ordinary value`);
                                continue;
                            }
                            // a tick's own arithmetic may turn a -0 to 0, as Java's does; an angle it wraps is checked apart
                            if (WRAPPED[kind]?.includes(field) && Math.abs(value) > 180) continue;
                            assert.ok(output === expected, `${version} ${field} ${value} gave ${output}`);
                        }
                    } else if (LIMITS[type]) {
                        for (const value of [...LIMITS[type], 0, 1]) assert.ok(Object.is(tick({ [field]: value })[field], value), `${version} ${field} ${value} gave ${tick({ [field]: value })[field]}`);
                    }
                }
            });

            it('gives a zero the reader folds with absent the same result on each release, and keeps any other apart', () => {
                for (const version of running) for (const field of fields) {
                    if (recomputed(version, field)) continue;
                    const tick = ticker(version);
                    if (!types[kind][field].startsWith('Option<') || bare(types[kind][field]) === 'bool') continue;
                    const fallback = /\.unwrap_or\(([^()]*)\)/.exec(read[kind][field]?.expression ?? '')?.[1];
                    if (fallback === undefined) continue;
                    if (/^-?0(\.0)?$/.test(fallback)) assert.deepEqual(tick({ [field]: 0 }), tick({}), `${version} ${field}`);
                    else assert.equal(tick({ [field]: 0 })[field], 0, `${version} ${field}`);
                }
            });
        });
    }
});

describe('the player', () => {
    const { NativePlayerInput: IN, NativePlayerInputFlag: FLAG } = physics;
    const { EPhysicsCtx } = require('@nxg-org/mineflayer-physics-util/dist/physics/settings/entityPhysicsCtx');
    const base = {
        position: { x: 0.5, y: 70.25, z: 0.5 }, velocity: { x: 0.125, y: -0.08, z: 1 / 3 }, yaw: 0, pitch: 0, look: { x: 0, y: 0, z: 1 },
        halfWidth: 0.3, height: 1.8, eyeHeight: 1.62, onGround: false, collidedHorizontally: false, collidedVertically: false, isPlayer: true, clientSide: true,
    };
    const fields = types.NativeSimulationState;
    const held = (version, field, value) => new native.NativePlayer(version, { ...base, [field]: value }, {}).state()[field];
    const narrowedOnTheWay = field => read.NativeSimulationState[field]?.narrowed;
    /** One tick's scene: a tick reads them, and the state a player keeps does not hold them (docs/state-contract.md). */
    const scene = ['heightAfterTick', 'fireworkRockets', 'fireworkRocketsBeforePlayer'];

    it('keeps each scalar through a NativePlayer on each release: bounds, fractions, zero and absent', () => {
        let kept = 0;
        for (const version of RELEASES) for (const [field, type] of Object.entries(fields)) {
            if (scene.includes(field)) continue;
            const kind = bare(type);
            const values = kind === 'f64' ? [0, 0.1, 1 / 3] : LIMITS[kind] ? [...LIMITS[kind], 0] : kind === 'bool' ? [true, false] : [];
            for (const value of values) {
                let output;
                try {
                    output = held(version, field, value);
                } catch (error) {
                    assert.match(error.message, new RegExp(`${field}|fire draw|entity type|out of range`), `${version} ${field} ${value}`);
                    continue;
                }
                const expected = kind === 'f64' && narrowedOnTheWay(field) ? Math.fround(value) : value;
                assert.equal(output, expected, `${version} ${field} ${value}`);
                kept += 1;
            }
            if (type.startsWith('Option<') && !read.NativeSimulationState[field]?.expression.includes('unwrap_or')) {
                const { [field]: left, ...without } = base;
                assert.equal(new native.NativePlayer(version, without, {}).state()[field] ?? undefined, undefined, `${version} ${field} absent stays absent`);
            }
        }
        assert.ok(kept > 200 * RELEASES.length, `${kept}`);
    });

    it('gives the same fractional tick through the object and packed paths on each release', () => {
        for (const version of RELEASES) for (const size of [{ halfWidth: Math.fround(0.3) * Math.fround(0.0625), height: Math.fround(1.8) * Math.fround(0.0625) }, { halfWidth: Math.fround(0.45), height: Math.fround(2.7) }]) {
            const state = { ...base, ...size, eyeHeight: Math.fround(size.height * 0.85), fallDistance: 0.1 };
            const input = new Float64Array(IN.LENGTH);
            input.set([state.position.x, state.position.y, state.position.z], IN.POSITION);
            input.set([state.velocity.x, state.velocity.y, state.velocity.z], IN.VELOCITY);
            input.set([0, 0, 1], IN.LOOK);
            input[IN.HALF_WIDTH] = state.halfWidth;
            input[IN.HEIGHT] = state.height;
            input[IN.EYE_HEIGHT] = state.eyeHeight;
            input[IN.FALL_DISTANCE] = state.fallDistance;
            input[IN.FLYING_SPEED] = Math.fround(0.05);
            input[IN.ITEM_USE_SPEED_MULTIPLIER] = 0.2;
            input[IN.FLAGS] = FLAG.UNMOVED;
            const output = new native.NativePlayer(version, state, {}).tick(world(), input, new Float64Array(0));
            const direct = native.simulateVersionedTick(version, world(), state, {});
            const { NativePlayerOutput: OUT } = physics;
            assert.deepEqual([output[OUT.POSITION], output[OUT.POSITION + 1], output[OUT.POSITION + 2]], [direct.position.x, direct.position.y, direct.position.z], version);
            assert.deepEqual([output[OUT.VELOCITY], output[OUT.VELOCITY + 1], output[OUT.VELOCITY + 2]], [direct.velocity.x, direct.velocity.y, direct.velocity.z], version);
            assert.equal(output[OUT.FALL_DISTANCE], direct.fallDistance, version);
        }
    });

    for (const version of ADAPTED) it(`gives the same tick through the adapter and the object path on ${version}`, () => {
        const bot = botOf(version);
        bot.entity.position.set(0.5, 70.25, 0.5);
        bot.entity.velocity.set(0.125, -0.08, 1 / 3);
        bot.entity.onGround = false;
        bot.controlState.forward = false;
        const engine = physics.createNativePhysics(bot.registry);
        const ctx = EPhysicsCtx.FROM_BOT(engine, bot);
        const direct = native.simulateVersionedTick(version, loaded(), { ...base, yaw: ctx.state.yaw ?? 0, look: undefined }, {});
        engine.simulate(ctx, bot.world);
        assert.deepEqual({ x: ctx.state.pos.x, y: ctx.state.pos.y, z: ctx.state.pos.z }, direct.position);
        assert.deepEqual({ x: ctx.state.vel.x, y: ctx.state.vel.y, z: ctx.state.vel.z }, direct.velocity);
    });
});

describe('fractional dimensions', () => {
    const mob = { position: { x: 0.5, y: 100, z: 0.5 }, velocity: { x: 0, y: 0, z: 0 }, yaw: 0, forward: 0, strafe: 0, jumpRequested: false, onGround: false, inWater: false };

    it('come back as the float vanilla holds, a half doubled and halved', () => {
        for (const width of [0.6, 1.3964844, 0.2734375, 0.45]) {
            const output = native.simulateVersionedHorseTick('26.3', world(), { ...mob, entityType: 'horse', halfWidth: Math.fround(width) / 2, height: 1.6 });
            assert.equal(output.halfWidth * 2, Math.fround(width), `${width}`);
            assert.equal(output.height, Math.fround(1.6));
        }
    });

    it('refuses a zero or absent-looking size where the box needs one', () => {
        assert.throws(() => native.simulateVersionedHorseTick('26.3', world(), { ...mob, height: NaN }), /ridden mob simulation state contains invalid values: height is NaN, it must be finite and above 0/);
        assert.throws(() => native.simulateVersionedHorseTick('26.3', world(), { ...mob, halfWidth: 0 }), /halfWidth is 0, it must be finite and above 0/);
    });
});

describe('a refused state of an entity that is not the player', () => {
    const at = { position: { x: 0.5, y: 100, z: 0.5 }, velocity: { x: 0, y: 0, z: 0 } };
    const mob = { ...at, yaw: 0, forward: 0, strafe: 0, jumpRequested: false, onGround: false, inWater: false, entityType: 'horse' };
    const boat = { ...at, yaw: 0, forward: false, left: false, right: false, inWater: false };
    const push = { ...at, noPhysics: false, vehicle: false, pushable: true, pushthrough: 0 };

    it('names the field, its value and its rule, for every kind', () => {
        const refusals = [
            [() => native.simulateHorseTick({ ...mob, strafe: Infinity }), /horse simulation state contains invalid values: strafe is Infinity, it must be finite/],
            [() => native.simulateHorseTick({ ...mob, movementSpeed: -1 }), /horse simulation state contains invalid values: movementSpeed is -1, it must be finite and not negative/],
            [() => native.simulateVersionedHorseTick('26.3', world(), { ...mob, velocity: { x: 0, y: NaN, z: 0 } }), /ridden mob simulation state contains invalid values: velocity\.y is NaN/],
            [() => native.simulateVersionedHorseTick('1.20.4', world(), { ...mob, jumpStrength: 3 }), /jumpStrength is 3, it must be from 0 to 2/],
            [() => native.simulateVersionedHorseTick('26.3', world(), { ...mob, jumpStrength: 33 }), /jumpStrength is 33, it must be from 0 to 32/],
            [() => native.simulateVersionedBoatTick('26.3', world(), { ...boat, deltaRotation: NaN }), /boat simulation state contains invalid values: deltaRotation is NaN/],
            [() => native.simulateVersionedProjectileTicks('26.3', world(), [{ ...at, gravity: NaN }], 1), /projectile simulation state contains invalid values: gravity is NaN/],
            [() => native.simulateVersionedArrowTicks('26.3', world(), [{ ...at, airDrag: Infinity }], 1), /arrow simulation state contains invalid values: airDrag is Infinity/],
            [() => native.simulateVersionedTridentTicks('26.3', world(), [{ ...at, waterInertia: NaN }], 1), /trident simulation state contains invalid values: waterInertia is NaN/],
            [() => native.simulateVersionedEntityPush('26.3', push, { ...push, pushthrough: 2 }, false), /entity push state contains invalid values: right\.pushthrough is 2, it must be from 0 to 1/],
            [() => native.simulateVersionedMovingEntityTick('26.3', world(), { ...at, kind: 'item', fallDistance: NaN }), /moving entity state contains invalid values: fallDistance is NaN/],
            [() => native.createSimulationState({ ...at, halfWidth: 0.3, height: -1, onGround: false, yaw: 0, inWater: false, inLava: false }), /entity descriptor contains invalid values: height is -1, it must be finite and above 0/],
        ];
        for (const [call, message] of refusals) assert.throws(call, message);
    });
});

for (const version of ADAPTED) describe(`the same input through each path on ${version}`, () => {
    /** What does not change by release: once. */
    const latest = version === '26.3' ? it : () => {};
    const scene = () => {
        const { world: prismarine, registry } = botOf(version);
        return { prismarine, registry };
    };
    const position = { x: 0.5, y: 100.25, z: 0.5 };
    /** The box class of mineflayer-physics-utils' states, which a release before 1.17 keeps. */
    const getBB = () => ({ minX: 0, minY: 0, minZ: 0, maxX: 0, maxY: 0, maxZ: 0, clone() { return { ...this }; } });

    it('projectile, arrow and trident: adapter and binding agree', () => {
        const { prismarine, registry } = scene();
        const engine = new adapter.NativeProjectilePhysics(registry);
        const velocity = { x: 0.125, y: -0.1, z: 1 / 3 };
        const thrown = { kind: 'snowball', position, velocity, inWater: false, firstTick: true, gravity: 0, airDrag: 0.5 };
        assert.deepEqual(engine.predictProjectile(prismarine, thrown).result, native.simulateVersionedProjectileTicks(version, world(), [thrown], 1)[0]);
        const hits = {};
        const arrow = { position, velocity, life: 65535, shakeTime: 255, inGroundTime: 0, gravity: 0 };
        const direct = native.simulateVersionedArrowHitTick(version, world(), arrow, hits, [], null);
        const through = engine.tickArrow(prismarine, arrow, hits, { entities: [] });
        assert.deepEqual(through.arrow, direct.arrow);
    });

    latest('projectile: a refused tick gives each kind its own acceleration, and the owner comes at its scale', () => {
        const { registry } = scene();
        const engine = new adapter.NativeProjectilePhysics(registry);
        const far = { position, velocity: { x: 1e10, y: 0, z: 0 } };
        const { prismarine } = scene();
        assert.equal(engine.tickProjectile(prismarine, { ...far, kind: 'wind_charge' }, undefined, { entities: [] }).accelerationPower, 0);
        assert.equal(engine.tickProjectile(prismarine, { ...far, kind: 'fireball' }, undefined, { entities: [] }).accelerationPower, 0.1);
        const owner = { id: 1, type: 'player', position: { x: 0, y: 0, z: 0 }, width: 0.6, height: 1.8, attributes: { 'minecraft:scale': { value: 2, modifiers: [] } } };
        const made = engine.ownerOf({ entities: [owner], owner });
        assert.equal(made.width, Math.fround(Math.fround(0.6) * 2));
        assert.equal(made.eyePosition.y, Math.fround(Math.fround(1.62) * 2));
    });

    latest('moving entity: a draw that would round up to 1 is refused', () => {
        const item = { kind: 'item', position, velocity: { x: 0, y: 0, z: 0 } };
        assert.throws(() => native.simulateVersionedMovingEntityTick(version, world(), item, { random: [1 - 2 ** -30] }), /nextFloat/);
        native.simulateVersionedMovingEntityTick(version, world(), item, { random: [Math.fround(1 - 2 ** -24)] });
    });

    it('moving entity: adapter and binding agree', () => {
        const { prismarine, registry } = scene();
        const engine = new adapter.NativeMovingEntityPhysics(registry);
        engine.track({ id: 5, name: 'item', position: new Vec3(position.x, position.y, position.z), velocity: new Vec3(0, 0, 0) });
        const state = engine.state(5);
        const direct = native.simulateVersionedMovingEntityTick(version, world(), { ...state });
        assert.deepEqual(engine.tick(5, prismarine).state, direct.state);
    });

    it('minecart: adapter and binding agree', () => {
        const { prismarine, registry } = scene();
        const engine = new adapter.NativeMinecartPhysics(registry);
        const cart = { variant: 'minecart', position, velocity: { x: 0.25, y: 0, z: 0 }, yRot: 0, flipped: false, onGround: false, inWater: false, fuel: 0, tickCount: 2147483647, hurtTime: 0 };
        const rules = { improvements: false };
        assert.deepEqual(engine.simulate(cart, prismarine, rules).state, native.simulateVersionedMinecartTick(version, world(), cart, rules).state);
    });

    it('boat: adapter and binding agree', () => {
        const { prismarine, registry } = scene();
        const engine = new adapter.NativeBoatPhysics(registry);
        const state = {
            getBB, age: 0, pos: new Vec3(position.x, position.y, position.z), vel: new Vec3(0, -0.04, 0), yawDegrees: 12.5, yaw: 0, onGround: false, isCollidedHorizontally: false, isCollidedVertically: false,
            halfWidth: 0.6875, height: 0.5625, yawVelocity: 0.25, landFriction: 0, lastVerticalVelocity: 0, speedMultiplier: 1, controllingPlayer: true, status: 4, previousStatus: 4, waterLevel: 0,
            control: { forward: false, back: false, left: false, right: false },
        };
        const direct = native.simulateVersionedBoatTick(version, world(), {
            position, velocity: { x: 0, y: -0.04, z: 0 }, yaw: 0, yawDegrees: 12.5, forward: false, left: false, right: false, inWater: false, onGround: false, collidedHorizontally: false, collidedVertically: false,
            deltaRotation: 0.25, landFriction: 0, lastVerticalVelocity: 0, speedMultiplier: 1, playerPassenger: true, status: 'in_air', previousStatus: 'in_air', waterLevel: 0, ultraWarm: false, noGravity: false,
        });
        engine.simulate({ state }, prismarine);
        assert.deepEqual({ x: state.pos.x, y: state.pos.y, z: state.pos.z }, direct.position);
        assert.deepEqual({ x: state.vel.x, y: state.vel.y, z: state.vel.z }, direct.velocity);
        assert.equal(state.yawVelocity, direct.deltaRotation);
    });

    it('ridden mob: adapter and binding agree', () => {
        const { prismarine, registry } = scene();
        const engine = new adapter.NativeRiddenMobPhysics(registry);
        const mob = {
            getBB, age: 0, type: 'horse', pos: new Vec3(position.x, position.y, position.z), vel: new Vec3(0, 0, 0), yawDegrees: 0, pitchDegrees: 0, onGround: false, isCollidedHorizontally: false, isCollidedVertically: false,
            dimensions: { width: Math.fround(1.3964844), height: Math.fround(1.6) }, attributes: {}, effects: {}, movementSpeed: 0, jumpStrength: 0, saddled: true, fallDistance: 0,
        };
        const direct = native.simulateVersionedHorseTick(version, world(), {
            entityType: 'horse', position, velocity: { x: 0, y: 0, z: 0 }, yaw: 0, yawDegrees: 0, pitchDegrees: 0, forward: 1, strafe: 0, jumpRequested: false, onGround: false, inWater: false,
            movementSpeed: 0, jumpStrength: 0, saddled: true, halfWidth: Math.fround(1.3964844) / 2, height: Math.fround(1.6), fallDistance: 0, riderYaw: 0, riderPitch: 0, riderSprinting: false, inLava: false, collidedHorizontally: false, collidedVertically: false,
        });
        engine.simulateRidden({ state: mob }, prismarine, { zza: 1, xxa: 0, yawDegrees: 0, pitchDegrees: 0 });
        assert.deepEqual({ x: mob.pos.x, y: mob.pos.y, z: mob.pos.z }, direct.position);
        assert.deepEqual({ x: mob.vel.x, y: mob.vel.y, z: mob.vel.z }, direct.velocity);
    });

    latest('ridden mob: a push does not guess at a size the state does not hold', () => {
        const { prismarine, registry } = scene();
        const engine = new adapter.NativeRiddenMobPhysics(registry);
        const mob = { age: 0, type: 'horse', pos: new Vec3(0.5, 100, 0.5), vel: new Vec3(0, 0, 0), onGround: false, isCollidedHorizontally: false, isCollidedVertically: false, attributes: {} };
        const piston = { pos: [1, 100, 0], direction: [1, 0, 0], extending: true, source: false, progress: 0, shapes: [[0, 0, 0, 1, 1, 1]] };
        assert.throws(() => engine.pushByMovingPiston({ state: mob }, prismarine, piston), /needs its dimensions/);
    });

    it('world: a collision box brings its bounce, and a box that would drop it is refused', () => {
        const box = { minX: 0, minY: 0, minZ: 0, maxX: 1, maxY: 1, maxZ: 1, slipperiness: 0.6 };
        const made = world();
        made.applyBlockUpdates([{ x: 0, y: 0, z: 0, id: 1, shape: { ...box, bounceRestitution: 0.75, suppressesBounce: true, fallDamageResetting: true } }]);
        const block = made.blockAt(0, 0, 0);
        assert.equal(block.bounceRestitution, 0.75);
        assert.equal(block.suppressesBounce, true);
        assert.equal(block.fallDamageResetting, true);
        made.applyBlockUpdates([{ x: 1, y: 0, z: 0, id: 1, bounceRestitution: 0, shape: { ...box, bounceRestitution: 0.75 } }]);
        assert.equal(made.blockAt(1, 0, 0).bounceRestitution, 0, 'the update speaks over its shape, a zero included');
        for (const channel of ['outlineShape', 'interactionShape']) {
            assert.throws(() => made.applyBlockUpdates([{ x: 2, y: 0, z: 0, id: 1, shape: box, [channel]: { ...box, bounceRestitution: 1 } }]), /carries no bounceRestitution/, channel);
        }
        assert.throws(() => made.setBlockOutlineShape(0, 0, 0, { ...box, suppressesBounce: true }), /carries no bounceRestitution/);
        assert.throws(() => made.setBlockShape(3, 0, 0, 1, { ...box, bounceRestitution: -1 }), /bounce restitution/);
        assert.throws(() => made.setBlockShapes(3, 0, 0, 1, [{ ...box, bounceRestitution: NaN }]), /bounce restitution/);
    });

    it('world: the boxes of a cell in a batch keep what any of them said of the block', () => {
        const box = { minX: 0, minY: 0, minZ: 0, maxX: 1, maxY: 0.5, maxZ: 1, slipperiness: 0.6 };
        const made = world();
        made.applyBlockUpdates([
            { x: 0, y: 0, z: 0, id: 7, stateVersion: version, properties: [{ name: 'half', value: 'bottom' }], blockEntity: 'chest', shape: box },
            { x: 0, y: 0, z: 0, id: 7, shape: { ...box, minY: 0.5, maxY: 1 } },
        ]);
        const block = made.blockAt(0, 0, 0);
        assert.deepEqual(block.properties, [{ name: 'half', value: 'bottom' }]);
        assert.equal(block.stateVersion, version);
        assert.equal(block.blockEntity, 'chest');
    });

    it('world: the last block state of the release reads back as its own, by its id', () => {
        const { registry } = scene();
        const last = registry.blocksArray.reduce((top, block) => (block.maxStateId > top.maxStateId ? block : top));
        const id = last.maxStateId ?? last.id;
        const made = world();
        made.applyBlockUpdates([{ x: 0, y: 0, z: 0, id, stateVersion: version }]);
        assert.equal(made.blockAt(0, 0, 0).id, id);
        assert.equal(made.blockAt(0, 0, 0).stateVersion, version);
        assert.throws(() => made.applyBlockUpdates([{ x: 1, y: 0, z: 0, id, stateVersion: '1.19.0' }]), /not present|invalid|version/);
    });

    it('world: what an update writes reads back, whole, with its zeros, and a state id survives absence', () => {
        const made = world();
        const shape = { minX: 0.0625, minY: 0, minZ: 0, maxX: 0.9375, maxY: 0.5, maxZ: 1, slipperiness: 0.98, bounceRestitution: 0, suppressesBounce: true, fallDamageResetting: false, effect: 0 };
        made.applyBlockUpdates([{
            x: -2147483648, y: 0, z: 2147483647, id: 4294967295, properties: [{ name: 'waterlogged', value: 'true' }, { name: 'a', value: 'b' }], tags: 0, shape, fluid: 1, fluidLevel: 0, fluidTags: 0,
            fluidSource: true, fluidFalling: false, fluidFlow: { x: 0, y: 0, z: 0 },
        }]);
        const block = made.blockAt(-2147483648, 0, 2147483647);
        assert.equal(block.id, 4294967295);
        assert.deepEqual(block.properties, [{ name: 'waterlogged', value: 'true' }, { name: 'a', value: 'b' }], 'exact tuple, in order');
        assert.equal(block.fluid, 1);
        assert.equal(block.fluidLevel, 0);
        assert.equal(block.fluidSource, true);
        assert.equal(block.slipperiness, 0.98);
        assert.equal(block.shape.min.x, 0.0625);
        assert.equal(block.shape.max.y, 0.5);
        assert.throws(() => made.applyBlockUpdates([{ x: 0, y: 0, z: 0, fluidLevel: 16, fluid: 1 }]), /fluid level/);
        assert.throws(() => made.applyBlockUpdates([{ x: 0, y: 0, z: 0, fluid: 4 }]), /fluid must be/);
        assert.throws(() => made.applyBlockUpdates([{ x: 0, y: 0, z: 0, fluid: 1.5 }]), /fluid is 1\.5/);
    });
});
