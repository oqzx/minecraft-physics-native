// Prismarine block states as NativeWorld block updates: the collision boxes (minecraft-data's, through prismarine-block),
// the friction, the movement effect of the block, its fluid with vanilla's flow vector, the bounce restitution and
// whether it suffocates. A port of the parity harness's blocks.cjs, whose recorded worlds the engine reproduces exactly.
import type { NativeBlockUpdate, NativeVec3 } from '../native';
import { legacyFlowAt, legacyShapes, type Box, type LegacyFlowBlock, type LegacyShapes, type Neighbour } from './legacy';

/** What the adapter reads of a prismarine block. */
export interface PrismarineBlock {
    type: number;
    name: string;
    metadata: number;
    stateId?: number;
    shapes: number[][];
    getProperties(): Record<string, unknown>;
}

/** Block.friction in vanilla's Blocks registry; every other block keeps the default 0.6. (Before the flattening the
 * slime block is "slime".) */
const FRICTION: Record<string, number> = { ice: 0.98, packed_ice: 0.98, frosted_ice: 0.98, blue_ice: 0.989, slime_block: 0.8, slime: 0.8 };

/** The native BlockEffect codes (native/src/napi_api.rs block_effect). */
export const EFFECT = {
    soulSand: 1, honey: 2, slime: 3, web: 4, climbable: 5, bubbleUp: 6, bubbleDown: 7, powderSnow: 8,
    sweetBerryBush: 9, soulSoil: 10, scaffolding: 11, cactus: 12, magma: 13, campfire: 14, soulCampfire: 15,
    fire: 16, soulFire: 17, bed: 18, hay: 19, pointedDripstoneTip: 20,
} as const;

/** Vanilla's climbable blocks: the #climbable tag from 1.16, hard-coded ladder, vine and scaffolding before it. */
const CLIMBABLE = new Set(['ladder', 'vine', 'weeping_vines', 'weeping_vines_plant', 'twisting_vines', 'twisting_vines_plant', 'cave_vines', 'cave_vines_plant']);

// (the names before the flattening: slime, web, magma, bed)
function effectOf(name: string, props: Record<string, string>): number {
    switch (name) {
        case 'soul_sand': return EFFECT.soulSand;
        case 'soul_soil': return EFFECT.soulSoil;
        case 'honey_block': return EFFECT.honey;
        case 'slime_block':
        case 'slime': return EFFECT.slime;
        case 'cobweb':
        case 'web': return EFFECT.web;
        case 'powder_snow': return EFFECT.powderSnow;
        case 'sweet_berry_bush': return EFFECT.sweetBerryBush;
        case 'scaffolding': return EFFECT.scaffolding;
        case 'cactus': return EFFECT.cactus;
        case 'magma_block':
        case 'magma': return EFFECT.magma;
        case 'campfire': return props.lit === 'true' ? EFFECT.campfire : 0;
        case 'soul_campfire': return props.lit === 'true' ? EFFECT.soulCampfire : 0;
        case 'fire': return EFFECT.fire;
        case 'soul_fire': return EFFECT.soulFire;
        case 'bubble_column': return props.drag === 'true' ? EFFECT.bubbleDown : EFFECT.bubbleUp;
        // Block.fallOn's damage: the hay bale's fifth, an upward dripstone tip's double (PointedDripstoneBlock)
        case 'hay_block': return EFFECT.hay;
        case 'pointed_dripstone': return props.vertical_direction === 'up' && props.thickness === 'tip' ? EFFECT.pointedDripstoneTip : 0;
        // the bed bounce (BedBlock, and the shelf mushroom with the same restitution in 26.3)
        case 'shelf_mushroom':
        case 'bed': return EFFECT.bed;
        default:
            if (name.endsWith('_bed') && name !== 'straw_bed') return EFFECT.bed;
            return CLIMBABLE.has(name) ? EFFECT.climbable : 0;
    }
}

// BlockState.isSuffocating where it is not "a full collision cube" (the engine's default): never for glass, leaves,
// grates and mangrove roots; always for farmland, soul sand, dirt paths and mud from 1.16.2 (1.16.5 Blocks.java:1843-1846
// `suffocates(Blocks::always)`, which 1.16 and 1.16.1 do not give them: 1.16 Blocks.java:503-504, 1833).
const NEVER_SUFFOCATING = /glass$|_leaves$|copper_grate$|^mangrove_roots$|^moving_piston$/;
const ALWAYS_SUFFOCATING = new Set(['farmland', 'soul_sand', 'dirt_path', 'grass_path', 'mud']);
// 1.13's BlockState.causesSuffocation: a solid material and Block.isFullCube (1.13.2 Block.java:362-369), true but where
// a class says otherwise: soul sand, whose box is lower, is; the beacon, a full cube, is not (BeaconBlock)
const SUFFOCATING_1_13: Record<string, boolean> = { soul_sand: true, beacon: false };
// 1.13's pushOutOfBlocks tests IBlockState.isNormalCube instead (1.13.2 EntityPlayerSP.java:386-444, EntityPlayer.java
// :1454-1460): an opaque material, Block.isFullCube and no redstone power (Block.java:278-280). Where that is not a full
// collision cube, from 1.13.2's Block.registerBlocks materials and the classes' isFullCube / canProvidePower: soul sand
// is a normal cube; these full cubes are not (a material that is not opaque: glass, ice, glowstone, the sea lantern,
// the beacon, tnt, leaves, the chorus flower; redstone power: the redstone block, the observer; isFullCube false: the
// piston base, the shulker boxes).
const NOT_NORMAL_CUBES_1_13 = /glass$|_leaves$|^(frosted_)?ice$|^glowstone$|^sea_lantern$|^beacon$|^tnt$|^chorus_flower$|^redstone_block$|^observer$|^(sticky_)?piston$|shulker_box$/;
// Entity.isInsideWall's block test before 1.13, a solid material and renderAsNormalBlock (1.8.9 Block.isLeafBlock,
// Block.java:330-332), from 1.9 the default state's isFullCube (1.9.4 Block.java:297-299; 1.11.2 - 1.12.2
// causesSuffocation, Block.java:323-325): the full cubes whose class says no (GlassBlock, StainedGlassBlock,
// BeaconBlock, FarmlandBlock, PistonBlock; LeavesBlock.isLeafBlock), and soul sand, whose class says nothing
const LEGACY_NOT_IN_WALL = new Set(['glass', 'stained_glass', 'beacon', 'farmland', 'leaves', 'leaves2', 'piston', 'sticky_piston']);

/** The OffsetType.XZ blocks with a collision shape, by their maximum horizontal offset (BlockBehaviour 0.25F,
 * PointedDripstoneBlock and the sulfur spike 0.125F). minecraft-data's boxes carry the offset of the column at x 0, z 0. */
const OFFSET_BLOCKS: Record<string, number> = { pointed_dripstone: 0.125, bamboo: 0.25, sulfur_spike: 0.125 };

// 1.16 and 1.16.1: minecraft-data gives the chain and the lanterns the boxes of 1.16.2's states (an axis, a waterlogged),
// so prismarine-block picks another state's. The chain stands upright (ChainBlock.SHAPE, 6.5 - 9.5) and a lantern
// stands or hangs (LanternBlock.STANDING_SHAPE, HANGING_SHAPE).
const CHAIN_1_16: Box[] = [[0.40625, 0, 0.40625, 0.59375, 1, 0.59375]];
const LANTERN_STANDING: Box[] = [[0.3125, 0, 0.3125, 0.6875, 0.4375, 0.6875], [0.375, 0.4375, 0.375, 0.625, 0.5625, 0.625]];
const LANTERN_HANGING: Box[] = [[0.3125, 0.0625, 0.3125, 0.6875, 0.5, 0.6875], [0.375, 0.5, 0.375, 0.625, 0.625, 0.625]];
function boxes1_16(described: DescribedBlock): Box[] | undefined {
    if (described.name === 'chain') return CHAIN_1_16;
    if (described.name === 'lantern' || described.name === 'soul_lantern') return described.props.hanging === 'true' ? LANTERN_HANGING : LANTERN_STANDING;
    return undefined;
}

/** The XZ offset of a column: from Mth.getSeed(x, 0, z), each axis ((n / 15F) - 0.5) * 0.5 for a 4-bit n, clamped to
 * the block's maximum. */
/** One box, the whole cell: the engine's default for `suffocating` and `inWall`. */
function isFullCube(boxes: Box[]): boolean {
    return boxes.length === 1 && boxes[0].every((value, i) => value === (i < 3 ? 0 : 1));
}

function horizontalOffset(x: number, z: number, maxOffset: number): [number, number] {
    const big = BigInt;
    let seed = big.asIntN(32, big(x) * big(3129871)) ^ (big(z) * big(116129781));
    seed = big.asIntN(64, seed * seed * big(42317861) + seed * big(11)) >> big(16);
    const axis = (bits: bigint) => Math.min(Math.max((Math.fround(Number(bits & big(15)) / 15) - 0.5) * 0.5, -maxOffset), maxOffset);
    return [axis(seed), axis(seed >> big(8))];
}

/** Blocks whose fluid state is always a water source. */
const WATER_PLANTS = new Set(['water', 'bubble_column', 'kelp', 'kelp_plant', 'seagrass', 'tall_seagrass']);

export const FLUID_WATER = 1;
export const FLUID_LAVA = 2;
export const FLUID_WATERLOGGED = 3;

/** The liquid blocks; before the flattening the still and the flowing liquid are two blocks, the level their data value. */
const LIQUIDS: Record<string, number> = { water: FLUID_WATER, flowing_water: FLUID_WATER, lava: FLUID_LAVA, flowing_lava: FLUID_LAVA };

export interface DescribedFluid {
    type: number;
    level: number;
    source: boolean;
    falling: boolean;
}

/** A block state as the engine needs it. */
export interface DescribedBlock {
    name: string;
    props: Record<string, string>;
    id: number;
    meta: number;
    boxes: Box[];
    friction: number;
    effect: number;
    bounceRestitution: number;
    fluid: DescribedFluid | null;
}

export function describeBlock(block: PrismarineBlock): DescribedBlock {
    const props: Record<string, string> = {};
    for (const [key, value] of Object.entries(block.getProperties())) props[key] = String(value);
    const name = block.name;
    let fluid: DescribedFluid | null = null;
    if (name in LIQUIDS) {
        const level = Number(props.level ?? block.metadata);
        fluid = { type: LIQUIDS[name], level, source: level === 0, falling: level >= 8 };
    } else if (WATER_PLANTS.has(name)) {
        fluid = { type: FLUID_WATER, level: 0, source: true, falling: false };
    } else if (props.waterlogged === 'true') {
        fluid = { type: FLUID_WATERLOGGED, level: 0, source: true, falling: false };
    }
    const effect = effectOf(name, props);
    // Block.getBounceRestitution (26.2+): the slime block and the beds
    const bounceRestitution = effect === EFFECT.slime ? 1 : effect === EFFECT.bed ? 0.75 : 0;
    return {
        name,
        props,
        id: block.type,
        meta: block.metadata,
        boxes: block.shapes.map(shape => shape.slice(0, 6) as Box),
        friction: FRICTION[name] ?? 0.6,
        effect,
        bounceRestitution,
        fluid,
    };
}

const HORIZONTAL: [number, number][] = [[0, -1], [0, 1], [-1, 0], [1, 0]]; // north, south, west, east

/** FlowingFluid.getOwnHeight: amount / 9 as a float; a source or falling fluid has amount 8. */
function ownHeight(fluid: DescribedFluid | null): number {
    if (!fluid) return 0;
    const amount = fluid.source || fluid.falling ? 8 : 8 - fluid.level;
    return Math.fround(amount / 9);
}

const sameFluid = (a: DescribedFluid, b: DescribedFluid | null) => b !== null && (a.type === FLUID_LAVA) === (b.type === FLUID_LAVA);

/**
 * Block.isFaceFull: the slice of the shape just inside its face toward `step` (VoxelShape.getFaceShape, 1.0E-7 in from
 * the face) is the whole unit square, no more and no less (1.14 Block.java:411-418, 1.20.4 :245-248); `dy` -1 or 1 asks
 * for the bottom or the top face.
 */
function faceFull(boxes: Box[], [dx, dz]: [number, number], dy = 0): boolean {
    const axis = dy !== 0 ? 1 : dx !== 0 ? 0 : 2;
    const positive = (dy || dx || dz) > 0;
    const [u, v] = axis === 0 ? [1, 2] : axis === 1 ? [0, 2] : [0, 1];
    const slice = boxes.filter(box => (positive
        ? box[axis] <= 1 - 1.0e-7 && box[axis + 3] > 1 - 1.0e-7
        : box[axis] <= 1.0e-7 && box[axis + 3] > 1.0e-7));
    if (slice.length === 0) return false;
    if (slice.some(box => box[u] < 0 || box[u + 3] > 1 || box[v] < 0 || box[v + 3] > 1)) return false;
    const cuts = (at: number) => [...new Set([0, 1, ...slice.flatMap(box => [box[at], box[at + 3]])])].sort((a, b) => a - b);
    const us = cuts(u);
    const vs = cuts(v);
    for (let i = 0; i + 1 < us.length; i += 1) {
        const mu = (us[i] + us[i + 1]) / 2;
        for (let j = 0; j + 1 < vs.length; j += 1) {
            const mv = (vs[j] + vs[j + 1]) / 2;
            if (!slice.some(box => box[u] <= mu && mu <= box[u + 3] && box[v] <= mv && mv <= box[v + 3])) return false;
        }
    }
    return true;
}

export type BlockAt = (x: number, y: number, z: number) => DescribedBlock | null;

/** What FlowingFluid.getFlow asks of the blocks around the fluid, as the release answers it. */
interface FlowBlocks {
    /** Whether the fluid looks no further down past a block with no fluid of its own (blocksMotion, #blocks_fluid_flow). */
    blocksFlow(block: DescribedBlock | null): boolean;
    /** BlockState.isFaceSturdy toward `step`, which a falling fluid's -6 turn reads. */
    faceSturdy(block: DescribedBlock, step: [number, number]): boolean;
}

/**
 * FlowingFluid.getFlow for the fluid at (x, y, z). Before 1.17 Vec3.normalize divides by a float length (Mth.sqrt); a
 * vector shorter than `minimumLength` (1.0E-4, 1.0E-5F from 1.21.2: 1.21.1 Vec3.java:63-65, 1.21.2 :71-73) is zero.
 */
function flowAt(at: BlockAt, x: number, y: number, z: number, fluid: DescribedFluid, floatLength: boolean, minimumLength: number, blocks: FlowBlocks): [number, number, number] {
    const own = ownHeight(fluid);
    let dx = 0;
    let dz = 0;
    for (const [sx, sz] of HORIZONTAL) {
        const neighbour = at(x + sx, y, z + sz);
        const neighbourFluid = neighbour?.fluid ?? null;
        if (neighbourFluid && !sameFluid(fluid, neighbourFluid)) continue;
        let height = ownHeight(neighbourFluid);
        let delta = 0;
        if (height === 0) {
            if (!blocks.blocksFlow(neighbour)) {
                const belowFluid = at(x + sx, y - 1, z + sz)?.fluid ?? null;
                if (belowFluid && sameFluid(fluid, belowFluid)) {
                    height = ownHeight(belowFluid);
                    if (height > 0) delta = Math.fround(own - Math.fround(height - Math.fround(0.8888889)));
                }
            }
        } else if (height > 0) {
            delta = Math.fround(own - height);
        }
        if (delta !== 0) {
            dx += sx * delta;
            dz += sz * delta;
        }
    }
    let flow: [number, number, number] = [dx, 0, dz];
    const normalize = ([a, b, c]: [number, number, number]): [number, number, number] => {
        const exact = Math.sqrt(a * a + b * b + c * c);
        const length = floatLength ? Math.fround(exact) : exact;
        return length < minimumLength ? [0, 0, 0] : [a / length, b / length, c / length];
    };
    if (fluid.falling) {
        for (const step of HORIZONTAL) {
            const side = at(x + step[0], y, z + step[1]);
            const sideAbove = at(x + step[0], y + 1, z + step[1]);
            // FlowingFluid.isSolidFace: never the fluid's own kind or ice
            const solid = (block: DescribedBlock | null) => block !== null && !(block.fluid && sameFluid(fluid, block.fluid))
                && block.name !== 'ice' && block.name !== 'frosted_ice' && blocks.faceSturdy(block, step);
            if (solid(side) || solid(sideAbove)) {
                const [a, b, c] = normalize(flow);
                flow = [a, b - 6, c];
                break;
            }
        }
    }
    return normalize(flow);
}

interface VersionCheck {
    '<'(version: string): boolean;
    '>='(version: string): boolean;
    majorVersion: string;
    minecraftVersion?: string;
}

/** Whether a block (by name) is in a block tag of the release. */
export type BlockTagTest = (name: string, tag: string) => boolean;

/** ClipContext.Block.FALLDAMAGE_RESETTING's blocks a player clips as full cubes from 1.21.9 (ClipContext.java:65-68). */
const PLAYER_FALL_DAMAGE_RESETTING = new Set(['end_gateway', 'end_portal']);

/**
 * 1.13's FlowingFluid.isSolidFace takes none of Block.isExceptBlockForAttachWithPiston's blocks nor the stairs for solid
 * (1.13.2 FlowingFluid.java:131-143, Block.java:264-270): the shulker boxes, the leaves, the #trapdoors, the stained
 * glass, the beacon, the cauldron, the glass, the glowstone, the ice, the sea lantern, the conduit and the pistons; nor
 * the chorus flower, whose getBlockFaceShape is UNDEFINED (BlockChorusFlower.java:242-244).
 */
const NOT_SOLID_FACES_1_13 = /shulker_box$|_leaves$|_trapdoor$|_stained_glass$|^(beacon|cauldron|glass|glowstone|ice|sea_lantern|conduit|piston|sticky_piston|piston_head|chorus_flower)$|_stairs$/;

const FULL_CUBE: Box = [0, 0, 0, 1, 1, 1];

/** The blocks BlockState.isAir answers true for: those of Settings.air() (1.16 Blocks.java:31, 2398-2399). */
const AIR_BLOCKS = new Set(['air', 'cave_air', 'void_air']);

/** The blocks of one release as NativeWorld block updates. */
export class BlockMapper {
    private readonly legacy: LegacyShapes | null;
    private readonly flattened: boolean;
    private readonly floatFlowLength: boolean;
    private readonly minimumFlowLength: number;
    private readonly trapdoorLadders: boolean;
    private readonly suffocatingSoils: boolean;
    private readonly chainsWithoutAxis: boolean;
    private readonly suffocation1_13: boolean;
    private readonly flowBlocks: FlowBlocks;
    /** The #fall_damage_resetting blocks by name (the tag exists from 1.18.2). */
    private readonly fallDamageResetting = new Map<string, boolean>();

    constructor(
        private readonly version: VersionCheck,
        registry: Parameters<typeof legacyShapes>[1],
        private readonly hasTag?: BlockTagTest,
    ) {
        this.flattened = version['>=']('1.13');
        this.legacy = this.flattened ? null : legacyShapes(version.majorVersion, registry);
        this.floatFlowLength = version['<']('1.17');
        this.minimumFlowLength = version['>=']('1.21.2') ? Math.fround(1.0e-5) : 1.0e-4;
        this.trapdoorLadders = version['>=']('1.9');
        this.suffocatingSoils = version['>=']('1.16.2');
        this.chainsWithoutAxis = version['>=']('1.16') && version['<']('1.16.2');
        this.suffocation1_13 = version['>=']('1.13') && version['<']('1.14');
        this.flowBlocks = this.flowBlocksOf(version);
    }

    /**
     * FlowingFluid.getFlow's questions to the blocks. A block blocks the look below it by blocksMotion
     * (Material.blocksMotion to 1.19, BlockState.blocksMotion to 26.2, the #blocks_fluid_flow tag from 26.3; 1.16.4
     * FlowingFluid.java:62, 1.20.4 :70, 26.3 :61), which the adapter takes for having a collision box. The face
     * is sturdy where the support shape's face is full (isFaceFull): 1.13 the collision faces (getBlockFaceShape's SOLID,
     * every face of soul sand, Block.java:394-396) but the exceptions of isSolidFace, 1.14 - 1.15 the collision shape of
     * all but the #leaves (1.14 Block.java:407-409), from 1.16 getBlockSupportShape (1.16 Block.java:232-234): empty for
     * leaves, the layers' full shape for snow, the full cube for soul sand and (1.19+) mud, the 1..15 box for the chorus
     * flower from 1.20.3.
     */
    private flowBlocksOf(version: VersionCheck): FlowBlocks {
        // (the generated 26.3 #blocks_fluid_flow has no members: its #blocks_motion is empty in the generated data)
        const blocksFlow = (block: DescribedBlock | null) => block !== null && block.boxes.length > 0;
        if (version['<']('1.14')) {
            return {
                blocksFlow,
                faceSturdy: (block, step) => !NOT_SOLID_FACES_1_13.test(block.name)
                    && (block.name === 'soul_sand' || faceFull(block.boxes, step)),
            };
        }
        if (version['<']('1.16')) {
            return { blocksFlow, faceSturdy: (block, step) => !block.name.endsWith('_leaves') && faceFull(block.boxes, step) };
        }
        return { blocksFlow, faceSturdy: (block, step) => faceFull(this.supportShape(block), step) };
    }

    /**
     * getBlockSupportShape from 1.16 (1.16 Block.java:232-234): empty for leaves, the layers' full shape for snow, the
     * full cube for soul sand and (1.19+) mud, the 1..15 box for the chorus flower from 1.20.3; the collision boxes else.
     */
    private supportShape({ name, props, boxes }: DescribedBlock): Box[] {
        if (name.endsWith('_leaves')) return [];
        if (name === 'snow') return [[0, 0, 0, 1, Number(props.layers ?? 1) * 2 / 16, 1]];
        if (name === 'soul_sand' || name === 'mud') return [FULL_CUBE];
        if (name === 'chorus_flower' && this.version['>=']('1.20.3')) return [[1 / 16, 0, 1 / 16, 15 / 16, 15 / 16, 15 / 16]];
        return boxes;
    }

    /** BlockState.isFaceSturdy(DOWN) from 1.16: the bottom face of the support shape is full. */
    bottomFaceSturdy(block: DescribedBlock): boolean {
        return faceFull(this.supportShape(block), [0, 0], -1);
    }

    /** How far from a block the blocks its update is made from lie (2 for 1.12's fences beside stairs). */
    get reach(): number {
        return this.legacy?.reach ?? 1;
    }

    /**
     * The block updates of the block at (x, y, z): one per collision box (any other block but air gets an empty one; a
     * fluid alone none), from the blocks around it (`at`: null where unknown).
     */
    updatesAt(at: BlockAt, x: number, y: number, z: number): NativeBlockUpdate[] {
        const described = at(x, y, z);
        if (!described) return [];
        const { id, friction, fluid, name, bounceRestitution } = described;
        const boxes = this.boxesAt(at, x, y, z, described);
        const effect = this.trapdoorLadder(described, at(x, y - 1, z)) ? EFFECT.climbable : described.effect;
        const fluidFields = fluid ? this.fluidUpdate(at, x, y, z, fluid) : {};
        // the honey block in #suppresses_bounce (26.2+, 26.3 VanillaBlockTagsProvider.java:224; the engine reads it
        // from 26.2 alone)
        const bounceFields = {
            ...(bounceRestitution ? { bounceRestitution } : {}),
            ...(name === 'honey_block' ? { suppressesBounce: true } : {}),
        };
        // Blocks.WATER itself, whose speed factor Entity.getBlockSpeedFactor keeps without reading the block below
        // (a waterlogged block or a water plant does read it: 1.21.11 Entity.java:996-1004)
        const waterFields = name === 'water' || name === 'flowing_water' ? { waterBlock: true } : {};
        // the properties a collision shape depends on (ScaffoldingBlock: distance and bottom)
        const propertyFields = name === 'scaffolding'
            ? { properties: Object.entries(described.props).map(([property, value]) => ({ name: property, value })) }
            : {};
        const suffocating = this.legacy
            ? this.legacy.isNormalCube(id, described.meta)
            : this.suffocation1_13 ? this.normalCube1_13(name) : this.suffocatingOverride(name);
        const suffocatingFields = suffocating === undefined ? {} : { suffocating };
        const inWall = this.legacy
            ? this.legacyInWall(name, described.meta, boxes)
            // 1.13's Entity.isInsideWall: causesSuffocation, which `suffocating` (isNormalCube) does not give
            : this.suffocation1_13 ? this.suffocatingOverride(name) ?? isFullCube(boxes) : undefined;
        const inWallFields = inWall === undefined ? {} : { inWall };
        // Entity.move's FALLDAMAGE_RESETTING clip: the tagged blocks, and the end gateway and portal for a player (the
        // engine gates the clip by release and entity)
        const resetting = this.isFallDamageResetting(name);
        const playerResetting = PLAYER_FALL_DAMAGE_RESETTING.has(name);
        const resettingFields = {
            ...(resetting ? { fallDamageResetting: true } : {}),
            ...(playerResetting ? { playerFallDamageResetting: true } : {}),
        };
        if (boxes.length > 0) {
            return boxes.map(([minX, minY, minZ, maxX, maxY, maxZ]) => ({
                x, y, z, id, ...fluidFields, ...bounceFields, ...propertyFields, ...suffocatingFields, ...inWallFields, ...resettingFields,
                shape: { minX, minY, minZ, maxX, maxY, maxZ, slipperiness: friction, effect },
            }));
        }
        if (effect || resetting || playerResetting) {
            return [{ x, y, z, id, ...fluidFields, ...resettingFields, shape: { minX: 0, minY: 0, minZ: 0, maxX: 0, maxY: 0, maxZ: 0, slipperiness: friction, effect } }];
        }
        // (an update with an id and no shape is a full block: a fluid alone goes without one)
        if (fluid) return [{ x, y, z, ...fluidFields, ...waterFields }];
        // BlockState.isAir is the air blocks alone: a torch or a sign above a bubble column is not air to
        // BubbleColumnBlock.entityInside before 1.21.5 (1.21.4 BubbleColumnBlock.java:48-49), so it is sent with an
        // empty shape
        if (!AIR_BLOCKS.has(name)) {
            return [{ x, y, z, id, shape: { minX: 0, minY: 0, minZ: 0, maxX: 0, maxY: 0, maxZ: 0, slipperiness: friction, effect } }];
        }
        return [];
    }

    private isFallDamageResetting(name: string): boolean {
        if (!this.hasTag) return false;
        let resetting = this.fallDamageResetting.get(name);
        if (resetting === undefined) {
            resetting = this.hasTag(name, 'fall_damage_resetting');
            this.fallDamageResetting.set(name, resetting);
        }
        return resetting;
    }

    /** The block's collision boxes in its cell, from the blocks around it. */
    boxesAt(at: BlockAt, x: number, y: number, z: number, described: DescribedBlock): Box[] {
        if (this.legacy) {
            const neighbour: Neighbour = (dx, dy, dz) => {
                const other = at(x + dx, y + dy, z + dz);
                return other && other.name !== 'air' ? { type: other.id, metadata: other.meta } : null;
            };
            return this.legacy.shapes(described.id, described.meta, neighbour) ?? described.boxes;
        }
        if (this.chainsWithoutAxis) {
            const boxes = boxes1_16(described);
            if (boxes) return boxes;
        }
        const maxOffset = OFFSET_BLOCKS[described.name];
        if (maxOffset === undefined) return described.boxes;
        const [x0, z0] = horizontalOffset(0, 0, maxOffset);
        const [ox, oz] = horizontalOffset(x, z, maxOffset);
        return described.boxes.map(([minX, minY, minZ, maxX, maxY, maxZ]): Box =>
            [minX - x0 + ox, minY, minZ - z0 + oz, maxX - x0 + ox, maxY, maxZ - z0 + oz]);
    }

    /** LivingEntity.trapdoorUsableAsLadder (1.9+): an open trapdoor over a ladder facing the same way climbs like one. */
    private trapdoorLadder(described: DescribedBlock, below: DescribedBlock | null): boolean {
        return this.trapdoorLadders && described.name.endsWith('trapdoor') && described.props.open === 'true'
            && below?.name === 'ladder' && below.props.facing === described.props.facing;
    }

    /**
     * Entity.isInsideWall's test of a block before 1.13 (LEGACY_NOT_IN_WALL): one with a full collision cube but those,
     * and soul sand; from 1.11 the piston base while retracted and the shulker box (their causesSuffocation, 1.12.2
     * PistonBlock.java:48-50, ShulkerBoxBlock.java:109-111); in 1.8 the end portal frame, whose class says nothing yet.
     */
    private legacyInWall(name: string, metadata: number, boxes: Box[]): boolean {
        const causesSuffocation = this.version['>=']('1.11');
        if ((name === 'piston' || name === 'sticky_piston') && causesSuffocation) return (metadata & 8) === 0;
        if (name.endsWith('_shulker_box')) return true;
        if (LEGACY_NOT_IN_WALL.has(name)) return false;
        if (name === 'soul_sand' || (name === 'end_portal_frame' && this.version['<']('1.9'))) return true;
        return isFullCube(boxes);
    }

    /** 1.13's isNormalCube where it is not a full collision cube (NOT_NORMAL_CUBES_1_13). */
    private normalCube1_13(name: string): boolean | undefined {
        if (name === 'soul_sand') return true;
        return NOT_NORMAL_CUBES_1_13.test(name) ? false : undefined;
    }

    private suffocatingOverride(name: string): boolean | undefined {
        if (this.suffocation1_13 && name in SUFFOCATING_1_13) return SUFFOCATING_1_13[name];
        if (NEVER_SUFFOCATING.test(name)) return false;
        if (ALWAYS_SUFFOCATING.has(name) && this.suffocatingSoils) return true;
        return undefined;
    }

    private fluidUpdate(at: BlockAt, x: number, y: number, z: number, fluid: DescribedFluid): Partial<NativeBlockUpdate> {
        let flow: [number, number, number];
        if (this.flattened) {
            flow = flowAt(at, x, y, z, fluid, this.floatFlowLength, this.minimumFlowLength, this.flowBlocks);
        } else {
            const legacyAt = (bx: number, by: number, bz: number): LegacyFlowBlock | null => {
                const other = at(bx, by, bz);
                if (!other) return null;
                const otherFluid = other.fluid ? { lava: other.fluid.type === FLUID_LAVA, level: other.fluid.level } : null;
                return { type: other.id, metadata: other.meta, fluid: otherFluid };
            };
            flow = legacyFlowAt(this.version.majorVersion, legacyAt, x, y, z, { lava: fluid.type === FLUID_LAVA, level: fluid.level });
        }
        const fluidFlow: NativeVec3 = { x: flow[0], y: flow[1], z: flow[2] };
        return { fluid: fluid.type, fluidLevel: fluid.level, fluidSource: fluid.source, fluidFalling: fluid.falling, fluidFlow };
    }
}
