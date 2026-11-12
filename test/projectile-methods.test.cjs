'use strict';

// B040: the projectile sources of every release, thrown motion apart from arrows and tridents, and an
// entity's absence told from a method's. B033: the two loops that define block states.

const assert = require('node:assert/strict');
const { describe, it } = require('node:test');
const audit = require('../generated/physics-methods.json').releases;
const { registeredReleases, compareReleases } = require('../scripts/registered-releases.cjs');
const { stateDefinitionOf } = require('../scripts/block-state-fields.cjs');

const versions = registeredReleases().map(({ version }) => version);
const legacy = version => compareReleases(version, '1.14') < 0;

describe('the projectile inventory', () => {
    it('reads the arrow, thrown and accelerated families of every release', () => {
        for (const version of versions) {
            const release = audit[version];
            assert.ok(release.files['AbstractArrow.java']?.methods.tick?.length > 0, `${version} arrow`);
            assert.ok(release.files['ThrowableProjectile.java']?.methods.tick?.length > 0, `${version} thrown`);
            assert.ok(release.files['AbstractHurtingProjectile.java']?.methods.tick?.length > 0, `${version} fireball`);
        }
    });

    it('has the trident from 1.13 and calls it absent before, as a file and not a gap', () => {
        for (const version of versions) {
            const introduced = compareReleases(version, '1.13') >= 0;
            assert.equal(Boolean(audit[version].files['ThrownTrident.java']), introduced, version);
            assert.equal(audit[version].absentFiles.includes('ThrownTrident.java'), !introduced, version);
        }
    });

    it('has Projectile.java in every release: the interface of shoot, as 1.14.4 has it, until a shared base class', () => {
        // (the legacy releases' own decompiles name IProjectile as 1.14.4 does; the yarn trees read before did not)
        for (const version of versions) assert.ok(audit[version].files['Projectile.java'], version);
        for (const version of versions.filter(legacy)) assert.deepEqual(Object.keys(audit[version].files['Projectile.java'].methods), [], version);
    });

    it('tells a thrown tick from an arrow\'s, and keeps the impact callbacks', () => {
        const release = audit['26.3'].files;
        assert.notEqual(release['ThrowableProjectile.java'].methods.tick[0].sha256, release['AbstractArrow.java'].methods.tick[0].sha256);
        assert.deepEqual(release['AbstractArrow.java'].methods.onHitBlock.map(method => method.signature), ['(BlockHitResult)']);
        assert.ok(release['Projectile.java'].methods.onHitEntity.length > 0);
        assert.ok(release['ThrownTrident.java'].methods.findHitEntity.length > 0);
    });

    it('lists each target a present file does not declare', () => {
        for (const version of versions) {
            for (const [file, names] of Object.entries(audit[version].absentMethods)) {
                for (const name of names) assert.equal(audit[version].files[file].methods[name], undefined, `${version} ${file} ${name}`);
            }
        }
        assert.equal(audit['26.3'].absentMethods['Entity.java'], undefined);
    });

    it('reads a method by its class, not a method of the same name in a nested class', () => {
        for (const method of audit['26.3'].files['Entity.java'].methods.move) assert.equal(method.declaringClass, 'Entity');
    });
});

describe('state definitions built in a loop', () => {
    it('reads the multiface loop and a list\'s forEach instead of giving up', () => {
        const multiface = stateDefinitionOf(`class A {
            protected void createBlockStateDefinition(StateDefinition.Builder<Block, BlockState> builder) {
                for (Direction direction : DIRECTIONS) {
                    if (this.isFaceSupported(direction)) {
                        builder.add(getFaceProperty(direction));
                    }
                }
            }
        }`);
        assert.equal(multiface.faces, true);
        assert.equal(multiface.dynamic, false);
        const shelf = stateDefinitionOf(`class A {
            protected void createBlockStateDefinition(StateDefinition.Builder<Block, BlockState> builder) {
                builder.add(FACING);
                SLOT_OCCUPIED_PROPERTIES.forEach(value -> builder.add(value));
            }
        }`);
        assert.deepEqual(shelf.lists, ['SLOT_OCCUPIED_PROPERTIES']);
        assert.deepEqual(shelf.added, ['FACING']);
        assert.equal(shelf.dynamic, false);
    });

    it('still calls any other loop dynamic', () => {
        const other = stateDefinitionOf(`class A {
            protected void createBlockStateDefinition(StateDefinition.Builder<Block, BlockState> builder) {
                for (Foo foo : foos()) builder.add(foo.property());
            }
        }`);
        assert.equal(other.dynamic, true);
    });
});
