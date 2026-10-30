// A NativeWorld kept in step with a prismarine world (mineflayer's bot.world, or any world with getBlock): before each
// tick the blocks around the simulated entity are read again, and every block whose own state or a neighbour's changed
// is written anew (its shape before 1.13, its fluid flow and the trapdoor over a ladder depend on the blocks around).
import type { NativeAabb, NativeBlockUpdate, NativePhysicsBinding, NativeWorld } from '../native';
import { BlockMapper, describeBlock, type DescribedBlock, type PrismarineBlock } from './blocks';
import type { Box } from './legacy';

export interface Vec3Like {
    x: number;
    y: number;
    z: number;
}

/** A prismarine world: mineflayer's bot.world, or anything that gives the prismarine block at a position. */
export interface PrismarineWorld {
    getBlock(pos: Vec3Like): PrismarineBlock | null | undefined;
    /** The state id at a position, read without building a block (prismarine-world's). */
    getBlockStateId?(pos: Vec3Like): number | null | undefined;
    /** The chunk column at a position (prismarine-world's): where it has none, its state id is 0 and no block. */
    getColumnAt?(pos: Vec3Like): unknown;
    /**
     * prismarine-world's events: a world that emits 'blockUpdate' (and the chunk columns it loads and unloads) has only
     * the blocks it changed and those newly near the entity read again; any other world has all of them read each tick.
     */
    on?(event: string, listener: (...args: never[]) => void): unknown;
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
const key = (x: number, y: number, z: number) => `${x},${y},${z}`;

/** Cells from (x0, y0, z0) to (x1, y1, z1), inclusive. */
type Region = [number, number, number, number, number, number];
const inside = (r: Region, x: number, y: number, z: number) => x >= r[0] && x <= r[3] && y >= r[1] && y <= r[4] && z >= r[2] && z <= r[5];
const grown = (r: Region, by: number): Region => [r[0] - by, r[1] - by, r[2] - by, r[3] + by, r[4] + by, r[5] + by];
const sameRegion = (a: Region, b: Region) => a[0] === b[0] && a[1] === b[1] && a[2] === b[2] && a[3] === b[3] && a[4] === b[4] && a[5] === b[5];
class Cursor implements Vec3Like {
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

export class NativeWorldSync {
    world: NativeWorld;
    private readonly binding: NativePhysicsBinding;
    private readonly mapper: BlockMapper;
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
    private readonly changed = new Set<string>();
    private region: Region | null = null;

    constructor(binding: NativePhysicsBinding, registry: SyncRegistry) {
        this.binding = binding;
        this.world = new binding.NativeWorld();
        this.mapper = new BlockMapper(registry.version, registry);
    }

    /** A block the world changed: it is read again at the next sync (the world's own events do this when it has them). */
    blockChanged(pos: Vec3Like): void {
        this.changed.add(key(Math.floor(pos.x), Math.floor(pos.y), Math.floor(pos.z)));
    }

    /** A chunk column the world loaded or unloaded: its blocks are read again where the entity reaches them. */
    columnChanged(chunkX: number, chunkZ: number, loaded: boolean): void {
        for (const cell of [...this.ids.keys()]) {
            const [x, y, z] = cell.split(',').map(Number);
            if (x >> 4 !== chunkX || z >> 4 !== chunkZ) continue;
            this.ids.delete(cell);
            this.outdateAround(x, y, z);
        }
        if (!loaded) {
            this.loadedChunks.delete(`${chunkX},${chunkZ}`);
            this.lastChunk = '';
            this.world.setChunkLoaded(chunkX, chunkZ, false);
        }
        this.region = null;
    }

    /**
     * Brings the blocks within `margin` of `box` up to date with `source` (a new source forgets the blocks of the old
     * one). The blocks within one more block are read too, as the ones nearest the box are made from them.
     */
    sync(source: PrismarineWorld, box: BoxLike, margin: number): void {
        if (source !== this.source) this.reset(source);
        else if (this.ids.size > CACHE_LIMIT) this.forget();
        this.lastChunk = '';
        const region: Region = [
            Math.floor(box.minX - margin), Math.floor(box.minY - margin), Math.floor(box.minZ - margin),
            Math.floor(box.maxX + margin), Math.floor(box.maxY + margin), Math.floor(box.maxZ + margin),
        ];
        const last = this.region;
        // a world that tells its changes: only the blocks the last sync did not read, and those that changed
        const watched = this.watched && last !== null;
        if (watched && this.changed.size === 0 && sameRegion(last, region)) return;
        const reach = grown(region, 1);
        const lastReach = last && grown(last, 1);
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
        for (let x = x0 - 1; x <= x1 + 1; x += 1) {
            for (let z = z0 - 1; z <= z1 + 1; z += 1) {
                for (let y = y0 - 1; y <= y1 + 1; y += 1) {
                    // (the blocks of a world that tells its changes stay as read until it does)
                    if (!watched) readAt(x, y, z);
                    else if (!inside(lastReach as Region, x, y, z) && !this.ids.has(key(x, y, z))) readAt(x, y, z);
                }
            }
        }
        for (const cell of this.changed) {
            const [x, y, z] = cell.split(',').map(Number);
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
            else this.present.delete(cell);
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
                } catch {
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
        for (let dx = -1; dx <= 1; dx += 1) {
            for (let dy = -1; dy <= 1; dy += 1) {
                for (let dz = -1; dz <= 1; dz += 1) {
                    const cell = key(x + dx, y + dy, z + dz);
                    if (this.current.delete(cell) || (dx === 0 && dy === 0 && dz === 0)) this.outdated.add(cell);
                }
            }
        }
    }

    /** The collision boxes the engine sees within the box, those that meet it. */
    collisions(box: BoxLike): NativeAabb[] {
        const shapes = this.world.collisionShapes({ x: box.minX, y: box.minY, z: box.minZ }, { x: box.maxX, y: box.maxY, z: box.maxZ });
        return shapes.filter(({ min, max }) => min.x < box.maxX && max.x > box.minX && min.y < box.maxY && max.y > box.minY && min.z < box.maxZ && max.z > box.minZ);
    }

    /** The collision boxes of the block the sync read at a cell, in the cell. */
    boxesAt(x: number, y: number, z: number): Box[] {
        const block = this.blockAt(x, y, z);
        return block ? this.mapper.boxesAt(this.blockAt, x, y, z, block) : [];
    }

    /** The block the sync read at a cell (null where the world had none or it was not read). */
    readonly blockAt = (x: number, y: number, z: number): DescribedBlock | null => {
        const id = this.ids.get(key(x, y, z));
        return id === undefined || id === UNKNOWN ? null : this.described.get(id) ?? null;
    };

    private reset(source: PrismarineWorld) {
        this.source = source;
        this.forget();
        this.watched = typeof source.on === 'function';
        if (!this.watched) return;
        // (a listener of a world the sync no longer reads changes only what it would read again anyway)
        const listen = (event: string, listener: (...args: never[]) => void) => source.on?.(event, (...args: never[]) => {
            if (this.source === source) listener(...args);
        });
        listen('blockUpdate', (_old: unknown, block: { position?: Vec3Like } | null) => {
            if (block?.position) this.blockChanged(block.position);
        });
        listen('chunkColumnLoad', (corner: Vec3Like) => this.columnChanged(Math.floor(corner.x / 16), Math.floor(corner.z / 16), true));
        listen('chunkColumnUnload', (corner: Vec3Like) => this.columnChanged(Math.floor(corner.x / 16), Math.floor(corner.z / 16), false));
    }

    /** Forgets every block read, in a fresh NativeWorld. */
    private forget() {
        this.ids.clear();
        this.present.clear();
        this.current.clear();
        this.outdated.clear();
        this.changed.clear();
        this.loadedChunks.clear();
        this.lastChunk = '';
        this.region = null;
        this.world = new this.binding.NativeWorld();
    }

    private read(source: PrismarineWorld, x: number, y: number, z: number): number {
        const pos = this.cursor.set(x, y, z);
        const chunk = `${x >> 4},${z >> 4}`;
        if (chunk !== this.lastChunk) {
            this.lastChunk = chunk;
            this.lastChunkLoaded = !source.getColumnAt || source.getColumnAt(pos) != null;
        }
        if (!this.lastChunkLoaded) return UNKNOWN;
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
            this.described.set(id, describeBlock(block));
        }
        if (!this.loadedChunks.has(chunk)) {
            this.loadedChunks.add(chunk);
            this.world.setChunkLoaded(x >> 4, z >> 4, true);
        }
        return id;
    }
}
