// Small Java source helpers shared by the data generators: comment/string blanking, balanced
// bracket matching, top-level argument splitting and a class index that knows nested classes.

const fs = require("node:fs");
const path = require("node:path");

/** The source with comments and string/char literal contents blanked to spaces (same offsets). */
function javaCode(source) {
  let result = "";
  let state = "code";
  for (let index = 0; index < source.length; index += 1) {
    const current = source[index];
    const next = source[index + 1];
    if (state === "line-comment") {
      if (current === "\n") {
        state = "code";
        result += current;
      } else {
        result += " ";
      }
      continue;
    }
    if (state === "block-comment") {
      if (current === "*" && next === "/") {
        state = "code";
        result += "  ";
        index += 1;
      } else {
        result += current === "\n" ? "\n" : " ";
      }
      continue;
    }
    if (state === "string" || state === "char") {
      if (current === "\\") {
        result += "  ";
        index += 1;
      } else if ((state === "string" && current === "\"") || (state === "char" && current === "'")) {
        state = "code";
        result += " ";
      } else {
        result += current === "\n" ? "\n" : " ";
      }
      continue;
    }
    if (current === "/" && next === "/") {
      state = "line-comment";
      result += "  ";
      index += 1;
    } else if (current === "/" && next === "*") {
      state = "block-comment";
      result += "  ";
      index += 1;
    } else if (current === "\"") {
      state = "string";
      result += " ";
    } else if (current === "'") {
      state = "char";
      result += " ";
    } else {
      result += current;
    }
  }
  return result;
}

const OPENERS = { "(": ")", "{": "}", "[": "]" };

/** The index of the bracket closing the one at `open` in blanked code, or -1. */
function matchingBracket(code, open) {
  const stack = [];
  for (let index = open; index < code.length; index += 1) {
    const current = code[index];
    if (OPENERS[current]) stack.push(OPENERS[current]);
    else if (current === ")" || current === "}" || current === "]") {
      if (stack.pop() !== current) return -1;
      if (stack.length === 0) return index;
    }
  }
  return -1;
}

/** `text` split at top-level `separator`s (outside brackets), trimmed, empties dropped. */
function splitTopLevel(text, separator = ",") {
  const code = javaCode(text);
  const parts = [];
  let depth = 0;
  let start = 0;
  for (let index = 0; index < code.length; index += 1) {
    const current = code[index];
    if (OPENERS[current]) depth += 1;
    else if (current === ")" || current === "}" || current === "]") depth -= 1;
    else if (current === separator && depth === 0) {
      parts.push(text.slice(start, index).trim());
      start = index + 1;
    }
  }
  parts.push(text.slice(start).trim());
  return parts.filter((part) => part.length > 0);
}

/**
 * Every class, enum and interface declared in the .java files under `root` (nested ones too),
 * by simple name: `{ name, kind, file, outer, extends, body, own }` where `body` is the text
 * between its braces and `own` the same with nested declarations blanked out.
 */
function indexClasses(root) {
  const byName = new Map();
  const files = [];
  const visit = (directory) => {
    for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
      const entryPath = path.join(directory, entry.name);
      if (entry.isDirectory()) visit(entryPath);
      else if (entry.isFile() && entry.name.endsWith(".java")) files.push(entryPath);
    }
  };
  visit(root);
  files.sort();
  for (const file of files) {
    const source = fs.readFileSync(file, "utf8");
    const code = javaCode(source);
    const declarations = [];
    const pattern = /\b(class|enum|interface)\s+([A-Za-z_$][\w$]*)([^{;]*)\{/g;
    for (const match of code.matchAll(pattern)) {
      const open = match.index + match[0].length - 1;
      const close = matchingBracket(code, open);
      if (close < 0) continue;
      const header = match[3];
      declarations.push({
        name: match[2],
        kind: match[1],
        file,
        start: open + 1,
        end: close,
        extends: /\bextends\s+([\w$.]+)/.exec(header)?.[1] ?? null,
        source,
      });
    }
    for (const declaration of declarations) {
      const outer = declarations
        .filter((other) => other !== declaration && other.start < declaration.start && other.end > declaration.end)
        .sort((left, right) => right.start - left.start)[0];
      declaration.outer = outer?.name ?? null;
      const nested = declarations.filter((other) => other !== declaration
        && other.start > declaration.start && other.end < declaration.end);
      let own = source.slice(declaration.start, declaration.end);
      for (const inner of nested) {
        const from = inner.start - declaration.start;
        const to = inner.end - declaration.start;
        own = own.slice(0, from) + " ".repeat(to - from) + own.slice(to);
      }
      declaration.body = source.slice(declaration.start, declaration.end);
      declaration.own = own;
      if (!byName.has(declaration.name)) byName.set(declaration.name, []);
      byName.get(declaration.name).push(declaration);
    }
  }
  return {
    files,
    /** The declaration a (possibly qualified) name refers to, or null when absent or ambiguous. */
    resolve(name, context = null) {
      const parts = name.split(".");
      const simple = parts.at(-1);
      let candidates = byName.get(simple) ?? [];
      if (parts.length > 1) {
        const outer = parts.at(-2);
        const nested = candidates.filter((candidate) => candidate.outer === outer);
        if (nested.length > 0) candidates = nested;
      }
      if (candidates.length > 1 && context) {
        const local = candidates.filter((candidate) => candidate.file === context.file);
        if (local.length > 0) candidates = local;
      }
      if (candidates.length > 1) {
        const topLevel = candidates.filter((candidate) => candidate.outer === null);
        if (topLevel.length === 1) candidates = topLevel;
      }
      return candidates.length === 1 ? candidates[0] : null;
    },
    all(name) {
      return byName.get(name) ?? [];
    },
  };
}

/** The superclass chain of a declaration, itself first. */
function classChain(index, declaration) {
  const chain = [];
  let current = declaration;
  while (current && chain.length < 16 && !chain.includes(current)) {
    chain.push(current);
    current = current.extends ? index.resolve(current.extends, current) : null;
  }
  return chain;
}

/** The body of the first method `name(...) {` in `text` (blanked-code aware), or null. */
function methodBodyIn(text, name) {
  const code = javaCode(text);
  const pattern = new RegExp(String.raw`\b${name}\s*\(`, "g");
  for (const match of code.matchAll(pattern)) {
    const openParen = match.index + match[0].length - 1;
    const closeParen = matchingBracket(code, openParen);
    if (closeParen < 0) continue;
    const after = code.slice(closeParen + 1);
    const brace = /^\s*(?:throws\s+[\w.,\s]+)?\{/.exec(after);
    if (!brace) continue;
    const open = closeParen + 1 + brace[0].length - 1;
    const close = matchingBracket(code, open);
    if (close < 0) continue;
    // a declaration has a type or modifier before the name, not `.` or `new`
    const before = code.slice(Math.max(0, match.index - 40), match.index);
    if (/(?:\.|\bnew\s+|\breturn\s+|=\s*)$/.test(before)) continue;
    return text.slice(open + 1, close);
  }
  return null;
}

module.exports = { javaCode, matchingBracket, splitTopLevel, indexClasses, classChain, methodBodyIn };
