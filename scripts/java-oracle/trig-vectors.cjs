'use strict';

// node scripts/java-oracle/trig-vectors.cjs <directory>: the seeded sets of angles for TrigDigest.java and its Rust twin, one file
// of little-endian doubles per set, and picked.bin, the few thousand of the hardest that native/fixtures/java-oracle-trig-bits.txt
// keeps with Java's bits. A seeded generator: the same set on every machine, though `boundaries` was seen to differ in 12 words between Node 24.15 on Windows and 24.21 on Linux, so compare digests of one file.

const { mkdirSync, writeFileSync } = require('node:fs');
const path = require('node:path');

const view = new DataView(new ArrayBuffer(8));
const fromBits = bits => { view.setBigUint64(0, bits); return view.getFloat64(0); };
const toBits = value => { view.setFloat64(0, value); return view.getBigUint64(0); };
const parts = (high, low) => { view.setUint32(0, high); view.setUint32(4, low); return view.getFloat64(0); };
const floatView = new DataView(new ArrayBuffer(4));
const floatFromBits = bits => { floatView.setUint32(0, bits); return floatView.getFloat32(0); };
const f = Math.fround;

function generator(seed) { // sfc32
    let a = seed | 0, b = 0x9e3779b9 | 0, c = 0x243f6a88 | 0, d = 1;
    const next = () => {
        const t = (((a + b) | 0) + d) | 0;
        d = (d + 1) | 0; a = b ^ (b >>> 9); b = (c + (c << 3)) | 0; c = (c << 21) | (c >>> 11); c = (c + t) | 0;
        return t >>> 0;
    };
    for (let i = 0; i < 16; i += 1) next();
    return next;
}
const uniform = next => (next() + next() / 2 ** 32) / 2 ** 32;
const signed = next => (next() & 1 ? -1 : 1);
const withExponent = (next, biased) => parts(((next() & 1) << 31 | biased << 20 | (next() & 0xfffff)) >>> 0, next());

const RANDOM = [
    ['float-quarter', 5e6, 1, r => f(uniform(r) * (Math.PI / 2))],
    ['float-turn', 3e6, 2, r => f(Math.PI / 2 + uniform(r) * (1.5 * Math.PI))],
    ['float-far', 2e6, 3, r => signed(r) * f(2 * Math.PI * 2 ** (uniform(r) * 17.3))],
    ['float-any', 2e6, 4, r => { let v; do v = floatFromBits(r()); while (!Number.isFinite(v) || v === 0); return v; }],
    ['double-quarter', 2e6, 5, r => uniform(r) * (Math.PI / 2)],
    ['double-main', 3e6, 6, r => withExponent(r, 771 + Math.floor(uniform(r) * 268))],
    ['double-any', 3e6, 7, r => withExponent(r, Math.floor(uniform(r) * 2047))],
    ['double-large', 1e6, 8, r => withExponent(r, 1039 + Math.floor(uniform(r) * 1008))],
    ['double-edge', 1e6, 9, r => signed(r) * (65536 + uniform(r) * (2 ** 21 - 65536))],
];

function atanInverse(n, bits) {
    const one = 1n << bits, square = n * n;
    let term = one / n, sum = term, k = 3n, sign = -1n;
    while (term > 0n) { term /= square; sum += sign * (term / k); k += 2n; sign = -sign; }
    return sum;
}
const PRECISION = 1700n;
const PI = (16n * atanInverse(5n, PRECISION + 32n) - 4n * atanInverse(239n, PRECISION + 32n)) >> 32n;
const bitLength = value => value.toString(2).length;

// The doubles nearest a multiple of pi/2 at each exponent: the convergents of 2^e * 2/pi with at most 53 bits of denominator.
function hardReductions() {
    const out = [];
    const twoOverPi = (1n << (2n * PRECISION + 1n)) / PI;
    const one = 1n << PRECISION;
    for (let e = -60; e <= 971; e += 1) {
        const fraction = (e >= 0 ? twoOverPi << BigInt(e) : twoOverPi >> BigInt(-e)) & (one - 1n);
        if (fraction === 0n) continue;
        let before = 0n, denominator = 1n, num = one, den = fraction;
        for (let i = 0; i < 80; i += 1) {
            const quotient = num / den;
            const next = quotient * denominator + before;
            if (bitLength(next) > 53) break;
            before = denominator; denominator = next;
            [num, den] = [den, num - quotient * den];
            if (den === 0n) break;
            if (bitLength(denominator) >= 40) out.push(Number(denominator) * 2 ** e, -Number(denominator) * 2 ** e);
        }
    }
    out.push(6381956970095103 * 2 ** 797, -6381956970095103 * 2 ** 797);
    return out.filter(Number.isFinite);
}

function boundaries() {
    const values = [];
    const around = (x, count) => {
        const bits = toBits(x);
        for (let i = -count; i <= count; i += 1) {
            const y = fromBits(bits + BigInt(i));
            if (y === y) values.push(y, -y);
        }
    };
    values.push(0, -0, 5e-324, -5e-324, Infinity, -Infinity, Number.MAX_VALUE, -Number.MAX_VALUE);
    for (const bits of [0x000fffffffffffffn, 0x800fffffffffffffn, 0x0000ffffffffffffn, 0x0010000000000000n, 0x000f000000000000n, 0x0001000000000000n]) values.push(fromBits(bits));
    for (let k = -1074; k <= 1023; k += 1) {
        if (k >= -1021) around(2 ** k, 3);
        else values.push(2 ** k, -(2 ** k));
    }
    for (const x of [90112, 2 ** -252, 2 ** -55, 2 ** -27, 1, Math.PI, Math.PI / 2, Math.PI / 4, Math.PI / 32, 2 * Math.PI, 65536]) around(x, 64);
    for (let n = 1; n <= 4096; n += 1) { around(n * Math.PI / 32, 2); around((n + 0.5) * Math.PI / 32, 2); }
    for (let n = 917800; n <= 918000; n += 1) { around(n * Math.PI / 32, 2); around((n + 0.5) * Math.PI / 32, 2); } // |x| = 90112 is N = 917887
    for (let n = 932500; n <= 932620; n += 1) around(n * Math.PI / 32, 2); // the |N| = 932560 of the assembly's comment
    for (let k = 0; k < 4096; k += 1) around(2 ** (k * 1020 / 4096), 1);
    return values;
}

// Non-finite words stay words: a NaN that passes through a JavaScript number may lose its payload.
const NAN_WORDS = [0x7ff8000000000000n, 0xfff8000000000000n, 0x7ff0000000000001n, 0xfff0000000000001n, 0x7ff8dead0000beefn];

function write(directory, name, doubles, extraWords = []) {
    const words = new BigUint64Array(doubles.length + extraWords.length);
    new Float64Array(words.buffer).set(doubles);
    extraWords.forEach((word, index) => { words[doubles.length + index] = word; });
    writeFileSync(path.join(directory, `${name}.bin`), Buffer.from(words.buffer));
    process.stdout.write(`${name} ${words.length}\n`);
}

const directory = process.argv[2];
mkdirSync(directory, { recursive: true });
const hard = hardReductions();
const edges = boundaries();
write(directory, 'hard', hard);
write(directory, 'boundaries', edges, NAN_WORDS);
for (const [name, count, seed, draw] of RANDOM) {
    const random = generator(seed);
    const list = new Float64Array(count);
    for (let i = 0; i < count; i += 1) list[i] = draw(random);
    write(directory, name, list);
}
write(directory, 'picked', [...hard.filter((_, index) => index % 17 === 0), ...edges.filter((_, index) => index % 97 === 0)], NAN_WORDS);
