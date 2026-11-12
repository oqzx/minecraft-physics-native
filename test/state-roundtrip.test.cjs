'use strict';

// B005: the round trip of every boat, mount, cart, projectile and moving entity state: every field in one class, false,
// zero, absent and nondefault values kept apart.

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { describe, it } = require('node:test');
const physics = require('..');
const { STATES } = require('../scripts/state-roundtrip.cjs');

const native = physics.LoadNativePhysics();
const declarations = fs.readFileSync(path.join(__dirname, '..', 'native', 'artifacts', 'native.d.ts'), 'utf8');
const fieldsOf = kind => {
    const body = new RegExp(`export interface ${kind} \\{([\\s\\S]*?)\\n\\}`).exec(declarations)[1];
    return [...body.matchAll(/^ {2}(\w+)(\??): ([^\r\n]+)/gm)].map(([, name, optional, type]) => ({ name, optional: optional === '?', type }));
};
const scalar = field => /^(boolean|number)\b/.test(field.type);
const valuesOf = field => (field.type.startsWith('boolean') ? [true, false] : [0, 1, 2]);
const stateOf = result => result.state ?? result;

for (const [kind, spec] of Object.entries(STATES)) {
    describe(kind, () => {
        const fields = fieldsOf(kind);
        const world = () => new native.NativeWorld();
        const tick = extra => stateOf(spec.tick(native, world(), { ...spec.base, ...extra }));
        const special = new Set([...spec.evolved, ...spec.config, ...spec.refused, ...spec.structured]);
        const kept = fields.filter(field => scalar(field) && !special.has(field.name));

        it('has every field in one class, and no class names a field it does not have', () => {
            const names = fields.map(field => field.name);
            for (const name of special) assert.ok(names.includes(name), `${name} is not a field`);
            assert.equal(special.size, spec.evolved.length + spec.config.length + spec.refused.length + spec.structured.length);
            for (const field of fields.filter(candidate => !scalar(candidate))) assert.ok(spec.structured.includes(field.name), field.name);
        });

        it('keeps false, zero and a nondefault value', () => {
            for (const field of kept) {
                for (const value of valuesOf(field)) {
                    assert.equal(tick({ [field.name]: value })[field.name], value, `${field.name} ${value}`);
                }
            }
        });

        it('leaves an absent optional out of what it did not compute', () => {
            const empty = tick({});
            for (const field of kept.filter(candidate => candidate.optional && candidate.type.startsWith('number'))) {
                assert.notEqual(empty[field.name], 987.5, field.name);
                assert.equal(tick({ [field.name]: 1 })[field.name], 1, field.name);
            }
        });

        it('recomputes what it owns, and returns it', () => {
            for (const name of spec.evolved) assert.ok(name in tick({ [name]: fields.find(field => field.name === name).type.startsWith('boolean') ? true : 1 }), name);
        });

        it('takes what the caller keeps, and does not hand it back', () => {
            for (const name of spec.config) {
                const value = fields.find(field => field.name === name).type.startsWith('boolean') ? true : 1;
                assert.equal(tick({ [name]: value })[name], undefined, name);
            }
        });

        it('refuses what cannot be read', () => {
            for (const name of spec.refused) assert.throws(() => tick({ [name]: 0 }), /invalid/, name);
        });

        for (const name of spec.zeroMatters) {
            it(`tells an explicit zero ${name} from an absent one`, () => {
                const moving = { velocity: { x: 0.1, y: 0.1, z: 0 } };
                assert.notDeepEqual(tick({ ...moving, [name]: 0 }).velocity, tick(moving).velocity, name);
            });
        }
    });
}

describe('the table', () => {
    it('covers each family the contract gives a state of its own', () => {
        assert.deepEqual([...new Set(Object.values(STATES).map(({ family }) => family))].sort(), ['boat', 'environment', 'minecart', 'mount', 'projectile']);
    });
});
