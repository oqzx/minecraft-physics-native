'use strict';

const assert = require('node:assert/strict');
const { it } = require('node:test');
const adapter = require('..');
const native = adapter.LoadNativePhysics();

const zombie = (id, x, extra = {}) => ({ id, position: { x, y: 0, z: 0 }, width: 0.6, height: 1.95, ...extra });
const arrow = () => ({ position: { x: 0, y: 1, z: 0 }, velocity: { x: 3, y: 0, z: 0 } });
const aged = { tickCount: 20, flyingTicks: 20, leftOwner: true };

it('binds an arrow tick among ordered targets, with its structured hit', () => {
    const world = new native.NativeWorld();
    const tick = native.simulateVersionedArrowHitTick('26.3', world, arrow(), aged, [zombie(2, 2.5), zombie(1, 1.5)]);
    assert.equal(tick.entityHits.length, 1);
    const [hit] = tick.entityHits;
    assert.equal(hit.target, 1);
    assert.equal(hit.reaction, 'hurt');
    assert.equal(hit.damage, 6);
    assert.equal(hit.location.y, 1);
    assert.equal(tick.arrow.discarded, true);
});

it('finds a dragon by its parts, through its box before 1.21.4', () => {
    const world = new native.NativeWorld();
    const box = { min: { x: 1.5, y: 0, z: -1 }, max: { x: 3.5, y: 2, z: 1 } };
    const part = { id: 2, position: { x: 2.5, y: 0, z: 0 }, boundingBox: box };
    const far = { min: { x: 100, y: 0, z: 100 }, max: { x: 116, y: 8, z: 116 } };
    const hitOf = (version, target) => native.simulateVersionedArrowHitTick(version, world, arrow(), aged, [target]).entityHits;
    assert.equal(hitOf('26.3', part)[0].target, 2);
    assert.equal(hitOf('1.21.3', part)[0].target, 2);
    assert.deepEqual(hitOf('1.21.3', { ...part, dragonBox: far }), []);
});

it('reports the block a ray stops at by its face', () => {
    const world = new native.NativeWorld();
    world.applyBlockUpdates([{ x: 2, y: 1, z: 0, id: 1 }]);
    const tick = native.simulateVersionedArrowHitTick('26.3', world, arrow(), aged, [zombie(1, 4)]);
    assert.deepEqual(tick.entityHits, []);
    assert.deepEqual(tick.blockHit.block, { x: 2, y: 1, z: 0 });
    assert.equal(tick.blockHit.face, 'west');
    assert.equal(tick.blockHit.border, false);
});

it('keeps pierced ids across batches through hitState', () => {
    const world = new native.NativeWorld();
    const targets = [zombie(1, 1.5), zombie(2, 4.5)];
    let tick = native.simulateVersionedArrowHitTick('1.20.4', world, arrow(), { ...aged, pierceLevel: 2 }, targets);
    assert.deepEqual(tick.hitState.pierced, [1]);
    tick = native.simulateVersionedArrowHitTick('1.20.4', world, { ...arrow(), position: tick.arrow.position }, tick.hitState, targets);
    assert.deepEqual(tick.hitState.pierced, [1, 2]);
    assert.deepEqual(tick.entityHits.map(hit => hit.target), [2]);
});

it('takes an owner explicitly: no owner, no return', () => {
    const world = new native.NativeWorld();
    const state = { position: { x: 5, y: 1, z: 0 }, velocity: { x: 0, y: 0, z: 0 }, dealtDamage: true };
    const hits = { ...aged, loyalty: 2 };
    const alone = native.simulateVersionedTridentHitTick('26.3', world, state, hits, []);
    assert.equal(alone.trident.position.x, 5);
    const owner = { id: 9, eyePosition: { x: 0, y: 1.62, z: 0 }, width: 0.6, alive: true, player: true };
    const home = native.simulateVersionedTridentHitTick('26.3', world, state, hits, [], owner);
    assert.ok(home.trident.position.x < 5);
    const dead = native.simulateVersionedTridentHitTick('26.3', world, state, hits, [], { ...owner, alive: false });
    assert.equal(dead.trident.discarded, true);
    assert.throws(() => native.simulateVersionedTridentHitTick('1.12.2', world, state, hits, []), /before 1.13/);
});

it('refuses a target without a box or a size', () => {
    const world = new native.NativeWorld();
    assert.throws(() => native.simulateVersionedArrowHitTick('26.3', world, arrow(), aged, [{ id: 1, position: { x: 1, y: 0, z: 0 } }]), /boundingBox/);
});

it('sizes a target from its width, height and scale', () => {
    const box = native.projectileTargetHitbox({ x: 0, y: 0, z: 0 }, 0.6, 1.8, 2);
    assert.equal(box.max.y, Math.fround(Math.fround(1.8) * 2));
    assert.equal(box.max.x, Math.fround(Math.fround(Math.fround(0.6) * 2) / 2));
});

it('the mineflayer adapter makes candidates of entities: pose, scale, vehicle, type', () => {
    const registry = require('prismarine-registry')('1.21.1');
    const physics = new adapter.NativeProjectilePhysics(registry);
    const swimmer = physics.targetOf({ id: 1, type: 'player', position: { x: 0, y: 0, z: 0 }, width: 0.6, height: 1.8, metadata: { 6: 3 } });
    assert.equal(swimmer.height, Math.fround(0.6));
    const crouched = physics.targetOf({ id: 2, type: 'player', position: { x: 0, y: 0, z: 0 }, width: 0.6, height: 1.8, metadata: { 6: 5 } });
    assert.equal(crouched.height, Math.fround(1.5));
    const big = physics.targetOf({ id: 3, name: 'zombie', type: 'mob', position: { x: 0, y: 0, z: 0 }, width: 0.6, height: 1.95, attributes: { 'minecraft:generic.scale': { value: 3 } } });
    assert.equal(big.scale, 3);
    const rider = physics.targetOf({ id: 4, type: 'player', position: { x: 0, y: 0, z: 0 }, width: 0.6, height: 1.8, vehicle: { id: 9, vehicle: { id: 10 } } });
    assert.equal(rider.rootVehicle, 10);
    assert.equal(physics.targetOf({ id: 5, name: 'breeze', position: { x: 0, y: 0, z: 0 }, width: 0.6, height: 1.77 }).deflects, true);
    assert.equal(physics.targetOf({ id: 6, name: 'enderman', position: { x: 0, y: 0, z: 0 }, width: 0.6, height: 2.9 }).enderman, true);
    assert.equal(physics.targetOf({ id: 7, name: 'cow', position: { x: 0, y: 0, z: 0 }, width: 0.9, height: 1.4 }, { hurt: false }).hurt, false);
});

it('a pose does not size a target before 1.14', () => {
    const registry = require('prismarine-registry')('1.12.2');
    const physics = new adapter.NativeProjectilePhysics(registry);
    const target = physics.targetOf({ id: 1, type: 'player', position: { x: 0, y: 0, z: 0 }, width: 0.6, height: 1.8, metadata: { 6: 3 } });
    assert.equal(target.height, Math.fround(1.8));
    assert.equal(target.scale, 1);
});

it('the mineflayer adapter ticks an arrow through a prismarine world and its entities', () => {
    const registry = require('prismarine-registry')('1.21.1');
    const Block = require('prismarine-block')(registry);
    const air = registry.blocksByName.air.defaultState;
    const world = Object.assign(new (require('node:events').EventEmitter)(), {
        getBlock: () => Block.fromStateId(air, 0),
        getBlockStateId: () => air,
    });
    const physics = new adapter.NativeProjectilePhysics(registry);
    const owner = { id: 1, type: 'player', position: { x: 0, y: 0, z: 0 }, width: 0.6, height: 1.8 };
    const target = { id: 2, name: 'zombie', type: 'mob', position: { x: 2, y: 0, z: 0 }, width: 0.6, height: 1.95 };
    const scene = { entities: [owner, target], owner };
    const shot = { position: { x: 0, y: 1.5, z: 0 }, velocity: { x: 3, y: 0, z: 0 } };
    const tick = physics.tickArrow(world, shot, { tickCount: 1 }, scene);
    assert.deepEqual(tick.entityHits.map(hit => hit.target), [2]);
    assert.equal(tick.hitState.leftOwner, false, 'still beside its owner');
    const back = { position: { x: 1, y: 1.5, z: 0 }, velocity: { x: -3, y: 0, z: 0 } };
    assert.deepEqual(physics.tickArrow(world, back, { tickCount: 1, flyingTicks: 9 }, scene).entityHits, [], 'immune while it has not left');
});

const flight = (kind, extra = {}) => ({ kind, position: { x: 0, y: 1, z: 0 }, velocity: { x: 3, y: 0, z: 0 }, firstTick: false, gravity: 0, ...extra });

it('binds the other families among ordered targets, each to its own end', () => {
    const world = new native.NativeWorld();
    const snowball = native.simulateVersionedProjectileHitTick('26.3', world, flight('snowball'), aged, [zombie(2, 2.5), zombie(1, 1.5)]);
    assert.deepEqual(snowball.entityHits.map(hit => [hit.target, hit.reaction]), [[1, 'hurt']]);
    assert.equal(snowball.projectile.discarded, true);
    assert.ok(snowball.projectile.position.x < 2, 'it stops at the hit from 1.21.2');
    const spit = native.simulateVersionedProjectileHitTick('26.3', world, flight('llama_spit'), aged, [zombie(1, 1.5)]);
    assert.deepEqual(spit.entityHits.map(hit => hit.reaction), ['passed']);
    assert.equal(spit.projectile.discarded, false);
});

it('takes what the server decides as inputs of the state', () => {
    const world = new native.NativeWorld();
    const owner = { id: 9, eyePosition: { x: 0, y: 2, z: 0 }, width: 0.6, alive: true, player: true };
    const hook = () => flight('fishing_bobber');
    assert.equal(native.simulateVersionedProjectileHitTick('26.3', world, hook(), { ...aged, stopFishing: true }, [], owner).projectile.discarded, true);
    assert.equal(native.simulateVersionedProjectileHitTick('26.3', world, hook(), aged, [], undefined).projectile.discarded, true);
    assert.equal(native.simulateVersionedProjectileHitTick('26.3', world, hook(), aged, [], owner).projectile.discarded, false);
    const rocket = flight('firework_rocket', { shotAtAngle: true });
    const burst = native.simulateVersionedProjectileHitTick('26.3', world, rocket, { ...aged, life: 5, lifeTime: 5 }, []);
    assert.equal(burst.projectile.discarded, true);
    assert.equal(burst.hitState.life, 6);
    const bullet = native.simulateVersionedProjectileHitTick('26.3', world, flight('shulker_bullet'), { ...aged, bulletDelta: { x: 1, y: 0, z: 0 } }, []);
    assert.equal(bullet.projectile.velocity.x, 3 + (1 - 3) * 0.2);
});

it('hands a deflected fireball its halved acceleration for the next tick', () => {
    const world = new native.NativeWorld();
    const tick = native.simulateVersionedProjectileHitTick('26.3', world, flight('fireball'), aged, [zombie(1, 1.5, { deflects: true })]);
    assert.equal(tick.entityHits[0].reaction, 'deflected');
    assert.equal(tick.accelerationPower, 0.05);
});

it('the mineflayer adapter ticks a snowball through a prismarine world, and marks what its families pass', () => {
    const registry = require('prismarine-registry')('1.21.1');
    const Block = require('prismarine-block')(registry);
    const air = registry.blocksByName.air.defaultState;
    const world = Object.assign(new (require('node:events').EventEmitter)(), {
        getBlock: () => Block.fromStateId(air, 0),
        getBlockStateId: () => air,
    });
    const physics = new adapter.NativeProjectilePhysics(registry);
    const target = { id: 2, name: 'zombie', type: 'mob', position: { x: 2, y: 0, z: 0 }, width: 0.6, height: 1.95 };
    const shot = { kind: 'snowball', position: { x: 0, y: 1.5, z: 0 }, velocity: { x: 3, y: 0, z: 0 }, firstTick: false };
    const tick = physics.tickProjectile(world, shot, { tickCount: 20 }, { entities: [target] });
    assert.deepEqual(tick.entityHits.map(hit => hit.target), [2]);
    assert.equal(tick.projectile.discarded, true);
    assert.equal(physics.targetOf({ id: 3, name: 'item', position: { x: 0, y: 0, z: 0 }, width: 0.25, height: 0.25 }).item, true);
    assert.equal(physics.targetOf({ id: 4, name: 'wind_charge', position: { x: 0, y: 0, z: 0 }, width: 0.3, height: 0.3 }).windCharge, true);
    assert.equal(physics.targetOf({ id: 5, name: 'end_crystal', position: { x: 0, y: 0, z: 0 }, width: 2, height: 2 }).endCrystal, true);
});

it('hands a block without a collider its outline, which a thrown item clips until 1.16.1', () => {
    const outlined = version => {
        const registry = require('prismarine-registry')(version);
        const { NativeWorldSync } = adapter;
        const Block = require('prismarine-block')(registry);
        const torch = registry.blocksByName.torch.defaultState ?? registry.blocksByName.torch.id << 4 | 5;
        const air = registry.blocksByName.air.defaultState ?? 0;
        const stateAt = (x, y, z) => (x === 2 && y === 1 && z === 0 ? torch : air);
        const source = {
            getBlock: pos => Block.fromStateId(stateAt(pos.x, pos.y, pos.z), 0),
            getBlockStateId: pos => stateAt(pos.x, pos.y, pos.z),
        };
        const sync = new NativeWorldSync(native, registry);
        sync.sync(source, { minX: 0, minY: 0, minZ: -1, maxX: 4, maxY: 3, maxZ: 1 }, 1);
        const shot = { kind: 'snowball', position: { x: 0, y: 1.3, z: 0.5 }, velocity: { x: 3, y: 0, z: 0 }, firstTick: false, gravity: 0 };
        return native.simulateVersionedProjectileHitTick(version, sync.world, shot, aged, []).projectile.discarded;
    };
    assert.equal(outlined('1.16.1'), true, 'an outline stops it');
    assert.equal(outlined('1.16.2'), false, 'a collider does not');
});
