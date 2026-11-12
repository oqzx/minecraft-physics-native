'use strict';

const assert = require('node:assert/strict');
const { it } = require('node:test');
const { LoadNativePhysics } = require('..');
const native = LoadNativePhysics();

function worldOfFluid(fluid, flowing = false) {
    const world = new native.NativeWorld();
    // 26.1+ checks the scan's one-cell halo for loaded chunks. At this origin
    // the halo reaches the negative X/Z neighbours too.
    for (let x = -1; x <= 1; x++) {
        for (let z = -1; z <= 1; z++) world.setChunkLoaded(x, z, true);
    }
    world.applyBlockUpdates([{
        x: 0, y: 0, z: 0, fluid,
        fluidFlow: { x: flowing ? 1 : 0, y: 0, z: 0 },
    }]);
    return world;
}

function boat(maxY, underwater = false, min = { x: 0, y: -1, z: 0 }) {
    return { boundingBox: { min, max: { x: min.x + 1, y: maxY, z: min.z + 1 } }, underwater };
}

function state(fields = {}) {
    return {
        position: { x: 0.5, y: 0.2, z: 0.5 }, velocity: { x: 0, y: 0, z: 0 },
        halfWidth: 0.2, height: 1.6, eyeHeight: 0.4,
        onGround: false, collidedHorizontally: false, collidedVertically: false,
        isPlayer: true, clientSide: true, controlledByLocalInstance: true, noGravity: true,
        passenger: true,
        ...fields,
    };
}

function tick(version, world, input) {
    return native.simulateVersionedTick(version, world, input, {});
}

it('uses the 1.19.3 underwater-boat breakpoint for passenger water contact', () => {
    const world = worldOfFluid(1);
    for (const version of ['1.9.4', '1.12.2', '1.16.1', '1.19.2', '1.19.3', '1.21.11']) {
        const submerged = tick(version, world, state({ boatVehicle: boat(3, true) }));
        const conditional = ['1.19.3', '1.21.11'].includes(version);
        assert.equal(submerged.inWater, conditional, version);
        assert.equal(tick(version, world, state({ boatVehicle: boat(3) })).inWater, false, version);
        assert.equal(tick(version, world, state()).inWater, true, `${version}: nonboat passenger`);
    }
    // Historical boat exclusion concerns water, not the independent lava scan.
    assert.equal(tick('1.21.11', worldOfFluid(2), state({ boatVehicle: boat(3) })).inLava, true);
});

it('keeps the 1.16.2 eye-containment and 1.20.5 eye-offset changes distinct', () => {
    const world = worldOfFluid(1);
    const eyeState = state({ position: { x: 0.5, y: 0, z: 0.5 }, eyeHeight: 0.6 });
    const offsetEyeY = Math.fround(0.6) - Math.fround(0.11111111);
    const outsideXZ = boat(offsetEyeY + 0.1, false, { x: 2, y: offsetEyeY - 0.1, z: 2 });
    assert.equal(tick('1.16.1', world, { ...eyeState, boatVehicle: outsideXZ }).eyeInWater, true);
    assert.equal(tick('1.16.2', world, { ...eyeState, boatVehicle: outsideXZ }).eyeInWater, false);

    const atTop = boat(offsetEyeY);
    assert.equal(tick('1.16.1', world, { ...eyeState, boatVehicle: atTop }).eyeInWater, true);
    assert.equal(tick('1.16.2', world, { ...eyeState, boatVehicle: atTop }).eyeInWater, false);

    const actualEyeOnly = boat(0.65, false, { x: 0, y: 0.55, z: 0 });
    assert.equal(tick('1.20.4', world, { ...eyeState, boatVehicle: actualEyeOnly }).eyeInWater, true);
    assert.equal(tick('1.20.5', world, { ...eyeState, boatVehicle: actualEyeOnly }).eyeInWater, false);
});

it('uses the adjusted modern scan without changing the passenger depth baseline or dimensions', () => {
    for (const version of ['26.1', '26.1.1', '26.1.2', '26.2', '26.3']) {
        for (const fluid of [1, 2]) {
            const world = worldOfFluid(fluid, true);
            const plain = tick(version, world, state());
            assert.ok(plain.velocity.x > 0, `${version}, fluid=${fluid}: ordinary current`);
            // Trimming within this cell leaves the same fluid candidates. Depth and
            // current scaling still use the passenger's original lower boundary.
            for (const context of [boat(0.5), boat(0.1), boat(3, true)]) {
                const result = tick(version, world, state({ boatVehicle: context }));
                assert.deepEqual(result.position, plain.position, version);
                assert.deepEqual(result.velocity, plain.velocity, version);
                assert.equal(result.inWater, plain.inWater, version);
                assert.equal(result.inLava, plain.inLava, version);
                assert.equal(result.eyeInWater, plain.eyeInWater, version);
                assert.equal(result.eyeInLava, plain.eyeInLava, version);
                assert.equal(result.halfWidth, 0.2);
                assert.equal(result.height, 1.6);
                assert.deepEqual(result.boatVehicle, context);
            }
            const input = state({
                boatVehicle: boat(3), inWater: true, inLava: true, eyeInWater: true, eyeInLava: true,
            });
            const covered = tick(version, world, input);
            assert.deepEqual([covered.inWater, covered.inLava, covered.eyeInWater, covered.eyeInLava], [false, false, false, false]);
            assert.deepEqual(covered.velocity, input.velocity, version);
            assert.deepEqual(covered.position, input.position, version);
            assert.equal(covered.halfWidth, input.halfWidth);
            assert.equal(covered.height, input.height);
        }
    }
});

it('round-trips fixed boat geometry through batches and stops applying it on dismount or removal', () => {
    const world = worldOfFluid(1, true);
    const initial = state({ boatVehicle: boat(3) });
    let repeated = initial;
    for (let index = 0; index < 3; index++) repeated = tick('26.3', world, repeated);
    const [batched] = native.simulateVersionedTicks('26.3', world, [initial], 3, {});
    assert.deepEqual(batched, repeated);
    assert.deepEqual(batched.boatVehicle, initial.boatVehicle);
    assert.equal(batched.inWater, false);
    const dismounted = tick('26.3', world, { ...batched, passenger: false });
    assert.equal(dismounted.inWater, true);
    assert.ok(dismounted.velocity.x > 0);
    const removed = tick('26.3', world, { ...batched, boatVehicle: undefined });
    assert.equal(removed.inWater, true);
    assert.ok(removed.velocity.x > 0);
});
