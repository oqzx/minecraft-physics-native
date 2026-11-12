// The channels generated/block-shapes marks contextual, the jar in the context against the engine through the adapter, over
// test/fixtures/contexts (OracleContext.java): test/contextual-shapes.test.cjs and scripts/audit-adapter-world.cjs. Each side is the
// whole way: mapper, its updates, NativeWorld, the query.
const fs = require('node:fs');
const path = require('node:path');
const zlib = require('node:zlib');
const adapter = require('../..');
const native = require('../../native/artifacts/native.js');
const { releaseCheck } = require('../../dist/mineflayer/blocks.js');

const root = path.resolve(__dirname, '..', '..');
const table = version => JSON.parse(zlib.gunzipSync(fs.readFileSync(path.join(root, 'generated', 'block-shapes', `${version}.json.gz`))).toString('utf8'));
const fixture = version => zlib.gunzipSync(fs.readFileSync(path.join(root, 'test', 'fixtures', 'contexts', `${version}.tsv.gz`))).toString('utf8').split('\n').filter(Boolean).map(line => line.split('\t'));
const parseBoxes = text => (text === '' || text === undefined ? [] : text.split(';').map(box => box.split(',').map(Number)));
const sorted = boxes => boxes.map(box => box.join()).sort().join(' ');
const aabb = ({ min, max }) => [min.x, min.y, min.z, max.x, max.y, max.z];
const moved = ([a, b, c, d, e, f], x, y, z) => [a + x, b + y, c + z, d + x, e + y, f + z];
const around = (x, y, z) => [{ x: x - 2, y: y - 1, z: z - 2 }, { x: x + 3, y: y + 2, z: z + 3 }];
const nativeBox = ([a, b, c, d, e, f]) => ({ min: { x: a, y: b, z: c }, max: { x: d, y: e, z: f } });
/** The same solid in other boxes: every cell of the lines of both is in both or neither. */
function sameUnion(left, right) {
    const lines = axis => [...new Set([...left, ...right].flatMap(box => [box[axis], box[axis + 3]]))].sort((a, b) => a - b);
    const inside = (boxes, x, y, z) => boxes.some(b => b[0] < x && x < b[3] && b[1] < y && y < b[4] && b[2] < z && z < b[5]);
    const middles = list => list.slice(1).map((value, i) => (value + list[i]) / 2);
    for (const x of middles(lines(0))) for (const y of middles(lines(1))) for (const z of middles(lines(2))) {
        if (inside(left, x, y, z) !== inside(right, x, y, z)) return false;
    }
    return true;
}

const stateCount = entry => entry.properties.reduce((product, [, values]) => product * values.length, 1);

function setup(version) {
    const registry = require('prismarine-registry')(version);
    const Block = require('prismarine-block')(registry);
    const mapper = new adapter.BlockMapper(registry.version, registry, (name, tag) => native.generatedBlockHasTag(version, name, tag),
        name => native.generatedMethodShapeBoxes(version, name, 'outline'), (name, index, kind) => native.generatedStateShape(version, name, kind, index));
    const { blocks } = table(version);
    const state = (name, index) => mapper.describe(Block.fromStateId(blocks[name].firstId + Number(index), 0));
    /** The first state of a block whose properties pass. */
    const find = (name, wanted) => {
        for (let index = 0; index < stateCount(blocks[name]); index += 1) {
            const made = state(name, index);
            if (wanted(made.props)) return made;
        }
        throw new Error(`no such state of ${name}`);
    };
    return { mapper, state, find, registry, Block, stateId: (name, index) => blocks[name].firstId + Number(index) };
}

const FACINGS = ['down', 'up', 'north', 'south', 'west', 'east'];

/**
 * A shulker box's cell as the adapter gives it the engine: ShulkerBoxTracker's lid, from the block event, at each tick, and
 * NativeWorldSync.setShulkerBoxes on a world of that one block (`closing` opened all the way first).
 */
function shulkerWorld(version, registry, Block, id, way, step) {
    const binding = adapter.LoadNativePhysics();
    const [x, y, z] = [0, 64, 0];
    const world = { getBlock: pos => Block.fromStateId(pos.x === x && pos.y === y && pos.z === z ? id : 0, 0) };
    const sync = new adapter.NativeWorldSync(binding, registry, version);
    sync.sync(world, { minX: x - 1, minY: y - 1, minZ: z - 1, maxX: x + 1, maxY: y + 1, maxZ: z + 1 }, 1);
    const facing = FACINGS.indexOf(Block.fromStateId(id, 0).getProperties().facing);
    const lids = new adapter.ShulkerBoxTracker(binding, version);
    lids.event(x, y, z, facing, 1);
    if (way === 'closing') {
        for (let i = 0; i < 10; i += 1) lids.tick();
        lids.event(x, y, z, facing, 0);
    }
    for (let i = 0; i < step; i += 1) lids.tick();
    sync.setShulkerBoxes(lids.boxes());
    return sync.world;
}

function worldWith(mapper, described, x, y, z) {
    const world = new native.NativeWorld();
    world.setWorldBounds(-512, 512, 0, 0, 60000000);
    const updates = mapper.updatesAt((a, b, c) => (a === x && b === y && c === z ? described : null), x, y, z);
    if (updates.length > 0) world.applyBlockUpdates(updates);
    return world;
}

/** The answers of a release's fixture, and what differs by channel and block. */
function compare(version) {
    const { mapper, state, find, registry, Block, stateId } = setup(version);
    const differ = new Map();
    let compared = 0;
    const check = (what, got, want, text) => {
        compared += 1;
        // (a retracting piston's Shapes.or optimizes the base and the head into other boxes of the same solid; the engine keeps them)
        // (and 1.18's merger gives scaffolding other boxes than the table of the release before)
        if (sorted(got) === sorted(want) || (/^moving|scaffolding$/.test(what) && sameUnion(got, want))) return;
        const entry = differ.get(what) ?? { count: 0, first: `${text}: engine ${sorted(got)} jar ${sorted(want)}` };
        entry.count += 1;
        differ.set(what, entry);
    };
    const key = fields => fields.join(' ');
    const worlds = new Map();
    const worldOf = (described, name, index, x, y, z) => {
        const id = `${name} ${index} ${x} ${y} ${z}`;
        if (!worlds.has(id)) worlds.set(id, worldWith(mapper, described, x, y, z));
        return worlds.get(id);
    };
    for (const row of fixture(version)) {
        const [kind, name, index] = row;
        const described = state(name, index);
        if (kind === 'position') {
            const [x, y, z] = row.slice(3, 6).map(Number);
            const world = worldOf(described, name, index, x, y, z);
            const [min, max] = around(x, y, z);
            check(`position collision ${name}`, world.collisionShapes(min, max, version).map(aabb), parseBoxes(row[6]).map(box => moved(box, x, y, z)), key(row.slice(1, 6)));
            check(`position outline ${name}`, world.outlineShapes(min, max, version).map(aabb), parseBoxes(row[7]).map(box => moved(box, x, y, z)), key(row.slice(1, 6)));
        } else if (kind === 'entity') {
            const [y, bottom, descending, fall, boots, fluid] = [Number(row[3]), Number(row[4]), row[5] === '1', Number(row[6]), row[7] === '1', row[8] === '1'];
            if (name === 'water' || name === 'lava') {
                const box = adapter.liquidCollisionBox(described, null, y, { bottom, canStandOn: () => false, withFluidCollisions: fluid }, releaseCheck(version));
                check(`entity collision ${name}`, box ? [box] : [], parseBoxes(row[9]), key(row.slice(1, 9)));
                continue;
            }
            const world = worldOf(described, name, index, 0, y, 0);
            const [min, max] = around(0, y, 0);
            if (!fluid) {
                const got = world.entityCollisionShapes(min, max, { bottom, descending, fallDistance: fall, walksOnPowderSnow: name === 'powder_snow' && boots }, version).map(aabb);
                check(`entity collision ${name}`, got, parseBoxes(row[9]).map(box => moved(box, 0, y, 0)), key(row.slice(1, 9)));
            }
            // (who holds no such block: what a ray of the engine is, the same in each of these rows)
            if (!fluid && bottom === y + 1.5 && !descending && fall === 0 && !boots) {
                check(`outline ${name}`, world.outlineShapes(min, max, version).map(aabb), parseBoxes(row[10]).map(box => moved(box, 0, y, 0)), key(row.slice(1, 9)));
            }
        } else if (kind === 'falling') {
            const [y, bottom, descending] = [Number(row[3]), Number(row[4]), row[5] === '1'];
            const world = worldOf(described, name, index, 0, y, 0);
            const [min, max] = around(0, y, 0);
            // (a FallingBlockEntity at every height, sneaking or not: the engine's moving body says fallingBlock, entity_collision_context)
            const got = world.entityCollisionShapes(min, max, { bottom, descending, fallDistance: 0, walksOnPowderSnow: true, fallingBlock: true }, version).map(aabb);
            check('falling powder_snow', got, parseBoxes(row[6]).map(box => moved(box, 0, y, 0)), key([name, bottom, descending]));
        } else if (kind === 'shulker') {
            const world = shulkerWorld(version, registry, Block, stateId(name, index), row[3], Number(row[4]));
            const [min, max] = around(0, 64, 0);
            const label = key(row.slice(1, 5));
            check(`shulker collision ${name}`, world.collisionShapes(min, max, version).map(aabb), parseBoxes(row[7]).map(box => moved(box, 0, 64, 0)), label);
            check(`shulker outline ${name}`, world.outlineShapes(min, max, version).map(aabb), parseBoxes(row[8]).map(box => moved(box, 0, 64, 0)), label);
        } else if (kind === 'moving') {
            const [facing, extending, source, progress, x, y, z] = [Number(row[3]), row[4] === '1', row[5] === '1', Number(row[6]), Number(row[7]), Number(row[8]), Number(row[9])];
            const world = new native.NativeWorld();
            world.setWorldBounds(-512, 512, 0, 0, 60000000);
            world.applyBlockUpdates(mapper.updatesAt((a, b, c) => (a === x && b === y && c === z ? state('moving_piston', 0) : null), x, y, z));
            const named = described.props.facing;
            // (PistonMovingBlockEntity.getCollisionShape: the piston's head at its progress, MovingBlockTracker's rule, a base left behind)
            const short = extending !== (Math.fround(1 - progress) < (/^1.1[45]/.test(version) ? 4 : 0.25));
            const head = () => find('piston_head', props => props.facing === named && String(props.short) === String(short)).boxes;
            const retracting = source && !extending;
            const base = retracting ? find(name, props => props.facing === named && String(props.extended) === 'true').boxes : [];
            const shapes = source ? head() : described.boxes;
            world.setMovingBlock(version, { x, y, z, facing, extending, source, progress, shapes: shapes.map(nativeBox) }, base.map(nativeBox));
            const [min, max] = around(x, y, z);
            check(`moving ${name}`, world.collisionShapes(min, max, version).map(aabb), parseBoxes(row[10]).map(box => moved(box, x, y, z)), key(row.slice(1, 10)));
        }
    }
    return { compared, differ };
}

module.exports = { compare, sameUnion };
