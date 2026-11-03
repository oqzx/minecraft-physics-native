const fs = require("node:fs");
const path = require("node:path");

function usage() {
    console.error("Usage: node scripts/compare-tick-traces.cjs <expected.json> <actual.json> [--epsilon <number>]");
    process.exitCode = 1;
}

function readTrace(file) {
    const resolved = path.resolve(file);
    const trace = JSON.parse(fs.readFileSync(resolved, "utf8"));
    if (!Array.isArray(trace.observed) && !Array.isArray(trace.frames)) {
        throw new Error(`${file} does not contain frames or observed states`);
    }
    return trace;
}

function numericDifference(expected, actual, epsilon, field, differences) {
    const delta = Math.abs(expected - actual);
    if (delta > epsilon) {
        differences.push({ field, expected, actual, delta });
    }
}

function compareValue(expected, actual, epsilon, field, differences) {
    if (typeof expected === "number" && typeof actual === "number") {
        numericDifference(expected, actual, epsilon, field, differences);
        return;
    }
    if (expected === null || actual === null || typeof expected !== "object" || typeof actual !== "object") {
        if (expected !== actual) differences.push({ field, expected, actual });
        return;
    }
    const keys = new Set([...Object.keys(expected), ...Object.keys(actual)]);
    for (const key of keys) {
        compareValue(expected[key], actual[key], epsilon, `${field}.${key}`, differences);
    }
}

function states(trace) {
    if (Array.isArray(trace.observed)) {
        return trace.observed.map(item => ({ tick: item.tick, state: item.state }));
    }
    return trace.frames.map(item => ({ tick: item.tick, state: item.state }));
}

try {
    const [expectedFile, actualFile, ...args] = process.argv.slice(2);
    if (!expectedFile || !actualFile) {
        usage();
        return;
    }
    const epsilonIndex = args.indexOf("--epsilon");
    const epsilon = epsilonIndex === -1 ? 0 : Number(args[epsilonIndex + 1]);
    if (!Number.isFinite(epsilon) || epsilon < 0) throw new Error("--epsilon must be a finite non-negative number");

    const expected = states(readTrace(expectedFile));
    const actual = states(readTrace(actualFile));
    const firstLength = Math.min(expected.length, actual.length);
    const differences = [];
    let firstDivergence = null;
    for (let index = 0; index < firstLength; index += 1) {
        if (expected[index].tick !== actual[index].tick) {
            firstDivergence = { index, reason: "tick-order", expected: expected[index].tick, actual: actual[index].tick };
            break;
        }
        const tickDifferences = [];
        compareValue(expected[index].state, actual[index].state, epsilon, "state", tickDifferences);
        if (tickDifferences.length) {
            firstDivergence = { index, tick: expected[index].tick, differences: tickDifferences };
            break;
        }
    }
    if (!firstDivergence && expected.length !== actual.length) {
        firstDivergence = { index: firstLength, reason: "length", expected: expected.length, actual: actual.length };
    }
    const result = {
        pass: firstDivergence === null,
        epsilon,
        expectedTicks: expected.length,
        actualTicks: actual.length,
        firstDivergence,
    };
    console.log(JSON.stringify(result, null, 2));
    if (!result.pass) process.exitCode = 2;
} catch (error) {
    console.error(error instanceof Error ? error.message : error);
    process.exitCode = 1;
}
