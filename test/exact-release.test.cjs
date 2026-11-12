'use strict';

// B006: an unknown patch, a prerelease or a respelled release is refused at every entry that names a release; none is
// read as the nearest registered one. test/registry-release.test.cjs has the adapter's constructors.

const assert = require('node:assert/strict');
const { describe, it } = require('node:test');
const physics = require('..');

const native = physics.LoadNativePhysics();
const NOT_RELEASES = ['1.20.7', '1.20.99', '1.21.0', '1.20.04', '1.14', '1.8', '1.8.9', '1.20.4-pre1', '1.20.4-rc1', '24w14a', '26.1-snapshot-1', '1.21.2 ', 'v1.20.4', '', 'latest'];
const state = {
    position: { x: 0, y: 1, z: 0 }, velocity: { x: 0, y: 0, z: 0 }, halfWidth: 0.3, height: 1.8,
    onGround: false, collidedHorizontally: false, collidedVertically: false,
};

describe('a release that is not registered', () => {
    it('is unsupported on both registries', () => {
        for (const name of NOT_RELEASES) {
            assert.equal(native.isSupportedVersion(name), false, name);
            assert.equal(physics.isSupportedVersion(name), false, name);
        }
    });

    const entries = {
        simulateVersionedTick: name => native.simulateVersionedTick(name, new native.NativeWorld(), state, {}),
        simulateVersionedTicks: name => native.simulateVersionedTicks(name, new native.NativeWorld(), [state], 1, {}),
        simulateVersionedTickOutcome: name => native.simulateVersionedTickOutcome(name, new native.NativeWorld(), state, {}),
        NativePlayer: name => new native.NativePlayer(name, state, {}),
        entityTravelSupport: name => native.entityTravelSupport(name, 'boat'),
        createSimulationState: name => native.createSimulationState({ ...state, yaw: 0, inWater: false, inLava: false, version: name }),
        'a block state version': name => new native.NativeWorld().applyBlockUpdates([{
            x: 0, y: 0, z: 0, id: 1, stateVersion: name,
            shape: { minX: 0, minY: 0, minZ: 0, maxX: 1, maxY: 1, maxZ: 1, slipperiness: 0.6, effect: 0 },
        }]),
    };
    for (const [name, call] of Object.entries(entries)) {
        it(`is refused by ${name}`, () => {
            for (const release of NOT_RELEASES) assert.throws(() => call(release), /Minecraft version|version/i, `${name} ${release}`);
            call('1.20.4');
        });
    }

    it('is refused by the world border, which reads a release too', () => {
        const world = new native.NativeWorld();
        for (const release of ['1.20.99', '1.21.0', '1.20.4-pre1']) {
            assert.throws(() => world.isInsideWorldBorder(0, 0, release), /unsupported Minecraft version/, release);
        }
        assert.equal(world.isInsideWorldBorder(0, 0, '1.20.4'), true);
    });

    it('has no constructor, in the generic factory or by name', () => {
        const registry = version => ({ version: { minecraftVersion: version, '<': () => false, '>': () => false } });
        for (const release of NOT_RELEASES) {
            assert.equal(physics.NATIVE_PHYSICS_ENGINES[release], undefined, release);
            assert.throws(() => physics.createNativePhysics(registry(release)), Error, release);
        }
    });

    it('keeps patches of one line apart, each its own engine', () => {
        for (const [left, right] of [['1.15', '1.15.1'], ['1.21.9', '1.21.10'], ['26.1', '26.1.1']]) {
            assert.notEqual(physics.NATIVE_PHYSICS_ENGINES[left], physics.NATIVE_PHYSICS_ENGINES[right]);
        }
    });
});

describe('parity admission at construction', () => {
    const registry = version => ({ version: { minecraftVersion: version, '<': () => false, '>': () => false } });
    const unloadable = { get isSupportedVersion() { throw new Error('the binding was read'); } };

    it('says what a release has shown', () => {
        assert.equal(physics.releaseStatus('1.20.7'), undefined);
        assert.equal(physics.releaseStatus('constructor'), undefined);
        assert.equal(physics.releaseStatus('26.3').admitted, false);
        // (the legacy releases are read from their own decompiled jars: exact, as the later ones)
        assert.equal(physics.releaseStatus('1.12.2').source.status, 'exact');
        assert.equal(physics.releaseStatus('1.8.8').source.status, 'exact');
    });

    it('refuses a release not admitted when parity is required, naming what it lacks, before reading the binding', () => {
        assert.throws(() => physics.createNativePhysics(registry('26.3'), { requireParity: true, binding: unloadable }), /26\.3 is not admitted for parity: conformance unverified/);
        assert.throws(() => physics.createNativePhysics(registry('1.12.2'), { requireParity: true, binding: unloadable }), /1\.12\.2 is not admitted for parity: conformance unverified/);
        assert.throws(() => new physics.NativePhysics_1_20_4(registry('1.20.4'), { requireParity: true, binding: unloadable }), /1\.20\.4 is not admitted/);
        assert.throws(() => physics.createNativeBoatPhysics(registry('1.20.4'), { requireParity: true, binding: unloadable }), /1\.20\.4 is not admitted/);
        assert.throws(() => physics.assertReleaseAdmitted('1.20.7'), /1\.20\.7 is not a registered release/);
    });

    it('still runs a registered release for development, labelled by its status', () => {
        const engine = physics.createNativePhysics(registry('26.3'), { binding: native });
        assert.equal(engine.version, '26.3');
    });
});
