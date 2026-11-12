'use strict';

// node scripts/determinism/replay.cjs <corpus dir> <addon> <out.json> [--ticks n] [--only release] [--dump release/scene/family/tick]
// Replays the corpus on one build and writes a digest of every state, numbers by their bits. It needs no dependency, and
// its inputs come from an integer generator and arithmetic IEEE 754 fixes exactly, so no target's libm enters them.
// --dump prints the state, and the traced phases of the player's tick, at a tick: where compare.cjs said two builds part.

const { createHash } = require('node:crypto');
const { readFileSync, writeFileSync } = require('node:fs');
const { join, resolve } = require('node:path');

const [dir, addon, out] = process.argv.slice(2);
const option = (name, fallback) => {
    const at = process.argv.indexOf(`--${name}`);
    return at < 0 ? fallback : process.argv[at + 1];
};
const binding = require(resolve(addon));
const TICKS = Number(option('ticks', 2000));
const EVERY = 100;
const manifest = JSON.parse(readFileSync(join(dir, 'manifest.json'), 'utf8'));
const FLOOR_Y = 63;

function generator(seed) {
    let state = seed >>> 0;
    return () => {
        state = (state + 0x6D2B79F5) >>> 0;
        let t = state;
        t = Math.imul(t ^ (t >>> 15), t | 1);
        t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
        return (t ^ (t >>> 14)) >>> 0;
    };
}

const seedOf = text => [...text].reduce((hash, char) => (Math.imul(hash, 31) + char.charCodeAt(0)) >>> 0, 7);
const number = new DataView(new ArrayBuffer(8));

/** A value with its object keys in order and each number as its bits. */
function canonical(value) {
    if (typeof value === 'number') {
        number.setFloat64(0, value);
        return number.getBigUint64(0).toString(16);
    }
    if (Array.isArray(value)) return `[${value.map(canonical).join(',')}]`;
    if (value !== null && typeof value === 'object') {
        return `{${Object.keys(value).sort().filter(key => value[key] !== undefined && value[key] !== null).map(key => `${key}:${canonical(value[key])}`).join(',')}}`;
    }
    return String(value);
}

/** A tick's held keys and look, drawn so a seed is the same run everywhere. */
function drawInput(random) {
    const yaw = (random() % 62832) / 10000;
    const pitch = ((random() % 31416) - 15708) / 20000;
    const [x, z] = [(random() % 2001) / 1000 - 1, (random() % 2001) / 1000 - 1];
    const length = Math.sqrt(x * x + z * z + 0.25) || 1;
    return {
        yaw, pitch,
        look: { x: x / length, y: 0.5 / length, z: z / length },
        input: { x: (random() % 3) - 1, y: 0, z: (random() % 4) === 0 ? 0 : 1 },
        jumpRequested: (random() % 5) === 0,
        sprinting: (random() % 3) !== 0,
        sneaking: (random() % 11) === 0,
    };
}

const walker = (scene, fields = {}) => ({
    position: { x: 0.5, y: scene === 'fluid' ? FLOOR_Y : FLOOR_Y + 1, z: 0.5 }, velocity: { x: 0, y: 0, z: 0 }, yaw: 0, pitch: 0,
    look: { x: 0, y: 0, z: 1 }, halfWidth: 0.3, height: 1.8, eyeHeight: 1.62, onGround: scene !== 'fluid', collidedHorizontally: false,
    collidedVertically: false, isPlayer: true, clientSide: true, inWater: scene === 'fluid', ...fields,
});

/** One run of a family: the next state from the last, each tick. `first` is its start, `tick` its step. */
const FAMILIES = {
    walk: {
        first: scene => walker(scene),
        tick: (version, world, state, random) => binding.simulateVersionedTick(version, world, { ...state, ...drawInput(random) }, {}),
        trace: (version, world, state) => binding.simulateVersionedTickTraced(version, world, state, {}),
    },
    glide: {
        first: scene => walker(scene, { position: { x: 0.5, y: 71, z: 0.5 }, onGround: false, fallFlying: true }),
        tick: (version, world, state, random) => {
            const { look, yaw, pitch } = drawInput(random);
            const position = state.position.y < 60 ? { ...state.position, y: 71 } : state.position;
            return binding.simulateVersionedTick(version, world, { ...state, position, look, yaw, pitch, fallFlying: true }, {});
        },
    },
    crowd: {
        first: scene => [1, 2, 3, 4].map(id => ({ simulation: walker(scene, { isPlayer: false, input: { x: 0, y: 0, z: 1 } }), noPhysics: false, id, pushable: true, pushthrough: 0 })),
        tick: (version, world, states) => binding.simulateVersionedEntityTicks(version, world, states, 1, {}),
    },
    boat: {
        first: () => ({ position: { x: 0.5, y: FLOOR_Y + 1.6, z: 0.5 }, velocity: { x: 0, y: 0, z: 0 }, yaw: 0, forward: true, left: false, right: false, inWater: true }),
        tick: (version, world, state, random) => binding.simulateVersionedBoatTicks(version, world, [{ ...state, left: random() % 2 === 0, right: random() % 2 === 0 }], 1)[0],
    },
    arrow: {
        first: () => ({ position: { x: 0.5, y: 70, z: 0.5 }, velocity: { x: 1.5, y: 0.6, z: 0.4 } }),
        tick: (version, world, state, random) => {
            const [result] = binding.simulateVersionedArrowTicks(version, world, [state], 1);
            if (!result.discarded && !result.inGround && result.position.y > 40) return { ...state, position: result.position, velocity: result.velocity, inWater: result.inWater, life: result.life, marker: undefined };
            return { position: { x: (random() % 40) + 0.5, y: 70, z: (random() % 20) + 0.5 }, velocity: { x: (random() % 300) / 100 - 1.5, y: 0.3, z: (random() % 200) / 100 - 1 } };
        },
    },
    snowball: {
        first: () => ({ kind: 'snowball', position: { x: 0.5, y: 70, z: 0.5 }, velocity: { x: 1.2, y: 0.4, z: 0.3 } }),
        tick: (version, world, state, random) => {
            const [result] = binding.simulateVersionedProjectileTicks(version, world, [state], 1);
            if (!result.blockHit && !result.discarded && result.position.y > 40) return { ...state, position: result.position, velocity: result.velocity, inWater: result.inWater, firstTick: result.firstTick };
            return { kind: 'snowball', position: { x: (random() % 40) + 0.5, y: 70, z: (random() % 20) + 0.5 }, velocity: { x: (random() % 240) / 100 - 1.2, y: 0.2, z: (random() % 200) / 100 - 1 } };
        },
    },
};

/** The player families go through each scene, the others where they mean something. */
const familiesOf = scene => ({ empty: ['walk', 'glide', 'crowd', 'arrow', 'snowball'], dense: ['walk', 'crowd', 'arrow'], fluid: ['walk', 'boat', 'arrow', 'snowball'] }[scene]);

function run(version, scene, family, dumpTick) {
    const world = binding.nativeWorldFromCheckpoint(readFileSync(join(dir, `${version}.${scene}.world`)));
    const { first, tick, trace } = FAMILIES[family];
    const random = generator(seedOf(`${version}/${scene}/${family}`));
    let state = first(scene);
    let digest = createHash('sha256').digest();
    const marks = [];
    for (let at = 0; at < TICKS; at += 1) {
        if (at === dumpTick) {
            process.stdout.write(`${canonical(state)}\n`);
            if (trace) process.stdout.write(`${canonical(trace(version, world, state))}\n`);
            return null;
        }
        try {
            state = tick(version, world, state, random);
        } catch (error) {
            return { ticks: at, error: error.message, digest: digest.toString('hex').slice(0, 24), marks };
        }
        digest = createHash('sha256').update(digest).update(canonical(state)).digest();
        if ((at + 1) % EVERY === 0) marks.push(digest.toString('hex').slice(0, 16));
    }
    return { ticks: TICKS, digest: digest.toString('hex').slice(0, 24), marks };
}

const dump = option('dump');
if (dump) {
    const [version, scene, family, tick] = dump.split('/');
    run(version, scene, family, Number(tick));
} else {
    const only = option('only');
    const results = {};
    const started = Date.now();
    for (const version of Object.keys(manifest.releases)) {
        if (only && only !== version) continue;
        results[version] = {};
        for (const scene of Object.keys(manifest.releases[version])) {
            for (const family of familiesOf(scene)) results[version][`${scene}/${family}`] = run(version, scene, family);
        }
    }
    writeFileSync(out, `${JSON.stringify({
        target: `${process.platform}-${process.arch}`, node: process.version, addon: createHash('sha256').update(readFileSync(addon)).digest('hex'),
        ticksPerRun: TICKS, results,
    }, null, 0)}\n`);
    process.stdout.write(`${Object.keys(results).length} releases in ${(Date.now() - started) / 1000}s\n`);
}
