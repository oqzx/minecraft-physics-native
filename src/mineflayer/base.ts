import { ShulkerBoxTracker } from './shulkers';
import type { BlockActionPacket, OpenShulkerBox } from './shulkers';
// What the adapter's engines share: the registry they were made for, the native binding, the NativeWorld kept in step
// with the caller's world, and the rest of mineflayer-physics-utils' IPhysics, which its states and EPhysicsCtx call
// (effect and enchantment levels, supportFeature).
import type { IPhysics } from '@nxg-org/mineflayer-physics-util/dist/physics/engines/IPhysics';

import { guardNativeBinding, LoadNativePhysics, type NativeAabb, type NativePhysicsBinding } from '../native';
import { assertReleaseAdmitted } from '../release-status';
import { compareReleases } from './blocks';
import { attributesWearNamespace, physicsFeature } from './physics-features';
import { resolveRegistryRelease } from './registry-release';
import type { Attribute, AttributeModifier, Attributes } from './types';
import { DIRECTIONS, MovingBlockTracker, type MovableWorld, type MovingRegistry } from './moving';
import type { MovingPiston } from './player';
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
    /** The exact release to run (the registry's by default); it must match registry.version.minecraftVersion. */
    version?: string;
    /** Refuse a release that has not been admitted for parity (releaseStatus), rather than run what is registered. */
    requireParity?: boolean;
}

/** mineflayer-physics-utils' CheapEffects and CheapEnchantments, by their numbers. */
const EFFECT_NAMES: IPhysics['statusEffectNames'] = Object.freeze({
    0: 'JumpBoost', 1: 'Speed', 2: 'Slowness', 3: 'DolphinsGrace', 4: 'SlowFalling', 5: 'Levitation', 6: 'Blindness',
});
const ENCHANTMENT_NAMES: IPhysics['enchantmentNames'] = Object.freeze({ 0: 'depth_strider', 1: 'swift_sneak', 2: 'soul_speed', 3: 'riptide' });

/** A prismarine registry (minecraft-data's IndexedData of a release). */
export type PhysicsRegistry = IPhysics['data'];

type PistonBlock = { name: string; position?: { x: number; y: number; z: number } } | null | undefined;

type BotListener = (...args: never[]) => void;

/** The events of a mineflayer bot that say what its pistons do, and that it left. */
export interface PistonBot {
    on?(event: 'pistonMove', listener: (block: PistonBlock, event: number, param: number) => void): unknown;
    on?(event: 'blockUpdate', listener: (old: PistonBlock, block: PistonBlock) => void): unknown;
    on?(event: 'physicsTick', listener: () => void): unknown;
    on?(event: 'end', listener: () => void): unknown;
    removeListener?(event: string, listener: BotListener): unknown;
}

/**
 * What an engine keeps for a world: its sync, and the moving blocks of the bot that tells the world's piston events,
 * with the listeners that feed them. The engine owns all of it, and `release` undoes it.
 */
class WorldLink {
    tracker?: MovingBlockTracker;
    shulkers?: ShulkerBoxTracker;
    private client?: PistonBot;
    private clientListener?: BotListener;
    private lidBot?: PistonBot;
    private lidListeners: [string, BotListener][] = [];

    private lidWorld?: PistonBot;
    private lidWorldListener?: BotListener;

    /** A column's unloading, which the world tells. */
    listenWorld(world: PistonBot, listener: BotListener): void {
        this.lidWorld = world;
        this.lidWorldListener = listener;
        (world.on as ((event: string, listener: BotListener) => unknown) | undefined)?.call(world, 'chunkColumnUnload', listener);
    }

    listenBot(bot: PistonBot | undefined, listeners: [string, BotListener][]): void {
        this.lidBot = bot;
        this.lidListeners = listeners;
        for (const [event, listener] of listeners) (bot?.on as (event: string, listener: BotListener) => unknown)?.call(bot, event, listener);
    }

    listenClient(client: PistonBot, listener: BotListener): void {
        this.client = client;
        this.clientListener = listener;
        (client.on as (event: string, listener: BotListener) => unknown).call(client, 'block_action', listener);
    }

    private dropClient(): void {
        if (this.client && this.clientListener) {
            (this.client as unknown as { removeListener?(event: string, listener: BotListener): void }).removeListener?.('block_action', this.clientListener);
        }
        for (const [event, listener] of this.lidListeners) this.lidBot?.removeListener?.(event, listener);
        this.lidListeners = [];
        this.lidBot = undefined;
        if (this.lidWorldListener) this.lidWorld?.removeListener?.('chunkColumnUnload', this.lidWorldListener);
        this.lidWorld = undefined;
        this.client = undefined;
        this.shulkers = undefined;
    }

    private bot?: PistonBot;
    private listeners: [string, BotListener][] = [];

    constructor(readonly world: PrismarineWorld, readonly sync: NativeWorldSync) {}

    /** Listens on the bot for as long as this link lasts. */
    listen(bot: PistonBot, tracker: MovingBlockTracker, listeners: [string, BotListener][]): void {
        this.tracker = tracker;
        this.bot = bot;
        this.listeners = listeners;
        for (const [event, listener] of listeners) (bot.on as (event: string, listener: BotListener) => unknown).call(bot, event, listener);
    }

    release(): void {
        this.sync.dispose();
        const bot = this.bot;
        if (bot?.removeListener) for (const [event, listener] of this.listeners) bot.removeListener(event, listener);
        this.dropClient();
        this.listeners = [];
        this.bot = undefined;
    }
}

export class NativeEngine implements Omit<IPhysics, 'simulate'> {
    readonly data: PhysicsRegistry;
    readonly binding: NativePhysicsBinding;
    readonly version: string;
    readonly statusEffectNames = EFFECT_NAMES;
    readonly enchantmentNames = ENCHANTMENT_NAMES;
    private readonly registry: Registry;
    /** The links by world, the link each bot's piston events feed, and every link, none of which keeps a world alive. */
    private readonly links = new WeakMap<object, WorldLink>();
    private readonly linkOfBot = new WeakMap<object, WorldLink>();
    private readonly allLinks = new Set<WeakRef<WorldLink>>();
    private attributeRanges?: Map<string, { min: number; max: number }>;
    private readonly attributeKeys = new Map<string, string | null>();
    private readonly releaseOrder = new Map<string, number>();

    constructor(registry: PhysicsRegistry, options: NativeEngineOptions = {}) {
        this.version = resolveRegistryRelease(registry, options.version);
        if (options.requireParity) assertReleaseAdmitted(this.version);
        // an engine runs the one release it was made for, for as long as it lives
        Object.defineProperty(this, 'version', { writable: false, configurable: false });
        this.data = registry;
        this.registry = registry as unknown as Registry;
        this.binding = options.binding ? guardNativeBinding(options.binding) : LoadNativePhysics();
        // A binding may support fewer releases than this package's TypeScript manifest.
        if (this.binding.isSupportedVersion && !this.binding.isSupportedVersion(this.version)) {
            throw new Error(`unsupported decompiled Minecraft version: ${this.version}`);
        }
    }

    /**
     * mineflayer-physics-utils' catalog, against the release this engine runs (physics-features.ts); every other name is
     * minecraft-data's, as a yes or no: a feature with a value (metadataIxOfItem) is asked of the registry.
     */
    readonly supportFeature = (feature: string): boolean => physicsFeature(this.version, feature) ?? !!this.registry.supportFeature?.(feature);

    /** Whether the selected release, the one the engine runs, is older than `version`. */
    protected lt(version: string): boolean {
        return this.againstRelease(version) < 0;
    }

    protected gt(version: string): boolean {
        return this.againstRelease(version) > 0;
    }

    /**
     * The selected release against `version`, kept: a tick asks about a hundred times, and each fresh comparison splits two
     * strings (docs/benchmarks.md). An engine's release never changes, and the versions asked are the code's few literals.
     */
    private againstRelease(version: string): number {
        let compared = this.releaseOrder.get(version);
        if (compared === undefined) {
            compared = compareReleases(this.version, version);
            this.releaseOrder.set(version, compared);
        }
        return compared;
    }

    /**
     * Entity.isNoGravity (1.10+): a state's own noGravity when it has one, else the synced data at 5 of the entity it
     * was made from (1.10.2 Entity.java:142, 900; 1.21.11 Entity.java:1443-1445 getGravity 0); 1.8 and 1.9 have no
     * such flag. States without either field (older mineflayer-physics-utils ones) fall as before.
     */
    protected noGravityOf(state: { noGravity?: boolean; entity?: { metadata?: unknown[] } }, entity?: { metadata?: unknown[] }): boolean {
        if (this.lt('1.10')) return false;
        if (typeof state.noGravity === 'boolean') return state.noGravity;
        return (state.entity ?? entity)?.metadata?.[5] === true;
    }

    /** Entity.getSharedFlag(0): bit 0 of the entity's synced data at 0. */
    protected onFireFlagOf(entity?: { metadata?: unknown[] }): boolean {
        const flags = entity?.metadata?.[0];
        return typeof flags === 'number' && (flags & 1) !== 0;
    }

    /** The NativeWorld kept in step with a prismarine world. */
    syncOf(world: PrismarineWorld): NativeWorldSync {
        return this.linkOf(world).sync;
    }

    /**
     * The blocks pistons are moving in a world, as a bot's block events make them: undefined until a tick of a state with a bot
     * has started it (syncMovingBlocks), and for a world that has no such bot.
     */
    movingBlockTrackerOf(world: PrismarineWorld): MovingBlockTracker | undefined {
        return this.links.get(world)?.tracker ?? this.movingBlocksFrom?.movingBlockTrackerOf(world);
    }

    /**
     * The engine whose bot's piston events this one's worlds take their moving blocks from: a vehicle engine has no bot of its
     * own, and the vehicle collides with a block a piston moves as the player does (Entity.move, the moving_piston cell).
     */
    movingBlocksFrom?: NativeEngine;

    /**
     * The shulker boxes open in a world, as its bot's block events opened them and its physicsTicks moved their lids on (each
     * one's `pushes` says whether pushByShulkerBox moves an entity in its way this tick): undefined as movingBlockTrackerOf.
     */
    shulkerBoxesOf(world: PrismarineWorld): OpenShulkerBox[] | undefined {
        return this.links.get(world)?.shulkers?.boxes();
    }

    /**
     * The moving blocks of a world as pushByMovingPistons takes them, in the order they tick, whole: their boxes as the cells
     * have them, with the outline boxes that push on 1.9 - 1.10 and the boxes of their next tick. Undefined as movingBlockTrackerOf.
     */
    movingPistonsOf(world: PrismarineWorld): MovingPiston[] | undefined {
        const link = this.links.get(world);
        if (!link?.tracker) return undefined;
        const blocks = link.tracker.blocks();
        const lists = (boxes: readonly NativeAabb[]) => boxes.map(({ min, max }) => [min.x, min.y, min.z, max.x, max.y, max.z]);
        return link.sync.movingPistonsOf(blocks).map((piston, i) => ({
            pos: [piston.x, piston.y, piston.z],
            direction: DIRECTIONS[piston.facing]!,
            extending: piston.extending,
            source: piston.source,
            progress: piston.progress,
            shapes: lists(piston.shapes),
            ...(piston.outlineShapes ? { outlineShapes: lists(piston.outlineShapes) } : {}),
            ...(piston.nextShapes ? { nextShapes: lists(piston.nextShapes) } : {}),
            movedBlock: blocks[i]!.block.name,
        }));
    }

    private linkOf(world: PrismarineWorld): WorldLink {
        let link = this.links.get(world);
        if (!link) {
            link = new WorldLink(world, new NativeWorldSync(this.binding, this.registry, this.version));
            this.links.set(world, link);
            for (const known of this.allLinks) if (!known.deref()) this.allLinks.delete(known);
            this.allLinks.add(new WeakRef(link));
        }
        return link;
    }

    /**
     * Lets go of a world the bot left: its sync is disposed and the listeners on it and on its bot leave. The world's
     * next tick, if it gets one, starts over from its blocks. A world the engine does not know is left alone.
     */
    releaseWorld(world: PrismarineWorld): void {
        const link = this.links.get(world);
        if (!link) return;
        this.links.delete(world);
        link.release();
    }

    /**
     * Lets go of every world (plugin shutdown): as releaseWorld for each, which a bot's 'end' does for its own. The engine
     * stays usable, and a second call does nothing.
     */
    dispose(): void {
        for (const known of this.allLinks) {
            const link = known.deref();
            if (link) this.releaseWorld(link.world);
        }
        this.allLinks.clear();
    }

    /**
     * Gives a synced world the moving blocks of a bot that tells its piston events (mineflayer's pistonMove): the server
     * sends no moving_piston block, so the tracker keeps them, and the bot's ticks end them. A state made without a bot
     * has none to give. A bot in another world than its last has left that one, whose link is released.
     */
    syncMovingBlocks(sync: NativeWorldSync, world: PrismarineWorld, bot: PistonBot | undefined): void {
        const link = this.linkOf(world);
        if (!link.tracker && bot?.on) {
            const last = this.linkOfBot.get(bot);
            if (last && last.world !== world) this.releaseWorld(last.world);
            this.linkOfBot.set(bot, link);
            // (the bot's own world: what mineflayer hands the engine may read blocks only, and a tracker that sets none has no
            // moving block in its cell for setMovingBlocks to collide with)
            const made = new MovingBlockTracker(this.registry as unknown as MovingRegistry, this.version, ((bot as { world?: MovableWorld }).world ?? world) as MovableWorld);
            link.listen(bot, made, [
                ['pistonMove', ((block: PistonBlock, event: number, param: number) => {
                    if (block?.position) made.trigger(block.name, [block.position.x, block.position.y, block.position.z], event, param);
                }) as BotListener],
                ['blockUpdate', ((old: PistonBlock, block: PistonBlock) => {
                    const at = block?.position ?? old?.position;
                    if (at) made.blockChanged([at.x, at.y, at.z], block?.name);
                }) as BotListener],
                ['physicsTick', () => made.tick()],
                ['end', () => this.releaseWorld(world)],
            ]);
        }
        if (link.tracker) sync.setMovingBlocks(link.tracker.blocks());
        const client = (bot as (PistonBot & { _client?: PistonBot }) | undefined)?._client;
        if (!link.shulkers && client?.on) {
            const lids = new ShulkerBoxTracker(this.binding, this.version);
            link.shulkers = lids;
            link.listenClient(client, ((packet: BlockActionPacket) => {
                const { x, y, z } = packet.location;
                const block = (world as unknown as { getBlock(at: unknown): { name?: string; getProperties?(): { facing?: string } } | null }).getBlock({ x, y, z, floored: () => ({ x, y, z }) });
                if (packet.byte1 === 1 && block?.name?.endsWith('shulker_box')) {
                    lids.event(x, y, z, ['down', 'up', 'north', 'south', 'west', 'east'].indexOf(block.getProperties?.().facing ?? 'up'), packet.byte2);
                }
            }) as BotListener);
            link.listenBot(bot, [
                // (the block entities' tick, which the plugin emits after the entities when it does: from 26.3 the position is sent
                // after it; else the end of the physics tick)
                ['blockEntityTick', (() => lids.tick()) as BotListener],
                ['physicsTick', (() => { if (!(bot as { _emitsBlockEntityTick?: boolean })._emitsBlockEntityTick) lids.tick(); }) as BotListener],
                ['blockUpdate', ((old: PistonBlock, block: PistonBlock) => {
                    const at = block?.position ?? old?.position;
                    if (at && !block?.name?.endsWith('shulker_box')) lids.forget(at.x, at.y, at.z);
                }) as BotListener],
            ]);
            link.listenWorld(world as unknown as PistonBot, ((corner: { x: number; z: number }) => {
                lids.forgetColumn(Math.floor(corner.x / 16), Math.floor(corner.z / 16));
            }) as BotListener);
        }
        if (link.shulkers) sync.setShulkerBoxes(link.shulkers.boxes());
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
     * ClientboundUpdateMobEffectPacket.java:50, MobEffectInstance.java:76); from 1.20.5 it is a varint the instance clamps
     * to 0 - 255 (1.20.5 ClientboundUpdateMobEffectPacket.java:55, MobEffectInstance.java:75).
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
            const namespaced = attributesWearNamespace(this.version) && !data?.resource.startsWith('minecraft:');
            attributeKey = !data ? null : namespaced ? `minecraft:${data.resource}` : data.resource;
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
     * the order its container iterates (1.8.8 ModifiableAttributeInstance.java:147-165, 1.16 AttributeInstance.java:130-145,
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
        // NaN is the minimum from 1.19, before it Mth.clamp lets it through (1.18 RangedAttribute.java:36,
        // 1.19 RangedAttribute.java:32)
        if (Number.isNaN(value)) return this.lt('1.19') ? value : range.min;
        return Math.min(Math.max(value, range.min), range.max);
    }

    /**
     * The modifiers of one operation in the order vanilla's container iterates them, from the order they were added (the
     * attributes packet's, then setSprinting's). Before 1.16 computeValue copies the operation's HashSet into a new one
     * (Sets.newHashSet(Collection), 1.15 ModifiableAttributeInstance.java:161-162); 1.16 - 1.20.4 iterate the HashSet
     * itself (1.16 AttributeInstance.java:56-57, 150-151): buckets in order, a bucket's entries as they came, by
     * the UUID's hash (AttributeModifier.hashCode). From 1.20.5 a fastutil Object2ObjectOpenHashMap by the UUID
     * (1.20.5 AttributeInstance.java:22, 53-55), by the Identifier from 1.21 (1.21 :25, 56-58; 26.3
     * AttributeInstance.java:23, 54-55): linear probing from mix(hash), iterated from the last slot down.
     */
    protected iterationOrder(modifiers: AttributeModifier[]): AttributeModifier[] {
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

    /** UUID.hashCode before 1.21 (the four 32-bit words xor'd), ResourceLocation.hashCode from it (31 * namespace + path). */
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
