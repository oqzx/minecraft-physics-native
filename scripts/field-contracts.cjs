// The hand-held half of the field contracts (B209, B271): what each public field means, in what unit, and which vanilla
// member or binding rule says so. The other half comes from the code (scripts/field-facts.cjs): Rust type, narrowing,
// omission default, adapter expressions. scripts/field-catalogue.cjs joins them; test/field-catalogue.test.cjs fails on a
// field with no entry, an entry for no field, and a vague one.
//
// A file of scripts/field-contracts/ exports { interfaceName: { family, owner?, fields: { name: [means, unit, source, extra?] } } }:
//   means   what the value is, as a sentence fragment that says what reads or writes it; null takes the binding's own comment
//   unit    its unit ("blocks", "blocks a tick", "degrees", "ticks", "radians", "ratio", "count", "id", "bits"...). "" only for a
//           flag, a name, or a member whose type is another interface (its rows carry the units)
//   source  a vanilla anchor checked by scripts/field-evidence.cjs ("Boat.landFriction", "Boat|AbstractBoat.status"), or
//           "binding: <rule>" for what vanilla has no member for, naming the fn in native/src or src/mineflayer that owns it
//   extra   { range: legal values narrower than the type, since: "1.20.5" or until: "1.21.1" with why: where the release bound
//           comes from when the anchor cannot show it, capability: "attribute gravity" for a row of docs/capability-table.md }
const fs = require("node:fs");
const path = require("node:path");

const directory = path.join(__dirname, "field-contracts");

/** Every contract, by interface. A second entry for an interface is an error, so two files cannot disagree. */
function contracts() {
  const all = {};
  for (const file of fs.readdirSync(directory).filter((name) => name.endsWith(".cjs")).sort()) {
    for (const [name, contract] of Object.entries(require(path.join(directory, file)))) {
      if (all[name]) throw new Error(`${name} is in two contract files`);
      all[name] = { ...contract, file };
    }
  }
  return all;
}

const isAnchor = (source) => !source.startsWith("binding:");

/**
 * The anchors whose identity boundary (docs/field-evidence.json `since` or `until`, followed through renames by
 * scripts/field-lineage.cjs) is when vanilla gained or lost the mechanic, so that it bounds the anchor's rows. A name's range
 * never does: `Entity.wasTouchingWater` starts at 1.16 by name, and its id is `wasInWater`'s back to 1.8.8. Nor does every
 * identity boundary: a refactor moves a mechanic to a new member (Block.getFriction from 1.13 holds what a field held before,
 * LivingEntity.isUsingItem from 1.9 what Player held), which no mapping tells apart, so this list says which boundaries are
 * the mechanic's. test/field-evidence.test.cjs holds each to a real identity boundary: an anchor whose member reaches the
 * oldest and newest source by identity, or whose edge is unresolved, cannot be here. The list is kept sorted.
 */
const MECHANIC_ANCHORS = new Set([
  "AbstractArrow.deflect", "AbstractHurtingProjectile.accelerationPower", "AbstractHurtingProjectile.xPower", "AbstractMinecart.useExperimentalMovement",
  "AbstractWindCharge.canHitEntity", "Block.getBounceRestitution", "BlockHitResult.isWorldBorderHit", "Entity.canSimulateMovement",
  "Entity.finalMovementsThisTick", "Entity.getAirDrag", "Entity.isNoGravity", "Entity.minorHorizontalCollision", "Entity.pushthrough", "Entity.verticalCollisionBelow",
  "EntityTypeTags|Entity.DEFLECTS_ARROWS|deflection", "ExperienceOrb.followingTime", "FallingBlockEntity.removeAtMillis", "FishingHook.inGround",
  "GameRules.RULE_ENDER_PEARLS_VANISH_ON_DEATH|ENDER_PEARLS_VANISH_ON_DEATH", "GameRules.RULE_MINECART_MAX_SPEED|MAX_MINECART_SPEED",
  "GameRules.RULE_TNT_EXPLODES|TNT_EXPLODES", "LivingEntity.stabAttack", "LivingEntity.tryAddFrost", "MinecartTNT.explosionPowerBase",
  "MinecartTNT.explosionSpeedFactor", "NewMinecartBehavior.MinecartStep", "NewMinecartBehavior.currentLerpSteps", "NewMinecartBehavior.currentLerpStepsTotalWeight",
  "NewMinecartBehavior.lerpDelay", "NewMinecartBehavior.lerpSteps", "NewMinecartBehavior.oldLerp", "Projectile.lastDeflectedBy",
  "ProtectionEnchantment.getFireAfterDampener", "ServerPlayer.getLastClientMoveIntent", "SulfurCube.floatsInLiquids", "SulfurCube.hasBodyItem",
  "ThrowableProjectile.entityToIgnore", "ThrowableProjectile.timeToIgnore", "WindCharge|AbstractWindCharge.canHitEntity",
]);

/** Whether an identity boundary of an anchor bounds its rows. */
function isMechanic(anchor, boundary) {
  return MECHANIC_ANCHORS.has(anchor) && Boolean(boundary) && (boundary.by === "added" || boundary.by === "removed");
}

/** Every vanilla member the contracts cite. */
function allAnchors() {
  return Object.values(contracts()).flatMap(({ fields }) => Object.values(fields).map(([, , source]) => source).filter(isAnchor));
}

/** What a row must hold, whatever it says: returns the problem, or null. `member` has its type from the declarations. */
function rowProblem(member, row) {
  if (!Array.isArray(row) || row.length < 3) return "has no [means, unit, source]";
  const [written, unit, source] = row;
  const means = written ?? member.note ?? "";
  if (typeof means !== "string" || means.trim().split(/\s+/).length < 3) return "says too little";
  if (/as vanilla holds it|see the type|^TODO|\bunknown\b|presumably|probably/i.test(means)) return `is vague: ${means}`;
  if (/^(the )?(value|field|flag|state) of\b/i.test(means)) return `only names itself: ${means}`;
  if (/\bnumber\b/.test(member.type) && !unit) return "is a number with no unit";
  if (typeof source !== "string" || !source) return "cites nothing";
  if (source.startsWith("binding:") && source.trim().length < 18) return "names no rule of the binding";
  return null;
}

module.exports = { MECHANIC_ANCHORS, allAnchors, contracts, isAnchor, isMechanic, rowProblem };
