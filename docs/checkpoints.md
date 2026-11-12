# Checkpoints

`encodeCheckpoint(binding, { version, state, world, config })` gives a text, and `decodeCheckpoint(binding, text, version)`
gives those parts back. The run then goes on through any entry point: `test/state-contract.test.cjs` restores midway
on every registered release, compares every later state by its bits, and covers the object, persistent and batch paths.

## What a checkpoint holds

| part | form | exact because |
|---|---|---|
| schema | `CHECKPOINT_SCHEMA` | another number is refused |
| release | the exact string | another release is refused; no alias or neighbour is taken |
| packed layout | `nativePlayerLayout()` | another layout is refused |
| config | JSON | see the numbers below |
| state | JSON | entities, pistons, rockets, recorded draws and counters are fields of the state |
| world | bytes, base64 | `NativeWorld.checkpoint()`, restored by `nativeWorldFromCheckpoint`, in sorted order, with doubles by their bits |
| world digest | FNV-1a of the bytes | a world that does not hash to it is refused |

## Numbers

Finite numbers go as JSON, which round-trips a double in JavaScript. `-0`, NaN and the infinities go as
`{"$f64": "<16 hex digits>"}`. An absent optional stays absent: it is never written as `0` or `null`.

## Take the state from the right read

Use `NativePlayer.state()` or `simulateVersionedTickOutcome(...).state`. `simulateVersionedTick`'s result leaves out the
scene the next tick reads, so saving that alone cannot continue a run.

## What the caller still owns

A checkpoint is no input log. After restoring, each tick still takes its keys, look and the entities around it from
the caller. A `tickMove` not yet finished does not survive, as `setState` lets go of it: checkpoint between ticks.
The block registry and minecraft-data behind the world are not stored: the bytes hold resolved shapes, so a restored
world does not read them.

The recorder's recordings are another thing: logs of captured inputs and outputs, in the recorder's `schema/recording.ts`.

## Other families

A boat, a ridden mob, a minecart, a moving entity or a projectile is a plain state of its own type, so the same format
carries it. Name it with `kind` on both sides: a checkpoint of another kind is refused. `test/state-transitions.test.cjs`
restores each of them midway on every registered release and compares every later state by its bits: a boat, a horse, a
minecart, a snowball, an arrow, a trident and an item, a primed TNT and an orb, and a shulker lid.
