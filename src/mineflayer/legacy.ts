// The blocks before 1.13 whose collision boxes and liquid flow the client takes from their neighbours, which the data
// value minecraft-data's shapes go by does not carry. A port of mineflayer-physics-utils' LegacyShapes and of the
// parity harness's legacy-shapes.cjs and legacy-flow.cjs.
//
// legacy-connections.json (mineflayer-physics-utils' legacyConnections.json) lists per release the neighbour block
// states (id << 4 | data value) the fences, panes and walls join, with the boxes of each of the 16 ways to join, the
// normal cubes (EntityPlayerSP.pushOutOfBlocks), the states whose material blocks a liquid's flow and those solid
// beside falling liquid.
import connections from './legacy-connections.json';

export type Box = [number, number, number, number, number, number];
export type Range = [number, number];

/** A neighbouring block before the flattening: its id and data value. */
export interface LegacyBlock {
    type: number;
    metadata: number;
}

export type Neighbour = (dx: number, dy: number, dz: number) => LegacyBlock | null;

interface LegacyFamily {
    blocks: number[];
    connect: { all?: Range[] } & Partial<Record<Side, Range[]>>;
    boxes: Box[][];
}

interface LegacyRelease {
    families: LegacyFamily[];
    normalCube: Range[];
    flowBlocksMovement: Range[];
    flowSolid: Range[];
}

const RELEASES = connections as unknown as Record<string, LegacyRelease>;

type Side = 'north' | 'south' | 'west' | 'east';

const SIDES: [Side, number, number, number][] = [['north', 0, -1, 1], ['south', 0, 1, 2], ['west', -1, 0, 4], ['east', 1, 0, 8]];
const OFFSET: Record<Side, [number, number]> = { north: [0, -1], south: [0, 1], west: [-1, 0], east: [1, 0] };
const OPPOSITE: Record<Side, Side> = { north: 'south', south: 'north', west: 'east', east: 'west' };
const CCW: Record<Side, Side> = { north: 'west', west: 'south', south: 'east', east: 'north' };
const CW: Record<Side, Side> = { north: 'east', east: 'south', south: 'west', west: 'north' };
const axis = (side: Side) => (side === 'north' || side === 'south' ? 'z' : 'x');
// the stairs' FACING by data value & 3 (EnumFacing.getFront(5 - i)), the lower door's (fromHorizontal(i) rotated
// counterclockwise)
const STAIRS_FACING: Side[] = ['east', 'west', 'south', 'north'];
const DOOR_FACING: Side[] = ['east', 'south', 'west', 'north'];

const expand = (ranges: Range[]) => {
    const set = new Set<number>();
    for (const [from, to] of ranges) for (let raw = from; raw <= to; raw += 1) set.add(raw);
    return set;
};

const inRanges = (ranges: Range[], raw: number) => ranges.some(([from, to]) => raw >= from && raw <= to);

export interface LegacyShapes {
    /** A block's boxes among its neighbours, or null when minecraft-data's shape is the block's. */
    shapes(type: number, metadata: number, neighbour: Neighbour): Box[] | null;
    /**
     * Whether EntityPlayerSP.pushOutOfBlocks takes the block for a normal cube (1.8's Block.isFullCube, 1.9+'s
     * IBlockState.isNormalCube): the block's own property, not its collision boxes.
     */
    isNormalCube(type: number, metadata: number): boolean;
}

interface BlockNames {
    blocksArray: { id: number; name: string }[];
    blocksByName: Record<string, { id: number } | undefined>;
}

/** The legacy shapes of a release (its major version, "1.12"), or null after the flattening. */
export function legacyShapes(majorVersion: string, registry: BlockNames): LegacyShapes | null {
    const data = RELEASES[majorVersion];
    if (!data) return null;
    const normalCubes = expand(data.normalCube);
    const families = new Map<number, { joins: Record<Side, Set<number>>; boxes: Box[][] }>();
    for (const family of data.families) {
        const all = family.connect.all ? expand(family.connect.all) : null;
        const joins = Object.fromEntries(SIDES.map(([side]) => [side, all ?? expand(family.connect[side] ?? [])])) as Record<Side, Set<number>>;
        for (const id of family.blocks) families.set(id, { joins, boxes: family.boxes });
    }
    const idsMatching = (pattern: RegExp) => new Set(registry.blocksArray.filter(block => pattern.test(block.name)).map(block => block.id));
    const stairs = idsMatching(/_stairs$/);
    const doors = idsMatching(/_door$/);
    // (1.8's chest boxes are whatever bounds the block was last given, and it has no chorus plant)
    const modern = majorVersion !== '1.8';
    const chests = modern ? idsMatching(/^(chest|trapped_chest)$/) : new Set<number>();
    const idOf = (name: string) => (modern ? registry.blocksByName[name]?.id ?? -1 : -1);
    const chorusPlant = idOf('chorus_plant');
    const chorusFlower = idOf('chorus_flower');
    const endStone = idOf('end_stone');

    function joinMask(joins: Record<Side, Set<number>>, neighbour: Neighbour) {
        let mask = 0;
        for (const [side, dx, dz, bit] of SIDES) {
            const other = neighbour(dx, 0, dz);
            if (other && joins[side].has((other.type << 4) | other.metadata)) mask |= bit;
        }
        return mask;
    }

    // BlockStairs.getActualState's SHAPE (getStairsShape): a stair of the same half ahead turned across makes an outer
    // corner, one behind an inner corner, unless the stair beside on that side continues this one. The boxes: the half
    // slab, the upper half (straight and inner), the quarter (corners) (getCollisionBoxList).
    function stairsShapes(metadata: number, neighbour: Neighbour): Box[] {
        const facing = STAIRS_FACING[metadata & 3];
        const top = (metadata & 4) !== 0;
        const at = (side: Side) => {
            const other = neighbour(OFFSET[side][0], 0, OFFSET[side][1]);
            return other && stairs.has(other.type) ? { facing: STAIRS_FACING[other.metadata & 3], top: (other.metadata & 4) !== 0 } : null;
        };
        const differs = (side: Side) => {
            const other = at(side);
            return !other || other.facing !== facing || other.top !== top;
        };
        let shape = 'straight';
        const ahead = at(facing);
        const behind = at(OPPOSITE[facing]);
        if (ahead && ahead.top === top && axis(ahead.facing) !== axis(facing) && differs(OPPOSITE[ahead.facing])) {
            shape = ahead.facing === CCW[facing] ? 'outer_left' : 'outer_right';
        } else if (behind && behind.top === top && axis(behind.facing) !== axis(facing) && differs(behind.facing)) {
            shape = behind.facing === CCW[facing] ? 'inner_left' : 'inner_right';
        }
        const [y0, y1] = top ? [0, 0.5] : [0.5, 1];
        const half: Record<Side, Box> = {
            north: [0, y0, 0, 1, y1, 0.5],
            south: [0, y0, 0.5, 1, y1, 1],
            west: [0, y0, 0, 0.5, y1, 1],
            east: [0.5, y0, 0, 1, y1, 1],
        };
        const quarter: Record<Side, Box> = {
            north: [0, y0, 0, 0.5, y1, 0.5],
            south: [0.5, y0, 0.5, 1, y1, 1],
            west: [0, y0, 0.5, 0.5, y1, 1],
            east: [0.5, y0, 0, 1, y1, 0.5],
        };
        const boxes: Box[] = [top ? [0, 0.5, 0, 1, 1, 1] : [0, 0, 0, 1, 0.5, 1]];
        if (shape === 'straight' || shape === 'inner_left' || shape === 'inner_right') boxes.push(half[facing]);
        if (shape !== 'straight') {
            const corners: Record<string, Side> = { outer_left: facing, outer_right: CW[facing], inner_right: OPPOSITE[facing], inner_left: CCW[facing] };
            boxes.push(quarter[corners[shape]]);
        }
        return boxes;
    }

    // BlockDoor.getActualState: the lower half (data value < 8) has the facing and opening, the upper half the hinge, each
    // taking the other's from the same door above or below (without it, facing north, closed, hinge left; 1.8 takes the
    // data values of the blocks below and above, whatever they are). The box is 3/16 thick on the side the door closes or
    // opens to.
    function doorShapes(type: number, metadata: number, neighbour: Neighbour): Box[] {
        let facing: Side = 'north';
        let open = false;
        let right = false;
        if (majorVersion === '1.8') {
            const data = (other: LegacyBlock | null) => other?.metadata ?? 0;
            const lower = metadata & 8 ? data(neighbour(0, -1, 0)) : metadata;
            const upper = metadata & 8 ? metadata : data(neighbour(0, 1, 0));
            facing = DOOR_FACING[lower & 3];
            open = (lower & 4) !== 0;
            right = (upper & 1) !== 0;
        } else if (metadata & 8) {
            right = (metadata & 1) !== 0;
            const below = neighbour(0, -1, 0);
            if (below && below.type === type && !(below.metadata & 8)) {
                facing = DOOR_FACING[below.metadata & 3];
                open = (below.metadata & 4) !== 0;
            }
        } else {
            facing = DOOR_FACING[metadata & 3];
            open = (metadata & 4) !== 0;
            const above = neighbour(0, 1, 0);
            if (above && above.type === type && above.metadata & 8) right = (above.metadata & 1) !== 0;
        }
        const thickness = 0.1875;
        const side: Record<Side, Box> = {
            north: [0, 0, 0, 1, 1, thickness],
            south: [0, 0, 1 - thickness, 1, 1, 1],
            east: [1 - thickness, 0, 0, 1, 1, 1],
            west: [0, 0, 0, thickness, 1, 1],
        };
        // closed: at the side it faces away from; open: at the side of its hinge or the other (getBoundingBox)
        const closed: Record<Side, Side> = { east: 'west', south: 'north', west: 'east', north: 'south' };
        const opened: Record<Side, [Side, Side]> = { east: ['south', 'north'], south: ['west', 'east'], west: ['north', 'south'], north: ['east', 'west'] };
        return [side[open ? opened[facing][right ? 0 : 1] : closed[facing]]];
    }

    // BlockChest.getBoundingBox (1.9+): a chest of the same kind beside makes the box reach it (north, south, west, east).
    function chestShapes(type: number, neighbour: Neighbour): Box[] {
        const same = (dx: number, dz: number) => neighbour(dx, 0, dz)?.type === type;
        if (same(0, -1)) return [[0.0625, 0, 0, 0.9375, 0.875, 0.9375]];
        if (same(0, 1)) return [[0.0625, 0, 0.0625, 0.9375, 0.875, 1]];
        if (same(-1, 0)) return [[0, 0, 0.0625, 0.9375, 0.875, 0.9375]];
        if (same(1, 0)) return [[0.0625, 0, 0.0625, 1, 0.875, 0.9375]];
        return [[0.0625, 0, 0.0625, 0.9375, 0.875, 0.9375]];
    }

    // BlockChorusPlant.getActualState (1.9+): an arm toward each chorus plant or flower beside, above or below (end stone
    // too below), around the 3/16-inset core.
    function chorusShapes(neighbour: Neighbour): Box[] {
        const joins = (dx: number, dy: number, dz: number, also = -1) => {
            const other = neighbour(dx, dy, dz);
            return !!other && (other.type === chorusPlant || other.type === chorusFlower || other.type === also);
        };
        const a = 0.1875;
        const b = 0.8125;
        const boxes: Box[] = [[a, a, a, b, b, b]];
        if (joins(-1, 0, 0)) boxes.push([0, a, a, a, b, b]);
        if (joins(1, 0, 0)) boxes.push([b, a, a, 1, b, b]);
        if (joins(0, 1, 0)) boxes.push([a, b, a, b, 1, b]);
        if (joins(0, -1, 0, endStone)) boxes.push([a, 0, a, b, a, b]);
        if (joins(0, 0, -1)) boxes.push([a, a, 0, b, b, a]);
        if (joins(0, 0, 1)) boxes.push([a, a, b, b, b, 1]);
        return boxes;
    }

    return {
        shapes(type, metadata, neighbour) {
            const family = families.get(type);
            if (family) return family.boxes[joinMask(family.joins, neighbour)];
            if (stairs.has(type)) return stairsShapes(metadata, neighbour);
            if (doors.has(type)) return doorShapes(type, metadata, neighbour);
            if (chests.has(type)) return chestShapes(type, neighbour);
            if (type === chorusPlant) return chorusShapes(neighbour);
            return null;
        },
        isNormalCube: (type, metadata) => normalCubes.has((type << 4) | metadata),
    };
}

/** A block as the legacy flow reads it: its id, data value and liquid. */
export interface LegacyFlowBlock extends LegacyBlock {
    fluid: { lava: boolean; level: number } | null;
}

const HORIZONTAL_FLOW: [number, number][] = [[0, -1], [1, 0], [0, 1], [-1, 0]]; // north, east, south, west

function normalizeFloat([x, y, z]: [number, number, number]): [number, number, number] {
    const length = Math.fround(Math.sqrt(x * x + y * y + z * z));
    return length < 1.0e-4 ? [0, 0, 0] : [x / length, y / length, z / length];
}

/**
 * BlockLiquid.getFlow before 1.13 (1.8.9 getFlowVector, 1.12.2 getFlow): toward each horizontal neighbour of the
 * liquid's material, the difference of their effective flow decays (getEffectiveFlowDecay: the level, 0 for falling
 * liquid); a neighbour of another material that does not block movement compares with the block under it, less 8.
 * Falling liquid beside a block solid to it (isBlockSolid, the material's isSolid; 1.12 causesDownwardCurrent, a solid
 * face) turns down by 6. Vec3d.normalize takes a float square root. `at(x, y, z)` gives the block there (null when
 * unknown).
 */
export function legacyFlowAt(
    majorVersion: string,
    at: (x: number, y: number, z: number) => LegacyFlowBlock | null,
    x: number,
    y: number,
    z: number,
    fluid: { lava: boolean; level: number },
): [number, number, number] {
    const data = RELEASES[majorVersion];
    if (!data) throw new Error(`no legacy flow data for ${majorVersion}`);
    const raw = (block: LegacyFlowBlock) => (block.type << 4) | block.metadata;
    const decay = (block: LegacyFlowBlock | null) => {
        if (!block?.fluid || block.fluid.lava !== fluid.lava) return -1;
        return block.fluid.level >= 8 ? 0 : block.fluid.level;
    };
    const blocksMovement = (block: LegacyFlowBlock | null) => block !== null && inRanges(data.flowBlocksMovement, raw(block));
    const solid = (block: LegacyFlowBlock | null) => block !== null && decay(block) < 0 && inRanges(data.flowSolid, raw(block));
    const own = fluid.level >= 8 ? 0 : fluid.level;
    let flowX = 0;
    let flowZ = 0;
    for (const [dx, dz] of HORIZONTAL_FLOW) {
        const neighbour = at(x + dx, y, z + dz);
        let level = decay(neighbour);
        let difference: number | null = null;
        if (level < 0) {
            if (!blocksMovement(neighbour)) {
                level = decay(at(x + dx, y - 1, z + dz));
                if (level >= 0) difference = level - (own - 8);
            }
        } else {
            difference = level - own;
        }
        if (difference !== null) {
            flowX += dx * difference;
            flowZ += dz * difference;
        }
    }
    let flow: [number, number, number] = [flowX, 0, flowZ];
    if (fluid.level >= 8) {
        for (const [dx, dz] of HORIZONTAL_FLOW) {
            if (solid(at(x + dx, y, z + dz)) || solid(at(x + dx, y + 1, z + dz))) {
                const [nx, ny, nz] = normalizeFloat(flow);
                flow = [nx, ny - 6, nz];
                break;
            }
        }
    }
    return normalizeFloat(flow);
}
