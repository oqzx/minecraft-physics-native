// Block friction out of vanilla's block registry source: BlockBehaviour.Properties.friction (Block.slipperiness
// and its builders under the other mappings), the default it starts from, and what a copied Properties brings along.
// Whatever cannot be followed throws: an expression that is no float literal, a friction outside a registration, a
// copy of a block that cannot be named, or a nondefault block handed to something this does not understand.

const crypto = require("node:crypto");

/** The bits of a Java float literal as a binary32, "0x3f7ae148". */
function floatBits(value) {
  const buffer = Buffer.alloc(4);
  buffer.writeFloatBE(value);
  return `0x${buffer.toString("hex")}`;
}

const FLOAT_LITERAL = /^(\d+(?:\.\d*)?|\.\d+)[fF]$/;

function parseFloatLiteral(expression, where) {
  const literal = FLOAT_LITERAL.exec(expression.trim());
  if (!literal) throw new Error(`${where}: friction ${JSON.stringify(expression)} is not a float literal`);
  return Math.fround(Number(literal[1]));
}

/** The default friction in the class that declares the field. */
function parseDefaultFriction(source, where) {
  const field = /(?:\bfloat\s+|\bthis\.)(?:friction|slipperiness)\s*=\s*(\d[^;]*);/.exec(source);
  if (!field) throw new Error(`${where}: no default friction field`);
  return { expression: field[1].trim(), value: parseFloatLiteral(field[1], where) };
}

// a registration by name, by BlockItemIds/BlockIds symbol, or by legacy id and name
const REGISTRATION = /\bregister[A-Za-z]*\s*\(\s*(?:\([A-Za-z<>]+\)\s*)?(?:(?:\d+\s*,\s*)?"(?:minecraft:)?([a-z0-9_./-]+)"|(?:BlockItemIds|BlockIds)\.([A-Z0-9_]+))/g;
const FRICTION_CALL = /\.(?:friction|slipperiness|setSlipperiness)\(([^()]*)\)/g;
const COPY_CALL = /\b(?:ofFullCopy|ofLegacyCopy|copy)\(\s*(?:Blocks\.)?([A-Za-z_][A-Za-z0-9_]*)\s*\)/g;
const LOCAL_DECLARATION = /\b(?:Block|[A-Z][A-Za-z0-9_]*Block)\s+(lv\d*|arg\d*|var\d+|block\d*)\s*=/g;

/** The statements of a registry source: where each starts and ends, by the `;` at nesting depth 0 of its parentheses. */
function statementEnd(source, from) {
  let depth = 0;
  for (let at = from; at < source.length; at += 1) {
    const character = source[at];
    if (character === "(" || character === "{") depth += 1;
    else if (character === ")" || character === "}") depth -= 1;
    else if (character === ";" && depth <= 0) return at;
    else if (character === '"') {
      at = source.indexOf('"', at + 1);
      if (at < 0) break;
    }
  }
  return source.length;
}

/** The registrations of a Blocks source (Block.java's bootstrap in 1.13): name, constant, and the statement's text. */
function registrationsOf(source) {
  const registrations = [];
  for (const match of source.matchAll(REGISTRATION)) {
    const name = match[1] ?? match[2].toLowerCase();
    const end = statementEnd(source, match.index);
    const before = source.slice(Math.max(0, match.index - 120), match.index);
    const constant = /\b([A-Za-z_][A-Za-z0-9_]*)\s*=\s*$/.exec(before)?.[1];
    registrations.push({ name, constant, start: match.index, end, text: source.slice(match.index, end) });
  }
  return registrations;
}

const owner = (registrations, at) => {
  let found;
  for (const registration of registrations) {
    if (registration.start > at) break;
    if (at < registration.end) found = registration;
  }
  return found;
};

/**
 * The friction of a block built in a local variable (1.13's bootstrap): the local's statement, whose name is the
 * registration that takes the local.
 */
function ownerOfLocal(source, registrations, at) {
  let declaration;
  for (const match of source.matchAll(LOCAL_DECLARATION)) {
    if (match.index > at) break;
    declaration = match;
  }
  if (!declaration || at >= statementEnd(source, declaration.index)) return undefined;
  const local = declaration[1];
  return registrations.find(({ start, text }) => start > at && new RegExp(`,\\s*${local}\\s*\\)?\\s*$`).test(text));
}

/**
 * The nondefault frictions of one Blocks source: { name: { expression, value, bits, via? } }. `where` names the source
 * in what it throws.
 */
function parseRegistryFriction(source, where) {
  const registrations = registrationsOf(source);
  if (registrations.length === 0) throw new Error(`${where}: no block registrations`);
  const frictions = new Map();
  for (const call of source.matchAll(FRICTION_CALL)) {
    // (the declaration of the builder's own method)
    if (/^\s*float\s/.test(call[1])) continue;
    const registration = owner(registrations, call.index) ?? ownerOfLocal(source, registrations, call.index);
    if (!registration) throw new Error(`${where}: friction ${call[1]} is outside a registration`);
    const value = parseFloatLiteral(call[1], `${where} ${registration.name}`);
    if (frictions.has(registration.name)) throw new Error(`${where}: ${registration.name} sets friction twice`);
    frictions.set(registration.name, { expression: call[1].trim(), value });
  }
  const nameOf = new Map();
  for (const { constant, name } of registrations) if (constant) nameOf.set(constant, name);
  for (const { name } of registrations) nameOf.set(name.toUpperCase(), name);
  // local variables hand their blocks to a registration, which is their name here
  for (const { name, text } of registrations) {
    const local = /,\s*([a-z]+\d*)\s*\)?\s*$/.exec(text)?.[1];
    if (local) nameOf.set(local, name);
  }

  // what copies a Properties brings its friction along, unless it sets its own
  const copies = [];
  for (const registration of registrations) {
    for (const copy of registration.text.matchAll(COPY_CALL)) {
      const source_ = nameOf.get(copy[1]);
      if (source_ == null) {
        // a parameter is a helper's: a block named in the call is checked by the mention below
        if (/^(arg|var|base|block|from|parent|lv)\d*$/.test(copy[1])) continue;
        throw new Error(`${where}: ${registration.name} copies ${copy[1]}, which is no registered block`);
      }
      copies.push({ name: registration.name, from: source_ });
    }
  }
  for (let grown = true; grown;) {
    grown = false;
    for (const { name, from } of copies) {
      const inherited = frictions.get(from);
      if (inherited && !frictions.has(name)) {
        frictions.set(name, { expression: inherited.expression, value: inherited.value, via: from });
        grown = true;
      }
    }
  }

  // a nondefault block any other registration builds on, in a way that is not a copy: a helper could bring its friction
  for (const [name] of frictions) {
    const registration = registrations.find((candidate) => candidate.name === name);
    const constant = registration?.constant ?? name.toUpperCase();
    // (a field of another class by that name is none: SoundType.SLIME_BLOCK, BlockItemIds.ICE)
    const mention = new RegExp(`(?<![A-Za-z0-9_.])(?:Blocks\\.)?${constant}\\b`);
    for (const other of registrations) {
      if (other.name === name || frictions.has(other.name) || !mention.test(other.text)) continue;
      throw new Error(`${where}: ${other.name} builds on ${name}, whose friction cannot be followed`);
    }
  }
  return Object.fromEntries([...frictions].sort(([a], [b]) => a.localeCompare(b)).map(([name, entry]) => [name, { ...entry, bits: floatBits(entry.value) }]));
}

const LEGACY_REGISTRATION = /\bregister\s*\(\s*\d+\s*,\s*"([a-z0-9_]+)"\s*,\s*[^\n]{0,40}?\bnew\s+([A-Za-z0-9_]+)\s*\(/g;

/**
 * Before 1.13 a block class's constructor sets `this.friction` (Yarn: slipperiness), and Block.java registers the blocks. `classes`
 * gives { extends, expression? } of the classes that set it and those under them, and `declared` tells a class some
 * source declares: each registered block takes its class's nearest setter, and a class nothing declares throws.
 */
function parseLegacyFriction(registry, classes, declared, where) {
  const frictions = {};
  let count = 0;
  for (const match of registry.matchAll(LEGACY_REGISTRATION)) {
    count += 1;
    const [, name, className] = match;
    if (!declared(className)) throw new Error(`${where}: ${name} is a ${className}, which no source declares`);
    const seen = new Set();
    for (let current = className; classes[current]; current = classes[current].extends) {
      if (seen.has(current)) throw new Error(`${where}: ${current} extends itself`);
      seen.add(current);
      if (classes[current].expression != null) {
        const value = parseFloatLiteral(classes[current].expression, `${where} ${current}`);
        frictions[name] = { expression: classes[current].expression, value, bits: floatBits(value), via: current };
        break;
      }
    }
  }
  if (count === 0) throw new Error(`${where}: no legacy block registrations`);
  return Object.fromEntries(Object.entries(frictions).sort(([a], [b]) => a.localeCompare(b)));
}

/** A legacy block class's entry from its source. */
function parseLegacyClass(source) {
  const declaration = /\bclass\s+([A-Za-z0-9_]+)(?:\s+extends\s+([A-Za-z0-9_]+))?/.exec(source);
  if (!declaration) return undefined;
  const assignment = /\bthis\.(?:friction|slipperiness)\s*=\s*([^;]+);/.exec(source);
  return { name: declaration[1], extends: declaration[2], expression: assignment?.[1].trim() };
}

const sha256 = (text) => crypto.createHash("sha256").update(text).digest("hex");

module.exports = { floatBits, parseDefaultFriction, parseRegistryFriction, parseLegacyFriction, parseLegacyClass, sha256 };
