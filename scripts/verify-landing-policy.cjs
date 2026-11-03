// Checks each registered native version profile's landing-damage policy against the decompiled
// LivingEntity of its release: the formula (the legacy float `ceil((fd - 3.0F - jumpBoost) * mult)`
// or the attribute form with SAFE_FALL_DISTANCE / FALL_DAMAGE_MULTIPLIER), the rounding (ceil or
// floor) and, in the attribute form, the double calculateFallPower with its 1.0E-6 epsilon.
//
// The native side is read from the Rust sources themselves: the profile's landing_damage_rounding,
// the tick function its engine dispatches to and the flag that function hands on as
// `modern_fall_damage` (simulation.rs), and the release gate of the epsilon in
// attribute_landing_damage. Nothing is taken from a hand-kept table.
//
// Sources, the first that has the release's LivingEntity:
// - MC_DECOMP_SRC: the generators' mc-decomp tree, <era>/<version>/net/minecraft/world/entity/
// - MC_DECOMPILED_ROOT (default ../../pr/mc-decompiled from the package): <version>/net/minecraft/
//   entity/LivingEntity.java (Yarn, to 1.21.4) or world/entity/LivingEntity.java (Mojang)
// - MC_EXTRACTED_DATA_REPO: a git repository with origin/client<version> branches, read with
//   `git show` (never checked out)
// A release with no snapshot of its own is validated when the nearest snapshots below and above
// it both carry the policy the profile declares (it cannot differ from both of its neighbours
// without a change of its own); 1.8.8 stands on 1.8.9, the security-fix release of the same
// code. Exits non-zero when any profile disagrees or cannot be validated.

const fs = require("node:fs");
const path = require("node:path");
const { execFileSync } = require("node:child_process");

const repositoryRoot = path.resolve(__dirname, "..");
const versionedRoot = path.join(repositoryRoot, "native", "src", "versioned");
const simulationPath = path.join(repositoryRoot, "native", "src", "simulation.rs");

/** Releases validated on another release's snapshot, with the reason. */
const STAND_INS = new Map([
  ["1.8.8", { version: "1.8.9", reason: "1.8.9 is 1.8.8 with a security fix only" }],
]);

function parseVersion(version) {
  return version.split(".").map(Number);
}

function compareVersions(left, right) {
  const a = parseVersion(left);
  const b = parseVersion(right);
  for (let index = 0; index < Math.max(a.length, b.length); index += 1) {
    const difference = (a[index] ?? 0) - (b[index] ?? 0);
    if (difference !== 0) return difference;
  }
  return 0;
}

function atLeast(version, gate) {
  return compareVersions(version, gate.join(".")) >= 0;
}

/** The body of the first `fn <name>` in a Rust source, braces balanced. */
function rustFunctionBody(source, name) {
  const start = source.search(new RegExp(`fn ${name}\\b`));
  if (start < 0) return undefined;
  const open = source.indexOf("{", start);
  let depth = 0;
  for (let index = open; index < source.length; index += 1) {
    if (source[index] === "{") depth += 1;
    else if (source[index] === "}") {
      depth -= 1;
      if (depth === 0) return source.slice(open + 1, index);
    }
  }
  return undefined;
}

/**
 * What simulation.rs makes of a profile: per tick_with_profile* function the expression it hands
 * on as `modern_fall_damage` (the last argument of its tick_with_rules_and_rounding_and_restitution
 * call), and the release from which attribute_landing_damage adds the epsilon in double.
 */
function parseSimulation(source) {
  const dispatch = new Map();
  for (const match of source.matchAll(/pub fn (tick_with_profile\w*)<P: VersionPhysicsProfile>/g)) {
    const body = rustFunctionBody(source.slice(match.index), match[1]);
    const call = /tick_with_rules_and_rounding_and_restitution\(([\s\S]*)\)\s*$/.exec(body?.trim() ?? "");
    if (!call) throw new Error(`simulation.rs: ${match[1]} does not end in tick_with_rules_and_rounding_and_restitution`);
    const args = call[1].split(",").map((arg) => arg.trim()).filter(Boolean);
    dispatch.set(match[1], args[args.length - 1]);
  }
  // the trait's default of a profile method a tick hands on (P::modern_fall_damage())
  const trait = /pub trait VersionPhysicsProfile \{[\s\S]*?\r?\n\}/.exec(source)?.[0] ?? "";
  const defaults = new Map();
  for (const [, name] of trait.matchAll(/fn (\w+)\(\) -> bool \{/g)) {
    defaults.set(name, rustFunctionBody(trait, name)?.trim().replace(/^Self::/, "P::"));
  }
  const attribute = rustFunctionBody(source, "attribute_landing_damage");
  const gate = /release\.at_least\((\d+), (\d+), (\d+)\)/.exec(attribute ?? "");
  if (!gate) throw new Error("simulation.rs: attribute_landing_damage has no release gate for its double form");
  return { dispatch, defaults, epsilonFrom: gate.slice(1).map(Number) };
}

/** A profile module's release and landing policy as the engine runs it. */
function nativePolicy(moduleSource, simulation, file = "profile") {
  const version = /const VERSION: &'static str = "([^"]+)";/.exec(moduleSource)?.[1];
  if (!version) throw new Error(`${file}: no VERSION`);
  const roundingBody = rustFunctionBody(moduleSource, "landing_damage_rounding");
  const rounding = /LandingDamageRounding::(Ceil|Floor)/.exec(roundingBody ?? "")?.[1];
  if (!rounding) throw new Error(`${version}: no explicit landing_damage_rounding`);
  const tick = /(tick_with_profile\w*)::<Profile>/.exec(moduleSource)?.[1];
  if (!tick || !simulation.dispatch.has(tick)) throw new Error(`${version}: its engine dispatches to no known profile tick`);
  let flag = simulation.dispatch.get(tick);
  // a profile method: the profile's own body, else the trait's default
  const method = /^P::(\w+)\(\)$/.exec(flag)?.[1];
  if (method) {
    flag = rustFunctionBody(moduleSource, method)?.trim() ?? simulation.defaults?.get(method) ?? flag;
  }
  let modern;
  if (flag === "true" || flag === "false") {
    modern = flag === "true";
  } else {
    const field = /^P::rules\(\)\.(\w+)$/.exec(flag)?.[1];
    if (!field) throw new Error(`${version}: ${tick} hands on an unknown modern_fall_damage expression ${flag}`);
    const value = new RegExp(`\\b${field}: (true|false)`).exec(moduleSource)?.[1];
    if (!value) throw new Error(`${version}: the profile does not set ${field}`);
    modern = value === "true";
  }
  const formula = modern ? "attribute" : "legacy";
  return {
    version,
    formula,
    rounding: rounding.toLowerCase(),
    epsilon: formula === "attribute" && atLeast(version, simulation.epsilonFrom),
  };
}

/** The landing policy a LivingEntity source computes, or undefined where none is found. */
function sourcePolicy(java) {
  const found = new Map();
  const rounding = { ceil: "ceil", floor: "floor", ceiling_float_int: "ceil", func_76123_f: "ceil" };
  for (const [, call, argument] of java.matchAll(/\b(?:Mth|MathHelper)\.(ceil|floor|ceiling_float_int|func_76123_f)\(([^;]*)\);/g)) {
    let formula;
    if (/FALL_DAMAGE_MULTIPLIER/.test(argument)) formula = "attribute";
    // the legacy form: (distance - 3.0F - jumpBoost) * damageMultiplier, any local names
    else if (/^\(\w+ - 3\.0F - \w+\) \* \w+$/.test(argument.trim())) formula = "legacy";
    else continue;
    const epsilon = formula === "attribute"
      && /double calculateFallPower\((?:final )?double \w+\)\s*\{\s*return \w+ \+ 1\.0E-6 -/.test(java);
    const policy = { formula, rounding: rounding[call], epsilon };
    found.set(JSON.stringify(policy), policy);
  }
  if (found.size > 1) throw new Error(`more than one fall-damage computation: ${[...found.keys()].join(" ")}`);
  return found.values().next().value;
}

function samePolicy(left, right) {
  return left.formula === right.formula && left.rounding === right.rounding && left.epsilon === right.epsilon;
}

function describePolicy(policy) {
  return `${policy.formula}/${policy.rounding}${policy.epsilon ? "/epsilon" : ""}`;
}

/** Where to read LivingEntity.java for a release from. */
function sourceLocator(options = {}) {
  const decomp = options.decompSrc ?? process.env.MC_DECOMP_SRC ?? path.join(repositoryRoot, "..", "mc-decomp", "src");
  const decompiled = options.decompiledRoot ?? process.env.MC_DECOMPILED_ROOT ?? path.join(repositoryRoot, "..", "..", "pr", "mc-decompiled");
  const repo = options.extractedRepo ?? process.env.MC_EXTRACTED_DATA_REPO;
  const branches = new Map();
  if (repo && fs.existsSync(repo)) {
    const listing = execFileSync("git", ["-C", repo, "branch", "-r", "--list", "origin/client*"], { encoding: "utf8" });
    for (const line of listing.split("\n")) {
      const branch = line.trim();
      const version = /^origin\/client(\d+(?:\.\d+)+)$/.exec(branch)?.[1];
      if (version) branches.set(version, branch);
    }
  }
  const decompiledVersions = fs.existsSync(decompiled)
    ? fs.readdirSync(decompiled).filter((entry) => /^\d+(?:\.\d+)+$/.test(entry))
    : [];
  const read = (file) => (fs.existsSync(file) ? { java: fs.readFileSync(file, "utf8"), from: file } : undefined);
  const gitPaths = ["client/net/minecraft/world/entity/LivingEntity.java", "client/net/minecraft/entity/EntityLivingBase.java", "client/net/minecraft/entity/LivingEntity.java"];
  return {
    versions: [...new Set([...decompiledVersions, ...branches.keys()])].sort(compareVersions),
    livingEntity(version) {
      const era = version.split(".").slice(0, 2).join(".");
      const found = read(path.join(decomp, era, version, "net/minecraft/world/entity/LivingEntity.java"))
        ?? read(path.join(decompiled, version, "net/minecraft/world/entity/LivingEntity.java"))
        ?? read(path.join(decompiled, version, "net/minecraft/entity/LivingEntity.java"));
      if (found) return found;
      const branch = branches.get(version);
      if (!branch) return undefined;
      for (const file of gitPaths) {
        try {
          const java = execFileSync("git", ["-C", repo, "show", `${branch}:${file}`], { encoding: "utf8", stdio: ["ignore", "pipe", "ignore"], maxBuffer: 64 * 1024 * 1024 });
          return { java, from: `${branch}:${file}` };
        } catch {
          // not at this path in this release
        }
      }
      return undefined;
    },
  };
}

/** The validation of each profile: its native policy, the source's and where the source came from. */
function verifyLandingPolicies(options = {}) {
  const simulation = parseSimulation(options.simulationSource ?? fs.readFileSync(simulationPath, "utf8"));
  const modules = options.modules ?? fs.readdirSync(versionedRoot)
    .filter((entry) => /^v-.*\.rs$/.test(entry))
    .map((entry) => ({ file: entry, source: fs.readFileSync(path.join(versionedRoot, entry), "utf8") }));
  const locator = options.locator ?? sourceLocator(options);
  const cache = new Map();
  const policyOf = (version) => {
    if (!cache.has(version)) {
      const found = locator.livingEntity(version);
      const policy = found && sourcePolicy(found.java);
      cache.set(version, policy && { ...policy, from: found.from });
    }
    return cache.get(version);
  };
  const results = [];
  for (const { file, source } of modules) {
    const native = nativePolicy(source, simulation, file);
    const { version } = native;
    let checked = policyOf(version);
    let basis = checked ? "exact" : undefined;
    if (!checked && STAND_INS.has(version)) {
      const standIn = STAND_INS.get(version);
      checked = policyOf(standIn.version);
      basis = checked && `stand-in ${standIn.version} (${standIn.reason})`;
    }
    if (!checked) {
      const lower = locator.versions.filter((other) => compareVersions(other, version) < 0 && policyOf(other)).at(-1);
      const upper = locator.versions.find((other) => compareVersions(other, version) > 0 && policyOf(other));
      if (lower && upper && samePolicy(policyOf(lower), policyOf(upper))) {
        checked = policyOf(lower);
        basis = `bracketed by ${lower} and ${upper}`;
      }
    }
    results.push({
      version,
      native,
      source: checked,
      basis,
      ok: !!checked && samePolicy(native, checked),
    });
  }
  return results.sort((left, right) => compareVersions(left.version, right.version));
}

if (require.main === module) {
  const results = verifyLandingPolicies();
  let failed = 0;
  for (const result of results) {
    if (!result.source) {
      failed += 1;
      console.error(`${result.version}: no LivingEntity source to validate ${describePolicy(result.native)} against`);
    } else if (!result.ok) {
      failed += 1;
      console.error(`${result.version}: the profile runs ${describePolicy(result.native)}, ${result.source.from} computes ${describePolicy(result.source)}`);
    } else {
      console.log(`${result.version}: ${describePolicy(result.native)} (${result.basis}: ${result.source.from})`);
    }
  }
  if (failed > 0) {
    console.error(`${failed} of ${results.length} profiles fail landing-policy validation`);
    process.exit(1);
  }
  console.log(`Validated the landing policy of ${results.length} profiles against LivingEntity.`);
}

module.exports = { verifyLandingPolicies, nativePolicy, parseSimulation, sourcePolicy, sourceLocator };
