// One public engine class per release the engine has a module for, and the registry that dispatches a release to its
// class. The registry's type makes the compiler refuse a release of SUPPORTED_VERSIONS without its class, a class
// under another release's key, and a key that is not a supported release.
import { isSupportedVersion, type SupportedVersion } from '../versions';
import type { NativeEngineOptions, PhysicsRegistry } from './base';
import { NativePhysics } from './player';

/** The options of a release's own engine class: the release is the class's. */
export type VersionedNativePhysicsOptions = Omit<NativeEngineOptions, 'version'>;

/** The engine class of one release. */
export interface VersionedNativePhysicsClass<V extends SupportedVersion = SupportedVersion> {
    readonly version: V;
    new (registry: PhysicsRegistry, options?: VersionedNativePhysicsOptions): NativePhysics & { readonly version: V };
}

/** What every release's engine class is: NativePhysics running that release. */
export abstract class VersionedNativePhysics<V extends SupportedVersion> extends NativePhysics {
    declare readonly version: V;

    protected constructor(version: V, registry: PhysicsRegistry, options: VersionedNativePhysicsOptions = {}) {
        super(registry, { ...options, version });
    }
}

/** The native engine of Minecraft 1.8.8. */
export class NativePhysics_1_8_8 extends VersionedNativePhysics<'1.8.8'> {
    static readonly version = '1.8.8';

    constructor(registry: PhysicsRegistry, options: VersionedNativePhysicsOptions = {}) {
        super('1.8.8', registry, options);
    }
}

/** The native engine of Minecraft 1.9.4. */
export class NativePhysics_1_9_4 extends VersionedNativePhysics<'1.9.4'> {
    static readonly version = '1.9.4';

    constructor(registry: PhysicsRegistry, options: VersionedNativePhysicsOptions = {}) {
        super('1.9.4', registry, options);
    }
}

/** The native engine of Minecraft 1.10.2. */
export class NativePhysics_1_10_2 extends VersionedNativePhysics<'1.10.2'> {
    static readonly version = '1.10.2';

    constructor(registry: PhysicsRegistry, options: VersionedNativePhysicsOptions = {}) {
        super('1.10.2', registry, options);
    }
}

/** The native engine of Minecraft 1.11.2. */
export class NativePhysics_1_11_2 extends VersionedNativePhysics<'1.11.2'> {
    static readonly version = '1.11.2';

    constructor(registry: PhysicsRegistry, options: VersionedNativePhysicsOptions = {}) {
        super('1.11.2', registry, options);
    }
}

/** The native engine of Minecraft 1.12.2. */
export class NativePhysics_1_12_2 extends VersionedNativePhysics<'1.12.2'> {
    static readonly version = '1.12.2';

    constructor(registry: PhysicsRegistry, options: VersionedNativePhysicsOptions = {}) {
        super('1.12.2', registry, options);
    }
}

/** The native engine of Minecraft 1.13.2. */
export class NativePhysics_1_13_2 extends VersionedNativePhysics<'1.13.2'> {
    static readonly version = '1.13.2';

    constructor(registry: PhysicsRegistry, options: VersionedNativePhysicsOptions = {}) {
        super('1.13.2', registry, options);
    }
}

/** The native engine of Minecraft 1.14.4. */
export class NativePhysics_1_14_4 extends VersionedNativePhysics<'1.14.4'> {
    static readonly version = '1.14.4';

    constructor(registry: PhysicsRegistry, options: VersionedNativePhysicsOptions = {}) {
        super('1.14.4', registry, options);
    }
}

/** The native engine of Minecraft 1.15. */
export class NativePhysics_1_15 extends VersionedNativePhysics<'1.15'> {
    static readonly version = '1.15';

    constructor(registry: PhysicsRegistry, options: VersionedNativePhysicsOptions = {}) {
        super('1.15', registry, options);
    }
}

/** The native engine of Minecraft 1.15.1. */
export class NativePhysics_1_15_1 extends VersionedNativePhysics<'1.15.1'> {
    static readonly version = '1.15.1';

    constructor(registry: PhysicsRegistry, options: VersionedNativePhysicsOptions = {}) {
        super('1.15.1', registry, options);
    }
}

/** The native engine of Minecraft 1.15.2. */
export class NativePhysics_1_15_2 extends VersionedNativePhysics<'1.15.2'> {
    static readonly version = '1.15.2';

    constructor(registry: PhysicsRegistry, options: VersionedNativePhysicsOptions = {}) {
        super('1.15.2', registry, options);
    }
}

/** The native engine of Minecraft 1.16. */
export class NativePhysics_1_16 extends VersionedNativePhysics<'1.16'> {
    static readonly version = '1.16';

    constructor(registry: PhysicsRegistry, options: VersionedNativePhysicsOptions = {}) {
        super('1.16', registry, options);
    }
}

/** The native engine of Minecraft 1.16.1. */
export class NativePhysics_1_16_1 extends VersionedNativePhysics<'1.16.1'> {
    static readonly version = '1.16.1';

    constructor(registry: PhysicsRegistry, options: VersionedNativePhysicsOptions = {}) {
        super('1.16.1', registry, options);
    }
}

/** The native engine of Minecraft 1.16.2. */
export class NativePhysics_1_16_2 extends VersionedNativePhysics<'1.16.2'> {
    static readonly version = '1.16.2';

    constructor(registry: PhysicsRegistry, options: VersionedNativePhysicsOptions = {}) {
        super('1.16.2', registry, options);
    }
}

/** The native engine of Minecraft 1.16.3. */
export class NativePhysics_1_16_3 extends VersionedNativePhysics<'1.16.3'> {
    static readonly version = '1.16.3';

    constructor(registry: PhysicsRegistry, options: VersionedNativePhysicsOptions = {}) {
        super('1.16.3', registry, options);
    }
}

/** The native engine of Minecraft 1.16.4. */
export class NativePhysics_1_16_4 extends VersionedNativePhysics<'1.16.4'> {
    static readonly version = '1.16.4';

    constructor(registry: PhysicsRegistry, options: VersionedNativePhysicsOptions = {}) {
        super('1.16.4', registry, options);
    }
}

/** The native engine of Minecraft 1.16.5. */
export class NativePhysics_1_16_5 extends VersionedNativePhysics<'1.16.5'> {
    static readonly version = '1.16.5';

    constructor(registry: PhysicsRegistry, options: VersionedNativePhysicsOptions = {}) {
        super('1.16.5', registry, options);
    }
}

/** The native engine of Minecraft 1.17. */
export class NativePhysics_1_17 extends VersionedNativePhysics<'1.17'> {
    static readonly version = '1.17';

    constructor(registry: PhysicsRegistry, options: VersionedNativePhysicsOptions = {}) {
        super('1.17', registry, options);
    }
}

/** The native engine of Minecraft 1.17.1. */
export class NativePhysics_1_17_1 extends VersionedNativePhysics<'1.17.1'> {
    static readonly version = '1.17.1';

    constructor(registry: PhysicsRegistry, options: VersionedNativePhysicsOptions = {}) {
        super('1.17.1', registry, options);
    }
}

/** The native engine of Minecraft 1.18. */
export class NativePhysics_1_18 extends VersionedNativePhysics<'1.18'> {
    static readonly version = '1.18';

    constructor(registry: PhysicsRegistry, options: VersionedNativePhysicsOptions = {}) {
        super('1.18', registry, options);
    }
}

/** The native engine of Minecraft 1.18.1. */
export class NativePhysics_1_18_1 extends VersionedNativePhysics<'1.18.1'> {
    static readonly version = '1.18.1';

    constructor(registry: PhysicsRegistry, options: VersionedNativePhysicsOptions = {}) {
        super('1.18.1', registry, options);
    }
}

/** The native engine of Minecraft 1.18.2. */
export class NativePhysics_1_18_2 extends VersionedNativePhysics<'1.18.2'> {
    static readonly version = '1.18.2';

    constructor(registry: PhysicsRegistry, options: VersionedNativePhysicsOptions = {}) {
        super('1.18.2', registry, options);
    }
}

/** The native engine of Minecraft 1.19. */
export class NativePhysics_1_19 extends VersionedNativePhysics<'1.19'> {
    static readonly version = '1.19';

    constructor(registry: PhysicsRegistry, options: VersionedNativePhysicsOptions = {}) {
        super('1.19', registry, options);
    }
}

/** The native engine of Minecraft 1.19.1. */
export class NativePhysics_1_19_1 extends VersionedNativePhysics<'1.19.1'> {
    static readonly version = '1.19.1';

    constructor(registry: PhysicsRegistry, options: VersionedNativePhysicsOptions = {}) {
        super('1.19.1', registry, options);
    }
}

/** The native engine of Minecraft 1.19.2. */
export class NativePhysics_1_19_2 extends VersionedNativePhysics<'1.19.2'> {
    static readonly version = '1.19.2';

    constructor(registry: PhysicsRegistry, options: VersionedNativePhysicsOptions = {}) {
        super('1.19.2', registry, options);
    }
}

/** The native engine of Minecraft 1.19.3. */
export class NativePhysics_1_19_3 extends VersionedNativePhysics<'1.19.3'> {
    static readonly version = '1.19.3';

    constructor(registry: PhysicsRegistry, options: VersionedNativePhysicsOptions = {}) {
        super('1.19.3', registry, options);
    }
}

/** The native engine of Minecraft 1.19.4. */
export class NativePhysics_1_19_4 extends VersionedNativePhysics<'1.19.4'> {
    static readonly version = '1.19.4';

    constructor(registry: PhysicsRegistry, options: VersionedNativePhysicsOptions = {}) {
        super('1.19.4', registry, options);
    }
}

/** The native engine of Minecraft 1.20. */
export class NativePhysics_1_20 extends VersionedNativePhysics<'1.20'> {
    static readonly version = '1.20';

    constructor(registry: PhysicsRegistry, options: VersionedNativePhysicsOptions = {}) {
        super('1.20', registry, options);
    }
}

/** The native engine of Minecraft 1.20.1. */
export class NativePhysics_1_20_1 extends VersionedNativePhysics<'1.20.1'> {
    static readonly version = '1.20.1';

    constructor(registry: PhysicsRegistry, options: VersionedNativePhysicsOptions = {}) {
        super('1.20.1', registry, options);
    }
}

/** The native engine of Minecraft 1.20.2. */
export class NativePhysics_1_20_2 extends VersionedNativePhysics<'1.20.2'> {
    static readonly version = '1.20.2';

    constructor(registry: PhysicsRegistry, options: VersionedNativePhysicsOptions = {}) {
        super('1.20.2', registry, options);
    }
}

/** The native engine of Minecraft 1.20.3. */
export class NativePhysics_1_20_3 extends VersionedNativePhysics<'1.20.3'> {
    static readonly version = '1.20.3';

    constructor(registry: PhysicsRegistry, options: VersionedNativePhysicsOptions = {}) {
        super('1.20.3', registry, options);
    }
}

/** The native engine of Minecraft 1.20.4. */
export class NativePhysics_1_20_4 extends VersionedNativePhysics<'1.20.4'> {
    static readonly version = '1.20.4';

    constructor(registry: PhysicsRegistry, options: VersionedNativePhysicsOptions = {}) {
        super('1.20.4', registry, options);
    }
}

/** The native engine of Minecraft 1.20.5. */
export class NativePhysics_1_20_5 extends VersionedNativePhysics<'1.20.5'> {
    static readonly version = '1.20.5';

    constructor(registry: PhysicsRegistry, options: VersionedNativePhysicsOptions = {}) {
        super('1.20.5', registry, options);
    }
}

/** The native engine of Minecraft 1.20.6. */
export class NativePhysics_1_20_6 extends VersionedNativePhysics<'1.20.6'> {
    static readonly version = '1.20.6';

    constructor(registry: PhysicsRegistry, options: VersionedNativePhysicsOptions = {}) {
        super('1.20.6', registry, options);
    }
}

/** The native engine of Minecraft 1.21. */
export class NativePhysics_1_21 extends VersionedNativePhysics<'1.21'> {
    static readonly version = '1.21';

    constructor(registry: PhysicsRegistry, options: VersionedNativePhysicsOptions = {}) {
        super('1.21', registry, options);
    }
}

/** The native engine of Minecraft 1.21.1. */
export class NativePhysics_1_21_1 extends VersionedNativePhysics<'1.21.1'> {
    static readonly version = '1.21.1';

    constructor(registry: PhysicsRegistry, options: VersionedNativePhysicsOptions = {}) {
        super('1.21.1', registry, options);
    }
}

/** The native engine of Minecraft 1.21.2. */
export class NativePhysics_1_21_2 extends VersionedNativePhysics<'1.21.2'> {
    static readonly version = '1.21.2';

    constructor(registry: PhysicsRegistry, options: VersionedNativePhysicsOptions = {}) {
        super('1.21.2', registry, options);
    }
}

/** The native engine of Minecraft 1.21.3. */
export class NativePhysics_1_21_3 extends VersionedNativePhysics<'1.21.3'> {
    static readonly version = '1.21.3';

    constructor(registry: PhysicsRegistry, options: VersionedNativePhysicsOptions = {}) {
        super('1.21.3', registry, options);
    }
}

/** The native engine of Minecraft 1.21.4. */
export class NativePhysics_1_21_4 extends VersionedNativePhysics<'1.21.4'> {
    static readonly version = '1.21.4';

    constructor(registry: PhysicsRegistry, options: VersionedNativePhysicsOptions = {}) {
        super('1.21.4', registry, options);
    }
}

/** The native engine of Minecraft 1.21.5. */
export class NativePhysics_1_21_5 extends VersionedNativePhysics<'1.21.5'> {
    static readonly version = '1.21.5';

    constructor(registry: PhysicsRegistry, options: VersionedNativePhysicsOptions = {}) {
        super('1.21.5', registry, options);
    }
}

/** The native engine of Minecraft 1.21.6. */
export class NativePhysics_1_21_6 extends VersionedNativePhysics<'1.21.6'> {
    static readonly version = '1.21.6';

    constructor(registry: PhysicsRegistry, options: VersionedNativePhysicsOptions = {}) {
        super('1.21.6', registry, options);
    }
}

/** The native engine of Minecraft 1.21.7. */
export class NativePhysics_1_21_7 extends VersionedNativePhysics<'1.21.7'> {
    static readonly version = '1.21.7';

    constructor(registry: PhysicsRegistry, options: VersionedNativePhysicsOptions = {}) {
        super('1.21.7', registry, options);
    }
}

/** The native engine of Minecraft 1.21.8. */
export class NativePhysics_1_21_8 extends VersionedNativePhysics<'1.21.8'> {
    static readonly version = '1.21.8';

    constructor(registry: PhysicsRegistry, options: VersionedNativePhysicsOptions = {}) {
        super('1.21.8', registry, options);
    }
}

/** The native engine of Minecraft 1.21.9. */
export class NativePhysics_1_21_9 extends VersionedNativePhysics<'1.21.9'> {
    static readonly version = '1.21.9';

    constructor(registry: PhysicsRegistry, options: VersionedNativePhysicsOptions = {}) {
        super('1.21.9', registry, options);
    }
}

/** The native engine of Minecraft 1.21.10. */
export class NativePhysics_1_21_10 extends VersionedNativePhysics<'1.21.10'> {
    static readonly version = '1.21.10';

    constructor(registry: PhysicsRegistry, options: VersionedNativePhysicsOptions = {}) {
        super('1.21.10', registry, options);
    }
}

/** The native engine of Minecraft 1.21.11. */
export class NativePhysics_1_21_11 extends VersionedNativePhysics<'1.21.11'> {
    static readonly version = '1.21.11';

    constructor(registry: PhysicsRegistry, options: VersionedNativePhysicsOptions = {}) {
        super('1.21.11', registry, options);
    }
}

/** The native engine of Minecraft 26.1. */
export class NativePhysics_26_1 extends VersionedNativePhysics<'26.1'> {
    static readonly version = '26.1';

    constructor(registry: PhysicsRegistry, options: VersionedNativePhysicsOptions = {}) {
        super('26.1', registry, options);
    }
}

/** The native engine of Minecraft 26.1.1. */
export class NativePhysics_26_1_1 extends VersionedNativePhysics<'26.1.1'> {
    static readonly version = '26.1.1';

    constructor(registry: PhysicsRegistry, options: VersionedNativePhysicsOptions = {}) {
        super('26.1.1', registry, options);
    }
}

/** The native engine of Minecraft 26.1.2. */
export class NativePhysics_26_1_2 extends VersionedNativePhysics<'26.1.2'> {
    static readonly version = '26.1.2';

    constructor(registry: PhysicsRegistry, options: VersionedNativePhysicsOptions = {}) {
        super('26.1.2', registry, options);
    }
}

/** The native engine of Minecraft 26.2. */
export class NativePhysics_26_2 extends VersionedNativePhysics<'26.2'> {
    static readonly version = '26.2';

    constructor(registry: PhysicsRegistry, options: VersionedNativePhysicsOptions = {}) {
        super('26.2', registry, options);
    }
}

/** The native engine of Minecraft 26.3. */
export class NativePhysics_26_3 extends VersionedNativePhysics<'26.3'> {
    static readonly version = '26.3';

    constructor(registry: PhysicsRegistry, options: VersionedNativePhysicsOptions = {}) {
        super('26.3', registry, options);
    }
}

/** Each supported release's engine class. */
export const NATIVE_PHYSICS_ENGINES = {
    '1.8.8': NativePhysics_1_8_8,
    '1.9.4': NativePhysics_1_9_4,
    '1.10.2': NativePhysics_1_10_2,
    '1.11.2': NativePhysics_1_11_2,
    '1.12.2': NativePhysics_1_12_2,
    '1.13.2': NativePhysics_1_13_2,
    '1.14.4': NativePhysics_1_14_4,
    '1.15': NativePhysics_1_15,
    '1.15.1': NativePhysics_1_15_1,
    '1.15.2': NativePhysics_1_15_2,
    '1.16': NativePhysics_1_16,
    '1.16.1': NativePhysics_1_16_1,
    '1.16.2': NativePhysics_1_16_2,
    '1.16.3': NativePhysics_1_16_3,
    '1.16.4': NativePhysics_1_16_4,
    '1.16.5': NativePhysics_1_16_5,
    '1.17': NativePhysics_1_17,
    '1.17.1': NativePhysics_1_17_1,
    '1.18': NativePhysics_1_18,
    '1.18.1': NativePhysics_1_18_1,
    '1.18.2': NativePhysics_1_18_2,
    '1.19': NativePhysics_1_19,
    '1.19.1': NativePhysics_1_19_1,
    '1.19.2': NativePhysics_1_19_2,
    '1.19.3': NativePhysics_1_19_3,
    '1.19.4': NativePhysics_1_19_4,
    '1.20': NativePhysics_1_20,
    '1.20.1': NativePhysics_1_20_1,
    '1.20.2': NativePhysics_1_20_2,
    '1.20.3': NativePhysics_1_20_3,
    '1.20.4': NativePhysics_1_20_4,
    '1.20.5': NativePhysics_1_20_5,
    '1.20.6': NativePhysics_1_20_6,
    '1.21': NativePhysics_1_21,
    '1.21.1': NativePhysics_1_21_1,
    '1.21.2': NativePhysics_1_21_2,
    '1.21.3': NativePhysics_1_21_3,
    '1.21.4': NativePhysics_1_21_4,
    '1.21.5': NativePhysics_1_21_5,
    '1.21.6': NativePhysics_1_21_6,
    '1.21.7': NativePhysics_1_21_7,
    '1.21.8': NativePhysics_1_21_8,
    '1.21.9': NativePhysics_1_21_9,
    '1.21.10': NativePhysics_1_21_10,
    '1.21.11': NativePhysics_1_21_11,
    '26.1': NativePhysics_26_1,
    '26.1.1': NativePhysics_26_1_1,
    '26.1.2': NativePhysics_26_1_2,
    '26.2': NativePhysics_26_2,
    '26.3': NativePhysics_26_3,
} as const satisfies { readonly [V in SupportedVersion]: VersionedNativePhysicsClass<V> };

/**
 * The engine of a release (`options.version`, else the registry's): an instance of that release's class. A release
 * without one is refused here, before any binding is loaded.
 */
export function createNativePhysics(registry: PhysicsRegistry, options: NativeEngineOptions = {}): NativePhysics {
    const version = options.version ?? (registry as unknown as { version: { minecraftVersion: string } }).version.minecraftVersion;
    if (!isSupportedVersion(version)) {
        throw new Error(`unsupported decompiled Minecraft version: ${version}`);
    }
    const Engine: VersionedNativePhysicsClass = NATIVE_PHYSICS_ENGINES[version];
    return new Engine(registry, { binding: options.binding });
}
