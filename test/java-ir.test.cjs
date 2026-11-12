'use strict';

// B045: the typed form of a method. The methods are vanilla's own text, from the audit; the expected values are what the
// jar returned (native/fixtures/java-ir-26.3.txt, scripts/java-oracle/OracleIr.java), which native/src/oracle_tests.rs
// holds the engine to as well: source, form and engine answer the same inputs, and the steps between are compared too.

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { describe, it } = require('node:test');
const { parseMethod, evaluate, IrUnsupported } = require('../scripts/java-ir.cjs');

const root = path.resolve(__dirname, '..');
const fixture = fs.readFileSync(path.join(root, 'native', 'fixtures', 'java-ir-26.3.txt'), 'utf8').split('\n').filter(Boolean);
const f32 = Math.fround;

/** Vanilla's text of a method of 26.3, by file. */
function vanilla(file, name) {
    const source = fs.readFileSync(path.join(root, '..', '..', 'pr', 'mc-source', '26.3', 'client', 'net', 'minecraft', 'world', 'entity', file), 'utf8');
    const start = source.search(new RegExp(`\\n\\s+(?:public |protected |private |final |static )+[\\w.]+ ${name}\\(`));
    const open = source.indexOf('{', start);
    let depth = 0;
    for (let at = open; at < source.length; at += 1) {
        if (source[at] === '{') depth += 1;
        else if (source[at] === '}' && --depth === 0) return source.slice(start, at + 1).trim();
    }
    throw new Error(name);
}
const sourceAtHand = fs.existsSync(path.join(root, '..', '..', 'pr', 'mc-source', '26.3'));

const VEC3 = {
    'Vec3.lengthSqr': 'double', 'Vec3.normalize': 'Vec3', 'Vec3.scale': 'Vec3', 'Vec3.add': 'Vec3', 'Mth.sin': 'float', 'Mth.cos': 'float',
    'Vec3#x': 'double', 'Vec3#y': 'double', 'Vec3#z': 'double', 'Vec3#ZERO': 'Vec3',
};
const vec = (x, y, z) => ({ x, y, z });
const helpers = {
    'Vec3.lengthSqr': v => v.x * v.x + v.y * v.y + v.z * v.z,
    'Vec3.normalize': (v) => {
        const length = Math.sqrt(v.x * v.x + v.y * v.y + v.z * v.z);
        return length < 1.0E-5 ? vec(0, 0, 0) : vec(v.x / length, v.y / length, v.z / length);
    },
    'Vec3.scale': (v, [by]) => vec(v.x * by, v.y * by, v.z * by),
    'Vec3.new': (_, [x, y, z]) => vec(x, y, z),
    'Vec3.add': (v, [x, y, z]) => vec(v.x + x, v.y + y, v.z + z),
};
const environment = (self = {}, extra = {}) => ({
    self,
    statics: { 'Vec3.ZERO': vec(0, 0, 0) },
    call: (owner, name, receiver, args) => (extra[`${owner}.${name}`] ?? helpers[`${owner}.${name}`])(receiver, args),
});

const parse = (source, signatures = {}) => parseMethod(source, signatures);
const run = (source, args, signatures = {}, env = environment()) => evaluate(parse(source, signatures), args, env);

describe('the typed form', () => {
    it('makes each silent promotion an explicit cast', () => {
        const ir = parse('double f(float a, double b) { return a * b; }');
        const multiply = ir.body[0].value;
        assert.deepEqual([multiply.kind, multiply.operands, multiply.left.kind, multiply.left.implicit, multiply.left.to], ['binary', 'double', 'cast', true, 'double']);
        assert.equal(multiply.right.kind, 'name');
    });

    it('keeps a written cast apart from a silent one', () => {
        const ir = parse('double f(float a) { return (double)a; }');
        assert.deepEqual([ir.body[0].value.kind, ir.body[0].value.implicit], ['cast', false]);
    });

    it('does float arithmetic in float, and only then widens', () => {
        assert.equal(run('double f(float a, float b) { return a * b; }', [f32(0.1), f32(0.2)]).value, f32(f32(0.1) * f32(0.2)));
        assert.notEqual(run('double f(float a, float b) { return (double)a * (double)b; }', [f32(0.1), f32(0.2)]).value, f32(f32(0.1) * f32(0.2)));
    });

    it('reads a literal by its suffix: 0.1F is not 0.1', () => {
        assert.equal(run('double f() { return 0.1F; }', []).value, f32(0.1));
        assert.equal(run('double f() { return 0.1; }', []).value, 0.1);
        assert.equal(run('double f() { return (double)0.1F > 0.1 ? 1.0 : 0.0; }', []).value, 1);
    });

    it('casts to int as Java does: truncating, saturating, NaN to 0', () => {
        assert.equal(run('int f(double a) { return (int)a; }', [-1.9]).value, -1);
        assert.equal(run('int f(double a) { return (int)a; }', [3e10]).value, 2147483647);
        assert.equal(run('int f(double a) { return (int)a; }', [NaN]).value, 0);
    });

    it('takes branches in order and stops at an early return', () => {
        const source = 'double f(double a) { if (a < 0.0) { return -1.0; } double b = a * 2.0; if (b > 10.0) { return b; } return 0.0; }';
        assert.deepEqual(run(source, [-1]).trace.map(step => step.step), ['branch', 'return']);
        assert.deepEqual(run(source, [9]).trace.map(step => step.step), ['branch', 'local', 'branch', 'return']);
        assert.equal(run(source, [1]).value, 0);
    });

    it('short-circuits, so a call on the right of && does not run', () => {
        const calls = [];
        const signatures = { 'this.check': 'boolean', 'this.other': 'boolean' };
        const env = environment({}, { 'this.check': () => { calls.push('check'); return false; }, 'this.other': () => { calls.push('other'); return true; } });
        run('boolean f() { return this.check() && this.other(); }', [], signatures, env);
        assert.deepEqual(calls, ['check']);
    });

    it('sees a write in the read after it', () => {
        const signatures = { 'this#value': 'double' };
        const result = run('double f() { this.value = 2.0; double read = this.value * 3.0; return read; }', [], signatures, environment({ value: 1 }));
        assert.equal(result.value, 6);
        assert.deepEqual(result.trace.map(step => [step.step, step.value]), [['write', 2], ['local', 6], ['return', 6]]);
    });

    it('calls helpers in source order, arguments before the call', () => {
        const calls = [];
        const signatures = { 'this.a': 'double', 'this.b': 'double', 'this.c': 'double' };
        const record = name => (_, args) => { calls.push([name, ...args]); return calls.length; };
        const env = environment({}, { 'this.a': record('a'), 'this.b': record('b'), 'this.c': record('c') });
        run('void f() { this.c(this.a(), this.b()); }', [], signatures, env);
        assert.deepEqual(calls, [['a'], ['b'], ['c', 1, 2]]);
    });
});

describe('what it does not model it refuses, with a place', () => {
    const refused = (source, message, line, column, signatures = {}) => assert.throws(() => parse(source, signatures), error => {
        assert.ok(error instanceof IrUnsupported, error.message);
        assert.match(error.message, message);
        assert.equal(error.line, line);
        assert.equal(error.column, column);
        return true;
    });

    it('a loop, a switch, an increment, a compound assignment, a lambda and an array', () => {
        refused('void f() {\n   for (int i = 0; i < 2; i = i + 1) { }\n}', /for is not modelled at 2:4/, 2, 4);
        refused('void f() {\n   while (true) { }\n}', /while is not modelled/, 2, 4);
        refused('void f(int a) {\n   switch (a) { }\n}', /switch is not modelled/, 2, 4);
        refused('void f(int a) {\n   a++;\n}', /An increment is not modelled/, 2, 5);
        refused('void f(int a) {\n   a += 1;\n}', /A compound assignment is not modelled/, 2, 6);
        refused('void f(int a) {\n   int b = a[0];\n}', /An array access is not modelled/, 2, 13);
        refused('void f() {\n   int b = c -> 1;\n}', /A lambda is not modelled/, 2, 12);
    });

    it('a call or a field with no admitted signature', () => {
        refused('void f() {\n   this.unknown();\n}', /this\.unknown has no admitted signature/, 2, 16);
        refused('double f(Vec3 v) {\n   return v.w;\n}', /Vec3#w has no admitted type/, 2, 14, VEC3);
    });

    it('a narrowing Java would not do by itself', () => {
        refused('float f(double a) {\n   float b = a;\n   return b;\n}', /double does not widen to float/, 2, 15);
    });
});

describe('vanilla\'s own methods', { skip: !sourceAtHand }, () => {
    const inputs = fixture.filter(line => line.startsWith('input ')).map((line) => {
        const [left, right] = line.slice('input '.length).split(' = ');
        const [x, y, z, speed, yRot] = left.split(' ').map(Number);
        const [length, sin, cos, ox, oy, oz] = right.split(' ');
        return { x, y, z, speed: f32(Number(speed)), yRot: f32(Number(yRot)), length: Number(length), sin, cos, out: [ox, oy, oz].map(Number) };
    });

    it('holds a fixture with both paths in it', () => {
        assert.equal(inputs.length, 400);
        assert.ok(inputs.filter(row => row.sin === '-').length >= 50, 'the early return');
        assert.ok(inputs.filter(row => row.sin !== '-').length >= 300);
    });

    it('Entity.getInputVector: each result and each step between, what the jar gave, bit for bit', () => {
        const ir = parse(vanilla('Entity.java', 'getInputVector'), VEC3);
        for (const row of inputs) {
            // (Mth's table is the jar's: the fixture has what it answered, so this holds the method's own arithmetic)
            const mth = { 'Mth.sin': () => f32(Number(row.sin)), 'Mth.cos': () => f32(Number(row.cos)) };
            const { value, trace } = evaluate(ir, [vec(row.x, row.y, row.z), row.speed, row.yRot], environment({}, mth));
            assert.deepEqual([value.x, value.y, value.z], row.out, `${row.x} ${row.yRot}`);
            const local = name => trace.find(step => step.step === 'local' && step.name === name)?.value;
            assert.equal(local('length'), row.length);
            if (row.sin === '-') {
                assert.equal(local('sin'), undefined, 'an early return computes nothing after it');
                assert.equal(trace.at(-1).step, 'return');
            } else {
                assert.equal(local('sin'), f32(Number(row.sin)));
                assert.equal(local('cos'), f32(Number(row.cos)));
            }
        }
    });

    it('LivingEntity.getFrictionInfluencedSpeed: on the ground, either side of 0.6', () => {
        const signatures = { 'this.onGround': 'boolean', 'this.getSpeed': 'float', 'this.getFlyingSpeed': 'float' };
        const ir = parse(vanilla('LivingEntity.java', 'getFrictionInfluencedSpeed'), signatures);
        const rows = fixture.filter(line => line.startsWith('friction '));
        assert.equal(rows.length, 200);
        for (const row of rows) {
            const [left, right] = row.slice('friction '.length).split(' = ');
            const [friction, speed] = left.split(' ').map(Number);
            const env = environment({}, { 'this.onGround': () => true, 'this.getSpeed': () => f32(speed), 'this.getFlyingSpeed': () => assert.fail('off the ground') });
            assert.equal(evaluate(ir, [f32(friction)], env).value, f32(Number(right)), row);
        }
    });

    it('Entity.applyGravity: the read, the test and the write in order, and none of the write with no gravity', () => {
        const signatures = { 'this.getGravity': 'double', 'this.setDeltaMovement': 'void', 'this.getDeltaMovement': 'Vec3', 'Vec3.add': 'Vec3' };
        const ir = parse(vanilla('Entity.java', 'applyGravity'), signatures);
        const order = [];
        const env = gravity => environment({}, {
            'this.getGravity': () => { order.push('getGravity'); return gravity; },
            'this.getDeltaMovement': () => { order.push('getDeltaMovement'); return vec(0, 1, 0); },
            'this.setDeltaMovement': (_, [movement]) => { order.push(`set ${movement.y}`); },
        });
        evaluate(ir, [], env(0.08));
        assert.deepEqual(order, ['getGravity', 'getDeltaMovement', `set ${1 - 0.08}`]);
        order.length = 0;
        evaluate(ir, [], env(0));
        assert.deepEqual(order, ['getGravity']);
    });
});

describe('a projectile\'s methods', { skip: !sourceAtHand }, () => {
    const file = 'projectile/Projectile.java';
    const rows = fixture.filter(line => line.startsWith('shoot ')).map((line) => {
        const [left, right] = line.slice('shoot '.length).split(' = ');
        return { in: left.split(' ').map(Number), out: right.split(' ').map(Number) };
    });
    const signatures = {
        this: 'Projectile', 'Vec3.normalize': 'Vec3', 'Vec3.add': 'Vec3', 'Vec3.scale': 'Vec3', 'Vec3.horizontalDistance': 'double', 'Vec3#x': 'double', 'Vec3#y': 'double', 'Vec3#z': 'double',
        'Projectile#random': 'RandomSource', 'RandomSource.triangle': 'double', 'Projectile.getMovementToShoot': 'Vec3', 'Projectile.setDeltaMovement': 'void', 'Projectile#needsSync': 'boolean',
        'Mth.atan2': 'double', 'Projectile.setYRot': 'void', 'Projectile.setXRot': 'void', 'Projectile.getYRot': 'float', 'Projectile.getXRot': 'float', 'Projectile#yRotO': 'float', 'Projectile#xRotO': 'float',
    };

    it('Projectile.shoot: its motion, what it writes in order, and the rotation it reads from that motion', () => {
        const movementIr = parseMethod(vanilla(file, 'getMovementToShoot'), signatures);
        const shootIr = parseMethod(vanilla(file, 'shoot'), signatures);
        assert.equal(rows.length, 200);
        for (const row of rows) {
            const [x, y, z, pow] = row.in;
            const self = { random: {} };
            const writes = [];
            const extra = {
                'RandomSource.triangle': (_, [mean, spread]) => { assert.equal(spread, 0); return mean; },
                'Vec3.add': (v, [a, b, c]) => vec(v.x + a, v.y + b, v.z + c),
                'Projectile.getMovementToShoot': (_, args) => evaluate(movementIr, args, environment(self, extra)).value,
                'Projectile.setDeltaMovement': (_, [movement]) => { writes.push('setDeltaMovement'); self.movement = movement; },
                'Projectile.setYRot': (_, [value]) => { writes.push('setYRot'); self.yRot = value; },
                'Projectile.setXRot': (_, [value]) => { writes.push('setXRot'); self.xRot = value; },
                'Projectile.getYRot': () => self.yRot,
                'Projectile.getXRot': () => self.xRot,
                'Vec3.horizontalDistance': v => Math.sqrt(v.x * v.x + v.z * v.z),
                'Mth.atan2': (_, [a, b]) => Math.atan2(a, b),
            };
            evaluate(shootIr, [x, y, z, f32(pow), 0], environment(self, extra));
            assert.deepEqual([self.movement.x, self.movement.y, self.movement.z], row.out.slice(0, 3));
            assert.deepEqual(writes, ['setDeltaMovement', 'setYRot', 'setXRot']);
            assert.equal(self.needsSync, true);
            assert.equal(self.yRotO, self.yRot, 'read after the write');
            // (Mth.atan2 is an approximation of its own: native/src/oracle_tests.rs holds it to the jar by bits)
            assert.ok(Math.abs(self.yRot - row.out[3]) < 1e-3, `${self.yRot} ${row.out[3]}`);
            assert.ok(Math.abs(self.xRot - row.out[4]) < 1e-3, `${self.xRot} ${row.out[4]}`);
        }
    });

    it('refuses ThrowableProjectile.applyInertia at its loop, and says where', () => {
        const source = vanilla('projectile/ThrowableProjectile.java', 'applyInertia');
        assert.throws(() => parseMethod(source, { this: 'Throwable', 'Throwable.getDeltaMovement': 'Vec3', 'Throwable.position': 'Vec3', 'Throwable.isInWater': 'boolean' }), (error) => {
            assert.ok(error instanceof IrUnsupported);
            assert.match(error.message, /for is not modelled at 6:\d+/);
            return true;
        });
    });
});

describe('the fixture is the engine\'s too', () => {
    it('is read by the Rust test', () => {
        const tests = fs.readFileSync(path.join(root, 'native', 'src', 'oracle_tests.rs'), 'utf8');
        assert.match(tests, /java-ir-26\.3\.txt/);
    });
});
