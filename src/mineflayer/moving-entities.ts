// The entities a client ticks without a brain, for a bot that wants to know where they will be: primed TNT, falling blocks,
// dropped items and experience orbs. Each state starts from the entity as mineflayer has it, ticks in the native engine and
// takes the server's word again whenever a packet gives it. This is prediction: nothing here places a block, sets off an
// explosion, merges a stack or collects anything. The server does that, and says so in the block, explosion, and entity
// removal packets mineflayer already follows; a state whose entity went is dropped by the caller.
import type {
    NativeDragonPart,
    NativeDragonPush,
    NativeDragonPushTarget,
    NativeDragonState,
    NativeDragonTickResult,
    NativeMobTravelState,
    NativeMovingEntityInputs,
    NativeMovingEntityState,
    NativeMovingEntityTickResult,
    NativeVec3,
} from '../native';
import { NativeEngine } from './base';
import type { MoveRefusable } from './types';
import { bodyReach, syncReads, type PrismarineWorld } from './world';

/** The fields of a prismarine-entity the adapter reads. */
export interface MovingEntityLike {
    id: number;
    name?: string;
    position: NativeVec3;
    velocity: NativeVec3;
    onGround?: boolean;
}

/** What mineflayer's entity does not say: from the spawn packet and the entity's metadata. */
export interface MovingEntityExtras {
    /** PrimedTnt's fuse (its metadata). */
    fuse?: number;
    /** The carried block's id (the spawn packet's object data). */
    carriedBlock?: number;
    /** The carried block's name, for what the block's name decides. */
    carriedName?: string;
    noGravity?: boolean;
}

/** The names the releases have given the four entities. */
const KINDS: Readonly<Record<string, NativeMovingEntityState['kind']>> = {
    tnt: 'tnt', primed_tnt: 'tnt', PrimedTnt: 'tnt',
    falling_block: 'falling_block', FallingSand: 'falling_block', falling_sand: 'falling_block',
    item: 'item', Item: 'item',
    experience_orb: 'experience_orb', xp_orb: 'experience_orb', XPOrb: 'experience_orb',
};

/** The engine's kind of an entity, undefined for one that is not ticked here. */
export function movingEntityKind(name: string | undefined): NativeMovingEntityState['kind'] | undefined {
    return name === undefined ? undefined : KINDS[name.replace(/^minecraft:/, '')];
}

const vec = ({ x, y, z }: NativeVec3): NativeVec3 => ({ x, y, z });

/** The moving entities of a world as a client predicts them. */
export class NativeMovingEntityPhysics extends NativeEngine {
    private readonly states = new Map<number, NativeMovingEntityState>();

    /** The state of an entity from now on, as mineflayer has it. Undefined for an entity that is not ticked here. */
    track(entity: MovingEntityLike, extras: MovingEntityExtras = {}): NativeMovingEntityState | undefined {
        const kind = movingEntityKind(entity.name);
        if (!kind) return undefined;
        const state: NativeMovingEntityState = {
            kind,
            authority: 'client',
            id: entity.id,
            position: vec(entity.position),
            velocity: vec(entity.velocity),
            onGround: !!entity.onGround,
            ...(extras.noGravity === undefined ? {} : { noGravity: extras.noGravity }),
            ...(extras.fuse === undefined ? {} : { fuse: extras.fuse }),
            ...(extras.carriedBlock === undefined ? {} : { carriedBlock: extras.carriedBlock }),
            ...(extras.carriedName === undefined
                ? {}
                : { carriedAir: extras.carriedName === 'air', concretePowder: extras.carriedName.endsWith('concrete_powder') }),
        };
        this.states.set(entity.id, state);
        return state;
    }

    /** The server moved the entity: its position and motion are the packet's, and what the tick keeps goes on. */
    serverUpdate(id: number, update: { position?: NativeVec3; velocity?: NativeVec3; onGround?: boolean }): void {
        const state = this.states.get(id);
        if (!state) return;
        if (update.position) {
            state.position = vec(update.position);
            state.boundingBox = undefined;
        }
        if (update.velocity) state.velocity = vec(update.velocity);
        if (update.onGround !== undefined) state.onGround = update.onGround;
    }

    /** The entity left the world. */
    forget(id: number): void {
        this.states.delete(id);
    }

    state(id: number): NativeMovingEntityState | undefined {
        return this.states.get(id);
    }

    /**
     * One LivingEntity.travel of a squid, fish, guardian, ghast, phantom, allay or vex, from where its AI left it: the control
     * and the motion are the caller's, as the server's AI decides them. A mob the local client does not simulate follows the
     * server; this is for a caller that supplies what the AI would do. A happy ghast or nautilus the player rides stays with
     * NativeRiddenMobPhysics. Also a bee, parrot, dolphin, turtle, axolotl, frog, drowned, spider, polar bear, wither, armor
     * stand, strider, skeleton horse or sulfur cube, and `living` for any LivingEntity with nothing of its own. The dragon has
     * `tickDragon`.
     */
    travelMob(state: NativeMobTravelState, world: PrismarineWorld): MoveRefusable<NativeMobTravelState> {
        const { box, margin } = bodyReach(state.position, state.velocity, state.halfWidth, state.height);
        // (a motion past what a sync reads drops, as NativePhysics.simulate's: docs/numeric-contract.md)
        if (!syncReads(box, margin)) return { ...state, velocity: { x: 0, y: 0, z: 0 }, moveRefused: true };
        const sync = this.syncOf(world);
        sync.sync(world, box, margin);
        return this.binding.simulateVersionedMobTravel(this.version, sync.world, state);
    }

    /**
     * One tick of a dragon's flight, from what its phase decided, and where it leaves the parts. A dragon goes through
     * everything: the world does not come into it.
     */
    tickDragon(state: NativeDragonState): NativeDragonTickResult {
        return this.binding.simulateVersionedDragonTick(this.version, state);
    }

    /** EnderDragon.knockBack of both wings. `targets` are the living entities around, none in creative or spectator mode. */
    dragonWingPushes(parts: NativeDragonPart[], targets: NativeDragonPushTarget[]): NativeDragonPush[] {
        return this.binding.dragonWingPushesForVersion(this.version, parts, targets);
    }

    /**
     * One client tick of the entity. What the tick cannot see is `inputs`: an orb's players, and the random of a tick that
     * draws (an orb in lava, an item or orb in a block before 1.11). A tick of a TNT whose fuse ends takes the state out,
     * as the client discards it; the explosion is the server's packet.
     */
    tick(id: number, world: PrismarineWorld, inputs?: NativeMovingEntityInputs): MoveRefusable<NativeMovingEntityTickResult> | undefined {
        const state = this.states.get(id);
        if (!state) return undefined;
        const { box, margin } = bodyReach(state.position, state.velocity, 0.5, 1);
        if (!syncReads(box, margin)) {
            const refused = { ...state, velocity: { x: 0, y: 0, z: 0 } };
            this.states.set(id, refused);
            return { state: refused, explodes: false, expired: false, mergeCheck: false, randomUsed: 0, randomMissing: false, moveRefused: true };
        }
        const sync = this.syncOf(world);
        sync.sync(world, box, margin);
        const result = this.binding.simulateVersionedMovingEntityTick(this.version, sync.world, state, inputs);
        if (result.state.removed) this.states.delete(id);
        else this.states.set(id, result.state);
        return result;
    }
}
