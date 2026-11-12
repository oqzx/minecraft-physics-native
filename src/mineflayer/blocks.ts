// Prismarine block states as NativeWorld block updates: the collision boxes (minecraft-data's, through prismarine-block),
// the friction, the movement effect of the block, its fluid with vanilla's flow vector, the bounce restitution and
// whether it suffocates. A port of the parity harness's blocks.cjs, whose recorded worlds the engine reproduces exactly.
import type { NativeBlockUpdate, NativeGeneratedStateShape, NativeShapeChannel, NativeVec3 } from '../native';
import contextOutlines from './context-outlines.json';
import FLOW_BLOCKERS from './flow-blockers.json';
import { legacyFlowAt, legacyShapes, type Box, type LegacyFlowBlock, type LegacyShapes, type Neighbour } from './legacy';
import LEGACY_OUTLINES from './legacy-outlines.json';
import LEGACY_FULL_CUBES from './legacy-full-cubes.json';
import { resolveRegistryRelease } from './registry-release';

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
    fire: 16, soulFire: 17, bed: 18, hay: 19, pointedDripstoneTip: 20, glideThroughClimbable: 21, lilyPad: 22,
    snowLayer: 23, lavaCauldron: 24, layeredCauldron1: 25, layeredCauldron2: 26, layeredCauldron3: 27, sweetBerrySapling: 28,
    netherPortal: 29, endPortal: 30,
} as const;

/** Vanilla's climbable blocks: the #climbable tag from 1.16, hard-coded ladder, vine and scaffolding before it. */
const CLIMBABLE = new Set(['ladder', 'vine', 'weeping_vines', 'weeping_vines_plant', 'twisting_vines', 'twisting_vines_plant', 'cave_vines', 'cave_vines_plant']);
/** The climbable blocks of #can_glide_through, which a fall-flying player is not on from 1.21.11 (1.21.11
 * data/minecraft/tags/block/can_glide_through.json: the vines, #cave_vines). */
export const GLIDE_THROUGH = new Set(['vine', 'weeping_vines', 'weeping_vines_plant', 'twisting_vines', 'twisting_vines_plant', 'cave_vines', 'cave_vines_plant']);

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
        // age 0 slows and deals nothing (SweetBerryBushBlock.entityInside)
        case 'sweet_berry_bush': return props.age === '0' ? EFFECT.sweetBerrySapling : EFFECT.sweetBerryBush;
        case 'scaffolding': return EFFECT.scaffolding;
        case 'cactus': return EFFECT.cactus;
        case 'magma_block':
        case 'magma': return EFFECT.magma;
        case 'campfire': return props.lit === 'true' ? EFFECT.campfire : 0;
        case 'soul_campfire': return props.lit === 'true' ? EFFECT.soulCampfire : 0;
        // what takes a cart out of its level (`portal` before the flattening)
        case 'nether_portal':
        case 'portal': return EFFECT.netherPortal;
        case 'end_portal': return EFFECT.endPortal;
        case 'fire': return EFFECT.fire;
        case 'soul_fire': return EFFECT.soulFire;
        case 'bubble_column': return props.drag === 'true' ? EFFECT.bubbleDown : EFFECT.bubbleUp;
        // Block.fallOn's damage: the hay bale's fifth, an upward dripstone tip's double (PointedDripstoneBlock)
        case 'hay_block': return EFFECT.hay;
        case 'pointed_dripstone': return props.vertical_direction === 'up' && props.thickness === 'tip' ? EFFECT.pointedDripstoneTip : 0;
        // the bed bounce (BedBlock, and the shelf mushroom with the same restitution in 26.3)
        case 'shelf_mushroom':
        case 'bed': return EFFECT.bed;
        // the lily pad, out of a boat's ground friction and no box to a boat before 1.13 (LilyPadBlock), and 1.8's
        // snow layer, which a 1.8 boat clears (EntityBoat.onUpdate; "snow" is the snow block before the flattening)
        case 'lily_pad':
        case 'waterlily': return EFFECT.lilyPad;
        case 'snow_layer': return EFFECT.snowLayer;
        // the cauldrons whose content sets on fire or puts out (LavaCauldronBlock, LayeredCauldronBlock: the water and
        // powder snow cauldrons of a level 1 - 3, 1.17+)
        case 'lava_cauldron': return EFFECT.lavaCauldron;
        case 'water_cauldron':
        case 'powder_snow_cauldron': {
            const level = Number(props.level);
            return level >= 1 && level <= 3 ? EFFECT.layeredCauldron1 + level - 1 : 0;
        }
        default:
            if (name.endsWith('_bed') && name !== 'straw_bed') return EFFECT.bed;
            if (GLIDE_THROUGH.has(name)) return EFFECT.glideThroughClimbable;
            return CLIMBABLE.has(name) ? EFFECT.climbable : 0;
    }
}

// BlockState.isSuffocating where it is not "a full collision cube" (the engine's default): never for glass, leaves,
// grates and mangrove roots; always for farmland, soul sand, dirt paths and mud from 1.16.2 (1.16.5 Blocks.java:1877-1880
// `suffocates(Blocks::always)`, which 1.16 and 1.16.1 do not give them: 1.16 Blocks.java:480-481, 1863). Never for the
// chorus flower, a full collision cube until 1.20.3 whose material does not block movement: Material.PLANT in 1.13 -
// 1.19.4 under `blocksMovement() && isFullCube` (1.13.2 Block.java:349-351, 1543; 1.16.5 BlockBehaviour.java:796,
// Blocks.java:1857), notSolid from 1.20 under `state.blocksMovement() && ...` (1.20 BlockBehaviour.java:470-472, 935;
// Blocks.java:4741).
const NEVER_SUFFOCATING = /glass$|_leaves$|copper_grate$|^mangrove_roots$|^moving_piston$|^chorus_flower$/;
const ALWAYS_SUFFOCATING = new Set(['farmland', 'soul_sand', 'dirt_path', 'grass_path', 'mud']);
// 1.13's BlockState.causesSuffocation: a solid material and Block.isFullCube (1.13.2 Block.java:349-356), true but where
// a class says otherwise: soul sand, whose box is lower, is; the beacon, a full cube, is not (BeaconBlock)
const SUFFOCATING_1_13: Record<string, boolean> = { soul_sand: true, beacon: false };
// 1.13's pushOutOfBlocks tests IBlockState.isNormalCube instead (1.13.2 LocalPlayer.java:395-449, Player.java
// :189-195): an opaque material, Block.isFullCube and no redstone power (Block.java:278-280). Where that is not a full
// collision cube, from 1.13.2's Block.registerBlocks materials and the classes' isFullCube / canProvidePower: soul sand
// is a normal cube; these full cubes are not (a material that is not opaque: glass, ice, glowstone, the sea lantern,
// the beacon, tnt, leaves, the chorus flower; redstone power: the redstone block, the observer; isFullCube false: the
// piston base, the shulker boxes).
const NOT_NORMAL_CUBES_1_13 = /glass$|_leaves$|^(frosted_)?ice$|^glowstone$|^sea_lantern$|^beacon$|^tnt$|^chorus_flower$|^redstone_block$|^observer$|^(sticky_)?piston$|shulker_box$/;
// BlockState.isRedstoneConductor's never where the collision shape is a full cube (26.3 Blocks.java): those and the grates
const NOT_CONDUCTORS = new RegExp(`${NOT_NORMAL_CUBES_1_13.source}|copper_grate$`);
// Entity.isInsideWall's block test before 1.13, a solid material and renderAsNormalBlock (1.8.8 Block.isLeafBlock,
// Block.java:367-369), from 1.9 the default state's isFullCube (1.9.4 Block.java:333-335; 1.11.2 - 1.12.2
// causesSuffocation, Block.java:365-367): the full cubes whose class says no (GlassBlock, StainedGlassBlock,
// BeaconBlock, FarmlandBlock, PistonBlock; LeavesBlock.isLeafBlock), the chorus flower, whose Material.PLANT does not
// block movement (1.12.2 ChorusFlowerBlock.java:32, PlantMaterial.java:22-25; 1.9.4 ChorusFlowerBlock.java:28), and soul
// sand, whose class says nothing
const LEGACY_NOT_IN_WALL = new Set(['glass', 'stained_glass', 'beacon', 'farmland', 'leaves', 'leaves2', 'piston', 'sticky_piston', 'chorus_flower']);

/** The outlines no state decides alone, by release: scripts/generate-context-outlines.cjs. */
const CONTEXT_OUTLINES = contextOutlines as unknown as {
    releases: Record<string, number>;
    tables: Record<string, { max: number; shapes: Box[][]; states: number[] }>[];
};

/** The blocks of the names before 1.13 that join their neighbours in one box a ray meets (legacyJoinedOutline). */
const LEGACY_JOINED = /fence|_wall$|pane$|^iron_bars$|^chorus_plant$/;

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

export function horizontalOffset(x: number, z: number, maxOffset: number): [number, number] {
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

/** BlockState.tags, as native/src/minecart.rs reads them. */
const TAG_RAIL = 1;
const TAG_POWERED_RAIL = 2;
const TAG_ACTIVATOR_RAIL = 4;
const TAG_REDSTONE_CONDUCTOR = 8;
const RAIL_SHAPES = ['north_south', 'east_west', 'ascending_east', 'ascending_west', 'ascending_north', 'ascending_south', 'south_east', 'south_west', 'north_west', 'north_east'];

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
        // a copper grate's is a falling source (1.20.4 WaterloggedTransparentBlock.java:49 getStill(true), 26.3
        // WaterloggedTransparentBlock getSource(true)), which FlowingFluid.getFlow turns down beside a sturdy face
        fluid = { type: FLUID_WATERLOGGED, level: 0, source: true, falling: name.endsWith('copper_grate') };
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
 * the face) is the whole unit square, no more and no less (1.14.4 Block.java:408-415, 1.20.4 :244-247); `dy` -1 or 1 asks
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

const f32 = Math.fround;

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

export interface VersionCheck {
    '<'(version: string): boolean;
    '>='(version: string): boolean;
    majorVersion: string;
    minecraftVersion: string;
}

/** Who asks LiquidBlock.getCollisionShape, as the CollisionContext tells it. */
export interface LiquidCollisionContext {
    /** EntityCollisionContext.entityBottom. */
    readonly bottom: number;
    /** LivingEntity.canStandOnFluid: a strider's on lava, false for any other body and for no entity. */
    readonly canStandOn: (fluid: 'water' | 'lava') => boolean;
    /** CollisionGetter.getBlockAndLiquidCollisions, which only the server's spawn search asks (26.3 PlayerSpawnFinder.java:110). */
    readonly withFluidCollisions?: boolean;
}

const STABLE_TOP = 0.5;

/**
 * LiquidBlock.getCollisionShape for the fluid block at `y` under `above`, a box in the block's own coordinates or null. A block
 * that only holds water has no such shape: it answers with its own. An entity's way (1.16 LiquidBlock.java:36): the top of a
 * source, for a body above it that can stand on it, with no same fluid over it. The query with fluid collisions came with 1.21.2
 * and turned the capability on (CollisionContext.of(entity, true), 1.21.2 EntityCollisionContext.java:36-48); from 1.21.9 it is
 * the whole block before anything is asked, a flowing or falling fluid's too (1.21.9 LiquidBlock.java:69-71, 26.3 :61-63).
 */
export function liquidCollisionBox(
    block: DescribedBlock,
    above: DescribedBlock | null,
    y: number,
    context: LiquidCollisionContext,
    version: VersionCheck,
): Box | null {
    if (!(block.name in LIQUIDS) || block.fluid === null) return null;
    if (version['<']('1.16')) return null;
    if (context.withFluidCollisions) {
        if (version['<']('1.21.2')) throw new Error(`Minecraft ${version.minecraftVersion} has no query with fluid collisions`);
        if (version['>=']('1.21.9')) return [0, 0, 0, 1, 1, 1];
    }
    const fluid = block.fluid.type === FLUID_LAVA ? 'lava' : 'water';
    const stands = context.canStandOn(fluid) || (context.withFluidCollisions ?? false);
    const isAbove = context.bottom > y + STABLE_TOP - 9.999999747378752e-6;
    return isAbove && block.fluid.source && stands && !sameFluid(block.fluid, above?.fluid ?? null) ? [0, 0, 0, 1, STABLE_TOP, 1] : null;
}

/** The order of exact numeric release names, independent of registry comparison callbacks. */
export function compareReleases(left: string, right: string): number {
    const [a, b] = [left, right].map(release => release.split('.').map(Number));
    for (let i = 0; i < Math.max(a!.length, b!.length); i += 1) {
        const difference = (a![i] ?? 0) - (b![i] ?? 0);
        if (difference !== 0) return difference;
    }
    return 0;
}

/** A VersionCheck derived from the selected release's exact identity. */
export function releaseCheck(release: string): VersionCheck {
    return {
        '<': version => compareReleases(release, version) < 0,
        '>=': version => compareReleases(release, version) >= 0,
        majorVersion: release.split('.').slice(0, 2).join('.'),
        minecraftVersion: release,
    };
}

/** Whether a block (by name) is in a block tag of the release. */
export type BlockTagTest = (name: string, tag: string) => boolean;

/** ClipContext.Block.FALLDAMAGE_RESETTING's blocks a player clips as full cubes from 1.21.9 (ClipContext.java:66-69). */
const PLAYER_FALL_DAMAGE_RESETTING = new Set(['end_gateway', 'end_portal']);

/**
 * 1.13's FlowingFluid.isSolidFace takes none of Block.isExceptBlockForAttachWithPiston's blocks nor the stairs for solid
 * (1.13.2 FlowingFluid.java:131-143, Block.java:264-270): the shulker boxes, the leaves, the #trapdoors, the stained
 * glass, the beacon, the cauldron, the glass, the glowstone, the ice, the sea lantern, the conduit and the pistons; nor
 * the chorus flower, whose getBlockFaceShape is UNDEFINED (BlockChorusFlower.java:242-244).
 */
const NOT_SOLID_FACES_1_13 = /shulker_box$|_leaves$|_trapdoor$|_stained_glass$|^(beacon|cauldron|glass|glowstone|ice|sea_lantern|conduit|piston|sticky_piston|piston_head|chorus_flower)$|_stairs$/;

const FULL_CUBE: Box = [0, 0, 0, 1, 1, 1];
/**
 * 1.13.2's bed, a plain cuboid before its legs (BedBlock.java:41), and its cauldron, a tub with no feet (Block.java's
 * join in CauldronBlock), from the jar (scripts/java-oracle/LegacyShapeDump.java): minecraft-data's 1.13.2 has the shapes
 * of a later release for both.
 */
const BED_1_13: Box[] = [[0, 0, 0, 1, 0.5625, 1]];
const CAULDRON_1_13: Box[] = [[0,0,0,1,0.25,1],[0,0.25,0,0.125,1,1],[0.125,0.25,0,1,1,0.125],[0.125,0.25,0.875,1,1,1],[0.875,0.25,0.125,1,1,0.875]];
const EMPTY_BOX: Box = [0, 0, 0, 0, 0, 0];
const CHANNEL_INDEX: Record<NativeShapeChannel, number> = { collision: 0, outline: 1, interaction: 2, support: 3 };

/** The blocks BlockState.isAir answers true for: those of Settings.air() (1.16 Blocks.java:37, 2470-2471). */
const AIR_BLOCKS = new Set(['air', 'cave_air', 'void_air']);

/** The engine's tag bit (native/src/world.rs FENCE_LIKE): BlockTags.FENCES, BlockTags.WALLS and FenceGateBlock, which Entity.getLandingPos looks through to. */
const FENCE_LIKE = 16;
/** The engine's tag bit (native/src/world.rs WALL_OR_GATE): BlockTags.WALLS and FenceGateBlock, which Entity.getOnPos stops at for any offset, a fence only up to 0.5. */
const WALL_OR_GATE = 32;
/**
 * The engine's tag bits (native/src/world.rs LEGACY_PROBE_OUTLINE, LEGACY_PROBE_NONE): before 1.13 the getCollisionBoundingBox
 * auto-jump probes is the outline of a fence, brewing stand, end portal frame or end portal (Block.getCollisionShape returns
 * getShape, 1.12.2 Block.java:443-447; those override only appendCollisionBoxes), and none for the lily pad (BushBlock's).
 */
const LEGACY_PROBE_OUTLINE = 64;
const LEGACY_PROBE_NONE = 128;
const LEGACY_PROBE_OUTLINE_BLOCKS = new Set(['brewing_stand', 'end_portal_frame', 'end_portal']);
/**
 * The engine's tag bit (native/src/world.rs LEGACY_FULL_CUBE): World.isBlockFullCube before 1.13, the average edge of the
 * state's getCollisionBoundingBox at least 1 (1.12.2 World.java:1835-1838), for a state alone from each jar
 * (scripts/generate-legacy-shapes.cjs, legacy-full-cubes.json).
 */
const LEGACY_FULL_CUBE = 256;
const LEGACY_FULL_CUBE_STATES = LEGACY_FULL_CUBES as Record<string, Record<string, number[] | undefined> | undefined>;
/**
 * The blocks whose getCollisionBoundingBox reads their neighbours before 1.13 (getActualState): a fence's, a pane's and a
 * chorus plant's getBoundingBox, a wall's getCollisionBoundingBox override (its 1.5-high CLIP box), a chest beside another,
 * a gate's override (its 1.5-high box, none open): 1.12.2 FenceBlock.java:91, PaneBlock.java:95, WallBlock.java:106,
 * FenceGateBlock and ChestBlock getCollisionBox.
 */
const LEGACY_FULL_CUBE_BY_NEIGHBOURS = /(^|_)fence$|_wall$|fence_gate$|pane$|^iron_bars$|^chorus_plant$|chest$/;
const isWallOrGate = (name: string) => /(_wall|(^|_)fence_gate)$/.test(name);
const isFenceLike = (name: string) => /(^|_)fence$/.test(name) || isWallOrGate(name);

/** minecraft-data's collision shapes: a block's shape, or one for each of its states. */
interface CollisionTable {
    blocks: Record<string, number | number[] | undefined>;
    shapes: Record<number, number[][] | undefined>;
}

/** The blocks of one exact release as NativeWorld block updates; the registry must declare the same release. */
export class BlockMapper {
    private readonly version: VersionCheck;
    private readonly legacy: LegacyShapes | null;
    private readonly majorVersion: string;
    private readonly flattened: boolean;
    private readonly floatFlowLength: boolean;
    private readonly minimumFlowLength: number;
    private readonly trapdoorLadders: boolean;
    private readonly suffocatingSoils: boolean;
    /** getHeightForCamera reads the block above from 26.3 (26.3 FluidState.java:53-61). */
    private readonly tellsBottomFace: boolean;
    private readonly chainsWithoutAxis: boolean;
    private readonly suffocation1_13: boolean;
    private readonly collisionTable: CollisionTable | undefined;
    private readonly flowBlocks: FlowBlocks;
    /**
     * A block as the world takes it. Before the flattening prismarine-block picks a block's shape for its metadata from
     * the block's `variations`, which minecraft-data leaves out of most blocks before 1.12: every data value then has the
     * first one's shape (four layers of snow, none; a ladder facing east, facing north). The table has them all.
     *
     * The table's anvil goes by its facing, which 1.8 does not: its collision is Block.getCollisionBox, the bounds on
     * the block object (1.8.8 Block.java:327, AnvilBlock has no getCollisionBox), a full cube until the crosshair's ray
     * or the outline sets them for the state they meet, which a bot has neither of (AnvilBlock.java:85-91, Block.java:551,
     * LevelRenderer.java:1673). pa_anvil_* stop at the full cube on both axes of each facing; 1.9.4 goes by the facing
     * (AnvilBlock.java:102-105).
     */
    describe(block: PrismarineBlock): DescribedBlock {
        const described = describeBlock(block);
        // the release's own answer where it has one: minecraft-data serves 1.18 - 1.18.2 from 1.17's table
        const exact = this.exactChannel(described, 'collision');
        if (exact) return { ...described, boxes: exact };
        if (this.anvilHoldsBounds && block.name === 'anvil') return { ...described, boxes: [[0, 0, 0, 1, 1, 1]] };
        if (this.shapes1_13) {
            if (block.name.endsWith('_bed')) return { ...described, boxes: BED_1_13 };
            if (block.name === 'cauldron') return { ...described, boxes: CAULDRON_1_13 };
        }
        const ids = this.collisionTable?.blocks[block.name];
        if (!Array.isArray(ids)) return described;
        const shape = this.collisionTable!.shapes[ids[block.metadata] ?? ids[0]!];
        return shape ? { ...described, boxes: shape.map(box => box.slice(0, 6) as Box) } : described;
    }

    private readonly release: string;

    /** 1.8's anvil collides by the bounds on its block object, not by its state (describe). */
    private readonly anvilHoldsBounds: boolean;

    /** minecraft-data has the later shapes of 1.13's bed and cauldron (BED_1_13). */
    private readonly shapes1_13: boolean;

    /** The #fall_damage_resetting blocks by name (the tag exists from 1.18.2). */
    private readonly fallDamageResetting = new Map<string, boolean>();

    constructor(
        version: VersionCheck,
        registry: Parameters<typeof legacyShapes>[1] & { version: { minecraftVersion: string }; blockCollisionShapes?: CollisionTable; blocksByName?: Record<string, { resistance?: number | null } | undefined> },
        private readonly hasTag?: BlockTagTest,
        /** The generated outline boxes of a block, where the release has them. */
        private readonly outlines?: (name: string) => readonly { minX: number; minY: number; minZ: number; maxX: number; maxY: number; maxZ: number }[] | null,
        /** A channel of a state as the release's own code answers it (native generatedStateShape), where it has a table. */
        private readonly stateShape?: (name: string, index: number, kind: NativeShapeChannel) => NativeGeneratedStateShape | null | undefined,
    ) {
        const selected = resolveRegistryRelease(registry, resolveRegistryRelease({ version }));
        this.release = selected;
        this.version = version = releaseCheck(selected);
        this.flattened = version['>=']('1.13');
        this.collisionTable = this.flattened ? undefined : registry.blockCollisionShapes;
        this.anvilHoldsBounds = version['<']('1.9');
        this.shapes1_13 = version['>=']('1.13') && version['<']('1.14');
        this.majorVersion = version.majorVersion;
        this.legacy = this.flattened ? null : legacyShapes(version.majorVersion, registry);
        this.floatFlowLength = version['<']('1.17');
        // (Vec3.normalize: 1.21.1 Vec3.java:65 1.0E-4, 1.21.2 by Mth.EPSILON, Mth.java:26 1.0E-5F)
        this.minimumFlowLength = version['>=']('1.21.2') ? Math.fround(1.0e-5) : 1.0e-4;
        this.trapdoorLadders = version['>=']('1.9');
        this.motionBlocker = this.flowBlocker(version, hasTag, 'blocks_motion_in_heightmap');
        this.suffocatingSoils = version['>=']('1.16.2');
        this.tellsBottomFace = version['>=']('26.3');
        this.chainsWithoutAxis = version['>=']('1.16') && version['<']('1.16.2');
        this.suffocation1_13 = version['>=']('1.13') && version['<']('1.14');
        this.flowBlocks = this.flowBlocksOf(version, hasTag);
        const blocks = registry.blocksByName;
        this.explosionResistance = name => {
            const value = blocks?.[name]?.resistance;
            return typeof value === 'number' && Number.isFinite(value) && value >= 0 ? value : undefined;
        };
    }

    /** Block.getExplosionResistance of a block by name, from the release's registry. */
    private readonly explosionResistance: (name: string) => number | undefined;

    /**
     * FlowingFluid.getFlow's questions to the blocks. A block blocks the look below it by its Material.blocksMotion to
     * 1.19.4 (1.16.5 FlowingFluid.java:51-74, Material.java:80, 153-178; a wall sign's WOOD does with no collision, a
     * carpet's does not with one), BlockState.blocksMotion to 26.2: the forced solid flags, else the collision bounds of
     * size 0.7291666666666666 or a full height, but cobweb and bamboo sapling (1.20.4 FlowingFluid.java:70,
     * BlockBehaviour.java:472-493, 514-516; 26.2 BlockBehaviour.java:824-842, 880-883); and #blocks_fluid_flow from 26.3
     * (26.3 FlowingFluid.java:61, VanillaBlockTagsProvider.java:249). flow-blockers.json holds the sources' block lists
     * (scripts/generate-flow-blockers.cjs), each tree's until the next tree's. The face
     * is sturdy where the support shape's face is full (isFaceFull): 1.13 the collision faces (getBlockFaceShape's SOLID,
     * every face of soul sand, Block.java:394-396) but the exceptions of isSolidFace, 1.14 the collision shape: Block.isFaceSturdy
     * leaves out #leaves (1.14.4 Block.java:402-403), but BlockState answers from the cache initCache fills in Blocks' static block,
     * before any tag is loaded, so leaves are sturdy (1.14.4 BlockState.java:325-327, Blocks.java:1389). 1.15 the same shape but the
     * #leaves: MinecraftServer rebuilds the cache after the data packs load (1.15.2 MinecraftServer.java:1653, called on start and
     * on reload), which is the world a recording has, with the client in the server's JVM. A client alone on a dedicated server has
     * no such rebuild until 1.16's TagManager (1.16 TagManager.java:99), so there its own cache would still call them sturdy: not
     * recorded, and this follows the recorded world. From 1.16 getBlockSupportShape (1.16 Block.java:235-237): empty for
     * leaves, the layers' full shape for snow, the full cube for soul sand and (1.19+) mud, the 1..15 box for the chorus
     * flower from 1.20.3.
     */
    private readonly motionBlocker: FlowBlocks['blocksFlow'];

    private flowBlocksOf(version: VersionCheck, hasTag?: BlockTagTest): FlowBlocks {
        const blocksFlow = this.flowBlocker(version, hasTag);
        if (version['<']('1.14')) {
            return {
                blocksFlow,
                faceSturdy: (block, step) => !NOT_SOLID_FACES_1_13.test(block.name)
                    && (block.name === 'soul_sand' || faceFull(block.boxes, step)),
            };
        }
        // (1.14 reads the cache BlockState.initCache filled before the tags were loaded, so its #leaves test found nothing and leaves
        // are as sturdy as their shape: 1.14.4 BlockState.java:325-326, 412-418, and physrec/ff_oak_leaves_persistent_true on 1.14.4, 1.15.2)
        if (version['<']('1.15')) return { blocksFlow, faceSturdy: (block, step) => faceFull(block.boxes, step) };
        if (version['<']('1.16')) {
            return { blocksFlow, faceSturdy: (block, step) => !block.name.endsWith('_leaves') && faceFull(block.boxes, step) };
        }
        return { blocksFlow, faceSturdy: (block, step) => faceFull(this.supportShape(block), step) };
    }

    /**
     * Heightmap.Types.MOTION_BLOCKING's test: a fluid, or what blocks motion (the blocksMotion of flow-blockers.json, and
     * from 26.3 #blocks_motion_in_heightmap, 26.3 Heightmap.java:166). Before 1.13 Chunk's precipitation height asks the
     * Material (1.8.8 LevelChunk.java:897), which the data has not: a block with a collision stands for it there.
     */
    blocksPrecipitation(block: PrismarineBlock): boolean {
        const described = this.describe(block);
        return described.fluid !== null || this.motionBlocker(described);
    }

    private flowBlocker(version: VersionCheck, hasTag?: BlockTagTest, tag = 'blocks_fluid_flow'): FlowBlocks['blocksFlow'] {
        const release = version.minecraftVersion;
        const collides = (block: DescribedBlock | null) => block !== null && block.boxes.length > 0;
        if (release === undefined || compareReleases(release, '1.13') < 0) return collides;
        if (hasTag && compareReleases(release, '26.3') >= 0) {
            return block => block !== null && hasTag(block.name, tag);
        }
        const treeOf = <T>(trees: Record<string, T>): T | undefined => {
            const tree = Object.keys(trees).filter(name => compareReleases(name, release) <= 0).sort(compareReleases).pop();
            return tree === undefined ? undefined : trees[tree];
        };
        if (compareReleases(release, '1.20') < 0) {
            const open = new Set(treeOf(FLOW_BLOCKERS.material)?.open ?? []);
            return block => block !== null && !open.has(block.name);
        }
        const flags = treeOf(FLOW_BLOCKERS.solid);
        const solid = new Set(flags?.solid ?? []);
        const notSolid = new Set(flags?.notSolid ?? []);
        return block => {
            if (block === null || block.name === 'cobweb' || block.name === 'bamboo_sapling') return false;
            if (solid.has(block.name)) return true;
            if (notSolid.has(block.name) || block.boxes.length === 0) return false;
            const spans = [0, 1, 2].map(axis => {
                const lows = block.boxes.map(box => box[axis]!);
                const highs = block.boxes.map(box => box[axis + 3]!);
                return Math.max(...highs) - Math.min(...lows);
            });
            return (spans[0]! + spans[1]! + spans[2]!) / 3 >= 0.7291666666666666 || spans[1]! >= 1;
        };
    }

    /**
     * getBlockSupportShape from 1.16 (1.16 Block.java:235-237): empty for leaves, the layers' full shape for snow, the
     * full cube for soul sand and (1.19+) mud, the 1..15 box for the chorus flower from 1.20.3; the collision boxes else.
     */
    private supportShape(block: DescribedBlock): Box[] {
        const exact = this.exactChannel(block, 'support');
        if (exact) return exact;
        const { name, props, boxes } = block;
        if (name.endsWith('_leaves')) return [];
        if (name === 'snow') return [[0, 0, 0, 1, Number(props.layers ?? 1) * 2 / 16, 1]];
        if (name === 'soul_sand' || name === 'mud') return [FULL_CUBE];
        if (name === 'chorus_flower' && this.version['>=']('1.20.3')) return [[1 / 16, 0, 1 / 16, 15 / 16, 15 / 16, 15 / 16]];
        return boxes;
    }

    private readonly exactChannels = new Map<number, Box[] | null>();

    /**
     * A channel of a state where the state alone decides it, undefined where the release has no table or it also needs
     * the position, the level or an entity. One lookup per state and channel: a chunk asks for the same states over and over.
     */
    private exactChannel({ name, id, meta }: DescribedBlock, kind: NativeShapeChannel): Box[] | undefined {
        if (!this.stateShape) return undefined;
        const key = (id * 65536 + meta) * 4 + CHANNEL_INDEX[kind];
        let boxes = this.exactChannels.get(key);
        if (boxes === undefined) {
            const answer = this.stateShape(name, meta, kind);
            boxes = answer?.status === 'exact'
                ? answer.boxes.map(({ minX, minY, minZ, maxX, maxY, maxZ }): Box => [minX, minY, minZ, maxX, maxY, maxZ])
                : null;
            this.exactChannels.set(key, boxes);
        }
        return boxes ?? undefined;
    }

    /** The boxes of a state's outline where they are not its collision boxes, which the engine takes for none. */
    private outlineBoxes(block: DescribedBlock, boxes: Box[], x = 0, z = 0): Box[] | undefined {
        const outline = this.exactChannel(block, 'outline') ?? this.contextOutline(block, x, z);
        return outline && JSON.stringify(outline) !== JSON.stringify(boxes) ? outline : undefined;
    }

    /**
     * What a ray meets of a fence, pane, wall or closed gate before 1.13: not its collision boxes, whose top is 1.5 and which
     * come apart at a corner, but one box around them no higher than the block (1.12.2 Block.java:555 method_414 takes
     * getBoundingBox, FenceBlock's BOUNDING_BOXES and CollisionBoxes; a gate in a wall 0.8125, as outlineOf).
     */
    private legacyJoinedOutline(at: BlockAt, x: number, y: number, z: number, described: DescribedBlock, boxes: Box[]): Box[] | undefined {
        if (!this.legacy || boxes.length === 0) return undefined;
        const neighbour: Neighbour = (dx, dy, dz) => {
            const other = at(x + dx, y + dy, z + dz);
            return other && other.name !== 'air' ? { type: other.id, metadata: other.meta } : null;
        };
        const chest = this.legacy.chestOutline(described.id, neighbour);
        if (chest) return chest;
        if (!LEGACY_JOINED.test(described.name)) {
            return this.contextOutline(described, x, z);
        }
        const lowest = (axis: number) => Math.min(...boxes.map(box => box[axis]!));
        const highest = (axis: number) => Math.max(...boxes.map(box => box[axis]!));
        // (a wall that only runs through is lower: 1.12.2 WallBlock's AABB_BY_INDEX, 1.8.8 WallBlock.setBoundingBox)
        const through = /_wall$/.test(described.name) && (highest(3) - lowest(0) === 1) !== (highest(5) - lowest(2) === 1) && Math.min(highest(3) - lowest(0), highest(5) - lowest(2)) === 0.375;
        const low = this.majorVersion === '1.8' ? 0.8125 : 0.875;
        const top = this.gateInWall(at, x, y, z, described) ? 0.8125 : through ? low : 1;
        // (1.8's pane keeps the middle in its bounds, where its boxes stop at it: 1.8.8 CrossCollisionBlock.setBoundingBox)
        const pane = this.majorVersion === '1.8' && /pane$|^iron_bars$/.test(described.name);
        const near = (axis: number) => (pane ? Math.min(lowest(axis), 0.4375) : lowest(axis));
        const far = (axis: number) => (pane ? Math.max(highest(axis), 0.5625) : highest(axis));
        return [[near(0), lowest(1), near(2), far(3), Math.min(highest(4), top), far(5)]];
    }

    /**
     * The outline where the state alone does not decide it, so the shape table has no boxes for it: a plant's at its column's
     * offset, and scaffolding's and the light block's for whoever is not holding one (OracleContext.java, BlockBehaviour.getOffset).
     */
    private contextOutline({ name, meta }: DescribedBlock, x: number, z: number): Box[] | undefined {
        const entry = CONTEXT_OUTLINES.tables[CONTEXT_OUTLINES.releases[this.release] ?? -1]?.[name];
        const boxes = entry?.shapes[entry.states[meta] ?? -1];
        if (!entry || !boxes) return undefined;
        if (entry.max === 0) return boxes;
        const [ox, oz] = horizontalOffset(x, z, entry.max);
        return boxes.map(([minX, minY, minZ, maxX, maxY, maxZ]): Box => [minX + ox, minY, minZ + oz, maxX + ox, maxY, maxZ + oz]);
    }

    /**
     * The LEGACY_FULL_CUBE bit before 1.13 on each update of the cell. A state alone takes it from its jar's answer; a block
     * whose box reads its neighbours from the boxes sent for it here, the hull of its collision boxes for a wall or gate
     * (their override) and of its outline for the rest (getBoundingBox): what is approximated is that hull standing for
     * the one box, which holds for these shapes, and 1.8, whose box is the block's shared bounds as last set.
     */
    private withLegacyFullCube(described: DescribedBlock, updates: NativeBlockUpdate[]): NativeBlockUpdate[] {
        if (!this.legacy || updates.length === 0) return updates;
        const { name, meta } = described;
        let full: boolean;
        if (LEGACY_FULL_CUBE_BY_NEIGHBOURS.test(name)) {
            const ownBox = /_wall$|fence_gate$/.test(name);
            const boxes = updates.flatMap(update => {
                const box = ownBox ? update.shape : update.outlineShape ?? update.shape;
                return box && box.maxX > box.minX && box.maxY > box.minY && box.maxZ > box.minZ ? [box] : [];
            });
            if (boxes.length === 0 || updates.every(update => !update.shape || update.shape.maxY <= update.shape.minY)) return updates;
            const edge = (min: 'minX' | 'minY' | 'minZ', max: 'maxX' | 'maxY' | 'maxZ') =>
                Math.max(...boxes.map(box => box[max])) - Math.min(...boxes.map(box => box[min]));
            full = (edge('minX', 'maxX') + edge('minY', 'maxY') + edge('minZ', 'maxZ')) / 3 >= 1;
        } else {
            full = LEGACY_FULL_CUBE_STATES[this.release]?.[name]?.includes(meta) ?? false;
        }
        return full ? updates.map(update => ({ ...update, tags: (update.tags ?? 0) | LEGACY_FULL_CUBE })) : updates;
    }

    /** The LEGACY_PROBE_* bit of a block before 1.13, else 0. */
    private legacyProbe(name: string): number {
        if (!this.legacy) return 0;
        if (name === 'waterlily' || name === 'lily_pad') return LEGACY_PROBE_NONE;
        return (isFenceLike(name) && !isWallOrGate(name)) || LEGACY_PROBE_OUTLINE_BLOCKS.has(name) ? LEGACY_PROBE_OUTLINE : 0;
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
        const updates = this.withLegacyFullCube(described, this.shapeUpdatesAt(at, x, y, z, described));
        // BlockBehaviour.getInteractionShape, a channel of its own (a hopper's, a cauldron's): most blocks have none
        const interaction = this.exactChannel(described, 'interaction');
        if (!interaction?.length || !updates.length) return updates;
        const empty = { minX: 0, minY: 0, minZ: 0, maxX: 0, maxY: 0, maxZ: 0, slipperiness: updates[0]?.shape?.slipperiness ?? 0.6 };
        return Array.from({ length: Math.max(updates.length, interaction.length) }, (_, index) => {
            const [minX, minY, minZ, maxX, maxY, maxZ] = interaction[index] ?? EMPTY_BOX;
            return {
                ...(updates[index] ?? { ...updates[0], shape: updates[0]?.shape ? empty : undefined }),
                interactionShape: { ...empty, minX, minY, minZ, maxX, maxY, maxZ },
            };
        });
    }

    private shapeUpdatesAt(at: BlockAt, x: number, y: number, z: number, described: DescribedBlock): NativeBlockUpdate[] {
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
        // (a waterlogged block or a water plant does read it: 1.21.11 Entity.java:1026-1034)
        const waterFields = name === 'water' || name === 'flowing_water' ? { waterBlock: true } : {};
        // the properties a collision shape depends on (ScaffoldingBlock: distance and bottom)
        const rail = this.railFields(described);
        const propertyFields = name === 'scaffolding'
            ? { properties: Object.entries(described.props).map(([property, value]) => ({ name: property, value })) }
            : rail;
        const conductorFields = !rail.tags && this.redstoneConductor(described, boxes) ? { tags: TAG_REDSTONE_CONDUCTOR } : {};
        const suffocating = this.legacy
            ? this.legacy.isNormalCube(id, described.meta)
            : this.suffocation1_13 ? this.normalCube1_13(name) : this.suffocatingOverride(name);
        const suffocatingFields = suffocating === undefined ? {} : { suffocating };
        const inWall = this.legacy
            ? this.legacyInWall(name, described.meta, boxes)
            // 1.13's Entity.isInsideWall: causesSuffocation, which `suffocating` (isNormalCube) does not give
            : this.suffocation1_13 ? this.suffocatingOverride(name) ?? isFullCube(boxes) : undefined;
        const inWallFields = inWall === undefined ? {} : { inWall };
        const sturdyFields = this.tellsBottomFace && this.bottomFaceSturdy(described) !== faceFull(boxes, [0, 0], -1)
            ? { bottomFaceSturdy: !faceFull(boxes, [0, 0], -1) }
            : {};
        // Entity.move's FALLDAMAGE_RESETTING clip: the tagged blocks, and the end gateway and portal for a player (the
        // engine gates the clip by release and entity)
        const resetting = this.isFallDamageResetting(name);
        const playerResetting = PLAYER_FALL_DAMAGE_RESETTING.has(name);
        const resettingFields = {
            ...(isFenceLike(name) || this.legacyProbe(name) ? { tags: (isFenceLike(name) ? FENCE_LIKE | (isWallOrGate(name) ? WALL_OR_GATE : 0) : 0) | this.legacyProbe(name) } : {}),
            ...(resetting ? { fallDamageResetting: true } : {}),
            ...(playerResetting ? { playerFallDamageResetting: true } : {}),
        };
        // Block.getExplosionResistance, what a blast's rays lose in the block (minecraft-data's resistance is that
        // getter's value in every release: 1.8.8 Block.java:546 divides the stored one by 5); none for air
        const resistance = AIR_BLOCKS.has(name) ? undefined : this.explosionResistance(name);
        const resistanceFields = resistance === undefined ? {} : { explosionResistance: resistance };
        const exactOutline = this.outlineBoxes(described, boxes, x, z) ?? this.legacyJoinedOutline(at, x, y, z, described, boxes);
        // (the engine's clips filter empty boxes: the shorter list is padded with them)
        const outlineOf = (at: number) => {
            const [minX, minY, minZ, maxX, maxY, maxZ] = exactOutline?.[at] ?? EMPTY_BOX;
            return { outlineShape: { minX, minY, minZ, maxX, maxY, maxZ, slipperiness: friction, effect } };
        };
        const emptyShape = { minX: 0, minY: 0, minZ: 0, maxX: 0, maxY: 0, maxZ: 0, slipperiness: friction, effect };
        if (exactOutline) {
            const fields = boxes.length > 0
                ? { id, ...fluidFields, ...bounceFields, ...propertyFields, ...conductorFields, ...suffocatingFields, ...inWallFields, ...sturdyFields, ...resettingFields, ...resistanceFields }
                : { id, ...fluidFields, ...propertyFields, ...resettingFields, ...resistanceFields, ...waterFields };
            return Array.from({ length: Math.max(boxes.length, exactOutline.length) }, (_, at) => {
                const [minX, minY, minZ, maxX, maxY, maxZ] = boxes[at] ?? EMPTY_BOX;
                return { x, y, z, ...fields, shape: { ...emptyShape, minX, minY, minZ, maxX, maxY, maxZ }, ...outlineOf(at) };
            });
        }
        if (boxes.length > 0) {
            return boxes.map(([minX, minY, minZ, maxX, maxY, maxZ]) => ({
                x, y, z, id, ...fluidFields, ...bounceFields, ...propertyFields, ...conductorFields, ...suffocatingFields, ...inWallFields, ...sturdyFields, ...resettingFields, ...resistanceFields,
                shape: { minX, minY, minZ, maxX, maxY, maxZ, slipperiness: friction, effect },
            }));
        }
        if (effect || resetting || playerResetting || rail.tags) {
            const outline = this.legacy ? this.legacyNeighbourOutline(at, x, y, z, described) ?? this.outlineOf(name, described.meta, [x, z]) : undefined;
            return [{ ...(outline ? { outlineShape: { ...outline, slipperiness: friction, effect } } : {}), x, y, z, id, ...fluidFields, ...propertyFields, ...resettingFields, ...resistanceFields, shape: { minX: 0, minY: 0, minZ: 0, maxX: 0, maxY: 0, maxZ: 0, slipperiness: friction, effect } }];
        }
        // (an update with an id and no shape is a full block: a fluid alone goes without one)
        if (fluid) return [{ x, y, z, ...fluidFields, ...waterFields, ...resettingFields, ...resistanceFields }];
        // BlockState.isAir is the air blocks alone: a torch or a sign above a bubble column is not air to
        // BubbleColumnBlock.entityInside before 1.21.5 (1.21.4 BubbleColumnBlock.java:50-51), so it is sent with an
        // empty shape
        if (!AIR_BLOCKS.has(name)) {
            const outline = this.legacyNeighbourOutline(at, x, y, z, described) ?? this.outlineOf(name, described.meta, [x, z], this.gateInWall(at, x, y, z, described));
            return [{
                x, y, z, id, ...propertyFields, ...resettingFields, ...resistanceFields, shape: { minX: 0, minY: 0, minZ: 0, maxX: 0, maxY: 0, maxZ: 0, slipperiness: friction, effect },
                ...(outline ? { outlineShape: { ...outline, slipperiness: friction, effect } } : {}),
            }];
        }
        return [];
    }

    /**
     * A rail's kind (its tags) and the properties a cart reads. Before 1.13 they are its metadata: the shape in its low
     * bits (four for a plain rail, whose ten shapes need them; three for the others, whose 8 is `powered`).
     */
    private railFields({ name, props, meta }: DescribedBlock): { tags?: number; properties?: { name: string; value: string }[] } {
        const powered = name === 'powered_rail' || name === 'golden_rail';
        const activator = name === 'activator_rail';
        if (!powered && !activator && name !== 'rail' && name !== 'detector_rail') return {};
        const tags = TAG_RAIL | (powered ? TAG_POWERED_RAIL : 0) | (activator ? TAG_ACTIVATOR_RAIL : 0);
        const shape = props.shape ?? RAIL_SHAPES[name === 'rail' ? meta & 15 : meta & 7]!;
        const on = props.powered ?? String((meta & 8) !== 0);
        return { tags, properties: [{ name: 'shape', value: shape }, { name: 'powered', value: name === 'rail' ? 'false' : on }] };
    }

    /**
     * BlockState.isRedstoneConductor, which a powered rail asks of the block at its end: a full collision cube unless the
     * block says otherwise (26.3 BlockBehaviour.java, Blocks.java isRedstoneConductor; before 1.13 its isNormalCube, and
     * 1.8's Block.isFullCube is that too for every block a rail ends at).
     */
    private redstoneConductor({ name, id, meta }: DescribedBlock, boxes: Box[]): boolean {
        if (this.legacy) return this.legacy.isNormalCube(id, meta);
        if (name === 'soul_sand' || name === 'mud') return true;
        return isFullCube(boxes) && !NOT_CONDUCTORS.test(name);
    }

    /**
     * The outline of a block without a collider, which a thrown item, a llama's spit and the first fireballs clip. The
     * generated tables hold one for many blocks from 1.13; before, legacy-outlines.json, from the jars
     * (scripts/generate-legacy-outlines.cjs). Redstone wire's and a vine's there are those of the block alone: legacyNeighbourOutline
     * has them among their neighbours.
     */
    private outlineOf(name: string, meta?: number, cell?: readonly [number, number], inWall = false): { minX: number; minY: number; minZ: number; maxX: number; maxY: number; maxZ: number } | undefined {
        if (this.legacy) {
            const box = (LEGACY_OUTLINES as unknown as Record<string, Record<string, Record<string, number[]>>>)[this.majorVersion]?.[name]?.[meta ?? 0];
            if (!box) return undefined;
            const [minX, minY, minZ, maxX, maxY, maxZ, moves] = box as [number, number, number, number, number, number, number?];
            // Block.OffsetType.XZ from 1.11: the table is of the cell at the origin (1.11.2 FlowerBlock, 1.12.2 FlowerBlock$FlowerType.java:10)
            const [x0, z0] = moves ? horizontalOffset(0, 0, 0.25) : [0, 0];
            const [ox, oz] = moves ? horizontalOffset(cell?.[0] ?? 0, cell?.[1] ?? 0, 0.25) : [0, 0];
            // a gate between walls is lower, by its neighbours (1.12.2 FenceGateBlock.java:53-64, 42-47; none in 1.8.9, :66-73)
            return { minX: minX - x0 + ox, minY, minZ: minZ - z0 + oz, maxX: maxX - x0 + ox, maxY: inWall ? 0.8125 : maxY, maxZ: maxZ - z0 + oz };
        }
        const boxes = this.outlines?.(name);
        if (!boxes || boxes.length === 0) return undefined;
        return {
            minX: Math.min(...boxes.map(b => b.minX)), minY: Math.min(...boxes.map(b => b.minY)), minZ: Math.min(...boxes.map(b => b.minZ)),
            maxX: Math.max(...boxes.map(b => b.maxX)), maxY: Math.max(...boxes.map(b => b.maxY)), maxZ: Math.max(...boxes.map(b => b.maxZ)),
        };
    }

    /** The outline of redstone wire or a vine before 1.13 from the blocks around it (LegacyShapes.outline). */
    private legacyNeighbourOutline(at: BlockAt, x: number, y: number, z: number, described: DescribedBlock): { minX: number; minY: number; minZ: number; maxX: number; maxY: number; maxZ: number } | undefined {
        if (!this.legacy) return undefined;
        const neighbour: Neighbour = (dx, dy, dz) => {
            const other = at(x + dx, y + dy, z + dz);
            return other && other.name !== 'air' ? { type: other.id, metadata: other.meta } : null;
        };
        const box = this.legacy.outline(described.id, described.meta, neighbour);
        if (!box) return undefined;
        const [minX, minY, minZ, maxX, maxY, maxZ] = box;
        return { minX, minY, minZ, maxX, maxY, maxZ };
    }

    /** FenceGateBlock.getActualState's in_wall from 1.9: a wall beside it, on the sides across its way (1.12.2 FenceGateBlock.java:53-64). */
    private gateInWall(at: BlockAt, x: number, y: number, z: number, described: DescribedBlock): boolean {
        if (!this.legacy || this.majorVersion === '1.8' || !described.name.endsWith('fence_gate')) return false;
        // (facing's 2 bits: 1 west and 3 east have the x axis, whose sides are north and south)
        const sides = (described.meta & 1) !== 0 ? [[0, -1], [0, 1]] : [[-1, 0], [1, 0]];
        return sides.some(([dx, dz]) => /(^|_)wall$/.test(at(x + dx!, y, z + dz!)?.name ?? ''));
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
     * PistonBaseBlock.java:52-54, ShulkerBoxBlock.java:109-111); in 1.8 the end portal frame, whose class says nothing yet.
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
        // (the engine reads falling for a height a source already has, and refuses the two together: the flow holds the turn)
        return { fluid: fluid.type, fluidLevel: fluid.level, fluidSource: fluid.source, fluidFalling: fluid.falling && !fluid.source, fluidFlow };
    }
}
