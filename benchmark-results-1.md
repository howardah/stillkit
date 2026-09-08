# Still Preview Benchmark Results

## Overview

This report compares preview generation for the JPEG, HEIC, and RAF samples in `bench/`.
Each result is the median of 5 measured runs after one warm-up. The benchmark uses
each selected backend and validates that every run creates a non-empty JPEG preview. The tool
selection affects HEIC and RAW decoding; ordinary JPEG input uses Still's built-in image path.

- Generated: 2026-09-07T20:32:58.697Z
- Platform: `darwin-24.6.0`
- Logical CPUs: 8
- Binary: `/Users/innocentsmith/Dev/rust/stillkit/target/release/still` (20,403,392 bytes)
- Copies per run: 1
- Preview sizes: 1000, 2000px
- Rayon threads: default
- Tools: auto, no-deps, sips, magick

## Results

| Format | Sample          | Tool      |   Size | Decode path      | Threads |   Median |      Min |      Max |
| ------ | --------------- | --------- | -----: | ---------------- | ------: | -------: | -------: | -------: |
| JPG    | `DSCF0656.jpg`  | `auto`    | 1000px | Built-in         | default |  1.299 s |  1.165 s |  1.420 s |
| JPG    | `DSCF0656.jpg`  | `auto`    | 2000px | Built-in         | default |  1.399 s |  1.228 s |  1.479 s |
| HEIC   | `DSCF0656.HEIC` | `auto`    | 1000px | Automatic        | default | 787.8 ms | 780.4 ms | 796.5 ms |
| HEIC   | `DSCF0656.HEIC` | `auto`    | 2000px | Automatic        | default |  3.269 s |  3.057 s |  3.330 s |
| RAF    | `DSCF0750.RAF`  | `auto`    | 1000px | Automatic        | default |  1.240 s |  1.094 s |  1.610 s |
| RAF    | `DSCF0750.RAF`  | `auto`    | 2000px | Automatic        | default |  1.204 s |  1.182 s |  1.332 s |
| JPG    | `DSCF0656.jpg`  | `no-deps` | 1000px | Built-in         | default | 510.8 ms | 478.6 ms | 581.2 ms |
| JPG    | `DSCF0656.jpg`  | `no-deps` | 2000px | Built-in         | default | 725.2 ms | 617.4 ms | 834.9 ms |
| HEIC   | `DSCF0656.HEIC` | `no-deps` | 1000px | Embedded allowed | default | 165.6 ms | 155.2 ms | 173.6 ms |
| HEIC   | `DSCF0656.HEIC` | `no-deps` | 1000px | Primary          | default |  6.650 s |  6.467 s |  6.815 s |
| HEIC   | `DSCF0656.HEIC` | `no-deps` | 2000px | Embedded allowed | default |  6.736 s |  6.534 s |  7.062 s |
| HEIC   | `DSCF0656.HEIC` | `no-deps` | 2000px | Primary          | default |  6.849 s |  6.705 s |  6.906 s |
| RAF    | `DSCF0750.RAF`  | `no-deps` | 1000px | Embedded allowed | default | 542.0 ms | 528.9 ms | 600.0 ms |
| RAF    | `DSCF0750.RAF`  | `no-deps` | 1000px | Primary          | default |  4.803 s |  4.715 s |  5.351 s |
| RAF    | `DSCF0750.RAF`  | `no-deps` | 2000px | Embedded allowed | default | 593.5 ms | 588.7 ms | 594.0 ms |
| RAF    | `DSCF0750.RAF`  | `no-deps` | 2000px | Primary          | default |  4.920 s |  4.831 s |  5.525 s |
| JPG    | `DSCF0656.jpg`  | `sips`    | 1000px | Built-in         | default |  1.102 s |  1.096 s |  1.110 s |
| JPG    | `DSCF0656.jpg`  | `sips`    | 2000px | Built-in         | default |  1.203 s |  1.191 s |  1.226 s |
| HEIC   | `DSCF0656.HEIC` | `sips`    | 1000px | External tool    | default |  2.499 s |  2.432 s |  2.555 s |
| HEIC   | `DSCF0656.HEIC` | `sips`    | 2000px | External tool    | default |  3.504 s |  3.070 s |  3.831 s |
| JPG    | `DSCF0656.jpg`  | `magick`  | 1000px | Built-in         | default |  1.283 s |  1.153 s |  1.352 s |
| JPG    | `DSCF0656.jpg`  | `magick`  | 2000px | Built-in         | default |  1.304 s |  1.210 s |  1.324 s |
| HEIC   | `DSCF0656.HEIC` | `magick`  | 1000px | External tool    | default |  6.674 s |  6.304 s |  7.116 s |
| HEIC   | `DSCF0656.HEIC` | `magick`  | 2000px | External tool    | default |  8.704 s |  8.542 s |  8.854 s |

## Unsupported or failed combinations

| Tool     | Sample         | Result                                 |
| -------- | -------------- | -------------------------------------- |
| `sips`   | `DSCF0750.RAF` | sips could not generate this preview   |
| `magick` | `DSCF0750.RAF` | magick could not generate this preview |
