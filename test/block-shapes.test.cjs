'use strict';

// B078, B079, B080, B152, B153: every block state's geometry as each release's own code answers it
// (scripts/java-oracle/ShapeDump.java), checked against what came from elsewhere: the running clients' block lists and
// minecraft-data, the adapter's input. Each check here has a fixture with the defect put back.

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const zlib = require('node:zlib');
const { execFileSync } = require('node:child_process');
const { createRequire } = require('node:module');
const { describe, it } = require('node:test');
const { parseDump, buildTable, stateDomains, parseBoxes, parseContext, coverage, againstClient, againstCollisionTable, DEPENDS, CHANNELS } = require('../scripts/generate-block-shapes.cjs');
const { registeredReleases } = require('../scripts/registered-releases.cjs');
const native = require('../native/artifacts/native.js');

const root = path.resolve(__dirname, '..');
const generated = name => JSON.parse(fs.readFileSync(path.join(root, 'generated', name), 'utf8'));
const report = generated('block-shapes.json').releases;
const runtime = generated('runtime-registries.json').releases;
const table = version => JSON.parse(zlib.gunzipSync(fs.readFileSync(path.join(root, 'generated', 'block-shapes', `${version}.json.gz`))).toString('utf8'));
const exact = Object.keys(report).filter(version => report[version].status === 'exact');
const at = (values, index) => (Array.isArray(values) ? values[index] : values);
const stateCount = entry => entry.properties.reduce((product, [, values]) => product * values.length, 1);

/** What the Rust side gives: `[x0, y0, z0, x1, y1, z1]` boxes. */
const boxesOf = shape => shape.boxes.map(box => [box.minX, box.minY, box.minZ, box.maxX, box.maxY, box.maxZ]);
const tuple = (entry, index) => {
    let rest = index;
    const values = [];
    for (let at = entry.properties.length - 1; at >= 0; at -= 1) {
        const [name, domain] = entry.properties[at];
        values.unshift({ name, value: domain[rest % domain.length] });
        rest = Math.floor(rest / domain.length);
    }
    return values;
};

const dumpOf = (...lines) => ['tags\tholders', ...lines].join('\n');
const blockLine = (id, className = 'net.minecraft.world.level.block.Block') => ['block', id, className, '0.6', '1.0', '1.0', '0'].join('\t');
const stateLine = (id, index, stateId, props, collision = '', context = '') => ['state', id, index, stateId, props, collision, collision, '', collision, '111111', '0000000:EmptyFluid', context].join('\t');

describe('the report', () => {
    it('accounts for every registered release, an unavailable one by name and reason', () => {
        assert.deepEqual(Object.keys(report).sort(), registeredReleases().map(({ version }) => version).sort());
        assert.deepEqual(Object.keys(report).filter(version => report[version].status !== 'exact'), ['1.8.8', '1.9.4', '1.10.2', '1.11.2', '1.12.2', '1.13.2']);
        for (const version of Object.keys(report)) {
            if (report[version].status === 'exact') continue;
            assert.equal(report[version].status, 'unavailable');
            assert.match(report[version].reason, /CollisionContext/);
        }
    });

    it('has each exact release at the client jar it ran, with its tags bound', () => {
        for (const version of exact) {
            assert.match(report[version].client, /^[0-9a-f]{40}$/, version);
            assert.ok(['holders', 'helper', 'wrappers'].includes(report[version].tags), version);
            if (Number(version.split('.')[0]) < 26) assert.match(report[version].remappedSha256, /^[0-9a-f]{64}$/, version);
        }
    });

    it('counts every state in each channel as exact, contextual or absent', () => {
        for (const version of exact) {
            const { states, channels } = report[version];
            for (const channel of CHANNELS) {
                const { exact: fixed, contextual, absent } = channels[channel];
                assert.equal(fixed + contextual + absent, states, `${version} ${channel}`);
            }
        }
    });

    it('says the support shape is absent before 1.16 and only there', () => {
        assert.equal(report['1.15.2'].channels.support.absent, report['1.15.2'].states);
        assert.equal(report['1.16'].channels.support.absent, 0);
    });
});

describe('the checked-in tables', () => {
    it('are binary to git: normalising a line ending corrupts a gzip stream, which the working copy would not show', () => {
        const attributes = execFileSync('git', ['check-attr', 'text', 'eol', '--', 'generated/block-shapes/26.3.json.gz'], { cwd: root, encoding: 'utf8' });
        assert.match(attributes, /text: unset/);
    });
});

describe('the tables against the running clients', () => {
    for (const version of exact) {
        it(`${version} has the client's blocks and state counts`, () => {
            assert.deepEqual(againstClient(table(version), runtime[version].blocks), []);
        });
    }
});

describe('the tables against minecraft-data, the adapter\'s collision input', () => {
    const dataRoot = (() => {
        try {
            return path.join(path.dirname(createRequire(path.join(root, 'package.json')).resolve('minecraft-data')), 'minecraft-data', 'data');
        } catch {
            return null;
        }
    })();
    const dataPaths = dataRoot ? JSON.parse(fs.readFileSync(path.join(dataRoot, 'dataPaths.json'), 'utf8')).pc : {};
    // Where the data is not the release's: it is pinned, so a new difference fails, and the adapter reads the tables
    // first (test/adapter-block-states.test.cjs). 1.16 and 1.16.1 lay the chain along x and hang the lantern wrong;
    // 1.18 to 1.18.2 are served by 1.17's table.
    const hanging = ['chain', 'lantern', 'soul_lantern'];
    const differing = version => [...new Set(againstCollisionTable(table(version), JSON.parse(fs.readFileSync(path.join(dataRoot, dataPaths[version].blockCollisionShapes, 'blockCollisionShapes.json'), 'utf8'))).map(entry => entry.split(' ')[0]))].sort();
    for (const version of exact) {
        const file = dataPaths[version]?.blockCollisionShapes;
        it(`${version} matches state by state`, { skip: file ? false : 'no minecraft-data of it' }, () => {
            const expected = { '1.16': hanging, '1.16.1': hanging }[version];
            if (/^1\.18/.test(version)) assert.equal(file, 'pc/1.17', 'a table of its own would take this case away');
            else assert.deepEqual(differing(version), expected ?? [], version);
        });
    }
});

describe('the checks catch the defect put back', () => {
    const stone = () => buildTable('x', parseDump(dumpOf(blockLine('stone'), stateLine('stone', 0, 1, '', '0.0,0.0,0.0,1.0,1.0,1.0')), 'x'), { client: 'c' });

    it('a registration the table lost, or one it invented', () => {
        assert.deepEqual(againstClient(stone(), [['stone', 1], ['granite', 1]]), ['missing granite']);
        assert.deepEqual(againstClient(stone(), []), ['extra stone']);
    });

    it('an inherited property the table lost', () => {
        assert.deepEqual(againstClient(stone(), [['stone', 2]]), ['stone has 1 states, the client 2']);
    });

    it('an incorrect shape branch', () => {
        const data = { blocks: { stone: 0 }, shapes: { 0: [[0, 0, 0, 1, 0.5, 1]] } };
        assert.deepEqual(againstCollisionTable(stone(), data), ['stone state 0']);
        assert.deepEqual(againstCollisionTable(stone(), { blocks: {}, shapes: {} }), ['stone is not in the data']);
    });

    it('a contextual shape is not read as a static difference', () => {
        const swaying = buildTable('x', parseDump(dumpOf(blockLine('bamboo'), stateLine('bamboo', 0, 1, '', '0.1,0.0,0.1,0.3,1.0,0.3', 'c:position')), 'x'), { client: 'c' });
        assert.deepEqual(againstCollisionTable(swaying, { blocks: { bamboo: 0 }, shapes: { 0: [[0.2, 0, 0.2, 0.4, 1, 0.4]] } }), []);
        assert.equal(coverage(swaying).channels.collision.contextual, 1);
        assert.deepEqual(coverage(swaying).channels.collision.blocks, { position: ['bamboo'] });
    });

    it('states out of order, or that do not number by their properties', () => {
        assert.throws(() => parseDump(dumpOf(blockLine('a'), stateLine('a', 1, 1, '')), 'x'), /out of order/);
        const swapped = [{ props: 'a=2' }, { props: 'a=1' }, { props: 'a=1' }];
        assert.throws(() => stateDomains('b', swapped), /3 states are not its 2 property combinations/);
        const reordered = [{ props: 'a=1,b=1' }, { props: 'a=2,b=1' }, { props: 'a=1,b=2' }, { props: 'a=2,b=2' }];
        assert.throws(() => stateDomains('b', reordered), /does not number by its properties/);
    });

    it('a dump that cannot say how tags were bound, or an unknown line', () => {
        assert.throws(() => parseDump(blockLine('a'), 'x'), /does not say how tags were bound/);
        assert.throws(() => parseDump(dumpOf('nonsense'), 'x'), /unknown dump line/);
    });

    it('state ids that do not run on', () => {
        const dump = dumpOf(blockLine('a'), stateLine('a', 0, 5, 'p=1'), stateLine('a', 1, 7, 'p=2'));
        assert.throws(() => buildTable('x', parseDump(dump, 'x'), { client: 'c' }), /not contiguous/);
    });

    it('keeps no box apart from none, nor absent apart from empty', () => {
        assert.equal(parseBoxes('-'), null);
        assert.deepEqual(parseBoxes(''), []);
        assert.deepEqual(parseBoxes('0.0,0.0,0.0,1.0,1.0,1.0;0.0,1.0,0.0,1.0,1.5,1.0'), [[0, 0, 0, 1, 1, 1], [0, 1, 0, 1, 1.5, 1]]);
        assert.deepEqual(parseContext('c:position+entity,s:level'), { collision: DEPENDS.position | DEPENDS.entity, outline: 0, interaction: 0, support: DEPENDS.level });
        assert.throws(() => parseContext('q:level'), /Unknown channel/);
        assert.throws(() => parseContext('c:weather'), /Unknown dependence/);
    });
});

describe('the native lookup', () => {
    // every state of the first and last of the eras a shape's API changed in, and a stride of the rest
    const everyState = new Set(['1.14.4', '1.16', '1.20.5', '26.3']);

    it('gives every channel of every state the table has', () => {
        for (const version of exact) {
            const { shapes, blocks } = table(version);
            const stride = everyState.has(version) ? 1 : 37;
            for (const [id, entry] of Object.entries(blocks)) {
                for (let index = 0; index < stateCount(entry); index += stride) {
                    for (const channel of CHANNELS) {
                        const answer = native.generatedStateShape(version, id, channel, index);
                        const shape = at(entry[channel], index);
                        const label = `${version} ${id} ${index} ${channel}`;
                        if (shape === null) assert.equal(answer.status, 'absent', label);
                        else if (at(entry[`${channel}Depends`], index) !== 0) assert.equal(answer.status, 'contextual', label);
                        else {
                            assert.equal(answer.status, 'exact', label);
                            assert.deepEqual(boxesOf(answer), shapes[shape], label);
                        }
                    }
                }
            }
        }
    });

    it('round-trips each state through its properties', () => {
        for (const version of ['1.14.4', '1.21.4', '26.3']) {
            const { blocks } = table(version);
            for (const [id, entry] of Object.entries(blocks)) {
                for (let index = 0; index < stateCount(entry); index += 11) {
                    const byIndex = native.generatedStateShape(version, id, 'outline', index);
                    assert.deepEqual(native.generatedStateShapeForProperties(version, id, 'outline', tuple(entry, index)), byIndex, `${version} ${id} ${index}`);
                }
            }
        }
    });

    it('rejects what no state is', () => {
        const facing = { name: 'facing', value: 'north' };
        const half = { name: 'half', value: 'bottom' };
        const shape = { name: 'shape', value: 'straight' };
        const water = { name: 'waterlogged', value: 'false' };
        const stairs = (...properties) => native.generatedStateShapeForProperties('26.3', 'oak_stairs', 'collision', properties);
        assert.equal(stairs(facing, half, shape, water).status, 'exact');
        assert.equal(stairs(facing, half, shape), null, 'a property missing');
        assert.equal(stairs(facing, half, shape, water, { name: 'lit', value: 'true' }), null, 'one too many');
        assert.equal(stairs({ name: 'facing', value: 'up' }, half, shape, water), null, 'a value the property lacks');
        assert.equal(stairs({ name: 'FACING', value: 'north' }, half, shape, water), null, 'a Java field name is not a property');
        assert.equal(native.generatedStateShape('26.3', 'stone', 'collision', 1), null, 'past its states');
        assert.equal(native.generatedStateShape('26.3', 'stone', 'nonsense', 0), null, 'no such channel');
        assert.equal(native.generatedStateShape('26.3', 'no_such_block', 'collision', 0), null, 'no such block');
        assert.equal(native.generatedStateShape('1.12.2', 'stone', 'collision', 0), null, 'a release with no table');
    });

    it('resolves what a literal candidate cannot: rotations and connections', () => {
        const stairs = (facing, half = 'bottom', shape = 'straight') => boxesOf(native.generatedStateShapeForProperties('26.3', 'oak_stairs', 'collision', [
            { name: 'facing', value: facing }, { name: 'half', value: half }, { name: 'shape', value: shape }, { name: 'waterlogged', value: 'false' },
        ]));
        assert.notDeepEqual(stairs('north'), stairs('south'));
        assert.notDeepEqual(stairs('north'), stairs('north', 'top'));
        assert.notDeepEqual(stairs('north'), stairs('north', 'bottom', 'inner_left'));
        const post = name => [{ name: 'east', value: 'none' }, { name: 'north', value: name }, { name: 'south', value: 'none' }, { name: 'up', value: 'true' }, { name: 'waterlogged', value: 'false' }, { name: 'west', value: 'none' }];
        const wall = name => boxesOf(native.generatedStateShapeForProperties('26.3', 'cobblestone_wall', 'collision', post(name)));
        assert.notDeepEqual(wall('none'), wall('low'));
    });

    it('will not call a shape static that needs more than the state', () => {
        const contextual = (id, channel, ...what) => {
            const answer = native.generatedStateShape('26.3', id, channel, 0);
            assert.equal(answer.status, 'contextual', `${id} ${channel}`);
            assert.deepEqual(answer.dependsOn, what, `${id} ${channel}`);
            assert.deepEqual(answer.boxes, [], `${id} ${channel}`);
        };
        contextual('scaffolding', 'collision', 'entity'); // above it or descending
        contextual('powder_snow', 'collision', 'entity'); // its falling and boots branches
        contextual('water', 'collision', 'entity'); // a strider standing on it
        contextual('light', 'outline', 'entity'); // holding a light
        contextual('shulker_box', 'collision', 'level'); // its block entity's lid
        contextual('moving_piston', 'collision', 'level');
        contextual('bamboo', 'collision', 'position');
        contextual('poppy', 'outline', 'position');
    });

    it('still gives the static channels of a contextual block', () => {
        assert.equal(native.generatedStateShape('26.3', 'bamboo', 'interaction', 0).status, 'exact');
        assert.equal(native.generatedStateShape('26.3', 'poppy', 'collision', 0).status, 'exact');
        assert.deepEqual(boxesOf(native.generatedStateShape('26.3', 'poppy', 'collision', 0)), []);
    });

    it('keeps channels apart where they differ', () => {
        const layers = (channel, index) => boxesOf(native.generatedStateShape('26.3', 'snow', channel, index));
        for (let layer = 1; layer <= 8; layer += 1) {
            assert.deepEqual(layers('collision', layer - 1), layer === 1 ? [] : [[0, 0, 0, 1, (layer - 1) / 8, 1]], `collision ${layer}`);
            assert.deepEqual(layers('outline', layer - 1), [[0, 0, 0, 1, layer / 8, 1]], `outline ${layer}`);
            assert.deepEqual(layers('support', layer - 1), [[0, 0, 0, 1, layer / 8, 1]], `support ${layer}`);
        }
        const leaves = channel => boxesOf(native.generatedStateShape('26.3', 'oak_leaves', channel, 0));
        assert.deepEqual(leaves('collision'), [[0, 0, 0, 1, 1, 1]]);
        assert.deepEqual(leaves('support'), []);
    });

    it('holds a shape past its block, and an empty one, as they are', () => {
        const fence = boxesOf(native.generatedStateShape('26.3', 'oak_fence', 'collision', 0));
        assert.ok(fence.some(box => box[4] === 1.5), 'a fence stands 1.5 high');
        assert.deepEqual(boxesOf(native.generatedStateShape('26.3', 'air', 'collision', 0)), []);
    });

    it('leaves the literal candidates as they were, apart from exact answers', () => {
        assert.equal(native.generatedExactMethodShapeBoxes('1.20.6', 'oak_door', 'outline'), null);
        assert.ok(Array.isArray(native.generatedMethodShapeBoxes('1.20.6', 'oak_door', 'outline')));
    });
});
