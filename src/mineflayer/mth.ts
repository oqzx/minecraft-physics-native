// Vanilla's float rotation math the adapter needs around the engine: Mth.sin / Mth.cos (a 65536-entry float table, a
// float argument scaled by 10430.378F before 1.21.11, a double one truncated as a long since) and the view vector.
const f32 = Math.fround;

let legacyTable: Float32Array | undefined;
let modernTable: Float32Array | undefined;

function legacy(): Float32Array {
    if (!legacyTable) {
        legacyTable = new Float32Array(65536);
        for (let i = 0; i < 65536; i += 1) legacyTable[i] = Math.sin((i * Math.PI * 2) / 65536);
    }
    return legacyTable;
}

function modern(): Float32Array {
    if (!modernTable) {
        modernTable = new Float32Array(65536);
        for (let i = 0; i < 65536; i += 1) modernTable[i] = Math.sin(i / 10430.378350470453);
    }
    return modernTable;
}

/** Java's (int) of a float or double: toward zero, NaN to 0, saturating at the int range. */
function javaInt(value: number): number {
    if (Number.isNaN(value)) return 0;
    if (value >= 2147483647) return 2147483647;
    if (value <= -2147483648) return -2147483648;
    return Math.trunc(value);
}

/** The low 16 bits of Java's (long) of a double. */
function javaLongLow16(value: number): number {
    if (Number.isNaN(value)) return 0;
    if (value >= 2 ** 63) return 65535;
    if (value < -(2 ** 63)) return 0;
    return Math.trunc(value) & 65535;
}

export interface Mth {
    sin(radians: number): number;
    cos(radians: number): number;
}

const LEGACY_SCALE = f32(10430.378);

const legacyMth: Mth = {
    sin: radians => legacy()[javaInt(f32(f32(radians) * LEGACY_SCALE)) & 65535],
    cos: radians => legacy()[javaInt(f32(f32(f32(radians) * LEGACY_SCALE) + 16384)) & 65535],
};

const modernMth: Mth = {
    sin: radians => modern()[javaLongLow16(radians * 10430.378350470453)],
    cos: radians => modern()[javaLongLow16(radians * 10430.378350470453 + 16384)],
};

/** The Mth of a release: the double argument from 1.21.11. */
export function mthFor(doubleArgument: boolean): Mth {
    return doubleArgument ? modernMth : legacyMth;
}

/** Degrees to radians as vanilla scales a float rotation: degrees * 0.017453292F, in float. */
export function floatRadians(degrees: number): number {
    return f32(f32(degrees) * f32(0.017453292));
}

/**
 * Entity.calculateViewVector(pitch, yaw) of float degrees. Before 1.13 Entity.getVectorForRotation turns the yaw half a
 * turn and negates the pitch's cosine instead.
 */
export function viewVector(mth: Mth, legacyRotation: boolean, pitch: number, yaw: number): { x: number; y: number; z: number } {
    if (legacyRotation) {
        const yawRadians = f32(f32(f32(-yaw) * f32(0.017453292)) - f32(Math.PI));
        const pitchRadians = f32(f32(-pitch) * f32(0.017453292));
        const pitchCos = -mth.cos(pitchRadians);
        return { x: f32(mth.sin(yawRadians) * pitchCos), y: mth.sin(pitchRadians), z: f32(mth.cos(yawRadians) * pitchCos) };
    }
    const pitchRadians = f32(pitch * f32(0.017453292));
    const yawRadians = f32(f32(-yaw) * f32(0.017453292));
    const pitchCos = mth.cos(pitchRadians);
    return { x: f32(mth.sin(yawRadians) * pitchCos), y: -mth.sin(pitchRadians), z: f32(mth.cos(yawRadians) * pitchCos) };
}
