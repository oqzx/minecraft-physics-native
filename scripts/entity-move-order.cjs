// The order Entity.move runs its block callbacks in, read from the method itself. Each step is a call that hands an entity
// to a block or moves it again: the order is what a callback sees of the steps before it, a bounce read by a later speed
// factor, and it changes between releases. Yarn's names are mapped to Mojang's so that a change of names is not a change.
// (an entity's own call, plain or on `this`: Shapes.collide, a static of another class, is no step of the entity's)
const OWN = String.raw`(?:(?<![\w$.])|(?<=\bthis\.))`;
const STEPS = [
  ["collide", new RegExp(String.raw`${OWN}(?:collide|adjustMovementForCollisions)\(`, "g")],
  ["fallDamage", new RegExp(String.raw`${OWN}(?:checkFallDamage|fall)\(`, "g")],
  ["afterFallOn", /\.(?:updateEntityMovementAfterFallOn|updateEntityAfterFallOn|onEntityLand)\(/g],
  ["stepOn", /\.(?:stepOn|onSteppedOn)\(/g],
  ["inside", new RegExp(String.raw`${OWN}(?:tryCheckInsideBlocks|checkInsideBlocks|applyEffectsFromBlocks|checkBlockCollision|tryCheckBlockCollision)\(`, "g")],
  ["speedFactor", new RegExp(String.raw`${OWN}(?:getBlockSpeedFactor|getVelocityMultiplier)\(`, "g")],
];

/** The steps of a move's source in order. A step that repeats in place, as a branch of an if, is listed once. */
function moveOrder(source) {
  const hits = [];
  for (const [step, pattern] of STEPS) {
    for (const match of source.matchAll(pattern)) hits.push([match.index, step]);
  }
  hits.sort(([a], [b]) => a - b);
  return hits.map(([, step]) => step).filter((step, index, all) => step !== all[index - 1]);
}

/** Entity.move's own method of a release's audit entry, the two-argument one. */
function moveOf(release) {
  const methods = release.files["Entity.java"]?.methods?.move ?? [];
  return methods.find((method) => method.signature.includes(",")) ?? methods[0] ?? null;
}

module.exports = { moveOrder, moveOf };
