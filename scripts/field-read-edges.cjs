// The identity boundaries no mapping can follow, read in the trees instead (B271). scripts/field-lineage.cjs leaves an edge
// `unresolved` where nothing carries a member's identity across it: into 26.1.2 and the releases beside it (unobfuscated, no
// ids), across 1.13.2 and 1.14.4 where the legacy mappings left a member unnamed, and an override (intermediary lists a
// method only where it is first declared). For each such edge here, the release beside it was read: the files that would
// hold the member hold no declaration of any name it or its forerunner could have, and where the mechanic lived on in
// another member (`holder`), that member's declaration is named. scripts/field-evidence.cjs checks every entry against the
// trees when it writes the record and turns the edge into `added` only when the reading holds; it stops on one that does
// not. test/field-evidence.test.cjs reads them again. An `added` member is not a mechanic added unless MECHANIC_ANCHORS
// says so (scripts/field-contracts.cjs).
//
//   anchor: { release, beside, absent: { file: [names] }, missingFiles?: [file], holder?: { file, name }, note }
//   release   the first release that declares the member (the edge's `since`)
//   beside    the release before it, whose tree was read
//   absent    files of `beside` (relative to its net/minecraft) and the names none of them declares
//   holder    where `beside` keeps what the member holds from `release`, when it does
const fs = require("node:fs");
const path = require("node:path");

const READ_EDGES = {
  "Block.getBounceRestitution": {
    release: "26.2",
    beside: "26.1.2",
    absent: {
      "world/level/block/Block.java": ["getBounceRestitution", "bounceRestitution"],
      "world/level/block/state/BlockBehaviour.java": ["getBounceRestitution", "bounceRestitution"],
    },
    note: "26.1.2's blocks hold no restitution: a landing bounces only by SlimeBlock's and BedBlock's own bounceUp",
  },
  "Entity.getAirDrag": {
    release: "26.2",
    beside: "26.1.2",
    absent: {
      "world/entity/Entity.java": ["getAirDrag", "airDrag"],
      "world/entity/LivingEntity.java": ["getAirDrag"],
      "world/entity/item/ItemEntity.java": ["getAirDrag"],
    },
    note: "26.1.2 has no air drag member: each tick multiplies by its own constant, and no restitution reads one",
  },
  "Entity.Movement": {
    release: "1.21.2",
    beside: "1.21.1",
    absent: { "world/entity/Entity.java": ["Movement", "movementThisTick"] },
    note: "1.21.1's Entity keeps no movements: checkInsideBlocks reads the box once, after the move",
  },
  "Entity.getPose": {
    release: "1.14.4",
    beside: "1.13.2",
    absent: { "world/entity/Entity.java": ["getPose", "setPose", "DATA_POSE"] },
    missingFiles: ["world/entity/Pose.java"],
    note: "1.13.2 has no pose: a sneaking, gliding or swimming player is sized by Player.updateSize",
  },
  "HitResult.getLocation": {
    release: "1.14.4",
    beside: "1.13.2",
    absent: { "world/phys/HitResult.java": ["getLocation"] },
    holder: { file: "world/phys/HitResult.java", name: "location" },
    note: "1.13.2 keeps the point in HitResult's public field",
  },
  "ClipContext.FALLDAMAGE_RESETTING": {
    release: "1.18.2",
    beside: "1.18.1",
    absent: { "world/level/ClipContext.java": ["FALLDAMAGE_RESETTING"] },
    note: "1.18.1's ClipContext.Block is COLLIDER, OUTLINE and VISUAL",
  },
  "Boat|AbstractBoat.isControlledByLocalInstance|getControllingPassenger": {
    release: "1.9.4",
    beside: "1.8.8",
    absent: {
      "world/entity/vehicle/Boat.java": ["isControlledByLocalInstance", "getControllingPassenger"],
      "world/entity/Entity.java": ["isControlledByLocalInstance", "getControllingPassenger"],
    },
    holder: { file: "world/entity/Entity.java", name: "rider" },
    note: "1.8.8 has one rider, which steers the boat when it is living",
  },
  "Boat|AbstractBoat.isUnderWater": {
    release: "1.16",
    beside: "1.15.2",
    absent: { "world/entity/vehicle/Boat.java": ["isUnderWater"] },
    holder: { file: "world/entity/Entity.java", name: "isUnderWater" },
    note: "1.15.2's boat inherits Entity.isUnderWater",
  },
  "Boat|AbstractBoat.push": {
    release: "1.9.4",
    beside: "1.8.8",
    absent: { "world/entity/vehicle/Boat.java": ["push"] },
    holder: { file: "world/entity/Entity.java", name: "push" },
    note: "1.8.8's boat inherits Entity.push",
  },
};

/**
 * Reads an entry in the tree of its `beside` release (`base`: that tree's net/minecraft folder; `declarations(text)`: name ->
 * first declaring line). Returns { problems, holder } where holder is `file:line` of the holder's declaration.
 */
function readEdge(entry, base, declarations) {
  const problems = [];
  const declared = new Map();
  const of = (file) => {
    const full = path.join(base, file);
    if (!fs.existsSync(full)) return null;
    if (!declared.has(file)) declared.set(file, declarations(fs.readFileSync(full, "utf8")));
    return declared.get(file);
  };
  for (const [file, names] of Object.entries(entry.absent)) {
    const lines = of(file);
    if (!lines) problems.push(`${entry.beside} has no ${file}`);
    else for (const name of names) if (lines.has(name)) problems.push(`${entry.beside} ${file}:${lines.get(name)} declares ${name}`);
  }
  for (const file of entry.missingFiles ?? []) if (fs.existsSync(path.join(base, file))) problems.push(`${entry.beside} has ${file}`);
  let holder = null;
  if (entry.holder) {
    const line = of(entry.holder.file)?.get(entry.holder.name);
    if (line) holder = `${entry.holder.file}:${line}`;
    else problems.push(`${entry.beside} ${entry.holder.file} declares no ${entry.holder.name}`);
  }
  return { problems, holder };
}

module.exports = { READ_EDGES, readEdge };
