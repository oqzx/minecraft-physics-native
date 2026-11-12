// src/mineflayer/push-reactions.json from the lines of scripts/java-oracle/OraclePushReactions.java, one file a release: what
// PistonBaseBlock.isPushable reads of each block, the sets shared by the releases that have the same one.
//
//   node scripts/generate-push-reactions.cjs out-1.8.8.txt out-1.9.4.txt ...
const fs = require('node:fs');
const path = require('node:path');

const KINDS = { DESTROY: 'destroy', POPPED: 'destroy', BLOCK: 'block', IMMOVEABLE: 'block', PUSH_ONLY: 'push', PUSH: 'push' };
const sets = [];
const indexOf = names => {
    const text = JSON.stringify(names);
    const found = sets.findIndex(set => JSON.stringify(set) === text);
    return found >= 0 ? found : sets.push(names) - 1;
};
const releases = {};
for (const file of process.argv.slice(2)) {
    const lists = { destroy: [], block: [], push: [], entity: [] };
    let release;
    for (const line of fs.readFileSync(file, 'utf8').split(/\r?\n/)) {
        const [version, name, reaction, entity] = line.split(' ');
        if (!entity) continue;
        release = version;
        if (KINDS[reaction]) lists[KINDS[reaction]].push(name);
        if (entity === '1') lists.entity.push(name);
    }
    if (release) releases[release] = Object.fromEntries(Object.entries(lists).map(([kind, names]) => [kind, indexOf(names.sort())]));
}
fs.writeFileSync(path.join(__dirname, '..', 'src', 'mineflayer', 'push-reactions.json'), `${JSON.stringify({ sets, releases })}\n`);
