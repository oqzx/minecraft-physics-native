'use strict';

// B042, B152, B153, B154, B164: every block state of a release through the real BlockMapper, against what the release's
// own code answered (generated/block-shapes). Nothing here reads a shape the adapter could have read too: its boxes, its
// faces and its predicates each meet vanilla's. The handwritten sets meet the release's own tags.

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const zlib = require('node:zlib');
const { describe, it } = require('node:test');
const adapter = require('..');
const native = require('../native/artifacts/native.js');

const root = path.resolve(__dirname, '..');
const generated = name => JSON.parse(fs.readFileSync(path.join(root, 'generated', name), 'utf8'));
const report = generated('block-shapes.json').releases;
const tags = generated('block-tags.json').releases;
const table = version => JSON.parse(zlib.gunzipSync(fs.readFileSync(path.join(root, 'generated', 'block-shapes', `${version}.json.gz`))).toString('utf8'));
const at = (values, index) => (Array.isArray(values) ? values[index] : values);
const stateCount = entry => entry.properties.reduce((product, [, values]) => product * values.length, 1);
const STEPS = { n: [0, -1], s: [0, 1], w: [-1, 0], e: [1, 0] };
const DIRECTIONS = ['d', 'u', 'n', 's', 'w', 'e'];
const FENCE_LIKE = 16;
const WALL_OR_GATE = 32;

// every release with a table: 1.19.1, 1.21.2, 1.21.7, 1.21.10, 26.1.1 and 26.1.2 on another release's minecraft-data.
const RELEASES = Object.keys(report).filter(version => report[version].status === 'exact');

function mapperOf(version) {
    const registry = require('prismarine-registry')(version);
    const Block = require('prismarine-block')(registry);
    const mapper = new adapter.BlockMapper(registry.version, registry, (name, tag) => native.generatedBlockHasTag(version, name, tag),
        name => native.generatedMethodShapeBoxes(version, name, 'outline'), (name, index, kind) => native.generatedStateShape(version, name, kind, index));
    return { mapper, Block };
}

/**
 * A tag with any experimental pack on. The adapter goes by name, so a block only a pack adds passes as tagged in either
 * configuration: it cannot be in the world of a server without the pack, and is in the tag on one with it.
 */
function taggedAnyPack(version, tag) {
    const added = new Set(Object.values(tags[version].adds ?? {}).flatMap(pack => pack[tag] ?? []));
    return name => added.has(name) || native.generatedBlockHasTag(version, name, tag);
}

/** Calls `check(state)` for every state of a release. */
function eachState(version, check) {
    const { shapes, blocks } = table(version);
    const { mapper, Block } = mapperOf(version);
    for (const [name, entry] of Object.entries(blocks)) {
        for (let index = 0; index < stateCount(entry); index += 1) {
            const described = mapper.describe(Block.fromStateId(entry.firstId + index, 0));
            check({
                name, entry, index, mapper, described,
                label: `${version} ${name} ${index}`,
                boxes: channel => shapes[at(entry[channel], index)] ?? null,
                depends: channel => at(entry[`${channel}Depends`], index),
                flags: at(entry.flags, index),
                sturdy: at(entry.sturdy, index),
            });
        }
    }
}

describe('every state through the adapter', () => {
    for (const version of RELEASES) {
        it(`${version}: collision, support, sturdy faces, suffocation, friction, fluid`, () => {
            eachState(version, ({ label, name, entry, mapper, described, boxes, depends, flags, sturdy }) => {
                assert.equal(described.name, name, label);
                if (depends('collision') === 0) {
                    assert.deepEqual(described.boxes, boxes('collision'), `${label} collision`);
                    // what the engine is handed, past every step after describe (boxes1_16 on 1.16, 1.16.1)
                    const at = (x, y, z) => (x === 0 && y === 0 && z === 0 ? described : null);
                    assert.deepEqual(mapper.boxesAt(at, 0, 0, 0, described), boxes('collision'), `${label} boxesAt`);
                }
                assert.equal(Boolean(described.fluid), !flags.endsWith(':EmptyFluid'), `${label} fluid`);
                // (the engine reads it as the float it is: test/block-friction.test.cjs has the bits)
                assert.equal(Math.fround(described.friction), Math.fround(Number(entry.friction)), `${label} friction`);
                if (depends('collision') !== 0) return;
                const support = boxes('support');
                if (support && depends('support') === 0) {
                    assert.deepEqual(mapper.supportShape(described), support, `${label} support`);
                    assert.equal(mapper.bottomFaceSturdy(described), sturdy[0] === '1', `${label} sturdy d`);
                }
                if (depends('support') === 0) {
                    for (const direction of ['n', 's', 'w', 'e']) {
                        const oracle = sturdy[DIRECTIONS.indexOf(direction)] === '1';
                        // (the dump is a jar with no server, a client alone: on 1.15 MinecraftServer rebuilds BlockState's cache after
                        // the data packs load, so in a world with a server, the recorded one, #leaves are not sturdy: blocks.ts flowBlocksOf)
                        const leavesRebuilt = /^1\.15(\.|$)/.test(version) && name.endsWith('_leaves');
                        if (leavesRebuilt) assert.equal(oracle, boxes('collision')?.length === 1, `${label} sturdy ${direction}, the dump's`);
                        assert.equal(mapper.flowBlocks.faceSturdy(described, STEPS[direction]), leavesRebuilt ? false : oracle, `${label} sturdy ${direction}`);
                    }
                }
                if (flags[2] === '-') return;
                const updates = mapper.updatesAt((x, y, z) => (x === 0 && y === 0 && z === 0 ? described : null), 0, 0, 0);
                const full = described.boxes.length === 1 && described.boxes[0].join() === '0,0,0,1,1,1';
                assert.equal(updates[0]?.suffocating ?? full, flags[2] === '1', `${label} suffocating`);
            });
        });
    }
});

describe('the outlines the adapter sends', () => {
    /** The outline a state's updates leave, empty boxes dropped, as the engine reads them. */
    const sent = (updates) => updates.flatMap(({ outlineShape, shape }) => {
        const box = outlineShape ?? shape;
        return box ? [[box.minX, box.minY, box.minZ, box.maxX, box.maxY, box.maxZ]] : [];
    }).filter(([x0, y0, z0, x1, y1, z1]) => x1 > x0 && y1 > y0 && z1 > z0);

    for (const version of RELEASES) {
        it(`${version}: every static outline, a sapling's and a fence's among them`, () => {
            eachState(version, ({ label, mapper, described, boxes, depends }) => {
                if (depends('outline') !== 0 || depends('collision') !== 0) return;
                const updates = mapper.updatesAt((x, y, z) => (x === 0 && y === 0 && z === 0 ? described : null), 0, 0, 0);
                assert.deepEqual(sent(updates), boxes('outline'), label);
            });
        });
    }

    for (const version of RELEASES) {
        it(`${version}: every static interaction shape`, () => {
            let some = 0;
            eachState(version, ({ label, mapper, described, boxes, depends }) => {
                if (depends('interaction') !== 0) return;
                const want = boxes('interaction') ?? [];
                const updates = mapper.updatesAt((x, y, z) => (x === 0 && y === 0 && z === 0 ? described : null), 0, 0, 0);
                const got = updates.flatMap(({ interactionShape: box }) => (box ? [[box.minX, box.minY, box.minZ, box.maxX, box.maxY, box.maxZ]] : []))
                    .filter(([x0, y0, z0, x1, y1, z1]) => x1 > x0 && y1 > y0 && z1 > z0);
                assert.deepEqual(got, want, label);
                some += want.length > 0 ? 1 : 0;
            });
            assert.ok(some > 0, 'the channel has shapes to compare');
        });
    }

    // NativeWorld refuses a batch whole, and the adapter then leaves the block out: every state has to get through
    for (const version of RELEASES) {
        it(`${version}: the engine takes the updates of every state`, () => {
            const world = new native.NativeWorld();
            eachState(version, ({ mapper, described }) => {
                const updates = mapper.updatesAt((x, y, z) => (x === 0 && y === 0 && z === 0 ? described : null), 0, 0, 0);
                if (updates.length > 0) world.applyBlockUpdates(updates);
            });
        });
    }

    // a lectern's outline reaches 18/16 of its cell (26.3 LecternBlock.java:231): g_step_*_lectern, without a replay
    for (const version of ['1.14.4', '1.20.4', '1.21.4', '1.21.11', '26.3']) {
        it(`${version}: a player steps up onto a lectern`, () => {
            const { mapper, Block } = mapperOf(version);
            const lectern = mapper.describe(Block.fromStateId(table(version).blocks.lectern.firstId, 0));
            const at = (x, y, z) => (x === 1 && y === 100 && z === 0 ? lectern : null);
            const world = new native.NativeWorld();
            world.applyBlockUpdates([
                ...[0, 1, 2].map(x => ({ x, y: 99, z: 0, id: 1 })),
                ...mapper.updatesAt(at, 1, 100, 0),
            ]);
            const stepped = native.simulateVersionedTick(version, world, {
                position: { x: 0.65, y: 100, z: 0.5 }, velocity: { x: 0.2, y: 0, z: 0 }, halfWidth: 0.3, height: 1.8,
                onGround: true, collidedHorizontally: false, collidedVertically: false, isPlayer: true,
            }, {});
            assert.equal(stepped.position.y, 100.125, version);
        });
    }

    it('gives a plant no collision and an outline, by state', () => {
        const { mapper, Block } = mapperOf('1.16');
        const sapling = mapper.describe(Block.fromStateId(table('1.16').blocks.oak_sapling.firstId, 0));
        assert.deepEqual(sapling.boxes, []);
        const [update] = mapper.updatesAt(() => sapling, 0, 0, 0);
        assert.equal(update.outlineShape.maxY, 0.8125 - 0.0625);
    });
});

describe('the sets the adapter decides by, against each release\'s tags', () => {
    for (const version of ['1.14.4', '1.16', '1.19.4', '1.20.3', '1.21.3', '1.21.11', '26.3']) {
        it(`${version}: fences, walls and gates, where a block's tag may be an experimental pack's`, () => {
            const fence = taggedAnyPack(version, 'fences');
            const wall = taggedAnyPack(version, 'walls');
            eachState(version, ({ label, name, entry, mapper, described }) => {
                const gate = entry.class === 'FenceGateBlock';
                const bits = mapper.updatesAt((x, y, z) => (x === 0 && y === 0 && z === 0 ? described : null), 0, 0, 0).reduce((all, update) => all | (update.tags ?? 0), 0);
                assert.equal(Boolean(bits & FENCE_LIKE), fence(name) || wall(name) || gate, `${label} fence-like`);
                assert.equal(Boolean(bits & WALL_OR_GATE), wall(name) || gate, `${label} wall or gate`);
            });
        });

        it(`${version}: leaves and climbables`, () => {
            const climbable = taggedAnyPack(version, 'climbable');
            const leaves = taggedAnyPack(version, 'leaves');
            eachState(version, ({ label, name, described, entry }) => {
                assert.equal(name.endsWith('_leaves'), leaves(name), `${label} leaves`);
                if (tags[version].default.climbable && entry.class !== 'TrapDoorBlock') {
                    assert.equal(described.effect === 5 || described.effect === 21 || name === 'scaffolding', climbable(name), `${label} climbable`);
                }
            });
        });
    }

    it('a waterlogged wall with no collision keeps its tag', () => {
        const { mapper, Block } = mapperOf('26.3');
        const { blocks } = table('26.3');
        const wall = blocks.cobblestone_wall;
        for (let index = 0; index < stateCount(wall); index += 1) {
            const described = mapper.describe(Block.fromStateId(wall.firstId + index, 0));
            if (described.boxes.length > 0 || !described.fluid) continue;
            const [update] = mapper.updatesAt(() => described, 0, 0, 0);
            assert.equal(update.tags, FENCE_LIKE | WALL_OR_GATE);
            return;
        }
        assert.fail('no such state');
    });
});

describe('counterexamples: what one flag cannot say', () => {
    /** The answers of a block's first state in a release. */
    const state = (version, id) => {
        const { shapes, blocks } = table(version);
        const entry = blocks[id];
        return {
            collision: shapes[at(entry.collision, 0)], outline: shapes[at(entry.outline, 0)], support: shapes[at(entry.support, 0)],
            sturdy: at(entry.sturdy, 0), suffocating: at(entry.flags, 0)[2] === '1', fluid: at(entry.flags, 0).split(':')[1],
        };
    };

    it('a sign has an outline and no collision, and is no support', () => {
        const sign = state('26.3', 'oak_sign');
        assert.deepEqual(sign.collision, []);
        assert.ok(sign.outline.length > 0);
        assert.equal(sign.sturdy, '000000');
        assert.equal(sign.suffocating, false);
    });

    it('leaves collide as a full cube, support nothing, and do not suffocate', () => {
        const leaves = state('26.3', 'oak_leaves');
        assert.deepEqual(leaves.collision, [[0, 0, 0, 1, 1, 1]]);
        assert.deepEqual(leaves.support, []);
        assert.equal(leaves.sturdy, '000000');
        assert.equal(leaves.suffocating, false);
    });

    it('a layer of snow collides with nothing and still supports its own height', () => {
        const snow = state('26.3', 'snow');
        assert.deepEqual(snow.collision, []);
        assert.deepEqual(snow.support, [[0, 0, 0, 1, 0.125, 1]]);
        assert.equal(snow.sturdy, '100000');
    });

    it('a carpet is a low collision with a sturdy bottom and no sturdy top', () => {
        const carpet = state('26.3', 'white_carpet');
        assert.deepEqual(carpet.collision, [[0, 0, 0, 1, 0.0625, 1]]);
        assert.equal(carpet.sturdy, '100000');
        assert.equal(carpet.suffocating, false);
    });

    it('stone is all of them at once, which is why none of them can stand for another', () => {
        const stone = state('26.3', 'stone');
        assert.deepEqual(stone.collision, [[0, 0, 0, 1, 1, 1]]);
        assert.equal(stone.sturdy, '111111');
        assert.equal(stone.suffocating, true);
    });

    it('leaves: sturdy in a client alone to 1.15, and in a world with a server only on 1.14, whose cache nothing rebuilds', () => {
        // (the dump has no server; from 1.15 MinecraftServer rebuilds the cache after the data packs load, physrec/ff_oak_leaves_persistent_true)
        for (const version of ['1.14.4', '1.15', '1.15.1', '1.15.2']) assert.equal(state(version, 'oak_leaves').sturdy, '111111', version);
        for (const version of ['1.16', '1.16.5', '26.3']) assert.equal(state(version, 'oak_leaves').sturdy, '000000', version);
        const sturdy = (version) => {
            const { mapper, Block } = mapperOf(version);
            return mapper.flowBlocks.faceSturdy(mapper.describe(Block.fromStateId(table(version).blocks.oak_leaves.firstId, 0)), STEPS.n);
        };
        assert.equal(sturdy('1.14.4'), true);
        assert.equal(sturdy('1.15.2'), false);
    });

    it('the mud of 1.19 and up, a low collision with a full support', () => {
        const mud = state('1.19.4', 'mud');
        assert.deepEqual(mud.collision, [[0, 0, 0, 1, 0.875, 1]]);
        assert.deepEqual(mud.support, [[0, 0, 0, 1, 1, 1]]);
    });

    it('friction keeps its zero-free defaults and its exceptions at their bits', () => {
        const { blocks } = table('26.3');
        assert.equal(Math.fround(Number(blocks.blue_ice.friction)), Math.fround(0.989));
        assert.equal(Math.fround(Number(blocks.ice.friction)), Math.fround(0.98));
        assert.equal(Math.fround(Number(blocks.slime_block.friction)), Math.fround(0.8));
        assert.equal(Math.fround(Number(blocks.stone.friction)), Math.fround(0.6));
        assert.equal(blocks.soul_sand.speedFactor, '0.4');
        assert.equal(blocks.honey_block.jumpFactor, '0.5');
    });
});

describe('every release\'s friction, against generated/block-friction.json', () => {
    const friction = generated('block-friction.json').releases;
    for (const version of Object.keys(report).filter(version => report[version].status === 'exact')) {
        it(`${version} is the registry's reading, every block`, () => {
            const { blocks } = table(version);
            const { default: fallback, frictions } = friction[version];
            for (const [id, entry] of Object.entries(blocks)) {
                assert.equal(Math.fround(Number(entry.friction)), Math.fround(frictions[id]?.value ?? fallback.value), `${version} ${id}`);
            }
        });
    }
});

describe('the getCollisionBoundingBox the 1.10 - 1.12 auto-jump probes (D-196)', () => {
    // 1.12.2 Block.getCollisionShape returns getShape, which fences, the brewing stand, the end portal frame and the end
    // portal keep, overriding only appendCollisionBoxes; BushBlock's is none, the lily pad's too (native/src/world.rs
    // LEGACY_PROBE_OUTLINE, LEGACY_PROBE_NONE)
    const tagsOf = (version, name) => {
        const { mapper, Block } = mapperOf(version);
        const block = new Block(require('prismarine-registry')(version).blocksByName[name].id, 0, 0);
        const described = mapper.describe(block);
        return mapper.updatesAt((x, y, z) => (x === 0 && y === 0 && z === 0 ? described : null), 0, 0, 0).map(update => update.tags ?? 0);
    };
    it('marks the outline blocks and the lily pad before 1.13 only', () => {
        for (const version of ['1.10.2', '1.12.2']) {
            for (const name of ['fence', 'nether_brick_fence', 'brewing_stand', 'end_portal_frame', 'end_portal']) {
                assert.ok(tagsOf(version, name).every(tags => (tags & 64) !== 0), `${version} ${name}`);
            }
            assert.ok(tagsOf(version, 'waterlily').every(tags => (tags & 128) !== 0), `${version} waterlily`);
            for (const name of ['cobblestone_wall', 'fence_gate', 'stone', 'cactus', 'snow_layer']) {
                assert.ok(tagsOf(version, name).every(tags => (tags & 192) === 0), `${version} ${name}`);
            }
            assert.ok(tagsOf(version, 'fence').every(tags => (tags & 16) !== 0), `${version} fence stays fence-like`);
        }
        for (const name of ['oak_fence', 'brewing_stand', 'lily_pad']) assert.ok(tagsOf('1.13.2', name).every(tags => (tags & 192) === 0), name);
    });

    // World.isBlockFullCube before 1.13 (native/src/world.rs LEGACY_FULL_CUBE): the jar's answer for a state alone, a
    // stair's, a cauldron's and a hopper's whole-cell getBoundingBox included; soul sand's own box is 14/16 high
    const fullOf = (version, name, at = (x, y, z, described) => (x === 0 && y === 0 && z === 0 ? described : null)) => {
        const { mapper, Block } = mapperOf(version);
        const described = mapper.describe(new Block(require('prismarine-registry')(version).blocksByName[name].id, 0, 0));
        return mapper.updatesAt((x, y, z) => at(x, y, z, described), 0, 0, 0).every(update => ((update.tags ?? 0) & 256) !== 0);
    };
    it('marks a full cube of World.isBlockFullCube before 1.13 only', () => {
        for (const version of ['1.8.8', '1.12.2']) {
            for (const name of ['stone', 'oak_stairs', 'cauldron', 'hopper']) assert.ok(fullOf(version, name), `${version} ${name}`);
            for (const name of ['soul_sand', 'cactus', 'stone_slab', 'fence']) assert.ok(!fullOf(version, name), `${version} ${name}`);
        }
        // a fence joined on all four sides fills its cell's getBoundingBox, one on two does not
        const { mapper, Block } = mapperOf('1.12.2');
        const fence = mapper.describe(new Block(require('prismarine-registry')('1.12.2').blocksByName.fence.id, 0, 0));
        const around = (cells) => mapper.updatesAt((x, y, z) => (x === 0 && y === 0 && z === 0) || cells.some(([a, c]) => a === x && y === 0 && c === z) ? fence : null, 0, 0, 0)
            .every(update => ((update.tags ?? 0) & 256) !== 0);
        assert.ok(around([[1, 0], [-1, 0], [0, 1], [0, -1]]));
        assert.ok(!around([[1, 0], [-1, 0]]));
        assert.ok(!fullOf('1.13.2', 'oak_stairs'));
    });
});
