// What the adapter's engines share: the registry they were made for, the native binding, the NativeWorld kept in step
// with the caller's world, and the rest of mineflayer-physics-utils' IPhysics, which its states and EPhysicsCtx call
// (effect and enchantment levels, supportFeature).
import type { IPhysics } from '@nxg-org/mineflayer-physics-util/dist/physics/engines/IPhysics';

import { LoadNativePhysics, type NativePhysicsBinding } from '../native';
import type { Attribute, AttributeModifier, Attributes } from './types';
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
        return this.effectLevelByName(this.statusEffectNames[effect as keyof typeof EFFECT_NAMES], effects);
    }

    /** The level of an effect by its minecraft-data name ("Weaving"), 0 without it or where the release has none. */
    protected effectLevelByName(name: string, effects: object): number {
        const amplifier = this.effectAmplifierByName(name, effects);
        return amplifier === undefined ? 0 : amplifier + 1;
    }

    /**
     * The amplifier of an effect by its minecraft-data name, undefined without it. Before 1.20.5 the effect packet reads
     * it as a signed byte and MobEffectInstance keeps it, so 255 is -1 and the effect stays (1.20.4
     * EntityStatusEffectS2CPacket.java:49, StatusEffectInstance.java:73); from 1.20.5 it is a varint the instance clamps
     * to 0 - 255 (1.20.5 EntityStatusEffectS2CPacket.java:57, StatusEffectInstance.java:81).
     */
    protected effectAmplifierByName(name: string, effects: object): number | undefined {
        const descriptor = this.registry.effectsByName[name];
        if (!descriptor) return undefined;
        const info = (effects as unknown as Record<number, { amplifier: number } | undefined>)[descriptor.id];
        if (!info) return undefined;
        return this.lt('1.20.5') ? (info.amplifier << 24) >> 24 : Math.min(Math.max(info.amplifier, 0), 255);
    }

    getEnchantmentLevel(enchantment: number, enchantments: { id: string | number; lvl: number }[]): number {
        return this.enchantmentLevelByName(this.enchantmentNames[enchantment as keyof typeof ENCHANTMENT_NAMES], enchantments);
    }

    /** The level of an enchantment by its minecraft-data name ("frost_walker"), 0 without it or where the release has none. */
    protected enchantmentLevelByName(name: string, enchantments: { id: string | number; lvl: number }[]): number {
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

    /**
     * AttributeInstance.getValue clamped to the attribute's range (RangedAttribute.sanitizeValue): the base plus every
     * ADD_VALUE, plus the sum times every ADD_MULTIPLIED_BASE, times one plus every ADD_MULTIPLIED_TOTAL, each operation in
     * the order its container iterates (1.8.9 EntityAttributeInstanceImpl.java:147-165, 1.16 EntityAttributeInstance.java:132-147,
     * 1.21.11 AttributeInstance.java:150-172).
     */
    protected attributeValue(name: string, attribute: Attribute): number {
        if (!this.attributeRanges) {
            this.attributeRanges = new Map();
            for (const [key, data] of Object.entries(this.registry.attributesByName ?? {})) {
                if (data) this.attributeRanges.set(key, { min: data.min, max: data.max });
            }
        }
        const modifiers = attribute.modifiers ?? [];
        let base = attribute.value;
        for (const modifier of this.iterationOrder(modifiers.filter(entry => entry.operation === 0))) base += modifier.amount;
        let value = base;
        for (const modifier of this.iterationOrder(modifiers.filter(entry => entry.operation === 1))) value += base * modifier.amount;
        for (const modifier of this.iterationOrder(modifiers.filter(entry => entry.operation === 2))) value *= 1.0 + modifier.amount;
        const range = this.attributeRanges.get(name);
        if (!range) return value;
        // NaN is the minimum from 1.19, before it Mth.clamp lets it through (1.18 ClampedEntityAttribute.java:36,
        // 1.19 ClampedEntityAttribute.java:36)
        if (Number.isNaN(value)) return this.lt('1.19') ? value : range.min;
        return Math.min(Math.max(value, range.min), range.max);
    }

    /**
     * The modifiers of one operation in the order vanilla's container iterates them, from the order they were added (the
     * attributes packet's, then setSprinting's). Before 1.16 computeValue copies the operation's HashSet into a new one
     * (Sets.newHashSet(Collection), 1.15 EntityAttributeInstanceImpl.java:163-164); 1.16 - 1.20.4 iterate the HashSet
     * itself (1.16 EntityAttributeInstance.java:58-59, 152-153): buckets in order, a bucket's entries as they came, by
     * the UUID's hash (EntityAttributeModifier.hashCode). From 1.20.5 a fastutil Object2ObjectOpenHashMap by the UUID
     * (1.20.5 EntityAttributeInstance.java:21, 53-54), by the Identifier from 1.21 (1.21 :24, 56-57; 26.3
     * AttributeInstance.java:23, 54-55): linear probing from mix(hash), iterated from the last slot down.
     */
    private iterationOrder(modifiers: AttributeModifier[]): AttributeModifier[] {
        if (modifiers.length < 2) return modifiers;
        const hashes = modifiers.map(modifier => this.modifierIdHash(String(modifier.uuid ?? modifier.id ?? '')));
        if (this.lt('1.20.5')) {
            // HashMap: capacity 16, doubled past 0.75 of it; HashSet(Collection) sizes for (int) (size / .75f) + 1
            let capacity = 16;
            if (this.lt('1.16')) while (capacity < Math.trunc(Math.fround(modifiers.length / Math.fround(0.75))) + 1) capacity *= 2;
            else while (modifiers.length > capacity * 0.75) capacity *= 2;
            const bucket = (hash: number) => (hash ^ (hash >>> 16)) & (capacity - 1);
            return modifiers.map((modifier, i) => ({ modifier, bucket: bucket(hashes[i]), i }))
                .sort((a, b) => a.bucket - b.bucket || a.i - b.i)
                .map(entry => entry.modifier);
        }
        // Object2ObjectOpenHashMap: 32 slots for its default 16 entries, doubled past 0.75 of them (HashCommon.arraySize)
        let slots = 32;
        while (modifiers.length > Math.min(Math.ceil(slots * 0.75), slots - 1)) slots *= 2;
        const table: (AttributeModifier | undefined)[] = new Array(slots);
        modifiers.forEach((modifier, i) => {
            const h = Math.imul(hashes[i], 0x9e3779b9);
            let pos = (h ^ (h >>> 16)) & (slots - 1);
            while (table[pos] !== undefined) pos = (pos + 1) & (slots - 1);
            table[pos] = modifier;
        });
        const ordered: AttributeModifier[] = [];
        for (let pos = slots - 1; pos >= 0; pos -= 1) if (table[pos] !== undefined) ordered.push(table[pos]!);
        return ordered;
    }

    /** UUID.hashCode before 1.21 (the four 32-bit words xor'd), Identifier.hashCode from it (31 * namespace + path). */
    private modifierIdHash(id: string): number {
        if (this.lt('1.21')) {
            const hex = id.replace(/-/g, '');
            if (!/^[0-9a-fA-F]{32}$/.test(hex)) return 0;
            let hash = 0;
            for (let i = 0; i < 32; i += 8) hash ^= parseInt(hex.slice(i, i + 8), 16) | 0;
            return hash;
        }
        const colon = id.indexOf(':');
        const javaHash = (text: string) => {
            let hash = 0;
            for (let i = 0; i < text.length; i += 1) hash = (Math.imul(31, hash) + text.charCodeAt(i)) | 0;
            return hash;
        };
        return (Math.imul(31, javaHash(colon < 0 ? 'minecraft' : id.slice(0, colon))) + javaHash(colon < 0 ? id : id.slice(colon + 1))) | 0;
    }

    /** The synced attribute's value with its modifiers, or undefined without it. */
    protected attributeValueOf(attributes: Attributes, name: string): number | undefined {
        const attribute = this.attribute(attributes, name);
        return attribute ? this.attributeValue(name, attribute) : undefined;
    }
}
