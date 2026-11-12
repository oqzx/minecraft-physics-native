// Entity.isInRain from what a mineflayer bot knows: the rain level of its packets, the blocks and sky light of its chunks and
// the biome of a block. Vanilla's Level.isRainingAt per release:
//
//   1.8 - 1.12  no snow biome, not cold (temperature above 0.15), a biome with rain (1.8.8 Level.java:2534-2549, :1987-1991)
//   1.13 - 1.15 a biome whose precipitation is rain, whatever the height (1.13.2 Level.java, 1.15 Level.java hasRain)
//   1.16 on     that, and 0.15 warm where the height has cooled it (1.16 Level.java hasRain, 26.3 Biome.java:83-99, :152-154)
//
// The temperature falls with the height above y 64, by /30 and noise * 4, until 1.18: then above the sea level + 17, by /40
// and noise * 8 (1.17 Biome.java:133-141, 1.18 :107-115, 26.3 :91-99). The noise is seeded by constants, not by the world.
import type { VersionCheck } from './blocks';

const fround = Math.fround;
/** Math.floor as a Java int. */
const floor = (value: number): number => Math.floor(value) | 0;

/** java.util.Random, which WorldgenRandom over LegacyRandomSource repeats (nextDouble's 2^-53 is the float constant 1.110223E-16F). */
export class JavaRandom {
    /** The 48-bit state in two 24-bit halves, whose products a double holds. */
    private high: number;
    private low: number;

    /** @param seed below 2^24, as the constants here are. */
    constructor(seed: number) {
        this.high = 0x5DE;
        this.low = (seed ^ 0xECE66D) & 0xFFFFFF;
    }

    private next(bits: number): number {
        const low = this.low * 0xECE66D + 0xB;
        const high = this.high * 0xECE66D + this.low * 0x5DE + Math.floor(low / 0x1000000);
        this.low = low % 0x1000000;
        this.high = high % 0x1000000;
        return Math.floor((this.high * 0x1000000 + this.low) / 2 ** (48 - bits)) | 0;
    }

    nextInt(bound: number): number {
        if ((bound & -bound) === bound) return Math.floor(bound * this.next(31) / 0x80000000);
        let bits: number;
        let value: number;
        do {
            bits = this.next(31);
            value = bits % bound;
        } while (bits - value + (bound - 1) > 0x7fffffff);
        return value;
    }

    nextDouble(): number {
        return (this.next(26) * 134217728 + this.next(27)) * 2 ** -53;
    }
}

const GRADIENTS = [[1, 1], [-1, 1], [1, -1], [-1, -1], [1, 0], [-1, 0], [1, 0], [-1, 0], [0, 1], [0, -1], [0, 1], [0, -1]] as const;
const SQRT_3 = Math.sqrt(3);
const F2 = 0.5 * (SQRT_3 - 1);
const G2 = (3 - SQRT_3) / 6;

/** SimplexNoise's 2D value (1.8.8 SimplexNoise.java:48-105, 1.16.5 SimplexNoise.java:72, 26.3 SimplexNoise.java:39). */
class Simplex {
    private readonly permutation = new Int32Array(512);

    /** @param floorOf the 1.8 - 1.12 sampler's own floor, which an integer below 0 takes one further down, or Mth.floor. */
    constructor(random: JavaRandom, private readonly floorOf: (value: number) => number) {
        random.nextDouble();
        random.nextDouble();
        random.nextDouble();
        const table = this.permutation;
        for (let i = 0; i < 256; i += 1) table[i] = i;
        for (let i = 0; i < 256; i += 1) {
            const j = random.nextInt(256 - i) + i;
            const held = table[i]!;
            table[i] = table[j]!;
            table[j] = held;
            table[i + 256] = table[i]!;
        }
    }

    /** SimplexNoise.getValue (1.8.8 SimplexNoise.java:48-105). */
    sample(xin: number, yin: number): number {
        const table = this.permutation;
        const s = (xin + yin) * F2;
        const i = this.floorOf(xin + s);
        const j = this.floorOf(yin + s);
        const t = (i + j) * G2;
        const x0 = xin - (i - t);
        const y0 = yin - (j - t);
        const i1 = x0 > y0 ? 1 : 0;
        const j1 = x0 > y0 ? 0 : 1;
        const x1 = x0 - i1 + G2;
        const y1 = y0 - j1 + G2;
        const x2 = x0 - 1 + 2 * G2;
        const y2 = y0 - 1 + 2 * G2;
        const ii = i & 255;
        const jj = j & 255;
        const corner = (gradient: number, x: number, y: number): number => {
            const base = 0.5 - x * x - y * y;
            if (base < 0) return 0;
            const [gx, gy] = GRADIENTS[gradient]!;
            return base * base * base * base * (gx * x + gy * y);
        };
        const n0 = corner(table[ii + table[jj]!]! % 12, x0, y0);
        const n1 = corner(table[ii + i1 + table[jj + j1]!]! % 12, x1, y1);
        const n2 = corner(table[ii + 1 + table[jj + 1]!]! % 12, x2, y2);
        return 70 * (n0 + n1 + n2);
    }
}

const legacyFloor = (value: number): number => (value > 0 ? Math.trunc(value) : Math.trunc(value) - 1);

/** The temperature noise of the release: the old sampler to 1.12, one octave of the simplex sampler after. Made once. */
const temperatureNoise = new Map<boolean, Simplex>();
function temperatureSampler(legacy: boolean): Simplex {
    let sampler = temperatureNoise.get(legacy);
    if (!sampler) temperatureNoise.set(legacy, sampler = new Simplex(new JavaRandom(1234), legacy ? legacyFloor : floor));
    return sampler;
}

/** Biome.TEMPERATURE_NOISE. */
export const temperatureNoiseAt = (legacy: boolean, x: number, z: number): number => temperatureSampler(legacy).sample(x, z);

let frozen: { large: Simplex[]; info: Simplex } | undefined;

function frozenSamplers(): { large: Simplex[]; info: Simplex } {
    if (!frozen) {
        const random = new JavaRandom(3456);
        frozen = {
            large: [new Simplex(random, floor), new Simplex(random, floor), new Simplex(random, floor)],
            info: new Simplex(new JavaRandom(2345), floor),
        };
    }
    return frozen;
}

/** FROZEN_TEMPERATURE_NOISE: the octaves -2, -1, 0, frequency 1, 1/2, 1/4 at amplitude 1/7, 2/7, 4/7 (PerlinSimplexNoise.getValue). */
export function frozenNoiseAt(x: number, z: number): number {
    const [a, b, c] = frozenSamplers().large;
    return a!.sample(x, z) / 7 + b!.sample(x * 0.5, z * 0.5) * 2 / 7 + c!.sample(x * 0.25, z * 0.25) * 4 / 7;
}

/** BIOME_INFO_NOISE, FOLIAGE_NOISE before. */
export const biomeInfoNoiseAt = (x: number, z: number): number => frozenSamplers().info.sample(x, z);

/** TemperatureModifier.FROZEN, of the frozen oceans from 1.16 (1.16.5 Biome.java:629-642, 26.3 :271-283). */
function frozenTemperature(x: number, z: number, temperature: number): number {
    if (frozenNoiseAt(x * 0.05, z * 0.05) * 7 + biomeInfoNoiseAt(x * 0.2, z * 0.2) < 0.3 && biomeInfoNoiseAt(x * 0.09, z * 0.09) < 0.8) return fround(0.2);
    return temperature;
}

/** A biome as minecraft-data gives it. */
export interface BiomeData {
    name?: string;
    temperature?: number;
    /** To 1.19.3. */
    precipitation?: string;
    /** From 1.19.4. */
    has_precipitation?: boolean;
}

/** Biome.getTemperature(pos) of the release. */
export function biomeTemperature(version: VersionCheck, biome: BiomeData, x: number, y: number, z: number, seaLevel: number): number {
    const base = fround(biome.temperature ?? 0.5);
    const legacy = version['<']('1.13');
    const adjusted = !legacy && (biome.name === 'frozen_ocean' || biome.name === 'deep_frozen_ocean') ? frozenTemperature(x, z, base) : base;
    const modern = version['>=']('1.18');
    const snowLine = version['>=']('1.21.2') ? seaLevel + 17 : modern ? 80 : 64;
    if (y <= snowLine) return adjusted;
    const at = (value: number) => (version['<']('1.9') ? value * 1 / 8 : fround(value / 8));
    const noise = temperatureSampler(legacy).sample(at(x), at(z)) * (modern ? 8 : 4);
    return fround(adjusted - fround(fround(fround(noise) + y - snowLine) * fround(0.05)) / (modern ? 40 : 30));
}

/** Whether rain, not snow or nothing, falls in a biome at a position with the sky open above it (the last steps of isRainingAt). */
export function rainsInBiome(version: VersionCheck, biome: BiomeData, x: number, y: number, z: number, seaLevel: number): boolean {
    if (version['<']('1.13')) {
        if (biome.precipitation === 'snow' || biome.precipitation === 'none') return false;
        return biomeTemperature(version, biome, x, y, z, seaLevel) > fround(0.15);
    }
    const falls = biome.has_precipitation ?? biome.precipitation === 'rain';
    if (version['<']('1.16')) return falls;
    return falls && biomeTemperature(version, biome, x, y, z, seaLevel) >= fround(0.15);
}
