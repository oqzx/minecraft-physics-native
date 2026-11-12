// src/mineflayer/legacy-outline-neighbours.json: what redstone wire and a vine read of the blocks around them before 1.13 for the
// box a ray meets, from each release's own code (test/fixtures/legacy-neighbours, LegacyNeighbourDump.java). Neither collides, so
// only a ray that stops at blocks without collision meets them: a thrown item, llama spit, a fireball.
//
// Per release (its major version, as legacy-connections.json):
//   wire.connects[side]  the states beside a wire it joins on that side (getConnectingSide's shouldConnectTo(state, side))
//   wire.climbs[side]    with a wire above them, the states it climbs on that side: those it joins, those top solid, glowstone
//   wire.normal          the states isNormalCube: one above the wire stops each climb, one beside it stops the way down
//   vineUp               the states above a vine that hold it there: 1.12's solid bottom face, 1.9 - 1.11's full cube, 1.8's
//                        canPlaceOn (the vine with no side, whose box they change)
// States are `id << 4 | data value`, in ranges. The way down is checked here against each state's `below` answer.
//   node scripts/generate-legacy-outline-neighbours.cjs [--check]
const fs = require('node:fs');
const path = require('node:path');
const zlib = require('node:zlib');

const RELEASES = ['1.8.8', '1.9.4', '1.10.2', '1.11.2', '1.12.2'];
const SIDES = ['north', 'south', 'west', 'east'];
const root = path.resolve(__dirname, '..');
const output = path.join(root, 'src', 'mineflayer', 'legacy-outline-neighbours.json');
const raw = id => ((id & 4095) << 4) | (id >> 12);

function ranges(values) {
    const sorted = [...new Set(values)].sort((a, b) => a - b);
    const out = [];
    for (const value of sorted) {
        const last = out[out.length - 1];
        if (last && last[1] === value - 1) last[1] = value;
        else out.push([value, value]);
    }
    return out;
}

function release(version) {
    const lines = zlib.gunzipSync(fs.readFileSync(path.join(root, 'test', 'fixtures', 'legacy-neighbours', `${version}.tsv.gz`))).toString('utf8').split('\n');
    const entry = {};
    const flags = lines.filter(line => line.startsWith('wireflags\t')).map(line => line.split('\t').slice(1).map(Number));
    if (flags.length > 0) {
        const connects = Object.fromEntries(SIDES.map(side => [side, []]));
        const climbs = Object.fromEntries(SIDES.map(side => [side, []]));
        const normal = [];
        for (const [id, conn, top, below, isNormal] of flags) {
            SIDES.forEach((side, bit) => {
                if (conn & (1 << bit)) connects[side].push(raw(id));
                if (top & (1 << bit)) climbs[side].push(raw(id));
                // (a wire below joins unless the state beside is a normal cube: shouldConnectTo || !isNormalCube)
                const down = Boolean(conn & (1 << bit)) || !isNormal;
                if (down !== Boolean(below & (1 << bit))) throw new Error(`${version} ${id}: the way down is not shouldConnectTo or no normal cube`);
            });
            if (isNormal) normal.push(raw(id));
        }
        entry.wire = {
            connects: Object.fromEntries(SIDES.map(side => [side, ranges(connects[side])])),
            climbs: Object.fromEntries(SIDES.map(side => [side, ranges(climbs[side])])),
            normal: ranges(normal),
        };
    }
    // the vine with no side, data value 0: its id is the block's
    const vine = lines.find(line => line.startsWith('alone\tvine\t'));
    if (vine) {
        const home = vine.split('\t')[2];
        const above = lines.find(line => line.startsWith(`side\t${home}\t0,1,0\t`));
        const differ = above ? above.split('\t')[5] : '';
        entry.vineUp = ranges(differ ? differ.split(',').map(item => raw(Number(item.split(':')[0]))) : []);
    }
    return entry;
}

const fresh = `${JSON.stringify(Object.fromEntries(RELEASES.map(version => [version.split('.').slice(0, 2).join('.'), release(version)])))}\n`;
if (process.argv.includes('--check')) {
    if (fs.readFileSync(output, 'utf8').replaceAll('\r\n', '\n') !== fresh) {
        console.error('src/mineflayer/legacy-outline-neighbours.json differs from the fixtures');
        process.exit(1);
    }
} else {
    fs.writeFileSync(output, fresh);
}
