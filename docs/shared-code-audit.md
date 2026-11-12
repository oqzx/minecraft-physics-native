# Shared code audit

Every release has its own `Engine` and `Profile`, and every profile runs shared simulation, collision, vehicle and
projectile code. A release sharing that code is a claim that vanilla's behaviour is the same there, or that a gate
tells the releases apart. This records what has been checked.

## Coverage

`scripts/shared-gates.cjs` reads every `release.at_least(...)` in `native/src` outside tests and writes
`generated/shared-gates.json`: one entry per gated function, with its boundaries and a computed status.

| Status | Meaning |
| --- | --- |
| `source-compared` | At each boundary, a vanilla method the function stands for differs between the releases around it (`generated/gate-sources.json`). |
| `unverified` | At least one boundary, listed in `missing`, has no such finding. This is the incomplete marker: it claims nothing. |

A boundary below a registered release, such as a gate at 1.14, is read at the first release it affects, 1.14.4.

`scripts/compare-gate-sources.cjs` takes a function's methods from its citations, or from `audit/gate-source-reviews.json`,
where someone read both sides, and compares each between the two releases. A gate whose change is no class's but the release's
own version json (`declared_java_major`, the Java major it declares) names the key under `versionJson`, read in each
release's version json at the sha1 `generated/mojang-releases.json` pins, from `MC_CLIENT_JAR_ROOT` or `jar-inputs/clients`
beside the source cache. `generated/physics-breakpoints.json` could not:
it refuses a pair from different decompiles, and the source cache has two (a commit of the extracted repository, or
Vineflower over the client), which meet at 1.15, 1.18.1, 1.18.2, 1.19, 1.19.2, 1.19.3, 26.1.2 and 26.2. So the modern releases
are each put through one Vineflower (`--materialize`, into `compare-cache`). That takes the noise out: 1.18 to 1.18.1 and 1.19.1
to 1.19.2, which read as a third of the methods changed in the cache, read as none. What may differ is the list `ALLOWANCES`,
printed in the output, and a name the legacy mapping left as `method_123`.

`source-compared` says the method changed there. It does not show the gate does what changed, which is the review's and the
replays' to say. A review that named a method that turns out equal is listed under `refuted`, and gives nothing.

`test/shared-gates.test.cjs` fails when the list differs from the code, so a gate cannot be added, moved or removed
without the list changing in the same commit. That is the review point: an interval cannot widen unnoticed.

(The list first read 221 functions, because test code was cut from the first `#[cfg(test)]` in a file, which in `simulation.rs`
sits on one item at line 1287.)

## Ungated functions and callers

`scripts/shared-functions.cjs` covers every native function outside the tests, gated or not: 1292 at this writing. For each
it gives its callers, the contract families that reach it from their entry points, its interval (`every release` when it
has no gate) and a status. `node scripts/shared-functions.cjs <function>` prints one. Every function has the `unverified`
marker unless it is an exported entry or is `source-compared`.

It is computed when asked and not checked in, because it changes with nearly every native commit. Callers are matched by
name, so a name two functions share counts for both and a call through a pointer can be missed: it over-reports. Most
of the tick is reached by every family through the shared simulation, so family reach says where a change can land, not
that the family exercises that line. 258 functions are not reached from a contract entry point by this name match; why each is not has not been
classified, and some will be calls this match misses.

## Policy checks

The per-release policies are checked against each release's own source, and against a deliberately wrong profile:

| Policy | Check | Wrong-policy test |
| --- | --- | --- |
| Fall-damage rounding and formula | `scripts/verify-landing-policy.cjs` | `test/landing-policy.test.cjs`: a profile that rounds otherwise is rejected |
| Fluid gravity divisors | `scripts/verify-fluid-gravity-divisors.cjs` | `test/fluid-gravity-divisors.test.cjs`: a changed divisor is rejected |
| Block friction | `scripts/generate-block-friction.cjs` | `test/block-friction.test.cjs` |
| A gate moved in the engine | replay of its boundary | `docs/validation-records.md`: the 1.21.5 gate moved to 1.21.6 |

Identical hand-written profiles are not evidence: `verify-version-coverage.cjs` reads each module against its own
release, not against a neighbour.

## Generic dispatch

A named family that has its own integrator cannot reach another: arrow and trident names in the generic projectile
path are rejected or routed (B191), and `entityTravelSupport` reports `unsupported` for a kind the game has and the
engine does not move, instead of running it as something else (the recorder's `replay/verdict.ts` then calls such a tick
`unsupported`, not a match). Every kind of every release's registry is in the inventory (`test/mineflayer.test.cjs` holds it to
each release's dump), so `unknown` is a name outside the game, and a tick refuses it. A LivingEntity with no `travel` of its own
is `mob`, by name: no replay calls it `living` any more. The dragon is `dragon` (`simulateVersionedDragonTick`) and the shulker
`unsupported`: neither runs LivingEntity.travel to any end (`docs/mob-travel.md`). `entity` is no LivingEntity: a minecart, an item, a projectile.

## Not covered

- The ungated functions and the profile rules have a record of their own, `generated/shared-equivalence.json`, from
  `scripts/shared-equivalence.cjs` and held to the code by `test/shared-equivalence.test.cjs` (B280). Every profile rule
  (each `VersionPhysicsRules` field, the landing rounding, restitution and fall-damage choice) is read in the source of every
  release it serves, and all 24 agree. Of the ungated functions, those whose comments cite a vanilla method (or field) are
  compared at all 49 boundaries, and each change of a cited method is named by the hash of its two texts. A change is
  accounted for by reach (`generated/shared-reach.json`, `scripts/reach-probe.cjs`: the releases on which the Rust tests and
  a replay of the corpus of every release ran the function; a boundary none of those runs crosses), by a mechanical class
  (`renamed-locals`: the texts differ only in the names of locals, `this.` kept), or by a review in
  `audit/shared-equivalence-reviews/`: neutral for every caller (method level), or for this function `not-read`,
  `gated-caller`, `gated-callee`, `parameterised`, `equivalent`, or `no-engine-caller` (checked against the callers). A
  review lapses when either text changes, and the test reads the reviewed texts again where the compare trees are. A
  function that cites nothing is `helper` when a review names the Rust tests that call it or ran it, `glue` in a binding
  file, `jdk` when it cites Java platform methods only, and `uncited` otherwise. `node scripts/shared-equivalence.cjs --show
  <file:function>` prints a function's open changes as a reviewer reads them. Reach is what those runs did, not what a
  release can do: a function a release could reach on input no fixture gives is counted unreached there. The totals, and
  each function still open (`review`, `uncited`, `unresolved`), are in the record.
- The gate-mutation sweep (`docs/version-gates.md`, B290) ran against the whole corpus on all 50 releases: 489 of 781 gates
  are noticed on both sides, and the rest still want a fixture that notices them, or a reason from source that the move is
  behaviour-neutral.
