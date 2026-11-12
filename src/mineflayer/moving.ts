// The blocks pistons move, as the client makes them. The server sends only the block event: the client moves the blocks
// itself (PistonBaseBlock.triggerEvent, moveBlocks), and the moving_piston cells never come in a packet, so a world of
// packets has none of them until the blocks settle. This does for such a world what the client does for its level, and
// gives NativeWorldSync.setMovingBlocks what the recorder's replay/moving.ts gives it from a recording.
//
// What moves is PistonStructureResolver's, slime and honey branches included. What may be moved is PistonBaseBlock.isPushable,
// with a block's PushReaction and whether it has a block entity from push-reactions.json, read from each release's jar
// (scripts/generate-push-reactions.cjs). A release that is not there falls back on the names below.
import { compareReleases, type PrismarineBlock } from './blocks';
import PUSH_REACTIONS from './push-reactions.json';
import { Cursor, type MovingBlock, type PrismarineWorld, type Vec3Like } from './world';

type Cell = [number, number, number];

/** A world the tracker can move blocks in: prismarine-world's setBlockStateId. */
export interface MovableWorld extends PrismarineWorld {
    setBlockStateId?(pos: Vec3Like, stateId: number): unknown;
}

interface BlockClass {
    fromStateId(stateId: number, biomeId: number): PrismarineBlock;
    fromProperties(name: string, properties: Record<string, unknown>, biomeId: number): PrismarineBlock;
}

export interface MovingRegistry {
    blocksByName: Record<string, { id: number; defaultState?: number } | undefined>;
}

interface Entry {
    readonly pos: Cell;
    readonly facing: number;
    readonly extending: boolean;
    readonly source: boolean;
    progress: number;
    /** The block it moves, set when it ends (finalTick). */
    readonly moved: PrismarineBlock;
}

/** Direction.from3DDataValue: down, up, north, south, west, east. */
export const DIRECTIONS: Cell[] = [[0, -1, 0], [0, 1, 0], [0, 0, -1], [0, 0, 1], [-1, 0, 0], [1, 0, 0]];
const FACINGS = ['down', 'up', 'north', 'south', 'west', 'east'];
/** PistonStructureResolver.MAX_PUSH_DEPTH. */
const MAX_PUSHED = 12;
const PISTON_BASE = new Set(['piston', 'sticky_piston']);
const EMPTY = new Set(['air', 'cave_air', 'void_air']);
/** PushReaction.BLOCK, and those a hardness of -1 does not tell (26.3 Blocks.java, PistonBaseBlock.isPushable). */
const IMMOVABLE = /^(obsidian|crying_obsidian|respawn_anchor|reinforced_deepslate|enchanting_table|ender_chest|beacon|.*chest|.*furnace|smoker|hopper|dispenser|dropper|.*sign|spawner|mob_spawner|jukebox|lectern|.*banner)$/;
/** PushReaction.PUSH_ONLY, as far as a name tells it: a line may push these and no block drags them (GlazedTerracottaBlock from 1.12). */
const PUSH_ONLY = /glazed_terracotta$/;
const SLIME = new Set(['slime_block', 'slime']);
/** What isPushable names itself before PushReaction asks (1.8.8 PistonBaseBlock.java:253, 1.21.8 PistonBaseBlock.isPushable). */
const NAMED = /^(obsidian|crying_obsidian|respawn_anchor|reinforced_deepslate)$/;

interface Reactions { readonly destroy: ReadonlySet<string>; readonly block: ReadonlySet<string>; readonly push: ReadonlySet<string>; readonly entity: ReadonlySet<string> }

const reactionsOf = (release: string): Reactions | undefined => {
    const indices = (PUSH_REACTIONS.releases as Record<string, Record<keyof Reactions, number> | undefined>)[release];
    if (!indices) return undefined;
    const set = (kind: keyof Reactions) => new Set(PUSH_REACTIONS.sets[indices[kind]]);
    return { destroy: set('destroy'), block: set('block'), push: set('push'), entity: set('entity') };
};

const key = ([x, y, z]: Cell) => `${x},${y},${z}`;
const along = ([x, y, z]: Cell, [dx, dy, dz]: Cell, by = 1): Cell => [x + dx * by, y + dy * by, z + dz * by];
const reversed = ([x, y, z]: Cell): Cell => [-x, -y, -z];
const sameCell = (a: Cell, b: Cell) => a[0] === b[0] && a[1] === b[1] && a[2] === b[2];

export class MovingBlockTracker {
    private readonly release: string;
    private readonly registry: MovingRegistry;
    private readonly world: MovableWorld;
    private readonly entries = new Map<string, Entry>();
    private readonly reactions: Reactions | undefined;

    constructor(registry: MovingRegistry, release: string, world: MovableWorld) {
        this.reactions = reactionsOf(release);
        this.registry = registry;
        this.release = release;
        this.world = world;
    }

    private atLeast(release: string): boolean {
        return compareReleases(this.release, release) >= 0;
    }

    private get legacy(): boolean {
        return !this.atLeast('1.13');
    }

    private blockAt([x, y, z]: Cell): PrismarineBlock | null {
        return this.world.getBlock(new Cursor(x, y, z)) ?? null;
    }

    private place([x, y, z]: Cell, stateId: number): void {
        this.world.setBlockStateId?.(new Cursor(x, y, z), stateId);
    }

    private clear(cell: Cell): void {
        this.place(cell, this.registry.blocksByName.air?.defaultState ?? 0);
    }

    /** The state of a block by its name: its metadata before the flattening, its properties from it. */
    private state(like: PrismarineBlock, name: string, properties: Record<string, unknown>, metadata: number): PrismarineBlock {
        const Block = like.constructor as unknown as BlockClass;
        const info = this.registry.blocksByName[name];
        if (!info) throw new Error(`${this.release} has no block ${name}`);
        if (this.legacy) return Block.fromStateId(info.id << 4 | metadata, 0);
        const defaults = Block.fromStateId(info.defaultState as number, 0).getProperties();
        return Block.fromProperties(name, { ...defaults, ...properties }, 0);
    }

    private stateIdOf(block: PrismarineBlock): number {
        return block.stateId ?? (block.type << 4 | block.metadata);
    }

    /** The moving_piston block, whose shape is its block entity's. */
    private movingStateId(facing: number): number {
        const info = this.registry.blocksByName.moving_piston ?? this.registry.blocksByName.piston_extension;
        if (!info) throw new Error(`${this.release} has no moving piston block`);
        return info.defaultState ?? (info.id << 4 | facing);
    }

    /** A block that pops rather than moves, as far as its shape tells it (PushReaction.POPPED, DESTROY before 26.3). */
    private pops(block: PrismarineBlock): boolean {
        if (this.reactions) return this.reactions.destroy.has(block.name);
        return (block as { boundingBox?: string }).boundingBox === 'empty' && !EMPTY.has(block.name);
    }

    /** PushReaction.PUSH_ONLY. */
    private pushOnly(block: PrismarineBlock): boolean {
        return this.reactions ? this.reactions.push.has(block.name) : this.atLeast('1.12') && PUSH_ONLY.test(block.name);
    }

    /** PistonBaseBlock.isPushable. `connection` is the way the block is being reached, for a PUSH_ONLY block. */
    private pushable(block: PrismarineBlock | null, way: Cell, poppable: boolean, connection: Cell): block is PrismarineBlock {
        if (!block) return false;
        if (EMPTY.has(block.name)) return true;
        if (PISTON_BASE.has(block.name)) return this.legacy ? (block.metadata & 8) === 0 : block.getProperties().extended !== true;
        const hard = (block as { hardness?: number | null }).hardness;
        if (this.reactions) {
            if (hard === null || hard === -1 || NAMED.test(block.name) || this.reactions.block.has(block.name)) return false;
            if (this.reactions.destroy.has(block.name)) return poppable;
            if (this.reactions.push.has(block.name)) return sameCell(way, connection);
            return !this.reactions.entity.has(block.name);
        }
        if (this.pops(block)) return poppable;
        if (hard === null || hard === -1 || IMMOVABLE.test(block.name)) return false;
        return !this.pushOnly(block) || sameCell(way, connection);
    }

    /** PistonStructureResolver.isSticky: slime, and honey from 1.15 (1.14.4 PistonStructureResolver.java, 1.15 :42). */
    private sticky(block: PrismarineBlock | null): boolean {
        return block !== null && (SLIME.has(block.name) || block.name === 'honey_block');
    }

    /** canStickToEachOther, 1.15's: honey and slime do not hold each other, and before honey any block a slime block meets does. */
    private sticks(one: PrismarineBlock | null, other: PrismarineBlock | null): boolean {
        if (!one || !other) return false;
        if ((one.name === 'honey_block' && SLIME.has(other.name)) || (SLIME.has(one.name) && other.name === 'honey_block')) return false;
        return this.sticky(one) || this.sticky(other);
    }

    /** PistonStructureResolver.resolve: the blocks that move in the order they do, null when the piston cannot. */
    private resolve(base: Cell, start: Cell, push: Cell, extending: boolean): Cell[] | null {
        const direction = extending ? push : reversed(push);
        const toPush: Cell[] = [];
        const at = (list: Cell[], cell: Cell) => list.findIndex(other => sameCell(other, cell));
        const isAir = (block: PrismarineBlock | null) => block === null || EMPTY.has(block.name);

        const addLine = (from: Cell, connection: Cell): boolean => {
            let next = this.blockAt(from);
            if (isAir(next) || !this.pushable(next, push, false, connection) || sameCell(from, base) || at(toPush, from) >= 0) return true;
            let count = 1;
            if (count + toPush.length > MAX_PUSHED) return false;
            // the blocks a sticky block drags behind it
            while (this.sticky(next)) {
                const cell = along(from, reversed(push), count);
                const previous = next;
                next = this.blockAt(cell);
                if (isAir(next) || !this.sticks(previous, next) || !this.pushable(next, push, false, reversed(push)) || sameCell(cell, base)) break;
                count += 1;
                if (count + toPush.length > MAX_PUSHED) return false;
            }
            let added = 0;
            for (let i = count - 1; i >= 0; i -= 1) {
                toPush.push(along(from, reversed(push), i));
                added += 1;
            }
            for (let i = 1; ; i += 1) {
                const cell = along(from, push, i);
                const met = at(toPush, cell);
                if (met >= 0) {
                    // reorderListAtCollision: the new line goes before what it met, which it must move first
                    const head = toPush.slice(0, met);
                    const line = toPush.slice(toPush.length - added);
                    const rest = toPush.slice(met, toPush.length - added);
                    toPush.length = 0;
                    toPush.push(...head, ...line, ...rest);
                    for (let j = 0; j <= met + added; j += 1) {
                        if (this.sticky(this.blockAt(toPush[j]!)) && !addBranches(toPush[j]!)) return false;
                    }
                    return true;
                }
                next = this.blockAt(cell);
                if (isAir(next)) return true;
                if (!this.pushable(next, push, true, push) || sameCell(cell, base)) return false;
                if (this.pops(next!)) return true;
                if (toPush.length >= MAX_PUSHED) return false;
                toPush.push(cell);
                added += 1;
            }
        };
        const addBranches = (from: Cell): boolean => {
            const block = this.blockAt(from);
            for (const side of DIRECTIONS) {
                if (sameCell(side, push) || sameCell(side, reversed(push))) continue;
                const cell = along(from, side);
                if (this.sticks(this.blockAt(cell), block) && !addLine(cell, side)) return false;
            }
            return true;
        };

        const first = this.blockAt(start);
        if (!this.pushable(first, push, false, direction)) return extending && first !== null && this.pops(first) ? [] : null;
        if (!addLine(start, push)) return null;
        for (let i = 0; i < toPush.length; i += 1) {
            if (this.sticky(this.blockAt(toPush[i]!)) && !addBranches(toPush[i]!)) return null;
        }
        return toPush;
    }

    /** The blocks of the list go one cell along `way`, each cell left empty unless a block takes it (moveBlocks). */
    private moveLine(cells: Cell[], way: Cell, facing: number, extending: boolean): void {
        const line = cells.map((cell): [Cell, PrismarineBlock] => [cell, this.blockAt(cell)!]);
        for (const [cell] of line) this.clear(cell);
        for (const [cell, block] of [...line].reverse()) {
            const pos = along(cell, way);
            this.place(pos, this.movingStateId(facing));
            this.entries.set(key(pos), { pos, facing, extending, source: false, progress: 0, moved: block });
        }
    }

    /** PistonBaseBlock.triggerEvent on the client, for a block event of a piston: 0 extends it, 1 and 2 retract it. */
    trigger(name: string, base: Cell, event: number, param: number): void {
        if (!PISTON_BASE.has(name)) return;
        const facing = param & 7;
        const direction = DIRECTIONS[facing];
        const like = this.blockAt(base);
        if (!direction || !like) return;
        const sticky = name === 'sticky_piston';
        const named = FACINGS[facing];
        const type = sticky ? 'sticky' : 'normal';
        const front = along(base, direction);
        if (event === 0) {
            // moveBlocks(extending): before 1.11 the head is no source (1.10.2 PistonBaseBlock.java:332, 1.11.2 :321)
            this.place(base, this.stateIdOf(this.state(like, name, { facing: named, extended: true }, facing | 8)));
            // (a server sends the event only after this resolved: a list refused here is a world that is out of step)
            this.moveLine(this.resolve(base, front, direction, true) ?? [], direction, facing, true);
            const head = this.state(like, 'piston_head', { facing: named, short: false, type }, facing | (sticky ? 8 : 0));
            this.place(front, this.movingStateId(facing));
            this.entries.set(key(front), { pos: front, facing, extending: true, source: this.atLeast('1.11'), progress: 0, moved: head });
        } else if (event === 1 || event === 2) {
            // the head's own block entity ends now (finalTick, 26.3 PistonBaseBlock.java triggerEvent)
            const moved = this.state(like, name, { facing: named, extended: false }, facing);
            this.finish(front);
            this.place(base, this.movingStateId(facing));
            this.entries.set(key(base), { pos: base, facing, extending: false, source: true, progress: 0, moved });
            const dragged = along(base, direction, 2);
            // a sticky piston ends the block it was still pushing there, and drags nothing: pistonPiece, on every release
            // (26.3 PistonBaseBlock.java:173-186, 1.8.8 PistonBaseBlock.java:155)
            const pushing = sticky ? this.entries.get(key(dragged)) : undefined;
            if (pushing?.extending && pushing.facing === facing) {
                this.finish(dragged);
                return;
            }
            this.clear(front);
            if (sticky && event === 1) {
                // triggerEvent drags only what isPushable takes and pushes as a whole (26.3 PistonBaseBlock.java:181-197)
                const block = this.blockAt(dragged);
                if (block && !EMPTY.has(block.name) && this.pushable(block, reversed(direction), false, direction) && !this.pops(block)
                    && (PISTON_BASE.has(block.name) || !this.pushOnly(block))) {
                    this.moveLine(this.resolve(base, dragged, reversed(direction), false) ?? [], reversed(direction), facing, false);
                }
            }
        }
    }

    /** PistonMovingBlockEntity.finalTick: a block entity there ends now, the block it moved set in its cell. */
    private finish(cell: Cell): void {
        const entry = this.entries.get(key(cell));
        if (!entry) return;
        this.entries.delete(key(cell));
        this.place(cell, this.stateIdOf(entry.moved));
    }

    /** A block the server set: one other than the moving block where a block moves replaces it (setBlockState's removeBlockEntity). */
    blockChanged(cell: Cell, name: string | undefined): void {
        if (name !== 'moving_piston' && name !== 'piston_extension') this.entries.delete(key(cell));
    }

    /** The block entities' tick, after the entities': 0.5F further each, and one that had reached 1 ends with its block set. */
    tick(): void {
        for (const entry of [...this.entries.values()]) {
            if (entry.progress >= 1) this.finish(entry.pos);
            else {
                entry.progress = Math.min(Math.fround(entry.progress + 0.5), 1);
            }
        }
    }

    /** The block a block entity shows at its progress (the recorder's replay/moving.ts shown; 26.3 PistonMovingBlockEntity.java:340-344). */
    private shown(entry: Entry, progress = entry.progress): PrismarineBlock {
        if (!entry.source || !this.atLeast('1.11')) return entry.moved;
        // (1.13 to 1.15 compare with 4.0F, which every progress is under: 1.15.2 PistonMovingBlockEntity.java:351, 1.13.2 :344)
        const limit = this.atLeast('1.13') && !this.atLeast('1.16') ? 4 : 0.25;
        const short = entry.extending !== (Math.fround(1 - progress) < limit);
        return this.state(entry.moved, 'piston_head', { facing: FACINGS[entry.facing], short }, entry.facing);
    }

    /** The extended base a source piston leaves as it retracts, from 1.11 (26.3 PistonMovingBlockEntity.java:329-330). */
    private base(entry: Entry): PrismarineBlock | undefined {
        if (entry.extending || !entry.source || !this.atLeast('1.11') || !PISTON_BASE.has(entry.moved.name)) return undefined;
        return this.state(entry.moved, entry.moved.name, { facing: FACINGS[entry.facing], extended: true }, entry.facing | 8);
    }

    /** The moving blocks now, as NativeWorldSync.setMovingBlocks takes them. */
    blocks(): MovingBlock[] {
        return [...this.entries.values()].map(entry => {
            const [x, y, z] = entry.pos;
            const base = this.base(entry);
            return {
                x, y, z,
                facing: entry.facing,
                extending: entry.extending,
                source: entry.source,
                progress: entry.progress,
                block: this.shown(entry),
                next: this.shown(entry, Math.min(Math.fround(entry.progress + 0.5), 1)),
                ...(base ? { base } : {}),
            };
        });
    }
}
