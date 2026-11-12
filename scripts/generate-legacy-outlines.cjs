// src/mineflayer/legacy-outlines.json from the lines of scripts/java-oracle/OracleLegacyOutlines.java: the outline box of each
// state before 1.13 that has no collision box, which is what a thrown item's ray meets there. By the releases legacy.ts goes by.
//
//   node scripts/generate-legacy-outlines.cjs 1.8.8=out-1.8.8.txt 1.9.4=out-1.9.4.txt ...
const fs = require('node:fs');
const path = require('node:path');

/** TorchBlock.rayTrace of 1.8, which sets its bounds there alone (1.8.9 TorchBlock.java), by its data: 1 east, 2 west, 3 south, 4 north, else standing. */
const f = Math.fround;
function torch(meta) {
    const w = f(0.15);
    const [low, high, wide] = [f(f(0.5) - w), f(f(0.5) + w), f(w * 2)];
    const [bottom, top] = [f(0.2), f(0.8)];
    if (meta === 1) return [0, bottom, low, wide, top, high];
    if (meta === 2) return [f(1 - wide), bottom, low, 1, top, high];
    if (meta === 3) return [low, bottom, 0, high, top, wide];
    if (meta === 4) return [low, bottom, f(1 - wide), high, top, 1];
    const [a, b] = [f(f(0.5) - f(0.1)), f(f(0.5) + f(0.1))];
    return [a, 0, a, b, f(0.6), b];
}

const out = {};
for (const argument of process.argv.slice(2)) {
    const [release, file] = argument.split('=');
    const data = require('minecraft-data')(release);
    const major = release.split('.').slice(0, 2).join('.');
    const states = {};
    for (const line of fs.readFileSync(file, 'utf8').split(/\r?\n/)) {
        const parts = line.split(' ');
        if (parts[0] !== release || parts.length !== 10) continue;
        const [id, meta, offset] = parts.slice(1, 4).map(Number);
        const block = data.blocks[id];
        const open = block?.name.endsWith('fence_gate') && (meta & 4) !== 0;
        // (a moving block answers no ray: PistonExtensionBlock.rayTrace)
        if (!block || block.name === 'piston_extension' || (block.boundingBox !== 'empty' && !open)) continue;
        // (the air of a bit of a float, as the jar gives it: nothing rounds these)
        (states[block.name] ??= {})[meta] = major === '1.8' && /torch$/.test(block.name) ? torch(meta) : [...parts.slice(4).map(Number), ...(offset ? [1] : [])];
    }
    out[major] = states;
}
fs.writeFileSync(path.join(__dirname, '..', 'src', 'mineflayer', 'legacy-outlines.json'), `${JSON.stringify(out)}\n`);
