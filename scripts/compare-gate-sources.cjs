// generated/gate-sources.json (B007): for each version gate of the shared native code, the vanilla method it stands for and
// what that method did across the gate's boundary, read from the two releases either side of it.
//
// generated/physics-breakpoints.json refuses a pair of releases from different decompiles, which leaves a gate at such a
// pair with no finding. The source cache does not solve that: its trees come from two tools (a commit of the extracted
// repository, or Vineflower over the client), and they meet at 1.15, 1.18.1, 1.18.2, 1.19, 1.19.2, 1.19.3, 26.1.2 and 26.2,
// the very pairs left without a finding. So the modern releases are compared in MC_COMPARE_CACHE (default compare-cache beside
// the source cache): each release's Mojang-named jar put through the one Vineflower of DECOMPILER by
// `node scripts/compare-gate-sources.cjs --materialize [release...]`, its world, util, core and client/player packages.
// The legacy six come from MC_LEGACY_SOURCES, also Vineflower's, remapped: over 1.13.2 to 1.14.4 a raw comparison and an
// allowed one find the same 218 of 973 methods, and a name that mapping left as method_123 stands for any name there.
// What a decompile prints differently is taken out first, by ALLOWANCES alone: each is listed here and in the output.
//
// A gate's method comes from one of two places, and neither is inferred from the boundary:
//   citation  the Rust function's comments name the class and method, or a line of the release they name falls inside it
//   review    audit/gate-source-reviews.json: someone read both sides and named it, with what changed. A change that is
//             no method's (a constant's value, a field's initializer, an enum constant, a constructor) is named under
//             `members` as Class#name, and held to a difference of that declaration's text across the pair the same way. A
//             change that is no class's but the release's own version json (the Java major it declares) is named under
//             `versionJson` as a key path, read in each release's version json at its pin (generated/mojang-releases.json)
// Then `found` holds the methods of those that really differ across the pair, or are there on one side only. A method named
// by a review that does not differ is listed in `refuted`: nothing makes a gate source-compared but a difference found here.
//   node scripts/compare-gate-sources.cjs            writes generated/gate-sources.json
//   node scripts/compare-gate-sources.cjs --check    fails on a difference
const childProcess = require("node:child_process");
const fs = require("node:fs");
const path = require("node:path");
const { canonicalReleases } = require("./release-inventory.cjs");
const { compareReleases } = require("./registered-releases.cjs");
const { LEGACY_RELEASES } = require("./legacy-release-data.cjs");
const { sourceCache, mcSourceRoot, DECOMPILER, decompilerJar, clientJarRoot, NAMED_JARS, CLIENT_DATA, PINNED_CLIENTS } = require("./source-pins.cjs");
const { methodDeclarations, memberDeclarations } = require("./java-declarations.cjs");
const { scanRust, citationsOf, functionComments } = require("./movement-sites.cjs");
const { gatesOf } = require("./shared-gates.cjs");

const root = path.resolve(__dirname, "..");
const outputPath = path.join(root, "generated", "gate-sources.json");
const reviewsPath = path.join(root, "audit", "gate-source-reviews.json");
const nativeRoot = path.join(root, "native", "src");
const legacyRoot = process.env.MC_LEGACY_SOURCES ?? path.join(sourceCache(), "..", "legacy-src");
const compareCache = () => process.env.MC_COMPARE_CACHE ?? path.join(sourceCache(), "..", "compare-cache");
const legacy = new Set(LEGACY_RELEASES.map(({ version }) => version));

/** What may differ between two decompiles of the same code. Applied in order; nothing else is forgiven. */
const ALLOWANCES = [
  { name: "comments", why: "a decompiler's notes and a mapping's javadoc are no code", apply: (text) => text.replace(/\/\*[\s\S]*?\*\//g, " ").replace(/\/\/[^\n]*/g, " ") },
  { name: "annotations", why: "@Override, @Nullable and the side markers change no behaviour, and the trees differ in which they print", apply: (text) => text.replace(/@(?:Override|Nullable|Deprecated|OnlyIn\([^)]*\)|Environment\([^)]*\)|SuppressWarnings\([^)]*\)|NotNull|Contract\([^)]*\)|FunctionalInterface)\b/g, " ") },
  { name: "final", why: "a local's or parameter's `final` is the decompiler's reading of it, never in the bytecode's behaviour", apply: (text) => text.replace(/\bfinal\s+/g, "") },
  { name: "whitespace", why: "line endings changed at 1.18.2 and indents between trees", apply: (text) => text.replace(/\s+/g, " ").trim() },
  { name: "this", why: "`this.` on a member is printed or not by decompile", apply: (text) => text.replace(/\bthis\./g, "") },
  { name: "local names", why: "the names of parameters and locals are not in the bytecode: each is read by its order of first use", apply: renameLocals },
];

/** Identifiers in lower case that are no member access, call, keyword or type: a method's parameters and locals. */
const KEYWORDS = new Set("abstract assert boolean break byte case catch char class continue default do double else enum extends false float for if implements import instanceof int interface long native new null package private protected public return short static strictfp super switch synchronized this throw throws transient true try var void volatile while yield".split(" "));
function renameLocals(text) {
  const seen = new Map();
  return text.replace(/(?<![\w.$"'])[a-z$_][\w$]*(?![\w$]*\s*\()/g, (name) => {
    if (KEYWORDS.has(name)) return name;
    if (!seen.has(name)) seen.set(name, `l${seen.size}`);
    return seen.get(name);
  });
}

const normalised = (text) => ALLOWANCES.reduce((body, { apply }) => apply(body), text);

/** The directory holding a release's net/minecraft, in the one decompile of each. */
function treeOf(release) {
  const tree = legacy.has(release) ? path.join(legacyRoot, release) : path.join(compareCache(), release, "client");
  return fs.existsSync(path.join(tree, "net", "minecraft")) ? tree : null;
}

/** The tree a citation's line numbers are of: the release's exact sources, as a reader has them. */
function citedTree(release) {
  const candidates = legacy.has(release) ? [path.join(legacyRoot, release)] : [path.join(sourceCache(), release, "client"), path.join(mcSourceRoot(), release, "client")];
  return candidates.find((tree) => fs.existsSync(path.join(tree, "net", "minecraft"))) ?? null;
}

const indexes = new Map();
/** Each class of a tree by its name, a name two packages hold keeping both. */
function classIndex(tree) {
  if (indexes.has(tree)) return indexes.get(tree);
  const found = new Map();
  const walk = (directory) => {
    for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
      const file = path.join(directory, entry.name);
      if (entry.isDirectory()) walk(file);
      else if (entry.name.endsWith(".java") && !entry.name.includes("$")) {
        const name = entry.name.slice(0, -5);
        if (!found.has(name)) found.set(name, []);
        found.get(name).push(file);
      }
    }
  };
  walk(path.join(tree, "net", "minecraft"));
  indexes.set(tree, found);
  return found;
}

const parsed = new Map();
/** A file's methods: `name` to the normalised text of its overloads, sorted, and each one's lines. */
function methodsOf(file) {
  if (parsed.has(file)) return parsed.get(file);
  const source = fs.readFileSync(file, "latin1").replaceAll("\r\n", "\n");
  const methods = new Map();
  for (const method of methodDeclarations(source)) {
    if (!methods.has(method.name)) methods.set(method.name, []);
    const lines = method.source.split("\n").length;
    methods.get(method.name).push({ text: normalised(method.source), raw: method.source, from: method.line, to: method.line + lines - 1 });
  }
  parsed.set(file, methods);
  return methods;
}

const parsedMembers = new Map();
/** A file's fields, enum constants and constructors: `name` to the normalised text of each declaration, and its kind. */
function membersOf(file) {
  if (parsedMembers.has(file)) return parsedMembers.get(file);
  const members = new Map();
  for (const member of memberDeclarations(fs.readFileSync(file, "latin1").replaceAll("\r\n", "\n"))) {
    if (!members.has(member.name)) members.set(member.name, []);
    members.get(member.name).push({ text: normalised(member.source), kind: member.kind, from: member.line });
  }
  parsedMembers.set(file, members);
  return members;
}

/** How a field, enum constant or constructor of a class differs across a pair, as `difference` has a method's. */
function memberDifference(pair, className, member) {
  const [before, after] = pair.map((release) => filesOf(treeOf(release), className).map(membersOf));
  const declared = (files) => files.flatMap((members) => members.get(member) ?? []);
  const left = declared(before);
  const right = declared(after);
  if (left.length === 0 && right.length === 0) return null;
  const kind = (left[0] ?? right[0]).kind;
  if (left.length === 0) return { change: "added", kind };
  if (right.length === 0) return { change: "removed", kind };
  const texts = (items) => items.map(({ text }) => text).sort().join("\n");
  return sameUpToLeftovers(texts(left), texts(right)) ? null : { change: "body", kind };
}

/** The files a class name is in a tree; a package moved between releases still finds it. */
const filesOf = (tree, name) => classIndex(tree).get(name) ?? [];

/**
 * A release's version json, at the sha1 Mojang's list pinned (scripts/fetch-jar-inputs.cjs versionJsonPins): from
 * MC_CLIENT_JAR_ROOT, or jar-inputs/clients beside the source cache, where fetch-jar-inputs keeps them. Null when no
 * copy at the pin is on this machine: another revision of the json is not the release's input.
 */
const versionJsons = new Map();
function pinnedVersionJson(release) {
  if (versionJsons.has(release)) return versionJsons.get(release);
  const { versionJsonPins } = require("./fetch-jar-inputs.cjs");
  const pin = versionJsonPins().get(release);
  const roots = [...new Set([clientJarRoot(), path.join(sourceCache(), "..", "jar-inputs", "clients")])];
  let found = null;
  for (const directory of roots) {
    const file = path.join(directory, release, "version.json");
    if (!pin || !fs.existsSync(file)) continue;
    const bytes = fs.readFileSync(file);
    if (require("node:crypto").createHash("sha1").update(bytes).digest("hex") === pin) {
      found = JSON.parse(bytes.toString("utf8"));
      break;
    }
  }
  versionJsons.set(release, found);
  return found;
}

/** How a key path of the version json differs across a pair: { before, after }, null when equal, undefined unread. */
function versionJsonDifference(pair, key) {
  const jsons = pair.map(pinnedVersionJson);
  if (jsons.some((json) => !json)) return undefined;
  const [before, after] = jsons.map((json) => key.split(".").reduce((value, part) => value?.[part], json) ?? null);
  return JSON.stringify(before) === JSON.stringify(after) ? null : { before, after };
}

/** How a method of a class differs across a pair: `body`, `added`, `removed`, or null. */
function difference(pair, className, method) {
  const [before, after] = pair.map((release) => filesOf(treeOf(release), className).map(methodsOf));
  if (before.length === 0 && after.length === 0) return null;
  const bodies = (files) => files.flatMap((methods) => methods.get(method) ?? []).map(({ text }) => text).sort();
  const left = bodies(before);
  const right = bodies(after);
  if (left.length === 0 && right.length === 0) return null;
  if (left.length === 0) return "added";
  if (right.length === 0) return "removed";
  return sameUpToLeftovers(left.join("\n"), right.join("\n")) ? null : "body";
}

/** Two texts that agree, a name the legacy mapping left unnamed on the left (method_123) standing for any on the right. */
function sameUpToLeftovers(left, right) {
  if (left === right) return true;
  if (!/\b(?:method|field|class)_\d+\b/.test(left)) return false;
  const pattern = left.replace(/[.*+?^${}()|[\]\\]/g, "\\$&").replace(/\b(?:method|field|class)_\d+\b/g, "[\\w$]+");
  return new RegExp(`^${pattern}$`).test(right);
}

/** The method a line of a release's tree falls in, for a citation of that line. */
function methodAt(release, className, from, to) {
  const tree = citedTree(release);
  const out = new Set();
  if (!tree) return out;
  for (const file of filesOf(tree, className)) {
    for (const [name, overloads] of methodsOf(file)) {
      if (overloads.some((overload) => overload.from <= to && overload.to >= from)) out.add(name);
    }
  }
  return out;
}

/** The releases a gate's boundary lies between, as the registered list has them. */
function pairOf(boundary, registered) {
  const first = registered.findIndex((release) => compareReleases(release, boundary) >= 0);
  return first > 0 ? [registered[first - 1], registered[first]] : null;
}

/** Each gated function's comments as `{ class, method }`: by name, or by a cited line. */
function citedMethods(comment) {
  const found = new Map();
  const add = (className, method) => found.set(`${className}#${method}`, { className, method });
  for (const [, className, method] of comment.matchAll(/\b([A-Z][A-Za-z0-9]+)(?:\.java)?[.#]([a-z][A-Za-z0-9]*)\b/g)) if (method !== "java") add(className, method);
  // (a method the legacy mapping left unnamed, by its intermediary name: AABB#method_583)
  for (const [, className, method] of comment.matchAll(/\b([A-Z][A-Za-z0-9]+)[.#](method_\d+)\b/g)) add(className, method);
  for (const { release, file, from, to } of citationsOf(comment)) {
    if (from === null || !release) continue;
    for (const method of methodAt(release, file, from, to)) add(file, method);
  }
  return [...found.values()];
}

function gatedFunctions() {
  const out = [];
  const files = fs.readdirSync(nativeRoot).filter((name) => name.endsWith(".rs") && !/tests\.rs$/.test(name)).sort();
  for (const file of files) {
    const text = fs.readFileSync(path.join(nativeRoot, file), "utf8").replaceAll("\r\n", "\n");
    const { functions, comments } = scanRust(text);
    for (const [name, boundaries] of gatesOf(text)) {
      const held = functions.filter((fn) => fn.name === name);
      const comment = held.map((fn) => functionComments(fn, comments)).join(" ");
      for (const boundary of boundaries) out.push({ file, function: name, boundary, comment });
    }
  }
  return out;
}

const readReviews = () => (fs.existsSync(reviewsPath) ? JSON.parse(fs.readFileSync(reviewsPath, "utf8")).reviews : []);

function generate() {
  const registered = canonicalReleases();
  const reviews = readReviews();
  const usedReviews = new Set();
  const gates = [];
  const missing = new Set();
  for (const { file, function: name, boundary, comment } of gatedFunctions()) {
    const pair = pairOf(boundary, registered);
    if (!pair) {
      gates.push({ file, function: name, boundary, pair: null, reason: "below every registered release" });
      continue;
    }
    for (const release of pair) if (!treeOf(release)) missing.add(release);
    if (pair.some((release) => !treeOf(release))) {
      gates.push({ file, function: name, boundary, pair, reason: "no tree on this machine" });
      continue;
    }
    const entry = { file, function: name, boundary, pair, found: [], refuted: [] };
    const seen = new Set();
    for (const { className, method } of citedMethods(comment)) {
      const change = difference(pair, className, method);
      if (change && !seen.has(`${className}#${method}`)) {
        seen.add(`${className}#${method}`);
        entry.found.push({ class: className, method, change, via: "citation" });
      }
    }
    for (const review of reviews.filter((item) => item.file === file && item.function === name && item.boundary === boundary)) {
      usedReviews.add(review);
      for (const target of review.methods ?? []) {
        const [className, method] = target.split("#");
        const change = difference(pair, className, method);
        const id = `${className}#${method}`;
        if (!change) entry.refuted.push({ class: className, method, note: review.note });
        else if (seen.has(id)) Object.assign(entry.found.find((item) => `${item.class}#${item.method}` === id), { via: "review", note: review.note });
        else {
          seen.add(id);
          entry.found.push({ class: className, method, change, via: "review", note: review.note });
        }
      }
      for (const key of review.versionJson ?? []) {
        const change = versionJsonDifference(pair, key);
        if (change === undefined) entry.reason = `no version json at its pin for ${pair.join(" or ")} on this machine`;
        else if (!change) entry.refuted.push({ versionJson: key, note: review.note });
        else entry.found.push({ versionJson: key, change: "value", before: change.before, after: change.after, via: "review", note: review.note });
      }
      for (const target of review.members ?? []) {
        const [className, member] = target.split("#");
        const change = memberDifference(pair, className, member);
        if (!change) entry.refuted.push({ class: className, member, note: review.note });
        else entry.found.push({ class: className, member, kind: change.kind, change: change.change, via: "review", note: review.note });
      }
    }
    gates.push(entry);
  }
  const unused = reviews.filter((review) => !usedReviews.has(review)).map(({ file, function: name, boundary }) => `${file}#${name}@${boundary}`);
  return { schema: 1, generatedBy: "scripts/compare-gate-sources.cjs", decompiler: DECOMPILER, allowances: ALLOWANCES.map(({ name, why }) => ({ name, why })), missingTrees: [...missing].sort(compareReleases), staleReviews: unused, gates };
}

/** Puts the releases through the one decompiler, a partial run never looking finished. */
function materialize(wanted) {
  const packages = ["world", "util", "core", "client/player"].map((name) => `--only=net/minecraft/${name}`);
  for (const release of wanted.length > 0 ? wanted : canonicalReleases().filter((name) => !legacy.has(name))) {
    const target = path.join(compareCache(), release);
    if (fs.existsSync(path.join(target, "client", "net"))) continue;
    const directory = path.join(clientJarRoot(), release);
    const named = path.join(directory, `intermediary-${release}-mojmap-${release}`, `minecraft-${release}-fabric-merged+custom-no-environment+fixed-mojmap+intermediary.jar`);
    const jar = fs.existsSync(named) ? named : path.join(directory, `minecraft-${release}-client.jar`);
    // the jar and the decompiler only at their pins: a Mojang-named jar by sha256, an unobfuscated client by sha1
    const digest = (file, algorithm) => require("node:crypto").createHash(algorithm).update(fs.readFileSync(file)).digest("hex");
    const [algorithm, pinned] = NAMED_JARS[release] ? ["sha256", NAMED_JARS[release]] : ["sha1", CLIENT_DATA[release] ?? PINNED_CLIENTS[release]?.client];
    if (!pinned) throw new Error(`${release}: no pinned jar to decompile`);
    if (!fs.existsSync(jar) || digest(jar, algorithm) !== pinned) throw new Error(`${release}: ${jar} is not the pinned jar (${algorithm} ${pinned})`);
    if (!fs.existsSync(decompilerJar()) || digest(decompilerJar(), "sha1") !== DECOMPILER.sha1) throw new Error(`${DECOMPILER.name} ${DECOMPILER.version} is not at ${decompilerJar()}`);
    const staging = `${target}.partial`;
    fs.rmSync(staging, { recursive: true, force: true });
    fs.mkdirSync(path.join(staging, "client"), { recursive: true });
    childProcess.execFileSync("java", ["-Xmx4g", "-jar", decompilerJar(), "--silent", "--folder", ...packages, jar, path.join(staging, "out")], { stdio: ["ignore", "ignore", "inherit"] });
    fs.renameSync(path.join(staging, "out", "net"), path.join(staging, "client", "net"));
    fs.rmSync(path.join(staging, "out"), { recursive: true, force: true });
    fs.renameSync(staging, target);
    console.log(release);
  }
}

function main() {
  if (process.argv.includes("--materialize")) {
    materialize(process.argv.slice(3));
    return;
  }
  const output = `${JSON.stringify(generate(), null, 1)}\n`;
  if (process.argv.includes("--check")) {
    const present = fs.existsSync(outputPath) ? fs.readFileSync(outputPath, "utf8").replaceAll("\r\n", "\n") : "";
    if (present !== output) throw new Error("generated/gate-sources.json differs from a fresh generation");
    return;
  }
  fs.writeFileSync(outputPath, output);
}

if (require.main === module) {
  try {
    main();
  } catch (error) {
    console.error(error.stack);
    process.exit(1);
  }
}

module.exports = { membersOf, versionJsonDifference, ALLOWANCES, normalised, renameLocals, difference, memberDifference, methodAt, pairOf, citedMethods, treeOf, citedTree, filesOf, methodsOf, gatedFunctions, reviewsPath, sameUpToLeftovers };
