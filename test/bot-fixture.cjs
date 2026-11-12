'use strict';

// A bot as mineflayer's is, as far as EPhysicsCtx.FROM_BOT, PlayerState and the adapter read it, on a stone floor at y 63.
// Its requires are of the caller's resolution, so that a packed copy runs it with its own consumer's dependencies.

const { EventEmitter } = require('node:events');
const { Vec3 } = require('vec3');

function worldOf(registry, Block) {
    const floor = registry.blocksByName.stone.defaultState ?? registry.blocksByName.stone.id << 4;
    const air = registry.blocksByName.air.defaultState ?? 0;
    const stateAt = pos => (Math.floor(pos.y) === 63 ? floor : air);
    return Object.assign(new EventEmitter(), {
        getBlock: pos => Object.assign(Block.fromStateId(stateAt(pos), 0), { position: new Vec3(Math.floor(pos.x), Math.floor(pos.y), Math.floor(pos.z)) }),
        getBlockStateId: stateAt,
    });
}

function botOf(version, fields = {}) {
    const registry = require('prismarine-registry')(version);
    const Block = require('prismarine-block')(registry);
    const bot = new EventEmitter();
    return Object.assign(bot, {
        version,
        registry,
        world: worldOf(registry, Block),
        game: { dimension: 'overworld', gameMode: 'survival', minY: 0, height: 256 },
        entity: {
            position: new Vec3(0.5, 64, 0.5), velocity: new Vec3(0, 0, 0), onGround: true, yaw: 0, pitch: 0,
            height: 1.8, width: 0.6, effects: {}, attributes: {}, metadata: [], equipment: [], type: 'player', username: 'bot',
        },
        inventory: { slots: [] },
        heldItem: null,
        fireworkRocketDuration: 0,
        controlState: { forward: true, back: false, left: false, right: false, jump: false, sprint: false, sneak: false },
        getControlState(name) { return bot.controlState[name]; },
        setControlState(name, value) { bot.controlState[name] = value; },
        getEquipmentDestSlot: () => 0,
        usingHeldItem: false,
        ...fields,
    });
}

module.exports = { botOf };
