const assert = require("node:assert/strict");
const { LoadNativePhysics } = require("../dist/native");

const native = LoadNativePhysics();
const version = "1.20.6";
const simulateTick = (worldSnapshot, state, config) =>
    native.simulateVersionedTick(version, worldSnapshot, state, config);
const simulateTicks = (worldSnapshot, states, ticks, config) => states.map((state) => {
    let current = state;
    for (let index = 0; index < ticks; index += 1) {
        current = simulateTick(worldSnapshot, current, config);
    }
    return current;
});
const capabilities = native.versionCapabilities(1, 21, 4);
assert.equal(capabilities.waterMovementEfficiency, true);
assert.equal(native.generatedBlockHasTag("1.20.6", "oak_leaves", "leaves"), true);
assert.equal(native.generatedBlockHasTag("1.20.6", "stone", "leaves"), false);
assert.equal(native.generatedStateIndex("1.20.6", "stone", []), 0);
assert.equal(native.generatedBlockStateCount("1.20.6", "stone"), 1);
assert.equal(native.generatedFluidStateCount("1.20.6", "water"), 2);
assert.equal(native.generatedFluidStateCount("1.20.6", "flowing_water"), 16);
assert.equal(
  native.generatedFluidStateIndex("1.20.6", "flowing_water", [
    ["LEVEL", "1"],
    ["FALLING", "false"],
  ]),
  0,
);
assert.equal(
  native.generatedFluidStateIndex("1.20.6", "flowing_water", [
    ["LEVEL", "8"],
    ["FALLING", "true"],
  ]),
  15,
);
assert.deepEqual(native.generatedBlockDefaults("1.20.6", "oak_door"), [
    { name: "FACING", value: "Direction.NORTH" },
    { name: "OPEN", value: "false" },
    { name: "HINGE", value: "DoorHingeSide.LEFT" },
    { name: "POWERED", value: "false" },
    { name: "HALF", value: "DoubleBlockHalf.LOWER" },
]);
assert.ok(Array.isArray(native.generatedMethodShapeBoxes("1.20.6", "oak_door", "outline")));
assert.equal(native.generatedExactMethodShapeBoxes("1.20.6", "oak_door", "outline"), null);
assert.ok(Array.isArray(native.generatedExactMethodShapeBoxes("1.20.6", "air", "outline")));
assert.ok(Array.isArray(native.generatedExactMethodShapeBoxesForState("1.20.6", "air", "outline", [])));
assert.equal(native.generatedExactMethodShapeBoxesForState("1.20.6", "air", "outline", [{ name: "UNKNOWN", value: "x" }]), null);
assert.equal(native.generatedExactMethodShapeBoxesForState("1.20.6", "oak_door", "outline", [{ name: "OPEN", value: "false" }]), null);
assert.equal(native.generatedMethodShapeBoxes("1.20.6", "oak_door", "invalid"), null);
assert.equal(native.generatedMethodShapeBoxes("1.20.6", "stone", "interaction"), null);
assert.deepEqual(native.generatedShapePropertyDependencies("1.20.6", "oak_door"), [
  "FACING",
  "HALF",
  "HINGE",
  "OPEN",
  "POWERED",
]);
assert.deepEqual(native.generatedMethodShapePropertyDependencies("1.20.6", "oak_door", "outline"), [
  "FACING",
  "HINGE",
  "OPEN",
]);
assert.deepEqual(native.generatedMethodShapePredicates("1.20.6", "oak_door", "outline"), [
  { property: "HINGE", operator: "==", value: "DoorHingeSide.RIGHT" },
]);
assert.equal(native.generatedMethodShapePredicatesMatchState("1.20.6", "oak_door", "outline", [
  { name: "FACING", value: "north" },
  { name: "OPEN", value: "false" },
  { name: "HINGE", value: "right" },
  { name: "POWERED", value: "false" },
  { name: "HALF", value: "lower" },
]), true);
assert.equal(native.generatedMethodShapePredicatesMatchState("1.20.6", "oak_door", "outline", [
  { name: "FACING", value: "north" },
  { name: "OPEN", value: "false" },
  { name: "HINGE", value: "left" },
  { name: "POWERED", value: "false" },
  { name: "HALF", value: "lower" },
]), false);
assert.equal(native.generatedMethodShapePredicatesMatchState("1.20.6", "oak_door", "outline", [
  { name: "OPEN", value: "false" },
]), null);
assert.equal(native.generatedMethodShapeIsStateDependent("1.20.6", "oak_door", "outline"), true);
assert.equal(native.generatedMethodShapeIsStateDependent("1.20.6", "oak_door", "collision"), null);
assert.equal(native.generatedMethodShapeIsStateDependent("1.20.6", "oak_door", "unknown"), null);
assert.deepEqual(native.generatedMethodShapeProvenance("1.20.6", "oak_door", "collision"), {
  hasMethod: false,
  stateDependent: false,
  propertyDependencies: [],
  candidateBoxCount: 0,
});
const outlineProvenance = native.generatedMethodShapeProvenance("1.20.6", "oak_door", "outline");
assert.equal(outlineProvenance.hasMethod, true);
assert.equal(outlineProvenance.stateDependent, true);
assert.deepEqual(outlineProvenance.propertyDependencies, ["FACING", "HINGE", "OPEN"]);
assert.ok(outlineProvenance.candidateBoxCount > 0);
const world = new native.NativeWorld();
world.setWorldBounds(-64, 320, 2, -3, 100);
assert.deepEqual(world.worldBounds(), {
    minBuildHeight: -64,
    maxBuildHeight: 320,
    borderCenterX: 2,
    borderCenterZ: -3,
    borderSize: 100,
});
world.setFullBlock(0, 0, 0, 1);
assert.equal(world.isSectionPresent(0, 0, 0), true);
assert.equal(world.sectionBlockCount(0, 0, 0), 1);
world.setSectionPresent(0, 2, 0, true);
assert.equal(world.isSectionPresent(0, 2, 0), true);
assert.equal(world.sectionBlockCount(0, 2, 0), 0);
world.setSectionPresent(0, 2, 0, false);
world.setBlockStateId(0, 0, 0, "1.20.6", 1);
world.setBlockEntity(0, 0, 0, "minecraft:campfire");
assert.throws(
    () => world.setBlockStateId(0, 0, 0, "1.21.99", 1),
    /not present in the source-backed release registry/,
);
world.setBlockProperties(0, 0, 0, [{ name: "facing", value: "north" }]);
assert.throws(
    () => world.setBlockProperties(0, 0, 0, [
        { name: "facing", value: "north" },
        { name: "facing", value: "south" },
    ]),
    /block property names must be unique/,
);
assert.equal(world.fluidAt(0, 0, 0), 0);
assert.equal(world.blockEffectAt(0, 0, 0), 0);
assert.deepEqual(world.blockAt(0, 0, 0), {
    present: true,
    id: 1,
    stateVersion: "1.20.6",
    blockEntity: "minecraft:campfire",
    properties: [{ name: "facing", value: "north" }],
    tags: 0,
    fluid: 0,
    fluidTags: 0,
    fluidLevel: 0,
    fluidSource: false,
    fluidFlow: { x: 0, y: 0, z: 0 },
    fluidFalling: false,
    fallDamageResetting: false,
    effect: 0,
    slipperiness: 0.6,
    bounceRestitution: 0,
    suppressesBounce: false,
    shape: { min: { x: 0, y: 0, z: 0 }, max: { x: 1, y: 1, z: 1 } },
    outlineShape: { min: { x: 0, y: 0, z: 0 }, max: { x: 1, y: 1, z: 1 } },
    interactionShape: { min: { x: 0, y: 0, z: 0 }, max: { x: 1, y: 1, z: 1 } },
});
world.applyBlockUpdates([
    { x: 1, y: 0, z: 0, id: 42, stateVersion: "1.20.6", properties: [{ name: "level", value: "4" }], fluid: 1, fluidTags: 3 },
    { x: 2, y: 0, z: 0, fluid: 1, fluidLevel: 4, fluidSource: false },
    { x: 5, y: 0, z: 0, fluid: 1, fluidLevel: 12, fluidSource: false, fluidFalling: true },
]);
assert.deepEqual(world.blockAt(1, 0, 0).properties, [{ name: "level", value: "4" }]);
assert.equal(world.blockAt(1, 0, 0).stateVersion, "1.20.6");
world.removeBlockEntity(0, 0, 0);
assert.equal(world.blockAt(0, 0, 0).blockEntity, undefined);
world.setBlockEntity(0, 0, 0, "minecraft:campfire");
world.applyBlockUpdates([{ x: 0, y: 0, z: 0, id: 1 }]);
assert.equal(world.blockAt(0, 0, 0).blockEntity, "minecraft:campfire");
world.applyBlockUpdates([{ x: 0, y: 0, z: 0, removeBlockEntity: true }]);
assert.equal(world.blockAt(0, 0, 0).blockEntity, undefined);
world.setBlockProperties(0, 0, 0, [{ name: "lit", value: "true" }]);
world.applyBlockUpdates([{ x: 0, y: 0, z: 0, id: 2 }]);
assert.deepEqual(world.blockAt(0, 0, 0).properties, [{ name: "lit", value: "true" }]);
assert.equal(world.fluidTagsAt(1, 0, 0), 3);
world.setFluidState(7, 0, 0, 1, 9, 5, false, false, { x: 0.25, y: 0, z: -0.5 });
assert.equal(world.blockAt(7, 0, 0).fluidTags, 9);
assert.equal(world.blockAt(7, 0, 0).fluidLevel, 5);
assert.deepEqual(world.blockAt(7, 0, 0).fluidFlow, { x: 0.25, y: 0, z: -0.5 });
assert.throws(
    () => world.setFluidState(8, 0, 0, 1, 0, 16, false, false, { x: 0, y: 0, z: 0 }),
    /fluid level must be between 0 and 15/,
);
world.applyBlockUpdates([
    {
        x: 6, y: 0, z: 0, id: 6,
        shape: { minX: 0, minY: 0, minZ: 0, maxX: 0.25, maxY: 1, maxZ: 1, slipperiness: 0.6 },
        outlineShape: { minX: 0, minY: 0, minZ: 0, maxX: 0.5, maxY: 1, maxZ: 1, slipperiness: 0.6 },
        interactionShape: { minX: 0, minY: 0, minZ: 0, maxX: 0.75, maxY: 1, maxZ: 1, slipperiness: 0.6 },
    },
    {
        x: 6, y: 0, z: 0,
        shape: { minX: 0.75, minY: 0, minZ: 0, maxX: 1, maxY: 1, maxZ: 1, slipperiness: 0.6 },
        outlineShape: { minX: 0.5, minY: 0, minZ: 0, maxX: 1, maxY: 1, maxZ: 1, slipperiness: 0.6 },
        interactionShape: { minX: 0.75, minY: 0, minZ: 0, maxX: 1, maxY: 1, maxZ: 1, slipperiness: 0.6 },
    },
]);
assert.equal(world.collisionShapes({ x: 6, y: 0, z: 0 }, { x: 7, y: 1, z: 1 }).length, 2);
assert.equal(world.outlineShapes({ x: 6, y: 0, z: 0 }, { x: 7, y: 1, z: 1 }).length, 2);
assert.equal(world.interactionShapes({ x: 6, y: 0, z: 0 }, { x: 7, y: 1, z: 1 }).length, 2);
assert.equal(world.blockAt(1, 0, 0).fluidSource, true);
assert.equal(world.blockAt(1, 0, 0).fluidLevel, 0);
assert.equal(world.fluidHeightAt(1, 0, 0), 1);
assert.equal(world.fluidHeightAt(2, 0, 0), 4 / 9);
assert.equal(world.fluidHeightAt(5, 0, 0), 4 / 9);
assert.throws(
    () => world.applyBlockUpdates([{ x: 3, y: 0, z: 0, fluid: 0, fluidSource: true }]),
    /empty fluid states cannot be marked as source fluids/,
);
assert.throws(
    () => world.applyBlockUpdates([{ x: 4, y: 0, z: 0, fluid: 1, fluidSource: true, fluidFalling: true }]),
    /source fluid states cannot be marked as falling/,
);
assert.equal(world.collisionShapes({ x: 0, y: 0, z: 0 }, { x: 1, y: 1, z: 1 }).length, 1);
assert.throws(
    () => world.collisionShapes({ x: Number.NaN, y: 0, z: 0 }, { x: 1, y: 1, z: 1 }),
    /collision query bounds must be finite and ordered/,
);

const initialState = {
    position: { x: 0.5, y: 1.9, z: 0.5 },
    velocity: { x: 0, y: -1, z: 0 },
    halfWidth: 0.3,
    height: 1.8,
    onGround: false,
    collidedHorizontally: false,
    collidedVertically: false,
};
assert.throws(
    () => native.simulateTick(world, initialState, {}),
    /concrete Minecraft version is required/,
);
assert.throws(
    () => native.simulateTicks(world, [initialState], 1, {}),
    /concrete Minecraft version is required/,
);
native.simulateTick = simulateTick;
native.simulateTicks = simulateTicks;

const constructedState = native.createSimulationState({
    position: { x: 0.5, y: 1.9, z: 0.5 },
    velocity: { x: 0, y: -1, z: 0 },
    halfWidth: 0.3,
    height: 1.8,
    onGround: false,
    yaw: 0,
    inWater: false,
    inLava: false,
    pose: 5,
    input: { x: 1, y: 0, z: 0 },
    jumpRequested: true,
    sprinting: true,
    pitch: 0,
});
assert.equal(constructedState.position.x, 0.5);
assert.equal(constructedState.height, 1.5);
assert.equal(constructedState.sprinting, true);
assert.equal(constructedState.input.x, 1);
assert.equal(constructedState.jumpRequested, true);
assert.equal(constructedState.yaw, 0);
assert.equal(native.createSimulationState({
    position: { x: 0, y: 0, z: 0 },
    velocity: { x: 0, y: 0, z: 0 },
    halfWidth: 0.3,
    height: 1.8,
    onGround: false,
    yaw: 0,
    inWater: false,
    inLava: false,
    version: "1.14.4",
    pose: 6,
}).height, Math.fround(0.2));
assert.equal(native.createSimulationState({
    position: { x: 0, y: 0, z: 0 },
    velocity: { x: 0, y: 0, z: 0 },
    halfWidth: 0.3,
    height: 1.8,
    onGround: false,
    yaw: 0,
    inWater: false,
    inLava: false,
    version: "1.20.6",
    pose: 7,
}).height, Math.fround(0.2));
assert.throws(
    () => native.createSimulationState({
        position: { x: 0, y: 0, z: 0 },
        velocity: { x: 0, y: 0, z: 0 },
        halfWidth: 0.3,
        height: 1.8,
        onGround: false,
        yaw: 0,
        inWater: false,
        inLava: false,
        version: "1.14.4",
        pose: 7,
    }),
    /dying pose id 7 is unavailable for this version/,
);
assert.equal(native.createSimulationState({
    position: { x: 0, y: 0, z: 0 },
    velocity: { x: 0, y: 0, z: 0 },
    halfWidth: 0.3,
    height: 1.8,
    onGround: false,
    yaw: 0,
    inWater: false,
    inLava: false,
    version: "1.17.1",
    pose: 6,
}).height, Math.fround(1.8));
// Player.getDimensions(pose) sizes are floats (1.14 PlayerEntity.java:109-118); poses come with 1.14
assert.throws(
    () => native.createSimulationState({
        position: { x: 0, y: 0, z: 0 },
        velocity: { x: 0, y: 0, z: 0 },
        halfWidth: 0.3,
        height: 1.8,
        onGround: false,
        yaw: 0,
        inWater: false,
        inLava: false,
        version: "1.13.2",
        pose: 0,
    }),
    /entity poses are unavailable before 1\.14/,
);
assert.throws(
    () => native.createSimulationState({
        position: { x: 0, y: 0, z: 0 },
        velocity: { x: 0, y: 0, z: 0 },
        halfWidth: 0.3,
        height: 1.8,
        onGround: false,
        yaw: 0,
        inWater: false,
        inLava: false,
        pose: 6,
    }),
    /long-jumping pose is unavailable for this version/,
);
assert.throws(
    () => native.createSimulationState({
        position: { x: 0, y: 0, z: 0 },
        velocity: { x: 0, y: 0, z: 0 },
        halfWidth: 0,
        height: 1.8,
        onGround: false,
        yaw: 0,
        inWater: false,
        inLava: false,
        pose: 0,
    }),
    /entity descriptor contains invalid values/,
);

const state = native.simulateTick(world, initialState, {});
assert.equal(state.onGround, true);
assert.equal(state.position.y, 1);

const yawState = native.simulateTick(world, {
    ...initialState,
    position: { x: 0.5, y: 2, z: 0.5 },
    velocity: { x: 0, y: 0, z: 0 },
    yaw: Math.PI / 2,
    input: { x: 1, y: 0, z: 0 },
}, {});
assert.ok(yawState.position.z > 0.5);

const states = native.simulateTicks(world, [initialState, initialState], 1, {});
assert.equal(states.length, 2);
assert.equal(states[0].onGround, true);
assert.equal(states[1].onGround, true);

const replayStart = {
    ...initialState,
    position: { x: 0.5, y: 8, z: -2.5 },
    velocity: { x: 0.03, y: 0, z: -0.02 },
    input: { x: 0.4, y: 0, z: 0.7 },
};
const bulkReplay = native.simulateTicks(world, [replayStart], 200, {})[0];
let singleReplay = replayStart;
for (let index = 0; index < 200; index += 1) {
    singleReplay = native.simulateTick(world, singleReplay, {});
}
assert.deepEqual(bulkReplay, singleReplay);

world.setFluid(0, 1, 0, 1);
const fluidState = native.simulateTick(world, {
    ...initialState,
    position: { x: 0.5, y: 1.5, z: 0.5 },
    velocity: { x: 0, y: 0, z: 0 },
}, {});
assert.equal(fluidState.inWater, true);
assert.equal(fluidState.inLava, false);
world.setFluid(0, 1, 0, 3);
const waterloggedState = native.simulateTick(world, {
    ...initialState,
    position: { x: 0.5, y: 1.5, z: 0.5 },
    velocity: { x: 0, y: 0, z: 0 },
}, {});
assert.equal(waterloggedState.inWater, true);
assert.equal(world.fluidAt(0, 1, 0), 3);
assert.equal(world.blockAt(0, 1, 0).fluid, 3);

world.removeBlock(0, 0, 0);
world.setBlockShape(0, 0, 0, 2, {
    minX: 0,
    minY: 0,
    minZ: 0,
    maxX: 1,
    maxY: 0.5,
    maxZ: 1,
    slipperiness: 0.98,
    effect: 0,
});
world.setBlockShapes(4, 0, 0, 4, [
    {
        minX: 0,
        minY: 0,
        minZ: 0,
        maxX: 0.25,
        maxY: 1,
        maxZ: 1,
        slipperiness: 0.6,
    },
    {
        minX: 0.75,
        minY: 0,
        minZ: 0,
        maxX: 1,
        maxY: 1,
        maxZ: 1,
        slipperiness: 0.6,
    },
]);
assert.deepEqual(
    world.collisionShapes({ x: 4, y: 0, z: 0 }, { x: 5, y: 1, z: 1 }),
    [
        { min: { x: 4, y: 0, z: 0 }, max: { x: 4.25, y: 1, z: 1 } },
        { min: { x: 4.75, y: 0, z: 0 }, max: { x: 5, y: 1, z: 1 } },
    ],
);
world.applyBlockUpdates([
    { x: 1, y: 0, z: 0, id: 3 },
    { x: 2, y: 0, z: 0, fluid: 2 },
    { x: 3, y: 0, z: 0, remove: true },
]);
const steppedState = native.simulateTick(world, {
    ...initialState,
    position: { x: 0.5, y: 0.5, z: 0.5 },
    velocity: { x: 0.2, y: 0, z: 0 },
    onGround: true,
}, { stepHeight: 0.6 });
assert.equal(steppedState.onGround, true);

world.setBlockShape(0, 0, 0, 2, {
    minX: 0,
    minY: 0,
    minZ: 0,
    maxX: 1,
    maxY: 1,
    maxZ: 1,
    slipperiness: 0.6,
    effect: 3,
});
const slimeBounceState = native.simulateTick(world, {
    ...initialState,
    position: { x: 0.5, y: 1, z: 0.5 },
    velocity: { x: 0, y: -0.2, z: 0 },
    onGround: true,
}, {});
assert.ok(slimeBounceState.velocity.y > 0);

const boatState = native.simulateBoatTick({
    position: { x: 0, y: 0, z: 0 },
    velocity: { x: 0, y: 0, z: 0 },
    yaw: 0,
    forward: true,
    left: false,
    right: true,
    inputDown: false,
    inWater: true,
});
// Boat.controlBoat: the right key turns the boat a degree (deltaRotation++), forward pushes it along cos(yRot) in z
assert.ok(boatState.position.z > 0);
assert.ok(boatState.yaw > 0);
assert.equal(boatState.yawDegrees, 1);
assert.equal(boatState.deltaRotation, 1);
const batchedBoatStates = native.simulateVersionedBoatTicks("1.17.1", world, [
    boatState,
    { ...boatState, forward: false, right: false },
], 2);
assert.equal(batchedBoatStates.length, 2);
// (no water here: in the air floatBoat keeps 0.9F of its motion, and the forward key pushes it on along cos(yRot), so
// the steered boat goes furthest in z; 1.17.1 BoatEntity.java floatBoat, controlBoat)
assert.equal(batchedBoatStates[0].status, "in_air");
assert.ok(batchedBoatStates[1].position.z > boatState.position.z);
assert.ok(batchedBoatStates[0].position.z > batchedBoatStates[1].position.z);
assert.throws(
    () => native.simulateVersionedBoatTicks("1.99.1", world, [boatState], 1),
    /unsupported Minecraft version for boat simulation/,
);
assert.throws(() => native.simulateBoatTick({
    position: { x: Number.NaN, y: 0, z: 0 },
    velocity: { x: 0, y: 0, z: 0 },
    yaw: 0,
    forward: false,
    left: false,
    right: false,
    inWater: true,
}), /boat simulation state contains invalid values/);

const horseState = native.simulateHorseTick({
    position: { x: 0, y: 0, z: 0 },
    velocity: { x: 0, y: 0, z: 0 },
    yaw: 0,
    forward: 1,
    strafe: 0,
    jumpRequested: true,
    onGround: true,
    inWater: false,
    movementSpeed: 0.1,
    jumpStrength: 0.42,
});
assert.ok(horseState.position.z > 0);
assert.ok(horseState.position.y > 0);
assert.equal(horseState.onGround, false);
const batchedHorseStates = native.simulateVersionedHorseTicks("1.17.1", world, [
    horseState,
    { ...horseState, forward: 0, jumpRequested: false },
], 2);
assert.equal(batchedHorseStates.length, 2);
assert.throws(
    () => native.simulateVersionedHorseTicks("1.99.1", world, [horseState], 1),
    /unsupported Minecraft version for horse simulation/,
);
assert.throws(() => native.simulateHorseTick({
    position: { x: 0, y: 0, z: 0 },
    velocity: { x: 0, y: 0, z: 0 },
    yaw: Number.NaN,
    forward: 0,
    strafe: 0,
    jumpRequested: false,
    onGround: false,
    inWater: false,
    movementSpeed: 0.1,
    jumpStrength: 0.42,
}), /horse simulation state contains invalid values/);

const projectileWorld = new native.NativeWorld();
projectileWorld.setFullBlock(2, 0, 0, 1);
const bubbleWorld = new native.NativeWorld();
// a bubble column has an empty collision shape (BubbleColumnBlock), which only carries the effect
bubbleWorld.applyBlockUpdates([{
    x: 0, y: 0, z: 0, id: 1,
    shape: {
        minX: 0, minY: 0, minZ: 0, maxX: 0, maxY: 0, maxZ: 0,
        slipperiness: 0.6, effect: 6,
    },
}]);
const legacyBubbleTick = native.simulateVersionedProjectileTicks("1.20.6", bubbleWorld, [{
    position: { x: 0.5, y: 0.1, z: 0.5 },
    velocity: { x: 0, y: 0, z: 0 },
    gravity: 0.03,
    airDrag: 1,
}], 1)[0];
const modernBubbleTick = native.simulateVersionedProjectileTicks("26.2", bubbleWorld, [{
    position: { x: 0.5, y: 0.1, z: 0.5 },
    velocity: { x: 0, y: 0, z: 0 },
    gravity: 0.03,
    airDrag: 1,
}], 1)[0];
// 1.20.6 ThrownEntity.tick: checkBlockCollision pushes up by the column under air
// (min(1.8, 0 + 0.1), 1.20.5 ThrownEntity.java tick, BubbleColumnBlock.java:46-49,
// Entity.java:2305-2316), the move takes that motion, then the gravity
assert.equal(legacyBubbleTick.position.y, 0.1 + 0.1);
assert.equal(legacyBubbleTick.velocity.y, 0.1 - 0.03);
// 26.2: the first tick's column, the gravity, the move, then the column again
// (applyEffectsFromBlocks, 1.21.2 ThrowableProjectile.java:43-57)
assert.equal(modernBubbleTick.position.y, 0.17);
assert.equal(modernBubbleTick.velocity.y, 0 + 0.1 - 0.03 + 0.1);
const projectile = native.simulateProjectileTick(projectileWorld, {
    position: { x: 0.5, y: 0.5, z: 0.5 },
    velocity: { x: 3, y: 0, z: 0 },
    gravity: 0,
    airDrag: 1,
    kind: "ender_pearl",
    horizontalCollision: false,
    shotAtAngle: false,
});
assert.equal(projectile.blockHit, true);
const projectileBatch = native.simulateVersionedProjectileTicks("1.20.6", projectileWorld, [{
    position: { x: 0.5, y: 0.5, z: 0.5 },
    velocity: { x: 0.1, y: 0, z: 0 },
    gravity: 0,
    airDrag: 1,
    kind: "snowball",
}], 2);
assert.equal(projectileBatch.length, 1);
const projectileSingle = native.simulateProjectileTick(projectileWorld, {
    position: { x: 0.5, y: 0.5, z: 0.5 },
    velocity: { x: 0.1, y: 0, z: 0 },
    gravity: 0,
    airDrag: 1,
    kind: "snowball",
});
const projectileSingleSecond = native.simulateProjectileTick(projectileWorld, {
    position: projectileSingle.position,
    velocity: projectileSingle.velocity,
    gravity: 0,
    airDrag: 1,
    firstTick: projectileSingle.firstTick,
    kind: "snowball",
});
assert.deepEqual(projectileBatch[0], projectileSingleSecond);
assert.throws(
    () => native.simulateVersionedProjectileTicks("1.99.1", projectileWorld, [], 1),
    /unsupported Minecraft version for projectile simulation/,
);
assert.throws(
    () => native.simulateProjectileTick(projectileWorld, {
        position: { x: 0, y: 0, z: 0 },
        velocity: { x: 0, y: 0, z: 0 },
        kind: "unknown_projectile_family",
    }),
    /unsupported projectile family/,
);
assert.equal(projectile.position.x, 2);
assert.equal(projectile.firstTick, false);
const firework = native.simulateProjectileTick(projectileWorld, {
    position: { x: 0.5, y: 0.5, z: 0.5 },
    velocity: { x: 1, y: 0, z: 1 },
    gravity: 0,
    airDrag: 1,
    kind: "firework",
    horizontalCollision: false,
    shotAtAngle: false,
});
assert.equal(firework.velocity.x, 1.15);
assert.equal(firework.velocity.z, 1.15);
assert.equal(firework.velocity.y, 0.04);
const fireball = native.simulateProjectileTick(projectileWorld, {
    position: { x: 0.5, y: 0.5, z: 0.5 },
    velocity: { x: 1, y: 0, z: 0 },
    airDrag: 0.95,
    kind: "fireball",
    power: { x: 0.1, y: 0, z: 0 },
});
// 26.2 AbstractHurtingProjectile: (v + normalize(v) * accelerationPower 0.1) * inertia, then the move
assert.ok(Math.abs(fireball.velocity.x - 1.045) < 1e-12);
assert.equal(fireball.position.x, 0.5 + fireball.velocity.x);
// before 1.21 the power vector is added after the move, with the 0.95F drag
const legacyFireball = native.simulateVersionedProjectileTicks("1.20.4", projectileWorld, [{
    position: { x: 0.5, y: 0.5, z: 0.5 },
    velocity: { x: 1, y: 0, z: 0 },
    kind: "fireball",
    power: { x: 0.1, y: 0, z: 0 },
}], 1)[0];
assert.equal(legacyFireball.position.x, 1.5);
assert.equal(legacyFireball.velocity.x, 1.1 * Math.fround(0.95));
const dangerousSkull = native.simulateVersionedProjectileTicks("1.21.11", new native.NativeWorld(), [{
    position: { x: 0, y: 0, z: 0 },
    velocity: { x: 1, y: 0, z: 0 },
    kind: "wither_skull",
    dangerous: true,
}], 1)[0];
assert.equal(dangerousSkull.velocity.x, 1.1 * Math.fround(0.73));
assert.throws(
    () => native.simulateVersionedProjectileTicks("1.10.2", projectileWorld, [{
        position: { x: 0, y: 0, z: 0 },
        velocity: { x: 0, y: 0, z: 0 },
        kind: "fishing_hook",
    }], 1),
    /fishing hook simulation is not supported before 1.11/,
);
const projectileSecondTick = native.simulateProjectileTick(projectileWorld, {
    position: projectile.position,
    velocity: projectile.velocity,
    gravity: 0,
    airDrag: 1,
    firstTick: projectile.firstTick,
});
assert.equal(projectileSecondTick.firstTick, false);
const arrow = native.simulateArrowTick(projectileWorld, {
    position: { x: 0.5, y: 0.5, z: 0.5 },
    velocity: { x: 3, y: 0, z: 0 },
    gravity: 0,
    airDrag: 1,
});
assert.equal(arrow.blockHit, true);
assert.equal(arrow.inGround, true);
assert.equal(arrow.discarded, false);
// 26.2 AbstractArrow: inGroundTime stays 0 on the hit tick; the hit pulls back by 0.05F
assert.equal(arrow.inGroundTime, 0);
assert.equal(arrow.shakeTime, 7);
assert.equal(arrow.position.x, 2 - Math.fround(0.05));
assert.deepEqual(arrow.lastBlockPosition, { x: 2, y: 0, z: 0 });
const arrowBatch = native.simulateVersionedArrowTicks("1.20.6", projectileWorld, [{
    position: { x: 0.5, y: 0.5, z: 0.5 },
    velocity: { x: 0.1, y: 0, z: 0 },
    gravity: 0,
    airDrag: 1,
}], 2);
assert.equal(arrowBatch.length, 1);
const arrowSingle = native.simulateArrowTick(projectileWorld, {
    position: { x: 0.5, y: 0.5, z: 0.5 },
    velocity: { x: 0.1, y: 0, z: 0 },
    gravity: 0,
    airDrag: 1,
});
const arrowSingleSecond = native.simulateArrowTick(projectileWorld, {
    position: arrowSingle.position,
    velocity: arrowSingle.velocity,
    gravity: 0,
    airDrag: 1,
    inGround: arrowSingle.inGround,
    inGroundTime: arrowSingle.inGroundTime,
    shakeTime: arrowSingle.shakeTime,
});
assert.deepEqual(arrowBatch[0], arrowSingleSecond);
assert.throws(
    () => native.simulateVersionedArrowTicks("1.99.1", projectileWorld, [], 1),
    /unsupported Minecraft version for arrow simulation/,
);
const tridentBatch = native.simulateVersionedTridentTicks("1.20.6", projectileWorld, [{
    position: { x: 0.5, y: 0.5, z: 0.5 },
    velocity: { x: 0.1, y: 0, z: 0 },
    gravity: 0,
    airDrag: 1,
}], 2);
assert.equal(tridentBatch.length, 1);
const tridentSingle = native.simulateTridentTick(projectileWorld, {
    position: { x: 0.5, y: 0.5, z: 0.5 },
    velocity: { x: 0.1, y: 0, z: 0 },
    gravity: 0,
    airDrag: 1,
});
const tridentSingleSecond = native.simulateTridentTick(projectileWorld, {
    position: tridentSingle.position,
    velocity: tridentSingle.velocity,
    gravity: 0,
    airDrag: 1,
});
assert.deepEqual(tridentBatch[0], tridentSingleSecond);
assert.throws(
    () => native.simulateVersionedTridentTicks("1.99.1", projectileWorld, [], 1),
    /unsupported Minecraft version for trident simulation/,
);
assert.throws(
    () => native.simulateVersionedTridentTicks("1.12.2", projectileWorld, [], 1),
    /trident simulation is not supported before 1.13/,
);
assert.throws(
    () => native.simulateVersionedProjectileTicks("1.20.2", projectileWorld, [{
        position: { x: 0, y: 0, z: 0 },
        velocity: { x: 0, y: 0, z: 0 },
        kind: "minecraft:wind_charge",
    }], 1),
    /wind charge simulation is not supported before 1.20.3/,
);
assert.throws(
    () => native.simulateProjectileTick(projectileWorld, {
        position: { x: Number.NaN, y: 0, z: 0 },
        velocity: { x: 0, y: 0, z: 0 },
    }),
    /projectile simulation state contains invalid values/,
);
