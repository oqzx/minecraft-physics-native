// The identity of a vanilla member across releases (B271), for scripts/field-evidence.cjs: a name says only what a member
// was called in one release, so a range of names never bounds a mechanic on its own. Fabric's intermediary names stay with
// a member through Mojang's renames, so a member is followed release to release by them:
//   1.14.4 to 1.21.11  Mojang's client mappings (MC_CLIENT_JAR_ROOT/<release>/client_mappings.txt) take a Mojang name to the
//                      obfuscated one, and Fabric's intermediary of that release (MC_INTERMEDIARY_ROOT/<release>.tiny, the
//                      `mappings/mappings.tiny` of net.fabricmc:intermediary:<release>:v2) takes that to the stable id
//   1.8.8 to 1.13.2    the recorder's legacy mappings (legacy-mappings/<release>.tiny) take Legacy Fabric's stable id to
//                      the name the same member has in 1.14.4; the legacy trees are remapped with them, so across 1.13.2 and
//                      1.14.4 the name is the link
//   26.1 on            unobfuscated: no ids, so a member is followed by its name, and a boundary there is known only when
//                      the class itself is new or gone
// An override has no id of its own (intermediary lists a method only where it is first declared), so it is followed by name
// the same way. `since(anchor, ...)` walks back from the first release whose source declares the anchor; `until` walks
// forward from the last. Each returns { release, by, note, classBeside }: `by` is `oldest`/`newest` when the member reaches
// the end of the sources, `added`/`removed` when the release beside it has no such member by identity, or `unresolved` when
// identity cannot be followed there; `classBeside` says whether the anchor's class is there in that release beside it.
// A member added by identity is not always a mechanic added: a refactor moves a mechanic to a new member (Block.getFriction
// at 1.13 holds what a field held before), so a boundary bounds a row only by scripts/field-contracts.cjs MECHANIC_ANCHORS.
const fs = require("node:fs");
const path = require("node:path");
const { clientJarRoot, sourceCache } = require("./source-pins.cjs");

const root = path.resolve(__dirname, "..");
const intermediaryRoot = () => process.env.MC_INTERMEDIARY_ROOT ?? path.join(sourceCache(), "..", "intermediary");
const legacyMappings = (release) => path.join(root, "legacy-mappings", `${release}.tiny`);
const LEGACY = new Set(["1.8.8", "1.9.4", "1.10.2", "1.11.2", "1.12.2", "1.13.2"]);
const isLegacy = (release) => LEGACY.has(release);
const unobfuscated = (release) => /^2\d\./.test(release);

const PRIMITIVE = { boolean: "Z", byte: "B", char: "C", short: "S", int: "I", long: "J", float: "F", double: "D", void: "V" };

/** Mojang's mappings of a release: named class -> { obf, fields: name -> obf, methods: name -> [{ obf, args, ret }] }. */
function mojang(release) {
  const text = fs.readFileSync(path.join(clientJarRoot(), release, "client_mappings.txt"), "utf8");
  const classes = new Map();
  let current = null;
  for (const line of text.split(/\r?\n/)) {
    if (!line || line.startsWith("#")) continue;
    if (!line.startsWith(" ")) {
      const [, named, obf] = /^(\S+) -> (\S+):$/.exec(line) ?? [];
      current = named ? { obf, fields: new Map(), methods: new Map() } : null;
      if (named) classes.set(named, current);
      continue;
    }
    if (!current) continue;
    const method = /^\s+(?:\d+:\d+:)?(\S+) ([\w$<>]+)\(([^)]*)\)(?::\d+:\d+)? -> (\S+)$/.exec(line);
    if (method) {
      const [, ret, name, args, obf] = method;
      if (!current.methods.has(name)) current.methods.set(name, []);
      current.methods.get(name).push({ obf, args: args ? args.split(",") : [], ret });
      continue;
    }
    const field = /^\s+(\S+) ([\w$]+) -> (\S+)$/.exec(line);
    if (field) current.fields.set(field[2], { obf: field[3], type: field[1] });
  }
  return classes;
}

/** A Mojang type name as an obfuscated descriptor. */
function descriptor(type, classes) {
  const dims = (type.match(/\[\]/g) ?? []).length;
  const bare = type.replace(/\[\]/g, "");
  const base = PRIMITIVE[bare] ?? `L${(classes.get(bare)?.obf ?? bare).replaceAll(".", "/")};`;
  return "[".repeat(dims) + base;
}

/** A descriptor with each class reduced to `L`: what two namespaces' descriptors of one member still share. */
const shape = (desc) => desc.replace(/L[^;]+;/g, "L");

/** The shape of a Mojang-named member's descriptor, from its declared types. */
function namedShape(entry, member) {
  const type = (name) => {
    const dims = (name.match(/\[\]/g) ?? []).length;
    return "[".repeat(dims) + (PRIMITIVE[name.replace(/\[\]/g, "")] ?? "L");
  };
  const field = entry.fields.get(member);
  if (field) return { kind: "fields", shapes: [type(field.type)] };
  const methods = entry.methods.get(member) ?? [];
  return { kind: "methods", shapes: methods.map((method) => `(${method.args.map(type).join("")})${type(method.ret)}`) };
}

/** A tiny v2 file of two namespaces: from-class -> { to, fields: from -> [{ desc, to }], methods: from -> [{ desc, to }] }, and every `to` id. */
function tiny(file) {
  const classes = new Map();
  const ids = new Set();
  let current = null;
  for (const line of fs.readFileSync(file, "utf8").split(/\r?\n/)) {
    const parts = line.split("\t");
    if (parts[0] === "c") {
      current = { to: parts[2], fields: new Map(), methods: new Map() };
      classes.set(parts[1], current);
      ids.add(parts[2]);
    } else if (current && parts[0] === "" && (parts[1] === "f" || parts[1] === "m")) {
      const [, kind, desc, from, to] = parts;
      const table = kind === "f" ? current.fields : current.methods;
      if (!table.has(from)) table.set(from, []);
      table.get(from).push({ desc, to: to ?? "" });
      if (to) ids.add(to);
    }
  }
  return { classes, ids };
}

/** Everything one release says of identity, or null when its mappings are not here. */
function releaseMaps(release, cache) {
  if (cache.has(release)) return cache.get(release);
  let made = null;
  if (isLegacy(release)) {
    const file = legacyMappings(release);
    if (fs.existsSync(file)) {
      // legacy file: intermediary -> named; invert it so a name finds its id
      const { classes } = tiny(file);
      const byName = new Map();
      const ids = new Set();
      for (const [id, entry] of classes) {
        ids.add(id);
        const members = { id, fields: new Map(), methods: new Map(), unnamed: { fields: [], methods: [] } };
        for (const [kind, table] of [["fields", entry.fields], ["methods", entry.methods]]) {
          for (const [memberId, list] of table) {
            for (const { desc, to } of list) {
              ids.add(memberId);
              if (!to) members.unnamed[kind].push(shape(desc));
              if (!to) continue;
              if (!members[kind].has(to)) members[kind].set(to, []);
              members[kind].get(to).push(memberId);
            }
          }
        }
        if (entry.to) byName.set(entry.to.replaceAll("/", "."), members);
      }
      made = { kind: "legacy", byName, ids };
    }
  } else if (!unobfuscated(release)) {
    const mappings = path.join(clientJarRoot(), release, "client_mappings.txt");
    const intermediary = path.join(intermediaryRoot(), `${release}.tiny`);
    if (fs.existsSync(mappings) && fs.existsSync(intermediary)) made = { kind: "modern", named: mojang(release), inter: tiny(intermediary) };
  }
  cache.set(release, made);
  return made;
}

/** The named classes of a release whose simple name (or nested name) is `simple`. */
function classesNamed(names, simple) {
  return [...names].filter((name) => name === simple || name.endsWith(`.${simple}`) || name.endsWith(`$${simple}`));
}

/**
 * The stable ids of an anchor's members in one release: `{ ids: Set, names: [full Mojang names] }`, empty when its mappings
 * hold no such member. A class alone gives the class's id.
 */
function idsOf(groups, release, cache) {
  const maps = releaseMaps(release, cache);
  const ids = new Set();
  const names = [];
  if (!maps) return null;
  for (const { classes, members } of groups) {
    for (const simple of classes) {
      if (maps.kind === "legacy") {
        for (const full of classesNamed(maps.byName.keys(), simple)) {
          const entry = maps.byName.get(full);
          if (members.length === 0) { ids.add(entry.id); names.push(full); }
          for (const member of members) {
            for (const id of [...(entry.fields.get(member) ?? []), ...(entry.methods.get(member) ?? [])]) { ids.add(id); names.push(`${full}.${member}`); }
          }
        }
        continue;
      }
      for (const full of classesNamed(maps.named.keys(), simple)) {
        const entry = maps.named.get(full);
        const inter = maps.inter.classes.get(entry.obf);
        if (!inter) continue;
        if (members.length === 0) { ids.add(inter.to); names.push(full); }
        for (const member of members) {
          const field = entry.fields.get(member);
          if (field) {
            const desc = descriptor(field.type, maps.named);
            for (const candidate of inter.fields.get(field.obf) ?? []) if (candidate.desc === desc) { ids.add(candidate.to); names.push(`${full}.${member}`); }
          }
          for (const method of entry.methods.get(member) ?? []) {
            const desc = `(${method.args.map((arg) => descriptor(arg, maps.named)).join("")})${descriptor(method.ret, maps.named)}`;
            for (const candidate of inter.methods.get(method.obf) ?? []) if (candidate.desc === desc) { ids.add(candidate.to); names.push(`${full}.${member}`); }
          }
        }
      }
    }
  }
  return { ids, names };
}

/** The Mojang names (class.member) a set of ids has in a modern release. */
function namesOfIds(ids, release, cache) {
  const maps = releaseMaps(release, cache);
  const found = [];
  if (!maps || maps.kind !== "modern") return found;
  const obfToNamed = new Map([...maps.named].map(([name, entry]) => [entry.obf, [name, entry]]));
  for (const [obf, inter] of maps.inter.classes) {
    const owner = obfToNamed.get(obf);
    if (!owner) continue;
    if (ids.has(inter.to)) found.push({ owner: owner[0], member: null });
    for (const [kind, table] of [["fields", inter.fields], ["methods", inter.methods]]) {
      for (const [memberObf, list] of table) {
        for (const { desc, to } of list) {
          if (!ids.has(to)) continue;
          // an obfuscated name is shared by overloads and by fields of other types: the descriptor tells them apart
          const named = kind === "fields"
            ? [...owner[1].fields].find(([, field]) => field.obf === memberObf && descriptor(field.type, maps.named) === desc)?.[0]
            : [...owner[1].methods].find(([, methods]) => methods.some((method) => method.obf === memberObf
              && `(${method.args.map((arg) => descriptor(arg, maps.named)).join("")})${descriptor(method.ret, maps.named)}` === desc))?.[0];
          if (named) found.push({ owner: owner[0], member: named });
        }
      }
    }
  }
  return found;
}

/**
 * One step of the walk from release `from` (where the member is held as `held`) to the release `to` beside it. Returns
 * { held } to go on, or { by, note } to stop.
 */
function step(held, from, to, context) {
  const { cache, declares, classExists } = context;
  const bothModern = !isLegacy(from) && !isLegacy(to) && !unobfuscated(from) && !unobfuscated(to);
  const bothLegacy = isLegacy(from) && isLegacy(to);
  if ((bothModern || bothLegacy) && held.ids) {
    const there = releaseMaps(to, cache);
    if (!there) return { by: "unresolved", note: `no mappings of ${to}` };
    const kept = new Set([...held.ids].filter((id) => (there.kind === "legacy" ? there.ids.has(id) : there.inter.ids.has(id))));
    return kept.size > 0 ? { held: { ...held, ids: kept } } : { by: "gone", note: `no member of ${to} has its id` };
  }
  // across 1.13.2 and 1.14.4, or between legacy-named and Mojang-named, the name is the link: the legacy trees carry 1.14.4's names
  if (held.ids && ((isLegacy(from) && to === "1.14.4") || (from === "1.14.4" && isLegacy(to)))) {
    const modern = isLegacy(from) ? to : from;
    const names = isLegacy(from) ? null : namesOfIds(held.ids, from, cache);
    const groups = names
      ? names.map(({ owner, member }) => ({ classes: [owner.split(".").at(-1)], members: member ? [member] : [] }))
      : held.groups;
    const next = idsOf(groups, to, cache);
    if (next && next.ids.size > 0) return { held: { groups, ids: next.ids } };
    if (!declares(held.groups, to) && groups.every(({ classes }) => !classExists(classes, to))) return { by: "gone", note: `${to} has no class ${groups.map(({ classes }) => classes.join("|")).join(", ")}` };
    // going back into a legacy release: a member of the class its mapping left unnamed may be this one, unless none has its shape
    if (names && names.length > 0) {
      const here = releaseMaps(from, cache);
      const there = releaseMaps(to, cache);
      const possible = names.some(({ owner, member }) => {
        const legacy = there.byName.get(owner);
        if (!legacy || !member) return false;
        const { kind, shapes } = namedShape(here.named.get(owner), member);
        return legacy.unnamed[kind].some((candidate) => shapes.includes(candidate));
      });
      if (!possible) return { by: "gone", note: `no member of ${to}'s class is named so, and none it left unnamed has its descriptor's shape` };
    }
    return { by: "unresolved", note: `${to} has no member named as ${modern} names it; a member its mappings left unnamed may be it` };
  }
  // 26.1 on, and a member no id names (an override, which intermediary lists only where it is first declared): by name, and
  // a boundary is known only when the class is new or gone
  const named = !unobfuscated(from) && held.ids ? namesOfIds(held.ids, from, cache).filter(({ member }) => member || held.groups.every((group) => group.members.length === 0)) : null;
  const groups = named && named.length > 0 ? named.map(({ owner, member }) => ({ classes: [owner.split(".").at(-1).split("$").at(-1)], members: member ? [member] : [] })) : held.groups;
  if (declares(groups, to)) {
    const ids = unobfuscated(to) ? null : idsOf(groups, to, cache)?.ids ?? null;
    return { held: { groups, ids: ids && ids.size > 0 ? ids : null } };
  }
  if (groups.every(({ classes }) => !classExists(classes, to))) return { by: "gone", note: `${to} has no class ${groups.map(({ classes }) => classes.join("|")).join(", ")}` };
  // a registered entry (an attribute, a tag, a game rule) carries its key: a key no source of the release beside it holds is new
  const key = context.registryKey?.(groups, from);
  if (key && !context.holdsLiteral(key, to)) return { by: "gone", note: `no source of ${to} holds its key "${key}"` };
  return { by: "unresolved", note: held.ids || unobfuscated(from) || unobfuscated(to) ? `${to} is unobfuscated or beside it: no ids to follow a renamed member`
    : `${to} has the class but no member of that name, and no id names it in ${from}: an override, which intermediary lists only where it is first declared` };
}

/** Whether the anchor's class is there in the release beside a boundary: by the class's id where both have one, else by name. */
function classBeside(groups, at, next, context) {
  const classes = groups.map(({ classes }) => ({ classes, members: [] }));
  const here = releaseMaps(at, context.cache);
  const there = releaseMaps(next, context.cache);
  if (here && there && here.kind === there.kind) {
    const ids = idsOf(classes, at, context.cache)?.ids ?? new Set();
    const all = there.kind === "legacy" ? there.ids : there.inter.ids;
    if (ids.size > 0) return [...ids].some((id) => all.has(id));
  }
  return groups.some(({ classes: names }) => context.classExists(names, next));
}

/** Walk an anchor from `start` through `order` (releases beyond it, nearest first). */
function walk(groups, start, order, context, ends) {
  let held = { groups, ids: idsOf(groups, start, context.cache)?.ids ?? null };
  if (held.ids && held.ids.size === 0) held.ids = null;
  let at = start;
  for (const next of order) {
    const result = step(held, at, next, context);
    if (!result.held) {
      if (result.by !== "gone") return { release: at, by: result.by, note: result.note };
      return { release: at, by: ends.gone, note: result.note, classBeside: classBeside(held.groups, at, next, context) };
    }
    held = result.held;
    at = next;
  }
  return { release: at, by: ends.end, note: "" };
}

function since(groups, first, releases, context) {
  const order = releases.slice(0, releases.indexOf(first)).reverse();
  return walk(groups, first, order, context, { gone: "added", end: "oldest" });
}

function until(groups, last, releases, context) {
  const order = releases.slice(releases.indexOf(last) + 1);
  return walk(groups, last, order, context, { gone: "removed", end: "newest" });
}

module.exports = { descriptor, idsOf, intermediaryRoot, isLegacy, mojang, since, tiny, until, unobfuscated };
