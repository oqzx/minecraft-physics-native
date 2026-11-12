'use strict';

// B041, B042, B044: what each release's own code says of its entities, attributes, effects and block callbacks
// (scripts/java-oracle/EntityDump.java), against the sizes and rules the adapter and minecraft-data hold.

const assert = require('node:assert/strict');
const fs = require('node:fs');
const zlib = require('node:zlib');
const path = require('node:path');
const { createRequire } = require('node:module');
const { describe, it } = require('node:test');
const { parseDump, parseLive } = require('../scripts/generate-entity-data.cjs');
const { moveOrder, moveOf } = require('../scripts/entity-move-order.cjs');
const { registeredReleases, compareReleases } = require('../scripts/registered-releases.cjs');

const root = path.resolve(__dirname, '..');
const generated = name => JSON.parse(fs.readFileSync(path.join(root, 'generated', name), 'utf8'));
const { releases } = generated('entity-data.json');
const exact = Object.keys(releases).filter(version => releases[version].status === 'exact');
const f32 = Math.fround;

/** The adapter's POSE_SIZES, read from its source: pose number -> size. */
function adapterPoses() {
    const source = fs.readFileSync(path.join(root, 'src', 'mineflayer', 'player.ts'), 'utf8');
    const names = Object.fromEntries([.../(\w+): (\d)/g[Symbol.matchAll](/export const Pose = \{([^}]*)\}/.exec(source)[1])].map(match => [match[2], match[1]]));
    const body = /const POSE_SIZES[^=]*= \{([\s\S]*?)\n\};/.exec(source)[1];
    return Object.fromEntries([...body.matchAll(/(\d): \{ width: f32\(([\d.]+)\), height: f32\(([\d.]+)\) \}/g)].map(match => [names[match[1]], { width: f32(Number(match[2])), height: f32(Number(match[3])) }]));
}

/** Vanilla's name of a pose of the adapter's; SNEAKING became CROUCHING. */
const poseName = (pose, poses) => (pose === 'SNEAKING' && !poses.SNEAKING ? 'CROUCHING' : pose);

describe('the report', () => {
    it('accounts for every registered release', () => {
        assert.deepEqual(Object.keys(releases).sort(), registeredReleases().map(({ version }) => version).sort());
        assert.deepEqual(Object.keys(releases).filter(version => !exact.includes(version)), ['1.8.8', '1.9.4', '1.10.2', '1.11.2', '1.12.2', '1.13.2']);
    });

    it('keeps an eye height it cannot read apart from one of 0', () => {
        for (const version of exact) {
            const value = releases[version].entities.player.eyeHeight;
            if (compareReleases(version, '1.20.5') < 0) assert.equal(value, null, version);
            else assert.equal(f32(value), f32(1.62), version);
        }
    });
});

describe('the reader', () => {
    const lines = ['entity\tpig\tCREATURE\t0.9\t0.9\tfalse\t-', 'pose\tSTANDING\t0.6\t1.8\tfalse\t1.62', 'attribute\tspeed\t0.7\t0.0\t1024.0\ttrue', 'effect\tspeed\tBENEFICIAL\tspeed|0.2|ADD', 'callback\tweb\tentityInside\tWebBlock'];

    it('reads a dump, and null for an absent value', () => {
        const release = parseDump(lines.join('\n'), 'x');
        assert.equal(release.entities.pig.eyeHeight, null);
        assert.equal(release.poses.STANDING.eyeHeight, 1.62);
        assert.deepEqual(release.effects.speed.modifiers, [{ attribute: 'speed', amount: 0.2, operation: 'ADD' }]);
        assert.equal(release.callbacks.entityInside.web, 'WebBlock');
    });

    it('fails on a part of the dump that is missing, a modifier it could not read, and a line it does not know', () => {
        assert.throws(() => parseDump(lines.slice(1).join('\n'), 'x'), /the dump has no entities/);
        assert.throws(() => parseDump([...lines, 'effect\thaste\tBENEFICIAL\tspeed|-|ADD'].join('\n'), 'x'), /modifier of speed was not read/);
        assert.throws(() => parseDump([...lines, 'nonsense\tx'].join('\n'), 'x'), /unknown dump line/);
    });
});

describe('the player\'s poses', () => {
    const adapter = adapterPoses();

    for (const version of exact) {
        it(`${version}: the adapter's size of every pose the release has`, () => {
            const { poses } = releases[version];
            for (const [pose, size] of Object.entries(adapter)) {
                const vanilla = poses[poseName(pose, poses)];
                if (!vanilla) continue; // (a pose the map leaves to STANDING: LONG_JUMPING's size is STANDING's)
                assert.equal(size.width, f32(vanilla.width), `${pose} width`);
                assert.equal(size.height, f32(vanilla.height), `${pose} height`);
            }
        });
    }

    it('has the eye heights the adapter hard-codes, from 1.20.5 where they are values', () => {
        for (const version of exact.filter(release => compareReleases(release, '1.20.5') >= 0)) {
            const { poses } = releases[version];
            assert.equal(f32(poses.CROUCHING.eyeHeight), f32(1.27), version);
            assert.equal(f32(poses.SLEEPING.eyeHeight), f32(0.2), version);
            assert.equal(f32(poses.FALL_FLYING.eyeHeight), f32(0.4), version);
            assert.equal(f32(poses.SWIMMING.eyeHeight), f32(0.4), version);
            assert.equal(f32(poses.DYING.eyeHeight), f32(1.62), version, 'fixed, so the scale leaves it');
            assert.equal(poses.SLEEPING.fixed, true, version);
            assert.equal(poses.DYING.fixed, true, version);
            assert.equal(poses.STANDING.fixed, false, version);
        }
    });
});

describe('every entity\'s size, against minecraft-data', () => {
    const dataRoot = (() => {
        try {
            return path.join(path.dirname(createRequire(path.join(root, 'package.json')).resolve('minecraft-data')), 'minecraft-data', 'data');
        } catch {
            return null;
        }
    })();
    const dataPaths = dataRoot ? JSON.parse(fs.readFileSync(path.join(dataRoot, 'dataPaths.json'), 'utf8')).pc : {};
    for (const version of exact) {
        const file = dataPaths[version]?.entities;
        it(`${version}`, { skip: file ? false : 'no minecraft-data of it' }, () => {
            const data = JSON.parse(fs.readFileSync(path.join(dataRoot, file, 'entities.json'), 'utf8'));
            const differing = [];
            for (const entity of data) {
                const vanilla = releases[version].entities[entity.name];
                if (!vanilla || entity.width == null) continue;
                if (f32(entity.width) !== f32(vanilla.width) || f32(entity.height) !== f32(vanilla.height)) differing.push(entity.name);
            }
            assert.deepEqual(differing, KNOWN_SIZE_DIFFERENCES[version] ?? []);
        });
    }
});

/** Where minecraft-data's entities are another release's: pinned, so a new one fails. */
const KNOWN_SIZE_DIFFERENCES = {};

describe('attributes', () => {
    const named = (version, ...names) => {
        const { attributes } = releases[version];
        const id = names.find(name => attributes[name]);
        return attributes[id];
    };

    it('keeps the movement attributes of every release at their bounds', () => {
        for (const version of exact) {
            const speed = named(version, 'movement_speed', 'generic.movement_speed', 'generic.movementSpeed');
            assert.ok(speed, version);
            assert.equal(speed.min, 0, version);
            assert.equal(speed.max, 1024, version);
            assert.equal(speed.syncable, true, version);
        }
    });

    it('tells 0.7F widened from 0.7: the default changed type', () => {
        assert.equal(named('1.20.4', 'generic.movement_speed').default, f32(0.7));
        assert.equal(named('26.3', 'movement_speed').default, 0.7);
    });

    it('reads what 1.20.5 and up added, from where it starts', () => {
        for (const version of exact) {
            const has = name => Boolean(named(version, name, `generic.${name}`, `player.${name}`));
            assert.equal(has('step_height'), compareReleases(version, '1.20.5') >= 0, `${version} step_height`);
            assert.equal(has('gravity'), compareReleases(version, '1.20.5') >= 0, `${version} gravity`);
            assert.equal(has('scale'), compareReleases(version, '1.20.5') >= 0, `${version} scale`);
        }
    });

    it('has legal zeros and defaults of the movement attributes on 26.3', () => {
        const { attributes } = releases['26.3'];
        assert.equal(attributes.step_height.default, 0.6);
        assert.equal(attributes.gravity.default, 0.08);
        assert.equal(attributes.jump_strength.default, 0.41999998688697815);
        assert.equal(attributes.safe_fall_distance.default, 3);
        assert.equal(attributes.fall_damage_multiplier.default, 1);
        assert.equal(attributes.scale.default, 1);
        assert.equal(attributes.water_movement_efficiency.default, 0, 'a legal zero');
        assert.equal(attributes.movement_efficiency.default, 0, 'a legal zero');
    });
});

describe('effects', () => {
    const speedOf = version => releases[version].effects.speed.modifiers[0];

    it('reads the speed effect of every release, one modifier of a total multiply', () => {
        for (const version of exact) {
            assert.equal(releases[version].effects.speed.modifiers.length, 1, version);
            assert.equal(speedOf(version).amount, f32(0.2), version);
            assert.ok(['MULTIPLY_TOTAL', 'ADD_MULTIPLIED_TOTAL'].includes(String(speedOf(version).operation)), version);
        }
    });

    it('keeps slowness at its -0.15 and slow falling and levitation without a modifier until they get one', () => {
        for (const version of exact) {
            assert.equal(releases[version].effects.slowness.modifiers[0].amount, f32(-0.15), version);
            assert.equal(releases[version].effects.levitation.modifiers.length, 0, version);
        }
    });
});

/**
 * Every class that overrides a movement callback of Block, in any release, and what the adapter does of it. `effect`:
 * the update carries it. `no-motion`: vanilla's override does not move an entity the engine models (a trigger, a sound,
 * growth, damage the server decides). Anything new fails here until someone decides which it is.
 */
const CALLBACKS = {
    entityInside: {
        // effectOf's codes, LiquidBlock the update's fluid, SoulsandBlock the slowing it has before getSpeedFactor
        effect: ['BaseFireBlock', 'BubbleColumnBlock', 'CactusBlock', 'CampfireBlock', 'EndPortalBlock', 'HoneyBlock', 'LavaCauldronBlock', 'LayeredCauldronBlock', 'LilyPadBlock', 'LiquidBlock', 'NetherPortalBlock', 'PowderSnowBlock', 'SoulsandBlock', 'SweetBerryBushBlock', 'WaterlilyBlock', 'WebBlock'],
        // triggers, growth and items; the wither's damage and the gateway's teleport reach the client from the server
        'no-motion': ['BasePressurePlateBlock', 'BigDripleafBlock', 'ButtonBlock', 'CauldronBlock', 'CropBlock', 'DetectorRailBlock', 'EndGatewayBlock', 'EyeblossomBlock', 'FrogspawnBlock', 'HopperBlock', 'PitcherCropBlock', 'TripWireBlock', 'WitherRoseBlock'],
    },
    stepOn: {
        effect: ['MagmaBlock', 'SlimeBlock'],
        // (a stair only forwards to the block it is made from)
        'no-motion': ['RedStoneOreBlock', 'SculkSensorBlock', 'SculkShriekerBlock', 'StairBlock', 'TurtleEggBlock'],
    },
    fallOn: {
        effect: ['BedBlock', 'HayBlock', 'HoneyBlock', 'PointedDripstoneBlock', 'PowderSnowBlock', 'SlimeBlock'],
        'no-motion': ['FarmBlock', 'FarmlandBlock', 'TurtleEggBlock'],
    },
    updateEntityMovementAfterFallOn: { effect: ['BedBlock', 'SlimeBlock'], 'no-motion': [] },
    updateEntityAfterFallOn: { effect: ['BedBlock', 'SlimeBlock'], 'no-motion': [] },
    onProjectileHit: {
        effect: [],
        // what it does to the block: the engine stops the projectile at the hit either way
        'no-motion': ['AbstractCandleBlock', 'AmethystBlock', 'BellBlock', 'BigDripleafBlock', 'CampfireBlock', 'ChorusFlowerBlock', 'DecoratedPotBlock', 'LightningRodBlock', 'PointedDripstoneBlock', 'SpeleothemBlock', 'TargetBlock', 'TntBlock'],
    },
};

describe('the block callbacks of every release', () => {
    it('has each overriding class decided, as moving an entity or not', () => {
        const undecided = new Set();
        for (const version of exact) {
            for (const [method, blocks] of Object.entries(releases[version].callbacks)) {
                const decided = CALLBACKS[method];
                assert.ok(decided, method);
                for (const owner of Object.values(blocks)) {
                    if (!decided.effect.includes(owner) && !decided['no-motion'].includes(owner)) undecided.add(`${method} ${owner} (${version})`);
                }
            }
        }
        assert.deepEqual([...undecided].sort(), []);
    });

    it('lists no class that no release has', () => {
        const seen = new Set();
        for (const version of exact) {
            for (const [method, blocks] of Object.entries(releases[version].callbacks)) {
                for (const owner of Object.values(blocks)) seen.add(`${method} ${owner}`);
            }
        }
        for (const [method, decided] of Object.entries(CALLBACKS)) {
            for (const owner of [...decided.effect, ...decided['no-motion']]) assert.ok(seen.has(`${method} ${owner}`), `${method} ${owner}`);
        }
    });

    it('holds each slowing and bouncing block to the factor the adapter gives it', () => {
        const { entityInside, stepOn, fallOn } = releases['26.3'].callbacks;
        assert.equal(entityInside.cobweb, 'WebBlock');
        assert.equal(entityInside.sweet_berry_bush, 'SweetBerryBushBlock');
        assert.equal(entityInside.powder_snow, 'PowderSnowBlock');
        assert.equal(stepOn.slime_block, 'SlimeBlock');
        assert.equal(stepOn.magma_block, 'MagmaBlock');
        assert.equal(fallOn.hay_block, 'HayBlock');
        assert.equal(fallOn.pointed_dripstone, 'PointedDripstoneBlock');
    });
});

describe('speed and jump factors, against what consumes them', () => {
    const table = version => JSON.parse(zlib.gunzipSync(fs.readFileSync(path.join(root, 'generated', 'block-shapes', `${version}.json.gz`))).toString('utf8'));
    const simulation = fs.readFileSync(path.join(root, 'native', 'src', 'simulation.rs'), 'utf8');
    const blocks = fs.readFileSync(path.join(root, 'src', 'mineflayer', 'blocks.ts'), 'utf8');

    it('has soul sand and honey as the only slow blocks and honey as the only low jump, of every release', () => {
        for (const version of exact) {
            const slow = [];
            const low = [];
            for (const [id, entry] of Object.entries(table(version).blocks)) {
                if (entry.speedFactor === '-') continue;
                if (entry.speedFactor !== '1.0') slow.push(`${id} ${entry.speedFactor}`);
                if (entry.jumpFactor !== '1.0') low.push(`${id} ${entry.jumpFactor}`);
            }
            if (compareReleases(version, '1.15') < 0) continue;
            assert.deepEqual(slow.sort(), ['honey_block 0.4', 'soul_sand 0.4'], version);
            assert.deepEqual(low, ['honey_block 0.5'], version);
        }
    });

    it('has no factor as a method before 1.15, where soul sand slows from entityInside', () => {
        assert.equal(table('1.14.4').blocks.soul_sand.speedFactor, '-');
        assert.equal(releases['1.14.4'].callbacks.entityInside.soul_sand, 'SoulsandBlock');
        assert.equal(releases['1.15'].callbacks.entityInside?.soul_sand, undefined);
        assert.match(simulation, /BlockEffect::SoulSand if !release\.at_least\(1, 15, 0\)/);
    });

    it('holds the engine to the same two blocks and the same numbers', () => {
        assert.match(simulation, /BlockEffect::SoulSand \| BlockEffect::Honey => 0\.4_f32,\s+_ => 1\.0,/);
        assert.match(simulation, /fn block_jump_factor[\s\S]*?BlockEffect::Honey[\s\S]*?0\.5\s+\} else \{\s+1\.0/);
        assert.match(blocks, /case 'soul_sand': return EFFECT\.soulSand/);
        assert.match(blocks, /case 'honey_block': return EFFECT\.honey/);
    });
});

describe('the order Entity.move runs block callbacks in', () => {
    const audit = generated('physics-methods.json').releases;
    const orderOf = version => moveOrder(moveOf(audit[version]).source);
    /** Each release's order, as the first release that has it: a new one fails here. */
    // (1.8.8 lands as later releases do, Block.onLanded under Mojang's name updateEntityAfterFallOn: the yarn trees this
    // was read from before named it otherwise until 1.13.2, which read as a step added there)
    const ERAS = [
        ['1.8.8', ['fallDamage', 'afterFallOn', 'stepOn', 'inside']],
        ['1.14.4', ['collide', 'fallDamage', 'afterFallOn', 'stepOn', 'inside']],
        ['1.15', ['collide', 'fallDamage', 'afterFallOn', 'stepOn', 'inside', 'speedFactor']],
        ['1.21.2', ['collide', 'fallDamage', 'afterFallOn', 'speedFactor']],
        ['26.2', ['collide', 'fallDamage', 'speedFactor']],
    ];

    it('changes where the source changes it, and nowhere else', () => {
        const found = [];
        for (const version of Object.keys(audit)) {
            const order = orderOf(version);
            if (JSON.stringify(found.at(-1)?.[1]) !== JSON.stringify(order)) found.push([version, order]);
        }
        assert.deepEqual(found, ERAS);
    });

    it('reads Mojang\'s and Yarn\'s names as the same steps, and repeats in place once', () => {
        const mojang = 'void move() { this.collide(a); this.checkFallDamage(b); block.updateEntityMovementAfterFallOn(c); block.stepOn(d); this.tryCheckInsideBlocks(); this.getBlockSpeedFactor(); }';
        const yarn = 'void move() { this.adjustMovementForCollisions(a); this.fall(b); block.onEntityLand(c); block.onSteppedOn(d); this.checkBlockCollision(); this.getVelocityMultiplier(); }';
        assert.deepEqual(moveOrder(mojang), moveOrder(yarn));
        assert.deepEqual(moveOrder('void f() { if (a) { block.stepOn(x); } else { block.stepOn(y); } this.collide(z); }'), ['stepOn', 'collide']);
    });

    it('keeps speed after the fall, on every release that reads it in move', () => {
        for (const version of Object.keys(audit)) {
            const order = orderOf(version);
            if (order.includes('speedFactor')) assert.ok(order.indexOf('fallDamage') < order.indexOf('speedFactor'), version);
        }
    });
});

describe('what only a built entity knows (B041)', () => {
    const before = version => compareReleases(version, '1.20.5') < 0;

    it('builds nearly all of each release, and names the ones it cannot, with why', () => {
        for (const version of exact) {
            const { live, noLive } = releases[version];
            const built = Object.values(live).filter(entity => entity.eye !== undefined).length;
            assert.ok(built >= 90, `${version} built ${built}`);
            assert.ok(Object.keys(noLive).length > 0, version);
            for (const [id, reason] of Object.entries(noLive)) assert.match(reason, /^[A-Za-z]+$/, `${version} ${id}`);
            for (const id of Object.keys(releases[version].entities)) assert.ok(live[id] || noLive[id], `${version} ${id} is neither built nor refused`);
        }
    });

    it('reads a dump, an attachment and a refusal, and fails on an unknown line or nothing built', () => {
        const parsed = parseLive(['live\tpig\t0.765\t0.765\t0.675\t0.14\t-', 'attachment\tpig\t0\t0.0\t0.86\t0.0', 'riderpoint\tcow\t0.0\t1.0\t0.0', 'nolive\twither\tNullPointerException', 'playereye\tSTANDING\t1.62'].join('\n'), 'x');
        assert.deepEqual(parsed.live.pig, { eye: 0.765, eyeCrouching: 0.765, passengersRidingOffset: 0.675, myRidingOffset: 0.14, maxPassengers: null, attachments: [[0, 0.86, 0]] });
        assert.equal(parsed.noLive.wither, 'NullPointerException');
        assert.equal(parsed.playerEyes.STANDING, 1.62);
        assert.throws(() => parseLive('nolive\tpig\tX', 'x'), /no entity could be built/);
        assert.throws(() => parseLive('live\tpig\t1\t1\t1\t1\t-\nnonsense\tx', 'x'), /unknown live line/);
    });

    it('has the eye heights of every release before 1.20.5, values a static table would not', () => {
        for (const version of exact.filter(before)) {
            assert.equal(f32(releases[version].live.pig.eye), f32(0.765), version);
            assert.equal(f32(releases[version].live.zombie.eye), f32(1.74), version);
        }
        assert.equal(f32(releases['1.14.4'].live.boat.eye), f32(0.478125), 'an eye below 0.85 of its height');
        assert.equal(f32(releases['1.20.4'].live.boat.eye), f32(0.5625));
    });

    it('holds the adapter\'s player eyes to Player.getStandingEyeHeight before 1.20.5', () => {
        for (const version of exact.filter(before)) {
            const { playerEyes } = releases[version];
            assert.equal(f32(playerEyes.STANDING), f32(1.62), version);
            assert.equal(f32(playerEyes.SWIMMING), f32(0.4), version);
            assert.equal(f32(playerEyes.FALL_FLYING), f32(0.4), version);
            assert.equal(f32(playerEyes.SPIN_ATTACK), f32(0.4), version);
            assert.equal(f32(playerEyes.CROUCHING ?? playerEyes.SNEAKING), f32(1.27), version);
            assert.equal(f32(playerEyes.DYING), f32(1.62), version);
        }
        const source = fs.readFileSync(path.join(root, 'src', 'mineflayer', 'player.ts'), 'utf8');
        assert.match(source, /eyeHeight = 1\.27;/);
        assert.match(source, /eyeHeight = 1\.62;/);
        assert.match(source, /eyeHeight = 0\.4;/);
    });

    it('has a rider\'s point by era: an offset to 1.20.1, a method on a live entity to 1.20.4, a table from 1.20.5', () => {
        assert.equal(f32(releases['1.16.5'].live.boat.passengersRidingOffset), f32(-0.1));
        assert.equal(f32(releases['1.20.1'].live.boat.passengersRidingOffset), f32(-0.1));
        assert.deepEqual(releases['1.20.4'].live.pig.riderPoint.map(f32), [0, f32(0.86875), 0]);
        for (const version of exact.filter(release => !before(release))) {
            const held = releases[version].live.oak_boat ?? releases[version].live.boat;
            assert.deepEqual(held.attachments[0].map(f32), [0, f32(0.5625), 0], version);
        }
    });

    it('holds each rider of a table apart, and says where a table is not the answer', () => {
        const { happy_ghast: ghast, camel } = releases['26.3'].live;
        assert.equal(new Set(ghast.attachments.map(String)).size, ghast.attachments.length);
        assert.ok(ghast.attachments.length >= 4);
        assert.equal(camel.attachments.length, 1);
        assert.equal(camel.riderComputedBy, 'Camel', 'its second rider comes from its index, at runtime');
    });

    it('holds a rider\'s point facing south: a constructor\'s random yaw is no property of the entity', () => {
        for (const version of ['1.20.2', '1.20.4', '1.20.5', '1.21.1']) {
            for (const id of ['ravager', 'trader_llama', 'turtle']) {
                const point = releases[version].live[id]?.riderPoint;
                if (point) assert.equal(Math.abs(point[0]), 0, `${version} ${id}`);
            }
        }
    });

    it('keeps the adapter\'s and the engine\'s boat seat at the jar\'s', () => {
        const vehicle = fs.readFileSync(path.join(root, 'native', 'src', 'vehicle.rs'), 'utf8');
        assert.match(vehicle, /f64::from\(-0\.6_f32\)/, 'the player on a seat, 1.20.2 to 1.20.4');
        assert.equal(f32(releases['1.20.1'].live.boat.passengersRidingOffset), f32(-0.1));
    });
});

/**
 * Each attribute the adapter reads, by the name it asks mineflayer for, with vanilla's name and what the adapter does
 * from a release that has it. A new read of an attribute fails here until it is linked.
 */
const CONSUMED = {
    movementSpeed: 'movement_speed', sneakingSpeed: 'sneaking_speed', playerSneakingSpeed: 'sneaking_speed', safeFallDistance: 'safe_fall_distance', scale: 'scale',
    gravity: 'gravity', stepHeight: 'step_height', jumpStrength: 'jump_strength', movementEfficiency: 'movement_efficiency', waterMovementEfficiency: 'water_movement_efficiency',
    knockbackResistance: 'knockback_resistance', burningTime: 'burning_time', fallDamageMultiplier: 'fall_damage_multiplier', airDragModifier: 'air_drag_modifier', frictionModifier: 'friction_modifier', bounciness: 'bounciness',
};

describe('attributes, from what the adapter reads to the inventory (B044)', () => {
    const source = fs.readFileSync(path.join(root, 'src', 'mineflayer', 'player.ts'), 'utf8');
    const asked = [...new Set([...source.matchAll(/attribute(?:ValueOf)?\((?:state\.)?attributes, '([a-zA-Z]+)'/g)].map(match => match[1]))].sort();
    const inRelease = (version, name) => Object.entries(releases[version].attributes).find(([id]) => id.replace(/^(generic|player|horse)\./, '') === name)?.[1];

    it('links every attribute the adapter reads', () => {
        assert.ok(asked.length >= 15, asked.join());
        assert.deepEqual(asked.filter(name => !CONSUMED[name]), []);
    });

    it('finds each in the jar of the newest release, with its bounds', () => {
        for (const [name, id] of Object.entries(CONSUMED)) {
            const attribute = inRelease('26.3', id);
            assert.ok(attribute, `${name} ${id}`);
            assert.ok(attribute.syncable !== undefined && attribute.min !== null && attribute.max !== null, name);
        }
    });

    it('has the releases that lack one lack it for the adapter too: undefined, not 0', () => {
        assert.equal(inRelease('1.20.4', 'gravity'), undefined);
        assert.equal(inRelease('1.20.6', 'movement_efficiency'), undefined);
        assert.ok(inRelease('1.21', 'movement_efficiency'));
        assert.equal(inRelease('1.21.11', 'air_drag_modifier'), undefined);
        assert.match(source, /gravity: this\.attributeValueOf\(attributes, 'gravity'\)/);
    });
});

describe('enchantments and equipment (B044)', () => {
    const enchant = (version, name) => releases[version].enchantments[name];

    it('reads classes before 1.21 and data from it', () => {
        for (const version of exact) {
            const { attributes } = enchant(version, 'depth_strider');
            assert.equal(attributes.length > 0, compareReleases(version, '1.21') >= 0, version);
        }
    });

    it('moves depth strider from any armour to the boots at 1.21, as the adapter does', () => {
        assert.deepEqual(enchant('1.20.6', 'depth_strider').slots.map(String).sort(), ['CHEST', 'FEET', 'HEAD', 'LEGS']);
        assert.deepEqual(enchant('1.21', 'depth_strider').slots, ['feet']);
        const player = fs.readFileSync(path.join(root, 'src', 'mineflayer', 'player.ts'), 'utf8');
        assert.match(player, /depthStrider: this\.lt\('1\.21'\) \? this\.wornEnchantmentLevel\(state, 'depth_strider', ARMOR_SLOTS\)/);
        assert.match(player, /soulSpeed: this\.lt\('1\.21'\) \? this\.wornEnchantmentLevel\(state, 'soul_speed', \['feet'\]\)/);
        assert.match(player, /wornEnchantmentLevel\(state, 'swift_sneak', \['legs'\]\)/);
        assert.match(player, /wornEnchantmentLevel\(state, 'frost_walker', \['feet'\]\)/);
    });

    it('holds what the adapter assumes of the other slots on every release that has them', () => {
        for (const version of exact) {
            const slot = name => enchant(version, name)?.slots.map(item => String(item).toLowerCase());
            assert.deepEqual(slot('frost_walker'), ['feet'], version);
            if (enchant(version, 'soul_speed')) assert.deepEqual(slot('soul_speed'), ['feet'], version);
            if (enchant(version, 'swift_sneak')) assert.deepEqual(slot('swift_sneak'), ['legs'], version);
        }
    });

    it('from 1.21 ties each to the attribute it modifies, by operation and amount', () => {
        for (const version of exact.filter(release => compareReleases(release, '1.21') >= 0)) {
            const [depth] = enchant(version, 'depth_strider').attributes;
            assert.equal(depth.attribute.replace(/^generic\./, ''), 'water_movement_efficiency', version);
            assert.equal(depth.operation, 'add_value', version);
            assert.equal(f32(depth.amount.base), f32(0.33333334), version);
            assert.equal(f32(enchant(version, 'depth_strider').maxLevel), 3, version);
            const [soul] = enchant(version, 'soul_speed').attributes ?? [];
            if (soul) assert.ok(soul.attribute, version);
        }
    });

});

/** Entity.positionRider itself, with a zombie on a vehicle at the origin: LiveEntityDump.riderCases, LegacyEntityDump. */
describe('where a rider sits, turned and scaled (B041)', () => {
    const all = Object.keys(releases);
    const boatOf = version => releases[version].live.oak_boat ?? releases[version].live.boat;
    const seat = (entity, slot, yaw, size = 1) => entity?.riderCases?.find(([s, y, z]) => s === slot && y === yaw && z === size)?.slice(3);
    const { LoadNativePhysics } = require('..');
    const native = LoadNativePhysics();
    const boatState = yaw => ({ position: { x: 0, y: 0, z: 0 }, velocity: { x: 0, y: 0, z: 0 }, yaw, forward: false, left: false, right: false, inWater: true });

    it('covers every registered release, each with its own jar', () => {
        assert.equal(all.length, 50);
        for (const version of all) {
            assert.match(releases[version].status, /^(exact|legacy)$/, version);
            assert.match(releases[version].client, /^[0-9a-f]{40}$/, version);
            assert.ok(boatOf(version)?.riderCases?.length, `${version} has a boat's seats`);
        }
    });

    it('sizes the legacy releases by their own entities, and names what would not build', () => {
        const size = (version, id) => releases[version].live[id].size.map(f32);
        assert.deepEqual(size('1.8.8', 'boat'), [f32(1.5), f32(0.6)]);
        assert.deepEqual(size('1.9.4', 'boat'), [f32(1.375), f32(0.5625)]);
        assert.deepEqual(size('1.13.2', 'boat'), [f32(1.375), f32(0.5625)]);
        for (const version of all.filter(name => releases[name].status === 'legacy')) {
            assert.ok(Object.keys(releases[version].live).length >= 55, version);
            for (const reason of Object.values(releases[version].noLive)) assert.match(reason, /^[A-Za-z]+$/, version);
        }
    });

    it('keeps 1.8\'s seat at the boat\'s nose, in double arithmetic', () => {
        for (const yaw of [0, 37.5, 90]) {
            const radians = yaw * Math.PI / 180;
            const [x, , z] = seat(releases['1.8.8'].live.boat, 0, yaw);
            assert.equal(x, Math.cos(radians) * 0.4, `x at ${yaw}`);
            assert.equal(z, Math.sin(radians) * 0.4, `z at ${yaw}`);
        }
    });

    it('has the engine\'s boat seat at the jar\'s, at each yaw and with one or two aboard', () => {
        for (const version of all) {
            for (const yaw of [37.5, 90]) {
                const alone = version === '1.8.8' ? 0 : -1;
                for (const [slot, passengers] of version === '1.8.8' ? [[alone, 1]] : [[alone, 1], [0, 2]]) {
                    const point = seat(boatOf(version), slot, yaw);
                    const { position } = native.boatPassengerPlacement(version, boatState(yaw * Math.PI / 180), 0, false, false, passengers);
                    assert.equal(position.x, point[0] + 0, `${version} x, yaw ${yaw}, ${passengers} aboard`);
                    assert.equal(position.z, point[2] + 0, `${version} z, yaw ${yaw}, ${passengers} aboard`);
                }
            }
        }
    });

    it('turns a seat off the middle with its vehicle, and leaves one on it where it is', () => {
        for (const version of all.filter(name => compareReleases(name, '1.9.4') >= 0)) {
            const boat = boatOf(version);
            const [x0, , z0] = seat(boat, 1, 0);
            const [x, , z] = seat(boat, 1, 90);
            assert.ok(Math.abs(Math.hypot(x, z) - Math.hypot(x0, z0)) < 1e-6, version);
            assert.ok(Math.abs(x) > 0.59, `${version}: the back seat swings to the side`);
            assert.deepEqual(seat(boat, -1, 0).map(f32), seat(boat, -1, 90).map(f32), `${version}: alone is on the axis`);
        }
    });

    it('holds each rider apart: a boat\'s two, a happy ghast\'s four', () => {
        for (const version of all.filter(name => compareReleases(name, '1.9.4') >= 0)) {
            assert.notDeepEqual(seat(boatOf(version), 0, 0), seat(boatOf(version), 1, 0), version);
        }
        const ghast = releases['26.3'].live.happy_ghast;
        assert.equal(new Set([0, 1, 2, 3].map(slot => String(seat(ghast, slot, 37.5)))).size, 4);
        for (const version of all.filter(name => releases[name].live.camel?.riderCases)) {
            const camel = releases[version].live.camel;
            assert.notDeepEqual(seat(camel, 0, 0), seat(camel, 1, 0), version);
        }
    });

    it('scales a table with the scale attribute, from 1.20.5', () => {
        const scaled = all.filter(version => compareReleases(version, '1.20.5') >= 0);
        for (const version of scaled) {
            const horse = releases[version].live.horse;
            const [, one] = seat(horse, 0, 0);
            const [, two] = seat(horse, 0, 0, 2);
            assert.ok(Math.abs((two - one) - horse.attachments[0][1]) < 1e-6, `${version}: twice the size adds one more seat height`);
        }
        for (const version of all.filter(name => compareReleases(name, '1.20.5') < 0)) {
            assert.equal(seat(releases[version].live.horse, 0, 0, 2), undefined, `${version} has no scale attribute`);
        }
    });

    it('reproduces each table at a turn, where a table is the whole answer', () => {
        const { live } = releases['26.3'];
        for (const id of ['horse', 'donkey', 'ravager']) {
            const [slot] = live[id].attachments;
            const [x0, y0, z0] = seat(live[id], 0, 0);
            const [x, y, z] = seat(live[id], 0, 90);
            assert.equal(y, y0, id);
            // Vec3.yRot of the rider's point at 90 degrees: z goes to x
            assert.ok(Math.abs(x - (z0 - slot[2] + slot[2])) < 1e-6 || Math.abs(x + z0) < 1e-6, `${id} x`);
            assert.ok(Math.abs(z) < 1e-6 || Math.abs(z - x0) < 1e-6, `${id} z`);
        }
    });

    it('a baby rides its own scale', () => {
        const { pig } = releases['1.21.1'].live;
        const adult = seat(pig, 0, 37.5);
        const baby = seat(pig, 0, 37.5, 'baby');
        assert.ok(baby[1] < adult[1]);
    });

    it('names the vehicles a blank level cannot build, each with a table from 1.20.5', () => {
        for (const version of all.filter(name => compareReleases(name, '1.21.11') >= 0)) {
            const { live, noLive } = releases[version];
            for (const id of ['pig', 'minecart']) {
                assert.ok(noLive[id], `${version} ${id}: seats no rider here, and says so`);
                assert.ok(live[id].attachments?.length, `${version} ${id} keeps its table`);
            }
        }
    });
});

describe('a legacy size is the constructor\'s (B041)', () => {
    it('leaves an entity sized later at Entity\'s own default, which the docs name', () => {
        for (const version of ['1.8.8', '1.12.2', '1.13.2']) {
            assert.deepEqual(releases[version].live.slime.size.map(f32), [f32(0.6), f32(1.8)], version);
        }
    });
});

describe('a camel seats two in every release that has one (B041)', () => {
    it('has its riders from its own method, whose seat follows its animation', () => {
        const camels = Object.keys(releases).filter(version => releases[version].live?.camel);
        assert.ok(camels.length >= 26);
        for (const version of camels) assert.ok(releases[version].live.camel.riderCases?.length, version);
    });
});
