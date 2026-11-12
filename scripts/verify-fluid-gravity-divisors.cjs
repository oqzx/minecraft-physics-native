// Checks each registered native version profile's fluid gravity divisors against the decompiled
// LivingEntity of its release, so a profile cannot carry a divisor its source does not compute:
// - water: the divisor of getFluidFallingAdjustedMovement's `Math.abs(y - gravity / N) < 0.003`
//   test and its `y - gravity / N` fall (1.13+; inline in travel through 1.15, its own method
//   after), which the profile carries as water_gravity_divisor;
// - lava: the `-gravity / N` that travel's lava branch adds (`velocityY -= gravity / N` in 1.13,
//   `add(0.0, -gravity / N, 0.0)` from 1.14), which the profile carries as lava_gravity_divisor.
// Before 1.13 both branches subtract a literal 0.02 (EntityLivingBase.travel's
// `motionY -= 0.02D`): the lava literal must be the profile's gravity / lava_gravity_divisor and the
// water literal the 0.02 legacy_fluid::sink_in_water subtracts; water_gravity_divisor is unused.
//
// An extraction is rejected, not guessed, when it finds no candidate or more than one distinct
// divisor for a fluid, or a divisor that is not a positive finite number. Sources are found as
// verify-landing-policy.cjs finds them: each release's exact Mojang-named tree (scripts/source-roots.cjs);
// a release with no tree at hand is validated between neighbours that agree. Exits non-zero when any
// profile disagrees or cannot be validated.

const fs = require("node:fs");
const path = require("node:path");
const { sourceLocator } = require("./verify-landing-policy.cjs");

const repositoryRoot = path.resolve(__dirname, "..");
const versionedRoot = path.join(repositoryRoot, "native", "src", "versioned");
const legacyFluidPath = path.join(repositoryRoot, "native", "src", "legacy_fluid.rs");

/** Releases validated on another release's tree, with the reason: none, every release has its own. */
const STAND_INS = new Map();

function compareVersions(left, right) {
  const a = left.split(".").map(Number);
  const b = right.split(".").map(Number);
  for (let index = 0; index < Math.max(a.length, b.length); index += 1) {
    const difference = (a[index] ?? 0) - (b[index] ?? 0);
    if (difference !== 0) return difference;
  }
  return 0;
}

const NUMBER = String.raw`(\d+(?:\.\d+)?(?:E-?\d+)?)D?(?![\dFf])`;

function single(values, what) {
  const distinct = [...new Set(values)];
  if (distinct.length === 0) return undefined;
  if (distinct.length > 1) throw new Error(`more than one ${what}: ${distinct.join(", ")}`);
  const value = distinct[0];
  if (!Number.isFinite(value) || value <= 0) throw new Error(`${what} ${value} is not a positive finite number`);
  return value;
}

/**
 * The fluid gravity a LivingEntity source computes: `{ form: "divisor", water, lava }` from 1.13,
 * `{ form: "literal", water, lava }` (the subtracted constants) before, or undefined.
 */
function sourceDivisors(java) {
  const waterChecks = [...java.matchAll(new RegExp(String.raw`Math\.abs\(\s*[\w.]+\s*-\s*\w+\s*/\s*${NUMBER}\s*\)\s*<\s*0\.003D?`, "g"))]
    .map((match) => Number(match[1]));
  const water = single(waterChecks, "water fluid-falling divisor");
  if (water !== undefined) {
    // 1.14+: add(0.0, -gravity / N, 0.0); 1.13: velocityY -= gravity / N (the water fall too)
    const falls = [...java.matchAll(new RegExp(String.raw`(?:\.add\(\s*0\.0D?\s*,\s*-\s*\w+\s*/\s*${NUMBER}\s*,\s*0\.0D?\s*\)|\b(?:motionY|velocityY)\s*-=\s*\w+\s*/\s*${NUMBER}\s*;)`, "g"))]
      .map((match) => match[1] ?? match[2])
      .map(Number)
      .filter((value) => value !== water);
    const lava = single(falls, "lava gravity divisor");
    if (lava === undefined) throw new Error("no lava gravity divisor beside the water one");
    return { form: "divisor", water, lava };
  }
  // the sink follows the branch's vertical drag: 0.8F in water, 0.5 in lava
  const sinks = [...java.matchAll(/\b(?:motionY|velocityY)\s*\*=\s*(0\.8F|0\.800000011920929D?|0\.5D?);(?:(?!motionY|velocityY)[\s\S]){0,200}?\b(?:motionY|velocityY)\s*-=\s*(0\.\d+)D?;/g)];
  const sinkOf = (drag, what) => single(
    sinks.filter((match) => match[1].startsWith(drag)).map((match) => Number(match[2])),
    what,
  );
  const waterSink = sinkOf("0.8", "legacy water sink literal");
  const lavaSink = sinkOf("0.5", "legacy lava sink literal");
  if (waterSink === undefined && lavaSink === undefined) return undefined;
  if (waterSink === undefined || lavaSink === undefined) throw new Error("a legacy fluid branch has no sink literal");
  return { form: "literal", water: waterSink, lava: lavaSink };
}

function rustNumber(source, field, file) {
  const value = new RegExp(String.raw`\b${field}:\s*(-?\d+(?:\.\d+)?)`).exec(source)?.[1];
  if (value === undefined) throw new Error(`${file}: the profile does not set ${field}`);
  return Number(value);
}

/** A profile module's release and the fluid gravity it runs. */
function nativeDivisors(moduleSource, legacySink, file = "profile") {
  const version = /const VERSION: &'static str = "([^"]+)";/.exec(moduleSource)?.[1];
  if (!version) throw new Error(`${file}: no VERSION`);
  const gravity = rustNumber(moduleSource, "gravity", file);
  const water = rustNumber(moduleSource, "water_gravity_divisor", file);
  const lava = rustNumber(moduleSource, "lava_gravity_divisor", file);
  for (const [field, value] of [["water_gravity_divisor", water], ["lava_gravity_divisor", lava]]) {
    if (!Number.isFinite(value) || value <= 0) throw new Error(`${version}: ${field} ${value} is not a positive finite number`);
  }
  return compareVersions(version, "1.13") >= 0
    ? { version, form: "divisor", water, lava }
    : { version, form: "literal", water: legacySink, lava: gravity / lava };
}

/** The literal sink_in_water subtracts before 1.13. */
function legacySinkLiteral(source) {
  const body = /fn sink_in_water\b[\s\S]*?\{([\s\S]*?)\n\}/.exec(source)?.[1] ?? "";
  const literal = /velocity\.y\s*-=\s*(0\.\d+)/.exec(body)?.[1];
  if (!literal) throw new Error("legacy_fluid.rs: sink_in_water subtracts no literal");
  return Number(literal);
}

function sameDivisors(left, right) {
  return left.form === right.form
    && Math.abs(left.water - right.water) < 1e-12
    && Math.abs(left.lava - right.lava) < 1e-12;
}

function describe(divisors) {
  return divisors.form === "divisor"
    ? `water /${divisors.water}, lava /${divisors.lava}`
    : `legacy literal water -${divisors.water}, lava -${divisors.lava}`;
}

function verifyFluidGravityDivisors(options = {}) {
  const modules = options.modules ?? fs.readdirSync(versionedRoot)
    .filter((entry) => /^v-.*\.rs$/.test(entry))
    .map((entry) => ({ file: entry, source: fs.readFileSync(path.join(versionedRoot, entry), "utf8") }));
  const legacySink = legacySinkLiteral(options.legacyFluidSource ?? fs.readFileSync(legacyFluidPath, "utf8"));
  const locator = options.locator ?? sourceLocator(options);
  const cache = new Map();
  const divisorsOf = (version) => {
    if (!cache.has(version)) {
      const found = locator.livingEntity(version);
      const divisors = found && sourceDivisors(found.java);
      cache.set(version, divisors && { ...divisors, from: found.from });
    }
    return cache.get(version);
  };
  const results = [];
  for (const { file, source } of modules) {
    const native = nativeDivisors(source, legacySink, file);
    const { version } = native;
    let checked = divisorsOf(version);
    let basis = checked ? "exact" : undefined;
    if (!checked && STAND_INS.has(version)) {
      const standIn = STAND_INS.get(version);
      checked = divisorsOf(standIn.version);
      basis = checked && `stand-in ${standIn.version} (${standIn.reason})`;
    }
    if (!checked) {
      const lower = locator.versions.filter((other) => compareVersions(other, version) < 0 && divisorsOf(other)).at(-1);
      const upper = locator.versions.find((other) => compareVersions(other, version) > 0 && divisorsOf(other));
      if (lower && upper && sameDivisors(divisorsOf(lower), divisorsOf(upper))) {
        checked = divisorsOf(lower);
        basis = `bracketed by ${lower} and ${upper}`;
      }
    }
    results.push({ version, native, source: checked, basis, ok: !!checked && sameDivisors(native, checked) });
  }
  return results.sort((left, right) => compareVersions(left.version, right.version));
}

if (require.main === module) {
  const results = verifyFluidGravityDivisors();
  let failed = 0;
  for (const result of results) {
    if (!result.source) {
      failed += 1;
      console.error(`${result.version}: no LivingEntity source to validate ${describe(result.native)} against`);
    } else if (!result.ok) {
      failed += 1;
      console.error(`${result.version}: the profile runs ${describe(result.native)}, ${result.source.from} computes ${describe(result.source)}`);
    } else {
      console.log(`${result.version}: ${describe(result.native)} (${result.basis}: ${result.source.from})`);
    }
  }
  if (failed > 0) {
    console.error(`${failed} of ${results.length} profiles fail fluid gravity validation`);
    process.exit(1);
  }
  console.log(`Validated the fluid gravity divisors of ${results.length} profiles against LivingEntity.`);
}

module.exports = { verifyFluidGravityDivisors, sourceDivisors, nativeDivisors, legacySinkLiteral };
