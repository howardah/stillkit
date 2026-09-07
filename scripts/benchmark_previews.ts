#!/usr/bin/env bun

/** Compare release binaries without touching source images or existing previews.
 *
 * Example:
 *   bun scripts/benchmark_previews.ts demo/DSCF0656.HEIC \
 *     /tmp/still-before target/release/still
 *
 * Use --copies 17 for a repeated-sample batch, not a diverse-photo benchmark.
 */

import {
  copyFileSync,
  mkdirSync,
  mkdtempSync,
  readdirSync,
  realpathSync,
  rmSync,
  statSync,
} from "node:fs";
import { cpus, platform, release } from "node:os";
import { basename, extname, join } from "node:path";
import { tmpdir } from "node:os";

type Options = {
  sample: string;
  binaries: string[];
  copies: number;
  runs: number;
  sizes: number[];
  threads: string[];
  primaryOnly: boolean;
};

const usage = `Usage: bun scripts/benchmark_previews.ts <sample> <binary...> [options]

Compare preview-generation performance between release binaries.

Options:
  --copies <count>       Copies of the sample per run (default: 1)
  --runs <count>         Measured runs after one warm-up (default: 5)
  --sizes <sizes...>     Maximum preview sizes (default: 1000 2000)
  --threads <values...>  Rayon thread counts or "default" (default: default)
  --primary-only         Disable embedded previews for every run
  -h, --help             Show this help
`;

function fail(message: string): never {
  console.error(`error: ${message}\n\n${usage}`);
  process.exit(2);
}

function positiveInteger(value: string, option: string): number {
  const parsed = Number(value);
  if (!Number.isInteger(parsed) || parsed < 1) fail(`${option} must contain positive integers`);
  return parsed;
}

function parseArgs(args: string[]): Options {
  const positional: string[] = [];
  const options: Options = {
    sample: "",
    binaries: [],
    copies: 1,
    runs: 5,
    sizes: [1000, 2000],
    threads: ["default"],
    primaryOnly: false,
  };

  for (let index = 0; index < args.length; index++) {
    const argument = args[index];
    if (argument === "-h" || argument === "--help") {
      console.log(usage);
      process.exit(0);
    } else if (argument === "--primary-only") {
      options.primaryOnly = true;
    } else if (argument === "--copies" || argument === "--runs") {
      const value = args[++index];
      if (value === undefined) fail(`${argument} requires a value`);
      options[argument === "--copies" ? "copies" : "runs"] = positiveInteger(value, argument);
    } else if (argument === "--sizes" || argument === "--threads") {
      const values: string[] = [];
      while (args[index + 1] !== undefined && !args[index + 1].startsWith("--")) {
        values.push(args[++index]);
      }
      if (values.length === 0) fail(`${argument} requires at least one value`);
      if (argument === "--sizes") {
        options.sizes = values.map((value) => positiveInteger(value, argument));
      } else {
        if (values.some((value) => value !== "default" && !/^\d+$/.test(value))) {
          fail('--threads values must be "default" or positive integers');
        }
        values.filter((value) => value !== "default").forEach((value) => positiveInteger(value, argument));
        options.threads = values;
      }
    } else if (argument.startsWith("-")) {
      fail(`unknown option ${argument}`);
    } else {
      positional.push(argument);
    }
  }

  if (positional.length < 2) fail("provide a sample and at least one binary");
  options.sample = resolveExisting(positional[0], "sample");
  options.binaries = positional.slice(1).map((path) => resolveExisting(path, "binary"));
  return options;
}

function resolveExisting(path: string, label: string): string {
  try {
    return realpathSync(path);
  } catch {
    fail(`${label} does not exist: ${path}`);
  }
}

function median(values: number[]): number {
  const sorted = values.toSorted((left, right) => left - right);
  const middle = Math.floor(sorted.length / 2);
  return sorted.length % 2 === 0 ? (sorted[middle - 1] + sorted[middle]) / 2 : sorted[middle];
}

function main(): void {
  const options = parseArgs(Bun.argv.slice(2));
  console.log(JSON.stringify({
    platform: `${platform()}-${release()}`,
    cpu_count: cpus().length,
    sample: options.sample,
    copies: options.copies,
    binary_bytes: Object.fromEntries(options.binaries.map((binary) => [binary, statSync(binary).size])),
  }));

  const workspace = mkdtempSync(join(tmpdir(), "still-preview-bench-"));
  try {
    const source = join(workspace, "inputs");
    mkdirSync(source);
    for (let index = 0; index < options.copies; index++) {
      copyFileSync(options.sample, join(source, `sample-${index}${extname(options.sample)}`));
    }

    let sequence = 0;
    for (const threads of options.threads) {
      const env = { ...process.env };
      delete env.RAYON_NUM_THREADS;
      if (threads !== "default") env.RAYON_NUM_THREADS = threads;
      for (const size of options.sizes) {
        for (const primaryOnly of options.primaryOnly ? [true] : [false, true]) {
          const timings = new Map(options.binaries.map((binary) => [binary, [] as number[]]));
          for (let iteration = 0; iteration <= options.runs; iteration++) {
            const order = iteration % 2 === 0 ? options.binaries : options.binaries.toReversed();
            for (const binary of order) {
              const output = join(workspace, `output-${sequence++}`);
              const command = [binary, "previews", source, "--no-deps", "--max-size", `${size}`, "--output", output];
              if (primaryOnly) command.push("--no-embedded-preview");
              const start = Bun.nanoseconds();
              const result = Bun.spawnSync(command, { env, stdout: "pipe", stderr: "pipe" });
              const elapsed = (Bun.nanoseconds() - start) / 1e9;
              const stdout = result.stdout.toString();
              const stderr = result.stderr.toString();
              const previews = readdirSync(output).filter((path) => extname(path).toLowerCase() === ".jpg");
              const invalidOutput = previews.length !== options.copies
                || previews.some((path) => statSync(join(output, path)).size === 0);
              if (!result.success || invalidOutput || stderr.toLowerCase().includes("error")) {
                throw new Error(`benchmark failed for ${basename(binary)}:\n${stdout}${stderr}`);
              }
              if (iteration > 0) timings.get(binary)!.push(elapsed);
            }
          }

          for (const [binary, seconds] of timings) {
            console.log(JSON.stringify({
              binary,
              size,
              primary_only: primaryOnly,
              threads,
              median_seconds: median(seconds),
              seconds,
            }));
          }
        }
      }
    }
  } finally {
    rmSync(workspace, { recursive: true, force: true });
  }
}

main();
