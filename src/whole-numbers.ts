// N-API reads a JavaScript number into a Rust integer by truncating a fraction, wrapping what is past the type and turning
// NaN and infinity into 0, so what the engine holds would not be what the caller sent (B209, docs/numeric-contract.md).
// Each call is checked here against the binding's own types (src/whole-fields.ts) before any of it reaches the addon. Doubles
// have nothing to check here: the engine's validation has them.
import { WHOLE_FUNCTIONS, WHOLE_INTERFACES, WHOLE_METHODS } from './whole-fields';

type Whole = 'u8' | 'u16' | 'u32' | 'i32' | 'i64';
type Kind = Whole | { readonly of: string } | { readonly list: Kind };
type Check = (value: unknown, where: string) => void;
type Method = (...args: unknown[]) => unknown;

const BOUNDS: Record<Whole, readonly [number, number]> = {
    u8: [0, 255],
    u16: [0, 65535],
    u32: [0, 4294967295],
    i32: [-2147483648, 2147483647],
    i64: [Number.MIN_SAFE_INTEGER, Number.MAX_SAFE_INTEGER],
};

const absent = (value: unknown): boolean => value === undefined || value === null;

function checkWhole(kind: Whole): Check {
    const [low, high] = BOUNDS[kind];
    return (value, where) => {
        if (absent(value) || typeof value !== 'number') return; // the binding refuses a value of another type by itself
        if (!Number.isInteger(value) || value < low || value > high) {
            throw new RangeError(`${where} is ${value}: it must be a whole number from ${low} to ${high}`);
        }
    };
}

const interfaces = new Map<string, Check>();

/** The check of an interface, made once. A later reference finds it, so a type that holds itself ends. */
function checkInterface(name: string): Check {
    const made = interfaces.get(name);
    if (made) return made;
    const fields: [string, Check][] = [];
    const check: Check = (value, where) => {
        if (absent(value) || typeof value !== 'object') return;
        const object = value as Record<string, unknown>;
        for (const [field, inner] of fields) {
            const held = object[field];
            if (held !== undefined && held !== null) inner(held, `${where}.${field}`);
        }
    };
    interfaces.set(name, check);
    const table = WHOLE_INTERFACES as unknown as Record<string, Record<string, Kind>>;
    for (const [field, kind] of Object.entries(table[name])) fields.push([field, checkOf(kind)]);
    return check;
}

function checkOf(kind: Kind): Check {
    if (typeof kind === 'string') return checkWhole(kind);
    if ('of' in kind) return checkInterface(kind.of);
    const item = checkOf(kind.list);
    return (value, where) => {
        if (!Array.isArray(value)) return;
        for (let index = 0; index < value.length; index += 1) item(value[index], `${where}[${index}]`);
    };
}

type Parameter = { readonly name: string; readonly kind: Kind } | null;

function guarded(call: Method, label: string, parameters: readonly Parameter[]): Method {
    const checks = parameters.flatMap((parameter, index) => (parameter ? [{ index, where: `${label}: ${parameter.name}`, check: checkOf(parameter.kind) }] : []));
    return function guardedCall(this: unknown, ...args: unknown[]) {
        for (const { index, where, check } of checks) check(args[index], where);
        return call.apply(this, args);
    };
}

const done = new WeakSet<object>();

/** Checks every call of a loaded binding that takes a whole number. A binding is guarded once. */
export function guardWholeNumbers(binding: object): void {
    const exports = binding as Record<string, unknown>;
    if (done.has(binding)) return;
    done.add(binding);
    for (const [name, parameters] of Object.entries(WHOLE_FUNCTIONS as unknown as Record<string, readonly Parameter[]>)) {
        const call = exports[name];
        if (typeof call === 'function') exports[name] = guarded(call as Method, name, parameters);
    }
    for (const [owner, methods] of Object.entries(WHOLE_METHODS as unknown as Record<string, Record<string, readonly Parameter[]>>)) {
        const prototype = (exports[owner] as { prototype?: Record<string, unknown> } | undefined)?.prototype;
        if (!prototype) continue;
        for (const [name, parameters] of Object.entries(methods)) {
            const call = prototype[name];
            if (name !== 'constructor' && typeof call === 'function') prototype[name] = guarded(call as Method, `${owner}.${name}`, parameters);
        }
        const construction = Object.entries(methods).find(([name]) => name === 'constructor')?.[1];
        if (construction) {
            const check = guarded((...args) => args, `new ${owner}`, construction);
            exports[owner] = new Proxy(exports[owner] as object, {
                construct(target, args, newTarget) {
                    check(...(args as unknown[]));
                    return Reflect.construct(target as new (...made: unknown[]) => object, args, newTarget);
                },
            });
        }
    }
}
