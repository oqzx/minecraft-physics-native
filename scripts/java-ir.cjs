// A typed intermediate form of a Java method, as small as what it has to keep: literals with their type, locals, casts,
// branches, ordered calls, field reads and writes. Each numeric promotion Java does silently is an explicit cast here, so
// `(double)speed` and a widening in `movement.x * cos` look alike and neither can go missing. Evaluation is in source
// order with float arithmetic in float, and short-circuiting as Java's, and it leaves a trace of each step.
//
// It covers the shape of a straight-line tick method and nothing more: a construct it does not model (a loop, a lambda, a
// switch, an increment, instanceof) is refused with its line and column, never approximated. Which methods it holds is
// decided by whether they parse. What a helper returns has to be declared in `signatures`: a call with no admitted
// signature is refused too, as nothing else says what its type is.
class IrUnsupported extends Error {
  constructor(message, source, offset) {
    const before = source.slice(0, offset).split("\n");
    super(`${message} at ${before.length}:${before.at(-1).length + 1}`);
    this.line = before.length;
    this.column = before.at(-1).length + 1;
  }
}

const REJECTED = new Set(["for", "while", "do", "switch", "try", "catch", "throw", "synchronized", "instanceof", "yield", "assert", "break", "continue"]);
const PRIMITIVES = new Set(["float", "double", "int", "long", "boolean"]);
const TOKEN = /\s+|\/\/[^\n]*|\/\*[\s\S]*?\*\/|(?<number>\d+\.\d*(?:[eE][-+]?\d+)?[FfDd]?|\.\d+(?:[eE][-+]?\d+)?[FfDd]?|\d+[eE][-+]?\d+[FfDd]?|\d+[FfDdLl]?)|(?<word>[A-Za-z_$][\w$]*)|(?<op>->|\+\+|--|\+=|-=|\*=|\/=|&&|\|\||==|!=|<=|>=|::|[-+*/%<>=!?:;,.(){}[\]@&|^~])/y;

function tokenize(source) {
  const tokens = [];
  let at = 0;
  while (at < source.length) {
    TOKEN.lastIndex = at;
    const match = TOKEN.exec(source);
    if (!match) throw new IrUnsupported(`Unknown text ${JSON.stringify(source[at])}`, source, at);
    const { number, word, op } = match.groups;
    if (number !== undefined) tokens.push({ kind: "number", text: number, at });
    else if (word !== undefined) tokens.push({ kind: "word", text: word, at });
    else if (op !== undefined) tokens.push({ kind: "op", text: op, at });
    at = TOKEN.lastIndex;
  }
  tokens.push({ kind: "end", text: "", at });
  return tokens;
}

const rank = { int: 1, long: 2, float: 3, double: 4 };
const promoted = (a, b) => (rank[a] >= rank[b] ? a : b);
const isNumeric = (type) => rank[type] !== undefined;

class Parser {
  constructor(source, signatures) {
    this.source = source;
    this.signatures = signatures;
    this.tokens = tokenize(source);
    this.at = 0;
    this.scopes = [new Map()];
  }

  get token() { return this.tokens[this.at]; }
  peek(offset = 1) { return this.tokens[this.at + offset] ?? this.tokens.at(-1); }
  is(text) { return this.token.text === text; }
  fail(message, token = this.token) { throw new IrUnsupported(message, this.source, token.at); }
  take(text) {
    if (!this.is(text)) this.fail(`Expected ${text}, found ${JSON.stringify(this.token.text)}`);
    this.at += 1;
  }
  accept(text) {
    if (!this.is(text)) return false;
    this.at += 1;
    return true;
  }
  declare(name, type) { this.scopes.at(-1).set(name, type); }
  lookup(name) {
    for (const scope of [...this.scopes].reverse()) if (scope.has(name)) return scope.get(name);
    return undefined;
  }

  method() {
    while (["public", "protected", "private", "static", "final", "abstract", "synchronized"].includes(this.token.text)) this.at += 1;
    const returns = this.type();
    const name = this.token.text;
    this.at += 1;
    this.take("(");
    const parameters = [];
    while (!this.is(")")) {
      this.accept("final");
      const type = this.type();
      const id = this.token.text;
      this.at += 1;
      parameters.push({ name: id, type });
      this.declare(id, type);
      if (!this.accept(",")) break;
    }
    this.take(")");
    return { kind: "method", name, returns, parameters, body: this.block() };
  }

  type() {
    if (this.token.kind !== "word") this.fail("Expected a type");
    let name = this.token.text;
    this.at += 1;
    if (this.is("<") || this.is("[")) this.fail(`Generic or array type ${name} is not modelled`);
    while (this.is(".") && this.peek().kind === "word" && /^[A-Z]/.test(this.peek().text)) {
      this.at += 2;
      name = this.tokens[this.at - 1].text;
    }
    return name;
  }

  block() {
    this.take("{");
    this.scopes.push(new Map());
    const statements = [];
    while (!this.is("}")) {
      if (this.token.kind === "end") this.fail("The method does not end");
      statements.push(this.statement());
    }
    this.take("}");
    this.scopes.pop();
    return statements;
  }

  isDeclaration() {
    if (this.is("final")) return true;
    if (this.token.kind !== "word") return false;
    return this.peek().kind === "word" && (PRIMITIVES.has(this.token.text) || /^[A-Z]/.test(this.token.text));
  }

  statement() {
    if (REJECTED.has(this.token.text)) this.fail(`${this.token.text} is not modelled`);
    if (this.is("{")) return { kind: "block", body: this.block() };
    if (this.is("if")) return this.branch();
    if (this.accept("return")) {
      const value = this.is(";") ? null : this.expression();
      this.take(";");
      return { kind: "return", value };
    }
    if (this.isDeclaration()) {
      this.accept("final");
      const type = this.type();
      const name = this.token.text;
      this.at += 1;
      if (this.accept(";")) {
        this.declare(name, type);
        return { kind: "block", body: [] };
      }
      this.take("=");
      const value = this.convert(this.expression(), type);
      this.take(";");
      this.declare(name, type);
      return { kind: "local", name, type, value };
    }
    const target = this.expression();
    if (this.is("+=") || this.is("-=") || this.is("*=") || this.is("/=")) this.fail("A compound assignment is not modelled");
    if (this.accept("=")) {
      if (target.kind !== "field" && target.kind !== "name") this.fail("Only a field or a local can be assigned");
      const value = this.convert(this.expression(), target.type);
      this.take(";");
      return { kind: "assign", target, value };
    }
    this.take(";");
    if (target.kind !== "call") this.fail("A statement that does nothing");
    return { kind: "call", call: target };
  }

  branch() {
    this.take("if");
    this.take("(");
    const test = this.expression();
    this.take(")");
    const then = this.is("{") ? this.block() : [this.statement()];
    let otherwise = [];
    if (this.accept("else")) otherwise = this.is("{") ? this.block() : [this.statement()];
    if (test.type !== "boolean") this.fail("A branch on a value that is no boolean");
    return { kind: "if", test, then, otherwise };
  }

  /** The node of `value` as `type`: an explicit cast where Java widens or narrows it silently. */
  convert(value, type) {
    if (value.type === type || !isNumeric(type) || !isNumeric(value.type)) return value;
    if (rank[type] < rank[value.type]) this.fail(`${value.type} does not widen to ${type}`);
    return { kind: "cast", to: type, value, type, implicit: true };
  }

  expression() { return this.ternary(); }

  ternary() {
    const test = this.logical("||", () => this.logical("&&", () => this.equality()));
    if (!this.accept("?")) return test;
    const then = this.ternary();
    this.take(":");
    const otherwise = this.ternary();
    if (isNumeric(then.type) && isNumeric(otherwise.type)) {
      const type = promoted(then.type, otherwise.type);
      return { kind: "ternary", test, then: this.convert(then, type), otherwise: this.convert(otherwise, type), type };
    }
    return { kind: "ternary", test, then, otherwise, type: then.type };
  }

  logical(op, next) {
    let left = next();
    while (this.accept(op)) left = { kind: "logical", op, left, right: next(), type: "boolean" };
    return left;
  }

  equality() {
    let left = this.comparison();
    while (this.is("==") || this.is("!=")) {
      const { text: op } = this.token;
      this.at += 1;
      left = this.binary(op, left, this.comparison(), "boolean");
    }
    return left;
  }

  comparison() {
    let left = this.additive();
    while (["<", ">", "<=", ">="].includes(this.token.text)) {
      const { text: op } = this.token;
      this.at += 1;
      left = this.binary(op, left, this.additive(), "boolean");
    }
    return left;
  }

  additive() {
    let left = this.multiplicative();
    while (this.is("+") || this.is("-")) {
      const { text: op } = this.token;
      this.at += 1;
      left = this.binary(op, left, this.multiplicative());
    }
    return left;
  }

  multiplicative() {
    let left = this.unary();
    while (this.is("*") || this.is("/") || this.is("%")) {
      const { text: op } = this.token;
      this.at += 1;
      left = this.binary(op, left, this.unary());
    }
    return left;
  }

  binary(op, left, right, result) {
    if (!isNumeric(left.type) || !isNumeric(right.type)) this.fail(`${op} on ${left.type} and ${right.type} is not modelled`);
    const type = promoted(left.type, right.type);
    return { kind: "binary", op, left: this.convert(left, type), right: this.convert(right, type), type: result ?? type, operands: type };
  }

  unary() {
    if (this.is("++") || this.is("--")) this.fail("An increment is not modelled");
    if (this.accept("-")) {
      const value = this.unary();
      return { kind: "negate", value, type: value.type };
    }
    if (this.accept("!")) return { kind: "not", value: this.unary(), type: "boolean" };
    if (this.is("(") && PRIMITIVES.has(this.peek().text) && this.peek(2).text === ")") {
      this.at += 1;
      const type = this.token.text;
      this.at += 2;
      const value = this.unary();
      return { kind: "cast", to: type, value, type, implicit: false };
    }
    return this.postfix(this.primary());
  }

  primary() {
    const { token } = this;
    if (token.kind === "number") {
      this.at += 1;
      return this.literal(token.text);
    }
    if (this.accept("(")) {
      const value = this.expression();
      this.take(")");
      return value;
    }
    if (this.is("true") || this.is("false")) {
      this.at += 1;
      return { kind: "literal", value: token.text === "true", type: "boolean" };
    }
    if (this.accept("new")) {
      const type = this.type();
      const args = this.arguments();
      return { kind: "call", owner: type, name: "new", receiver: null, args, type };
    }
    if (token.kind !== "word") this.fail(`${JSON.stringify(token.text)} is not modelled`);
    if (this.peek().text === "->" || this.peek().text === "::") this.fail("A lambda is not modelled");
    this.at += 1;
    if (token.text === "this") return { kind: "this", type: this.signatures.this ?? "this" };
    const local = this.lookup(token.text);
    if (local !== undefined) return { kind: "name", name: token.text, type: local };
    if (/^[A-Z]/.test(token.text)) return { kind: "class", name: token.text, type: token.text };
    if (this.is("(")) return this.call({ kind: "this", type: this.signatures.this ?? "this" }, token.text);
    this.fail(`${token.text} is neither a local nor a field`, token);
    return null;
  }

  literal(text) {
    const last = text.at(-1);
    if (/[Ff]$/.test(text)) return { kind: "literal", value: Math.fround(Number(text.slice(0, -1))), type: "float" };
    if (/[Dd]$/.test(text)) return { kind: "literal", value: Number(text.slice(0, -1)), type: "double" };
    if (/[Ll]$/.test(last)) this.fail("A long literal is not modelled");
    return /[.eE]/.test(text) ? { kind: "literal", value: Number(text), type: "double" } : { kind: "literal", value: Number(text), type: "int" };
  }

  arguments() {
    this.take("(");
    const args = [];
    while (!this.is(")")) {
      args.push(this.expression());
      if (!this.accept(",")) break;
    }
    this.take(")");
    return args;
  }

  call(receiver, name) {
    const owner = receiver.type;
    const key = `${owner}.${name}`;
    const signature = this.signatures[key];
    if (signature === undefined) this.fail(`${key} has no admitted signature`);
    const args = this.arguments();
    return { kind: "call", owner, name, receiver: receiver.kind === "class" ? null : receiver, args, type: signature };
  }

  postfix(start) {
    let value = start;
    while (this.is(".")) {
      this.at += 1;
      const { text: name } = this.token;
      this.at += 1;
      if (this.is("(")) value = this.call(value, name);
      else {
        const type = this.signatures[`${value.type}#${name}`];
        if (type === undefined) this.fail(`${value.type}#${name} has no admitted type`);
        value = value.kind === "class" ? { kind: "static", owner: value.name, name, type } : { kind: "field", object: value, name, type };
      }
    }
    if (this.is("[")) this.fail("An array access is not modelled");
    if (this.is("++") || this.is("--")) this.fail("An increment is not modelled");
    return value;
  }
}

/** The IR of a method's source. Throws IrUnsupported with the line and column of what it does not model. */
function parseMethod(source, signatures) {
  return new Parser(source, signatures).method();
}

const numericCast = (value, to) => {
  if (to === "float") return Math.fround(value);
  if (to === "double") return value;
  if (to === "int") {
    if (Number.isNaN(value)) return 0;
    return Math.max(-2147483648, Math.min(2147483647, Math.trunc(value)));
  }
  throw new Error(`No cast to ${to}`);
};

function arithmetic(op, a, b, type) {
  let value;
  if (op === "+") value = a + b;
  else if (op === "-") value = a - b;
  else if (op === "*") value = a * b;
  else if (op === "/") value = a / b;
  else if (op === "%") value = a % b;
  else throw new Error(`No operation ${op}`);
  return type === "float" ? Math.fround(value) : type === "int" ? value | 0 : value;
}

function compare(op, a, b) {
  return { "<": a < b, ">": a > b, "<=": a <= b, ">=": a >= b, "==": a === b, "!=": a !== b }[op];
}

/**
 * Runs a method's IR. `env` holds `self` (the fields of `this`), `call(owner, name, receiver, args)` for each helper and
 * `statics`. Returns `{ value, trace }`, the trace each local, write, call and branch in the order they happened.
 */
function evaluate(method, args, env) {
  const trace = [];
  const locals = new Map(method.parameters.map(({ name }, index) => [name, args[index]]));
  const RETURN = Symbol("return");
  let returned;
  const expression = (node) => {
    switch (node.kind) {
      case "literal": return node.value;
      case "name": return locals.get(node.name);
      case "this": return env.self;
      case "static": return env.statics[`${node.owner}.${node.name}`];
      case "field": return expression(node.object)[node.name];
      case "cast": return numericCast(expression(node.value), node.to);
      case "negate": return -expression(node.value);
      case "not": return !expression(node.value);
      case "logical": {
        const left = expression(node.left);
        if (node.op === "&&" ? !left : left) return left;
        return expression(node.right);
      }
      case "ternary": return expression(node.test) ? expression(node.then) : expression(node.otherwise);
      case "binary": {
        const left = expression(node.left);
        const right = expression(node.right);
        return node.type === "boolean" ? compare(node.op, left, right) : arithmetic(node.op, left, right, node.operands);
      }
      case "call": {
        const receiver = node.receiver ? expression(node.receiver) : null;
        const values = node.args.map(expression);
        const value = env.call(node.owner, node.name, receiver, values);
        trace.push({ step: "call", name: `${node.owner}.${node.name}`, value });
        return value;
      }
      default: throw new Error(`No expression ${node.kind}`);
    }
  };
  const run = (statements) => {
    for (const statement of statements) {
      if (statement.kind === "local") {
        const value = expression(statement.value);
        locals.set(statement.name, value);
        trace.push({ step: "local", name: statement.name, value });
      } else if (statement.kind === "assign") {
        const value = expression(statement.value);
        if (statement.target.kind === "name") locals.set(statement.target.name, value);
        else expression(statement.target.object)[statement.target.name] = value;
        trace.push({ step: "write", name: statement.target.name, value });
      } else if (statement.kind === "call") {
        expression(statement.call);
      } else if (statement.kind === "block") {
        if (run(statement.body) === RETURN) return RETURN;
      } else if (statement.kind === "if") {
        const taken = expression(statement.test);
        trace.push({ step: "branch", value: Boolean(taken) });
        if (run(taken ? statement.then : statement.otherwise) === RETURN) return RETURN;
      } else if (statement.kind === "return") {
        returned = statement.value ? expression(statement.value) : undefined;
        trace.push({ step: "return", value: returned });
        return RETURN;
      }
    }
    return undefined;
  };
  run(method.body);
  return { value: returned, trace };
}

module.exports = { parseMethod, evaluate, IrUnsupported };
