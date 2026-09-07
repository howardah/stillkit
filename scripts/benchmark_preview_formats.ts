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
  tool: "auto" | "no-deps" | "sips" | "magick";
  threads: string;
  median_seconds: number;
  seconds: number[];
};

type Row = Result & {
  sample: string;
  format: string;
};

type BenchmarkRun = {
  metadata?: Metadata;
  rows: Row[];
  failure?: { sample: string; tool: string; reason: string };
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
  --tools <tools...>     Backends to benchmark (default: available tools)
  --primary-only         Disable embedded previews for every run
  -h, --help             Show this help
`;

function fail(message: string): never {
  console.error(`error: ${message}\n\n${usage}`);
  process.exit(2);
}

function parseArgs(args: string[]): {
  output?: string;
  benchmarkArgs: string[];
  tools: string[];
} {
  const benchmarkArgs: string[] = [];
  let tools: string[] | undefined;
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
    if (argument === "--sizes" || argument === "--threads" || argument === "--tools") {
      const values: string[] = [];
      while (args[index + 1] !== undefined && !args[index + 1].startsWith("-")) {
        values.push(args[++index]);
      }
      if (values.length === 0) fail(`${argument} requires at least one value`);
      if (argument === "--tools") {
        if (values.some((value) => !["auto", "no-deps", "sips", "magick"].includes(value))) {
          fail("--tools values must be auto, no-deps, sips, or magick");
        }
        tools = values;
      } else {
        benchmarkArgs.push(argument, ...values);
      }
      continue;
    }
    fail(`unknown option ${argument}`);
  }

  const selected = tools ?? ["auto", "no-deps", "sips", "magick"];
  const available = selected.filter((tool) => ["auto", "no-deps"].includes(tool) || Bun.which(tool) !== null);
  for (const tool of selected.filter((tool) => !available.includes(tool))) {
    console.error(`Skipping unavailable tool: ${tool}`);
  }
  if (available.length === 0) fail("none of the selected tools are available");
  return { output, benchmarkArgs, tools: available };
}

function runBenchmark(
  script: string,
  sample: string,
  binary: string,
  tool: string,
  benchmarkArgs: string[],
): BenchmarkRun {
  console.error(`Benchmarking ${basename(sample)} with ${tool}...`);
  const result = Bun.spawnSync([process.execPath, script, sample, binary, "--tool", tool, ...benchmarkArgs], {
    stdout: "pipe",
    stderr: "pipe",
  });
  if (!result.success) {
    const reason = `${tool} could not generate this preview`;
    console.error(`Skipping ${basename(sample)} with ${tool}: ${reason}`);
    return { rows: [], failure: { sample: basename(sample), tool, reason } };
  }

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

function decodePath(row: Row): string {
  if (row.format === "JPG") return "Built-in";
  if (row.tool === "auto") return "Automatic";
  if (row.tool !== "no-deps") return "External tool";
  return row.primary_only ? "Primary" : "Embedded allowed";
}

function terminalReport(rows: Row[]): string {
  return table([
    ["Format", "Sample", "Tool", "Size", "Decode path", "Threads", "Median", "Min", "Max"],
    ...rows.map((row) => [
      row.format,
      row.sample,
      row.tool,
      `${row.size}px`,
      decodePath(row),
      row.threads,
      seconds(row.median_seconds),
      seconds(Math.min(...row.seconds)),
      seconds(Math.max(...row.seconds)),
    ]),
  ]);
}

function markdownReport(
  metadata: Metadata,
  rows: Row[],
  failures: NonNullable<BenchmarkRun["failure"]>[],
  binary: string,
): string {
  const generated = new Date().toISOString();
  const measuredRuns = rows[0]?.seconds.length ?? 0;
  const sizes = [...new Set(rows.map((row) => row.size))].join(", ");
  const threads = [...new Set(rows.map((row) => row.threads))].join(", ");
  const tools = [...new Set(rows.map((row) => row.tool))].join(", ");
  const body = rows.map((row) => [
    row.format,
    `\`${row.sample}\``,
    `\`${row.tool}\``,
    `${row.size}px`,
    decodePath(row),
    row.threads,
    seconds(row.median_seconds),
    seconds(Math.min(...row.seconds)),
    seconds(Math.max(...row.seconds)),
  ].join(" | "));

  return `# Still Preview Benchmark Results

## Overview

This report compares preview generation for the JPEG, HEIC, and RAF samples in \`bench/\`.
Each result is the median of ${measuredRuns} measured runs after one warm-up. The benchmark uses
each selected backend and validates that every run creates a non-empty JPEG preview. The tool
selection affects HEIC and RAW decoding; ordinary JPEG input uses Still's built-in image path.

- Generated: ${generated}
- Platform: \`${metadata.platform}\`
- Logical CPUs: ${metadata.cpu_count}
- Binary: \`${binary}\` (${statSync(binary).size.toLocaleString("en-US")} bytes)
- Copies per run: ${metadata.copies}
- Preview sizes: ${sizes}px
- Rayon threads: ${threads}
- Tools: ${tools}

## Results

Format | Sample | Tool | Size | Decode path | Threads | Median | Min | Max
--- | --- | --- | ---: | --- | ---: | ---: | ---: | ---:
${body.join("\n")}
${failures.length === 0 ? "" : `
## Unsupported or failed combinations

Tool | Sample | Result
--- | --- | ---
${failures.map((failure) => `\`${failure.tool}\` | \`${failure.sample}\` | ${failure.reason}`).join("\n")}
`}
`;
}

function main(): void {
  const { output, benchmarkArgs, tools } = parseArgs(Bun.argv.slice(2));
  const root = join(import.meta.dir, "..");
  const script = realpathSync(join(import.meta.dir, "benchmark_previews.ts"));
  const binary = realpathSync(join(root, "target", "release", "still"));
  const samples = ["DSCF0656.jpg", "DSCF0656.HEIC", "DSCF0750.RAF"]
    .map((name) => realpathSync(join(root, "bench", name)));

  const runs = tools.flatMap((tool) => samples.map(
    (sample) => runBenchmark(script, sample, binary, tool, benchmarkArgs),
  ));
  const rows = runs.flatMap((run) => run.rows);
  const failures = runs.flatMap((run) => run.failure === undefined ? [] : [run.failure]);
  const metadata = runs.find((run) => run.metadata !== undefined)?.metadata;
  if (metadata === undefined || rows.length === 0) fail("all benchmark combinations failed");
  if (output !== undefined) {
    writeFileSync(output, markdownReport(metadata, rows, failures, binary));
    console.log(`Wrote benchmark report to ${output}`);
  } else {
    console.log(`\nStill preview benchmark — ${metadata.platform}, ${metadata.cpu_count} CPUs\n`);
    console.log(terminalReport(rows));
    if (failures.length > 0) {
      console.log("\nUnsupported or failed combinations:\n");
      console.log(table([
        ["Tool", "Sample", "Result"],
        ...failures.map((failure) => [failure.tool, failure.sample, failure.reason]),
      ]));
    }
  }
}

main();
