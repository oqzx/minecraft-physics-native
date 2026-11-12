// A real consumer of a packed tarball: an empty project outside any workspace installs it with npm
// from the registry, with its own npm cache, and type-checks and runs the README's examples.
//   node scripts/consumer-check.cjs <tarball> --work <empty dir> --out <dir for logs and the report>
//
// Then what must fail is made to: a consumer without the peers, an addon removed, an addon of another
// platform, a declaration that changed. Nothing here may reach this checkout: the fixtures are
// written into the consumer, NODE_PATH and the napi overrides are cleared, and no parent directory of
// the work directory may hold a node_modules.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const { spawnSync } = require("node:child_process");
const { TARGETS, hostTarget } = require("./native-artifacts.cjs");
const { fileHash, packageProblems, recordsOf } = require("./build-provenance.cjs");

const windows = process.platform === "win32";
const PACKAGE = "minecraft-physics-native";

// the versions this contract was executed against: a new one is a new run of this check
const PEERS = { mineflayer: "4.39.0", "@nxg-org/mineflayer-physics-util": "1.9.10" };
const FIXTURE_DEPENDENCIES = { "prismarine-block": "1.23.0", "prismarine-registry": "1.12.0", vec3: "0.2.0" };
const TYPESCRIPT = "5.9.3";

// The README's examples, as written there.
const EXAMPLE_DIRECT = `import { LoadNativePhysics, nativeBackend } from '${PACKAGE}/dist/native';

const binding = LoadNativePhysics();
const world = new binding.NativeWorld();
world.setFullBlock(0, 0, 0, 1);
const state = binding.simulateVersionedTick('1.21.11', world, {
    position: { x: 0.5, y: 1.5, z: 0.5 }, velocity: { x: 0, y: -0.1, z: 0 },
    halfWidth: 0.3, height: 1.8, onGround: false, collidedHorizontally: false, collidedVertically: false,
}, {});
console.log(JSON.stringify({ backend: nativeBackend(binding), y: state.position.y }));
`;

const EXAMPLE_ENGINE = `import { NATIVE_PHYSICS_ENGINES, NativePhysics_1_20_4, SUPPORTED_VERSIONS, createBotPhysics, createNativePhysics } from '${PACKAGE}';
import type { Bot } from 'mineflayer';
import { EPhysicsCtx } from '@nxg-org/mineflayer-physics-util/dist/physics/settings/entityPhysicsCtx';

export function predict(bot: Bot): void {
    const setup = createBotPhysics(bot);
    const ctx = setup.context(EPhysicsCtx);
    setup.physics.simulate(ctx, bot.world);
    ctx.state.apply(bot);
    setup.dispose();
}

export function byRegistry(bot: Bot): string {
    return createNativePhysics(bot.registry).version;
}

export const direct = (bot: Bot): string => new NativePhysics_1_20_4(bot.registry).version;
export const everyRelease = SUPPORTED_VERSIONS.map(version => NATIVE_PHYSICS_ENGINES[version].version);
`;

// The packed player and a vehicle, from the installed declarations.
const EXAMPLE_CALLS = `import { LoadNativePhysics, NativePlayerInput as In, NativePlayerOutput as Out } from '${PACKAGE}/dist/native';

const binding = LoadNativePhysics();
const world = new binding.NativeWorld();
for (let x = -8; x < 8; x += 1) for (let z = -8; z < 8; z += 1) world.setFullBlock(x, 0, z, 1);
const player = new binding.NativePlayer('1.21.11', {
    position: { x: 0.5, y: 1, z: 0.5 }, velocity: { x: 0, y: 0, z: 0 }, halfWidth: 0.3, height: 1.8, onGround: true,
    collidedHorizontally: false, collidedVertically: false, isPlayer: true, movementSpeed: 0.1,
}, {});
const input = new Float64Array(In.LENGTH);
input.set([0.5, 1, 0.5], In.POSITION);
input[In.FORWARD] = 1;
input[In.HALF_WIDTH] = 0.3;
input[In.HEIGHT] = 1.8;
input[In.EYE_HEIGHT] = 1.62;
const output = player.tick(world, input, new Float64Array());
const boat = binding.simulateVersionedBoatTick('1.20.4', world, {
    position: { x: 0.5, y: 5, z: 0.5 }, velocity: { x: 0, y: 0, z: 0 }, yaw: 0, forward: true, left: false, right: true, inWater: true,
});
console.log(JSON.stringify({ length: output.length, moved: output[Out.POSITION + 2] !== 0.5, boatYaw: boat.yaw }));
`;

// A bot as mineflayer's is, over the consumer's own prismarine (test/bot-fixture.cjs's shape).
const CONSUMER_RUN = String.raw`
const assert = require('node:assert/strict');
const { EventEmitter } = require('node:events');
const { Vec3 } = require('vec3');
const physics = require('${PACKAGE}');
const { EPhysicsCtx } = require('@nxg-org/mineflayer-physics-util/dist/physics/settings/entityPhysicsCtx');
const compiled = './out/engine.js';
const { predict, byRegistry, everyRelease } = require(compiled);

function botOf(version) {
    const registry = require('prismarine-registry')(version);
    const Block = require('prismarine-block')(registry);
    const floor = registry.blocksByName.stone.defaultState ?? registry.blocksByName.stone.id << 4;
    const air = registry.blocksByName.air.defaultState ?? 0;
    const stateAt = pos => (Math.floor(pos.y) === 63 ? floor : air);
    const world = Object.assign(new EventEmitter(), {
        getBlock: pos => Object.assign(Block.fromStateId(stateAt(pos), 0), { position: new Vec3(Math.floor(pos.x), Math.floor(pos.y), Math.floor(pos.z)) }),
        getBlockStateId: stateAt,
    });
    const bot = new EventEmitter();
    return Object.assign(bot, {
        version, registry, world,
        game: { dimension: 'overworld', gameMode: 'survival', minY: 0, height: 256 },
        entity: {
            position: new Vec3(0.5, 64, 0.5), velocity: new Vec3(0, 0, 0), onGround: true, yaw: 0, pitch: 0,
            height: 1.8, width: 0.6, effects: {}, attributes: {}, metadata: [], equipment: [], type: 'player', username: 'bot',
        },
        inventory: { slots: [] }, heldItem: null, fireworkRocketDuration: 0,
        controlState: { forward: true, back: false, left: false, right: false, jump: false, sprint: false, sneak: false },
        getControlState(name) { return bot.controlState[name]; },
        setControlState(name, value) { bot.controlState[name] = value; },
        getEquipmentDestSlot: () => 0,
        usingHeldItem: false,
    });
}

const out = { releases: {}, classes: everyRelease.length };
assert.deepEqual(everyRelease, [...physics.SUPPORTED_VERSIONS]);
const binding = physics.LoadNativePhysics();
for (const version of physics.SUPPORTED_VERSIONS) {
    assert.equal(binding.isSupportedVersion(version), true, version);
    assert.ok(binding.generatedBlockStateCount(version, 'stone') > 0, version);
}
// what this consumer's minecraft-data has; 26.x needs a data package that holds them
for (const version of ['1.8.8', '1.12.2', '1.13.2', '1.16.5', '1.20.4', '1.20.5', '1.21.4', '1.21.11']) {
    const bot = botOf(version);
    assert.equal(byRegistry(bot), version);
    // the registry, the adapter and the engine name one release, an explicit one included
    const setup = physics.createBotPhysics(bot, { version });
    assert.equal(setup.physics.version, version);
    const other = version === '1.20.4' ? '1.20.5' : '1.20.4';
    assert.throws(() => physics.createNativePhysics(bot.registry, { version: other }), /cannot serve Minecraft/, version);
    const ctx = setup.context(EPhysicsCtx);
    ctx.state = ctx.state.clone();
    for (let i = 0; i < 5; i += 1) setup.physics.simulate(ctx, bot.world);
    assert.equal(bot.entity.position.z, 0.5, 'a prediction leaves the bot alone');
    ctx.state.apply(bot);
    setup.dispose();
    predict(bot);
    assert.equal(bot.eventNames().length, 0, version);
    out.releases[version] = { z: bot.entity.position.z, x: bot.entity.position.x, y: bot.entity.position.y };
    assert.ok(bot.entity.position.z !== 0.5 || bot.entity.position.x !== 0.5, version + ' moved');
}
out.backend = physics.nativeBackend(binding);
out.resolved = require.resolve('${PACKAGE}');
process.stdout.write(JSON.stringify(out));
`;

const ESM_RUN = `import physics from '${PACKAGE}';
const binding = physics.LoadNativePhysics();
process.stdout.write(JSON.stringify({ esm: physics.nativeBackend(binding).target, versions: physics.SUPPORTED_VERSIONS.length }));
`;

const TSCONFIG = (files, extra = {}) => JSON.stringify({
  compilerOptions: { target: "es2021", module: "commonjs", moduleResolution: "node", strict: true, esModuleInterop: true, skipLibCheck: false, types: ["node"], outDir: "out", ...extra },
  files,
}, null, 2);

function option(name) {
  const index = process.argv.indexOf(`--${name}`);
  assert.ok(index >= 0, `--${name} is required`);
  return path.resolve(process.argv[index + 1]);
}

function cleanEnvironment(cache) {
  const environment = { ...process.env, npm_config_cache: cache, npm_config_audit: "false", npm_config_fund: "false", npm_config_update_notifier: "false" };
  for (const name of Object.keys(environment)) {
    if (name === "NODE_PATH" || name.startsWith("NAPI_RS_") || /^(?:npm_(?:package|lifecycle|execpath|node_execpath|command)|PNPM_)/i.test(name)) delete environment[name];
  }
  return environment;
}

function main() {
  const tarball = path.resolve(process.argv[2]);
  const work = option("work");
  const out = option("out");
  assert.ok(fs.existsSync(tarball), tarball);
  fs.mkdirSync(out, { recursive: true });
  fs.mkdirSync(work, { recursive: true });
  assert.deepEqual(fs.readdirSync(work), [], `${work} is not empty`);
  for (let directory = path.dirname(work); ; directory = path.dirname(directory)) {
    assert.equal(fs.existsSync(path.join(directory, "node_modules")), false, `${directory} holds a node_modules a consumer would find`);
    if (directory === path.dirname(directory)) break;
  }
  const environment = cleanEnvironment(path.join(work, "npm-cache"));
  const log = (name, result) => {
    fs.writeFileSync(path.join(out, `${name}.log`), `exit ${result.status}\n--- stdout\n${result.stdout}\n--- stderr\n${result.stderr}\n`);
    return result;
  };
  const run = (name, cwd, command, args, overrides = {}) => log(name, spawnSync(command, args, {
    cwd, encoding: "utf8", shell: windows && !command.endsWith("node.exe"), env: { ...environment, ...overrides }, maxBuffer: 2 ** 28,
  }));
  const npm = windows ? "npm.cmd" : "npm";
  const node = process.execPath;
  const ok = (result, what) => assert.equal(result.status, 0, `${what}\n${result.stdout}\n${result.stderr}`);

  const project = (name, dependencies, files) => {
    const directory = path.join(work, name);
    fs.mkdirSync(directory);
    fs.writeFileSync(path.join(directory, "package.json"), JSON.stringify({ name: `consumer-${name}`, private: true, dependencies }, null, 2));
    for (const [file, text] of Object.entries(files)) fs.writeFileSync(path.join(directory, file), text);
    ok(run(`${name}-install`, directory, npm, ["install", "--no-package-lock"]), `npm install of ${name}`);
    return directory;
  };
  const thirdParty = {};
  // skipLibCheck stays off, so that it cannot hide a module this package's declarations miss. That
  // shows the declarations of the peers' own dependencies, some of which do not compile: those are
  // counted and reported, and only those may fail. An error in the consumer or in this package fails.
  const tsc = (directory, name, config) => {
    fs.writeFileSync(path.join(directory, "tsconfig.json"), config);
    const result = run(name, directory, node, [path.join(directory, "node_modules", "typescript", "bin", "tsc"), "-p", "."]);
    const errors = [...result.stdout.matchAll(/^(\S+?)\(\d+,\d+\): error (TS\d+)/gm)].map((match) => ({ file: match[1].replaceAll("\\", "/"), code: match[2] }));
    const foreign = errors.filter(({ file }) => file.startsWith("node_modules/") && !file.startsWith(`node_modules/${PACKAGE}/`));
    for (const { file } of foreign) {
      const owner = file.split("/").slice(1, file.startsWith("node_modules/@") ? 3 : 2).join("/");
      thirdParty[owner] = (thirdParty[owner] ?? 0) + 1;
    }
    return { ...result, status: errors.length > foreign.length ? 1 : result.status === 0 || errors.length > 0 ? 0 : result.status };
  };
  const types = { typescript: TYPESCRIPT, "@types/node": "22.20.1" };
  const dependency = { [PACKAGE]: `file:${tarball.replaceAll("\\", "/")}` };

  // the full consumer: the documented peers, what the fixture needs, and nothing of the workspace
  const full = project("full", { ...dependency, ...PEERS, ...FIXTURE_DEPENDENCIES, ...types }, {
    "direct.ts": EXAMPLE_DIRECT, "engine.ts": EXAMPLE_ENGINE, "calls.ts": EXAMPLE_CALLS, "run.cjs": CONSUMER_RUN, "esm.mjs": ESM_RUN,
  });
  ok(tsc(full, "full-tsc", TSCONFIG(["direct.ts", "engine.ts", "calls.ts"])), "the README's examples type-check with skipLibCheck off");
  const calls = run("full-calls", full, node, ["out/calls.js"]);
  ok(calls, "the packed player and a vehicle run");
  const callResult = JSON.parse(calls.stdout);
  assert.equal(callResult.moved, true, "the packed player walked");
  assert.ok(callResult.boatYaw > 0, "the boat turned");
  const direct = run("full-direct", full, node, ["out/direct.js"]);
  ok(direct, "the direct example runs");
  const directResult = JSON.parse(direct.stdout);
  assert.ok(directResult.y < 1.5, "a direct tick falls");
  const adapter = run("full-adapter", full, node, ["run.cjs"]);
  ok(adapter, "the adapter and FROM_BOT run");
  const adapterResult = JSON.parse(adapter.stdout);
  const esm = run("full-esm", full, node, ["esm.mjs"]);
  ok(esm, "the ES module import runs");

  const installed = path.join(full, "node_modules", PACKAGE);
  assert.equal(path.relative(installed, adapterResult.resolved).startsWith(".."), false, "the package came from elsewhere");
  const installProblems = packageProblems(installed);
  assert.deepEqual(installProblems, [], "the installed package is not the recorded build");
  const host = hostTarget();
  const record = recordsOf(installed).find((entry) => entry.record.target === host)?.record;
  assert.ok(record, `the tarball has no build of ${host}`);
  assert.equal(adapterResult.backend.target, host, "another backend answered");
  assert.equal(adapterResult.backend.backend, "native");
  assert.equal(adapterResult.backend.sourceHash, record.inputs.sourceHash, "the loaded binary is not the recorded one");

  // what must fail
  const failures = {};
  const expectFailure = (name, result, pattern) => {
    assert.notEqual(result.status, 0, `${name} succeeded`);
    const text = `${result.stdout}\n${result.stderr}`;
    assert.match(text, pattern, name);
    failures[name] = pattern.exec(text)[0];
  };

  // no peers: the low-level entry still compiles and runs, the adapter's types say what is missing
  const bare = project("bare", { ...dependency, ...types }, { "direct.ts": EXAMPLE_DIRECT, "adapter.ts": `import { createBotPhysics } from '${PACKAGE}';\nexport { createBotPhysics };\n` });
  ok(tsc(bare, "bare-tsc-direct", TSCONFIG(["direct.ts"])), "the low-level entry needs no peer");
  ok(run("bare-direct", bare, node, ["out/direct.js"]), "the low-level entry runs without peers");
  expectFailure("types without the peers", tsc(bare, "bare-tsc-adapter", TSCONFIG(["adapter.ts"])), /Cannot find module '(?:mineflayer|@nxg-org\/mineflayer-physics-util[^']*)'/);

  const addonName = TARGETS[host].outputs[0];
  const addon = path.join(installed, "native", "artifacts", addonName);
  const load = (name) => run(name, full, node, ["out/direct.js"]);
  const genuine = fs.readFileSync(addon);

  fs.rmSync(addon);
  expectFailure("addon removed", load("addon-removed"), /Cannot find native binding|did not load, and WebAssembly was not asked for|unavailable at/);
  assert.match(packageProblems(installed).join("\n"), /is missing/, "provenance misses the removed addon");

  // another platform's binary under this platform's name
  fs.writeFileSync(addon, windows ? Buffer.concat([Buffer.from([0x7f, 0x45, 0x4c, 0x46, 2, 1, 1]), Buffer.alloc(4096)]) : Buffer.concat([Buffer.from("MZ"), Buffer.alloc(4096)]));
  expectFailure("addon of another platform", load("addon-foreign"), /Cannot find native binding|did not load, and WebAssembly was not asked for|unavailable at/);
  const foreign = packageProblems(installed).join("\n");
  assert.match(foreign, /is not the file|no native binary/, "provenance misses a foreign addon");
  fs.writeFileSync(addon, genuine);

  const declarations = path.join(installed, "native", "artifacts", "native.d.ts");
  const text = fs.readFileSync(declarations);
  fs.writeFileSync(declarations, Buffer.concat([text, Buffer.from("export declare function retired(): void\n")]));
  assert.match(packageProblems(installed).join("\n"), /native\.d\.ts is not the file/, "provenance misses a stale declaration");
  fs.writeFileSync(declarations, text);
  assert.deepEqual(packageProblems(installed), [], "the package restored is the build again");

  const listing = run("full-ls", full, npm, ["ls", "--all", "--json"]);
  const report = {
    tarball: path.basename(tarball),
    sha256: fileHash(tarball),
    host: { platform: process.platform, arch: process.arch, node: process.version },
    backend: adapterResult.backend,
    direct: directResult,
    calls: callResult,
    adapter: adapterResult.releases,
    classes: adapterResult.classes,
    esm: JSON.parse(esm.stdout),
    peers: PEERS,
    typescript: TYPESCRIPT,
    thirdPartyDeclarationErrors: thirdParty,
    installed: Object.fromEntries(Object.entries(JSON.parse(listing.stdout).dependencies).map(([name, entry]) => [name, entry.version])),
    expectedFailures: failures,
  };
  fs.writeFileSync(path.join(out, "consumer.report.json"), `${JSON.stringify(report, null, 2)}\n`);
  console.log(JSON.stringify({ backend: report.backend, classes: report.classes, adapter: Object.keys(report.adapter), failures: Object.keys(failures) }, null, 2));
}

if (require.main === module) main();
