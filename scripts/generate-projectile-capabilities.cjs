// generated/projectile-capabilities.json: what each release's source says of each projectile the engine models. For a
// native ProjectileKind: the class, the classes it runs through up to Entity, which of them declares each method of a
// tick and an impact, and the instance fields those classes hold, which is the state a projectile carries between ticks.
//
// `status` keeps three answers apart: `extracted`, `absent-entity` (the release's tree has no such class: the entity does
// not exist yet) and `unavailable-source` (no tree of the release at hand, which says nothing of the entity). A method
// the chain never declares is listed under `undeclared`, and with --check a difference from the file fails. An impact
// method keeps `writes`: the state it writes and the methods it hands on to, in source order, since a later step reads
// what an earlier one left (an arrow's onHitBlock reads its motion before it zeroes it).
const crypto = require("node:crypto");
const fs = require("node:fs");
const path = require("node:path");
const { registeredReleases } = require("./registered-releases.cjs");
const { LEGACY_RELEASES } = require("./legacy-release-data.cjs");
const { methodsNamed } = require("./java-declarations.cjs");
const { modernTree, legacyTree } = require("./source-roots.cjs");

const repositoryRoot = path.resolve(__dirname, "..");
const outputPath = path.join(repositoryRoot, "generated", "projectile-capabilities.json");

/** native ProjectileKind -> its class, the first of these a tree has (every tree is Mojang-named). */
const KINDS = {
  Snowball: { mojang: ["Snowball"] },
  Egg: { mojang: ["ThrownEgg"] },
  EnderPearl: { mojang: ["ThrownEnderpearl"] },
  Potion: { mojang: ["ThrownPotion", "AbstractThrownPotion"] },
  ExperienceBottle: { mojang: ["ThrownExperienceBottle"] },
  Firework: { mojang: ["FireworkRocketEntity"] },
  FishingHook: { mojang: ["FishingHook"] },
  SpectralArrow: { mojang: ["SpectralArrow"] },
  // (1.8.8 has one arrow class, which the mapping names AbstractArrow)
  TippedArrow: { mojang: ["Arrow", "AbstractArrow"] },
  Arrow: { mojang: ["Arrow", "AbstractArrow"] },
  Trident: { mojang: ["ThrownTrident"] },
  SmallFireball: { mojang: ["SmallFireball"] },
  Fireball: { mojang: ["LargeFireball"] },
  WitherSkull: { mojang: ["WitherSkull"] },
  DragonFireball: { mojang: ["DragonFireball"] },
  WindCharge: { mojang: ["WindCharge", "AbstractWindCharge"] },
  LlamaSpit: { mojang: ["LlamaSpit"] },
  ShulkerBullet: { mojang: ["ShulkerBullet"] },
};

/** The methods of a projectile's tick and impact: the Mojang name, and Yarn's where it differs. */
const HOOKS = {
  tick: ["tick"],
  shoot: ["shoot", "setVelocity"],
  onHit: ["onHit", "onCollision"],
  onHitEntity: ["onHitEntity", "onEntityHit"],
  onHitBlock: ["onHitBlock", "onBlockHit"],
  canHitEntity: ["canHitEntity", "method_26958", "canHit"],
  getGravity: ["getGravity", "getDefaultGravity", "getGravityVelocity", "method_7490"],
  getWaterInertia: ["getWaterInertia", "getDragInWater", "method_7436"],
  getInertia: ["getInertia", "getDrag", "method_7466"],
  applyInertia: ["applyInertia"],
  findHitEntity: ["findHitEntity", "getEntityCollision", "method_7434"],
  explode: ["explode"],
};

const ENTITY_ROOTS = ["net/minecraft/world/entity", "net/minecraft/entity"];

function javaFiles(directory, found = new Map()) {
  if (!fs.existsSync(directory)) return found;
  for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
    const file = path.join(directory, entry.name);
    if (entry.isDirectory()) javaFiles(file, found);
    else if (entry.name.endsWith(".java") && !found.has(entry.name.slice(0, -5))) found.set(entry.name.slice(0, -5), file);
  }
  return found;
}

const superclass = (source, name) => new RegExp(`\\bclass\\s+${name}\\b[^{]*?\\bextends\\s+([A-Za-z0-9_.]+)`).exec(source)?.[1]?.split(".").pop() ?? null;

/** The instance fields a class declares itself, at its first level of nesting. */
function instanceFields(source, name) {
  const header = new RegExp(`\\bclass\\s+${name}\\b[^{]*\\{`).exec(source);
  if (!header) return [];
  const fields = [];
  let depth = 1;
  let line = "";
  for (let at = header.index + header[0].length; at < source.length && depth > 0; at += 1) {
    const character = source[at];
    if (character === "{") depth += 1;
    else if (character === "}") depth -= 1;
    else if (depth === 1 && character !== ";") line += character;
    if (depth === 1 && character === ";") {
      const declaration = /^\s*(?:@\w+\s+)*((?:public|protected|private|final|volatile|transient)\s+)*[\w.<>[\], ?]+\s+([A-Za-z_$][\w$]*)\s*(?:=[\s\S]*)?$/.exec(line);
      if (declaration && !/\bstatic\b/.test(line)) fields.push(declaration[2]);
      line = "";
    } else if (character === "}" && depth === 1) {
      line = "";
    }
  }
  return fields.sort();
}

const IMPACTS = new Set(["onHit", "onHitBlock", "onHitEntity"]);
const WRITE = /\bthis\.([A-Za-z_$][\w$]*)\s*(?:=(?!=)|\+=|-=)|\bthis\.((?:set|reset|add|discard|remove)[A-Z]?[\w$]*)\s*\(|\bsuper\.([A-Za-z_$][\w$]*)\s*\(|\bthis\.(remove|discard|kill)\s*\(/g;

/** The writes of a method body in source order: `field=`, `setter()` and `super.method()`. */
function impactWrites(body) {
  const writes = [];
  for (const match of body.matchAll(WRITE)) {
    if (match[1]) writes.push(`${match[1]}=`);
    else if (match[2]) writes.push(`${match[2]}()`);
    else if (match[3]) writes.push(`super.${match[3]}()`);
    else writes.push(`${match[4]}()`);
  }
  return writes;
}

const parsed = new Map();
/** methodsNamed, once for a text: the same few files come round for every kind of every release. */
function declarationsOf(source, name) {
  const key = `${crypto.createHash("sha1").update(source).digest("hex")}:${name}`;
  if (!parsed.has(key)) parsed.set(key, methodsNamed(source, name));
  return parsed.get(key);
}

function kindEntry(names, files) {
  const className = names.find((name) => files.has(name));
  if (!className) return { status: "absent-entity" };
  const chain = [];
  for (let name = className; name && name !== "Object" && !chain.some((link) => link.name === name);) {
    const file = files.get(name);
    if (!file) {
      chain.push({ name, source: null });
      break;
    }
    const source = fs.readFileSync(file, "utf8");
    chain.push({ name, source });
    if (name === "Entity") break;
    name = superclass(source, name);
  }
  const methods = {};
  const undeclared = [];
  for (const [hook, candidates] of Object.entries(HOOKS)) {
    let owner = null;
    for (const link of chain) {
      if (!link.source) continue;
      for (const candidate of candidates) {
        const declared = declarationsOf(link.source, candidate).filter((method) => method.declaringClass === link.name);
        if (declared.length > 0) {
          owner = {
            owner: link.name, ...(candidate !== hook ? { as: candidate } : {}), signatures: declared.map((method) => method.signature), sha256: declared[0].sha256,
            ...(IMPACTS.has(hook) ? { writes: declared.flatMap((method) => impactWrites(method.source)) } : {}),
          };
          break;
        }
      }
      if (owner) break;
    }
    if (owner) methods[hook] = owner;
    else undeclared.push(hook);
  }
  return {
    status: "extracted",
    class: className,
    chain: chain.map((link) => link.name),
    fields: Object.fromEntries(chain.filter((link) => link.source && link.name !== "Entity").map((link) => [link.name, instanceFields(link.source, link.name)])),
    methods,
    undeclared,
  };
}

function releaseEntry(version, legacySource) {
  const root = legacySource ? legacyTree(version) : modernTree(version);
  if (!root || !fs.existsSync(root)) return Object.fromEntries(Object.keys(KINDS).map((kind) => [kind, { status: "unavailable-source" }]));
  const files = new Map();
  for (const entities of ENTITY_ROOTS) javaFiles(path.join(root, entities), files);
  return Object.fromEntries(Object.entries(KINDS).map(([kind, names]) => [kind, kindEntry(names.mojang, files)]));
}

function generate() {
  const legacy = new Map(LEGACY_RELEASES.map((release) => [release.version, release.source]));
  const releases = {};
  for (const { version } of registeredReleases()) releases[version] = releaseEntry(version, legacy.get(version));
  return { schema: 1, generatedBy: "scripts/generate-projectile-capabilities.cjs", releases };
}

function main() {
  const output = `${JSON.stringify(generate())}\n`;
  if (process.argv.includes("--check")) {
    const present = fs.existsSync(outputPath) ? fs.readFileSync(outputPath, "utf8").replaceAll("\r\n", "\n") : "";
    if (present !== output) throw new Error("generated/projectile-capabilities.json differs from a fresh generation");
    return;
  }
  fs.writeFileSync(`${outputPath}.tmp`, output);
  fs.renameSync(`${outputPath}.tmp`, outputPath);
  console.log(`digest ${crypto.createHash("sha256").update(output).digest("hex").slice(0, 16)}`);
}

if (require.main === module) {
  try {
    main();
  } catch (error) {
    console.error(error.message);
    process.exit(1);
  }
}

module.exports = { KINDS, HOOKS, instanceFields, superclass, impactWrites };
