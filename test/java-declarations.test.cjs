'use strict';

// B035: a method is read as a declaration of its class, with its signature, whatever comes before
// it and whatever is inside it.

const assert = require('node:assert/strict');
const { describe, it } = require('node:test');
const { methodDeclarations, methodsNamed, memberDeclarations } = require('../scripts/java-declarations.cjs');
const { extractMethod } = require('../scripts/generate-version-manifest.cjs');
const manifest = require('../generated/minecraft-versions.json');
const audit = require('../generated/physics-methods.json');

const source = [
    'package a;',
    '',
    '/** move(int x) { in a comment } */',
    'public abstract class Entity<T> extends Base implements Runnable {',
    '    private static final Runnable FIELD = new Runnable() {',
    '        public void move() {',
    '        }',
    '    };',
    '    private final int[] table = {1, 2};',
    '',
    '    public Entity(int value) {',
    '        this.move(MoverType.SELF, null);',
    '    }',
    '',
    '    void caller() {',
    '        move(MoverType.SELF, delta);',
    '        new Object() {',
    '            void move(int inner) {',
    '            }',
    '        };',
    '    }',
    '',
    '    @Override',
    '    public void move(final MoverType type, Vec3 delta) {',
    '        String text = "} move(";',
    '        char brace = \'{\';',
    '        if (type != null) {',
    '            this.other();',
    '        }',
    '    }',
    '',
    '    protected <U extends Comparable<U>> java.util.List<U> move(',
    '        java.util.Map<String, java.util.List<U>> values,',
    '        double... rest',
    '    ) throws java.io.IOException {',
    '        return null;',
    '    }',
    '',
    '    public abstract void declaredOnly(int x);',
    '',
    '    static enum Kind {',
    '        A,',
    '        B(1) {',
    '            void move() {',
    '            }',
    '        };',
    '',
    '        Kind() {',
    '        }',
    '',
    '        Kind(int x) {',
    '        }',
    '    }',
    '',
    '    static class Inner {',
    '        void move(String text) {',
    '        }',
    '    }',
    '}',
].join('\n');

describe('method declarations', () => {
    it('finds the declarations of the class, not its call sites, anonymous classes or nested classes', () => {
        const found = methodsNamed(source, 'move');
        assert.deepEqual(found.map(method => method.signature), ['(MoverType,Vec3)', '(Map,double...)'.replace('Map', 'java.util.Map')]);
        assert.equal(found.every(method => method.declaringClass === 'Entity'), true);
        assert.equal(found[0].line, 24);
        assert.equal(found[0].source.endsWith('        }\n    }'), true);
    });

    it('keeps a nested class\'s method apart, under its own class', () => {
        const inner = methodDeclarations(source).filter(method => method.declaringClass === 'Entity.Inner');
        assert.deepEqual(inner.map(method => `${method.name}${method.signature}`), ['move(String)']);
    });

    it('takes no constructor, enum constant, field initializer or bodiless method', () => {
        const names = methodDeclarations(source).map(method => `${method.declaringClass}#${method.name}`);
        assert.deepEqual(names, ['Entity#caller', 'Entity#move', 'Entity#move', 'Entity.Inner#move']);
    });

    it('fingerprints each overload, and the whole when any of them changes', () => {
        const before = extractMethod(source, 'move');
        const after = extractMethod(source.replace('double... rest', 'float... rest'), 'move');
        assert.equal(before.overloads, 2);
        assert.equal(before.declarations[0].sha256, after.declarations[0].sha256);
        assert.notEqual(before.declarations[1].sha256, after.declarations[1].sha256);
        assert.notEqual(before.sha256, after.sha256);
    });

    it('is not moved by a brace in a string, char or comment', () => {
        const quiet = source.replace('"} move("', '"plain"').replace("'{'", "'x'");
        assert.equal(methodsNamed(quiet, 'move')[0].source.split('\n').length, methodsNamed(source, 'move')[0].source.split('\n').length);
    });
});

describe('member declarations', () => {
    // B007: a gate whose change is a constant, a field's initializer, an enum constant or a constructor
    it('finds the fields, enum constants and constructors, not locals, methods or what an initializer holds', () => {
        const names = memberDeclarations(source).map(member => `${member.kind} ${member.declaringClass}#${member.name}`);
        assert.deepEqual(names, [
            'field Entity#FIELD', 'field Entity#table', 'constructor Entity#Entity',
            'constant Entity.Kind#A', 'constant Entity.Kind#B', 'constructor Entity.Kind#Kind', 'constructor Entity.Kind#Kind',
        ]);
    });

    it('keeps each declaration\'s whole text, its initializer and a constant\'s body too', () => {
        const members = memberDeclarations(source);
        const named = name => members.find(member => member.name === name);
        assert.equal(named('table').source, 'private final int[] table = {1, 2};');
        assert.equal(named('table').line, 9);
        assert.ok(named('FIELD').source.endsWith('};'));
        assert.ok(named('B').source.startsWith('B(1) {') && named('B').source.endsWith('}'));
        assert.ok(named('Entity').source.includes('this.move(MoverType.SELF, null);'));
    });
});

describe('the recorded method spans', () => {
    it('records each modern release\'s Entity.move as one declared signature, whole', () => {
        for (const version of manifest.versions) {
            const entry = manifest.sourceFiles[version];
            const move = entry.methods['net/minecraft/world/entity/Entity.java#move'];
            assert.equal(move.declarations.length, 1, version);
            assert.equal(move.declarations[0].declaringClass, 'Entity', version);
            assert.match(move.declarations[0].signature, /^\(MoverType,Vec3d?\)$/, version);
            const audited = audit.releases[version].files['Entity.java'].methods.move[0];
            assert.equal(move.declarations[0].sha256, audited.sha256, version);
            assert.ok(move.bytes > 1500, version);
        }
    });

    it('compares methods only within one decompile, and names the pairs it cannot compare', () => {
        const pairs = manifest.versions.slice(1).map((version, index) => [manifest.versions[index], version]);
        for (const [left, right] of pairs) {
            const key = `${left}->${right}`;
            const same = manifest.sourceFiles[left].decompile === manifest.sourceFiles[right].decompile;
            assert.equal(manifest.unavailableDiffs.includes(key), !same, key);
            if (!same) assert.equal(manifest.methodDiffs[key], undefined, key);
        }
    });

    it('records where each present file comes from', () => {
        for (const version of manifest.versions) {
            for (const file of manifest.sourceFiles[version].files.filter(candidate => candidate.present)) {
                assert.ok(file.source, `${version} ${file.path}`);
            }
        }
    });
});
