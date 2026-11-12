// The blocks of the six releases before 1.14 whose collision is their block entity's, the jar against the engine through the adapter,
// over test/fixtures/legacy-block-entities (LegacyNeighbourDump.java with `entities`): a piston_extension cell with a real
// PistonMovingBlockEntity, and a shulker box with a real ShulkerBoxBlockEntity, opening and closing. Each side is the whole way: the
// adapter's trackers make the block entity's state, NativeWorldSync gives the cell its shapes, NativeWorld answers the query.
// test/legacy-block-entities.test.cjs and scripts/audit-adapter-world.cjs.
const fs = require('node:fs');
const path = require('node:path');
const zlib = require('node:zlib');
const { EventEmitter } = require('node:events');
const { Vec3 } = require('vec3');
const adapter = require('../..');
const { MovingBlockTracker } = require('../../dist/mineflayer/moving.js');
const { compareReleases } = require('../../dist/mineflayer/blocks.js');
const { sameUnion } = require('./contexts.cjs');

const RELEASES = ['1.8.8', '1.9.4', '1.10.2', '1.11.2', '1.12.2', '1.13.2'];
const folder = path.resolve(__dirname, '..', '..', 'test', 'fixtures', 'legacy-block-entities');
const fixture = version => zlib.gunzipSync(fs.readFileSync(path.join(folder, `${version}.tsv.gz`))).toString('utf8').split('\n').filter(Boolean).map(line => line.split('\t'));
const flattened = version => version === '1.13.2';
const stateId = (version, id) => (flattened(version) ? id : ((id & 4095) << 4) | (id >> 12));
const parseBoxes = text => (text === '' ? [] : text.split(';').map(box => box.split(',').map(Number)));
const sorted = boxes => boxes.map(box => box.join()).sort().join(' ');
const aabb = ({ min, max }) => [min.x, min.y, min.z, max.x, max.y, max.z];
// (the dump's cell: LegacyNeighbourDump X, Y, Z)
const [X, Y, Z] = [8, 72, 8];
const FACINGS = ['down', 'up', 'north', 'south', 'west', 'east'];
const around = { minX: X - 2, minY: Y - 2, minZ: Z - 2, maxX: X + 3, maxY: Y + 3, maxZ: Z + 3 };
const query = [{ x: X - 2, y: Y - 2, z: Z - 2 }, { x: X + 3, y: Y + 3, z: Z + 3 }];

/** A world of one block at the dump's cell, which tells its changes as prismarine-world does. */
function worldOf(registry, Block) {
    const blocks = new Map();
    const at = pos => blocks.get(`${Math.floor(pos.x)},${Math.floor(pos.y)},${Math.floor(pos.z)}`) ?? 0;
    const world = Object.assign(new EventEmitter(), {
        getBlock: pos => Object.assign(Block.fromStateId(at(pos), 0), { position: pos }),
        getBlockStateId: at,
        setBlockStateId(pos, id) {
            blocks.set(`${pos.x},${pos.y},${pos.z}`, id);
            world.emit('blockUpdate', null, { position: new Vec3(pos.x, pos.y, pos.z) });
        },
    });
    return world;
}

function compare(version) {
    const registry = require('prismarine-registry')(version);
    const Block = require('prismarine-block')(registry);
    const binding = adapter.LoadNativePhysics();
    const differ = new Map();
    const shapes = [];
    let compared = 0;
    let skipped = 0;
    const note = (what, got, want, text) => {
        compared += 1;
        // (1.13.2 joins a retracting base and its head with Shapes.or, which optimizes them into other boxes of the same solid)
        if (sorted(got) === sorted(want) || (flattened(version) && what.startsWith('moving') && sameUnion(got, want))) return;
        const entry = differ.get(what) ?? { count: 0, first: `${text}: engine ${sorted(got)} jar ${sorted(want)}` };
        entry.count += 1;
        differ.set(what, entry);
    };
    const moved = boxes => boxes.map(([a, b, c, d, e, f]) => [a + X, b + Y, c + Z, d + X, e + Y, f + Z]);
    const cellId = registry.blocksByName.piston_extension ?? registry.blocksByName.moving_piston;
    for (const row of fixture(version)) {
        if (row[0] === 'shape') {
            shapes[Number(row[1])] = parseBoxes(row[2] ?? '');
        } else if (row[0] === 'moving') {
            const [, name, id, facing, extending, source, progress, progressO, answer] = row;
            const block = Block.fromStateId(stateId(version, Number(id)), 0);
            const ext = extending === '1';
            const src = source === '1';
            // (what MovingBlockTracker makes: a head is the source from 1.11, a base drawn back always, what a piston moves never)
            const wanted = name === 'piston_head' ? ext && src === (compareReleases(version, '1.11') >= 0)
                : /piston$/.test(name) && src ? !ext : !src;
            if (!wanted || Number(progressO) !== Math.max(0, Number(progress) - 0.5)) {
                skipped += 1;
                continue;
            }
            const world = worldOf(registry, Block);
            world.setBlockStateId({ x: X, y: Y, z: Z }, flattened(version) ? cellId.defaultState : cellId.id << 4 | Number(facing));
            const sync = new adapter.NativeWorldSync(binding, registry);
            sync.sync(world, around, 1);
            const tracker = new MovingBlockTracker(registry, version, world);
            tracker.entries.set(`${X},${Y},${Z}`, { pos: [X, Y, Z], facing: Number(facing), extending: ext, source: src, progress: Number(progress), moved: block });
            sync.setMovingBlocks(tracker.blocks());
            note(`moving ${name}`, sync.world.collisionShapes(...query, version).map(aabb), moved(shapes[Number(answer)]), `${version} ${name} ${id} facing ${facing} extending ${extending} source ${source} progress ${progress}`);
        } else if (row[0] === 'shulker') {
            const [, name, id, way, step, , , collision, outline] = row;
            const block = Block.fromStateId(stateId(version, Number(id)), 0);
            const facing = flattened(version) ? FACINGS.indexOf(block.getProperties().facing) : block.metadata & 7;
            const world = worldOf(registry, Block);
            world.setBlockStateId({ x: X, y: Y, z: Z }, stateId(version, Number(id)));
            const sync = new adapter.NativeWorldSync(binding, registry);
            sync.sync(world, around, 1);
            // (the client's own lid: the block event, then the tick of each step; a closing one opened first)
            const lids = new adapter.ShulkerBoxTracker(binding, version);
            if (way === 'closing') {
                lids.event(X, Y, Z, facing, 1);
                for (let i = 0; i < 10; i += 1) lids.tick();
                lids.event(X, Y, Z, facing, 0);
            } else {
                lids.event(X, Y, Z, facing, 1);
            }
            for (let i = 0; i < Number(step); i += 1) lids.tick();
            sync.setShulkerBoxes(lids.boxes());
            const label = `${version} ${name} ${id} ${way} ${step}`;
            note(`shulker collision ${name}`, sync.world.collisionShapes(...query, version).map(aabb), moved(shapes[Number(collision)]), label);
            note(`shulker outline ${name}`, sync.world.outlineShapes(...query, version).map(aabb), moved(shapes[Number(outline)]), label);
        }
    }
    return { compared, skipped, differ };
}

module.exports = { RELEASES, compare };
