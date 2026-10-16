# Still Preview Benchmark Results

## Overview

This report compares preview generation for the JPEG, HEIC, and RAF samples in `bench/`.
Each result is the median of 5 measured runs after one warm-up. The benchmark uses
each selected backend and validates that every run creates a non-empty JPEG preview. The tool
selection affects HEIC and RAW decoding; ordinary JPEG input uses Still's built-in image path.

- Generated: 2026-09-08T11:40:09.407Z
- Platform: `darwin-24.6.0`
- Logical CPUs: 8
- Binary: `/Users/innocentsmith/Dev/rust/stillkit/target/release/still` (20,480,848 bytes)
- Copies per run: 1
- Preview sizes: 1000, 2000px
- Rayon threads: default
- Tools: auto, no-deps, sips, magick

## Results

| Format | Sample          | Tool      |   Size | Decode path      | Threads |   Median |      Min |      Max |
| ------ | --------------- | --------- | -----: | ---------------- | ------: | -------: | -------: | -------: |
| JPG    | `DSCF0656.jpg`  | `auto`    | 1000px | Built-in         | default | 318.9 ms | 310.9 ms | 334.3 ms |
| JPG    | `DSCF0656.jpg`  | `auto`    | 2000px | Built-in         | default | 371.0 ms | 363.1 ms | 415.9 ms |
| HEIC   | `DSCF0656.HEIC` | `auto`    | 1000px | Automatic        | default | 100.6 ms |  93.2 ms | 101.5 ms |
| HEIC   | `DSCF0656.HEIC` | `auto`    | 2000px | Automatic        | default |  1.580 s |  1.559 s |  1.597 s |
| RAF    | `DSCF0750.RAF`  | `auto`    | 1000px | Automatic        | default | 753.1 ms | 716.2 ms | 833.2 ms |
| RAF    | `DSCF0750.RAF`  | `auto`    | 2000px | Automatic        | default | 777.7 ms | 768.7 ms | 808.4 ms |
| JPG    | `DSCF0656.jpg`  | `no-deps` | 1000px | Built-in         | default | 307.1 ms | 306.8 ms | 309.8 ms |
| JPG    | `DSCF0656.jpg`  | `no-deps` | 2000px | Built-in         | default | 359.0 ms | 357.7 ms | 361.5 ms |
| HEIC   | `DSCF0656.HEIC` | `no-deps` | 1000px | Embedded allowed | default |  88.6 ms |  87.6 ms |  90.5 ms |
| HEIC   | `DSCF0656.HEIC` | `no-deps` | 1000px | Primary          | default |  4.131 s |  4.093 s |  4.404 s |
| HEIC   | `DSCF0656.HEIC` | `no-deps` | 2000px | Embedded allowed | default |  5.468 s |  4.731 s |  5.909 s |
| HEIC   | `DSCF0656.HEIC` | `no-deps` | 2000px | Primary          | default |  8.240 s |  4.517 s | 20.618 s |
| RAF    | `DSCF0750.RAF`  | `no-deps` | 1000px | Embedded allowed | default | 450.0 ms | 406.9 ms | 487.3 ms |
| RAF    | `DSCF0750.RAF`  | `no-deps` | 1000px | Primary          | default |  3.708 s |  3.644 s |  3.893 s |
| RAF    | `DSCF0750.RAF`  | `no-deps` | 2000px | Embedded allowed | default | 461.5 ms | 430.6 ms | 614.2 ms |
| RAF    | `DSCF0750.RAF`  | `no-deps` | 2000px | Primary          | default |  4.584 s |  3.777 s |  5.214 s |
| JPG    | `DSCF0656.jpg`  | `sips`    | 1000px | Built-in         | default | 324.8 ms | 315.9 ms | 339.1 ms |
| JPG    | `DSCF0656.jpg`  | `sips`    | 2000px | Built-in         | default | 359.4 ms | 352.4 ms | 374.0 ms |
| HEIC   | `DSCF0656.HEIC` | `sips`    | 1000px | External tool    | default |  1.220 s |  1.179 s |  1.240 s |
| HEIC   | `DSCF0656.HEIC` | `sips`    | 2000px | External tool    | default |  1.546 s |  1.529 s |  1.565 s |
| JPG    | `DSCF0656.jpg`  | `magick`  | 1000px | Built-in         | default | 345.6 ms | 309.5 ms | 378.8 ms |
| JPG    | `DSCF0656.jpg`  | `magick`  | 2000px | Built-in         | default | 392.9 ms | 372.5 ms | 442.0 ms |
| HEIC   | `DSCF0656.HEIC` | `magick`  | 1000px | External tool    | default |  4.821 s |  4.455 s |  6.146 s |
| HEIC   | `DSCF0656.HEIC` | `magick`  | 2000px | External tool    | default |  6.224 s |  6.104 s |  6.666 s |

## Unsupported or failed combinations

| Tool     | Sample         | Result                                 |
| -------- | -------------- | -------------------------------------- |
| `sips`   | `DSCF0750.RAF` | sips could not generate this preview   |
| `magick` | `DSCF0750.RAF` | magick could not generate this preview |
