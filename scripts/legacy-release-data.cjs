// Source-derived block, fluid and tag data for the legacy releases (1.8.8 - 1.13.2), read from each release's own
// pinned jar, remapped by legacy-mappings/<release>.tiny and decompiled by scripts/materialize-sources.cjs
// (MC_LEGACY_SOURCES, scripts/source-roots.cjs legacyTree). Those names are Mojang's where 1.14.4 has the member and
// Yarn's or intermediary's otherwise, so a construct below is matched by either spelling. The description that follows
// gives the Yarn names this was first written for.
//
// The two eras are told apart by their sources, not by a table:
// - before the flattening (1.8.8 - 1.12.2) Block.setup registers numeric ids (`register(1,
//   "stone", ...)`), a block's state is `appendProperties() { return new StateManager(this, ...) }`,
//   there are no tags, no VoxelShape methods and no Fluid objects: water and lava are the
//   still/flowing liquid blocks with LEVEL 0-15 (BlockLiquid), whose tick rate
//   (BlockLiquid.tickRate) and drop-off (BlockDynamicLiquid's `int j = 1; if LAVA ... j = 2`)
//   come from the liquid block classes;
// - 1.13.2 registers string ids, builds states with `appendProperties(StateManager.Builder)`,
//   declares FluidTags/BlockTags keys with `register("...")` and their members in the
//   data/minecraft/tags JSON files, and has the FlowableFluid source/flowing fluids registered in
//   Fluid.java, whose tick rate and drop-off are the intermediary-named `method_17778` and
//   `method_17767` (Yarn names them getTickRate and getLevelDecreasePerBlock from 1.14).
//
// Property names are the serialized names upper-cased (`IntProperty.of("level", 0, 15)` is
// LEVEL), since the Yarn field names of 1.13.2 are partly unmapped. Domains follow the shared
// generator's conventions: booleans "false", "true"; integers min..max; enums their constants in
// declaration order (or the listed order), by the first string literal of each constant (the
// serialized name) or else the lower-cased constant name; a Direction filter keeps
// Direction.values() order. Anything the sources do not prove is recorded as unresolved.

const fs = require("node:fs");
const path = require("node:path");
const crypto = require("node:crypto");
const { javaCode, matchingBracket, splitTopLevel, indexClasses, classChain, methodBodyIn } = require("./java-source.cjs");

/** The body of the first of `names` a class declares. */
const bodyOf = (own, names) => names.map((name) => methodBodyIn(own, name)).find((body) => body !== null) ?? null;

// The spellings of each construct: the recorder mapping's (Mojang's where 1.14.4 has it) and Yarn's.
const STATE_METHODS = ["createBlockStateDefinition", "appendProperties"];
const DEFAULT_STATE = /\b(?:registerDefaultState|setDefaultState)\s*\(/;
const STATE_MANAGER = "(?:StateDefinition|StateManager)";
const PROPERTY_CLASSES = "(BooleanProperty|IntegerProperty|IntProperty|EnumProperty|DirectionProperty)";
const SHAPE_METHODS = {
  collision: ["getCollisionShape"],
  outline: ["getShape", "getOutlineShape"],
  interaction: ["getInteractionShape", "getRayTraceShape"],
};
const BOX = String.raw`(?:createCuboidShape|Block\.box|box)`;
const GET_PROPERTY = String.raw`(?:getProperty|getValue)`;

/**
 * The legacy releases, each read from its own decompiled jar (`source`, kept for the records that name the tree a
 * release is read from: 1.8.8 was read from 1.8.9's yarn tree until its own jar was decompiled).
 */
const LEGACY_RELEASES = [
  { version: "1.8.8", source: "1.8.8" },
  { version: "1.9.4", source: "1.9.4" },
  { version: "1.10.2", source: "1.10.2" },
  { version: "1.11.2", source: "1.11.2" },
  { version: "1.12.2", source: "1.12.2" },
  { version: "1.13.2", source: "1.13.2" },
];

function sha256(text) {
  return crypto.createHash("sha256").update(text).digest("hex");
}

function lastSegment(name) {
  return name.trim().split(".").at(-1);
}

function firstString(text) {
  return /"([^"\\]*)"/.exec(text)?.[1];
}

/** The constants of an enum declaration: `{ name, value, args }` in declaration order. */
function enumConstants(declaration) {
  const text = declaration.body;
  const code = javaCode(text);
  const constants = [];
  let index = 0;
  while (index < code.length) {
    while (index < code.length && /[\s,]/.test(code[index])) index += 1;
    const match = /^([A-Za-z_$][\w$]*)/.exec(code.slice(index));
    if (!match || code[index] === ";") break;
    const name = match[1];
    index += name.length;
    let args = "";
    while (index < code.length && /\s/.test(code[index])) index += 1;
    if (code[index] === "(") {
      const close = matchingBracket(code, index);
      args = text.slice(index + 1, close);
      index = close + 1;
    }
    while (index < code.length && /\s/.test(code[index])) index += 1;
    if (code[index] === "{") index = matchingBracket(code, index) + 1;
    constants.push({ name, value: firstString(args) ?? name.toLowerCase(), args: splitTopLevel(args) });
    while (index < code.length && /\s/.test(code[index])) index += 1;
    if (code[index] === ",") continue;
    break;
  }
  return constants;
}

class LegacyTree {
  constructor(root, release) {
    this.root = root;
    this.release = release;
    this.index = indexClasses(path.join(root, "net"));
    // (from 1.13 fluids are objects of their own: Yarn's net/minecraft/fluid, Mojang's world/level/material/Fluid.java)
    this.flattened = fs.existsSync(path.join(root, "net", "minecraft", "fluid"))
      || fs.existsSync(path.join(root, "net", "minecraft", "world", "level", "material", "Fluid.java"));
    this.unresolved = [];
  }

  file(...parts) {
    return path.join(this.root, "net", "minecraft", ...parts);
  }

  /** The first of these net/minecraft-relative files the tree has. */
  firstFile(...candidates) {
    const found = candidates.map((parts) => this.file(...parts)).find((file) => fs.existsSync(file));
    if (!found) throw new Error(`${this.release}: none of ${candidates.map((parts) => parts.join("/")).join(", ")}`);
    return found;
  }

  /** The value expression a field is initialised with, looked up from `context` up its chain. */
  fieldInitializer(name, chain) {
    for (const declaration of chain) {
      const code = javaCode(declaration.own);
      const pattern = new RegExp(String.raw`(?:(?<![\w$.])|(?<=\bthis\.))${name.replace(/\$/g, "\\$")}\s*=(?!=)`, "g");
      for (const match of code.matchAll(pattern)) {
        const start = match.index + match[0].length;
        let depth = 0;
        let end = start;
        for (; end < code.length; end += 1) {
          const current = code[end];
          if ("({[".includes(current)) depth += 1;
          else if (")}]".includes(current)) depth -= 1;
          else if (current === ";" && depth === 0) break;
        }
        return { text: declaration.own.slice(start, end).trim(), declaration };
      }
    }
    return null;
  }

  /** The expression a no-argument method returns, evaluated from the concrete class up. */
  methodReturn(name, chain) {
    for (const declaration of chain) {
      const body = methodBodyIn(declaration.own, name);
      if (body === null) continue;
      const returned = [...body.matchAll(/return\s+([\s\S]+?);/g)].at(-1)?.[1]?.trim();
      // a lazily built field (getFlowerProperties): the value assigned before it is returned
      const field = /^this\.(\w+)$/.exec(returned ?? "")?.[1];
      if (field) {
        const assigned = this.fieldInitializer(field, [{ own: body }]);
        if (assigned) return { text: assigned.text, declaration };
      }
      if (returned !== undefined) return { text: returned, declaration };
    }
    return null;
  }

  /** The constructor argument an enum constant gives the field a no-argument getter returns. */
  enumAccessor(declaration, constant, getter) {
    const field = /^this\.(\w+)$/.exec(this.methodReturn(getter, [declaration])?.text ?? "")?.[1];
    if (!field) throw new Error(`${declaration.name}.${getter}() returns no field`);
    let args = constant.args;
    const code = javaCode(declaration.own);
    for (let hops = 0; hops < 4; hops += 1) {
      // (a nested class the mapping leaves unnamed is the top-level `Outer$Inner`, whose `$` is no anchor)
      const constructors = [...code.matchAll(new RegExp(String.raw`(?<![\w$])${declaration.name.replaceAll("$", "\\$")}\s*\(([^)]*)\)\s*\{`, "g"))]
        .map((match) => {
          const open = match.index + match[0].indexOf("(");
          return {
            params: splitTopLevel(declaration.own.slice(open + 1, matchingBracket(code, open))),
            at: match.index + match[0].length - 1,
          };
        })
        .filter((candidate) => candidate.params.length === args.length);
      if (constructors.length !== 1) throw new Error(`${declaration.name} has no single constructor of ${args.length} arguments`);
      const [constructor] = constructors;
      const body = declaration.own.slice(constructor.at + 1, matchingBracket(code, constructor.at));
      const names = constructor.params.map((param) => param.trim().split(/\s+/).at(-1));
      const delegated = /^\s*this\(([\s\S]*?)\);/.exec(body);
      if (delegated) {
        args = splitTopLevel(delegated[1]).map((arg) => (names.includes(arg) ? args[names.indexOf(arg)] : arg));
        continue;
      }
      const assigned = new RegExp(String.raw`this\.${field}\s*=\s*([\w$]+)\s*;`).exec(body)?.[1];
      if (!assigned || !names.includes(assigned)) throw new Error(`${declaration.name}.${field} is not set from a constructor argument`);
      return args[names.indexOf(assigned)];
    }
    throw new Error(`${declaration.name} constructors delegate too deeply`);
  }

  /** Whether an enum constant passes a predicate expression over `parameter`. */
  passes(constant, parameter, expression, concreteChain, declaration) {
    const disjunction = expression.split("||");
    return disjunction.some((term) => term.split("&&").every((atom) => {
      const match = /^\s*\(?\s*([\w$]+)(?:\.(\w+)\(\))?\s*(==|!=|<=|>=|<|>)\s*([\w$.]+(?:\(\))?)\s*\)?\s*$/.exec(atom);
      if (!match || match[1] !== parameter) throw new Error(`unsupported predicate ${atom.trim()}`);
      let target = match[4];
      const call = /^(?:[\w$]+\.)?this\.(\w+)\(\)$/.exec(target);
      if (call) {
        const returned = this.methodReturn(call[1], concreteChain);
        if (!returned) throw new Error(`unresolved predicate operand ${target}`);
        target = returned.text;
      }
      const subject = match[2] ? this.enumAccessor(declaration, constant, match[2]) : constant.name;
      if (/^-?\d+$/.test(target)) {
        const left = Number(subject);
        const right = Number(target);
        if (!Number.isInteger(left)) throw new Error(`${constant.name}: ${match[2]}() is not an integer`);
        return {
          "==": left === right, "!=": left !== right, "<": left < right,
          "<=": left <= right, ">": left > right, ">=": left >= right,
        }[match[3]];
      }
      const equal = lastSegment(subject) === lastSegment(target);
      if (match[3] === "==") return equal;
      if (match[3] === "!=") return !equal;
      throw new Error(`unsupported predicate ${atom.trim()}`);
    }));
  }

  /** The predicate inside an anonymous Predicate class or a lambda: [parameter, expression]. */
  predicate(text) {
    const lambda = /^\(?\s*([\w$]+)\s*\)?\s*->\s*([\s\S]+)$/.exec(text.trim());
    if (lambda) return [lambda[1], lambda[2]];
    const anonymous = /apply\(\s*(?:@\w+\s+)?[\w$.<>]+\s+([\w$]+)\s*\)\s*\{\s*return\s+([\s\S]+?);\s*\}/.exec(text);
    if (anonymous) return [anonymous[1], anonymous[2]];
    return null;
  }

  enumOf(className, context) {
    const declaration = this.index.resolve(className, context);
    return declaration && declaration.kind === "enum" ? declaration : null;
  }

  directions() {
    const declaration = this.index.resolve("Direction");
    if (!declaration || declaration.kind !== "enum") throw new Error(`${this.release}: no Direction enum`);
    return enumConstants(declaration);
  }

  /** A property constructor or reference, resolved to `{ name, kind, values }`. */
  property(expression, chain, concreteChain, depth = 0) {
    const text = expression.trim().replace(/^\(\s*[\w$.<>]+\s*\)\s*/, "");
    if (depth > 12) throw new Error(`property ${text} does not resolve`);
    const construct = new RegExp(String.raw`^${PROPERTY_CLASSES}\.(?:of|create)\(`).exec(text);
    if (construct) {
      const open = construct[0].length - 1;
      const args = splitTopLevel(text.slice(open + 1, matchingBracket(javaCode(text), open)));
      const name = firstString(args[0]);
      if (!name) throw new Error(`property ${text} has no name`);
      if (construct[1] === "BooleanProperty") return { name, kind: "boolean", values: ["false", "true"] };
      if (construct[1] === "IntProperty" || construct[1] === "IntegerProperty") {
        const min = Number(args[1]);
        const max = Number(args[2]);
        if (!Number.isInteger(min) || !Number.isInteger(max) || max < min) throw new Error(`integer property ${text}`);
        return { name, kind: "integer", values: Array.from({ length: max - min + 1 }, (_, i) => String(min + i)) };
      }
      let constants;
      let rest;
      let enumDeclaration;
      if (construct[1] === "EnumProperty") {
        const className = /^([\w$.]+)\.class$/.exec(args[1] ?? "")?.[1];
        const declaration = className && this.enumOf(className, chain[0]);
        if (!declaration) throw new Error(`enum ${args[1]} is not declared in the sources`);
        constants = enumConstants(declaration);
        enumDeclaration = declaration;
        rest = args.slice(2);
      } else {
        enumDeclaration = this.index.resolve("Direction");
        constants = this.directions();
        rest = args.slice(1);
      }
      if (rest.length === 0) return { name, kind: "enum", values: constants.map((constant) => constant.value) };
      // (Yarn: Direction.DirectionType, Mojang: Direction.Plane)
      if (rest.length === 1 && /(?:DirectionType|Plane)\.(HORIZONTAL|VERTICAL)$/.test(rest[0])) {
        const vertical = rest[0].endsWith("VERTICAL");
        const kept = constants.filter((constant) => constant.args.some((arg) => /Axis\.Y$/.test(arg)) === vertical);
        return { name, kind: "enum", values: kept.map((constant) => constant.value) };
      }
      const predicate = rest.length === 1 ? this.predicate(rest[0]) : null;
      if (predicate) {
        const kept = constants.filter((constant) => this.passes(constant, predicate[0], predicate[1], concreteChain, enumDeclaration));
        return { name, kind: "enum", values: kept.map((constant) => constant.value) };
      }
      const listed = rest.map((arg) => constants.find((constant) => constant.name === lastSegment(arg)));
      if (listed.some((constant) => !constant)) throw new Error(`property ${name} lists values its enum does not declare`);
      return { name, kind: "enum", values: listed.map((constant) => constant.value) };
    }
    const indexed = /^([\w$.]+)\[(\d+)\]$/.exec(text);
    if (indexed) {
      const array = this.reference(indexed[1], chain);
      const elements = /^new\s+[\w$.]+\[\]\s*\{([\s\S]*)\}$/.exec(array.text.trim());
      if (!elements) throw new Error(`array ${indexed[1]} has no initializer`);
      const element = splitTopLevel(elements[1])[Number(indexed[2])];
      return this.property(element, classChain(this.index, array.declaration), concreteChain, depth + 1);
    }
    const call = /^(?:this\.)?(\w+)\(\)$/.exec(text);
    if (call) {
      const returned = this.methodReturn(call[1], concreteChain);
      if (!returned) throw new Error(`method ${call[1]} returns no property`);
      return this.property(returned.text, classChain(this.index, returned.declaration), concreteChain, depth + 1);
    }
    if (/^(?:this\.)?[\w$]+(?:\.[\w$]+)*$/.test(text)) {
      const found = this.reference(text.replace(/^this\./, ""), chain);
      return this.property(found.text, classChain(this.index, found.declaration), concreteChain, depth + 1);
    }
    throw new Error(`unsupported property expression ${text}`);
  }

  /** A field reference (`NAME` or `Class.NAME`) resolved to its initializer. */
  reference(text, chain) {
    const parts = text.split(".");
    const field = parts.pop();
    const searched = parts.length > 0
      ? classChain(this.index, this.index.resolve(parts.join("."), chain[0]) ?? { own: "", extends: null })
      : chain;
    const found = this.fieldInitializer(field, searched.filter(Boolean));
    if (!found) throw new Error(`field ${text} has no initializer`);
    return found;
  }

  /** The property expressions a block class declares for its state, in declaration order. */
  stateProperties(chain) {
    for (let position = 0; position < chain.length; position += 1) {
      const declaration = chain[position];
      const body = bodyOf(declaration.own, STATE_METHODS);
      if (body === null) continue;
      if (this.flattened) {
        const properties = [];
        if (/super\.(?:createBlockStateDefinition|appendProperties)\(/.test(body)) properties.push(...this.stateProperties(chain.slice(position + 1)));
        for (const match of javaCode(body).matchAll(/\.(?:method_16928|add|with)\(/g)) {
          const open = match.index + match[0].length - 1;
          properties.push(...splitTopLevel(body.slice(open + 1, matchingBracket(javaCode(body), open))));
        }
        return properties;
      }
      let returned = /return\s+([\s\S]+?);\s*$/.exec(body.trim())?.[1];
      if (!returned) throw new Error(`${declaration.name}: its state definition returns nothing`);
      const ternary = new RegExp(String.raw`^(?:this\.)?(\w+)\(\)\s*\?\s*([\s\S]+?)\s*:\s*(new\s+${STATE_MANAGER}\([\s\S]+\))$`).exec(returned);
      if (ternary) {
        const condition = this.methodReturn(ternary[1], chain)?.text;
        if (condition !== "true" && condition !== "false") throw new Error(`${declaration.name}: ${ternary[1]}() is not a constant`);
        returned = condition === "true" ? ternary[2] : ternary[3];
      }
      const manager = new RegExp(String.raw`^new\s+${STATE_MANAGER}\(([\s\S]*)\)$`).exec(returned.trim());
      if (!manager) throw new Error(`${declaration.name}: its state definition builds no StateDefinition`);
      const args = splitTopLevel(manager[1]);
      if (args[0] !== "this") throw new Error(`${declaration.name}: its StateDefinition is not built for this`);
      const listed = args.slice(1);
      if (listed.length === 1 && /^new\s+[\w$.]+\[\]\s*\{/.test(listed[0])) {
        return splitTopLevel(/\{([\s\S]*)\}/.exec(listed[0])[1]);
      }
      return listed;
    }
    return [];
  }

  /** The default value expressions setDefaultState gives each property identifier. */
  defaults(chain) {
    for (const declaration of chain) {
      const code = javaCode(declaration.own);
      const at = code.search(DEFAULT_STATE);
      if (at < 0) continue;
      const open = code.indexOf("(", at);
      const text = declaration.own.slice(open + 1, matchingBracket(code, open));
      const values = [];
      for (const match of javaCode(text).matchAll(/\.(?:with|withProperty|setValue)\(/g)) {
        const argsOpen = match.index + match[0].length - 1;
        const args = splitTopLevel(text.slice(argsOpen + 1, matchingBracket(javaCode(text), argsOpen)));
        if (args.length === 2) values.push([args[0], args[1]]);
      }
      return values;
    }
    return [];
  }

  /** The id → block class map of Block.setup, with the class names. */
  registry() {
    const sourcePath = this.firstFile(["block", "Block.java"], ["world", "level", "block", "Block.java"]);
    const source = fs.readFileSync(sourcePath, "utf8");
    const code = javaCode(source);
    const blocks = new Map();
    const classOf = (expression, position) => {
      const text = expression.trim().replace(/^\(\s*(?:\(\s*[\w$.]+\s*\)\s*)?/, "");
      const created = /^new\s+([\w$.]+)\s*[(<]/.exec(text);
      if (created) return created[1];
      const variable = /^([\w$]+)$/.exec(text)?.[1];
      if (variable) {
        const assignments = [...code.slice(0, position).matchAll(new RegExp(String.raw`\b${variable}\s*=\s*new\s+([\w$.]+)\s*[(<]`, "g"))];
        if (assignments.length > 0) return assignments.at(-1)[1];
      }
      throw new Error(`${this.release}: block ${expression.trim().slice(0, 60)} has no class`);
    };
    // a class by its simple name, unless two top-level classes share it (the recorder's mapping names 1.13.2's snow
    // block net.minecraft.block.SnowLayerBlock beside the snow layer's SnowLayerBlock): then by its qualified name,
    // from Block.java's imports or package
    const blockPackage = /\bpackage\s+([\w.]+)\s*;/.exec(source)?.[1];
    const named = (written) => {
      const simple = lastSegment(written);
      if (this.index.all(simple).filter((declaration) => declaration.outer === null).length < 2) return simple;
      if (written.includes(".")) return written;
      return new RegExp(String.raw`\bimport\s+([\w.]+\.${simple})\s*;`).exec(source)?.[1] ?? `${blockPackage}.${simple}`;
    };
    // string literals are blanked in `code`: a registration's name argument starts with a quote
    // (or is AIR_ID) at the same offset of the source
    const pattern = this.flattened ? /\bregister\(/g : /\bregister\(\s*\d+\s*,/g;
    for (const match of code.matchAll(pattern)) {
      const next = source.slice(match.index + match[0].length).trimStart();
      if (!next.startsWith("\"") && !next.startsWith("AIR_ID")) continue;
      const open = code.indexOf("(", match.index);
      const args = splitTopLevel(source.slice(open + 1, matchingBracket(code, open)));
      const nameArg = this.flattened ? args[0] : args[1];
      const id = nameArg === "AIR_ID"
        ? firstString(/AIR_ID\s*=\s*new\s+(?:Identifier|ResourceLocation)\(([^)]*)\)/.exec(source)?.[1] ?? "")
        : firstString(nameArg);
      if (!id) throw new Error(`${this.release}: register(${args.join(", ").slice(0, 40)}) has no id`);
      if (blocks.has(id)) throw new Error(`${this.release}: ${id} is registered twice`);
      blocks.set(id, named(classOf(args.at(-1), match.index)));
    }
    if (this.flattened) {
      const registrySource = fs.readFileSync(this.firstFile(["util", "registry", "Registry.java"], ["core", "Registry.java"]), "utf8");
      const defaultId = /\bBLOCK\s*=\s*\w+\(\s*"block"\s*,[^;]*?(?:Identifier|ResourceLocation)\(\s*"([a-z0-9_]+)"/.exec(registrySource)?.[1];
      for (const match of code.matchAll(/\badd\(\s*Registry\.BLOCK\.(?:getDefaultId|getDefaultKey)\(\)\s*,\s*([\w$]+)\s*\)/g)) {
        if (!defaultId) throw new Error(`${this.release}: Registry.BLOCK has no default id`);
        blocks.set(defaultId, named(classOf(match[1], match.index)));
      }
    }
    if (blocks.size === 0) throw new Error(`${this.release}: no block registrations`);
    return {
      source: sourcePath,
      sha256: sha256(source),
      classes: Object.fromEntries([...blocks.entries()].sort(([left], [right]) => (left < right ? -1 : left > right ? 1 : 0))),
    };
  }

  /** A block class's state definition and, from 1.13, its VoxelShape method provenance. */
  block(className) {
    const declaration = this.index.resolve(className);
    if (!declaration) {
      this.unresolved.push(`${className}: class not found`);
      return { className, file: null, properties: [], propertyOrder: [], stateCount: null, unresolved: ["class"] };
    }
    const chain = classChain(this.index, declaration);
    const properties = [];
    const identifiers = new Map();
    const unresolved = [];
    for (const expression of this.stateProperties(chain)) {
      try {
        const property = this.property(expression, chain, chain);
        const name = property.name.toUpperCase();
        identifiers.set(lastSegment(expression.replace(/\[\d+\]$/, "")), name);
        identifiers.set(expression.trim(), name);
        if (properties.some((existing) => existing.name === name)) throw new Error(`property ${name} declared twice`);
        properties.push({ name, kind: property.kind, values: property.values, defaults: [] });
      } catch (error) {
        unresolved.push(`${expression}: ${error.message}`);
      }
    }
    for (const [identifier, value] of this.defaults(chain)) {
      const name = identifiers.get(identifier.trim()) ?? identifiers.get(lastSegment(identifier));
      const property = properties.find((candidate) => candidate.name === name);
      // (as Java writes it: an unnamed nested class's `Outer$Inner` is Outer.Inner)
      const written = value.trim().replaceAll("$", ".");
      if (property && !property.defaults.includes(written)) property.defaults.push(written);
    }
    for (const entry of unresolved) this.unresolved.push(`${className}: ${entry}`);
    const stateCount = unresolved.length === 0 && properties.every((property) => property.values.length > 0)
      ? properties.reduce((count, property) => count * property.values.length, 1)
      : null;
    return {
      className,
      file: declaration.file,
      sha256: sha256(fs.readFileSync(declaration.file, "utf8")),
      properties,
      propertyOrder: properties.map((property) => property.name),
      stateCount,
      unresolved,
      shapes: this.flattened ? this.shapes(declaration, chain, identifiers) : null,
    };
  }

  /** 1.13's VoxelShape methods (SHAPE_METHODS: Mojang's getShape and getInteractionShape are Yarn's getOutlineShape and getRayTraceShape). */
  shapes(declaration, chain, identifiers) {
    const constants = new Map();
    for (const link of chain) {
      for (const match of javaCode(link.own).matchAll(/\bstatic\s+(?:final\s+)?VoxelShape(?:\[\])?\s+([\w$]+)\s*=/g)) {
        if (!constants.has(match[1])) constants.set(match[1], this.fieldInitializer(match[1], [link])?.text ?? "");
      }
    }
    const boxes = (text) => {
      const seen = new Set();
      const expressions = [text];
      while (expressions.length > 0) {
        for (const identifier of expressions.pop().match(/[\w$]+/g) ?? []) {
          if (seen.has(identifier) || !constants.has(identifier)) continue;
          seen.add(identifier);
          expressions.push(constants.get(identifier));
        }
      }
      const all = [text, ...[...seen].map((name) => constants.get(name))].join("\n");
      const number = String.raw`\s*(-?[\d.]+)\s*`;
      const found = [...all.matchAll(new RegExp(String.raw`(?<![\w$])${BOX}\(${Array(6).fill(number).join(",")}\)`, "g"))]
        .map((match) => match.slice(1).map(Number));
      return [...new Map(found.map((box) => [box.join(","), box])).values()];
    };
    const propertyName = (identifier) => identifiers.get(identifier) ?? identifier;
    const result = {};
    const declared = chain.map((link) => javaCode(link.own)).join("\n");
    for (const [kind, names] of Object.entries(SHAPE_METHODS)) {
      const name = names.find((candidate) => new RegExp(String.raw`\bVoxelShape\s+${candidate}\s*\(`).test(declared)) ?? names[0];
      const direct = new RegExp(String.raw`\bVoxelShape\s+${name}\s*\(`).test(javaCode(declaration.own));
      const own = methodBodyIn(declaration.own, name);
      let inherited = null;
      for (const link of chain) {
        inherited = methodBodyIn(link.own, name);
        if (inherited !== null) break;
      }
      const body = javaCode(inherited ?? "");
      const signature = own === null ? null : new RegExp(String.raw`\b${name}\s*\([^)]*\)\s*\{`).exec(declaration.own)?.[0];
      result[kind] = {
        hasMethod: direct,
        sha256: own === null || !signature ? null : sha256(`${signature}${own}`),
        boxes: inherited === null ? [] : boxes(inherited),
        dependencies: [...new Set([...body.matchAll(new RegExp(String.raw`${GET_PROPERTY}\(\s*(?:[\w$]+\.)?([\w$]+)\s*\)`, "g"))].map((match) => propertyName(match[1])))].sort(),
        predicates: [...new Map([...body.matchAll(new RegExp(String.raw`${GET_PROPERTY}\(\s*(?:[\w$]+\.)?([\w$]+)\s*\)\s*(==|!=)\s*([\w$.]+)`, "g"))]
          .map((match) => [propertyName(match[1]), match[2], match[3]])
          .map((predicate) => [predicate.join("\0"), predicate])).values()]
          .sort((left, right) => left.join("\0").localeCompare(right.join("\0"))),
        stateDependent: direct && /\.(?:getProperty|getValue)\s*\(|\bgetFluidState\s*\(|\bswitch\s*\(|\?/.test(body),
      };
    }
    result.union = {
      boxes: boxes(chain.map((link) => link.own).join("\n")),
      dependencies: [...new Set([...javaCode(chain.map((link) => link.own).join("\n")).matchAll(new RegExp(String.raw`${GET_PROPERTY}\(\s*(?:[\w$]+\.)?([\w$]+)\s*\)`, "g"))].map((match) => propertyName(match[1])))].sort(),
    };
    return result;
  }

  /** The baseline (non-ultrawarm) value a tick-rate or drop-off body returns. */
  static baseline(body, what) {
    const returned = [...javaCode(body).matchAll(/return\s+([^;]+);/g)].map((match) => match[1].trim());
    if (returned.length !== 1) throw new Error(`${what} has ${returned.length} returns`);
    const ternary = /\?\s*(\d+)\s*:\s*(\d+)$/.exec(returned[0]);
    const value = ternary ? ternary[2] : /^(\d+)$/.exec(returned[0])?.[1];
    if (value === undefined) throw new Error(`${what} returns ${returned[0]}`);
    return Number(value);
  }

  fluids() {
    if (this.flattened) return this.modernFluids();
    // (Mojang: LiquidBlock; Yarn: AbstractFluidBlock. The flowing and still blocks keep Yarn's names in both)
    const abstractPath = this.firstFile(["world", "level", "block", "LiquidBlock.java"], ["block", "AbstractFluidBlock.java"]);
    const flowingPath = this.firstFile(["block", "FlowingFluidBlock.java"]);
    const stillPath = this.firstFile(["block", "FluidBlock.java"]);
    const abstractSource = fs.readFileSync(abstractPath, "utf8");
    const flowingSource = fs.readFileSync(flowingPath, "utf8");
    const level = /\bLEVEL\s*=\s*(?:IntProperty\.of|IntegerProperty\.create)\(\s*"level"\s*,\s*(\d+)\s*,\s*(\d+)\s*\)/.exec(abstractSource);
    if (!level) throw new Error(`${this.release}: the liquid block has no LEVEL property`);
    const values = Array.from({ length: Number(level[2]) - Number(level[1]) + 1 }, (_, i) => String(Number(level[1]) + i));
    const tickBody = bodyOf(abstractSource, ["getTickDelay", "getTickRate"]);
    const tickCode = javaCode(tickBody ?? "");
    const water = /Material\.WATER\)\s*\{\s*return\s+(\d+);/.exec(tickCode)?.[1];
    const lava = /Material\.LAVA\)\s*\{\s*return\s+[^;]*?\?\s*\d+\s*:\s*(\d+);/.exec(tickCode)?.[1];
    if (!water || !lava) throw new Error(`${this.release}: the tick delay does not give water and lava rates`);
    const drop = /int\s+(\w+)\s*=\s*(\d+);\s*if\s*\(\s*this\.material\s*==\s*Material\.LAVA[^{]*\)\s*\{\s*\1\s*=\s*(\d+);/.exec(javaCode(flowingSource));
    if (!drop) throw new Error(`${this.release}: the flowing liquid block has no water/lava drop-off`);
    const registry = this.registry().classes;
    const fluids = [];
    for (const [fluid, tick, dropOff] of [["water", water, drop[2]], ["lava", lava, drop[3]]]) {
      for (const flowing of [false, true]) {
        const id = `${flowing ? "flowing_" : ""}${fluid}`;
        const expected = flowing ? "FlowingFluidBlock" : "FluidBlock";
        if (registry[id] !== expected) throw new Error(`${this.release}: ${id} is a ${registry[id]}, not a ${expected}`);
        fluids.push({
          id,
          source: !flowing,
          propertyNames: ["LEVEL"],
          propertyDomains: [values],
          stateCount: values.length,
          tickDelay: Number(tick),
          normalDropOff: Number(dropOff),
          sourceSha256: sha256(fs.readFileSync(flowing ? flowingPath : stillPath, "utf8")),
          flowSha256: null,
          heightSha256: null,
          amountSha256: null,
        });
      }
    }
    return fluids;
  }

  modernFluids() {
    // (Mojang: world/level/material Fluid, FlowingFluid and BlockStateProperties; Yarn: fluid/Fluid, FlowableFluid and Properties)
    const fluidSource = fs.readFileSync(this.firstFile(["world", "level", "material", "Fluid.java"], ["fluid", "Fluid.java"]), "utf8");
    const flowableSource = fs.readFileSync(this.firstFile(["world", "level", "material", "FlowingFluid.java"], ["fluid", "FlowableFluid.java"]), "utf8");
    const propertiesSource = fs.readFileSync(this.firstFile(["world", "level", "block", "state", "properties", "BlockStateProperties.java"], ["states", "property", "Properties.java"]), "utf8");
    const levelField = /\bLEVEL\s*=\s*(?:Properties|BlockStateProperties)\.(\w+)\s*;/.exec(flowableSource)?.[1];
    const level = levelField && new RegExp(String.raw`\b${levelField}\s*=\s*(?:IntProperty\.of|IntegerProperty\.create)\(\s*"level"\s*,\s*(\d+)\s*,\s*(\d+)\s*\)`).exec(propertiesSource);
    const falling = /\bFALLING\s*=\s*(?:Properties|BlockStateProperties)\.FALLING\s*;/.test(flowableSource)
      && /\bFALLING\s*=\s*BooleanProperty\.(?:of|create)\(\s*"falling"\s*\)/.test(propertiesSource);
    if (!level || !falling) throw new Error(`${this.release}: cannot prove the FlowableFluid state definition`);
    const levels = Array.from({ length: Number(level[2]) - Number(level[1]) + 1 }, (_, i) => String(Number(level[1]) + i));
    const fluids = [];
    for (const [fluid] of [["water"], ["lava"]]) {
      for (const flowing of [false, true]) {
        const id = `${flowing ? "flowing_" : ""}${fluid}`;
        const registered = new RegExp(String.raw`\(\s*"${id}"\s*,\s*new\s+([\w$]+)\.([\w$]+)\(\)\s*\)`).exec(fluidSource);
        if (!registered) throw new Error(`${this.release}: fluid ${id} is not registered`);
        const declaration = this.index.resolve(registered[1]);
        if (!declaration) throw new Error(`${this.release}: fluid class ${registered[1]} not found`);
        const source = fs.readFileSync(declaration.file, "utf8");
        // Mojang's getTickDelay and getDropOff, intermediary method_17778 and method_17767 in Yarn's 1.13.2
        const tick = LegacyTree.baseline(bodyOf(declaration.own, ["getTickDelay", "method_17778"]) ?? "", `${id} tick rate`);
        const dropOff = LegacyTree.baseline(bodyOf(declaration.own, ["getDropOff", "method_17767"]) ?? "", `${id} drop-off`);
        fluids.push({
          id,
          source: !flowing,
          propertyNames: flowing ? ["LEVEL", "FALLING"] : ["FALLING"],
          propertyDomains: flowing ? [levels, ["false", "true"]] : [["false", "true"]],
          stateCount: flowing ? levels.length * 2 : 2,
          tickDelay: tick,
          normalDropOff: dropOff,
          sourceSha256: sha256(source),
          flowSha256: null,
          heightSha256: null,
          amountSha256: null,
        });
      }
    }
    return fluids;
  }

  /** FluidTags and BlockTags keys with the block tag members of the data pack. */
  tags() {
    if (!this.flattened) return { fluidTags: [], blockTags: [] };
    const keys = (file, type) => {
      const source = fs.readFileSync(this.firstFile(["tags", file], ["tag", file]), "utf8");
      return {
        sha256: sha256(source),
        ids: [...source.matchAll(new RegExp(String.raw`public\s+static\s+final\s+Tag<${type}>\s+[\w$]+\s*=\s*(?:register|of|bind)\("([a-z0-9_/]+)"\)`, "g"))].map((match) => match[1]),
      };
    };
    const fluid = keys("FluidTags.java", "Fluid");
    const block = keys("BlockTags.java", "Block");
    const dataRoot = path.join(this.root, "data", "minecraft", "tags", "blocks");
    const files = fs.readdirSync(dataRoot).filter((entry) => entry.endsWith(".json")).sort();
    const membershipSha256 = sha256(files.map((entry) => `${entry}:${fs.readFileSync(path.join(dataRoot, entry), "utf8")}`).join("\n"));
    const members = new Map(files.map((entry) => {
      const values = JSON.parse(fs.readFileSync(path.join(dataRoot, entry), "utf8")).values ?? [];
      return [entry.slice(0, -".json".length), {
        blocks: values.filter((value) => !value.startsWith("#")).map((value) => value.replace(/^minecraft:/, "")).sort(),
        includes: values.filter((value) => value.startsWith("#")).map((value) => value.replace(/^#minecraft:/, "")).sort(),
      }];
    }));
    return {
      fluidTags: fluid.ids.map((id) => ({ id, sha256: fluid.sha256 })),
      blockTags: block.ids.map((id) => {
        const membership = members.get(id);
        if (!membership) throw new Error(`${this.release}: block tag ${id} has no data file`);
        return { id, sha256: block.sha256, ...membership, membershipSha256 };
      }),
    };
  }
}

/** Everything the native data generator emits for one legacy release. */
function extractLegacyRelease(release, decompiledRoot) {
  const root = path.join(decompiledRoot, release.source);
  if (!["block", path.join("world", "level", "block")].some((family) => fs.existsSync(path.join(root, "net", "minecraft", family, "Block.java")))) return null;
  const tree = new LegacyTree(root, release.version);
  const registry = tree.registry();
  const classes = [...new Set(Object.values(registry.classes))].sort();
  const blocks = {};
  for (const className of classes) blocks[className] = tree.block(className);
  return {
    version: release.version,
    sourceRelease: release.source,
    standIn: release.reason ?? null,
    flattened: tree.flattened,
    registry,
    blocks,
    fluids: tree.fluids(),
    ...tree.tags(),
    unresolved: tree.unresolved,
  };
}

module.exports = { LEGACY_RELEASES, extractLegacyRelease, enumConstants };
