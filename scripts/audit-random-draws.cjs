// test/fixtures/random-draws.tsv: every random draw near movement, in every registered release, by a walk of its own source.
// The trees are Mojang-named: 42 of the source cache, 26.2 and 26.3 of mc-source, and for 1.8.8 to 1.13.2 each client decompiled from its pinned jar
// under legacy-mappings' names, so one set of class names serves all 50 and nothing is a rename.
//
// A consumer is a method of an audited class, or a block's method that is handed an entity, that draws. A draw is a call on a Random, a helper of
// Mth or Util, or a Random handed to something else. `reach` walks the calls a LocalPlayer's own tick makes on itself,
// through its superclasses and into helpers, so a draw routed through a helper shows with the path to it.
//   node scripts/audit-random-draws.cjs --legacy DIR [--check]
// Rows: release, kind (consumer or reach), Class.method, what is drawn, cosmetic or state, and for reach the path.
const fs = require("node:fs");
const path = require("node:path");
const { sourceCache, mcSourceRoot } = require("./source-pins.cjs");

const repositoryRoot = path.resolve(__dirname, "..");
const outputPath = path.join(repositoryRoot, "test", "fixtures", "random-draws.tsv");

const LEGACY = ["1.8.8", "1.9.4", "1.10.2", "1.11.2", "1.12.2", "1.13.2"];

/** Directories whose classes the call walk can enter. */
const ENTITY_DIRECTORIES = ["world/entity", "client/player", "server/level"];
/** Classes audited as consumers, by a path under net/minecraft. */
const AUDITED = /^(world\/entity\/(Entity|LivingEntity|Mob|PathfinderMob|AgeableMob|ExperienceOrb|Mob|player\/Player|item\/.*|vehicle\/.*|projectile\/.*|animal\/(horse\/.*|camel\/.*|Pig.*|pig\/.*|.*Horse.*|Llama|nautilus\/.*|equine\/.*|happyghast\/.*|HappyGhast)|monster\/Strider|boss\/.*|Interaction|TamableAnimal)|client\/player\/(LocalPlayer|AbstractClientPlayer|RemotePlayer|Input|KeyboardInput)|server\/level\/ServerPlayer)\.java$|^world\/entity\/(item|vehicle|projectile|fishing)\//;
const BLOCK_CALLBACKS = new Set(["entityInside", "stepOn", "fallOn", "updateEntityMovementAfterFallOn", "onEntityCollision", "onSteppedOn", "onLandedUpon", "onEntityLand"]);
const TICK_ENTRIES = ["tick", "rideTick", "aiStep", "serverAiStep", "travel", "move", "baseTick"];

const DRAW = /\b(?:this\.)?(?:level(?:\(\))?\.)?(?:level\.)?(?:random|getRandom\(\))\.(next\w*|triangle|consumeCount)\(|\bMath\.random\(\)|\bMth\.(randomBetween\w*|nextInt|nextFloat|nextDouble|nextBoolean)\(|\bUtil\.(getRandom\w*|shuffle\w*)\(|\bUUID\.randomUUID\(\)|\bMth\.createInsecureUUID\(|\bRandomSource\.create\w*\(|\bnew Random\(/g;
/** A generator handed on: the callee draws, whoever it is. */
const HANDED = /([A-Za-z_][\w.]*)\(([^();]*(?:\([^()]*\)[^();]*)*)\b(?:this\.)?(?:level(?:\(\))?\.)?(?:random|getRandom\(\))\s*[,)]/g;
const COSMETIC = /addParticle|addAlwaysVisibleParticle|sendParticles|spawnParticle|playSound|playLocalSound|makeSound|gameEvent|Particle|Sound/;
const KEYWORDS = new Set(["if", "for", "while", "switch", "catch", "return", "new", "super", "this", "throw", "synchronized", "assert", "else", "do", "try", "yield"]);

/** Removes comments, strings and chars, keeping every line and offset. */
function blank(text) {
  let out = "";
  for (let i = 0; i < text.length;) {
    const c = text[i];
    const next = text[i + 1];
    if (c === "/" && next === "/") {
      const end = text.indexOf("\n", i);
      const stop = end < 0 ? text.length : end;
      out += " ".repeat(stop - i);
      i = stop;
    } else if (c === "/" && next === "*") {
      const end = text.indexOf("*/", i + 2);
      const stop = end < 0 ? text.length : end + 2;
      out += text.slice(i, stop).replace(/[^\n]/g, " ");
      i = stop;
    } else if (c === "\"" || c === "'") {
      let j = i + 1;
      while (j < text.length && text[j] !== c && text[j] !== "\n") j += text[j] === "\\" ? 2 : 1;
      out += c + " ".repeat(Math.max(j - i - 1, 0)) + c;
      i = j + 1;
    } else {
      out += c;
      i += 1;
    }
  }
  return out;
}

/** The classes of a source file: name, superclass and its methods' bodies, by a walk of the braces. */
function parseClasses(source) {
  const text = blank(source);
  const classes = [];
  const stack = [];
  let statement = 0;
  for (let i = 0; i < text.length; i += 1) {
    const c = text[i];
    if (c === ";" ) {
      statement = i + 1;
    } else if (c === "{") {
      const head = text.slice(statement, i);
      const top = stack[stack.length - 1];
      const declared = /\b(class|interface|enum|record)\s+([A-Za-z_]\w*)([^{]*)$/.exec(head);
      if ((!top || top.kind === "class") && declared) {
        const parent = /\bextends\s+([A-Za-z_][\w.]*)/.exec(declared[3]);
        const owner = { kind: "class", name: declared[2], parent: parent?.[1].split(".").pop() ?? null, methods: new Map() };
        classes.push(owner);
        stack.push(owner);
      } else if (top?.kind === "class") {
        const called = /([A-Za-z_]\w*)\s*\(/.exec(head.replace(/@[\w.]+(\([^)]*\))?/g, " "));
        const field = head.includes("=");
        if (field || !called) stack.push({ kind: "member", name: field ? "<field>" : "<init>", owner: top, start: i });
        else {
          const name = called[1] === top.name ? "<init>" : called[1];
          if (/\b\w*(Entity|Player)\s+\w+\s*[,)]/.test(head)) (top.touching ??= new Set()).add(name);
          stack.push({ kind: "member", name, owner: top, start: statement });
        }
      } else {
        stack.push({ kind: "block" });
      }
      statement = i + 1;
    } else if (c === "}") {
      const closed = stack.pop();
      if (closed?.kind === "member") {
        const bodies = closed.owner.methods.get(closed.name) ?? [];
        bodies.push(text.slice(closed.start, i));
        closed.owner.methods.set(closed.name, bodies);
      }
      statement = i + 1;
    }
  }
  return classes;
}

function walkJava(directory, found = []) {
  if (!fs.existsSync(directory)) return found;
  for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
    const full = path.join(directory, entry.name);
    if (entry.isDirectory()) walkJava(full, found);
    else if (entry.name.endsWith(".java")) found.push(full);
  }
  return found;
}

/** The statement around an offset, for telling a particle's draw from one that reaches state. */
function statementAt(body, at) {
  const from = Math.max(body.lastIndexOf(";", at), body.lastIndexOf("{", at), body.lastIndexOf("}", at)) + 1;
  let to = body.indexOf(";", at);
  if (to < 0) to = body.length;
  return body.slice(from, to);
}

/** The draws of one method body, each once. */
function drawsOf(body) {
  const found = new Map();
  const add = (what, at) => {
    const cosmetic = COSMETIC.test(statementAt(body, at));
    const before = found.get(what);
    found.set(what, before === undefined ? cosmetic : before && cosmetic);
  };
  for (const match of body.matchAll(DRAW)) add(match[0].replace(/^this\./, "").replace(/\($/, "").replace(/^var\d+\./, ""), match.index);
  // (a tree without local names holds a generator in a var: a local, or a parameter of the method)
  const held = new Set([...body.matchAll(/\b(?:RandomSource|Random)\s+(\w+)\s*[=,)]/g)].map((match) => match[1]));
  for (const name of held) {
    for (const match of body.matchAll(new RegExp(String.raw`\b${name}\.(next\w*|triangle)\(`, "g"))) add(`random.${match[1]}`, match.index);
  }
  for (const match of body.matchAll(HANDED)) add(`hands its random to ${match[1].replace(/^this\./, "")}`, match.index);
  return found;
}

function callsOf(body) {
  const calls = new Set();
  for (const match of body.matchAll(/(?:^|[^.\w])(this\.|super\.)?([a-zA-Z_]\w*)\s*\(/g)) {
    if (!KEYWORDS.has(match[2])) calls.add(match[1] === "super." ? `super.${match[2]}` : match[2]);
  }
  return calls;
}

function indexTree(root) {
  const index = new Map();
  for (const directory of ENTITY_DIRECTORIES) {
    for (const file of walkJava(path.join(root, directory))) {
      const relative = path.relative(root, file).replaceAll(path.sep, "/");
      for (const owner of parseClasses(fs.readFileSync(file, "utf8"))) {
        if (!index.has(owner.name)) index.set(owner.name, { ...owner, relative });
      }
    }
  }
  return index;
}

/** Follows the unqualified calls of a class's tick, resolved up its superclasses, and lists the draws on the way. */
function reach(index, start) {
  const resolve = (from, name) => {
    for (let at = index.get(from); at; at = at.parent ? index.get(at.parent) : null) {
      if (at.methods.has(name)) return at;
    }
    return null;
  };
  const rows = new Map();
  const seen = new Set();
  const queue = [];
  for (const entry of TICK_ENTRIES) {
    const owner = resolve(start, entry);
    if (owner) queue.push([owner, entry, [`${owner.name}.${entry}`]]);
  }
  while (queue.length > 0) {
    const [owner, name, trail] = queue.shift();
    const key = `${owner.name}.${name}`;
    if (seen.has(key)) continue;
    seen.add(key);
    for (const body of owner.methods.get(name)) {
      for (const [what, cosmetic] of drawsOf(body)) rows.set(`${key}\t${what}`, { key, what, cosmetic, trail });
      for (const call of callsOf(body)) {
        // (super goes above the class that says it; anything else starts over from the object's own class)
        const above = call.startsWith("super.");
        const called = above ? call.slice("super.".length) : call;
        const target = above ? (owner.parent ? resolve(owner.parent, called) : null) : resolve(start, called) ?? resolve(owner.name, called);
        if (target) queue.push([target, called, [...trail, `${target.name}.${called}`]]);
      }
    }
  }
  return [...rows.values()];
}

function audit(root) {
  const rows = [];
  const index = indexTree(root);
  for (const owner of index.values()) {
    if (!AUDITED.test(owner.relative)) continue;
    for (const [name, bodies] of owner.methods) {
      const draws = new Map();
      for (const body of bodies) for (const [what, cosmetic] of drawsOf(body)) draws.set(what, cosmetic);
      for (const [what, cosmetic] of draws) rows.push(["consumer", `${owner.name}.${name}`, what, cosmetic ? "cosmetic" : "state", ""]);
    }
  }
  for (const file of walkJava(path.join(root, "world/level/block"))) {
    for (const owner of parseClasses(fs.readFileSync(file, "utf8"))) {
      const helpers = new Map();
      for (const [name, bodies] of owner.methods) helpers.set(name, bodies);
      for (const [name, bodies] of owner.methods) {
        if (!BLOCK_CALLBACKS.has(name) && !owner.touching?.has(name)) continue;
        const seen = new Set([name]);
        const queue = [[name, bodies]];
        while (queue.length > 0) {
          const [at, held] = queue.shift();
          for (const body of held) {
            for (const [what, cosmetic] of drawsOf(body)) rows.push(["consumer", `${owner.name}.${name}${at === name ? "" : ` via ${at}`}`, what, cosmetic ? "cosmetic" : "state", ""]);
            for (const call of callsOf(body)) {
              if (helpers.has(call) && !seen.has(call)) {
                seen.add(call);
                queue.push([call, helpers.get(call)]);
              }
            }
          }
        }
      }
    }
  }
  if (!index.has("LocalPlayer")) throw new Error(`${root}: no LocalPlayer`);
  for (const row of reach(index, "LocalPlayer")) rows.push(["reach", row.key, row.what, row.cosmetic ? "cosmetic" : "state", row.trail.join(" > ")]);
  return rows;
}

function generate(legacyRoot) {
  const manifest = JSON.parse(fs.readFileSync(path.join(repositoryRoot, "generated", "release-manifest.json"), "utf8")).releases;
  const versions = Array.isArray(manifest) ? manifest.map((entry) => entry.release) : Object.keys(manifest);
  const lines = [];
  for (const version of versions) {
    const legacy = LEGACY.includes(version);
    if (legacy && !legacyRoot) throw new Error(`${version} needs --legacy`);
    const cached = path.join(sourceCache(), version, "client", "net", "minecraft");
    const root = legacy ? path.join(legacyRoot, version, "net", "minecraft")
      : fs.existsSync(path.join(cached, "client", "player", "LocalPlayer.java")) ? cached : path.join(mcSourceRoot(), version, "client", "net", "minecraft");
    if (!fs.existsSync(root)) throw new Error(`No source of ${version}: ${root}`);
    const seen = new Set();
    for (const row of audit(root)) {
      const line = [version, ...row].join("\t");
      if (!seen.has(line)) lines.push(line);
      seen.add(line);
    }
  }
  return lines;
}

if (require.main === module) {
  const at = process.argv.indexOf("--legacy");
  const lines = generate(at < 0 ? null : process.argv[at + 1]);
  const output = `${lines.join("\n")}\n`;
  if (process.argv.includes("--check")) {
    if (fs.readFileSync(outputPath, "utf8").replaceAll("\r\n", "\n") !== output) throw new Error("random-draws.tsv differs from a fresh audit");
  } else {
    fs.writeFileSync(outputPath, output);
  }
}

module.exports = { parseClasses, drawsOf, callsOf, reach, audit };
