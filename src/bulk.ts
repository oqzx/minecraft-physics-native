// A long bulk simulation that can be stopped. A native call cannot be interrupted, so a NativeBulkRun runs a few ticks at a
// time and this loop gives the event loop a turn between them. See docs/bulk-simulation.md for the contract.
import type { NativeBulkRun, NativeWorld } from './native';

/** One second of game time, the size of a chunk unless the caller chooses another. */
export const DEFAULT_CHUNK_TICKS = 20;

export interface BulkRunOptions {
    /** The ticks of a chunk: the most a cancellation waits for, times the entities. DEFAULT_CHUNK_TICKS by default. */
    chunkTicks?: number;
    /** Stops the run between two chunks. A run already stopped runs nothing. */
    signal?: AbortSignal;
    /** Called after each chunk with the ticks the run has done in all and the ticks of this call done so far. */
    onProgress?(ticksDone: number, ticksRun: number): void;
}

export interface BulkRunOutcome {
    /** The whole ticks of the run, this call's and those before. */
    ticksDone: number;
    /** The whole ticks of this call. */
    ticksRun: number;
    /** Whether the signal stopped this call short of its ticks. The run stays where it stopped, ready to go on. */
    cancelled: boolean;
}

const nextTurn = (): Promise<void> => new Promise(resolve => {
    if (typeof setImmediate === 'function') setImmediate(resolve);
    else setTimeout(resolve, 0);
});

/**
 * Runs `ticks` more ticks of `run` in `world`, a chunk at a time, and resolves at the end or at the first chunk boundary
 * after the signal aborts. An error of the engine rejects and finalizes the run; an error of `onProgress` rejects and
 * leaves the run at the chunk it finished. The world stays the caller's: what it changes between chunks, the next
 * chunk sees, as with ticks a caller runs one by one.
 */
export async function runBulk(run: NativeBulkRun, world: NativeWorld, ticks: number, options: BulkRunOptions = {}): Promise<BulkRunOutcome> {
    if (!Number.isInteger(ticks) || ticks < 0) throw new Error(`runBulk: ticks must be a whole number of ticks, not ${ticks}`);
    const chunkTicks = options.chunkTicks ?? DEFAULT_CHUNK_TICKS;
    if (!Number.isInteger(chunkTicks) || chunkTicks < 1) throw new Error(`runBulk: chunkTicks must be at least 1, not ${chunkTicks}`);
    // the most ticks that stay under the run's limit: one, whatever the entities, since a tick cannot be split
    const chunk = Math.min(chunkTicks, Math.max(1, Math.floor(run.maxChunkWork / Math.max(1, run.entityCount))));
    let ticksRun = 0;
    while (ticksRun < ticks) {
        if (options.signal?.aborted) return { ticksDone: run.ticksDone, ticksRun, cancelled: true };
        const size = Math.min(chunk, ticks - ticksRun);
        run.advance(world, size);
        ticksRun += size;
        options.onProgress?.(run.ticksDone, ticksRun);
        if (ticksRun < ticks) await nextTurn();
    }
    return { ticksDone: run.ticksDone, ticksRun, cancelled: false };
}
