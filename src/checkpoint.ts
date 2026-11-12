import type {
    NativePhysicsBinding,
    NativeSimulationConfig,
    NativeSimulationState,
    NativeWorld,
} from './native';

/**
 * What a checkpoint is: the release, the config, the whole state (its ordered entities, pistons, rockets, recorded
 * draws and counters) and the world as bytes, each number exact. It is no input log: restoring one continues a run,
 * and the ticks after it still take their inputs from the caller. `docs/checkpoints.md` has the contract.
 */
export const CHECKPOINT_SCHEMA = 1;

/** The families of state a checkpoint carries: each is ticked by its own entry point. */
export type NativeCheckpointKind = 'player' | 'boat' | 'horse' | 'minecart' | 'moving_entity' | 'projectile' | 'arrow' | 'trident' | 'shulker_box';

export interface NativeCheckpointParts<State = NativeSimulationState> {
    version: string;
    /** Which family `state` is of, `player` when not given. */
    kind?: NativeCheckpointKind;
    state: State;
    world: NativeWorld;
    config?: NativeSimulationConfig;
}

/** Numbers JSON would lose: -0 (as 0), NaN and the infinities (as null). They go by their bits. */
function exact(value: unknown): unknown {
    if (typeof value === 'number') {
        if (Number.isFinite(value) && !Object.is(value, -0)) return value;
        const bits = new DataView(new ArrayBuffer(8));
        bits.setFloat64(0, value);
        return { $f64: bits.getBigUint64(0).toString(16).padStart(16, '0') };
    }
    if (Array.isArray(value)) return value.map(exact);
    if (value && typeof value === 'object') {
        // (an absent optional stays absent: it is not a zero)
        return Object.fromEntries(Object.entries(value).filter(([, entry]) => entry !== undefined && entry !== null)
            .map(([key, entry]) => [key, exact(entry)]));
    }
    return value;
}

function restored(value: unknown): unknown {
    if (Array.isArray(value)) return value.map(restored);
    if (value && typeof value === 'object') {
        const bits = (value as { $f64?: string }).$f64;
        if (typeof bits === 'string') {
            if (!/^[0-9a-f]{16}$/.test(bits)) throw new Error(`checkpoint: ${bits} is no number's bits`);
            const view = new DataView(new ArrayBuffer(8));
            view.setBigUint64(0, BigInt(`0x${bits}`));
            return view.getFloat64(0);
        }
        return Object.fromEntries(Object.entries(value).map(([key, entry]) => [key, restored(entry)]));
    }
    return value;
}

/**
 * A text that restores the state and its world exactly. Take `state` from NativePlayer.state() or from
 * simulateVersionedTickOutcome: simulateVersionedTick's result leaves out the scene the next tick reads.
 */
export function encodeCheckpoint<State>(binding: NativePhysicsBinding, parts: NativeCheckpointParts<State>): string {
    const { version, state, world, config, kind = 'player' } = parts;
    if (binding.isSupportedVersion?.(version) === false) throw new Error(`checkpoint: unsupported release ${version}`);
    const bytes = world.checkpoint();
    return JSON.stringify({
        schema: CHECKPOINT_SCHEMA,
        release: version,
        kind,
        layout: binding.nativePlayerLayout(),
        config: exact(config ?? {}),
        state: exact(state),
        worldDigest: world.contentDigest(),
        world: bytes.toString('base64'),
    });
}

/**
 * The parts of a checkpoint: a NativePlayer or simulateVersionedTick goes on from them. `version` is the release the
 * caller means to run: another one than the checkpoint's is refused, as is another schema, packed layout or a
 * world that no longer has the digest the checkpoint took.
 */
export function decodeCheckpoint<State = NativeSimulationState>(
    binding: NativePhysicsBinding,
    text: string,
    version: string,
    kind: NativeCheckpointKind = 'player',
): Required<NativeCheckpointParts<State>> {
    const data = JSON.parse(text) as {
        schema?: number; release?: string; kind?: string; layout?: Record<string, number>; config?: unknown; state?: unknown;
        worldDigest?: string; world?: string;
    };
    if (data.schema !== CHECKPOINT_SCHEMA) {
        throw new Error(`checkpoint: schema ${data.schema}, and this build reads ${CHECKPOINT_SCHEMA}`);
    }
    if (data.release !== version) throw new Error(`checkpoint: of release ${data.release}, not ${version}`);
    if ((data.kind ?? 'player') !== kind) throw new Error(`checkpoint: a ${data.kind} checkpoint, not a ${kind} one`);
    const layout = binding.nativePlayerLayout();
    if (!data.layout || Object.entries(layout).some(([key, value]) => data.layout?.[key] !== value)) {
        throw new Error('checkpoint: of another packed layout');
    }
    if (typeof data.world !== 'string' || typeof data.worldDigest !== 'string') {
        throw new Error('checkpoint: no world');
    }
    if (!data.state || typeof data.state !== 'object') throw new Error('checkpoint: no state');
    const world = binding.nativeWorldFromCheckpoint(Buffer.from(data.world, 'base64'));
    if (world.contentDigest() !== data.worldDigest) throw new Error('checkpoint: the world is not the one it took');
    return {
        version,
        kind,
        world,
        state: restored(data.state) as State,
        config: restored(data.config ?? {}) as NativeSimulationConfig,
    };
}
