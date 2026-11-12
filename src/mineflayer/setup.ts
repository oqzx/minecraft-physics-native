// The way from a bot to its exact engine. The default loader only attaches the binding: this makes the engine of the bot's
// release, makes contexts with the caller's own mineflayer-physics-utils, and says who ticks. The package never ticks by
// itself: `simulate` runs when a caller calls it, or, after installTicks, when mineflayer's physics plugin does.
import type { NativeEngineOptions, PhysicsRegistry, PistonBot } from './base';
import type { NativePhysics } from './player';
import type { PhysicsCtx } from './types';
import { createNativePhysics } from './versions';
import type { PrismarineWorld } from './world';

type Simulate = (ctx: PhysicsCtx, world: PrismarineWorld) => unknown;

/** The parts of a mineflayer bot the setup reads. */
export interface PhysicsBot extends PistonBot {
    /** The release the bot talks, which its registry must be of. */
    version?: string;
    registry?: PhysicsRegistry;
    /** The engine of mineflayer's physics plugin, whose tick calls its `simulate` (lib/plugins/physics.js). */
    physicsEngine?: { simulate?: Simulate };
}

/** mineflayer-physics-utils' EPhysicsCtx, of the copy the caller has (a dependency this package does not load). */
export interface PhysicsCtxFactory<Ctx, Bot> {
    FROM_BOT(physics: never, bot: Bot): Ctx;
}

const setups = new WeakMap<object, NativeBotPhysics>();

/**
 * The native engine of a bot, one a bot at a time. It owns the engine, whose worlds it lets go of when the bot ends or
 * `dispose` is called. A context stays the caller's: mineflayer-physics-utils' state, applied to the bot by whoever ticks.
 */
export class NativeBotPhysics {
    readonly physics: NativePhysics;
    private readonly onEnd = () => this.dispose();
    private restoreTicks: (() => void) | null = null;
    private disposed = false;

    constructor(private readonly bot: PhysicsBot, registry: PhysicsRegistry, options: NativeEngineOptions) {
        this.physics = createNativePhysics(registry, options);
        if (bot.version !== undefined && bot.version !== this.physics.version) {
            throw new Error(`createBotPhysics: the bot runs ${bot.version}, and the engine asked for is ${this.physics.version}`);
        }
        bot.on?.('end', this.onEnd);
    }

    /** EPhysicsCtx.FROM_BOT over this engine, from the caller's EPhysicsCtx. */
    context<Ctx>(factory: PhysicsCtxFactory<Ctx, PhysicsBot>): Ctx {
        if (this.disposed) throw new Error('NativeBotPhysics.context: the setup was disposed');
        return factory.FROM_BOT(this.physics as never, this.bot);
    }

    /**
     * Has mineflayer's physics plugin tick this engine in place of its own: its loop stays what decides when a tick runs
     * and what it does while riding, and calls this engine's simulate where it called its own. Once is enough, a second
     * call does nothing, and `dispose` takes it back. A bot without the plugin is refused: nothing would tick.
     */
    installTicks(): void {
        if (this.disposed) throw new Error('NativeBotPhysics.installTicks: the setup was disposed');
        if (this.restoreTicks) return;
        const engine = this.bot.physicsEngine;
        if (typeof engine?.simulate !== 'function') {
            throw new Error('NativeBotPhysics.installTicks: the bot has no bot.physicsEngine.simulate: mineflayer\'s physics plugin is not loaded');
        }
        const own = Object.getOwnPropertyDescriptor(engine, 'simulate');
        engine.simulate = (ctx, world) => this.physics.simulate(ctx, world);
        this.restoreTicks = () => {
            if (own) Object.defineProperty(engine, 'simulate', own);
            else delete engine.simulate;
        };
    }

    /** Takes back installTicks and lets go of the engine's worlds and listeners. A second call does nothing. */
    dispose(): void {
        if (this.disposed) return;
        this.disposed = true;
        this.restoreTicks?.();
        this.restoreTicks = null;
        this.bot.removeListener?.('end', this.onEnd);
        this.physics.dispose();
        if (setups.get(this.bot) === this) setups.delete(this.bot);
    }
}

/**
 * The engine of the bot's exact release, by its registry (`options.version` must name the same). An unknown or mismatched
 * release, a bot not yet given a registry and a bot that has a setup already are refused before anything is listened to.
 */
export function createBotPhysics(bot: PhysicsBot, options: NativeEngineOptions = {}): NativeBotPhysics {
    if (!bot.registry) throw new Error('createBotPhysics: the bot has no registry yet (mineflayer gives it one when it knows the release)');
    if (setups.has(bot)) throw new Error('createBotPhysics: the bot has a setup already: dispose it before making another');
    const setup = new NativeBotPhysics(bot, bot.registry, options);
    setups.set(bot, setup);
    return setup;
}
