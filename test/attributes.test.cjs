'use strict';

// Attributes against the jars: test/fixtures/attributes.tsv is what scripts/java-oracle/OracleAttributes.java read from
// each release's own registry and DefaultAttributes, and OracleAttributesLegacy.java before 1.16.
// docs/attributes.md has the consumers.

const assert = require('node:assert/strict');
const { it } = require('node:test');
const { readFileSync } = require('node:fs');
const { join } = require('node:path');
const adapter = require('..');

const rows = readFileSync(join(__dirname, 'fixtures/attributes.tsv'), 'utf8').trim().split(/\r?\n/).map((line) => {
    const [release, key, fallback, min, max, player, zombie] = line.split('\t');
    return { release, key, fallback: Number(fallback), min: Number(min), max: Number(max), player, zombie };
});
const releases = [...new Set(rows.map(row => row.release))];

/** The attribute names the adapter sends on, with the base a player has where none is synced, and the first release. */
const CONSUMED = {
    movementSpeed: ['movement_speed', 0.1, '1.8.8'],
    scale: ['scale', 1, '1.20.5'],
    gravity: ['gravity', 0.08, '1.20.5'],
    stepHeight: ['step_height', 0.6, '1.20.5'],
    jumpStrength: ['jump_strength', Math.fround(0.42), '1.20.5'],
    safeFallDistance: ['safe_fall_distance', 3, '1.20.5'],
    fallDamageMultiplier: ['fall_damage_multiplier', 1, '1.20.5'],
    burningTime: ['burning_time', 1, '1.21'],
    movementEfficiency: ['movement_efficiency', 0, '1.21'],
    waterMovementEfficiency: ['water_movement_efficiency', 0, '1.21'],
    sneakingSpeed: ['sneaking_speed', 0.3, '1.21'],
    airDragModifier: ['air_drag_modifier', 1, '26.2'],
    frictionModifier: ['friction_modifier', 1, '26.2'],
    bounciness: ['bounciness', 0, '26.2'],
};

const path = key => key.replace(/^minecraft:/, '').replace(/^(generic|player)\./, '');
const words = name => name.replace(/[A-Z]/g, letter => `_${letter.toLowerCase()}`).replace(/\./g, '_');
const logical = key => words(path(key));
const dataOf = (version) => require('minecraft-data')(version);
const newer = (version, than) => {
    const parts = value => value.split('.').map(Number);
    const [a, b] = [parts(version), parts(than)];
    for (let i = 0; i < 3; i += 1) if ((a[i] ?? 0) !== (b[i] ?? 0)) return (a[i] ?? 0) > (b[i] ?? 0);
    return true;
};

it('holds every release it says, from the jars', () => {
    assert.deepEqual(new Set(releases), new Set(Object.keys(require('../generated/entity-data.json').releases)), 'every registered release');
    assert.equal(releases.length, 50);
    assert.ok(rows.length > 400);
});

it('minecraft-data has each attribute of the jar with its default and range', () => {
    const problems = [];
    for (const release of releases) {
        const data = dataOf(release);
        const known = new Map((data.attributesArray ?? []).map(entry => [path(entry.resource), entry]));
        for (const row of rows.filter(entry => entry.release === release)) {
            const key = path(row.key);
            const entry = known.get(key) ?? known.get(words(key));
            if (!entry) {
                problems.push(`${release} ${row.key}: absent`);
                continue;
            }
            for (const field of ['default', 'min', 'max']) {
                const want = field === 'default' ? row.fallback : row[field];
                if (Number.isFinite(want) && entry[field] !== want) problems.push(`${release} ${row.key} ${field}: data ${entry[field]} jar ${want}`);
            }
        }
    }
    // up to 1.20.6 the jars register 0.7F and 0.4F, widened; minecraft-data has the decimals. Nothing reads these two
    // defaults: a player's base is its own (below), so the difference moves nothing.
    const widened = /(flying_speed|movement_speed|flyingSpeed|movementSpeed) default: data (0\.4|0\.7) jar (0\.4000000059604645|0\.699999988079071)$/;
    // up to 1.13.2 minecraft-data holds 1 as the least of max_health; SharedMonsterAttributes has 0.0
    // (1.12.2 EntityAttributes.java:13). No movement reads max_health: the difference is stated, one release at a time.
    const health = ['1.8.8', '1.9.4', '1.10.2', '1.11.2', '1.12.2', '1.13.2'].map(release => `${release} generic.maxHealth min: data 1 jar 0`);
    assert.deepEqual(problems.filter(problem => !widened.test(problem)), health);
});

it('before 1.16 each entity gives its own base, as later releases do from a supplier', () => {
    for (const release of releases.filter(name => rows.some(row => row.release === name && row.key === 'generic.movementSpeed'))) {
        const speed = rows.find(row => row.release === release && row.key === 'generic.movementSpeed');
        assert.equal(Number(speed.player), Math.fround(0.1), release);
        assert.equal(Number(speed.zombie), Math.fround(0.23), release);
    }
});

it('a generic default is not a player default: the adapter and engine take the entity base', () => {
    const speeds = rows.filter(entry => logical(entry.key) === 'movement_speed');
    assert.equal(speeds.length, 50, 'every release, whichever way it spells the key');
    for (const row of speeds) {
        assert.equal(Math.fround(row.fallback), Math.fround(0.7), `${row.release}: registered`);
        assert.equal(Number(row.player), Math.fround(0.1), `${row.release}: Player`);
        assert.equal(Number(row.zombie), Math.fround(0.23), `${row.release}: Zombie`);
    }
});

it('each attribute the adapter forwards exists from its first release, with the base it falls back to', () => {
    for (const [name, [key, base, first]] of Object.entries(CONSUMED)) {
        for (const release of releases) {
            const row = rows.find(entry => entry.release === release && logical(entry.key) === key);
            if (newer(release, first)) {
                assert.ok(row, `${release} has ${key}`);
                assert.ok(Math.abs(Number(row.player) - base) < 1e-6, `${release} ${key}: player ${row.player}, fallback ${base}`);
            } else {
                assert.equal(row, undefined, `${release} has no ${key} yet (${name})`);
            }
        }
    }
});

it('folds modifiers as AttributeInstance.calculateValue does, whatever order they came in', () => {
    const physics = new adapter.NativePhysics(require('prismarine-registry')('26.3'));
    const value = (base, ...modifiers) => physics.attributeValue('movementSpeed', {
        value: base,
        modifiers: modifiers.map(([amount, operation], index) => ({ id: `minecraft:test_${index}`, amount, operation })),
    });
    // 26.3 AttributeInstance.java:151-169: add, then add on the added base, then multiply the total
    assert.equal(value(0.1, [0.1, 0], [0.5, 1], [1, 2]), ((0.1 + 0.1) + (0.1 + 0.1) * 0.5) * 2);
    assert.equal(value(0.1, [1, 2], [0.5, 1], [0.1, 0]), value(0.1, [0.1, 0], [0.5, 1], [1, 2]));
    assert.equal(value(0.1), 0.1, 'no modifier');
    assert.equal(value(0, [0.5, 1]), 0, 'a legal zero is kept');
    assert.equal(value(0.1, [-1, 2]), 0, 'down to the range');
    assert.equal(value(0.1, [5000, 0]), 1024, 'up to the range');
    assert.equal(value(NaN), 0, 'NaN is the minimum');
    // replacing is a new packet: the old modifier is simply not there
    assert.equal(value(0.1, [0.2, 0]), 0.1 + 0.2);
    assert.equal(value(0.1), 0.1, 'and removing it brings the base back');
});

it('a compact state and a complete one stay apart: absent is the engine fallback, a value is taken as it is', () => {
    const physics = new adapter.NativePhysics(require('prismarine-registry')('26.3'));
    const base = { attributes: {}, effects: {}, speed: 0, slowness: 0, jumpBoost: 0, levitation: 0, slowFalling: 0, dolphinsGrace: 0 };
    assert.equal(physics.attributesAndEffects({ ...base }).jumpStrength, undefined);
    assert.equal(physics.attributesAndEffects({ ...base }).gravity, undefined);
    const zero = name => ({ [physics.attributeKey(name)]: { value: 0, modifiers: [] } });
    assert.strictEqual(physics.attributesAndEffects({ ...base, attributes: zero('jumpStrength') }).jumpStrength, 0);
    assert.strictEqual(physics.attributesAndEffects({ ...base, attributes: zero('gravity') }).gravity, 0);
    assert.strictEqual(physics.attributesAndEffects({ ...base, attributes: zero('stepHeight') }).stepHeight, 0);
});

it('the key of an attribute follows the registry of the release, and a release without it has none', () => {
    const key = (version, name) => new adapter.NativePhysics(require('prismarine-registry')(version)).attributeKey(name);
    assert.equal(key('1.15.2', 'movementSpeed'), 'generic.movementSpeed');
    assert.equal(key('1.16.5', 'movementSpeed'), 'minecraft:generic.movement_speed');
    assert.equal(key('1.20.4', 'movementSpeed'), 'minecraft:generic.movement_speed');
    assert.equal(key('1.21.11', 'movementSpeed'), 'minecraft:movement_speed');
    assert.equal(key('1.20.4', 'gravity'), null);
    assert.equal(key('1.21.11', 'airDragModifier'), null);
    assert.equal(key('26.3', 'airDragModifier'), 'minecraft:air_drag_modifier');
});

it('each release\'s key is the one its own jar registers, on the registry of its own data', () => {
    for (const release of releases) {
        const physics = new adapter.NativePhysics(require('prismarine-registry')(release), { version: release });
        const speed = rows.find(row => row.release === release && logical(row.key) === 'movement_speed');
        assert.equal(physics.version, release);
        assert.equal(path(physics.attributeKey('movementSpeed')), path(speed.key), release);
    }
});

// D-161, D-167: mineflayer keeps a synced attribute under the key the protocol's mapper decodes
// (mineflayer entities.js: `entity.attributes[prop.key ?? prop.name]`), which from 1.20.5 is the
// registry id the server sends. The adapter reads each attribute it consumes by exactly that key,
// on every release from 1.20.5, and each one the release has resolves (1.20.5 and 1.20.6 too).
it('reads each consumed attribute by the very key the protocol hands mineflayer, from 1.20.5', () => {
    const problems = [];
    for (const release of releases.filter(version => newer(version, '1.20.5'))) {
        const data = dataOf(release);
        const packet = data.protocol.play.toClient.types.packet_entity_update_attributes ?? data.protocol.play.toClient.types.packet_update_attributes;
        const mapper = JSON.stringify(packet).match(/"mappings":(\{[^}]*\})/);
        const sent = new Set(Object.values(JSON.parse(mapper[1])));
        const physics = new adapter.NativePhysics(require('prismarine-registry')(release), { version: release });
        for (const [name, [, , since]] of Object.entries(CONSUMED)) {
            // (1.21 and 1.21.1 name sneaking speed player.sneaking_speed, which the adapter asks first: player.ts)
            const key = (name === 'sneakingSpeed' ? physics.attributeKey('playerSneakingSpeed') : null) ?? physics.attributeKey(name);
            if (!newer(release, since)) continue;
            if (key === null) problems.push(`${release} ${name}: no key`);
            else if (!sent.has(key)) problems.push(`${release} ${name}: ${key} is not a key the packet decodes`);
        }
        // and a state as mineflayer builds it from the packet reaches the engine
        const physicsKey = [...sent].find(key => path(key) === 'gravity' || path(key) === 'generic.gravity');
        const base = { effects: {}, speed: 0, slowness: 0, jumpBoost: 0, levitation: 0, slowFalling: 0, dolphinsGrace: 0 };
        const gravity = physics.attributesAndEffects({ ...base, attributes: { [physicsKey]: { value: 0.05, modifiers: [] } } }).gravity;
        if (gravity !== 0.05) problems.push(`${release} gravity from ${physicsKey}: ${gravity}`);
    }
    assert.deepEqual(problems, []);
});

it('a descriptor the registry lacks is no attribute, not a modern key', () => {
    const registry = require('prismarine-registry')('1.8.8');
    const physics = new adapter.NativePhysics(registry);
    for (const name of ['gravity', 'stepHeight', 'scale', 'jumpStrength', 'nonsense']) assert.equal(physics.attributeKey(name), null, name);
    const base = { attributes: {}, effects: {}, speed: 0, slowness: 0, jumpBoost: 0, levitation: 0, slowFalling: 0, dolphinsGrace: 0 };
    assert.equal(physics.attributesAndEffects({ ...base, attributes: { 'minecraft:gravity': { value: 0.5, modifiers: [] } } }).gravity, undefined);
    const stripped = Object.assign(Object.create(Object.getPrototypeOf(registry)), registry, { attributesByName: {} });
    assert.equal(new adapter.NativePhysics(stripped, { version: '1.8.8' }).attributeKey('movementSpeed'), null);
});

it('an engine runs one release: a registry of another version cannot change the semantics it selected', () => {
    const physics = new adapter.NativePhysics(require('prismarine-registry')('1.20.4'));
    assert.throws(() => { physics.version = '26.3'; }, TypeError);
    assert.equal(physics.version, '1.20.4');
    assert.throws(() => new adapter.NativePhysics(require('prismarine-registry')('1.20.4'), { version: '26.3' }));
});

it('names the client each row was read from: the sha1 classpath.cjs refuses any other jar by', () => {
    const data = require('../generated/entity-data.json').releases;
    for (const release of releases) assert.match(data[release].client, /^[0-9a-f]{40}$/, release);
});
