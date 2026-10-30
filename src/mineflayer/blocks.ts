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
    fire: 16, soulFire: 17, bed: 18,
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
        // the bed bounce (BedBlock, and the shelf mushroom with the same restitution in 26.3)
        case 'shelf_mushroom':
        case 'bed': return EFFECT.bed;
        default:
            if (name.endsWith('_bed') && name !== 'straw_bed') return EFFECT.bed;
            return CLIMBABLE.has(name) ? EFFECT.climbable : 0;
    }
}

// BlockState.isSuffocating where it is not "a full collision cube" (the engine's default): never for glass, leaves,
// grates and mangrove roots; always for farmland, soul sand, dirt paths and mud from 1.16.
const NEVER_SUFFOCATING = /glass$|_leaves$|copper_grate$|^mangrove_roots$|^moving_piston$/;
const ALWAYS_SUFFOCATING = new Set(['farmland', 'soul_sand', 'dirt_path', 'grass_path', 'mud']);

/** The OffsetType.XZ blocks with a collision shape, by their maximum horizontal offset (BlockBehaviour 0.25F,
 * PointedDripstoneBlock and the sulfur spike 0.125F). minecraft-data's boxes carry the offset of the column at x 0, z 0. */
const OFFSET_BLOCKS: Record<string, number> = { pointed_dripstone: 0.125, bamboo: 0.25, sulfur_spike: 0.125 };

/** The XZ offset of a column: from Mth.getSeed(x, 0, z), each axis ((n / 15F) - 0.5) * 0.5 for a 4-bit n, clamped to
 * the block's maximum. */
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

/** Whether the face of a block toward `step` is covered by one of its boxes (BlockState.isFaceSturdy for full faces). */
function faceSturdy(described: DescribedBlock | null, [dx, dz]: [number, number]): boolean {
    if (!described) return false;
    return described.boxes.some(([minX, minY, minZ, maxX, maxY, maxZ]) =>
        minY <= 0 && maxY >= 1
        && (dx !== 0 ? (dx > 0 ? maxX >= 1 : minX <= 0) && minZ <= 0 && maxZ >= 1 : (dz > 0 ? maxZ >= 1 : minZ <= 0) && minX <= 0 && maxX >= 1));
}

export type BlockAt = (x: number, y: number, z: number) => DescribedBlock | null;

/** FlowingFluid.getFlow for the fluid at (x, y, z). Before 1.17 Vec3.normalize divides by a float length (Mth.sqrt). */
function flowAt(at: BlockAt, x: number, y: number, z: number, fluid: DescribedFluid, floatLength: boolean): [number, number, number] {
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
            if (!neighbour || neighbour.boxes.length === 0) {
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
        return length < 1.0e-4 ? [0, 0, 0] : [a / length, b / length, c / length];
    };
    if (fluid.falling) {
        for (const step of HORIZONTAL) {
            const side = at(x + step[0], y, z + step[1]);
            const sideAbove = at(x + step[0], y + 1, z + step[1]);
            // FlowingFluid.isSolidFace: never the fluid's own kind or ice
            const solid = (block: DescribedBlock | null) => block !== null && !(block.fluid && sameFluid(fluid, block.fluid))
                && block.name !== 'ice' && block.name !== 'frosted_ice' && faceSturdy(block, step);
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
}

/** The blocks of one release as NativeWorld block updates. */
export class BlockMapper {
    private readonly legacy: LegacyShapes | null;
    private readonly flattened: boolean;
    private readonly floatFlowLength: boolean;
    private readonly trapdoorLadders: boolean;
    private readonly suffocatingSoils: boolean;

    constructor(private readonly version: VersionCheck, registry: Parameters<typeof legacyShapes>[1]) {
        this.flattened = version['>=']('1.13');
        this.legacy = this.flattened ? null : legacyShapes(version.majorVersion, registry);
        this.floatFlowLength = version['<']('1.17');
        this.trapdoorLadders = version['>=']('1.9');
        this.suffocatingSoils = version['>=']('1.16');
    }

    /**
     * The block updates of the block at (x, y, z): one per collision box (a block that carries a movement effect but
     * no box gets an empty one; a fluid alone none), from the blocks around it (`at`: null where unknown).
     */
    updatesAt(at: BlockAt, x: number, y: number, z: number): NativeBlockUpdate[] {
        const described = at(x, y, z);
        if (!described) return [];
        const { id, friction, fluid, name, bounceRestitution } = described;
        const boxes = this.boxesAt(at, x, y, z, described);
        const effect = this.trapdoorLadder(described, at(x, y - 1, z)) ? EFFECT.climbable : described.effect;
        const fluidFields = fluid ? this.fluidUpdate(at, x, y, z, fluid) : {};
        const bounceFields = bounceRestitution ? { bounceRestitution } : {};
        // the properties a collision shape depends on (ScaffoldingBlock: distance and bottom)
        const propertyFields = name === 'scaffolding'
            ? { properties: Object.entries(described.props).map(([property, value]) => ({ name: property, value })) }
            : {};
        const suffocating = this.legacy ? this.legacy.isNormalCube(id, described.meta) : this.suffocatingOverride(name);
        const suffocatingFields = suffocating === undefined ? {} : { suffocating };
        if (boxes.length > 0) {
            return boxes.map(([minX, minY, minZ, maxX, maxY, maxZ]) => ({
                x, y, z, id, ...fluidFields, ...bounceFields, ...propertyFields, ...suffocatingFields,
                shape: { minX, minY, minZ, maxX, maxY, maxZ, slipperiness: friction, effect },
            }));
        }
        if (effect) {
            return [{ x, y, z, id, ...fluidFields, shape: { minX: 0, minY: 0, minZ: 0, maxX: 0, maxY: 0, maxZ: 0, slipperiness: friction, effect } }];
        }
        // (an update with an id and no shape is a full block: a fluid alone goes without one)
        if (fluid) return [{ x, y, z, ...fluidFields }];
        return [];
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

    private suffocatingOverride(name: string): boolean | undefined {
        if (NEVER_SUFFOCATING.test(name)) return false;
        if (ALWAYS_SUFFOCATING.has(name) && this.suffocatingSoils) return true;
        return undefined;
    }

    private fluidUpdate(at: BlockAt, x: number, y: number, z: number, fluid: DescribedFluid): Partial<NativeBlockUpdate> {
        let flow: [number, number, number];
        if (this.flattened) {
            flow = flowAt(at, x, y, z, fluid, this.floatFlowLength);
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
