// Projectiles among a bot's entities: mineflayer's view of them as the engine's ordered candidates. The client does not
// know everything the server decides, so what only the server knows has a caller's say: `hurt`, what Entity.hurt answers,
// is true unless `targetOverrides` says otherwise, and NativeProjectileHitState holds the rest (a rocket's lifetime, a
// shulker bullet's next delta, whether the owner still holds its rod).
import type {
    NativeArrowHitTick,
    NativeArrowState,
    NativeDragonPart,
    NativeProjectileHitState,
    NativeProjectileHitTick,
    NativeProjectileOwner,
    NativeProjectileState,
    NativeProjectileTarget,
    NativeProjectileTickResult,
    NativeTridentHitTick,
    NativeTridentState,
    NativeVec3,
} from '../native';
import { NativeEngine } from './base';
import type { MoveRefusable, Vec3 } from './types';
import { syncReads, type BoxLike, type PrismarineWorld } from './world';

const f32 = Math.fround;

/** How far around a tick's path a sync reads. */
const PATH_MARGIN = 2;
const AT_REST: NativeVec3 = { x: 0, y: 0, z: 0 };

/** The box of a tick's path. */
function pathOf({ position: p, velocity: v }: { position: NativeVec3; velocity: NativeVec3 }): BoxLike {
    return {
        minX: Math.min(p.x, p.x + v.x), minY: Math.min(p.y, p.y + v.y), minZ: Math.min(p.z, p.z + v.z),
        maxX: Math.max(p.x, p.x + v.x), maxY: Math.max(p.y, p.y + v.y), maxZ: Math.max(p.z, p.z + v.z),
    };
}

/** A motion past what a sync reads drops, and nothing of the tick runs, as NativePhysics.simulate's: docs/numeric-contract.md. */
const refused = (state: { position: NativeVec3; velocity: NativeVec3 }) => !syncReads(pathOf(state), PATH_MARGIN);

/** AbstractHurtingProjectile.accelerationPower of a state with none: 0.1, and 0 for a wind charge (1.21.11 AbstractWindCharge.java:34-45). */
const defaultAccelerationPower = (kind: string | undefined) => (kind?.endsWith('wind_charge') ? 0 : 0.1);

/** What a tick of a refused motion leaves of a thrown item, fireball, rocket or hook. */
const flightOf = (state: NativeProjectileState): NativeProjectileTickResult => ({
    position: state.position, velocity: AT_REST, blockHit: false, inWater: !!state.inWater, firstTick: state.firstTick ?? true,
    horizontalCollision: !!state.horizontalCollision, onGround: !!state.onGround, bobbing: !!state.bobbing, discarded: false,
    ...(state.boundingBox ? { boundingBox: state.boundingBox } : {}),
    stuckSpeedMultiplier: state.stuckSpeedMultiplier ?? AT_REST,
});

/** What a tick of a refused motion leaves of an arrow or trident. */
const stuckOf = (state: NativeArrowState | NativeTridentState) => ({
    position: state.position, velocity: AT_REST, blockHit: false, inWater: !!state.inWater, inGround: !!state.inGround, discarded: false,
    life: state.life ?? 0, inGroundTime: state.inGroundTime ?? 0, shakeTime: state.shakeTime ?? 0, startedFalling: false,
    ...(state.lastBlockId === undefined ? {} : { lastBlockId: state.lastBlockId }),
    ...(state.lastBlockPosition ? { lastBlockPosition: state.lastBlockPosition } : {}),
});

/** What the adapter reads of a prismarine-entity. */
export interface TargetEntity {
    id: number;
    type?: string;
    name?: string;
    position: Vec3;
    width?: number;
    height?: number;
    metadata?: unknown[];
    vehicle?: TargetEntity | null;
    /** Attributes by name, as prismarine-entity keeps them. */
    attributes?: Record<string, { value?: number } | undefined>;
    isValid?: boolean;
}

/** Vanilla's say on one entity, by its id. */
export type TargetOverride = Partial<Omit<NativeProjectileTarget, 'id' | 'position'>>;

export interface ProjectileScene {
    /** The entities near the projectile, in the server's order where it matters (ties); the owner among them. */
    entities: readonly TargetEntity[];
    /** Projectile.getOwner: leave it out when the owner is gone, unloaded or was never known. */
    owner?: TargetEntity | null;
    /** The owner is alive; the entity's own isValid when absent, and true when that is unset too. */
    ownerAlive?: boolean;
    /** The owner is a spectating player. */
    ownerSpectator?: boolean;
    targetOverrides?: Readonly<Record<number, TargetOverride>>;
    /** The parts of each ender dragon among `entities`, by its id (NativeMovingEntityPhysics.tickDragon): what is hit of it. */
    dragonParts?: Readonly<Record<number, readonly NativeDragonPart[]>>;
}

/** Entity.pose in the synced data, from 1.14: the ordinals the engine's player poses share. */
const POSE_INDEX = 6;
const POSE_FALL_FLYING = 1;
const POSE_SLEEPING = 2;
const POSE_SWIMMING = 3;
const POSE_SPIN_ATTACK = 4;
const POSE_CROUCHING = 5;
const POSE_DYING = 7;

export class NativeProjectilePhysics extends NativeEngine {
    /** Player.getDimensions of the synced pose; other entities keep minecraft-data's size until they sleep or die. */
    private poseSize(entity: TargetEntity): { width: number; height: number } {
        const width = f32(entity.width ?? 0);
        const height = f32(entity.height ?? 0);
        if (this.lt('1.14')) return { width, height };
        const pose = entity.metadata?.[POSE_INDEX];
        if (pose === POSE_SLEEPING || pose === POSE_DYING) return { width: f32(0.2), height: f32(0.2) };
        if (entity.type !== 'player') return { width, height };
        if (pose === POSE_CROUCHING) return { width, height: f32(1.5) };
        if (pose === POSE_FALL_FLYING || pose === POSE_SWIMMING || pose === POSE_SPIN_ATTACK) return { width, height: f32(0.6) };
        return { width, height };
    }

    /** LivingEntity.getScale's SCALE attribute, from 1.20.5. An age's scale is not in mineflayer's entity: an override. */
    private scaleOf(entity: TargetEntity): number {
        if (this.lt('1.20.5')) return 1;
        const attributes = entity.attributes;
        return (attributes?.['minecraft:scale'] ?? attributes?.['scale'] ?? attributes?.['minecraft:generic.scale'])?.value ?? 1;
    }

    private rootOf(entity: TargetEntity): number {
        let root = entity;
        while (root.vehicle) root = root.vehicle;
        return root.id;
    }

    /** A candidate from a prismarine-entity. */
    targetOf(entity: TargetEntity, override: TargetOverride = {}): NativeProjectileTarget {
        const { width, height } = this.poseSize(entity);
        const { x, y, z } = entity.position;
        return {
            id: entity.id,
            rootVehicle: this.rootOf(entity),
            position: { x, y, z },
            width,
            height,
            scale: this.scaleOf(entity),
            enderman: entity.name === 'enderman',
            deflects: entity.name === 'breeze',
            alive: entity.isValid !== false,
            item: entity.name === 'item',
            windCharge: entity.name === 'wind_charge' || entity.name === 'breeze_wind_charge',
            endCrystal: entity.name === 'end_crystal',
            ...override,
        };
    }

    /** Entity.getEyePosition: a player's by its pose, a mob's at the 0.85 of its height of EntityDimensions. */
    private ownerOf(scene: ProjectileScene): NativeProjectileOwner | undefined {
        const owner = scene.owner;
        if (!owner) return undefined;
        const { width, height } = this.poseSize(owner);
        // EntityDimensions.scale: the eye and the width are the SCALE attribute's too
        const scale = f32(this.scaleOf(owner));
        const player = owner.type === 'player';
        const pose = owner.metadata?.[POSE_INDEX];
        const crouching = pose === POSE_CROUCHING && !this.lt('1.14');
        const low = height === f32(0.6);
        const eye = !player ? f32(height * f32(0.85)) : crouching ? f32(1.27) : low ? f32(0.4) : f32(1.62);
        const { x, y, z } = owner.position;
        return {
            id: owner.id,
            rootVehicle: this.rootOf(owner),
            eyePosition: { x, y: y + f32(eye * scale), z },
            width: f32(width * scale),
            alive: scene.ownerAlive ?? owner.isValid !== false,
            player,
            spectator: scene.ownerSpectator ?? false,
        };
    }

    /**
     * The candidates in Level.getEntities' order. A dragon is not pickable, its parts are; before 1.21.4 they follow the dragon
     * and the dragon's box has to meet the search, from then they follow every other entity, in the order of a hash map of
     * their ids here taken as the ids' own, which only an exact tie between parts of two dragons would show.
     */
    private targetsOf(scene: ProjectileScene): NativeProjectileTarget[] {
        const out: NativeProjectileTarget[] = [];
        const last: NativeProjectileTarget[] = [];
        const follow = this.lt('1.21.4');
        for (const entity of scene.entities) {
            const override = scene.targetOverrides?.[entity.id];
            const parts = scene.dragonParts?.[entity.id];
            if (!parts) {
                out.push(this.targetOf(entity, override));
                continue;
            }
            const dragon = this.targetOf(entity, { ...override, pickable: false });
            out.push(dragon);
            for (const part of parts) {
                const id = entity.id + part.idOffset;
                const target: NativeProjectileTarget = {
                    ...dragon, id, rootVehicle: id, position: part.position, boundingBox: part.boundingBox, pickable: true,
                    ...scene.targetOverrides?.[id],
                    ...(follow ? { dragonBox: { min: { x: entity.position.x - 8, y: entity.position.y, z: entity.position.z - 8 }, max: { x: entity.position.x + 8, y: entity.position.y + 8, z: entity.position.z + 8 } } } : {}),
                };
                (follow ? out : last).push(target);
            }
        }
        return [...out, ...last];
    }

    /** The world kept in step around the tick's path. */
    private syncAround(world: PrismarineWorld, state: { position: NativeVec3; velocity: NativeVec3 }) {
        const sync = this.syncOf(world);
        sync.sync(world, pathOf(state), PATH_MARGIN);
        return sync;
    }

    /**
     * One client tick of a thrown item, fireball, firework, fishing hook, llama spit or shulker bullet among the blocks: the state
     * for the next tick and what this one did. What a hit does to an entity is the server's, and an arrow or trident goes
     * through tickArrow or tickTrident, and the server's tick through tickProjectile. A kind its release does not have is refused,
     * and so is a hook before 1.11, whose client tick follows the server.
     */
    predictProjectile(world: PrismarineWorld, state: NativeProjectileState): { state: MoveRefusable<NativeProjectileState>; result: MoveRefusable<NativeProjectileTickResult> } {
        if (refused(state)) return { state: { ...state, velocity: AT_REST, moveRefused: true }, result: { ...flightOf(state), moveRefused: true } };
        const sync = this.syncAround(world, state);
        const [result] = this.binding.simulateVersionedProjectileTicks(this.version, sync.world, [state], 1);
        const { position, velocity, inWater, firstTick, horizontalCollision, onGround, bobbing, boundingBox } = result;
        // the box a release before 1.17 keeps goes on with the state, or the next tick starts from the seeded one
        return { state: { ...state, position, velocity, inWater, firstTick, horizontalCollision, onGround, bobbing, ...(boundingBox ? { boundingBox } : {}), moveRefused: false }, result };
    }

    /** One server tick of an arrow: pass the returned hitState to the next. */
    tickArrow(world: PrismarineWorld, state: NativeArrowState, hits: NativeProjectileHitState | undefined, scene: ProjectileScene): MoveRefusable<NativeArrowHitTick> {
        if (refused(state)) return { arrow: stuckOf(state), hitState: hits ?? {}, entityHits: [], moveRefused: true };
        const sync = this.syncAround(world, state);
        return this.binding.simulateVersionedArrowHitTick(this.version, sync.world, state, hits, this.targetsOf(scene), this.ownerOf(scene));
    }

    /**
     * One server tick of any other family, by its `kind`: pass the returned hitState to the next, and the returned
     * accelerationPower and power, which a deflection changes, in the state.
     */
    tickProjectile(world: PrismarineWorld, state: NativeProjectileState, hits: NativeProjectileHitState | undefined, scene: ProjectileScene): MoveRefusable<NativeProjectileHitTick> {
        if (refused(state)) {
            return {
                projectile: flightOf(state), hitState: hits ?? {}, entityHits: [],
                accelerationPower: state.accelerationPower ?? defaultAccelerationPower(state.kind), power: state.power ?? AT_REST, moveRefused: true,
            };
        }
        const sync = this.syncAround(world, state);
        return this.binding.simulateVersionedProjectileHitTick(this.version, sync.world, state, hits, this.targetsOf(scene), this.ownerOf(scene));
    }

    /** One server tick of a trident, from 1.13: pass the returned hitState to the next. */
    tickTrident(world: PrismarineWorld, state: NativeTridentState, hits: NativeProjectileHitState | undefined, scene: ProjectileScene): MoveRefusable<NativeTridentHitTick> {
        if (refused(state)) {
            return {
                trident: { ...stuckOf(state), dealtDamage: !!state.dealtDamage },
                noPhysics: !!state.noPhysics, hitState: hits ?? {}, entityHits: [], moveRefused: true,
            };
        }
        const sync = this.syncAround(world, state);
        return this.binding.simulateVersionedTridentHitTick(this.version, sync.world, state, hits, this.targetsOf(scene), this.ownerOf(scene));
    }
}
