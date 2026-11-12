'use strict';

// The addon, the loader and declarations generated with it, the hand-written NativePhysicsBinding and the packed layouts
// describe one thing: each check is shown to refuse the drift it is there for.

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { describe, it } = require('node:test');
const { declarationProblems } = require('../scripts/native-declarations.cjs');

const root = path.resolve(__dirname, '..');
const native = fs.readFileSync(path.join(root, 'src', 'native.ts'), 'utf8');
const declarations = fs.readFileSync(path.join(root, 'native', 'artifacts', 'native.d.ts'), 'utf8');
const rust = fs.readFileSync(path.join(root, 'native', 'src', 'napi_player.rs'), 'utf8');
const loader = fs.readFileSync(path.join(root, 'native', 'artifacts', 'native.js'), 'utf8');

/** The text with one replacement, which has to find something to replace. */
const changed = (text, from, to) => {
    const result = text.replace(from, to);
    assert.notEqual(result, text, `${from} is not in the text`);
    return result;
};
const refused = (overrides, pattern) => {
    const problems = declarationProblems(overrides);
    assert.ok(problems.some(problem => pattern.test(problem)), problems.join('\n') || 'no problem found');
};

describe('the native declarations', () => {
    it('agree with the addon, the loader, the binding interface and the Rust layouts', () => {
        assert.deepEqual(declarationProblems(), []);
    });

    it('refuse an export the loader, the declarations or the interface lacks', () => {
        refused({ loader: changed(loader, 'module.exports.entityTravelSupport = ', '// ') }, /entityTravelSupport is in the addon and not in its loader/);
        refused({ declarations: changed(declarations, 'export declare function entityTravelSupport', 'declare function entityTravelSupport') },
            /entityTravelSupport is in the addon and not in native\.d\.ts/);
        refused({ native: changed(native, /    entityTravelSupport\(version: string, entityType: string\): NativeEntityTravelSupport;\r?\n/, '') },
            /entityTravelSupport is in the addon and not in NativePhysicsBinding/);
        refused({ native: changed(native, 'export interface NativePhysicsBinding {', 'export interface NativePhysicsBinding {\n    goneFromTheAddon(): void;') },
            /goneFromTheAddon is in NativePhysicsBinding and not in the addon/);
    });

    it('refuse a method of NativeWorld or NativePlayer the addon does not have', () => {
        refused({ native: changed(native, '    entityPushes(): Float64Array;', '    entityPushes(): Float64Array;\n    gonePush(): number;') },
            /gonePush is in the NativePlayer interface and not in the addon's NativePlayer/);
    });

    it('refuse an incompatible signature, either way it differs', () => {
        refused({ native: changed(native, /simulateVersionedTick\(\s+version: string,/, 'simulateVersionedTick(\n        version: number,') },
            /simulateVersionedTick: .* what it takes/);
        refused({ native: changed(native, 'simulateBoatTick(state: NativeBoatState): NativeBoatState;', 'simulateBoatTick(state: NativeBoatState, extra: string): NativeBoatState;') },
            /simulateBoatTick: .* what it takes/);
        refused({ native: changed(native, 'minecartClientLerpSteps(version: string): number;', 'minecartClientLerpSteps(version: string): string;') },
            /minecartClientLerpSteps: .* what it gives/);
        // an optional result the interface promises
        refused({ native: changed(native, 'generatedBlockStateCount(version: string, block: string): number | null;', 'generatedBlockStateCount(version: string, block: string): number;') },
            /generatedBlockStateCount: .* what it gives/);
        refused({ native: changed(native, 'finishTick(world: NativeWorld, height: number): Float64Array;', 'finishTick(world: NativeWorld, height: string): Float64Array;') },
            /NativePlayer\.finishTick: .* what it takes/);
        // a field of a state: what a caller passes and what comes back
        refused({ native: changed(native, /export interface NativeSimulationState \{(\s+)position: NativeVec3;/, 'export interface NativeSimulationState {$1position: string;') },
            /simulateVersionedTick: /);
    });

    it('refuse a packed layout the engine does not read', () => {
        refused({ native: changed(native, 'LENGTH: 32,', 'LENGTH: 33,') }, /NativePlayerInput\.LENGTH is 33, input::LENGTH is 32/);
        refused({ rust: changed(rust, 'pub const ENTITY_LENGTH: usize = 21;', 'pub const ENTITY_LENGTH: usize = 22;') }, /NATIVE_PLAYER_ENTITY_LENGTH is 21, ENTITY_LENGTH is 22/);
        refused({ rust: changed(rust, 'pub const ON_GROUND: u32 = 1 << 11;', 'pub const ON_GROUND: u32 = 1 << 25;') }, /NativePlayerInputFlag\.ON_GROUND/);
        refused({ rust: changed(rust, 'pub const LENGTH: usize = 24;', 'pub const EXTRA: usize = 24;\n    pub const LENGTH: usize = 25;') }, /NativePlayerOutput\.EXTRA is undefined/);
    });
});
