// A bot's carts: what its client keeps of each cart, from the packets, through NativeMinecartPhysics' client paths. A client
// moves no cart: the rail is the server's, and everything here is what the packets say and the interpolation to it. Nothing
// the server decides (a fuse's length, a blast, a pickup, a cart breaking) is worked out here: the state follows the entity
// data, the entity events and the removal that tell of it.
//
// It needs a packet source and a tick, nothing of mineflayer's own: `bot._client` is a source, and whatever runs the bot's
// physics tick calls `tick()`, before the local player's own tick (the level ticks its vehicles first).
import type { NativeMinecartClientEffects, NativeMinecartClientSteps, NativeMinecartLerp, NativeMinecartStep, NativeVec3 } from '../native';
import type { NativeMinecartPhysics } from './vehicles';

const f32 = Math.fround;

/** What the tracker reads packets from: an EventEmitter of packets by name, as minecraft-protocol's client is. */
export interface MinecartPacketSource {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    on(event: string, listener: (packet: any) => void): unknown;
}

/** What a client's cart is between two ticks. */
export interface ClientMinecart {
    readonly id: number;
    /** The entity's name in any release (`minecart`, `tnt_minecart`, ...). */
    readonly variant: string;
    position: NativeVec3;
    velocity: NativeVec3;
    /** Float degrees. */
    yRot: number;
    xRot: number;
    onGround: boolean;
    /** The old path's interpolation, when it runs. */
    lerp: NativeMinecartLerp | null;
    /** The improved path's steps, when the level has the feature. */
    steps: NativeMinecartClientSteps | null;
    /** The velocity the next interpolation starts with (the behaviour's targetDeltaMovement). */
    targetVelocity: NativeVec3;
    /** Where the server last said it is: what its relative moves add to. */
    tracked: NativeVec3;
    effects: NativeMinecartClientEffects;
    hurtDir: number;
    /** The entity ids it carries. */
    passengers: number[];
}

/** The cart a release's spawn packet means. */
const CART_NAMES = new Set(['minecart', 'chest_minecart', 'hopper_minecart', 'furnace_minecart', 'tnt_minecart', 'spawner_minecart', 'command_block_minecart', 'commandblock_minecart']);
/** The object data of a cart before 1.14, in AbstractMinecart.Type's order (1.8.9, 1.13.2 AbstractMinecart.java). */
const OBJECT_CARTS = ['minecart', 'chest_minecart', 'furnace_minecart', 'tnt_minecart', 'spawner_minecart', 'hopper_minecart', 'command_block_minecart'] as const;
/** The object type of every cart before 1.14 (1.8.9 EntitySpawnS2CPacket, 1.13.2 the same). */
const OBJECT_CART = 10;

const copy = (v: NativeVec3): NativeVec3 => ({ x: v.x, y: v.y, z: v.z });
const byteAngle = (value: number) => f32(f32(value * 360) / 256);
const number = (value: unknown): number => (typeof value === 'bigint' ? Number(value) : value as number);

interface Delta { readonly dX: number; readonly dY: number; readonly dZ: number }

/** A move's deltas: 26.3 carries one or several ticks' in a VecDelta, each on the position the last left. */
function deltasOf(packet: { dX?: number; dY?: number; dZ?: number; delta?: Delta & { steps?: readonly Delta[] } }): readonly Delta[] {
    if (packet.delta) return packet.delta.steps ?? [packet.delta];
    return [{ dX: packet.dX ?? 0, dY: packet.dY ?? 0, dZ: packet.dZ ?? 0 }];
}

/**
 * Packets to a bot's carts, a tick at a time. `improvements` is the level's minecart_improvements: the login's feature_flags
 * packet sets it, or a caller who knows it does.
 */
export class MinecartTracker {
    readonly carts = new Map<number, ClientMinecart>();
    improvements: boolean;

    constructor(private readonly physics: NativeMinecartPhysics, source: MinecartPacketSource, options: { improvements?: boolean } = {}) {
        this.improvements = options.improvements ?? false;
        const on = <T>(name: string, listener: (packet: T) => void) => source.on(name, listener);
        on<{ features?: string[] }>('feature_flags', packet => { this.improvements = (packet.features ?? []).includes('minecraft:minecart_improvements'); });
        on<SpawnPacket>('spawn_entity', packet => this.spawned(packet));
        on<{ entityIds: number[] }>('entity_destroy', packet => { for (const id of packet.entityIds) this.carts.delete(id); });
        on<{ entityId: number } & MovePacket>('rel_entity_move', packet => this.moved(packet, false));
        on<{ entityId: number } & MovePacket>('entity_move_look', packet => this.moved(packet, true));
        on<{ entityId: number; yaw: number; pitch: number; onGround?: boolean }>('entity_look', packet => this.looked(packet));
        on<TeleportPacket>('entity_teleport', packet => this.teleported(packet));
        on<SyncPacket>('sync_entity_position', packet => this.synced(packet));
        on<{ entityId: number; velocity?: NativeVec3; velocityX?: number; velocityY?: number; velocityZ?: number }>('entity_velocity', packet => this.velocity(packet));
        on<{ entityId: number; metadata: readonly { key: number; value: unknown }[] }>('entity_metadata', packet => this.metadata(packet));
        on<{ entityId: number; entityStatus: number }>('entity_status', packet => this.status(packet));
        on<{ entityId: number; steps: readonly RawStep[] }>('move_minecart', packet => this.alongTrack(packet));
        on<{ entityId: number; passengers: number[] }>('set_passengers', packet => { const cart = this.carts.get(packet.entityId); if (cart) cart.passengers = [...packet.passengers]; });
    }

    /** A cart that is there already, as whoever joins late finds it, or a caller that knows its state. */
    adopt(cart: Pick<ClientMinecart, 'id' | 'variant' | 'position' | 'yRot' | 'xRot'> & Partial<ClientMinecart>): ClientMinecart {
        const made: ClientMinecart = {
            velocity: { x: 0, y: 0, z: 0 }, onGround: false, lerp: null,
            steps: null, targetVelocity: { x: 0, y: 0, z: 0 }, tracked: copy(cart.position),
            effects: { hurtTime: 0, damage: 0, fuse: -1 }, hurtDir: 1, passengers: [],
            ...cart,
        };
        if (this.improvements) made.steps ??= this.stepsOf(made);
        this.carts.set(cart.id, made);
        return made;
    }

    /**
     * One client tick of every cart: what ran its interpolation, the numbers that run down between the server's packets.
     * The old path goes to its target; the improved path along the steps the server sent (26.3 AbstractMinecart.tick and
     * the behaviours' tick on a client).
     */
    tick(): void {
        for (const cart of this.carts.values()) {
            if (cart.steps) {
                const steps = this.physics.interpolateSteps({ ...cart.steps, position: cart.position, velocity: cart.velocity, yRot: cart.yRot, xRot: cart.xRot });
                cart.steps = steps;
                cart.position = steps.position;
                cart.velocity = steps.velocity;
                cart.yRot = steps.yRot;
                cart.xRot = steps.xRot;
            } else if (cart.lerp && cart.lerp.steps > 0) {
                const lerp = this.physics.interpolate({ ...cart.lerp, position: cart.position, yRot: cart.yRot, xRot: cart.xRot });
                cart.lerp = lerp;
                cart.position = lerp.position;
                cart.yRot = lerp.yRot;
                cart.xRot = lerp.xRot;
            }
            cart.effects = this.physics.tickEffects(cart.effects);
        }
    }

    private lt(version: string): boolean {
        return this.physics.olderThan(version);
    }

    private stepsOf(cart: ClientMinecart): NativeMinecartClientSteps {
        const old: NativeMinecartStep = { position: copy(cart.position), movement: copy(cart.velocity), yRot: cart.yRot, xRot: cart.xRot, weight: 0 };
        return { position: copy(cart.position), velocity: copy(cart.velocity), yRot: cart.yRot, xRot: cart.xRot, delay: 0, pending: [], current: [], totalWeight: 0, old };
    }

    private variantOf(packet: SpawnPacket): string | null {
        const registry = this.physics.data as unknown as { entities?: Record<number, { name: string }>; entitiesArray?: readonly { internalId?: number; id: number; name: string }[] };
        if (this.lt('1.14')) return packet.type === OBJECT_CART ? (OBJECT_CARTS[packet.objectData ?? 0] ?? 'minecart') : null;
        const name = registry.entitiesArray?.find(entity => (entity.internalId ?? entity.id) === packet.type)?.name ?? registry.entities?.[packet.type]?.name;
        return name && CART_NAMES.has(name) ? name : null;
    }

    private spawned(packet: SpawnPacket): void {
        const variant = this.variantOf(packet);
        if (variant === null) return;
        // fixed point before 1.9 (1.8.9 EntitySpawnS2CPacket)
        const scale = this.lt('1.9') ? 32 : 1;
        const position = { x: packet.x / scale, y: packet.y / scale, z: packet.z / scale };
        const yRot = byteAngle(packet.yaw);
        const xRot = byteAngle(packet.pitch);
        const velocity = this.velocityOf(packet);
        this.adopt({ id: packet.entityId, variant, position, yRot, xRot, velocity: copy(velocity), targetVelocity: velocity });
    }

    /** The motion of a packet, in blocks a tick: a short vector over 8000 until 26.3 sent it as its own (lpVec3). */
    private velocityOf(packet: { velocity?: NativeVec3; velocityX?: number; velocityY?: number; velocityZ?: number }): NativeVec3 {
        const v = packet.velocity ?? { x: packet.velocityX ?? 0, y: packet.velocityY ?? 0, z: packet.velocityZ ?? 0 };
        return this.lt('26.3') ? { x: v.x / 8000, y: v.y / 8000, z: v.z / 8000 } : copy(v);
    }

    private velocity(packet: { entityId: number; velocity?: NativeVec3; velocityX?: number; velocityY?: number; velocityZ?: number }): void {
        const cart = this.carts.get(packet.entityId);
        if (!cart) return;
        // OldMinecartBehavior.lerpMotion: for the interpolation to start from too; NewMinecartBehavior.lerpMotion: alone
        const velocity = this.velocityOf(packet);
        cart.velocity = copy(velocity);
        cart.targetVelocity = velocity;
    }

    /**
     * Entity.lerpTo of the old path. A new interpolation resets the motion to what the server last sent
     * (1.20.4 AbstractMinecart.lerpTo, 26.3 OldMinecartBehavior.onInterpolationStart). The
     * improved path has no such thing: its handler is NO_OP and the entity goes there at once (26.3
     * InterpolationHandler.NO_OP, Entity.moveOrInterpolateTo).
     */
    private lerpTo(cart: ClientMinecart, target: NativeVec3, yRot: number, xRot: number, steps?: number): void {
        if (this.improvements) {
            cart.position = copy(target);
            cart.yRot = yRot;
            cart.xRot = xRot;
            return;
        }
        const started = this.physics.lerpTo(cart, target, yRot, xRot);
        cart.lerp = steps === undefined ? started : { ...started, steps };
        cart.velocity = copy(cart.targetVelocity);
    }

    /** VecDeltaCodec.decode: an axis no delta touches keeps the double the server gave; the rest are in 4096ths. */
    private addDeltas(cart: ClientMinecart, { dX, dY, dZ }: Delta): void {
        if (this.lt('1.9')) {
            // whole 32nds, added in the entity's ints (1.8.8 ClientPacketListener.handleMoveEntity)
            cart.tracked = { x: (Math.round(cart.tracked.x * 32) + dX) / 32, y: (Math.round(cart.tracked.y * 32) + dY) / 32, z: (Math.round(cart.tracked.z * 32) + dZ) / 32 };
            return;
        }
        // lfloor until 1.19.2 (ClientboundMoveEntityPacket.entityToPacket, 1.17.1 ClientboundMoveEntityPacket.java:23-25; 1.19.2
        // VecDeltaCodec.java:10-11), Math.round from 1.19.3 (VecDeltaCodec.java:15-16); 1.14 keeps the long itself, which is the same
        // as long as what was sent were 4096ths
        const encode = this.lt('1.19.3') ? Math.floor : Math.round;
        const axis = (base: number, delta: number) => (delta === 0 ? base : (encode(base * 4096) + delta) / 4096);
        cart.tracked = { x: axis(cart.tracked.x, dX), y: axis(cart.tracked.y, dY), z: axis(cart.tracked.z, dZ) };
    }

    private moved(packet: { entityId: number } & MovePacket, rotates: boolean): void {
        const cart = this.carts.get(packet.entityId);
        if (!cart) return;
        for (const delta of deltasOf(packet)) this.addDeltas(cart, delta);
        const [lastYRot, lastXRot] = this.lastRotation(cart);
        const yRot = rotates ? byteAngle(packet.yaw!) : lastYRot;
        const xRot = rotates ? byteAngle(packet.pitch!) : lastXRot;
        this.lerpTo(cart, cart.tracked, yRot, xRot);
        cart.onGround = packet.delta?.onGround ?? packet.onGround ?? cart.onGround;
    }

    /** The rotation a move without one keeps: the target's from 1.20.2 (1.20.2 getLerpTargetYaw), the entity's before. */
    private lastRotation(cart: ClientMinecart): [number, number] {
        if (!this.lt('1.20.2') && cart.lerp && cart.lerp.steps > 0) return [cart.lerp.targetYRot, cart.lerp.targetXRot];
        return [cart.yRot, cart.xRot];
    }

    private looked(packet: { entityId: number; yaw: number; pitch: number; onGround?: boolean }): void {
        const cart = this.carts.get(packet.entityId);
        if (!cart) return;
        // a turn alone: before 1.15 it lerps to the tracked place like any move (1.14.4 ClientPacketListener.handleMoveEntity); to where it is, or
        // from 1.20.2 where it is going (1.16.5, 1.21 ClientPacketListener.handleMoveEntity)
        const target = this.lt('1.15') ? cart.tracked : (!this.lt('1.20.2') && cart.lerp && cart.lerp.steps > 0 ? cart.lerp.target : cart.position);
        this.lerpTo(cart, target, byteAngle(packet.yaw), byteAngle(packet.pitch));
        cart.onGround = packet.onGround ?? cart.onGround;
    }

    private teleported(packet: TeleportPacket): void {
        const cart = this.carts.get(packet.entityId);
        if (!cart) return;
        const scale = this.lt('1.9') ? 32 : 1;
        const flags = typeof packet.flags === 'object' ? packet.flags : null;
        const base = cart.lerp && cart.lerp.steps > 0 ? cart.lerp.target : cart.position;
        const target = {
            x: flags?.x ? base.x + packet.x : packet.x / scale,
            y: flags?.y ? base.y + packet.y : packet.y / scale,
            z: flags?.z ? base.z + packet.z : packet.z / scale,
        };
        cart.tracked = copy(target);
        // floats from 1.21.2, a byte before; relative to its own where the flags say (PositionMoveRotation.calculateAbsolute)
        const yRot = flags ? (flags.yaw ? f32(cart.yRot + packet.yaw) : packet.yaw) : byteAngle(packet.yaw);
        const xRot = flags ? (flags.pitch ? f32(cart.xRot + packet.pitch) : packet.pitch) : byteAngle(packet.pitch);
        // Up to 1.16.1 a teleport this near only turns it: at once on 1.9 - 1.14.4 (a lerp of 0 steps, 1.14.4
        // ClientPacketListener.java:571), over the usual 3 before 1.9 and again on 1.15 - 1.16.1 (1.8.8 ClientPacketListener.java:452,
        // 1.15 ClientPacketListener.java:581-582, 1.16 :592-593)
        const near = this.lt('1.16.2') && Math.abs(cart.position.x - target.x) < 0.03125 && Math.abs(cart.position.y - target.y) < 0.015625 && Math.abs(cart.position.z - target.z) < 0.03125;
        if (near) this.lerpTo(cart, cart.position, yRot, xRot, this.lt('1.9') || !this.lt('1.15') ? undefined : 2);
        else this.lerpTo(cart, target, yRot, xRot);
        cart.onGround = packet.onGround ?? cart.onGround;
    }

    /** ClientPacketListener.handleEntityPositionSync: from 1.21.2. A jump of over 64 blocks is no interpolation. */
    private synced(packet: SyncPacket): void {
        const cart = this.carts.get(packet.entityId);
        if (!cart) return;
        const at = packet.path ? (packet.path.steps?.at(-1) ?? packet.path) : packet as unknown as NativeVec3;
        const target = { x: at.x, y: at.y, z: at.z };
        cart.tracked = copy(target);
        const dx = cart.position.x - target.x;
        const dy = cart.position.y - target.y;
        const dz = cart.position.z - target.z;
        if (dx * dx + dy * dy + dz * dz > 4096) {
            cart.position = copy(target);
            cart.yRot = packet.yaw;
            cart.xRot = packet.pitch;
            cart.lerp = null;
        } else {
            this.lerpTo(cart, target, packet.yaw, packet.pitch);
        }
        cart.onGround = packet.onGround ?? cart.onGround;
    }

    /** ClientPacketListener.handleMinecartAlongTrack: the steps go on the behaviour's list, in degrees of the packed byte. */
    private alongTrack(packet: { entityId: number; steps: readonly RawStep[] }): void {
        const cart = this.carts.get(packet.entityId);
        if (!cart?.steps) return;
        const steps = packet.steps.map(step => ({
            position: step.position, movement: step.velocity ?? step.movement!,
            yRot: byteAngle(step.yaw), xRot: byteAngle(step.pitch), weight: f32(step.weight),
        }));
        cart.steps = { ...cart.steps, pending: [...cart.steps.pending, ...steps] };
    }

    /** The synced numbers: the index of VehicleEntity's hurt, its direction and damage, by what a release defined before them. */
    private hurtIndex(): number {
        const keys = (this.physics.data as unknown as { entitiesByName?: Record<string, { metadataKeys?: readonly string[] }> }).entitiesByName?.minecart?.metadataKeys;
        const known = keys?.indexOf('hurt');
        if (known !== undefined && known >= 0) return known;
        if (this.lt('1.9')) return 17;
        if (this.lt('1.10')) return 5;
        if (this.lt('1.14')) return 6;
        return this.lt('1.17') ? 7 : 8;
    }

    private metadata(packet: { entityId: number; metadata: readonly { key: number; value: unknown }[] }): void {
        const cart = this.carts.get(packet.entityId);
        if (!cart) return;
        const hurt = this.hurtIndex();
        for (const { key, value } of packet.metadata) {
            if (key === hurt) cart.effects = { ...cart.effects, hurtTime: number(value) };
            else if (key === hurt + 1) cart.hurtDir = number(value);
            else if (key === hurt + 2) cart.effects = { ...cart.effects, damage: number(value) };
        }
    }

    /**
     * MinecartTNT.handleEntityEvent: the priming event, 10 up to 26.1 and 70 from 26.2 (26.1 and 26.2 MinecartTNT.java
     * handleEntityEvent). A client's fuse is 80 whatever the server drew for its own: the client only runs it down.
     */
    private status(packet: { entityId: number; entityStatus: number }): void {
        const cart = this.carts.get(packet.entityId);
        if (!cart || cart.variant !== 'tnt_minecart') return;
        if (packet.entityStatus === (this.lt('26.2') ? 10 : 70)) cart.effects = { ...cart.effects, fuse: 80 };
    }
}

interface SpawnPacket {
    entityId: number;
    type: number;
    x: number;
    y: number;
    z: number;
    yaw: number;
    pitch: number;
    objectData?: number;
    velocity?: NativeVec3;
    velocityX?: number;
    velocityY?: number;
    velocityZ?: number;
}

interface MovePacket {
    dX?: number;
    dY?: number;
    dZ?: number;
    delta?: Delta & { steps?: readonly Delta[]; onGround?: boolean };
    yaw?: number;
    pitch?: number;
    onGround?: boolean;
}

interface TeleportPacket {
    entityId: number;
    x: number;
    y: number;
    z: number;
    yaw: number;
    pitch: number;
    flags?: { x?: boolean; y?: boolean; z?: boolean; yaw?: boolean; pitch?: boolean } | number;
    onGround?: boolean;
}

interface SyncPacket {
    entityId: number;
    x?: number;
    y?: number;
    z?: number;
    path?: NativeVec3 & { steps?: readonly NativeVec3[] };
    yaw: number;
    pitch: number;
    onGround?: boolean;
}

interface RawStep {
    position: NativeVec3;
    movement?: NativeVec3;
    velocity?: NativeVec3;
    yaw: number;
    pitch: number;
    weight: number;
}
