// A NativeWorld kept in step with a prismarine world (mineflayer's bot.world, or any world with getBlock): before each
// tick the blocks around the simulated entity are read again, and every block whose own state or a neighbour's changed
// is written anew (its shape before 1.13, its fluid flow and the trapdoor over a ladder depend on the blocks around).
import { guardNativeBinding, type NativeAabb, type NativeBlockUpdate, type NativeMovingPiston, type NativePhysicsBinding, type NativeWorld } from '../native';
import { BlockMapper, compareReleases, describeBlock, releaseCheck, type DescribedBlock, type PrismarineBlock } from './blocks';
import type { Box } from './legacy';
import { rainsInBiome, type BiomeData } from './rain';
import { resolveRegistryRelease } from './registry-release';
import type { WorldBorder } from './types';

export interface Vec3Like {
    x: number;
    y: number;
    z: number;
}

/** A prismarine world: mineflayer's bot.world, or anything that gives the prismarine block at a position. */
export interface PrismarineWorld {
    getBlock(pos: Vec3Like): PrismarineBlock | null | undefined;
    /** The sky light at a position (prismarine-world's). */
    getSkyLight?(pos: Vec3Like): number | null | undefined;
    /** The state id at a position, read without building a block (prismarine-world's). */
    getBlockStateId?(pos: Vec3Like): number | null | undefined;
    /** The chunk column at a position (prismarine-world's): where it has none, its state id is 0 and no block. */
    getColumnAt?(pos: Vec3Like): unknown;
    /**
     * prismarine-world's events: a world that emits 'blockUpdate' (and the chunk columns it loads and unloads) has only
     * the blocks it changed and those newly near the entity read again; any other world has all of them read each tick.
     */
    on?(event: string, listener: (...args: any[]) => void): unknown;
    /** What takes such a listener off again (an EventEmitter's); a world without either keeps it, which then does nothing. */
    off?(event: string, listener: (...args: any[]) => void): unknown;
    removeListener?(event: string, listener: (...args: any[]) => void): unknown;
}

/**
 * A prismarine-chunk column: its sections from the bottom of the level up, an absent one null (from 1.9, as the chunk
 * packet's bitmask leaves it out; the 1.8 and 1.18+ columns make every section), its bottom at minY (from 1.17).
 */
interface SectionedColumn {
    sections?: unknown[];
    minY?: number;
}

export interface SyncRegistry {
    version: ConstructorParameters<typeof BlockMapper>[0];
    blocksArray: { id: number; name: string }[];
    blocksByName: Record<string, { id: number } | undefined>;
}

export interface BoxLike {
    minX: number;
    minY: number;
    minZ: number;
    maxX: number;
    maxY: number;
    maxZ: number;
}

const UNKNOWN = -1;
/** The blocks the sync remembers before it forgets them all and starts again. */
const CACHE_LIMIT = 1 << 18;
/**
 * The most cells one sync reads. A cell is a string and a map entry each, so a box past this runs out of memory long
 * before the engine's own bound (MAX_WALKED_CELLS): a motion of 150 blocks a tick on every axis is well inside it.
 */
const MAX_SYNCED_CELLS = 1 << 22;
/** The cells within `margin` of a box. */
function regionOf(box: BoxLike, margin: number): Region {
    return [
        Math.floor(box.minX - margin), Math.floor(box.minY - margin), Math.floor(box.minZ - margin),
        Math.floor(box.maxX + margin), Math.floor(box.maxY + margin), Math.floor(box.maxZ + margin),
    ];
}
/** (a NaN reads nothing, as the sync's loops take it) */
function cellsOf(region: Region): number {
    return (region[3] - region[0] + 1) * (region[4] - region[1] + 1) * (region[5] - region[2] + 1);
}
/** Whether a sync reads this box, or refuses it. */
export function syncReads(box: BoxLike, margin: number): boolean {
    return !(cellsOf(regionOf(box, margin)) > MAX_SYNCED_CELLS);
}

/** A sync of what a tick of a body reads: its box, and as far around it as its motion goes. */
export function bodyReach(pos: { x: number; y: number; z: number }, vel: { x: number; y: number; z: number }, halfWidth: number, height: number): { box: BoxLike; margin: number } {
    const reach = Math.max(Math.abs(vel.x), Math.abs(vel.y), Math.abs(vel.z));
    return {
        box: { minX: pos.x - halfWidth, minY: pos.y, minZ: pos.z - halfWidth, maxX: pos.x + halfWidth, maxY: pos.y + height, maxZ: pos.z + halfWidth },
        margin: 3 + Math.ceil(reach),
    };
}

/** WorldBorder.absoluteMaxSize until a server says another. */
const ABSOLUTE_MAX_SIZE = 29999984;
/** The centre, size and absolute max size of the WorldBorder the engine starts with. */
const DEFAULT_BORDER: [number, number, number, number] = [0, 0, 59999968, ABSOLUTE_MAX_SIZE];
const key = (x: number, y: number, z: number) => `${x},${y},${z}`;

/** Cells from (x0, y0, z0) to (x1, y1, z1), inclusive. */
type Region = [number, number, number, number, number, number];
const inside = (r: Region, x: number, y: number, z: number) => x >= r[0] && x <= r[3] && y >= r[1] && y <= r[4] && z >= r[2] && z <= r[5];
const grown = (r: Region, by: number): Region => [r[0] - by, r[1] - by, r[2] - by, r[3] + by, r[4] + by, r[5] + by];
/**
 * The vanilla level height of a dimension where the server told none (DimensionType's minY and height): 0 and 256 to
 * 1.17, then -64 and 384 in the overworld only (1.17 DimensionType.java:78-97, 1.18 :78-96, 98-116, 118-136). What the
 * server sends always wins: a data pack's dimension has its own.
 */
const defaultLevelHeight = (release: string, dimension: string | undefined): [number, number] => {
    const [major, minor] = release.split('.').map(Number);
    const caves = major > 1 || minor >= 18;
    const overworld = dimension === undefined || dimension === 'overworld';
    return caves && overworld ? [-64, 384] : [0, 256];
};
const sameRegion = (a: Region, b: Region) => a[0] === b[0] && a[1] === b[1] && a[2] === b[2] && a[3] === b[3] && a[4] === b[4] && a[5] === b[5];
export class Cursor implements Vec3Like {
    constructor(public x = 0, public y = 0, public z = 0) {}

    set(x: number, y: number, z: number) {
        this.x = x;
        this.y = y;
        this.z = z;
        return this;
    }

    floored() {
        return new Cursor(Math.floor(this.x), Math.floor(this.y), Math.floor(this.z));
    }

    clone() {
        return new Cursor(this.x, this.y, this.z);
    }

    offset(dx: number, dy: number, dz: number) {
        return new Cursor(this.x + dx, this.y + dy, this.z + dz);
    }
}

/**
 * A PistonMovingBlockEntity as a move meets it: its cell, its Direction.from3DDataValue, the block whose shape it moves
 * (at this progress: the head a source piston shows, else the block it moves) and the extended base a source piston
 * leaves as it retracts.
 */
export interface MovingBlock {
    x: number;
    y: number;
    z: number;
    facing: number;
    extending: boolean;
    source: boolean;
    progress: number;
    block: PrismarineBlock;
    /** The block it shows once it has gone on this tick, if that is another one. */
    next?: PrismarineBlock;
    base?: PrismarineBlock;
}

/**
 * The box that pushes on 1.9 - 1.10 (the block's bounding box, 1.9.4 PistonMovingBlockEntity.java): a moved block's own boxes. A head's is
 * its plate alone, which its collision boxes do not say: the engine draws that itself.
 */
const outlineOf = (block: PrismarineBlock, boxes: NativeAabb[]) => (block.name === 'piston_head' ? {} : { outlineShapes: boxes });

export class NativeWorldSync {
    world: NativeWorld;
    private readonly binding: NativePhysicsBinding;
    private readonly mapper: BlockMapper;
    private readonly release: string;
    /** The cells given a moving block's shape, by key: the cell and the boxes it collides with, in the cell (free to leave it). */
    private readonly moving = new Map<string, [number, number, number, Box[]]>();
    /** The open shulker boxes of setShulkerBoxes: the cell and its lid's box in the cell. */
    private readonly shulkers = new Map<string, [number, number, number, Box]>();
    /** The state id read at each cell (UNKNOWN where the world had no block), and the block of each state id. */
    private readonly ids = new Map<string, number>();
    private readonly described = new Map<number, DescribedBlock>();
    /**
     * The cells whose blocks the NativeWorld holds, those written from the blocks around them as they are read
     * (current), and those written before a block around them changed (outdated).
     */
    private readonly present = new Set<string>();
    private readonly current = new Set<string>();
    private readonly outdated = new Set<string>();
    private readonly loadedChunks = new Set<string>();
    /** The chunk column of the cell read last, and whether the world has it. */
    private lastChunk = '';
    private lastChunkLoaded = false;
    private readonly cursor = new Cursor();
    private source: PrismarineWorld | null = null;
    /** Whether the source tells its block changes, the cells it changed since the last sync, and the cells written then. */
    private watched = false;
    /** The listeners on the source: they leave it with reset and dispose. */
    private listeners: [string, (...args: any[]) => void][] = [];
    private disposed = false;
    /** The cells whose block the NativeWorld refused, and why: they hold nothing, so a caller that reads no further sees a gap. */
    readonly refused = new Map<string, string>();
    private readonly changed = new Set<string>();
    private region: Region | null = null;
    /** The bounds last given to the NativeWorld: the border's centre, size and absolute max size, the level's minY and height. */
    private bounds: [number, number, number, number, number, number] | null = null;

    constructor(binding: NativePhysicsBinding, registry: SyncRegistry, release?: string) {
        const selected = resolveRegistryRelease(registry, release);
        guardNativeBinding(binding);
        this.binding = binding;
        this.release = selected;
        this.world = new binding.NativeWorld();
        this.world.setRelease?.(selected);
        // the release's #fall_damage_resetting tag, from the engine's generated block data
        const hasTag = (name: string, tag: string) => binding.generatedBlockHasTag(selected, name, tag);
        this.mapper = new BlockMapper(releaseCheck(selected), registry, hasTag, name => binding.generatedMethodShapeBoxes(selected, name, 'outline'),
            (name, index, kind) => binding.generatedStateShape(selected, name, kind, index));
    }

    /** A block the world changed: it is read again at the next sync (the world's own events do this when it has them). */
    blockChanged(pos: Vec3Like): void {
        if (this.disposed) return;
        this.changed.add(key(Math.floor(pos.x), Math.floor(pos.y), Math.floor(pos.z)));
    }

    /** A chunk column the world loaded or unloaded: its blocks are read again where the entity reaches them. */
    columnChanged(chunkX: number, chunkZ: number, loaded: boolean): void {
        if (this.disposed) return;
        for (const cell of [...this.ids.keys()]) {
            const [x, y, z] = cell.split(',').map(Number);
            if (x >> 4 !== chunkX || z >> 4 !== chunkZ) continue;
            this.ids.delete(cell);
            this.outdateAround(x, y, z);
        }
        // (a column loaded anew has its sections read again with its first block)
        this.loadedChunks.delete(`${chunkX},${chunkZ}`);
        this.lastChunk = '';
        if (!loaded) this.world.setChunkLoaded(chunkX, chunkZ, false);
        this.region = null;
    }

    /**
     * The world border the client knows (WorldBorder, which Entity.collide collides with), and the height of its level
     * (minY and height, as mineflayer's bot.game has them; the engine does not use it in a move). Either may change
     * alone: a border the client has not heard of, or one not usable, leaves the last, and a dimension without a height
     * of its own takes the release's default.
     */
    setWorldBorder(border: WorldBorder | undefined, game?: { minY?: number; height?: number; dimension?: string }): void {
        const [centerX, centerZ, size, max] = this.borderOf(border) ?? this.bounds?.slice(0, 4) ?? DEFAULT_BORDER;
        const [defaultMinY, defaultHeight] = defaultLevelHeight(this.release, game?.dimension);
        const told = Number.isFinite(game?.height) && (game?.height as number) > 0;
        const minY = told && Number.isFinite(game?.minY) ? (game?.minY as number) : defaultMinY;
        const height = told ? (game?.height as number) : defaultHeight;
        const next: [number, number, number, number, number, number] = [centerX, centerZ, size, max, minY, height];
        if (this.bounds?.every((value, i) => value === next[i])) return;
        this.world.setWorldBounds(minY, minY + height, centerX, centerZ, size, max);
        this.bounds = next;
    }

    /** The centre, size and absolute max size of a border, null when it has none usable. */
    private borderOf(border: WorldBorder | undefined): [number, number, number, number] | null {
        if (!border) return null;
        const { centerX, centerZ } = border;
        // (the edges collide where getMinX(0.0F) puts them: a moving border's size before its last tick from 1.21.11)
        const size = Number.isFinite(border.previousSize) ? (border.previousSize as number) : border.size;
        const max = Number.isInteger(border.absoluteMaxSize) ? (border.absoluteMaxSize as number) : ABSOLUTE_MAX_SIZE;
        if (!Number.isFinite(centerX) || !Number.isFinite(centerZ) || !Number.isFinite(size) || size < 0) return null;
        return [centerX, centerZ, size, max];
    }

    /**
     * Brings the blocks within `margin` of `box` up to date with `source` (a new source forgets the blocks of the old
     * one). The blocks within one more block (two in 1.12) are read too, as the ones nearest the box are made from them.
     */
    sync(source: PrismarineWorld, box: BoxLike, margin: number): void {
        if (this.disposed) throw new Error('NativeWorldSync.sync: the sync was disposed; make a new one (NativeEngine.syncOf does)');
        if (source !== this.source) this.reset(source);
        else if (this.ids.size > CACHE_LIMIT) this.forget();
        this.lastChunk = '';
        const region = regionOf(box, margin);
        const cells = cellsOf(region);
        if (cells > MAX_SYNCED_CELLS) {
            throw new Error(`NativeWorldSync.sync: the blocks around the entity are ${cells} cells, past the ${MAX_SYNCED_CELLS} a sync reads`);
        }
        const last = this.region;
        // a world that tells its changes: only the blocks the last sync did not read, and those that changed
        const watched = this.watched && last !== null;
        if (watched && this.changed.size === 0 && sameRegion(last, region)) return;
        const around = this.mapper.reach;
        const reach = grown(region, around);
        const lastReach = last && grown(last, around);
        // (a block read for the first time is no neighbour of a block written before: those were written with all
        // theirs read; a block that changed makes the blocks around it out of date)
        const readAt = (x: number, y: number, z: number) => {
            const cell = key(x, y, z);
            const id = this.read(source, x, y, z);
            const before = this.ids.get(cell);
            if (before === id) return;
            this.ids.set(cell, id);
            if (before !== undefined) this.outdateAround(x, y, z);
        };
        const [x0, y0, z0, x1, y1, z1] = region;
        for (let x = x0 - around; x <= x1 + around; x += 1) {
            for (let z = z0 - around; z <= z1 + around; z += 1) {
                for (let y = y0 - around; y <= y1 + around; y += 1) {
                    // (the blocks of a world that tells its changes stay as read until it does)
                    if (!watched) readAt(x, y, z);
                    else if (!inside(lastReach as Region, x, y, z) && !this.ids.has(key(x, y, z))) readAt(x, y, z);
                }
            }
        }
        for (const cell of this.changed) {
            const [x, y, z] = cell.split(',').map(Number);
            this.readSection(source, x, y, z);
            if (inside(reach, x, y, z)) {
                readAt(x, y, z);
            } else if (this.ids.delete(cell)) {
                this.outdateAround(x, y, z);
            }
        }
        this.changed.clear();
        this.region = region;
        const rewritten: [number, number, number][] = [];
        for (let x = x0; x <= x1; x += 1) {
            for (let z = z0; z <= z1; z += 1) {
                for (let y = y0; y <= y1; y += 1) {
                    if (last && inside(last, x, y, z)) continue;
                    const cell = key(x, y, z);
                    // (air the NativeWorld does not hold is written as it is)
                    if (this.current.has(cell) || (!this.present.has(cell) && this.isAir(cell))) continue;
                    rewritten.push([x, y, z]);
                }
            }
        }
        for (const cell of this.outdated) {
            const [x, y, z] = cell.split(',').map(Number);
            if (inside(region, x, y, z) && last && inside(last, x, y, z)) rewritten.push([x, y, z]);
        }
        this.write(rewritten);
    }

    /** Writes the blocks of the cells anew; a block NativeWorld refuses stays out. */
    private write(cells: [number, number, number][]): void {
        const updates: NativeBlockUpdate[] = [];
        const removal = (x: number, y: number, z: number) => (this.present.has(key(x, y, z)) ? [{ x, y, z, remove: true }] : []);
        for (const [x, y, z] of cells) {
            const cell = key(x, y, z);
            const blocks = this.mapper.updatesAt(this.blockAt, x, y, z);
            updates.push(...removal(x, y, z), ...blocks);
            if (blocks.length) this.present.add(cell);
            // (NativeWorld leaves a cell of a chunk it has as unloaded alone, the removal too: it is removed once loaded)
            else if (this.loadedChunks.has(`${x >> 4},${z >> 4}`)) this.present.delete(cell);
            this.current.add(cell);
            this.outdated.delete(cell);
        }
        if (updates.length === 0) return;
        try {
            this.world.applyBlockUpdates(updates);
        } catch {
            for (const [x, y, z] of cells) {
                try {
                    this.world.applyBlockUpdates([{ x, y, z, remove: true }, ...this.mapper.updatesAt(this.blockAt, x, y, z)]);
                } catch (error) {
                    // (the world goes on without the block, as the game would with a chunk it could not read; `refused` says so)
                    this.refused.set(key(x, y, z), error instanceof Error ? error.message : String(error));
                    this.world.applyBlockUpdates([{ x, y, z, remove: true }]);
                }
            }
        }
    }

    private isAir(cell: string): boolean {
        const id = this.ids.get(cell);
        const name = id === undefined || id === UNKNOWN ? 'air' : this.described.get(id)?.name;
        return name === 'air' || name === 'cave_air' || name === 'void_air';
    }

    /** A changed block (air the NativeWorld did not hold too) and the blocks written around it are out of date. */
    private outdateAround(x: number, y: number, z: number): void {
        const around = this.mapper.reach;
        for (let dx = -around; dx <= around; dx += 1) {
            for (let dy = -around; dy <= around; dy += 1) {
                for (let dz = -around; dz <= around; dz += 1) {
                    const cell = key(x + dx, y + dy, z + dz);
                    if (this.current.delete(cell) || (dx === 0 && dy === 0 && dz === 0)) this.outdated.add(cell);
                }
            }
        }
    }

    /**
     * The moving blocks around the player when its move starts, after sync: a moving_piston cell collides as its block
     * entity says (MovingPistonBlock.getCollisionShape), which moves on every tick, so the whole list is given each one.
     * The shapes come from the blocks around the cell, as a block there would have them.
     */
    setMovingBlocks(blocks: readonly MovingBlock[]): void {
        const boxes = (block: PrismarineBlock, x: number, y: number, z: number) => this.movedBoxes(block, x, y, z);
        const before = new Map(this.moving);
        this.moving.clear();
        for (const { x, y, z, facing, extending, source, progress, block, base } of blocks) {
            const name = this.blockAt(x, y, z)?.name;
            // (the block entity's cell has another block once the packets replaced it: then it has that block's shape)
            if (name !== 'moving_piston' && name !== 'piston_extension') continue;
            const shapes = this.movedBoxes(block, x, y, z, { extending, source, progress });
            const moved = this.world.setMovingBlock(this.release, { x, y, z, facing, extending, source, progress, shapes, ...this.movedOutline(block, x, y, z, shapes) }, base ? boxes(base, x, y, z) : undefined);
            const cell = key(x, y, z);
            this.moving.set(cell, [x, y, z, moved.map(({ min, max }): Box => [min.x, min.y, min.z, max.x, max.y, max.z])]);
            before.delete(cell);
        }
        for (const [x, y, z] of before.values()) this.world.clearMovingBlock(x, y, z);
    }

    /**
     * The shulker boxes with a lid open around the player, after sync: such a cell collides as its block entity says
     * (ShulkerBoxBlock.getShape), so the whole list is given each tick, as setMovingBlocks does. A cell that left the
     * list is a plain block again, and the next sync reads it.
     */
    setShulkerBoxes(boxes: readonly { x: number; y: number; z: number; bounds: NativeAabb }[]): void {
        const before = new Map(this.shulkers);
        this.shulkers.clear();
        for (const { x, y, z, bounds } of boxes) {
            const block = this.blockAt(x, y, z);
            if (!block?.name?.endsWith('shulker_box')) continue;
            this.world.setBlockShapes(x, y, z, block.id, [{
                minX: bounds.min.x, minY: bounds.min.y, minZ: bounds.min.z, maxX: bounds.max.x, maxY: bounds.max.y, maxZ: bounds.max.z, slipperiness: 0.6,
            }]);
            const cell = key(x, y, z);
            this.shulkers.set(cell, [x, y, z, [bounds.min.x, bounds.min.y, bounds.min.z, bounds.max.x, bounds.max.y, bounds.max.z]]);
            before.delete(cell);
        }
        // (at once: the sync of this tick has run, and a shut lid is a cube for this tick's move)
        for (const [x, y, z] of before.values()) {
            const block = this.blockAt(x, y, z);
            if (block?.name?.endsWith('shulker_box')) {
                this.world.setBlockShapes(x, y, z, block.id, [{ minX: 0, minY: 0, minZ: 0, maxX: 1, maxY: 1, maxZ: 1, slipperiness: 0.6 }]);
            }
            this.changed.add(key(x, y, z));
        }
    }

    /**
     * The boxes of the block a block entity moves, in its cell. On 1.11 and 1.12 a source piston's head is SHORT by its progress,
     * which no data value holds: its arm then stops at its cell's face, where the long one reaches a quarter into the base
     * (1.12.2 PistonMovingBlockEntity.java:331-338, PistonHeadBlock.java:42-53; LegacyNeighbourDump `entities`).
     *
     * On 1.8 the moving block takes the moved block's getCollisionShape at its own cell (1.8.8 MovingPistonBlock.java getCollisionBox),
     * which is the block's bounds: a piston base sets them from the block at that cell, the piston_extension, so a cube
     * (PistonBaseBlock.java:179-225), and a fence's bounds are never left higher than its block (FenceBlock.java:41-86).
     */
    private movedBoxes(block: PrismarineBlock, x: number, y: number, z: number, moving?: { extending: boolean; source: boolean; progress: number }): NativeAabb[] {
        let boxes = this.mapper.boxesAt(this.blockAt, x, y, z, this.mapper.describe(block));
        if (compareReleases(this.release, '1.9') < 0) {
            if (block.name === 'piston' || block.name === 'sticky_piston') boxes = [[0, 0, 0, 1, 1, 1]];
            else if (/fence$/.test(block.name)) boxes = boxes.map(([minX, minY, minZ, maxX, maxY, maxZ]): Box => [minX, minY, minZ, maxX, Math.min(maxY, 1), maxZ]);
        }
        const legacyShort = compareReleases(this.release, '1.11') >= 0 && compareReleases(this.release, '1.13') < 0;
        if (moving && legacyShort && block.name === 'piston_head' && moving.source && moving.extending !== (Math.fround(1 - moving.progress) < 0.25)) {
            const held = (value: number) => Math.min(Math.max(value, 0), 1);
            boxes = boxes.map((box): Box => box.map(held) as Box);
        }
        return boxes.map(([minX, minY, minZ, maxX, maxY, maxZ]): NativeAabb => ({ min: { x: minX, y: minY, z: minZ }, max: { x: maxX, y: maxY, z: maxZ } }));
    }

    /**
     * The box that pushes on 1.9 - 1.10: the moved block's getBoundingBox in the cell, its outline before the flattening (a fence's no
     * higher than its block, a layer of snow's an eighth), its collision boxes after (1.9.4 PistonMovingBlockEntity.java:80-89,
     * LegacyNeighbourDump `entities`).
     */
    private movedOutline(block: PrismarineBlock, x: number, y: number, z: number, shapes: NativeAabb[]): { outlineShapes?: NativeAabb[] } {
        if (block.name === 'piston_head') return {};
        if (compareReleases(this.release, '1.13') >= 0) return outlineOf(block, shapes);
        const described = this.mapper.describe(block);
        const at = (a: number, b: number, c: number) => (a === x && b === y && c === z ? described : this.blockAt(a, b, c));
        const outlines = this.mapper.updatesAt(at, x, y, z).flatMap(({ outlineShape, shape }): NativeAabb[] => {
            const box = outlineShape ?? shape;
            return box && box.maxX > box.minX && box.maxY > box.minY && box.maxZ > box.minZ
                ? [{ min: { x: box.minX, y: box.minY, z: box.minZ }, max: { x: box.maxX, y: box.maxY, z: box.maxZ } }] : [];
        });
        return { outlineShapes: outlines };
    }

    /** The moving blocks as the engine's pushes take them (NativeMovingPiston), their shapes those setMovingBlocks gives the cells. */
    movingPistonsOf(blocks: readonly MovingBlock[]): NativeMovingPiston[] {
        const shapes = (block: PrismarineBlock, x: number, y: number, z: number, moving?: { extending: boolean; source: boolean; progress: number }) =>
            this.movedBoxes(block, x, y, z, moving);
        return blocks.map(({ x, y, z, facing, extending, source, progress, block, next }) => ({
            x, y, z, facing, extending, source, progress,
            shapes: shapes(block, x, y, z, { extending, source, progress }),
            ...this.movedOutline(block, x, y, z, shapes(block, x, y, z, { extending, source, progress })),
            ...(next ? { nextShapes: shapes(next, x, y, z, { extending, source, progress: Math.min(Math.fround(progress + 0.5), 1) }) } : {}),
            bounces: block.name === 'slime_block' || block.name === 'slime',
            stickyForEntities: block.name === 'honey_block',
        }));
    }

    /** Whether rain falls at a block, on the bot's own rain level, sky light and biome (Level.isRainingAt, rain.ts). */
    rainingAt(source: PrismarineWorld, x: number, y: number, z: number, top: number, seaLevel: number): boolean {
        const version = releaseCheck(this.release);
        const here = new Cursor(x, y, z);
        const biome = (source.getBlock(here) as { biome?: BiomeData } | null | undefined)?.biome;
        if (!biome?.name || !rainsInBiome(version, biome, x, y, z, seaLevel)) return false;
        if ((source.getSkyLight?.(here) ?? 0) < 15) return false;
        // getHeightmapPos(MOTION_BLOCKING) above it: a block of that kind here or higher
        for (let height = y; height < top; height += 1) {
            const block = source.getBlock(new Cursor(x, height, z));
            if (block && this.mapper.blocksPrecipitation(block)) return false;
        }
        return true;
    }

    /** The collision boxes the engine sees within the box, those that meet it. */
    collisions(box: BoxLike): NativeAabb[] {
        const shapes = this.world.collisionShapes({ x: box.minX, y: box.minY, z: box.minZ }, { x: box.maxX, y: box.maxY, z: box.maxZ }, this.release);
        return shapes.filter(({ min, max }) => min.x < box.maxX && max.x > box.minX && min.y < box.maxY && max.y > box.minY && min.z < box.maxZ && max.z > box.minZ);
    }

    /**
     * A moving piston block's collision boxes at a cell, in the cell and free to leave it, as setMovingBlocks gave them
     * (MovingPistonBlock.getCollisionShape, PistonMovingBlockEntity.getCollisionShape); undefined for any other cell, or one
     * whose block the packets have since replaced.
     */
    movingBoxesAt(x: number, y: number, z: number): Box[] | undefined {
        const moved = this.moving.get(key(x, y, z));
        const name = moved && this.blockAt(x, y, z)?.name;
        return name === 'moving_piston' || name === 'piston_extension' ? moved![3] : undefined;
    }

    /** An open shulker box's collision box at a cell, in the cell, as setShulkerBoxes gave it; undefined for any other cell. */
    shulkerLidAt(x: number, y: number, z: number): Box | undefined {
        return this.shulkers.get(key(x, y, z))?.[3];
    }

    /**
     * The collision boxes of the block the sync read at a cell, in the cell: an open shulker box's lid as setShulkerBoxes gave
     * it, a moving piston block's boxes as setMovingBlocks did.
     */
    boxesAt(x: number, y: number, z: number): Box[] {
        const lid = this.shulkers.get(key(x, y, z));
        if (lid) return [lid[3]];
        const moved = this.movingBoxesAt(x, y, z);
        if (moved) return moved;
        const block = this.blockAt(x, y, z);
        return block ? this.mapper.boxesAt(this.blockAt, x, y, z, block) : [];
    }

    /** Whether the NativeWorld holds the chunk column as loaded (the sync read a block of it from a loaded column). */
    chunkLoaded(chunkX: number, chunkZ: number): boolean {
        return this.world.isChunkLoaded(chunkX, chunkZ);
    }

    /** BlockState.isFaceSturdy(DOWN) (1.16+) of the block the sync read at a cell; false where it read none. */
    bottomFaceSturdyAt(x: number, y: number, z: number): boolean {
        const block = this.blockAt(x, y, z);
        return block !== null && this.mapper.bottomFaceSturdy(block);
    }

    /** The block the sync read at a cell (null where the world had none or it was not read). */
    readonly blockAt = (x: number, y: number, z: number): DescribedBlock | null => {
        const id = this.ids.get(key(x, y, z));
        return id === undefined || id === UNKNOWN ? null : this.described.get(id) ?? null;
    };

    /**
     * Takes the sync off its world and lets go of what it read, which a sync can do once: its listeners leave the world
     * (there is no other way out of an emitter), a change told after it is ignored, and `sync` refuses to run. The
     * NativeWorld stays readable for a caller that holds it. A second call does nothing.
     */
    dispose(): void {
        if (this.disposed) return;
        this.disposed = true;
        this.detach();
        this.source = null;
        this.watched = false;
        this.clearCaches();
        this.described.clear();
        this.moving.clear();
    }

    private reset(source: PrismarineWorld) {
        this.detach();
        this.source = source;
        this.forget();
        this.watched = typeof source.on === 'function';
        if (!this.watched) return;
        // (a listener a world has no way to take off stays, and changes nothing once this is not its source)
        const listen = (event: string, listener: (...args: any[]) => void) => {
            const guarded = (...args: any[]) => {
                if (this.source === source) listener(...args);
            };
            this.listeners.push([event, guarded]);
            source.on?.(event, guarded);
        };
        listen('blockUpdate', (_old: unknown, block: { position?: Vec3Like } | null) => {
            if (block?.position) this.blockChanged(block.position);
        });
        listen('chunkColumnLoad', (corner: Vec3Like) => this.columnChanged(Math.floor(corner.x / 16), Math.floor(corner.z / 16), true));
        listen('chunkColumnUnload', (corner: Vec3Like) => this.columnChanged(Math.floor(corner.x / 16), Math.floor(corner.z / 16), false));
    }

    /** Takes the listeners off the source they were put on. */
    private detach(): void {
        const source = this.source;
        const remove = source?.off ?? source?.removeListener;
        if (source && remove) for (const [event, listener] of this.listeners) remove.call(source, event, listener);
        this.listeners = [];
    }

    /** Forgets every block read, in a fresh NativeWorld. */
    private forget() {
        this.clearCaches();
        this.world = new this.binding.NativeWorld();
        this.world.setRelease?.(this.release);
    }

    private clearCaches() {
        this.ids.clear();
        this.present.clear();
        this.current.clear();
        this.outdated.clear();
        this.changed.clear();
        this.loadedChunks.clear();
        this.lastChunk = '';
        this.region = null;
        this.bounds = null;
    }

    private read(source: PrismarineWorld, x: number, y: number, z: number): number {
        const pos = this.cursor.set(x, y, z);
        const chunk = `${x >> 4},${z >> 4}`;
        if (chunk !== this.lastChunk) {
            this.lastChunk = chunk;
            this.lastChunkLoaded = !source.getColumnAt || source.getColumnAt(pos) != null;
        }
        if (!this.lastChunkLoaded) {
            // (a world that tells nothing: finding a column gone is how the rescan learns it, as columnChanged does from the event)
            if (this.loadedChunks.delete(chunk)) this.world.setChunkLoaded(x >> 4, z >> 4, false);
            return UNKNOWN;
        }
        let id = source.getBlockStateId?.(pos);
        let block: PrismarineBlock | null | undefined;
        if (id == null) {
            block = source.getBlock(pos);
            if (!block) return UNKNOWN;
            id = block.stateId ?? ((block.type << 4) | block.metadata);
        }
        if (!this.described.has(id)) {
            block ??= source.getBlock(pos);
            if (!block) return UNKNOWN;
            this.described.set(id, this.mapper.describe(block));
        }
        if (!this.loadedChunks.has(chunk)) {
            this.loadedChunks.add(chunk);
            this.world.setChunkLoaded(x >> 4, z >> 4, true);
            this.readSections(source, x, z);
        }
        return id;
    }

    /**
     * Whether each section of a column is there: an all-air section the column keeps is present, one it left out is not,
     * and neither holds a block for the NativeWorld to tell by.
     */
    private readSections(source: PrismarineWorld, x: number, z: number): void {
        const column = source.getColumnAt?.(new Cursor(x, 0, z)) as SectionedColumn | null | undefined;
        if (!Array.isArray(column?.sections)) return;
        const bottom = (column.minY ?? 0) >> 4;
        for (let i = 0; i < column.sections.length; i += 1) {
            this.world.setSectionPresent(x >> 4, bottom + i, z >> 4, column.sections[i] != null);
        }
    }

    /** A block changed: the column makes the section it fell in. */
    private readSection(source: PrismarineWorld, x: number, y: number, z: number): void {
        if (!this.loadedChunks.has(`${x >> 4},${z >> 4}`)) return;
        const column = source.getColumnAt?.(new Cursor(x, y, z)) as SectionedColumn | null | undefined;
        if (!Array.isArray(column?.sections)) return;
        const index = (y - (column.minY ?? 0)) >> 4;
        if (index < 0 || index >= column.sections.length) return;
        this.world.setSectionPresent(x >> 4, y >> 4, z >> 4, column.sections[index] != null);
    }
}
