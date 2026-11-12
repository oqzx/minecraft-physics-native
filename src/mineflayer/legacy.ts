// The blocks before 1.13 whose collision boxes and liquid flow the client takes from their neighbours, which the data
// value minecraft-data's shapes go by does not carry. A port of mineflayer-physics-utils' LegacyShapes and of the
// parity harness's legacy-shapes.cjs and legacy-flow.cjs.
//
// legacy-connections.json (mineflayer-physics-utils' legacyConnections.json) lists per release the neighbour block
// states (id << 4 | data value) the fences, panes and walls join, with the boxes of each of the 16 ways to join, the
// normal cubes (EntityPlayerSP.pushOutOfBlocks), the states whose material blocks a liquid's flow and those solid
// beside falling liquid.
import connections from './legacy-connections.json';
import outlineNeighbours from './legacy-outline-neighbours.json';

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

/** What redstone wire and a vine read of their neighbours for their outline (scripts/generate-legacy-outline-neighbours.cjs). */
interface OutlineNeighbours {
    wire?: { connects: Record<Side, Range[]>; climbs: Record<Side, Range[]>; normal: Range[] };
    vineUp?: Range[];
}

const OUTLINE_NEIGHBOURS = outlineNeighbours as unknown as Record<string, OutlineNeighbours>;

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
    /** The box a ray meets of a chest in 1.8, whose collision is what its bounds were last set to. */
    chestOutline(type: number, neighbour: Neighbour): Box[] | null;
    /** The box a ray meets of redstone wire or a vine among its neighbours; null for any other block. */
    outline(type: number, metadata: number, neighbour: Neighbour): Box | null;
    /** How far from a block the blocks its shape depends on lie: 2 in 1.12, whose fences read the stairs' corners. */
    reach: number;
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
    const idOfName = (name: string) => registry.blocksByName[name]?.id ?? -1;
    const idOf = (name: string) => (modern ? idOfName(name) : -1);
    const chorusPlant = idOf('chorus_plant');
    const chorusFlower = idOf('chorus_flower');
    const endStone = idOf('end_stone');
    const snowLayer = registry.blocksByName.snow_layer?.id ?? -1;
    const flowerPot = idOfName('flower_pot');
    const doublePlant = idOfName('double_plant');

    // 1.12's fences, panes and walls join a face SOLID by getBlockFaceShape (BlockFence.canConnectTo, BlockPane
    // .attachesTo, BlockWall.canConnectTo): a stair's after its SHAPE, the others' as the data value gives them.
    const faceShapes = majorVersion === '1.12';

    function joinMask(joins: Record<Side, Set<number>>, neighbour: Neighbour) {
        let mask = 0;
        for (const [side, dx, dz, bit] of SIDES) {
            const other = neighbour(dx, 0, dz);
            if (!other) continue;
            const joined = faceShapes && stairs.has(other.type)
                ? stairsFaceSolid(other.metadata, (ox, oy, oz) => neighbour(dx + ox, oy, dz + oz), OPPOSITE[side])
                : joins[side].has((other.type << 4) | other.metadata);
            if (joined) mask |= bit;
        }
        return mask;
    }

    // BlockStairs.getActualState's SHAPE (getStairsShape): a stair of the same half ahead turned across makes an outer
    // corner, one behind an inner corner, unless the stair beside on that side continues this one.
    function stairsShape(metadata: number, neighbour: Neighbour) {
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
        return { facing, top, shape };
    }

    // BlockStairs.getBlockFaceShape (1.12) of a side face: SOLID at the full side of a straight stair, at both of an inner
    // corner's, at none of an outer corner's.
    function stairsFaceSolid(metadata: number, neighbour: Neighbour, face: Side): boolean {
        const { facing, shape } = stairsShape(metadata, neighbour);
        if (shape === 'straight') return facing === face;
        if (shape === 'inner_right') return facing === face || facing === CCW[face];
        if (shape === 'inner_left') return facing === face || facing === CW[face];
        return false;
    }

    // The stair's boxes: the half slab, the upper half (straight and inner), the quarter (corners)
    // (getCollisionBoxList).
    function stairsShapes(metadata: number, neighbour: Neighbour): Box[] {
        const { facing, top, shape } = stairsShape(metadata, neighbour);
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
            // (Block.getData of the state its data value gives: a flower pot's state keeps none, a tall plant's top half 8 and its default facing, north)
            const data = (other: LegacyBlock | null) => {
                if (!other) return 0;
                if (other.type === flowerPot) return 0;
                return other.type === doublePlant && other.metadata & 8 ? 10 : other.metadata;
            };
            const lower = metadata & 8 ? data(neighbour(0, -1, 0)) : metadata;
            const upper = metadata & 8 ? metadata : data(neighbour(0, 1, 0));
            facing = DOOR_FACING[lower & 3];
            open = (lower & 4) !== 0;
            right = (upper & 1) !== 0;
        } else if (metadata & 8) {
            right = (metadata & 1) !== 0;
            const below = neighbour(0, -1, 0);
            if (below && below.type === type) {
                // (the same door's half: an upper half holds no facing, and the id map's state for it, the last of its data
                // value registered, says east and closed: LegacyNeighbourDump)
                facing = below.metadata & 8 ? 'east' : DOOR_FACING[below.metadata & 3];
                open = below.metadata & 8 ? false : (below.metadata & 4) !== 0;
            }
        } else {
            facing = DOOR_FACING[metadata & 3];
            open = (metadata & 4) !== 0;
            const above = neighbour(0, 1, 0);
            // (likewise a lower half holds no hinge, and the id map's state says right)
            if (above && above.type === type) right = above.metadata & 8 ? (above.metadata & 1) !== 0 : true;
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

    const read = OUTLINE_NEIGHBOURS[majorVersion] ?? {};
    const rawOf = (block: LegacyBlock | null) => (block ? (block.type << 4) | block.metadata : 0);
    const wireId = idOfName('redstone_wire');
    const vineId = idOfName('vine');
    const wire = read.wire && {
        connects: Object.fromEntries(SIDES.map(([side]) => [side, expand(read.wire!.connects[side])])) as Record<Side, Set<number>>,
        climbs: Object.fromEntries(SIDES.map(([side]) => [side, expand(read.wire!.climbs[side])])) as Record<Side, Set<number>>,
        normal: expand(read.wire.normal),
    };
    const vineUp = expand(read.vineUp ?? []);

    // RedStoneWireBlock.getShape from 1.9: SHAPE_BY_INDEX by the sides getConnectingSide does not leave NONE, a side alone running
    // through to the other (1.12.2 RedStoneWireBlock.java:35-51, 113-139, 144-165; 1.9.4 the same). A side joins the state beside it,
    // or a wire below where that is no normal cube, or climbs to a wire above a top-solid state or glowstone where the block above
    // the wire is no normal cube; only wire answers shouldConnectTo with no side.
    function wireOutline(neighbour: Neighbour): Box {
        const tables = wire!;
        const isWire = (dx: number, dy: number, dz: number) => neighbour(dx, dy, dz)?.type === wireId;
        const above = rawOf(neighbour(0, 1, 0));
        const joins = (side: Side) => {
            const [dx, dz] = OFFSET[side];
            const beside = rawOf(neighbour(dx, 0, dz));
            if (tables.connects[side].has(beside)) return true;
            if (!tables.normal.has(beside) && isWire(dx, -1, dz)) return true;
            return !tables.normal.has(above) && tables.climbs[side].has(beside) && isWire(dx, 1, dz);
        };
        const [n, s, w, e] = [joins('north'), joins('south'), joins('west'), joins('east')];
        const north = n || (s && !e && !w);
        const east = e || (w && !n && !s);
        const south = s || (n && !e && !w);
        const west = w || (e && !n && !s);
        return [west ? 0 : 0.1875, 0, north ? 0 : 0.1875, east ? 1 : 0.8125, 0.0625, south ? 1 : 0.8125];
    }

    // A vine's sides are its data value: south 1, west 2, north 4, east 8. From 1.9 getShape takes the one box of its only side,
    // UP among them (the block above holds it), else the cube (1.12.2 VineBlock.java:56-87, 1.9.4 the same). 1.8 sets its bounds
    // around its sides, and a vine with none takes the top plate where it can hang from the block above (1.8.8
    // VineBlock.java:86-142): with neither, bounds the wrong way round.
    function vineOutline(metadata: number, neighbour: Neighbour): Box {
        const up = vineUp.has(rawOf(neighbour(0, 1, 0)));
        const south = (metadata & 1) !== 0;
        const west = (metadata & 2) !== 0;
        const north = (metadata & 4) !== 0;
        const east = (metadata & 8) !== 0;
        if (majorVersion === '1.8') {
            let [minX, minY, minZ, maxX, maxY, maxZ] = [1, 1, 1, 0, 0, 0];
            if (west) [maxX, minX, minY, maxY, minZ, maxZ] = [Math.max(maxX, 0.0625), 0, 0, 1, 0, 1];
            if (east) [minX, maxX, minY, maxY, minZ, maxZ] = [Math.min(minX, 0.9375), 1, 0, 1, 0, 1];
            if (north) [maxZ, minZ, minX, maxX, minY, maxY] = [Math.max(maxZ, 0.0625), 0, 0, 1, 0, 1];
            if (south) [minZ, maxZ, minX, maxX, minY, maxY] = [Math.min(minZ, 0.9375), 1, 0, 1, 0, 1];
            if (!west && !east && !north && !south && up) [minY, maxY, minX, maxX, minZ, maxZ] = [Math.min(minY, 0.9375), 1, 0, 1, 0, 1];
            return [minX, minY, minZ, maxX, maxY, maxZ];
        }
        const boxes: Box[] = [];
        if (up) boxes.push([0, 0.9375, 0, 1, 1, 1]);
        if (north) boxes.push([0, 0, 0, 1, 1, 0.0625]);
        if (east) boxes.push([0.9375, 0, 0, 1, 1, 1]);
        if (south) boxes.push([0, 0, 0.9375, 1, 1, 1]);
        if (west) boxes.push([0, 0, 0, 0.0625, 1, 1]);
        return boxes.length === 1 ? boxes[0]! : [0, 0, 0, 1, 1, 1];
    }

    return {
        shapes(type, metadata, neighbour) {
            const family = families.get(type);
            if (family) return family.boxes[joinMask(family.joins, neighbour)];
            if (stairs.has(type)) return stairsShapes(metadata, neighbour);
            if (doors.has(type)) return doorShapes(type, metadata, neighbour);
            if (chests.has(type)) return chestShapes(type, neighbour);
            if (type === chorusPlant) return chorusShapes(neighbour);
            // BlockSnow.getCollisionBoundingBox: (layers - 1) / 8 high, so one layer is a box of no height at the bottom
            // of the cell, which minecraft-data leaves out
            if (type === snowLayer && (metadata & 7) === 0) return [[0, 0, 0, 1, 0, 1]];
            return null;
        },
        chestOutline: (type, neighbour) => (!modern && (type === idOfName('chest') || type === idOfName('trapped_chest')) ? chestShapes(type, neighbour) : null),
        outline(type, metadata, neighbour) {
            if (type === wireId && wire) return wireOutline(neighbour);
            if (type === vineId && read.vineUp) return vineOutline(metadata, neighbour);
            return null;
        },
        isNormalCube: (type, metadata) => normalCubes.has((type << 4) | metadata),
        reach: faceShapes ? 2 : 1,
    };
}

/** A block as the legacy flow reads it: its id, data value and liquid. */
export interface LegacyFlowBlock extends LegacyBlock {
    fluid: { lava: boolean; level: number } | null;
}

const HORIZONTAL_FLOW: [number, number][] = [[0, -1], [1, 0], [0, 1], [-1, 0]]; // north, east, south, west

/** Vec3.normalize of its float sqrt: 1.8.8 Vec3.java:48-50. */
function normalizeFloat([x, y, z]: [number, number, number]): [number, number, number] {
    const length = Math.fround(Math.sqrt(x * x + y * y + z * z));
    return length < 1.0e-4 ? [0, 0, 0] : [x / length, y / length, z / length];
}

/**
 * BlockLiquid.getFlow before 1.13 (1.8.9 getFlowVector, 1.12.2 getFlow): toward each horizontal neighbour of the
 * liquid's material, the difference of their effective flow decays (getEffectiveFlowDecay: the level, 0 for falling
 * liquid); a neighbour of another material that does not block movement compares with the block under it, less 8.
 * Falling liquid beside a block solid to it (isBlockSolid, the material's isSolid; 1.12 causesDownwardCurrent, a solid
 * face) turns down by 6. Vec3.normalize takes a float square root. `at(x, y, z)` gives the block there (null when
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
