// The equivalence claims of the shared native code that no `at_least` gate splits (B280, docs/shared-code-audit.md
// "Not covered"): what each ungated function and each profile rule says of vanilla, release by release, and what backs it.
// generated/shared-gates.json and generated/gate-sources.json cover the gated functions; this covers the rest.
//
// An ungated function serves every release it is reached on from one body. Its claim is that the vanilla methods it stands
// for behave the same on both sides of every boundary between the registered releases. Each function gets a status,
// computed and never set by hand:
//   source-unchanged  every vanilla method its comments cite is the same text (scripts/compare-gate-sources.cjs's
//                     normalisation) on both sides of all 49 boundaries: the claim holds from source
//   mechanical        a cited method differs at some boundary, and at each such boundary every change is accounted for by a
//                     sound rule, computed here: the function is not reached on one side of it (generated/shared-reach.json,
//                     scripts/reach-probe.cjs), or the two texts differ only in the names of locals (`renamed-locals`)
//   reviewed          as mechanical, with some change accounted for by a review in audit/shared-equivalence-reviews/ (see
//                     readReviews): the change is neutral for every caller, or not one this function reads, or a gated
//                     caller keeps one side away, or a per-release value carries it. A review names each change by its hash
//                     (changeHash), so it lapses when either text changes
//   review            a cited method differs at some boundary and nothing accounts for it there: the claim is open
//   unresolved        it cites methods, and none is in any tree: the citation does not name what it mirrors
//   jdk               every method it cites is of the Java platform (JDK, below): the Java SE specification, the same for
//                     every release
//   helper            it names no vanilla method, being an engine-internal helper, and a review names the Rust tests that
//                     check its result, each of which calls it
//   glue              it names no vanilla method, being a function of a binding file (napi_*.rs), which converts and returns
//   uncited           it names no vanilla method and is neither: no claim is made, and it is open
// Each change lists `by`, what accounts for it, and `hash`. A review that accounts for nothing is listed in staleReviews.
//
// A profile rule (a field of VersionPhysicsRules, or a profile's landing rounding, restitution and fall-damage choice) is a
// gate spelled as a value: its boundaries are where its value changes between registered releases. Its status is
// `checked` when a script or test holds the value to each release's source, named under `by`, and `unverified` otherwise.
//
//   node scripts/shared-equivalence.cjs            writes generated/shared-equivalence.json (needs the compare trees)
//   node scripts/shared-equivalence.cjs --check    fails on a difference
// test/shared-equivalence.test.cjs holds the checked-in file to the functions and rules of native/src now, with no trees,
// so a function or a rule added, removed or renamed without regenerating it fails there, and holds each review's hash to the
// record; where the compare trees are, it reads every reviewed change's two texts again.
const crypto = require("node:crypto");
const fs = require("node:fs");
const path = require("node:path");
const { scanRust, functionComments } = require("./movement-sites.cjs");
const { sharedFunctions } = require("./shared-functions.cjs");
const { canonicalReleases } = require("./release-inventory.cjs");

const root = path.resolve(__dirname, "..");
const outputPath = path.join(root, "generated", "shared-equivalence.json");
const reviewsDirectory = path.join(root, "audit", "shared-equivalence-reviews");
const sourceRoot = path.join(root, "native", "src");

/** The ungated functions that make a claim about vanilla: neither N-API entries nor gated. */
function ungatedFunctions() {
  // (a name a file declares twice, an impl's method and a free function, is one entry: their comments are read together)
  const seen = new Set();
  return sharedFunctions().filter((entry) => entry.interval === "every release" && entry.status !== "entry")
    .map(({ file, function: name }) => ({ file, function: name }))
    .filter((entry) => !seen.has(`${entry.file}:${entry.function}`) && seen.add(`${entry.file}:${entry.function}`));
}

/** Each function's comments, by `file:function` (a name declared twice in a file keeps both). */
function commentsOf(files) {
  const out = new Map();
  for (const file of files) {
    const text = fs.readFileSync(path.join(sourceRoot, file), "utf8").replaceAll("\r\n", "\n");
    const { functions, comments } = scanRust(text);
    for (const fn of functions) out.set(`${file}:${fn.name}`, `${out.get(`${file}:${fn.name}`) ?? ""} ${functionComments(fn, comments)}`);
  }
  return out;
}

const RULE_FIELD = /^\s*(\w+):\s*([^,]+),\s*$/;
/** Each release's profile values, from native/src/versioned/v-*.rs: the rules' fields and the profile's choices. */
function profileRules() {
  const directory = path.join(sourceRoot, "versioned");
  const out = {};
  for (const file of fs.readdirSync(directory).filter((name) => /^v-.*\.rs$/.test(name))) {
    const text = fs.readFileSync(path.join(directory, file), "utf8").replaceAll("\r\n", "\n");
    const release = /const VERSION: &'static str = "([^"]+)"/.exec(text)?.[1];
    if (!release) continue;
    const values = {};
    const body = /VersionPhysicsRules \{([\s\S]*?)\n\s*\}/.exec(text)?.[1] ?? "";
    for (const line of body.split("\n")) {
      const field = RULE_FIELD.exec(line);
      if (field && field[1] !== "release") values[field[1]] = field[2].trim();
    }
    values.landing_damage_rounding = /LandingDamageRounding::(\w+)\s*\n\s*\}/.exec(text)?.[1] ?? "unset";
    // (the trait's defaults: no restitution, and the fall damage of the water movement efficiency attribute's releases)
    values.restitution_enabled = /fn restitution_enabled\(\) -> bool \{\s*(true|false)/.exec(text)?.[1] ?? "false";
    values.modern_fall_damage = /fn modern_fall_damage\(\) -> bool \{\s*(true|false)/.exec(text)?.[1] ?? values.water_movement_efficiency_attribute;
    out[release] = values;
  }
  return out;
}

/** What holds each rule to the source of every release, where something does. */
const RULE_CHECKS = {
  water_gravity_divisor: "scripts/verify-fluid-gravity-divisors.cjs",
  lava_gravity_divisor: "scripts/verify-fluid-gravity-divisors.cjs",
  landing_damage_rounding: "scripts/verify-landing-policy.cjs",
};

/**
 * A rule read in the source of each release: `literal` holds the profile's value to a literal of one of `classes` (as Java
 * spells a double or a float of it), `token` holds a true or false value to whether the token is in those classes. `since`
 * leaves out the releases before the mechanic exists, where the value is never read. A literal's presence does not show
 * it is the one read where the rule is read: it shows the release's source has the value, which is what a wrong release
 * would lack.
 */
const RULE_PROBES = {
  gravity: { literal: ["LivingEntity", "Attributes"] },
  air_drag: { literal: ["LivingEntity"] },
  vertical_drag: { literal: ["LivingEntity"] },
  water_drag: { literal: ["LivingEntity"] },
  water_sprint_drag: { literal: ["LivingEntity"], since: "1.13" },
  lava_drag: { literal: ["LivingEntity"] },
  fluid_vertical_drag: { literal: ["LivingEntity"] },
  water_input_speed: { literal: ["LivingEntity"] },
  lava_input_speed: { literal: ["LivingEntity"] },
  step_height: { literal: ["Entity", "LivingEntity", "Player", "Attributes"] },
  swimming_vertical_steering: { literal: ["Player", "LocalPlayer", "LivingEntity"], since: "1.13" },
  swimming_downward_steering: { literal: ["Player", "LocalPlayer", "LivingEntity"], since: "1.13" },
  water_flow_scale: { literal: ["Entity", "Level"] },
  lava_flow_scale: { literal: ["Entity", "Level"] },
  ultra_warm_lava_flow_scale: { literal: ["Entity", "Level"], since: "1.16" },
  water_movement_efficiency_attribute: { token: ["Attributes"], pattern: /water_movement_efficiency/ },
  restitution_enabled: { token: ["Entity"], pattern: /[Rr]estitution/ },
  modern_fall_damage: { token: ["Attributes"], pattern: /safe_fall_distance/ },
  fluid_travel_split: { token: ["LivingEntity"], pattern: /\btravelInFluid\b/ },
  // (from 1.20.5 the gravity argument decides, where the entity's no-gravity flag did: 1.20.5 LivingEntity.getFluidFallingAdjustedMovement)
  fluid_falling_checks_gravity_argument: { token: ["LivingEntity"], pattern: /getFluidFallingAdjustedMovement\((?:final )?double (\w+), (?:final )?boolean \w+, (?:final )?Vec3 \w+\) \{\s*if \(\1 != 0\.0/ },
  // (the depth strider level divided by 3 before 1.21, the water movement efficiency attribute from it: 1.21 LivingEntity.travel)
  water_movement_efficiency_divisor: { token: ["LivingEntity"], pattern: /\bgetDepthStrider\b/, when: "3.0" },
};

/** How Java may spell a double or a float of `value`. */
function spellings(value) {
  const number = Number(value);
  if (!Number.isFinite(number)) return [];
  const plain = Number.isInteger(number) ? number.toFixed(1) : String(number);
  return [...new Set([plain, `${plain}D`, `${plain}F`, `${plain}f`, `${plain}d`, String(Math.fround(number))])];
}

/** Each release a probe holds the rule to, and those where the source does not say the profile's value. */
function probeRule(field, byRelease) {
  const probe = RULE_PROBES[field];
  if (!probe) return null;
  const { treeOf, filesOf } = require("./compare-gate-sources.cjs");
  const { compareReleases } = require("./registered-releases.cjs");
  const classes = probe.literal ?? probe.token;
  const read = (release) => classes.flatMap((name) => filesOf(treeOf(release), name)).map((file) => fs.readFileSync(file, "latin1")).join("\n");
  const differs = [];
  let held = 0;
  for (const [release, raw] of byRelease) {
    if (probe.since && compareReleases(release, probe.since) < 0) continue;
    const value = String(raw).replace(" (default)", "");
    const text = read(release);
    let agrees;
    if (probe.literal) {
      agrees = spellings(value).some((spelling) => new RegExp(`(?<![\\w.])${spelling.replace(/[.]/g, "\\.")}(?![\\w.])`).test(text));
    } else {
      agrees = probe.pattern.test(text) === (value === (probe.when ?? "true"));
    }
    held += 1;
    if (!agrees) differs.push(release);
  }
  return { held, differs, how: probe.literal ? `a literal of the value in ${classes.join(", ")}` : `${probe.pattern} in ${classes.join(", ")} exactly when ${probe.when ?? "true"}` };
}

/** The rules with their values by release and the boundaries where a value changes. */
function ruleClaims(registered) {
  const values = profileRules();
  const fields = [...new Set(Object.values(values).flatMap((entry) => Object.keys(entry)))].sort();
  return fields.map((field) => {
    const byRelease = registered.map((release) => [release, values[release]?.[field] ?? "missing"]);
    const boundaries = [];
    for (let index = 1; index < byRelease.length; index += 1) {
      if (byRelease[index][1] !== byRelease[index - 1][1]) boundaries.push({ boundary: byRelease[index][0], before: byRelease[index - 1][1], after: byRelease[index][1] });
    }
    const values_ = [...new Set(byRelease.map(([, value]) => value))];
    if (RULE_CHECKS[field]) return { rule: field, values: values_, boundaries, status: "checked", by: RULE_CHECKS[field] };
    const probe = probeRule(field, byRelease);
    if (!probe) return { rule: field, values: values_, boundaries, status: "unverified" };
    return { rule: field, values: values_, boundaries, status: probe.differs.length === 0 ? "checked" : "differs", by: `scripts/shared-equivalence.cjs: ${probe.how}, ${probe.held} releases`, ...(probe.differs.length > 0 ? { differs: probe.differs } : {}) };
  });
}

/**
 * The reviews, from every file of audit/shared-equivalence-reviews/ (one per native source, so that two reviewers of two
 * files never edit one file). Each file may hold:
 *   methods    { method, boundary, hash, note }: the change of a vanilla method at a boundary is behaviour-neutral for
 *              every function that cites it (a refactor that computes the same, a rename, logging or debug output)
 *   functions  { file, function, boundary, hashes: { "Class#method": hash }, reason, note }: why this function is right
 *              across the boundary although those methods changed there. `reason` is one of REASONS.
 *   helpers    { file, function, tests: [name], note }: a function that cites nothing because it mirrors no vanilla method
 *              (an engine-internal helper), with the Rust tests that check its result: each must exist in native/src and
 *              name the function
 * A hash is the change's own (`changeHash`): a review holds only while the two texts it read are the texts there now.
 */
const REASONS = {
  "not-read": "the change is in a part of the method this function does not mirror",
  "gated-caller": "every caller that reaches this function is gated so that the release on one side never does (the note names the gate)",
  parameterised: "the change is a value this function takes from the release's profile or data, not from its own body (the note names it)",
  equivalent: "the change computes the same for every input this function is given (the note says why)",
  "gated-callee": "the changed part is computed by a function this one calls, which is gated at this boundary or accounts for the change itself (the note names it)",
  "no-engine-caller": "no function of native/src outside the tests calls it, so it serves no release; held here: every caller of its name is itself called by none (deadFunctions)",
};

function readReviews() {
  const out = { methods: [], functions: [], helpers: [] };
  if (!fs.existsSync(reviewsDirectory)) return out;
  for (const name of fs.readdirSync(reviewsDirectory).filter((item) => item.endsWith(".json")).sort()) {
    const document = JSON.parse(fs.readFileSync(path.join(reviewsDirectory, name), "utf8"));
    for (const kind of Object.keys(out)) for (const review of document[kind] ?? []) out[kind].push({ ...review, from: name });
  }
  return out;
}

/** The hash a review names a change by: the two normalised texts either side, as the compare reads them. */
const changeHash = (before, after) => crypto.createHash("sha256").update(`${before}\n=>\n${after}`).digest("hex").slice(0, 16);

/**
 * A method's overloads in a release, raw and normalised; for a name that is no method of the class there, the field, enum
 * constant or constructor of that name (a citation of Entity#random reads the field's declaration and initializer).
 */
function overloads(release, className, method) {
  const { treeOf, filesOf, methodsOf } = require("./compare-gate-sources.cjs");
  const files = filesOf(treeOf(release), className);
  const methods = files.map(methodsOf).flatMap((found) => found.get(method) ?? []);
  if (methods.length > 0) return methods;
  return files.map(membersOf).flatMap((found) => found.get(method) ?? []).map(({ text }) => ({ text, raw: text }));
}

/** Is `name` a method or another member of the class in some release? */
function declared(registered, className, name) {
  const { treeOf, filesOf, methodsOf } = require("./compare-gate-sources.cjs");
  return registered.some((release) => filesOf(treeOf(release), className).some((at) => methodsOf(at).has(name) || membersOf(at).has(name)));
}

/** A file's fields, enum constants and constructors (scripts/compare-gate-sources.cjs). */
const membersOf = (file) => require("./compare-gate-sources.cjs").membersOf(file);

/**
 * The members of the Java platform a function may cite: the Java SE specification gives them, no release's tree holds
 * them, and no release boundary changes them. (Math.sin, cos and atan2 are specified to within an ulp or two, not exactly:
 * what a JVM gives is outside any release's source, and the engine's choice is its own tests'.)
 */
const JDK = {
  Math: new Set(["sin", "cos", "tan", "atan", "atan2", "asin", "acos", "sqrt", "min", "max", "abs", "floor", "ceil", "round", "signum", "floorDiv", "floorMod", "toRadians", "toDegrees", "pow", "exp", "log", "hypot", "fma", "copySign", "rint"]),
  StrictMath: new Set(["sin", "cos", "atan2", "sqrt"]),
  Float: new Set(["floatToIntBits", "floatToRawIntBits", "intBitsToFloat", "compare", "isNaN", "isFinite"]),
  Double: new Set(["doubleToLongBits", "doubleToRawLongBits", "longBitsToDouble", "compare", "isNaN", "isFinite"]),
  Integer: new Set(["compare", "signum", "numberOfLeadingZeros", "numberOfTrailingZeros", "bitCount", "highestOneBit"]),
  Long: new Set(["compare", "signum", "numberOfLeadingZeros", "numberOfTrailingZeros", "bitCount"]),
};
const isJdk = ({ className, method }) => Boolean(JDK[className]?.has(method));

/**
 * The mechanical class of a change, where one is sound, or null. `renamed-locals`: the two sides differ only in the names
 * of the method's parameters and locals. The compare numbers locals by first use and drops `this.`, which reads a local
 * that shares a field's name as that field, and numbers a name declared twice in sibling blocks as one; here `this.` is
 * kept and each declaration takes a fresh name that its uses until the next declaration of that name read, so two texts
 * agree only if one is the other with its locals renamed. (26.1 ships with its locals' names, which the obfuscated
 * releases before it do not have.)
 */
function mechanicalClass(before, after) {
  if (before.length === 0 || after.length === 0) return null;
  const { ALLOWANCES } = require("./compare-gate-sources.cjs");
  const kept = ALLOWANCES.filter(({ name }) => ["comments", "annotations", "final", "whitespace"].includes(name));
  const text = (items) => items.map(({ raw }) => scopedLocals(kept.reduce((body, { apply }) => apply(body), raw))).sort().join("\n");
  return text(before) === text(after) ? "renamed-locals" : null;
}

const JAVA_KEYWORDS = new Set("abstract assert boolean break byte case catch char class continue default do double else enum extends false float for if implements import instanceof int interface long native new null package private protected public return short static strictfp super switch synchronized this throw throws transient true try var void volatile while yield".split(" "));
const PRIMITIVES = new Set("boolean byte char double float int long short var".split(" "));
const DECLARATION_OPENERS = new Set(["(", "{", ";", ",", "}", "final", undefined]);

/** Where the type that ends at token `end` starts (Name(.Name)*(<...>)?([])*), or -1 when no type ends there. */
function typeStart(tokens, end) {
  let at = end;
  while (tokens[at] === "]" && tokens[at - 1] === "[") at -= 2;
  if (tokens[at] === ">") {
    let depth = 0;
    for (; at >= 0; at -= 1) {
      if (tokens[at] === ">") depth += 1;
      else if (tokens[at] === "<") {
        depth -= 1;
        if (depth === 0) break;
      } else if (!/^[\w$]+$/.test(tokens[at]) && ![",", ".", "?", "[", "]"].includes(tokens[at])) return -1;
    }
    at -= 1;
  }
  if (!(PRIMITIVES.has(tokens[at]) || /^[A-Z][\w$]*$/.test(tokens[at] ?? ""))) return -1;
  while (tokens[at - 1] === "." && /^[\w$]+$/.test(tokens[at - 2] ?? "")) at -= 2;
  return at;
}

/** A method's text with each declared parameter and local renamed by declaration order; a name never declared is kept. */
function scopedLocals(text) {
  const tokens = text.match(/"(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*'|[A-Za-z_$][\w$]*|\d[\w.]*|\S/g) ?? [];
  const current = new Map();
  const out = [];
  tokens.forEach((token, index) => {
    if (!/^[a-z_$][\w$]*$/.test(token) || JAVA_KEYWORDS.has(token) || tokens[index - 1] === "." || tokens[index + 1] === "(") {
      out.push(token);
      return;
    }
    const next = tokens[index + 1];
    let declared = false;
    if (["=", ";", ":", ",", ")"].includes(next)) {
      const start = typeStart(tokens, index - 1);
      declared = start >= 0 && DECLARATION_OPENERS.has(tokens[start - 1]);
    }
    if (next === "-" && tokens[index + 2] === ">" && DECLARATION_OPENERS.has(tokens[index - 1])) declared = true;
    if (declared) current.set(token, `v${current.size}_${out.length}`);
    out.push(current.get(token) ?? token);
  });
  // (a fresh name is its declaration's position, so that two declarations of one name never meet)
  const renumber = new Map();
  return out.map((token) => (/^v\d+_\d+$/.test(token) ? (renumber.has(token) ? renumber.get(token) : renumber.set(token, `v${renumber.size}`).get(token)) : token)).join(" ");
}

/** The Rust tests of native/src: name to body. */
function rustTests() {
  const out = new Map();
  const walk = (directory) => {
    for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
      const file = path.join(directory, entry.name);
      if (entry.isDirectory()) walk(file);
      else if (entry.name.endsWith(".rs")) {
        const text = fs.readFileSync(file, "utf8").replaceAll("\r\n", "\n");
        for (const match of text.matchAll(/#\[test\]\s*(?:#\[[^\]]*\]\s*)*fn (\w+)\s*\(\)[^{]*\{/g)) {
          let depth = 1;
          let at = match.index + match[0].length;
          while (depth > 0 && at < text.length) {
            if (text[at] === "{") depth += 1;
            else if (text[at] === "}") depth -= 1;
            at += 1;
          }
          out.set(match[1], text.slice(match.index + match[0].length, at));
        }
      }
    }
  };
  walk(sourceRoot);
  return out;
}

/**
 * Problems with a helper's tests: each must exist, and call the function by name or be one the reach probe saw run it
 * (generated/shared-reach.json `ranBy`).
 */
function helperProblems(helper, tests, reach = require("./reach-probe.cjs").readReach()) {
  const ranBy = reach?.byFunction.get(`${helper.file}:${helper.function}`)?.ranBy ?? new Set();
  const problems = [];
  if (!helper.note) problems.push("no note");
  if (!Array.isArray(helper.tests) || helper.tests.length === 0) problems.push("no tests");
  for (const name of helper.tests ?? []) {
    const body = tests.get(name);
    if (body === undefined) problems.push(`no test ${name}`);
    else if (!(callsByName(helper.function, body) || callsQualified(helper, body) || ranBy.has(name))) problems.push(`${name} neither calls ${helper.function} nor ran it`);
  }
  return problems;
}

/**
 * Whether a test's body calls the helper by a path that can only be its own: `Type::name(` (or `<_ as Trait>::name(`)
 * of a type or trait whose impl or declaration in the helper's file has a fn of that name, or `module::name(` of the
 * helper's own module. A shared name (new, parse, default) is then still one function's.
 */
function callsQualified(helper, body) {
  const owners = ownersOf(helper.file).get(helper.function) ?? new Set();
  const stem = path.basename(helper.file, ".rs");
  const names = [...owners, ...(stem === "mod" ? [path.basename(path.dirname(helper.file))] : [stem])];
  return names.some((owner) => new RegExp(`\\b${owner}>?::${helper.function}(?:::<[^()]*>)?\\s*\\(`).test(body));
}

const owners = new Map();
/** The types and traits of a source whose impl or trait block declares each fn name, by name. */
function ownersOf(file) {
  if (owners.has(file)) return owners.get(file);
  const text = fs.readFileSync(path.join(sourceRoot, file), "utf8").replaceAll("\r\n", "\n");
  const found = new Map();
  for (const match of text.matchAll(/^\s*(?:pub(?:\([a-z]+\))?\s+)?(?:impl(?:<[^>]*>)?\s+(?:[\w:]+(?:<[^>]*>)?\s+for\s+)?|trait\s+)(\w+)[^{;]*\{/gm)) {
    let depth = 1;
    let at = match.index + match[0].length;
    const start = at;
    while (depth > 0 && at < text.length) {
      if (text[at] === "{") depth += 1;
      else if (text[at] === "}") depth -= 1;
      at += 1;
    }
    for (const [, name] of text.slice(start, at).matchAll(/\bfn\s+(\w+)/g)) found.set(name, (found.get(name) ?? new Set()).add(match[1]));
  }
  owners.set(file, found);
  return found;
}

/**
 * Whether a test's body calls a function by its name: `name(`. A name that more than one native function declares (new,
 * parse, default) does not say which is called, so it counts only from the reach probe's `ranBy`.
 */
function callsByName(name, body) {
  if (!sharedNames) {
    const counts = new Map();
    for (const entry of sharedFunctions()) counts.set(entry.function, (counts.get(entry.function) ?? 0) + 1);
    sharedNames = new Set([...counts].filter(([, count]) => count > 1).map(([item]) => item));
  }
  return !sharedNames.has(name) && new RegExp(`\\b${name}\\s*\\(`).test(body);
}
let sharedNames = null;

/** Each boundary a function's reach crosses: releases on both sides of it ran the function. Null when there is no record. */
function crossed(reach, key, registered) {
  const entry = reach?.byFunction.get(key);
  if (!entry || entry.unprobed) return null;
  return (boundary) => {
    const at = registered.indexOf(boundary);
    const before = registered.slice(0, at).some((release) => entry.releases.has(release));
    const after = registered.slice(at).some((release) => entry.releases.has(release));
    return before && after ? null : before ? "reached only before" : after ? "reached only from" : "reached on neither side";
  };
}

/**
 * The names no function outside the tests reaches: called by none, or only by such names, an exported N-API entry
 * being a caller of its own. Callers are matched by name (scripts/shared-functions.cjs), so a name two functions
 * share is live if either is called.
 */
function deadFunctions(entries) {
  const callers = new Map();
  for (const entry of entries) callers.set(entry.function, new Set([...(callers.get(entry.function) ?? []), ...entry.callers.filter((caller) => caller !== entry.function)]));
  const entries_ = new Set(entries.filter((entry) => entry.status === "entry").map((entry) => entry.function));
  const dead = new Set([...callers].filter(([name, from]) => from.size === 0 && !entries_.has(name)).map(([name]) => name));
  for (let grew = true; grew;) {
    grew = false;
    for (const [name, from] of callers) {
      if (!dead.has(name) && !entries_.has(name) && [...from].every((caller) => dead.has(caller))) {
        dead.add(name);
        grew = true;
      }
    }
  }
  return dead;
}

function generate() {
  const { citedMethods, treeOf, filesOf, methodsOf } = require("./compare-gate-sources.cjs");
  const { readReach } = require("./reach-probe.cjs");
  const registered = canonicalReleases();
  const pairs = registered.slice(1).map((release, index) => [registered[index], release]);
  const missingTrees = registered.filter((release) => !treeOf(release));
  if (missingTrees.length > 0) throw new Error(`no compare tree for ${missingTrees.join(", ")}: run scripts/compare-gate-sources.cjs --materialize`);
  const functions = ungatedFunctions();
  const comments = commentsOf([...new Set(functions.map((entry) => entry.file))]);
  const reviews = readReviews();
  const reach = readReach();
  const tests = rustTests();
  const dead = deadFunctions(sharedFunctions());
  const used = new Set();
  const changesOf = new Map();
  /** A cited method's changes at every boundary, each with its hash and mechanical class. */
  const changesOfMethod = (className, method) => {
    const key = `${className}#${method}`;
    if (!changesOf.has(key)) {
      changesOf.set(key, pairs.flatMap((pair) => {
        const [before, after] = pair.map((release) => overloads(release, className, method));
        const texts = (items) => items.map(({ text }) => text).sort();
        const left = texts(before);
        const right = texts(after);
        if (left.length === 0 && right.length === 0) return [];
        const { sameUpToLeftovers } = require("./compare-gate-sources.cjs");
        if (left.length > 0 && right.length > 0 && sameUpToLeftovers(left.join("\n"), right.join("\n"))) return [];
        const change = left.length === 0 ? "added" : right.length === 0 ? "removed" : "body";
        const mechanical = mechanicalClass(before, after);
        return [{ boundary: pair[1], change, hash: changeHash(left.join("\n"), right.join("\n")), ...(mechanical ? { mechanical } : {}) }];
      }));
    }
    return changesOf.get(key);
  };
  const entries = functions.map(({ file, function: name }) => {
    const key = `${file}:${name}`;
    const glue = /^napi_/.test(file);
    const cited = citedMethods(comments.get(key) ?? "");
    if (cited.length === 0) {
      const helper = reviews.helpers.find((review) => review.file === file && review.function === name);
      if (helper) {
        used.add(helper);
        const problems = helperProblems(helper, tests, reach);
        return { file, function: name, status: problems.length === 0 ? "helper" : "uncited", tests: helper.tests, ...(problems.length > 0 ? { problems } : {}) };
      }
      return { file, function: name, status: glue ? "glue" : "uncited" };
    }
    const methods = cited.map(({ className, method }) => `${className}#${method}`).sort();
    const jdk = cited.filter(isJdk).map(({ className, method }) => `${className}#${method}`).sort();
    const present = cited.filter((item) => !isJdk(item) && declared(registered, item.className, item.method));
    if (present.length === 0) return { file, function: name, methods, status: jdk.length === cited.length ? "jdk" : "unresolved" };
    const absent = methods.filter((method) => !jdk.includes(method) && !present.some(({ className, method: item }) => `${className}#${item}` === method));
    const changes = present.flatMap(({ className, method }) => changesOfMethod(className, method).map((change) => ({ method: `${className}#${method}`, ...change })));
    changes.sort((left, right) => registered.indexOf(left.boundary) - registered.indexOf(right.boundary) || left.method.localeCompare(right.method));
    if (changes.length === 0) return { file, function: name, methods, ...(absent.length > 0 ? { absent } : {}), status: "source-unchanged" };
    const reachAt = crossed(reach, key, registered);
    const open = [];
    let reviewedAny = false;
    for (const boundary of [...new Set(changes.map((change) => change.boundary))]) {
      const here = changes.filter((change) => change.boundary === boundary);
      const unreached = reachAt?.(boundary);
      const functionReviews = reviews.functions.filter((review) => review.file === file && review.function === name && review.boundary === boundary);
      // (a review that names a change the record has is in use, whatever else accounts for that change first; two
      // reviews of one boundary may each account for some of its changes)
      for (const change of here) {
        const methodReview = reviews.methods.find((review) => review.method === change.method && review.boundary === boundary && review.hash === change.hash);
        if (methodReview) used.add(methodReview);
        const functionReview = functionReviews.find((review) => review.hashes?.[change.method] === change.hash && REASONS[review.reason] && review.note
          && (review.reason !== "no-engine-caller" || dead.has(name)));
        if (functionReview) used.add(functionReview);
        if (unreached) change.by = `unreached: ${unreached} ${boundary}`;
        else if (change.mechanical) change.by = change.mechanical;
        else if (methodReview) change.by = "neutral";
        else if (functionReview) change.by = functionReview.reason;
        if (change.by === "neutral" || (functionReview && change.by === functionReview.reason)) reviewedAny = true;
      }
      if (here.some((change) => !change.by)) open.push(boundary);
    }
    const status = open.length > 0 ? "review" : reviewedAny ? "reviewed" : "mechanical";
    return { file, function: name, methods, ...(absent.length > 0 ? { absent } : {}), changes, status, ...(open.length > 0 ? { open } : {}) };
  });
  const totals = {};
  for (const entry of entries) totals[entry.status] = (totals[entry.status] ?? 0) + 1;
  const boundaries = entries.flatMap((entry) => [...new Set((entry.changes ?? []).map((change) => change.boundary))].map((boundary) => (entry.changes.filter((change) => change.boundary === boundary))));
  totals.functionBoundaries = boundaries.length;
  totals.functionBoundariesOpen = entries.reduce((sum, entry) => sum + (entry.open?.length ?? 0), 0);
  totals.functionBoundariesUnreached = boundaries.filter((here) => here.every((change) => change.by?.startsWith("unreached"))).length;
  totals.functionBoundariesMechanical = boundaries.filter((here) => here.every((change) => change.by && !change.by.startsWith("unreached")) && here.every((change) => change.by === change.mechanical)).length;
  totals.functionBoundariesReviewed = totals.functionBoundaries - totals.functionBoundariesOpen - totals.functionBoundariesUnreached - totals.functionBoundariesMechanical;
  const rules = ruleClaims(registered);
  totals.rulesChecked = rules.filter((rule) => rule.status === "checked").length;
  totals.rulesUnverified = rules.filter((rule) => rule.status === "unverified").length;
  totals.rulesDiffer = rules.filter((rule) => rule.status === "differs").length;
  const staleReviews = Object.values(reviews).flat().filter((review) => !used.has(review))
    .map((review) => `${review.from}: ${review.method ?? `${review.file}#${review.function}`}${review.boundary ? `@${review.boundary}` : ""}`);
  return { schema: 2, generatedBy: "scripts/shared-equivalence.cjs", releases: registered.length, reach: reach ? `generated/shared-reach.json (probed at ${reach.probedAt})` : null, totals, staleReviews, functions: entries, rules };
}

const document = (value) => `${JSON.stringify(value, null, 1)}\n`;

/** A line diff of two texts (whitespace collapsed per line), the common lines around each change cut to `context`. */
function lineDiff(left, right, context = 2) {
  const split = (text) => text.split("\n").map((line) => line.trim().replace(/\s+/g, " ")).filter(Boolean);
  const a = split(left);
  const b = split(right);
  const table = Array.from({ length: a.length + 1 }, () => new Int32Array(b.length + 1));
  for (let i = a.length - 1; i >= 0; i -= 1) for (let j = b.length - 1; j >= 0; j -= 1) table[i][j] = a[i] === b[j] ? table[i + 1][j + 1] + 1 : Math.max(table[i + 1][j], table[i][j + 1]);
  const lines = [];
  let i = 0;
  let j = 0;
  while (i < a.length || j < b.length) {
    if (i < a.length && j < b.length && a[i] === b[j]) {
      lines.push(`  ${a[i]}`);
      i += 1;
      j += 1;
    } else if (j < b.length && (i === a.length || table[i][j + 1] >= table[i + 1][j])) lines.push(`+ ${b[j++]}`);
    else lines.push(`- ${a[i++]}`);
  }
  const keep = lines.map((line, index) => lines.slice(Math.max(0, index - context), index + context + 1).some((near) => !near.startsWith("  ")));
  return lines.map((line, index) => (keep[index] ? line : keep[index - 1] ? "  ..." : null)).filter((line) => line !== null).join("\n");
}

/** Prints the open changes of the functions named (file:function, or a file), as a reviewer reads them, from the record. */
function show(names) {
  const { canonicalReleases: releases } = require("./release-inventory.cjs");
  const registered = releases();
  const record = JSON.parse(fs.readFileSync(outputPath, "utf8"));
  const all = process.argv.includes("--all");
  for (const entry of record.functions) {
    if (!names.some((name) => name === entry.file || name === `${entry.file}:${entry.function}`)) continue;
    console.log(`\n## ${entry.file}:${entry.function} (${entry.status})`);
    for (const change of entry.changes ?? []) {
      if (change.by && !all) continue;
      const [className, method] = change.method.split("#");
      const pair = [registered[registered.indexOf(change.boundary) - 1], change.boundary];
      const [before, after] = pair.map((release) => overloads(release, className, method).map(({ raw }) => raw).join("\n"));
      console.log(`\n### ${change.method} ${pair[0]} -> ${pair[1]} (${change.change}) hash ${change.hash}${change.by ? ` by ${change.by}` : ""}`);
      console.log(change.change === "body" ? lineDiff(before, after) : (before || after).split("\n").map((line) => `${before ? "-" : "+"} ${line.trim()}`).join("\n"));
    }
  }
}

/** Prints each open change of the methods of the classes named (Class, or Class#method), once, with the functions citing it. */
function showMethods(names) {
  const { canonicalReleases: releases } = require("./release-inventory.cjs");
  const registered = releases();
  const record = JSON.parse(fs.readFileSync(outputPath, "utf8"));
  const changes = new Map();
  for (const entry of record.functions) {
    for (const change of entry.changes ?? []) {
      if (change.by || !names.some((name) => name === change.method || name === change.method.split("#")[0])) continue;
      const id = `${change.method}@${change.boundary}`;
      if (!changes.has(id)) changes.set(id, { ...change, functions: [] });
      changes.get(id).functions.push(`${entry.file}:${entry.function}`);
    }
  }
  for (const change of [...changes.values()].sort((left, right) => left.method.localeCompare(right.method) || registered.indexOf(left.boundary) - registered.indexOf(right.boundary))) {
    const [className, method] = change.method.split("#");
    const pair = [registered[registered.indexOf(change.boundary) - 1], change.boundary];
    const [before, after] = pair.map((release) => overloads(release, className, method).map(({ raw }) => raw).join("\n"));
    console.log(`\n### ${change.method} ${pair[0]} -> ${pair[1]} (${change.change}) hash ${change.hash}\ncited by ${change.functions.join(", ")}`);
    console.log(change.change === "body" ? lineDiff(before, after) : (before || after).split("\n").map((line) => `${before ? "-" : "+"} ${line.trim()}`).join("\n"));
  }
}

if (require.main === module && process.argv[2] === "--show") show(process.argv.slice(3).filter((name) => !name.startsWith("--")));
else if (require.main === module && process.argv[2] === "--show-methods") showMethods(process.argv.slice(3));
else if (require.main === module) {
  const generated = document(generate());
  if (process.argv.includes("--check")) {
    const current = fs.existsSync(outputPath) ? fs.readFileSync(outputPath, "utf8").replace(/\r\n/g, "\n") : "";
    if (current !== generated) {
      console.error("generated/shared-equivalence.json is stale: run node scripts/shared-equivalence.cjs");
      process.exit(1);
    }
  } else {
    fs.writeFileSync(outputPath, generated);
    console.log(JSON.stringify(JSON.parse(generated).totals));
  }
}

module.exports = { ungatedFunctions, profileRules, ruleClaims, outputPath, readReviews, changeHash, overloads, scopedLocals, mechanicalClass, helperProblems, rustTests, REASONS, reviewsDirectory };
