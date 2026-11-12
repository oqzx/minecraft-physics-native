'use strict';

// node scripts/determinism/corpus.cjs <dir>: the serialized worlds of the cross-target corpus, one per release and scene.
// This side needs the registries; replay.cjs needs only an addon and these bytes, so every target replays the same input.

const { mkdirSync, writeFileSync } = require('node:fs');
const { join } = require('node:path');
const { RELEASES, SCENES, arg, sceneWorld, sha256 } = require('../bench/lib.cjs');

const REGION = { minX: -8, minY: 54, minZ: -8, maxX: 40, maxY: 72, maxZ: 24 };
const dir = process.argv[2];
mkdirSync(dir, { recursive: true });
const manifest = {};
const releases = arg('releases', RELEASES.join(',')).split(',');
let total = 0;
for (const version of releases) {
    manifest[version] = {};
    for (const scene of Object.keys(SCENES)) {
        const { world, sync } = sceneWorld(version, scene, REGION);
        const bytes = world.checkpoint();
        writeFileSync(join(dir, `${version}.${scene}.world`), bytes);
        manifest[version][scene] = { bytes: bytes.length, sha256: sha256(bytes), contentDigest: world.contentDigest() };
        total += bytes.length;
        sync.dispose();
    }
}
writeFileSync(join(dir, 'manifest.json'), `${JSON.stringify({ region: REGION, releases: manifest }, null, 1)}\n`);
process.stdout.write(`${releases.length} releases, ${total} bytes of worlds in ${dir}\n`);
