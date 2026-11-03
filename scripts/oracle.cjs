#!/usr/bin/env node
"use strict";

const fs = require("node:fs");
const path = require("node:path");
const os = require("node:os");
const { spawn } = require("node:child_process");
const { once } = require("node:events");

const DEFAULT_VERSION = "1.20.6";
const TRACE_VERSION = process.env.ORACLE_VERSION || DEFAULT_VERSION;
const VANILLA_TRACE_SOURCE = "Mineflayer BotcraftPhysics client-side prediction";

function assert(condition, message) {
  if (!condition) {
    throw new Error(message);
  }
}

function loadNativeBinding() {
  const candidates = [
    path.resolve(__dirname, "..", "dist", "native.js"),
    path.resolve(__dirname, "..", "native", "artifacts", "native.js"),
  ];
  for (const candidate of candidates) {
    if (fs.existsSync(candidate)) {
      return require(candidate).LoadNativePhysics();
    }
  }
  throw new Error("No native physics binding was found. Build the project native module first.");
}

function seededScenario(scenarioName, seed) {
  const value = Number(seed) || 1337;
  const offset = (value % 11) - 5;
  return {
    name: scenarioName,
    start: {
      x: 0.5 + (offset * 0.125),
      y: 67 + ((value % 7) * 0.1),
      z: 0.5 + (((value >> 1) % 9) - 4) * 0.125,
      yaw: ((value % 37) - 18) * (Math.PI / 180),
    },
  };
}

function seededScenario(scenarioName, seed) {
  const value = Number(seed) || 1337;
  const offset = (value % 11) - 5;
  return {
    name: scenarioName,
    start: {
      x: 0.5 + (offset * 0.125),
      y: 67 + ((value % 7) * 0.1),
      z: 0.5 + (((value >> 1) % 9) - 4) * 0.125,
      yaw: ((value % 37) - 18) * (Math.PI / 180),
    },
  };
}

function generateRandomScenario(seed, version) {
  const base = Number(seed) || 1337;
  const issues = [
    "sprint_jump",
    "sneak_edge",
    "water_drag",
    "lava_push",
    "bubble_column",
    "slime_bounce",
    "cobweb_stick",
    "powder_snow",
    "boat_riding",
    "elytra_glide",
  ];
  const selected = issues[base % issues.length];
  return {
    name: `${selected}_${base}`,
    version,
    seed: base,
    start: {
      x: 0.5,
      y: 67,
      z: 0.5,
      yaw: 0,
    },
    commands: [
      { tick: 1, forward: true },
      { tick: 4, jump: true },
      { tick: 16, sprint: true },
      { tick: 21, sneak: true },
    ],
  };
}

function parseArgs(argv) {
  const result = {
    version: TRACE_VERSION,
    scenario: "flat-forward-jump",
    ticks: 50,
    seed: 1337,
    debug: false,
    help: false,
    random: false,
    minimize: false,
    replay: null,
    deep: false,
  };

  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === "--version") result.version = argv[index + 1];
    else if (arg === "--scenario") result.scenario = argv[index + 1];
    else if (arg === "--ticks") result.ticks = Number(argv[index + 1]);
    else if (arg === "--seed") result.seed = Number(argv[index + 1]);
    else if (arg === "--debug") result.debug = true;
    else if (arg === "--deep") result.deep = true;
    else if (arg === "--random") result.random = true;
    else if (arg === "--minimize") result.minimize = true;
    else if (arg === "--replay") result.replay = argv[index + 1];
    else if (arg === "--help" || arg === "-h") result.help = true;
    else if (arg.startsWith("--")) {
      throw new Error(`Unknown option: ${arg}`);
    }
  }

  if (!result.version) throw new Error("A --version is required.");
  if (!Number.isFinite(result.ticks) || result.ticks <= 0) throw new Error("--ticks must be a positive integer.");
  return result;
}

function usage() {
  console.log([
    "Usage: node scripts/oracle.cjs [--version 1.20.6] [--scenario flat-forward-jump] [--ticks 50] [--seed 1337] [--debug] [--random] [--minimize]",
    "",
    "This prototype runs the real vanilla server, but its movement trace comes from Mineflayer BotcraftPhysics client-side prediction.",
    "It is exploratory only and cannot certify vanilla parity; results are labeled candidate-divergence or inconclusive.",
    "",
    "Scenarios:",
    "  flat-forward-jump  deterministic flat-world forward and jump movement",
    "  --random           generate a reproducible seeded scenario from a version-specific operator set",
    "  --minimize         preserve a minimal failing trace artifact for later replay",
    "  --replay <path>    replay a saved failure artifact and print the first divergence",
    "  --deep            emit the full preceding state and mismatch payload",
  ].join("\n"));
}

async function fetchJson(url) {
  const response = await fetch(url);
  if (!response.ok) throw new Error(`Failed to fetch ${url}: ${response.status} ${response.statusText}`);
  return response.json();
}

async function downloadFile(url, target) {
  const response = await fetch(url);
  if (!response.ok) throw new Error(`Failed to download ${url}: ${response.status} ${response.statusText}`);
  const bytes = new Uint8Array(await response.arrayBuffer());
  fs.writeFileSync(target, Buffer.from(bytes));
}

async function ensureVanillaServerJar(version, cacheDir) {
  const manifestUrl = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
  const manifest = await fetchJson(manifestUrl);
  const versionInfo = manifest.versions.find((entry) => entry.id === version);
  if (!versionInfo) {
    throw new Error(`Version ${version} is not available in the Mojang manifest.`);
  }
  const details = await fetchJson(versionInfo.url);
  const serverUrl = details.downloads?.server?.url;
  if (!serverUrl) throw new Error(`No vanilla server jar was found for ${version}.`);
  fs.mkdirSync(cacheDir, { recursive: true });
  const jarPath = path.join(cacheDir, `minecraft_server.${version}.jar`);
  if (!fs.existsSync(jarPath)) {
    await downloadFile(serverUrl, jarPath);
  }
  return jarPath;
}

async function waitForServerReady(child, timeoutMs = 60000) {
  const start = Date.now();
  return new Promise((resolve, reject) => {
    const listener = (chunk) => {
      const text = chunk.toString("utf8");
      if (text.includes("Done") || text.includes("For help, type \"/help\"")) {
        cleanup();
        resolve();
      }
      if (Date.now() - start > timeoutMs) {
        cleanup();
        reject(new Error("Timed out while waiting for the vanilla server to start."));
      }
    };
    const cleanup = () => {
      child.stdout.off("data", listener);
      child.stderr.off("data", listener);
    };
    child.stdout.on("data", listener);
    child.stderr.on("data", listener);
  });
}

function writeServerProperties(dir, version) {
  const props = [
    "enable-command-block=false",
    "allow-flight=true",
    "difficulty=peaceful",
    "gamemode=creative",
    "level-name=oracle",
    "level-seed=oracle-seed",
    "level-type=flat",
    "generator-settings={\"biome\":\"plains\",\"layers\":[{\"block\":\"stone\",\"height\":1}],\"structures\":{\"structures\":{}}}",
    "max-tick-time=60000",
    "online-mode=false",
    "pvp=false",
    "server-port=25565",
    "spawn-protection=0",
    "view-distance=3",
    "simulation-distance=3",
    "motd=oracle",
    "white-list=false",
    "force-gamemode=true",
    "spawn-monsters=false",
    "spawn-animals=false",
    `rcon.password=oracle-${version}`,
    "sync-chunk-writes=true",
    "entity-broadcast-range-percentage=100",
    "log-ips=false",
  ].join("\n") + "\n";
  fs.writeFileSync(path.join(dir, "server.properties"), props, "utf8");
  fs.writeFileSync(path.join(dir, "eula.txt"), "eula=true\n", "utf8");
}

async function startVanillaServer(version, workDir) {
  const jarPath = await ensureVanillaServerJar(version, path.join(workDir, "vanilla-cache"));
  const serverDir = path.join(workDir, "server");
  fs.mkdirSync(serverDir, { recursive: true });
  writeServerProperties(serverDir, version);

  const javaBinary = process.env.JAVA_HOME
    ? path.join(process.env.JAVA_HOME, "bin", process.platform === "win32" ? "java.exe" : "java")
    : (process.platform === "win32" ? "java.exe" : "java");

  const child = spawn(javaBinary, ["-Xmx768M", "-Xms512M", "-jar", jarPath, "nogui"], {
    cwd: serverDir,
    stdio: ["pipe", "pipe", "pipe"],
    env: { ...process.env },
  });

  await waitForServerReady(child);

  const sendConsole = (line) => {
    if (child.stdin && !child.stdin.destroyed) {
      child.stdin.write(`${line}\n`);
    }
  };

  sendConsole("op oracle-bot");
  sendConsole("difficulty peaceful");
  sendConsole("time set day");
  sendConsole("weather clear");
  sendConsole("gamerule doDaylightCycle false");
  sendConsole("gamerule doWeatherCycle false");

  return { child, serverDir };
}

async function connectMineflayer(version) {
  const { createBot } = await import("mineflayer");
  const bot = createBot({
    host: "127.0.0.1",
    port: 25565,
    username: "oracle-bot",
    version,
    auth: "offline",
    skipValidation: true,
    viewDistance: "short",
  });

  await once(bot, "spawn");
  return bot;
}

function nativeWorldForScenario(version, scenario, start) {
  const world = new (loadNativeBinding().NativeWorld)();
  const floorY = Math.floor(start.y) - 1;
  const floorMinX = Math.floor(start.x) - 8;
  const floorMaxX = Math.floor(start.x) + 8;
  const floorMinZ = Math.floor(start.z) - 8;
  const floorMaxZ = Math.floor(start.z) + 8;

  for (let x = floorMinX; x <= floorMaxX; x += 1) {
    for (let z = floorMinZ; z <= floorMaxZ; z += 1) {
      world.setFullBlock(x, floorY, z, 1);
      for (let y = floorY - 1; y >= 0; y -= 1) {
        world.setFullBlock(x, y, z, 0);
      }
    }
  }

  world.setWorldBounds(-64, 320, 0, 0, 100000);

  return { world, start, floorY };
}

function nativeStateFromBot(bot, version) {
  const { entity } = bot;
  return {
    position: { x: entity.position.x, y: entity.position.y, z: entity.position.z },
    velocity: { x: entity.velocity.x, y: entity.velocity.y, z: entity.velocity.z },
    yaw: entity.yaw,
    pitch: entity.pitch,
    halfWidth: 0.3,
    height: 1.8,
    onGround: entity.onGround ?? false,
    collidedHorizontally: false,
    collidedVertically: false,
    input: { x: 0, y: 0, z: 0 },
    jumpRequested: false,
    sprinting: bot.getControlState("sprint") ?? false,
    sneaking: bot.getControlState("sneak") ?? false,
    swimming: entity.isInWater ?? false,
    flying: false,
    inWater: entity.isInWater ?? false,
    inLava: entity.isInLava ?? false,
    eyeHeight: 1.62,
    movementSpeed: 0.1,
    gravity: 0.08,
    jumpStrength: 0.42,
    stepHeight: 0.6,
    fallDistance: entity.fallDistance ?? 0,
    noPhysics: false,
    yawDegrees: entity.yaw * 180 / Math.PI,
    noGravity: false,
    tickCount: 0,
    isPlayer: true,
    firstTick: true,
  };
}

function normaliseFloat(value, epsilon = 1e-6) {
  if (Math.abs(value) < epsilon) return 0;
  return value;
}

function snapshotBot(bot) {
  const { entity } = bot;
  return {
    tick: undefined,
    position: {
      x: Number(normaliseFloat(entity.position.x)),
      y: Number(normaliseFloat(entity.position.y)),
      z: Number(normaliseFloat(entity.position.z)),
    },
    velocity: {
      x: Number(normaliseFloat(entity.velocity.x)),
      y: Number(normaliseFloat(entity.velocity.y)),
      z: Number(normaliseFloat(entity.velocity.z)),
    },
    yaw: Number(normaliseFloat(entity.yaw)),
    pitch: Number(normaliseFloat(entity.pitch)),
    onGround: !!entity.onGround,
    inWater: !!entity.isInWater,
    inLava: !!entity.isInLava,
    isSwimming: !!entity.isInWater,
    fallDistance: Number(normaliseFloat(entity.fallDistance ?? 0)),
  };
}

function comparablePhysicsState(state) {
  return {
    position: state.position,
    velocity: state.velocity,
    yaw: state.yaw,
    pitch: state.pitch,
    onGround: state.onGround,
    inWater: state.inWater,
    inLava: state.inLava,
    fallDistance: state.fallDistance ?? 0,
  };
}

function compareTickSnapshots(expected, actual) {
  const differences = [];
  const inspect = (left, right, prefix) => {
    if (left === null || right === null || typeof left !== "object" || typeof right !== "object") {
      if (typeof left === "number" && typeof right === "number") {
        const delta = Math.abs(left - right);
        if (delta > 1e-6) differences.push({ field: prefix, expected: left, actual: right, delta });
      } else if (left !== right) {
        differences.push({ field: prefix, expected: left, actual: right });
      }
      return;
    }
    const keys = new Set([...Object.keys(left), ...Object.keys(right)]);
    for (const key of keys) {
      if (key in left || key in right) {
        inspect(left[key], right[key], `${prefix}.${key}`);
      }
    }
  };

  inspect(expected, actual, "state");
  return differences;
}

function persistArtifacts(runMetadata, vanillaTrace, nativeTrace, directory) {
  fs.mkdirSync(directory, { recursive: true });
  fs.writeFileSync(path.join(directory, "metadata.json"), JSON.stringify(runMetadata, null, 2), "utf8");
  fs.writeFileSync(path.join(directory, "vanilla-trace.json"), JSON.stringify({ observed: vanillaTrace }, null, 2), "utf8");
  fs.writeFileSync(path.join(directory, "native-trace.json"), JSON.stringify({ observed: nativeTrace }, null, 2), "utf8");
}

async function runScenario({ version, scenario, ticks, seed, debug }) {
  const workDir = path.join(os.tmpdir(), "minecraft-physics-oracle", version.replace(/\./g, "-"), String(seed));
  const artifactDir = path.join(process.cwd(), "oracle-runs", `${version.replace(/\./g, "-")}-${scenario.name}-${seed}`);
  const { child } = await startVanillaServer(version, workDir);
  let bot;

  try {
    bot = await connectMineflayer(version);
    const binding = loadNativeBinding();
    const vanillaStart = {
      x: bot.entity.position.x,
      y: bot.entity.position.y,
      z: bot.entity.position.z,
    };
    const nativeWorld = nativeWorldForScenario(version, scenario, vanillaStart);
    const nativeScenarioState = nativeStateFromBot(bot, version);
    const controlAxis = (positive, negative) =>
      Number(bot.getControlState(positive)) - Number(bot.getControlState(negative));
    const syncNativeInput = () => {
      nativeScenarioState.input = {
        x: controlAxis("left", "right"),
        y: 0,
        z: controlAxis("forward", "back"),
      };
      nativeScenarioState.jumpRequested = bot.getControlState("jump");
      nativeScenarioState.sprinting = bot.getControlState("sprint");
      nativeScenarioState.sneaking = bot.getControlState("sneak");
    };

    const nativeTrace = []; 
    const vanillaTrace = [];

    const applyInput = (tick) => {
      const forward = tick >= 1 && tick <= 22;
      const jump = tick === 5 || tick === 6 || tick === 7;
      bot.setControlState("forward", forward);
      bot.setControlState("jump", jump);
      if (debug) {
        console.log(`[native] tick ${tick}: forward=${forward}, jump=${jump}`);
      }
    };

    for (let tick = 0; tick < ticks; tick += 1) {
      applyInput(tick);
      syncNativeInput();

      const vanillaBefore = snapshotBot(bot);
      await new Promise((resolve) => {
        const onPhysics = () => {
          bot.off("physicsTick", onPhysics);
          resolve();
        };
        bot.on("physicsTick", onPhysics);
      });

      const vanillaAfter = snapshotBot(bot);
      const nativeState = binding.simulateVersionedTick(
        version,
        nativeWorld.world,
        nativeScenarioState,
        {}
      );

      nativeScenarioState.position = nativeState.position;
      nativeScenarioState.velocity = nativeState.velocity;
      nativeScenarioState.onGround = nativeState.onGround;
      nativeScenarioState.fallDistance = nativeState.fallDistance ?? 0;
      nativeScenarioState.tickCount = tick + 1;

      vanillaTrace.push({ tick, state: vanillaAfter });
      nativeTrace.push({ tick, state: { ...nativeState, tick } });

      const differences = compareTickSnapshots(
        comparablePhysicsState(vanillaAfter),
        comparablePhysicsState(nativeState),
      );
      if (differences.length > 0) {
        const failure = {
          version,
          scenario: scenario.name,
          tick,
          vanilla: vanillaAfter,
          native: nativeState,
          differences,
          precedingVanilla: vanillaBefore,
        };
        const metadata = {
          version,
          scenario: scenario.name,
          seed,
          tick,
          vanillaTraceSource: VANILLA_TRACE_SOURCE,
          conformanceStatus: "inconclusive",
          observedAt: new Date().toISOString(),
          firstDivergence: failure,
        };
        persistArtifacts(metadata, vanillaTrace, nativeTrace, artifactDir);
        console.log(JSON.stringify({
          status: "candidate-divergence",
          vanillaTraceSource: VANILLA_TRACE_SOURCE,
          conformanceStatus: "inconclusive",
          failure,
        }, null, 2));
        process.exitCode = 2;
        return { failure, vanillaTrace, nativeTrace };
      }
    }

    const metadata = {
      version,
      scenario: scenario.name,
      seed,
      ticks,
      vanillaTraceSource: VANILLA_TRACE_SOURCE,
      conformanceStatus: "inconclusive",
      observedAt: new Date().toISOString(),
      status: "inconclusive-match",
    };
    persistArtifacts(metadata, vanillaTrace, nativeTrace, artifactDir);
    console.log(JSON.stringify({ status: "inconclusive-match", version, scenario: scenario.name, ticks, vanillaTraceSource: VANILLA_TRACE_SOURCE, nativeTraceCount: nativeTrace.length, vanillaTraceCount: vanillaTrace.length }, null, 2));
    return { inconclusive: true, vanillaTrace, nativeTrace };
  } finally {
    if (bot?._client && !bot._client.ended) {
      const botEnded = new Promise((resolve) => bot.once("end", resolve));
      bot.quit("oracle complete");
      await Promise.race([botEnded, new Promise((resolve) => setTimeout(resolve, 1000))]);
    }
    if (child && child.stdin) child.stdin.write("stop\n");
    child?.kill("SIGTERM");
  }
}

async function replayFailureArtifact(artifactPath) {
  const root = path.resolve(artifactPath);
  const metadata = JSON.parse(fs.readFileSync(path.join(root, "metadata.json"), "utf8"));
  const vanilla = JSON.parse(fs.readFileSync(path.join(root, "vanilla-trace.json"), "utf8"));
  const native = JSON.parse(fs.readFileSync(path.join(root, "native-trace.json"), "utf8"));
  console.log(JSON.stringify({
    status: "replay",
    vanillaTraceSource: metadata.vanillaTraceSource,
    conformanceStatus: metadata.conformanceStatus ?? "unknown",
    version: metadata.version,
    scenario: metadata.scenario,
    seed: metadata.seed,
    tick: metadata.tick,
    vanillaTrace: vanilla.observed.slice(-10),
    nativeTrace: native.observed.slice(-10),
    firstDivergence: metadata.firstDivergence,
  }, null, 2));
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  if (args.help) {
    usage();
    return;
  }

  if (args.replay) {
    await replayFailureArtifact(args.replay);
    return;
  }

  const scenario = args.random ? generateRandomScenario(args.seed, args.version) : seededScenario(args.scenario, args.seed);

  await runScenario({
    version: args.version,
    scenario,
    ticks: args.ticks,
    seed: args.seed,
    debug: args.debug,
  });
}

main().catch((error) => {
  console.error(error.stack || error.message || String(error));
  process.exitCode = 1;
});
