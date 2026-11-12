'use strict';

// B040: each projectile the engine models, per release, down to the class that declares each step of its tick and impact,
// with a missing entity, a missing method and a missing source kept apart.

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { describe, it } = require('node:test');
const { KINDS, HOOKS, instanceFields, superclass, impactWrites } = require('../scripts/generate-projectile-capabilities.cjs');
const { registeredReleases, compareReleases } = require('../scripts/registered-releases.cjs');

const root = path.resolve(__dirname, '..');
const { releases } = JSON.parse(fs.readFileSync(path.join(root, 'generated', 'projectile-capabilities.json'), 'utf8'));
const versions = registeredReleases().map(({ version }) => version);

/** The kinds of native ProjectileKind, read from its enum. */
function nativeKinds() {
    const source = fs.readFileSync(path.join(root, 'native', 'src', 'projectile.rs'), 'utf8');
    const body = /pub enum ProjectileKind \{([\s\S]*?)\n\}/.exec(source)[1];
    return [...body.matchAll(/^\s+([A-Z][A-Za-z]+),$/gm)].map(match => match[1]);
}

const firstWith = kind => versions.find(version => releases[version][kind].status === 'extracted');

describe('the inventory', () => {
    it('has every kind the engine models, for every registered release', () => {
        assert.deepEqual(Object.keys(KINDS).sort(), nativeKinds().filter(kind => kind !== 'Generic').sort());
        assert.deepEqual(Object.keys(releases).sort(), [...versions].sort());
        for (const version of versions) assert.deepEqual(Object.keys(releases[version]).sort(), Object.keys(KINDS).sort(), version);
    });

    it('reads every entity from the release that adds it, and calls it absent before', () => {
        assert.deepEqual(Object.fromEntries(['SpectralArrow', 'DragonFireball', 'ShulkerBullet', 'LlamaSpit', 'Trident', 'WindCharge'].map(kind => [kind, firstWith(kind)])), {
            SpectralArrow: '1.9.4', DragonFireball: '1.9.4', ShulkerBullet: '1.9.4', LlamaSpit: '1.11.2', Trident: '1.13.2', WindCharge: '1.20.3',
        });
        assert.equal(releases['1.20.2'].WindCharge.status, 'absent-entity');
        assert.equal(releases['1.12.2'].Trident.status, 'absent-entity');
    });

    it('has a source of every release at hand, so nothing is unavailable here', () => {
        for (const version of versions) {
            for (const [kind, entry] of Object.entries(releases[version])) assert.notEqual(entry.status, 'unavailable-source', `${version} ${kind}`);
        }
    });

    it('resolves a tick and an impact for each entity, in a class of its own chain', () => {
        for (const version of versions) {
            for (const [kind, entry] of Object.entries(releases[version])) {
                if (entry.status !== 'extracted') continue;
                assert.ok(entry.methods.tick, `${version} ${kind} tick`);
                assert.ok(entry.chain.includes('Entity') || entry.chain.includes('net.minecraft.entity.Entity'), `${version} ${kind} ${entry.chain}`);
                for (const [hook, method] of Object.entries(entry.methods)) assert.ok(entry.chain.includes(method.owner), `${version} ${kind} ${hook}`);
                for (const hook of Object.keys(HOOKS)) assert.ok(entry.methods[hook] || entry.undeclared.includes(hook), `${version} ${kind} ${hook}`);
            }
        }
    });

    it('never takes a method of Entity for a projectile\'s own step', () => {
        for (const version of versions) {
            for (const [kind, entry] of Object.entries(releases[version])) {
                for (const [hook, method] of Object.entries(entry.methods ?? {})) {
                    // (Entity.getGravity is real from 1.20.5, the base of every entity's gravity)
                    if (hook !== 'tick' && hook !== 'getGravity') assert.notEqual(method.owner, 'Entity', `${version} ${kind} ${hook}`);
                }
            }
        }
    });
});

describe('the families stay apart', () => {
    const latest = releases['26.3'];

    it('thrown motion is ThrowableProjectile\'s, an arrow\'s and a trident\'s AbstractArrow\'s', () => {
        assert.deepEqual(latest.Snowball.chain.slice(-3), ['ThrowableProjectile', 'Projectile', 'Entity']);
        assert.equal(latest.Snowball.methods.tick.owner, 'ThrowableProjectile');
        assert.ok(latest.Arrow.chain.includes('AbstractArrow') && !latest.Arrow.chain.includes('ThrowableProjectile'));
        assert.equal(latest.Arrow.methods.tick.owner, 'Arrow');
        assert.equal(latest.Trident.methods.tick.owner, 'ThrownTrident');
        assert.equal(latest.Trident.methods.getWaterInertia.owner, 'ThrownTrident');
        assert.equal(latest.Trident.methods.onHitBlock.owner, 'AbstractArrow');
        assert.notEqual(latest.Arrow.methods.tick.sha256, latest.Snowball.methods.tick.sha256);
    });

    it('a hurting projectile takes its inertia from its own base', () => {
        assert.equal(latest.Fireball.methods.tick.owner, 'AbstractHurtingProjectile');
        assert.equal(latest.Fireball.methods.getInertia.owner, 'AbstractHurtingProjectile');
        assert.ok(latest.WitherSkull.chain.includes('AbstractHurtingProjectile'));
    });

    it('the hook and the rocket keep ticks of their own', () => {
        assert.equal(latest.FishingHook.methods.tick.owner, 'FishingHook');
        assert.equal(latest.Firework.methods.tick.owner, 'FireworkRocketEntity');
        assert.equal(latest.ShulkerBullet.methods.tick.owner, 'ShulkerBullet');
        assert.equal(latest.LlamaSpit.methods.tick.owner, 'LlamaSpit');
    });

    it('holds what stays with a projectile between ticks', () => {
        assert.ok(latest.Arrow.fields.AbstractArrow.includes('inGroundTime'), 'an arrow in the ground');
        assert.ok(latest.Trident.fields.ThrownTrident.includes('dealtDamage'), 'a trident\'s hit');
        assert.ok(latest.FishingHook.fields.FishingHook.includes('life') || latest.FishingHook.fields.FishingHook.includes('hookedIn'));
    });

    it('reads the legacy releases under the Mojang names their decompile has', () => {
        assert.equal(releases['1.12.2'].Arrow.class, 'Arrow');
        assert.equal(releases['1.12.2'].Arrow.methods.shoot.owner, 'AbstractArrow');
        // (1.8.8's one arrow class, which the mapping names AbstractArrow)
        assert.equal(releases['1.8.8'].Arrow.class, 'AbstractArrow');
    });
});

describe('what an impact writes, in order', () => {
    const writes = (version, kind, hook) => releases[version][kind].methods[hook].writes;
    const before = (list, first, second) => {
        assert.ok(list.includes(first), `${first} in ${list}`);
        assert.ok(list.includes(second), `${second} in ${list}`);
        assert.ok(list.indexOf(first) < list.indexOf(second), `${first} before ${second}: ${list}`);
    };

    it('an arrow stops in the block in vanilla\'s order', () => {
        assert.deepEqual(writes('26.3', 'Arrow', 'onHitBlock'), [
            'lastState=', 'super.onHitBlock()', 'setPos()', 'setDeltaMovement()', 'setInGround()', 'shakeTime=',
            'setCritArrow()', 'setPierceLevel()', 'setSoundEvent()', 'resetPiercedEntities()',
        ]);
    });

    it("changes order at 1.21.2, as native's arrow_flight says: the offset as motion before, the step back and a stop after", () => {
        let eras = 0;
        for (const version of versions) {
            const hook = releases[version].Arrow.methods.onHitBlock;
            if (!hook) continue; // (inside onHit until 1.16)
            eras += 1;
            assert.equal(hook.owner, 'AbstractArrow', version);
            if (compareReleases(version, '1.21.2') >= 0) before(hook.writes, 'setPos()', 'setDeltaMovement()');
            else before(hook.writes, 'setDeltaMovement()', 'setPosRaw()');
            before(hook.writes, 'super.onHitBlock()', 'setDeltaMovement()');
            before(hook.writes, 'setDeltaMovement()', 'setCritArrow()');
            assert.ok(hook.writes.some(write => write === 'setInGround()' || write === 'inGround='), version);
            assert.ok(hook.writes.includes('shakeTime='), version);
        }
        assert.ok(eras > 30);
        const native = fs.readFileSync(path.join(root, 'native', 'src', 'projectile.rs'), 'utf8').split(/\r?\n\s*\/\/\/ /).join(' ');
        assert.ok(native.includes('a hit moves the arrow to the hit, back by 0.05F against the sign'));
        assert.ok(native.includes("a hit sets the motion to the hit's offset"));
    });

    it('a trident marks its hit before it hurts, and an arrow goes by its pierce', () => {
        assert.equal(writes('26.3', 'Trident', 'onHitEntity')[0], 'dealtDamage=');
        assert.equal(releases['26.3'].Trident.methods.onHitEntity.owner, 'ThrownTrident');
        assert.ok(writes('26.3', 'Arrow', 'onHitEntity').includes('super.onHitEntity()'));
    });

    it('a thrown item is discarded by its own onHit, after the base\'s', () => {
        for (const kind of ['Snowball', 'Egg', 'EnderPearl', 'Potion', 'ExperienceBottle']) {
            const list = writes('26.3', kind, 'onHit');
            before(list, 'super.onHit()', 'discard()');
        }
    });

    it('reads a body by what it writes, not by what it compares', () => {
        const body = 'void f() { if (this.life == 3) { this.life = 0; } this.shake += 1; super.onHit(hit); this.setPos(a); this.getPos(); this.discard(); }';
        assert.deepEqual(impactWrites(body), ['life=', 'shake=', 'super.onHit()', 'setPos()', 'discard()']);
    });

});

describe('the readers', () => {
    it('takes the fields of the class, not a static, a local or a nested class\'s', () => {
        const source = ['class A extends B {', '   private static final int C = 1;', '   private int life;', '   protected final double drag = 0.5;', '   void tick() {', '      int local = 1;', '   }', '   static class N {', '      int inner;', '   }', '   boolean inGround;', '}'].join('\n');
        assert.deepEqual(instanceFields(source, 'A'), ['drag', 'inGround', 'life']);
    });

    it('reads a superclass through generics and a qualified name', () => {
        assert.equal(superclass('public class A<T> extends net.minecraft.B<T> implements C {', 'A'), 'B');
        assert.equal(superclass('public abstract class A implements C {', 'A'), null);
    });
});
