// The public field catalogue (B209, B271): docs/field-catalogue.md and docs/field-maps.md, joined from the declarations the
// addon's build wrote, the adapter's own sources, the binding's readers (scripts/field-facts.cjs), the contracts
// (scripts/field-contracts.cjs), the vanilla record (docs/field-evidence.json) and what the addon does on each release.
// A field exported without an entry fails test/field-catalogue.test.cjs. The packed layouts come from the compiled package
// and are held to scripts/state-bridge.cjs.
//   node scripts/field-catalogue.cjs           writes both pages
//   node scripts/field-catalogue.cjs --check   fails on a difference
const fs = require("node:fs");
const path = require("node:path");
const ts = require("typescript");
const { FLAGS, OUTPUTS, OUTPUT_FLAGS, PACKED } = require("./state-bridge.cjs");
const { STATES } = require("./state-roundtrip.cjs");
const { contracts, isAnchor, isMechanic } = require("./field-contracts.cjs");
const { adapterMaps, readers, rustTypes } = require("./field-facts.cjs");
const { RELEASE: KEPT_RELEASE, measureKept } = require("./field-kept.cjs");
const { compareReleases, registeredReleases } = require("./registered-releases.cjs");

const root = path.resolve(__dirname, "..");
const catalogue = path.join(root, "docs", "field-catalogue.md");
const maps = path.join(root, "docs", "field-maps.md");

/** The interfaces docs/field-maps.md maps across the Rust state, the binding and the adapter. */
const MAPPED = ["NativeBoatState", "NativeHorseState", "NativeMinecartState", "NativeMovingEntityState", "NativeProjectileState", "NativeArrowState",
  "NativeTridentState", "NativeBlockUpdate", "NativeBlockShape", "NativeMovingPiston", "NativeNearbyEntity"];

const oneLine = (text) => text.replace(/\s+/g, " ").replaceAll("|", "\\|").trim();
const comment = (node) => oneLine(ts.getJSDocCommentsAndTags(node).filter(ts.isJSDoc).map((doc) => ts.getTextOfJSDocComment(doc.comment) ?? "").join(" "));
const cell = (text) => `\`${oneLine(text).replaceAll("`", "'")}\``;

/** The exported interfaces, functions and class methods of a declaration text. */
function parse(text, name = "source.ts") {
  const file = ts.createSourceFile(name, text, ts.ScriptTarget.Latest, true);
  const interfaces = [];
  const entries = [];
  for (const statement of file.statements) {
    const exported = statement.modifiers?.some((modifier) => modifier.kind === ts.SyntaxKind.ExportKeyword);
    if (!exported) continue;
    if (ts.isInterfaceDeclaration(statement)) {
      interfaces.push({
        name: statement.name.text,
        note: comment(statement),
        members: statement.members.filter((member) => member.name).map((member) => ({
          name: member.name.getText(file),
          type: member.type ? member.type.getText(file) : member.getText(file),
          optional: !!member.questionToken,
          note: comment(member),
        })),
      });
    } else if (ts.isFunctionDeclaration(statement)) {
      entries.push({
        name: statement.name.text,
        parameters: statement.parameters.map((parameter) => parameter.type?.getText(file) ?? ""),
        result: statement.type?.getText(file) ?? "void",
        note: comment(statement),
      });
    } else if (ts.isClassDeclaration(statement)) {
      for (const member of statement.members) {
        if (!ts.isMethodDeclaration(member) && !ts.isConstructorDeclaration(member)) continue;
        entries.push({
          name: `${statement.name.text}.${member.name ? member.name.getText(file) : "constructor"}`,
          parameters: member.parameters.map((parameter) => parameter.type?.getText(file) ?? ""),
          result: member.type?.getText(file) ?? statement.name.text,
          note: comment(member),
        });
      }
    }
  }
  return { interfaces, entries };
}

/** Which way each interface crosses the binding: what a call takes and what it gives back, through what they hold. */
function directions(interfaces, entries) {
  const known = new Map(interfaces.map((item) => [item.name, item]));
  const named = (type) => [...type.matchAll(/\b[A-Z]\w+/g)].map(([name]) => name).filter((name) => known.has(name));
  const reach = (types) => {
    const seen = new Set();
    const walk = (name) => {
      if (seen.has(name)) return;
      seen.add(name);
      for (const member of known.get(name).members) named(member.type).forEach(walk);
    };
    types.flatMap(named).forEach(walk);
    return seen;
  };
  const taken = reach(entries.flatMap((entry) => entry.parameters));
  const given = reach(entries.map((entry) => entry.result));
  return (name) => (taken.has(name) && given.has(name) ? "in and out" : taken.has(name) ? "in" : given.has(name) ? "out" : "none");
}

/** The unit a row gives, or what the type alone settles. */
function unit(member, row) {
  if (row?.[1]) return row[1];
  if (/\bboolean\b/.test(member.type)) return "flag";
  if (member.type === "string") return "name";
  return "";
}

const WHOLE = { i8: "whole, a Java byte", u8: "whole, 0..255", u16: "whole, 0..65535", i32: "whole, a Java int", u32: "whole, 0..2^32-1", i64: "whole, within 2^53" };

/** The Rust type a field is held as past its reader: a double the reader narrows is a float in the engine. */
function rustCell(type, read) {
  const bare = (type ?? "").replace(/^Option<(.*)>$/, "$1").replaceAll("Native", "");
  if (!type) return "";
  return bare === "f64" && read?.narrowed ? `f64, narrowed to f32 by \`${read.reader}\`` : `\`${bare}\``;
}

/** What the binding does with a field left out, as its reader says. */
function absentCell(member, read, direction) {
  if (!member.optional) return "required";
  if (direction === "out") return "left out when it has no value";
  if (!read) return "read in the binding's own loop";
  if (read.helper) return `\`${read.helper}\` in \`${read.reader}\``;
  const expression = read.expression.replace(/\s*\bas f32\b/, "").replace(/\s+/g, " ");
  const call = /^\.(unwrap_or(?:_else|_default)?|map_or(?:_else)?)\b/.test(expression);
  if (expression.length > 70) return `read by \`${read.reader}\``;
  return call ? `\`${expression.slice(1).replaceAll("|", "\\|")}\` in \`${read.reader}\`` : `an Option to \`${read.reader}\`: absent stays apart from any value`;
}

/** What an explicit zero is against an absent number, as the reader says. */
function zeroCell(member, read) {
  if (!/\bnumber\b/.test(member.type) || /\[\]|Array/.test(member.type)) return "";
  if (!member.optional) return "a value";
  const fallback = /\.unwrap_or\(([^()]*)\)/.exec(read?.expression ?? "")?.[1];
  if (fallback === undefined) return "a value, apart from absent";
  return /^-?0(\.0)?$/.test(fallback) ? "the same as absent" : `a value, apart from absent's \`${fallback}\``;
}

/** Who owns a field between ticks and when it is sampled. */
function ownerCell(item, member, direction, contract, matrix) {
  if (item.name === "NativeSimulationState") {
    const row = matrix[member.name];
    return row ? `${row.owner}; changes at ${row.changes}` : "";
  }
  const classes = STATES[item.name];
  if (classes) {
    if (classes.evolved.includes(member.name)) return "engine, seeded: each tick recomputes it, hand back what it gave";
    if (classes.config.includes(member.name)) return "caller: read by each tick and not given back";
    if (classes.refused.includes(member.name)) return "caller: read by each tick, an illegal value refused";
    if (classes.structured.includes(member.name)) return "read by each tick; the kept column of docs/field-maps.md says if it comes back";
    const until = classes.evolvedUntil?.[member.name];
    return until ? `engine, seeded: each tick recomputes it through ${until}, later it comes back as given` : "engine, seeded: comes back as given, hand it on";
  }
  if (contract?.owner) return contract.owner;
  if (direction === "in") return "caller: read when the call starts";
  if (direction === "out") return "engine: produced by the call, read when it returns";
  if (direction === "in and out") return "caller seeds it, the engine returns it: hand back what it gave";
  return "adapter: held on the caller's object";
}

/**
 * The releases of a row: the family's range cut by the row's own bound, and what the member it cites says of the rest. A bound
 * is the row's `since`, `until` or capability, or else an identity boundary of its anchor that `MECHANIC_ANCHORS` names a
 * mechanic's. Past that, where the cited member is narrower by identity (docs/field-evidence.json `since` and `until`), the cell
 * says so: `‡ member A..B` is where vanilla has that member, checked by name in every source down to 1.8.8, while the
 * mechanic before or after it may stand in another member. `†` marks a lower bound the sources could not check: the
 * member's identity is lost below the release shown (unnamed in a legacy mapping, or an override past the unobfuscated releases).
 */
function releaseCell(row, family, { evidence, capabilities, families }) {
  const range = families[family] ?? families.all;
  let first = range.first;
  let last = range.last;
  const extra = row?.[3] ?? {};
  const record = row && isAnchor(row[2]) ? evidence.records[row[2]] : null;
  const raise = (candidate) => { if (compareReleases(candidate, first) > 0) first = candidate; };
  const lower = (candidate) => { if (compareReleases(candidate, last) < 0) last = candidate; };
  if (extra.capability && !capabilities[extra.capability]) throw new Error(`no row ${extra.capability} in docs/capability-table.md`);
  // the row's own bound, where it has one, is more exact than the first source release that holds the member
  const bounded = Boolean(extra.since || extra.capability);
  if (extra.capability) raise(capabilities[extra.capability]);
  if (extra.since) raise(extra.since);
  if (!bounded && record && isMechanic(row[2], record.since)) raise(record.since.release);
  if (extra.until) lower(extra.until);
  else if (record && isMechanic(row[2], record.until)) lower(record.until.release);
  let note = "";
  if (record) {
    const memberFirst = record.since.release;
    const memberLast = record.until.release;
    const narrower = (!bounded && compareReleases(memberFirst, first) > 0) || compareReleases(memberLast, last) < 0;
    if (narrower && record.since.by !== "unresolved") note = ` ‡ member ${memberFirst}..${memberLast}`;
    else if (!bounded && record.since.by === "unresolved" && compareReleases(memberFirst, first) > 0) note = " †";
  }
  return `${first}..${last}${note}`;
}

function sourceCell(row, { evidence }) {
  if (!row) return "";
  if (!isAnchor(row[2])) return row[2].replace(/^binding:\s*/, "binding: ").replaceAll("|", "\\|");
  const record = evidence.records[row[2]];
  const named = record && (record.first !== "1.14.4" || record.last !== "26.3") ? `, named so ${record.first} to ${record.last}` : "";
  return `\`${row[2].replaceAll("|", " or ")}\`${record ? `, ${record.at}` : ""}${named}`;
}

function adapterFiles() {
  const listed = (directory) => fs.readdirSync(path.join(root, directory), { withFileTypes: true })
    .filter((entry) => entry.isFile() && entry.name.endsWith(".ts") && entry.name !== "native.ts" && !entry.name.includes("__"))
    .map((entry) => `${directory}/${entry.name}`);
  return [...listed("src"), ...listed("src/mineflayer")];
}

function section(item, direction, context) {
  const contract = context.contracts[item.name];
  const lines = [`### ${item.name}`, ""];
  if (item.note) lines.push(item.note, "");
  if (direction) lines.push(`Direction: ${direction}.`, "");
  if (contract?.family && contract.family !== "adapter") lines.push(`Releases: ${familyText(contract.family, context)}.`, "");
  if (item.members.length === 0) return [...lines, "No members.", ""];
  const rust = context.rust[item.name] ?? {};
  const read = context.read[item.name] ?? {};
  const bound = direction !== undefined;
  const heading = bound
    ? "| member | type | Rust | absent | unit | legal range | owner and timing | releases | meaning | source |"
    : "| member | type | absent | unit | legal range | owner and timing | releases | meaning | source |";
  lines.push(heading, heading.replace(/[^|]+/g, "---"));
  for (const member of item.members) {
    const row = contract?.fields?.[member.name];
    const cells = [`\`${member.name}\``, cell(member.type)];
    if (bound) cells.push(rustCell(rust[member.name], read[member.name]));
    cells.push(
      bound ? absentCell(member, read[member.name], direction) : member.optional ? "allowed" : "required",
      unit(member, row),
      rangeCell(row, rust[member.name]),
      ownerCell(item, member, direction, contract, context.matrix),
      contract?.family === "adapter" ? "as the release's engine" : releaseCell(row, contract?.family, context),
      oneLine(row?.[0] ?? member.note ?? ""),
      sourceCell(row, context),
    );
    lines.push(`| ${cells.join(" | ")} |`);
  }
  return [...lines, ""];
}

function rangeCell(row, type) {
  const bare = (type ?? "").replace(/^Option<(.*)>$/, "$1");
  return oneLine(row?.[3]?.range ?? WHOLE[bare] ?? "");
}

const familyText = (family, { families }) => {
  const range = families[family];
  return range ? `${range.first} to ${range.last}, ${range.note}` : "every registered release";
};

/** docs/field-maps.md: each mapped interface across Rust state, binding and adapter. */
function renderMaps({ declarations, kept, adapter, rust, read, contracts: all }) {
  const lines = [
    "# Field maps", "",
    "Generated by `node scripts/field-catalogue.cjs`: do not edit. One public input across the Rust state, the binding and the adapter. These families are object APIs with no packed form; the player's is `docs/state-contract.md`, its packed side `docs/state-bridge.md`. `docs/field-catalogue.md` has the same fields with their contracts, ranges, releases and sources.", "",
    "- **binding**: the type at the boundary, which is always a JavaScript double or a flag, then the Rust type behind it. `f64, narrowed to f32` is a value vanilla holds as a float: the reader rounds it there, so a caller gets the float's value back, not the double's.",
    "- **absent**: what the binding reader does with a field left out, quoted from it. Absent is never read as a zero by accident: where the column says `unwrap_or(0.0)` zero is vanilla's initial value.",
    "- **zero**: what an explicit `0` is against absent. `test/field-boundaries.test.cjs` holds it.",
    "- **unit**: from the field's contract.",
    `- **kept**: measured on a real tick on ${KEPT_RELEASE} each time the page is made (\`scripts/field-kept.cjs\`), so a change of the addon changes it here. \`yes\` means the tick hands the field back, so the caller passes it to the next tick and the state persists through it. \`no\` means it is an input of that tick, or absent in that tick's scene, so the caller owns it. An update or a shape is no state and has no such column.`,
    "- **adapter sends**, **adapter takes**: the expression `src/mineflayer` fills the field from, and the state field it writes back. Empty means the adapter does not touch it, so a caller of the binding alone owns it. `carried` means the adapter keeps the tick's whole result and spreads it into the next input.", "",
  ];
  const short = (text) => oneLine(text).replaceAll("`", "'");
  const adapters = (entries) => (entries ?? []).map(short).map((entry) => `\`${entry}\``).join("; ");
  for (const name of MAPPED) {
    const item = declarations.interfaces.find((candidate) => candidate.name === name);
    const hasKept = Boolean(kept[name]);
    const heading = `| field | type | binding | unit | absent | zero |${hasKept ? " kept |" : ""} adapter sends | adapter takes |`;
    lines.push(`## ${name}`, "", heading, heading.replace(/[^|]+/g, "---"));
    for (const member of item.members) {
      const row = all[name]?.fields?.[member.name];
      const type = member.type.replaceAll(" | undefined", "").replaceAll(" | null", " or null").replaceAll(" | ", " or ").replaceAll("Array<", "").replaceAll(">", "[]").replace(/\s+/g, " ");
      const cells = [`\`${member.name}\``, oneLine(type), rustCell(rust[name][member.name], read[name][member.name]), unit(member, row), absentCell(member, read[name][member.name], "in"), zeroCell(member, read[name][member.name])];
      if (hasKept) cells.push(kept[name].includes(member.name) ? "no" : "yes");
      cells.push(adapters(adapter.sends[name]?.[member.name]), adapters(adapter.takes[name]?.[member.name]));
      lines.push(`| ${cells.join(" | ")} |`);
    }
    lines.push("");
  }
  return `${lines.join("\n").trimEnd()}\n`;
}

function packedTable(title, layout, fields) {
  const byName = Object.fromEntries(Object.entries(fields).map(([field, name]) => [name, field]));
  const rows = Object.entries(layout).sort(([, a], [, b]) => a - b);
  const lines = [`### ${title}`, "", "| name | value | state field |", "|---|---:|---|"];
  for (const [name, value] of rows) lines.push(`| \`${name}\` | ${value} | ${byName[name] ? `\`${byName[name]}\`` : ""} |`);
  return [...lines, ""];
}

function packedSection(layouts) {
  const { NATIVE_PLAYER_LAYOUT: size, NativePlayerInput, NativePlayerInputFlag, NativePlayerOutput, NativePlayerOutputFlag, NativePlayerEntityFlag } = layouts;
  return [
    "## Packed layouts", "",
    `Layout version ${size.version}: ${size.inputLength} input slots, ${size.outputLength} output slots, ${size.entityLength} numbers per entity. \`nativePlayerLayout()\` says what the addon was built with. A flag value is a bit of the \`FLAGS\` slot. A name with no state field here has no one-to-one field in \`scripts/state-bridge.cjs\`, which is the mapping. \`test/state-bridge.test.cjs\` holds that mapping to these names.`, "",
    ...packedTable("NativePlayerInput", NativePlayerInput, PACKED),
    ...packedTable("NativePlayerInputFlag", NativePlayerInputFlag, FLAGS),
    ...packedTable("NativePlayerOutput", NativePlayerOutput, OUTPUTS),
    ...packedTable("NativePlayerOutputFlag", NativePlayerOutputFlag, OUTPUT_FLAGS),
    ...packedTable("NativePlayerEntityFlag", NativePlayerEntityFlag, {}),
  ];
}

function render(context) {
  const { declarations, adapters, layouts } = context;
  const { interfaces, entries } = declarations;
  const direction = directions(interfaces, entries);
  const members = interfaces.reduce((sum, item) => sum + item.members.length, 0);
  const lines = [
    "# Field catalogue", "",
    "Generated by `node scripts/field-catalogue.cjs` from `native/artifacts/native.d.ts`, the binding's readers, the adapter's sources, the contracts in `scripts/field-contracts/`, the source record in `docs/field-evidence.json` and the packed layouts: do not edit. `test/field-catalogue.test.cjs` fails when an exported field has no entry here, when an entry is vague or cites nothing, and when a packed name changes without this page.", "",
    `The binding exports ${interfaces.length} interfaces with ${members} members, and ${entries.length} functions and methods. A stable reference is the heading of the type and the name of the member, as \`docs/field-catalogue.md#nativesimulationconfig\` and \`gravity\`.`, "",
    "## How to read a row", "",
    "- **type**: as declared. **Rust**: what the binding holds it as. Every number crosses as a JavaScript double. A row narrowed to `f32` is a value vanilla holds as a float: the reader rounds it, deliberately, as vanilla would.",
    "- **absent**: `required` means the type needs it. Otherwise the reader's own expression: what a field left out reads as. Left out is not `0` or `false` unless the expression says so (`docs/state-contract.md`, Omission). `left out when it has no value` is a result's field.",
    "- **unit**: from the contract. `flag` and `name` are what the type settles.",
    "- **legal range**: a whole-number field refuses a fraction, a value past its type, `NaN` and infinity by name, where N-API alone would truncate or wrap it. A public position, size, box or config value that is not finite or is out of range is refused before anything changes, with the field's name on the player and the state's on the other families (`docs/numeric-contract.md`). A range narrower than the type is given where the code or vanilla sets it.",
    "- **owner and timing**: `caller` keeps it and sends it each call. `engine, seeded` is the engine's between ticks: a caller seeds it and must hand back what it read, not a guess (`docs/state-contract.md`, Owners). `one tick` is an event the next tick clears. The player's rows are that page's matrix.",
    "- **releases**: where the row can mean something: the family's range (below) cut by the row's own bound. The bound is the row's own `since` or `until` with the line that shows it, a row of `docs/capability-table.md`, or where vanilla added or dropped the cited member when that is the mechanic's own boundary (`MECHANIC_ANCHORS` in `scripts/field-contracts.cjs`). Where the member is, by identity, is read in every named source from 1.8.8 (the six legacy trees remapped to 1.14.4's names, then every Mojang-named client) and followed through renames by Fabric's intermediary ids (`scripts/field-lineage.cjs`, `docs/field-evidence.json`), and where no mapping can follow it, read in the tree of the release beside the edge (`scripts/field-read-edges.cjs`): `‡ member A..B` says the member is only there in A to B, and before or after it the mechanic, if any, lives in another member. `named so` in the source column is only when the member had that name: a rename bounds nothing. `†` marks a lower bound no source could check: below the release shown the member's identity is lost (a legacy mapping left it unnamed, or an override's name changed where no id follows it). The binding takes a field on any release its family runs on; on a release outside the row's range vanilla has nothing there and it reads as inert.",
    "- **meaning**: what the value is and what reads or writes it. **source**: the vanilla member and a line of the newest release that holds it, or the rule of the binding that owns a value vanilla has no member for.",
    "- **direction**: `in` is read when the call starts and stays the caller's. `out` is produced by the call and is the engine's. `in and out` is both. `none` is an interface no entry point names.",
    "- **adapter types** have no Rust column: they are the caller's own objects. Their `releases` is the release the adapter's engine was made for.", "",
    "## Families", "",
    "Measured on the addon by `FAMILY_PROBES`: the first and last registered release an entry point of the family runs on.", "",
    "| family | releases | note |", "|---|---|---|",
    ...Object.entries(context.families).filter(([name]) => name !== "all").map(([name, range]) => `| ${name} | ${range.first} to ${range.last} | ${range.note} |`), "",
    "## Examples", "",
    "Omitted versus zero attributes, and carrying persistent outputs into later ticks: `docs/state-contract.md`, Examples, for the player, and `docs/field-maps.md` for what each state keeps. Both ideas on a ridden mob, run by `test/doc-examples.test.cjs`:", "",
    "```js doctest",
    "const world = new binding.NativeWorld();",
    "const horse = { position: { x: 0.5, y: 100, z: 0.5 }, velocity: { x: 0, y: 0, z: 0 }, yaw: 0, forward: 1, strafe: 0, jumpRequested: false, onGround: false, inWater: false };",
    "// omitted is the release's gravity, an explicit 0 is none",
    "assert.ok(binding.simulateVersionedHorseTick('1.21.5', world, horse).velocity.y < 0);",
    "assert.equal(binding.simulateVersionedHorseTick('26.3', world, { ...horse, gravity: 0 }).velocity.y, 0);",
    "```", "",
    "A tick's result is the next tick's input: the kept fields go on, and a state rebuilt by hand starts them over.", "",
    "```js doctest",
    "let carried = { ...horse, boosting: true, boostTime: 0, boostTimeTotal: 100, entityType: 'pig' };",
    "for (let tick = 0; tick < 3; tick += 1) carried = binding.simulateVersionedHorseTick('1.21.5', world, carried);",
    "assert.equal(carried.boostTime, 3);",
    "assert.equal(binding.simulateVersionedHorseTick('1.21.5', world, { ...carried, boostTime: undefined }).boostTime, 1);",
    "```", "",
    "## Interfaces of the binding", "",
    "| interface | direction | members |", "|---|---|---:|",
    ...interfaces.map((item) => `| [\`${item.name}\`](#${item.name.toLowerCase()}) | ${direction(item.name)} | ${item.members.length} |`), "",
    ...interfaces.flatMap((item) => section(item, direction(item.name), context)),
    ...packedSection(layouts),
    "## Entry points", "",
    "| entry point | takes | gives | note |", "|---|---|---|---|",
    ...entries.map((entry) => `| \`${entry.name}\` | ${entry.parameters.map(cell).join(", ")} | ${cell(entry.result)} | ${entry.note} |`), "",
    "## Adapter types", "",
    "The interfaces the TypeScript side declares beside the binding. The adapter reads and writes their fields on the caller's objects.", "",
    ...adapters.flatMap(({ file, interfaces: declared }) => declared.flatMap((item) => [...section(item, undefined, context), `Declared in \`${file}\`.`, ""])),
  ];
  return `${lines.join("\n").trimEnd()}\n`;
}

/** What one call of a family asks of the addon: it throws on a release that does not have the family. */
const position = { x: 0.5, y: 100, z: 0.5 };
const still = { x: 0, y: 0, z: 0 };
const FAMILY_PROBES = {
  player: { note: "the player's tick", probe: (binding, world, version) => binding.simulateVersionedTick(version, world, { position, velocity: still, halfWidth: 0.3, height: 1.8, onGround: false, collidedHorizontally: false, collidedVertically: false }, {}) },
  boat: { note: "the boat's tick", probe: (binding, world, version) => binding.simulateVersionedBoatTick(version, world, { position, velocity: still, yaw: 0, forward: false, left: false, right: false, inWater: false }) },
  mount: { note: "a ridden mob's tick, each kind from `entityTravelSupport`", probe: (binding, world, version) => binding.simulateVersionedHorseTick(version, world, { position, velocity: still, yaw: 0, forward: 0, strafe: 0, jumpRequested: false, onGround: false, inWater: false }) },
  minecart: { note: "a cart's tick", probe: (binding, world, version) => binding.simulateVersionedMinecartTick(version, world, { variant: "minecart", position, velocity: still, yRot: 0, flipped: false, onGround: false, inWater: false }, { improvements: false }) },
  "moving-entity": { note: "an item's tick; each kind checks its own release", probe: (binding, world, version) => binding.simulateVersionedMovingEntityTick(version, world, { kind: "item", position, velocity: still }) },
  projectile: { note: "a snowball's tick; each kind checks its own release (`docs/projectile-capabilities.md`)", probe: (binding, world, version) => binding.simulateVersionedProjectileTicks(version, world, [{ kind: "snowball", position, velocity: still }], 1) },
  arrow: { note: "an arrow's tick", probe: (binding, world, version) => binding.simulateVersionedArrowTicks(version, world, [{ position, velocity: still }], 1) },
  trident: { note: "a trident's tick", probe: (binding, world, version) => binding.simulateVersionedTridentTicks(version, world, [{ position, velocity: still }], 1) },
  dragon: { note: "the dragon's tick", probe: (binding, world, version) => binding.simulateVersionedDragonTick(version, { position, velocity: still, yawDegrees: 0 }) },
  shulker: { note: "a shulker box's tick", probe: (binding, world, version) => binding.simulateVersionedShulkerBoxTick(version, { facing: 1, status: 0, progress: 0, progressOld: 0 }) },
  "mob-travel": { note: "a travelling mob's tick, each kind from `entityTravelSupport`", probe: (binding, world, version) => binding.simulateVersionedMobTravel(version, world, { kind: "zombie", position, velocity: still, yawDegrees: 0, input: still, speed: 0.1, halfWidth: 0.3, height: 1.95 }) },
  piston: { note: "a piston's push", probe: (binding, world, version) => binding.simulateVersionedEntityPistonPush(version, world, { position, velocity: still, halfWidth: 0.3, height: 1.8 }, []) },
  world: { note: "a block update", probe: (binding, world, version) => world.applyBlockUpdates([{ x: 0, y: 0, z: 0, id: 1, stateVersion: version }]) },
};

function measuredFamilies(binding) {
  const releases = registeredReleases().map(({ version }) => version).sort(compareReleases);
  const families = { all: { first: releases[0], last: releases.at(-1), note: "every registered release" } };
  for (const [name, { note, probe }] of Object.entries(FAMILY_PROBES)) {
    const ran = releases.filter((version) => {
      try {
        probe(binding, new binding.NativeWorld(), version);
        return true;
      } catch {
        return false;
      }
    });
    if (ran.length === 0) throw new Error(`the ${name} family's probe runs on no release`);
    const holes = releases.slice(releases.indexOf(ran[0]), releases.indexOf(ran.at(-1)) + 1).filter((version) => !ran.includes(version));
    families[name] = { first: ran[0], last: ran.at(-1), note: holes.length ? `${note}; not ${holes.join(", ")}` : note };
  }
  return families;
}

/** The player's owner matrix of docs/state-contract.md. */
function stateMatrix() {
  const text = fs.readFileSync(path.join(root, "docs", "state-contract.md"), "utf8").replace(/\r\n/g, "\n");
  const matrix = {};
  for (const [, name, owner, changes] of text.matchAll(/^\| `(\w+)` \| [^|]* \| ([^|]*) \| ([^|]*) \| (?:yes|no) \| (?:yes|no) \|/gm)) matrix[name] = { owner: owner.trim(), changes: changes.trim() };
  return matrix;
}

/** `Kind mechanic` -> the release of docs/capability-table.md. */
function capabilityTable() {
  const text = fs.readFileSync(path.join(root, "docs", "capability-table.md"), "utf8").replace(/\r\n/g, "\n");
  return Object.fromEntries([...text.matchAll(/^\| (\w+) \| ([^|]+?) \| ([\d.]+) \|/gm)].map(([, kind, mechanic, since]) => [`${kind} ${mechanic}`, since]));
}

/** What the adapter sends each field from, and what it takes each result into. */
function adapterSides(files, interfaces, entries) {
  return { sends: adapterMaps(files, new Set(interfaces.map(({ name }) => name))), takes: adapterTakes(files, entries) };
}

/** The fields of a tick's result the adapter writes back: `output.field` on the right of an assignment. */
function adapterTakes(files, entries) {
  const results = new Map(entries.map((entry) => [entry.name, /Native\w+/.exec(entry.result)?.[0]]));
  const taken = {};
  for (const file of files) {
    const source = ts.createSourceFile(file, fs.readFileSync(path.join(root, file), "utf8"), ts.ScriptTarget.Latest, true);
    const visit = (node, scope) => {
      let inner = scope;
      if (ts.isVariableDeclaration(node) && node.initializer) {
        const called = /\.(\w+)\(/.exec(node.initializer.getText(source))?.[1];
        const kind = results.get(called);
        if (kind && ts.isIdentifier(node.name)) inner = scope.set(node.name.text, kind);
      }
      if (ts.isBinaryExpression(node) && node.operatorToken.kind === ts.SyntaxKind.EqualsToken) {
        const visitRight = (part) => {
          if (ts.isPropertyAccessExpression(part) && ts.isIdentifier(part.expression) && scope.has(part.expression.text)) {
            const kind = scope.get(part.expression.text);
            ((taken[kind] ??= {})[part.name.text] ??= []);
            const target = node.left.getText(source);
            if (!taken[kind][part.name.text].includes(target)) taken[kind][part.name.text].push(target);
          }
          ts.forEachChild(part, visitRight);
        };
        visitRight(node.right);
      }
      ts.forEachChild(node, (child) => visit(child, inner));
    };
    visit(source, new Map());
  }
  return taken;
}

/** What the catalogue is made from, as this checkout has it. */
function current() {
  const declarations = parse(fs.readFileSync(path.join(root, "native", "artifacts", "native.d.ts"), "utf8"));
  const files = adapterFiles();
  const adapters = files.map((file) => ({ file, interfaces: parse(fs.readFileSync(path.join(root, file), "utf8"), file).interfaces }))
    .filter(({ interfaces }) => interfaces.length > 0);
  const layouts = require(path.join(root, "dist", "native.js"));
  const rust = rustTypes();
  const evidencePath = path.join(root, "docs", "field-evidence.json");
  const binding = layouts.LoadNativePhysics();
  const members = Object.fromEntries(declarations.interfaces.map((item) => [item.name, item.members.map((member) => member.name)]));
  return {
    declarations, adapters, layouts, rust, read: readers(rust), contracts: contracts(), matrix: stateMatrix(), capabilities: capabilityTable(),
    evidence: fs.existsSync(evidencePath) ? JSON.parse(fs.readFileSync(evidencePath, "utf8")) : { records: {} },
    families: measuredFamilies(binding), adapter: adapterSides(files, declarations.interfaces, declarations.entries),
    kept: measureKept(binding, members),
  };
}

if (require.main === module) {
  const made = current();
  const pages = [[catalogue, render(made)], [maps, renderMaps(made)]];
  if (process.argv.includes("--check")) {
    for (const [file, text] of pages) {
      if (fs.readFileSync(file, "utf8").replace(/\r\n/g, "\n") !== text) {
        console.error(`${path.basename(file)} differs: node scripts/field-catalogue.cjs`);
        process.exitCode = 1;
      }
    }
  } else {
    for (const [file, text] of pages) fs.writeFileSync(file, text);
  }
}

module.exports = { MAPPED, current, parse, render, renderMaps };
