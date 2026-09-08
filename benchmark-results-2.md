# Still Preview Benchmark Results

## Overview

This report compares preview generation for the JPEG, HEIC, and RAF samples in `bench/`.
Each result is the median of 5 measured runs after one warm-up. The benchmark uses
each selected backend and validates that every run creates a non-empty JPEG preview. The tool
selection affects HEIC and RAW decoding; ordinary JPEG input uses Still's built-in image path.

- Generated: 2026-09-08T11:18:51.431Z
- Platform: `darwin-24.6.0`
- Logical CPUs: 8
- Binary: `/Users/innocentsmith/Dev/rust/stillkit/target/release/still` (20,480,840 bytes)
- Copies per run: 1
- Preview sizes: 1000, 2000px
- Rayon threads: default
- Tools: auto, no-deps, sips, magick

## Results

| Format | Sample          | Tool      |   Size | Decode path      | Threads |   Median |      Min |      Max |
| ------ | --------------- | --------- | -----: | ---------------- | ------: | -------: | -------: | -------: |
| JPG    | `DSCF0656.jpg`  | `auto`    | 1000px | Built-in         | default | 319.2 ms | 313.1 ms | 325.2 ms |
| JPG    | `DSCF0656.jpg`  | `auto`    | 2000px | Built-in         | default | 369.5 ms | 366.8 ms | 375.3 ms |
| HEIC   | `DSCF0656.HEIC` | `auto`    | 1000px | Automatic        | default | 508.5 ms | 507.8 ms | 511.5 ms |
| HEIC   | `DSCF0656.HEIC` | `auto`    | 2000px | Automatic        | default |  2.160 s |  2.056 s |  2.336 s |
| RAF    | `DSCF0750.RAF`  | `auto`    | 1000px | Automatic        | default | 797.6 ms | 758.5 ms | 918.7 ms |
| RAF    | `DSCF0750.RAF`  | `auto`    | 2000px | Automatic        | default | 930.2 ms | 819.8 ms |  1.192 s |
| JPG    | `DSCF0656.jpg`  | `no-deps` | 1000px | Built-in         | default | 327.7 ms | 315.2 ms | 338.5 ms |
| JPG    | `DSCF0656.jpg`  | `no-deps` | 2000px | Built-in         | default | 367.3 ms | 364.8 ms | 371.6 ms |
| HEIC   | `DSCF0656.HEIC` | `no-deps` | 1000px | Embedded allowed | default |  90.2 ms |  89.2 ms |  90.8 ms |
| HEIC   | `DSCF0656.HEIC` | `no-deps` | 1000px | Primary          | default |  4.751 s |  4.503 s |  5.956 s |
| HEIC   | `DSCF0656.HEIC` | `no-deps` | 2000px | Embedded allowed | default |  5.750 s |  5.661 s |  6.224 s |
| HEIC   | `DSCF0656.HEIC` | `no-deps` | 2000px | Primary          | default |  7.070 s |  4.686 s | 10.489 s |
| RAF    | `DSCF0750.RAF`  | `no-deps` | 1000px | Embedded allowed | default | 458.5 ms | 430.0 ms | 500.9 ms |
| RAF    | `DSCF0750.RAF`  | `no-deps` | 1000px | Primary          | default |  4.166 s |  3.561 s |  5.266 s |
| RAF    | `DSCF0750.RAF`  | `no-deps` | 2000px | Embedded allowed | default | 423.4 ms | 418.7 ms | 437.7 ms |
| RAF    | `DSCF0750.RAF`  | `no-deps` | 2000px | Primary          | default |  3.565 s |  3.515 s |  3.996 s |
| JPG    | `DSCF0656.jpg`  | `sips`    | 1000px | Built-in         | default | 292.7 ms | 289.9 ms | 297.3 ms |
| JPG    | `DSCF0656.jpg`  | `sips`    | 2000px | Built-in         | default | 360.7 ms | 351.0 ms | 403.2 ms |
| HEIC   | `DSCF0656.HEIC` | `sips`    | 1000px | External tool    | default |  1.649 s |  1.625 s |  1.720 s |
| HEIC   | `DSCF0656.HEIC` | `sips`    | 2000px | External tool    | default |  1.995 s |  1.982 s |  2.035 s |
| JPG    | `DSCF0656.jpg`  | `magick`  | 1000px | Built-in         | default | 315.3 ms | 312.2 ms | 337.4 ms |
| JPG    | `DSCF0656.jpg`  | `magick`  | 2000px | Built-in         | default | 364.7 ms | 359.4 ms | 365.7 ms |
| HEIC   | `DSCF0656.HEIC` | `magick`  | 1000px | External tool    | default |  4.078 s |  3.989 s |  4.116 s |
| HEIC   | `DSCF0656.HEIC` | `magick`  | 2000px | External tool    | default |  6.072 s |  5.718 s |  6.491 s |

## Unsupported or failed combinations

| Tool     | Sample         | Result                                 |
| -------- | -------------- | -------------------------------------- |
| `sips`   | `DSCF0750.RAF` | sips could not generate this preview   |
| `magick` | `DSCF0750.RAF` | magick could not generate this preview |
