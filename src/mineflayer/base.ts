// What the adapter's engines share: the registry they were made for, the native binding, the NativeWorld kept in step
// with the caller's world, and the rest of mineflayer-physics-utils' IPhysics, which its states and EPhysicsCtx call
// (effect and enchantment levels, supportFeature).
import type { IPhysics } from '@nxg-org/mineflayer-physics-util/dist/physics/engines/IPhysics';

import { LoadNativePhysics, type NativePhysicsBinding } from '../native';
import type { Attribute, Attributes } from './types';
import { NativeWorldSync, type PrismarineWorld, type SyncRegistry } from './world';

/** The parts of a prismarine registry (minecraft-data) the adapter reads. */
interface Registry extends SyncRegistry {
    version: SyncRegistry['version'] & { minecraftVersion: string; '>'(version: string): boolean };
    attributesByName?: Record<string, { resource: string; min: number; max: number } | undefined>;
    effectsByName: Record<string, { id: number } | undefined>;
    enchantmentsByName: Record<string, { id: number } | undefined>;
    supportFeature?(feature: string): unknown;
}

type AABB = Parameters<IPhysics['getSurroundingBBs']>[0];
type AABBClass = new (minX: number, minY: number, minZ: number, maxX: number, maxY: number, maxZ: number) => AABB;

export interface NativeEngineOptions {
    /** The native binding (LoadNativePhysics() by default). */
    binding?: NativePhysicsBinding;
    /** The release the engine runs (the registry's by default); the engine refuses one it has no module for. */
    version?: string;
}

/** mineflayer-physics-utils' CheapEffects and CheapEnchantments, by their numbers. */
const EFFECT_NAMES: IPhysics['statusEffectNames'] = Object.freeze({
    0: 'JumpBoost', 1: 'Speed', 2: 'Slowness', 3: 'DolphinsGrace', 4: 'SlowFalling', 5: 'Levitation', 6: 'Blindness',
});
const ENCHANTMENT_NAMES: IPhysics['enchantmentNames'] = Object.freeze({ 0: 'depth_strider', 1: 'swift_sneak', 2: 'soul_speed', 3: 'riptide' });

/** A comparison of releases, or `unknown` when minecraft-data does not know the other release (it throws). */
function compared(compare: () => boolean, unknown: boolean): boolean {
    try {
        return compare();
    } catch {
        return unknown;
    }
}

/** A prismarine registry (minecraft-data's IndexedData of a release). */
export type PhysicsRegistry = IPhysics['data'];

export class NativeEngine implements Omit<IPhysics, 'simulate'> {
    readonly data: PhysicsRegistry;
    readonly binding: NativePhysicsBinding;
    readonly version: string;
    readonly statusEffectNames = EFFECT_NAMES;
    readonly enchantmentNames = ENCHANTMENT_NAMES;
    private readonly registry: Registry;
    private readonly syncs = new WeakMap<object, NativeWorldSync>();
    private attributeRanges?: Map<string, { min: number; max: number }>;
    private readonly attributeKeys = new Map<string, string | null>();
    private readonly older = new Map<string, boolean>();
    private readonly newer = new Map<string, boolean>();

    constructor(registry: PhysicsRegistry, options: NativeEngineOptions = {}) {
        this.data = registry;
        this.registry = registry as unknown as Registry;
        this.binding = options.binding ?? LoadNativePhysics();
        this.version = options.version ?? this.registry.version.minecraftVersion;
    }

    readonly supportFeature = (feature: string): boolean => !!this.registry.supportFeature?.(feature);

    /** Whether the registry's release is older than `version` (one its minecraft-data does not know yet is newer). */
    protected lt(version: string): boolean {
        let older = this.older.get(version);
        if (older === undefined) this.older.set(version, (older = compared(() => this.registry.version['<'](version), true)));
        return older;
    }

    protected gt(version: string): boolean {
        let newer = this.newer.get(version);
        if (newer === undefined) this.newer.set(version, (newer = compared(() => this.registry.version['>'](version), false)));
        return newer;
    }

    /** The NativeWorld kept in step with a prismarine world. */
    syncOf(world: PrismarineWorld): NativeWorldSync {
        let sync = this.syncs.get(world);
        if (!sync) {
            sync = new NativeWorldSync(this.binding, this.registry);
            this.syncs.set(world, sync);
        }
        return sync;
    }

    /** The level of an effect (its amplifier + 1, 0 without it), as mineflayer-physics-utils counts it. */
    getEffectLevel(effect: number, effects: object): number {
        const descriptor = this.registry.effectsByName[this.statusEffectNames[effect as keyof typeof EFFECT_NAMES]];
        if (!descriptor) return 0;
        const info = (effects as unknown as Record<number, { amplifier: number } | undefined>)[descriptor.id];
        return info ? info.amplifier + 1 : 0;
    }

    getEnchantmentLevel(enchantment: number, enchantments: { id: string | number; lvl: number }[]): number {
        const name = this.enchantmentNames[enchantment as keyof typeof ENCHANTMENT_NAMES];
        const descriptor = this.registry.enchantmentsByName[name];
        if (!descriptor) return 0;
        for (const info of enchantments) {
            if (typeof info.id === 'string' ? info.id.includes(name) : info.id === descriptor.id) return info.lvl;
        }
        return 0;
    }

    /** The collision boxes the engine sees around a box in a world it keeps in step, of the query's own AABB class. */
    getSurroundingBBs(queryBB: AABB, world: PrismarineWorld): AABB[] {
        const sync = this.syncOf(world);
        sync.sync(world, queryBB, 1);
        const Box = queryBB.constructor as AABBClass;
        return sync.collisions(queryBB).map(({ min, max }) => new Box(min.x, min.y, min.z, max.x, max.y, max.z));
    }

    /**
     * The key an attribute is synced under (mineflayer's bot.entity.attributes), from its minecraft-data name
     * ("movementSpeed"): its resource, namespaced on 1.16 - 1.20.4; null when the release has no such attribute.
     */
    attributeKey(name: string): string | null {
        let attributeKey = this.attributeKeys.get(name);
        if (attributeKey === undefined) {
            const data = this.registry.attributesByName?.[name];
            const namespaced = !this.lt('1.16') && this.lt('1.20.5');
            attributeKey = !data ? null : namespaced && !data.resource.startsWith('minecraft:') ? `minecraft:${data.resource}` : data.resource;
            this.attributeKeys.set(name, attributeKey);
        }
        return attributeKey;
    }

    /** The synced attribute of that name, when the state has it. */
    protected attribute(attributes: Attributes, name: string): Attribute | undefined {
        const key = this.attributeKey(name);
        return key === null ? undefined : attributes[key];
    }

    /** AttributeInstance.getValue clamped to the attribute's range (RangedAttribute.sanitizeValue). */
    protected attributeValue(name: string, attribute: Attribute): number {
        if (!this.attributeRanges) {
            this.attributeRanges = new Map();
            for (const [key, data] of Object.entries(this.registry.attributesByName ?? {})) {
                if (data) this.attributeRanges.set(key, { min: data.min, max: data.max });
            }
        }
        let base = attribute.value;
        for (const modifier of attribute.modifiers) if (modifier.operation === 0) base += modifier.amount;
        let value = base;
        for (const modifier of attribute.modifiers) if (modifier.operation === 1) value += base * modifier.amount;
        for (const modifier of attribute.modifiers) if (modifier.operation === 2) value += value * modifier.amount;
        const range = this.attributeRanges.get(name);
        return range ? Math.min(Math.max(value, range.min), range.max) : value;
    }

    /** The synced attribute's value with its modifiers, or undefined without it. */
    protected attributeValueOf(attributes: Attributes, name: string): number | undefined {
        const attribute = this.attribute(attributes, name);
        return attribute ? this.attributeValue(name, attribute) : undefined;
    }

    /** The synced attribute's base with its additions (the engine applies the sprint and the effects itself). */
    protected attributeBaseOf(attributes: Attributes, name: string): number | undefined {
        const attribute = this.attribute(attributes, name);
        if (!attribute) return undefined;
        return attribute.modifiers.filter(modifier => modifier.operation === 0).reduce((value, modifier) => value + modifier.amount, attribute.value);
    }
}
