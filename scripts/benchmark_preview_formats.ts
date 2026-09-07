#!/usr/bin/env bun

/** Benchmark the JPEG, HEIC, and RAF samples in bench/ with target/release/still. */

import { realpathSync, statSync, writeFileSync } from "node:fs";
import { basename, join } from "node:path";

type Metadata = {
  platform: string;
  cpu_count: number;
  sample: string;
  copies: number;
  binary_bytes: Record<string, number>;
};

type Result = {
  binary: string;
  size: number;
  primary_only: boolean;
  threads: string;
  median_seconds: number;
  seconds: number[];
};

type Row = Result & {
  sample: string;
  format: string;
};

const usage = `Usage: bun scripts/benchmark_preview_formats.ts [options]

Run benchmark_previews.ts for all three samples in bench/ using
target/release/still, then print a terminal table or write a Markdown report.

Options:
  -o, --output <file>    Write a Markdown report instead of a terminal table
  --copies <count>       Copies of each sample per run (default: 1)
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

function parseArgs(args: string[]): { output?: string; benchmarkArgs: string[] } {
  const benchmarkArgs: string[] = [];
  let output: string | undefined;

  for (let index = 0; index < args.length; index++) {
    const argument = args[index];
    if (argument === "-h" || argument === "--help") {
      console.log(usage);
      process.exit(0);
    }
    if (argument === "-o" || argument === "--output") {
      output = args[++index];
      if (output === undefined) fail(`${argument} requires a file path`);
      continue;
    }
    if (argument === "--primary-only") {
      benchmarkArgs.push(argument);
      continue;
    }
    if (argument === "--copies" || argument === "--runs") {
      const value = args[++index];
      if (value === undefined) fail(`${argument} requires a value`);
      benchmarkArgs.push(argument, value);
      continue;
    }
    if (argument === "--sizes" || argument === "--threads") {
      const values: string[] = [];
      while (args[index + 1] !== undefined && !args[index + 1].startsWith("-")) {
        values.push(args[++index]);
      }
      if (values.length === 0) fail(`${argument} requires at least one value`);
      benchmarkArgs.push(argument, ...values);
      continue;
    }
    fail(`unknown option ${argument}`);
  }

  return { output, benchmarkArgs };
}

function runBenchmark(
  script: string,
  sample: string,
  binary: string,
  benchmarkArgs: string[],
): { metadata: Metadata; rows: Row[] } {
  console.error(`Benchmarking ${basename(sample)}...`);
  const result = Bun.spawnSync([process.execPath, script, sample, binary, ...benchmarkArgs], {
    stdout: "pipe",
    stderr: "inherit",
  });
  if (!result.success) fail(`benchmark failed for ${sample}`);

  const lines = result.stdout.toString().trim().split("\n");
  if (lines.length < 2) fail(`benchmark returned no results for ${sample}`);
  try {
    const metadata = JSON.parse(lines[0]) as Metadata;
    const format = basename(sample).split(".").at(-1)!.toUpperCase();
    const rows = lines.slice(1).map((line) => ({
      ...(JSON.parse(line) as Result),
      sample: basename(sample),
      format,
    }));
    return { metadata, rows };
  } catch (error) {
    fail(`could not parse benchmark results for ${sample}: ${error}`);
  }
}

function seconds(value: number): string {
  return value < 1 ? `${(value * 1_000).toFixed(1)} ms` : `${value.toFixed(3)} s`;
}

function table(rows: string[][]): string {
  const widths = rows[0].map((_, column) => Math.max(...rows.map((row) => row[column].length)));
  const render = (row: string[]) => row.map((cell, column) => cell.padEnd(widths[column])).join("  ");
  return [render(rows[0]), widths.map((width) => "─".repeat(width)).join("  "), ...rows.slice(1).map(render)].join("\n");
}

function terminalReport(rows: Row[]): string {
  return table([
    ["Format", "Sample", "Size", "Decode path", "Threads", "Median", "Min", "Max"],
    ...rows.map((row) => [
      row.format,
      row.sample,
      `${row.size}px`,
      row.primary_only ? "Primary" : "Embedded allowed",
      row.threads,
      seconds(row.median_seconds),
      seconds(Math.min(...row.seconds)),
      seconds(Math.max(...row.seconds)),
    ]),
  ]);
}

function markdownReport(metadata: Metadata, rows: Row[], binary: string): string {
  const generated = new Date().toISOString();
  const measuredRuns = rows[0]?.seconds.length ?? 0;
  const sizes = [...new Set(rows.map((row) => row.size))].join(", ");
  const threads = [...new Set(rows.map((row) => row.threads))].join(", ");
  const body = rows.map((row) => [
    row.format,
    `\`${row.sample}\``,
    `${row.size}px`,
    row.primary_only ? "Primary image" : "Embedded allowed",
    row.threads,
    seconds(row.median_seconds),
    seconds(Math.min(...row.seconds)),
    seconds(Math.max(...row.seconds)),
  ].join(" | "));

  return `# Still Preview Benchmark Results

## Overview

This report compares preview generation for the JPEG, HEIC, and RAF samples in \`bench/\`.
Each result is the median of ${measuredRuns} measured runs after one warm-up. The benchmark uses
the built-in decoding pipeline through \`--no-deps\` and validates that every run creates a
non-empty JPEG preview.

- Generated: ${generated}
- Platform: \`${metadata.platform}\`
- Logical CPUs: ${metadata.cpu_count}
- Binary: \`${binary}\` (${statSync(binary).size.toLocaleString("en-US")} bytes)
- Copies per run: ${metadata.copies}
- Preview sizes: ${sizes}px
- Rayon threads: ${threads}

## Results

Format | Sample | Size | Decode path | Threads | Median | Min | Max
--- | --- | ---: | --- | ---: | ---: | ---: | ---:
${body.join("\n")}
`;
}

function main(): void {
  const { output, benchmarkArgs } = parseArgs(Bun.argv.slice(2));
  const root = join(import.meta.dir, "..");
  const script = realpathSync(join(import.meta.dir, "benchmark_previews.ts"));
  const binary = realpathSync(join(root, "target", "release", "still"));
  const samples = ["DSCF0656.jpg", "DSCF0656.HEIC", "DSCF0750.RAF"]
    .map((name) => realpathSync(join(root, "bench", name)));

  const runs = samples.map((sample) => runBenchmark(script, sample, binary, benchmarkArgs));
  const rows = runs.flatMap((run) => run.rows);
  if (output !== undefined) {
    writeFileSync(output, markdownReport(runs[0].metadata, rows, binary));
    console.log(`Wrote benchmark report to ${output}`);
  } else {
    console.log(`\nStill preview benchmark — ${runs[0].metadata.platform}, ${runs[0].metadata.cpu_count} CPUs\n`);
    console.log(terminalReport(rows));
  }
}

main();
