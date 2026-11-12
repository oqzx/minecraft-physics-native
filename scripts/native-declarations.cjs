// Whether the three descriptions of the addon agree: what it exports at run time, the declarations and loader napi-rs
// generated from the Rust bindings (native/artifacts), and the hand-written NativePhysicsBinding (src/native.ts). The
// packed layouts are checked against the Rust modules they are read by. A compiler alone proves none of it: it reads the
// declarations, not the addon, and takes a method's arguments either way round.

const fs = require('node:fs');
const path = require('node:path');
const ts = require('typescript');
const { TARGETS, hostTarget } = require('./native-artifacts.cjs');

const root = path.resolve(__dirname, '..');
const artifacts = path.join(root, 'native', 'artifacts');
const CONTRACT = path.join(root, 'src', '__native_contract__.ts').replaceAll('\\', '/');

/** Members of an interface or class in a source text, in source order. */
function membersOf(text, name) {
    const source = ts.createSourceFile('source.ts', text, ts.ScriptTarget.Latest, true);
    const found = source.statements.find(statement => (ts.isInterfaceDeclaration(statement) || ts.isClassDeclaration(statement)) && statement.name?.text === name);
    if (!found) return null;
    return found.members.filter(member => member.name).map(member => member.name.getText(source));
}

function exportedNames(text) {
    const source = ts.createSourceFile('source.ts', text, ts.ScriptTarget.Latest, true);
    const exported = statement => statement.modifiers?.some(modifier => modifier.kind === ts.SyntaxKind.ExportKeyword);
    return source.statements
        .filter(statement => (ts.isFunctionDeclaration(statement) || ts.isClassDeclaration(statement)) && exported(statement))
        .map(statement => statement.name.text);
}

const difference = (left, right) => left.filter(name => !right.includes(name));

/** Each name one list has and the other does not, as a problem. */
function compareNames(problems, leftName, left, rightName, right) {
    for (const name of difference(left, right)) problems.push(`${name} is in ${leftName} and not in ${rightName}`);
    for (const name of difference(right, left)) problems.push(`${name} is in ${rightName} and not in ${leftName}`);
}

/**
 * Each member of the binding interface against the generated export of that name: anything a caller may pass by the
 * interface the export takes, and anything the export returns the interface promises, so the interface may narrow but
 * not widen, and the comparison runs one way. `Loose` takes out the two differences meant to be there, which the
 * generated text cannot say: a napi `Option` takes null as well as undefined, and a Rust `String` the interface lists the
 * values of is a string. Everything else, a field or an argument of another type or order, is refused.
 */
function contractSource(names, methods) {
    const lines = [
        "import type * as Generated from '../native/artifacts/native';",
        "import type { NativeBulkRun as BulkRun, NativePhysicsBinding as Binding, NativePlayer as Player, NativeWorld as World } from './native';",
        'type Fn = (...args: never[]) => unknown;',
        'type Loose<T> = T extends string ? string : T extends number | boolean | null | undefined | Fn | ArrayBufferView ? T',
        '    : T extends readonly (infer U)[] ? (number extends T["length"] ? Loose<U>[] : { [K in keyof T]: Loose<T[K]> })',
        '    : { [K in keyof T]: Loose<T[K]> | (undefined extends T[K] ? null : never) };',
        'type Takes<A extends Fn, B extends Fn> = Loose<Parameters<A>> extends Loose<Parameters<B>> ? true : false;',
        'type Gives<A extends Fn, B extends Fn> = Loose<ReturnType<A>> extends Loose<ReturnType<B>> ? true : false;',
        'type Methods<T> = { [K in keyof T]: T[K] extends Fn ? T[K] : never };',
        'function agree<_T extends true>(): void {}',
    ];
    const compare = (name, generated, declared) => {
        lines.push(`/* ${name} */ agree<Takes<${declared}, ${generated}>>();`);
        lines.push(`/* ${name} */ agree<Gives<${generated}, ${declared}>>();`);
    };
    for (const name of names) {
        if (!['NativeBulkRun', 'NativeWorld', 'NativePlayer'].includes(name)) compare(name, `(typeof Generated)['${name}']`, `NonNullable<Binding['${name}']>`);
    }
    for (const [name, generated, declared, members] of [
        ['NativeWorld', 'Generated.NativeWorld', 'World', methods.NativeWorld],
        ['NativePlayer', 'Generated.NativePlayer', 'Player', methods.NativePlayer],
        ['NativeBulkRun', 'Generated.NativeBulkRun', 'BulkRun', methods.NativeBulkRun],
    ]) {
        for (const member of members) compare(`${name}.${member}`, `Methods<${generated}>['${member}']`, `Methods<${declared}>['${member}']`);
    }
    for (const factory of methods.NativeBulkRunFactories) {
        // what a factory gives is the run, whose methods are compared above
        lines.push(`/* NativeBulkRun.${factory} */ agree<Takes<Binding['NativeBulkRun']['${factory}'], (typeof Generated.NativeBulkRun)['${factory}']>>();`);
    }
    lines.push('/* new NativePlayer */ agree<Loose<ConstructorParameters<Binding["NativePlayer"]>> extends Loose<ConstructorParameters<typeof Generated.NativePlayer>> ? true : false>();');
    return lines.map(line => `${line}\n`).join('');
}

/** The compiler's complaints about the contract, with `nativeText` standing in for src/native.ts. */
function checkContract(contract, nativeText) {
    const options = {
        strict: true, noEmit: true, skipLibCheck: true, target: ts.ScriptTarget.ES2019, module: ts.ModuleKind.CommonJS,
        moduleResolution: ts.ModuleResolutionKind.Node10, esModuleInterop: true, types: ['node'],
        typeRoots: [path.join(root, 'node_modules', '@types')], lib: ['lib.es2019.d.ts', 'lib.es2021.weakref.d.ts', 'lib.dom.d.ts'],
    };
    const host = ts.createCompilerHost(options);
    const nativePath = path.join(root, 'src', 'native.ts').replaceAll('\\', '/');
    const same = (a, b) => a.replaceAll('\\', '/').toLowerCase() === b.toLowerCase();
    const { getSourceFile, fileExists, readFile } = host;
    const stood = file => (same(file, CONTRACT) ? contract : same(file, nativePath) ? nativeText : undefined);
    host.getSourceFile = (file, language, ...rest) => {
        const text = stood(file);
        return text === undefined ? getSourceFile.call(host, file, language, ...rest) : ts.createSourceFile(file, text, language, true);
    };
    host.fileExists = file => same(file, CONTRACT) || fileExists.call(host, file);
    host.readFile = file => stood(file) ?? readFile.call(host, file);
    const program = ts.createProgram([CONTRACT], options, host);
    return ts.getPreEmitDiagnostics(program).map(diagnostic => {
        const position = diagnostic.file?.getLineAndCharacterOfPosition(diagnostic.start ?? 0);
        const line = position ? diagnostic.file.text.split('\n')[position.line].trim() : '';
        const member = /^\/\* (.+?) \*\//.exec(line)?.[1];
        // (the contract's line says all a member's disagreement can: the compiler says only that false is not true)
        if (member) return `${member}: NativePhysicsBinding and native.d.ts disagree on ${/Takes</.test(line) ? 'what it takes' : 'what it gives'}`;
        return `${diagnostic.file ? path.basename(diagnostic.file.fileName) : 'the contract'}: ${line}: ${ts.flattenDiagnosticMessageText(diagnostic.messageText, ' ')}`;
    });
}

/** `pub const NAME: <type> = <value>;` of a Rust module of napi_player.rs, a shift as its value. */
function rustConstants(text, moduleName) {
    const body = new RegExp(`pub mod ${moduleName} \\{([\\s\\S]*?)\\n\\}`).exec(text)?.[1];
    if (body === undefined) return null;
    const constants = {};
    for (const [, name, value] of body.matchAll(/pub const (\w+): \w+ = ([^;]+);/g)) {
        const shift = /^1 << (\d+)$/.exec(value.trim());
        constants[name] = shift ? 2 ** Number(shift[1]) : Number(value);
    }
    return constants;
}

/** The numbers of a `export const NAME = { ... } as const` of src/native.ts. */
function layoutConstants(text, name) {
    const body = new RegExp(`export const ${name} = \\{([\\s\\S]*?)\\n\\} as const`).exec(text)?.[1];
    if (body === undefined) return null;
    const constants = {};
    for (const [, key, value] of body.matchAll(/^\s+(\w+): ([^,\n]+),/gm)) {
        const shift = /^1 << (\d+)$/.exec(value.trim());
        constants[key] = shift ? 2 ** Number(shift[1]) : Number(value);
    }
    return constants;
}

function compareLayouts(problems, nativeText, rustText) {
    for (const [name, moduleName] of [
        ['NativePlayerInput', 'input'],
        ['NativePlayerInputFlag', 'input_flag'],
        ['NativePlayerOutput', 'output'],
        ['NativePlayerOutputFlag', 'output_flag'],
        ['NativePlayerEntityFlag', 'entity_flag'],
    ]) {
        const declared = layoutConstants(nativeText, name);
        const rust = rustConstants(rustText, moduleName);
        if (!declared || !rust) {
            problems.push(`${name} or the Rust module ${moduleName} was not found`);
            continue;
        }
        for (const key of new Set([...Object.keys(declared), ...Object.keys(rust)])) {
            if (declared[key] !== rust[key]) problems.push(`${name}.${key} is ${declared[key]}, ${moduleName}::${key} is ${rust[key]}`);
        }
    }
    const declared = /export const NATIVE_PLAYER_ENTITY_LENGTH = (\d+);/.exec(nativeText)?.[1];
    const rust = /pub const ENTITY_LENGTH: usize = (\d+);/.exec(rustText)?.[1];
    if (declared === undefined || declared !== rust) problems.push(`NATIVE_PLAYER_ENTITY_LENGTH is ${declared}, ENTITY_LENGTH is ${rust}`);
}

const methodsOf = constructor => Object.getOwnPropertyNames(constructor.prototype).filter(name => name !== 'constructor');

/**
 * Every disagreement among the addon, its loader, its declarations, the binding interface and the packed layouts.
 * `overrides` stands in for a file's text or a loaded module, for a test of what would be refused.
 */
function declarationProblems(overrides = {}) {
    const nativeText = overrides.native ?? fs.readFileSync(path.join(root, 'src', 'native.ts'), 'utf8');
    const declarations = overrides.declarations ?? fs.readFileSync(path.join(artifacts, 'native.d.ts'), 'utf8');
    const rustText = overrides.rust ?? fs.readFileSync(path.join(root, 'native', 'src', 'napi_player.rs'), 'utf8');
    // (the host's addon: a checkout can hold another platform's beside it)
    const addon = overrides.addon ?? require(path.join(artifacts, TARGETS[hostTarget()].outputs[0]));
    // (its text: the loader exports the whole addon as well, so an export it does not name still loads, but not by name from ESM)
    const loader = overrides.loader ?? fs.readFileSync(path.join(artifacts, 'native.js'), 'utf8');
    const problems = [];
    const runtime = Object.keys(addon);
    const declared = exportedNames(declarations);
    const bound = membersOf(nativeText, 'NativePhysicsBinding');
    compareNames(problems, 'the addon', runtime, 'its loader', [...loader.matchAll(/^module\.exports\.(\w+) = /gm)].map(match => match[1]));
    compareNames(problems, 'the addon', runtime, 'native.d.ts', declared);
    compareNames(problems, 'the addon', runtime, 'NativePhysicsBinding', bound);
    for (const name of ['NativeWorld', 'NativePlayer']) {
        const own = methodsOf(addon[name]);
        compareNames(problems, `the addon's ${name}`, own, `native.d.ts's ${name}`, difference(membersOf(declarations, name), ['constructor']));
        compareNames(problems, `the addon's ${name}`, own, `the ${name} interface`, membersOf(nativeText, name));
    }
    compareLayouts(problems, nativeText, rustText);
    const methods = {
        NativeWorld: methodsOf(addon.NativeWorld),
        NativePlayer: methodsOf(addon.NativePlayer),
        NativeBulkRun: methodsOf(addon.NativeBulkRun).filter(name => typeof Object.getOwnPropertyDescriptor(addon.NativeBulkRun.prototype, name)?.value === 'function'),
        NativeBulkRunFactories: Object.getOwnPropertyNames(addon.NativeBulkRun).filter(name => typeof addon.NativeBulkRun[name] === 'function'),
    };
    problems.push(...checkContract(contractSource(bound, methods), nativeText));
    return problems;
}

module.exports = { declarationProblems };
