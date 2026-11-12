// The shulker boxes a bot sees opening. The server sends a block event (block_action, ShulkerBoxBlockEntity.triggerEvent)
// and each client animates the lid itself: the engine's tick says the progress, this keeps one per open box, and its
// box is the cell's collision shape until the lid is shut (ShulkerBoxBlock.getShape).

import { guardNativeBinding, type NativeAabb, type NativePhysicsBinding, type NativeShulkerBox } from '../native';

/** The block_action packet, as minecraft-protocol hands it on. */
export interface BlockActionPacket {
    location: { x: number; y: number; z: number };
    byte1: number;
    byte2: number;
}

export interface OpenShulkerBox {
    x: number;
    y: number;
    z: number;
    state: NativeShulkerBox;
    /** The cell's collision box after the last tick, at the cell's origin. */
    bounds: NativeAabb;
    /** Whether the lid pushes this tick. */
    pushes: boolean;
}

const key = (x: number, y: number, z: number) => `${x},${y},${z}`;

export class ShulkerBoxTracker {
    private readonly entries = new Map<string, OpenShulkerBox>();

    constructor(private readonly binding: NativePhysicsBinding, private readonly version: string) {
        guardNativeBinding(binding);
    }

    /** The block event of a box; `facing` is its FACING as a Direction's ordinal. */
    event(x: number, y: number, z: number, facing: number, count: number): void {
        const at = key(x, y, z);
        const shut = { min: { x: 0, y: 0, z: 0 }, max: { x: 1, y: 1, z: 1 } };
        const entry = this.entries.get(at) ?? { x, y, z, state: { facing, status: 0, progress: 0, progressOld: 0, pushProgress: 0 }, bounds: shut, pushes: false };
        this.entries.set(at, { ...entry, state: this.binding.shulkerBoxBlockEvent(this.version, entry.state, count) });
    }

    /** The block entity's tick, for each box: one that came to rest shut is a plain cube again, and goes. */
    tick(): void {
        for (const [at, entry] of this.entries) {
            const { state, bounds, pushes } = this.binding.simulateVersionedShulkerBoxTick(this.version, entry.state);
            if (state.status === 0) this.entries.delete(at);
            else this.entries.set(at, { ...entry, state, bounds, pushes });
        }
    }

    /** A cell that no longer holds a shulker box, replaced or unloaded, keeps no lid. */
    forget(x: number, y: number, z: number): void {
        this.entries.delete(key(x, y, z));
    }

    /** A column that left the client: its block entities go with it, and one that loads again starts shut. */
    forgetColumn(chunkX: number, chunkZ: number): void {
        for (const [at, { x, z }] of this.entries) {
            if (x >> 4 === chunkX && z >> 4 === chunkZ) this.entries.delete(at);
        }
    }

    boxes(): OpenShulkerBox[] {
        return [...this.entries.values()];
    }
}
