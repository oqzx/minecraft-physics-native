'use strict';

// B303: Player.stabAttack's recoil on the attacker. The server's copy of the player alone has one.

const assert = require('node:assert/strict');
const { it } = require('node:test');
const { LoadNativePhysics } = require('..');
const native = LoadNativePhysics();

const sprinting = (clientSide, stabHit) => ({
    position: { x: 0.5, y: 100, z: 0.5 },
    velocity: { x: 0.25, y: 0, z: -0.125 },
    halfWidth: Math.fround(0.3), height: Math.fround(1.8),
    onGround: false, collidedHorizontally: false, collidedVertically: false,
    isPlayer: true, clientSide, controlledByLocalInstance: true, sprinting: true,
    noGravity: true,
    ...(stabHit ? { stabHit } : {}),
});

const tick = (version, state) => native.simulateVersionedTick(version, new native.NativeWorld(), state, {});

/** The share of the horizontal motion the stab left, against the same tick without one. */
function recoil(version, clientSide, stabHit) {
    const hit = tick(version, sprinting(clientSide, stabHit));
    const plain = tick(version, sprinting(clientSide));
    return { share: hit.velocity.x / plain.velocity.x, sprinting: hit.sprinting, stabHit: hit.stabHit };
}

it('takes 0.6 on a server state, once up to 26.1.2, from 1.21.11', () => {
    // causeExtraKnockback(0.4F + knockback) (1.21.11 Player.java:1200, the jars of 26.1 to 26.1.2)
    for (const version of ['1.21.11', '26.1', '26.1.1', '26.1.2']) {
        for (const knockback of [undefined, 0, 1]) {
            const { share, sprinting: running } = recoil(version, false, { knockback });
            assert.ok(Math.abs(share - 0.6) < 1e-6, `${version} ${knockback} ${share}`);
            assert.equal(running, false, version);
        }
    }
});

it("takes it again for a weapon's knockback from 26.2", () => {
    // causeExtraKnockback(0.4F), then causeExtraKnockback(getKnockback), nothing at 0 (26.2 Player.java:1152-1153, 26.3 :1142-1143)
    for (const version of ['26.2', '26.3']) {
        assert.ok(Math.abs(recoil(version, false, {}).share - 0.6) < 1e-6, version);
        assert.ok(Math.abs(recoil(version, false, { knockback: 0.5 }).share - 0.36) < 1e-6, version);
    }
});

it('leaves a client alone, and a release before spears', () => {
    // STAB is all a client does (26.3 MultiPlayerGameMode.java:513-516); recordings show its motion and sprint untouched
    for (const version of ['1.21.11', '26.1', '26.3']) {
        const { share, sprinting: running } = recoil(version, true, { knockback: 1 });
        assert.equal(share, 1, version);
        assert.equal(running, true, version);
    }
    // the control: no spear before 1.21.11
    const { share, sprinting: running } = recoil('1.21.10', false, { knockback: 1 });
    assert.equal(share, 1);
    assert.equal(running, true);
});

it('takes it once', () => {
    const first = tick('26.3', sprinting(false, { knockback: 1 }));
    assert.equal(first.stabHit, undefined);
});
