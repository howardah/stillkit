# Still Preview Benchmark Results

## Overview

This report compares preview generation for the JPEG, HEIC, and RAF samples in `bench/`.
Each result is the median of 5 measured runs after one warm-up. The benchmark uses
the built-in decoding pipeline through `--no-deps` and validates that every run creates a
non-empty JPEG preview.

- Generated: 2026-09-07T06:51:45.670Z
- Platform: `darwin-24.6.0`
- Logical CPUs: 8
- Binary: `/Users/innocentsmith/Dev/rust/stillkit/target/release/still` (20,400,792 bytes)
- Copies per run: 6
- Preview sizes: 1000, 2000px
- Rayon threads: default

## Results

| Format | Sample          |   Size | Decode path      | Threads |   Median |      Min |      Max |
| ------ | --------------- | -----: | ---------------- | ------: | -------: | -------: | -------: |
| JPG    | `DSCF0656.jpg`  | 1000px | Embedded allowed | default | 517.8 ms | 500.7 ms | 722.9 ms |
| JPG    | `DSCF0656.jpg`  | 1000px | Primary image    | default | 730.9 ms | 658.5 ms | 918.0 ms |
| JPG    | `DSCF0656.jpg`  | 2000px | Embedded allowed | default | 745.8 ms | 685.7 ms | 838.1 ms |
| JPG    | `DSCF0656.jpg`  | 2000px | Primary image    | default | 652.1 ms | 643.7 ms | 679.2 ms |
| HEIC   | `DSCF0656.HEIC` | 1000px | Embedded allowed | default | 173.1 ms | 170.9 ms | 193.5 ms |
| HEIC   | `DSCF0656.HEIC` | 1000px | Primary image    | default |  7.886 s |  7.397 s | 10.384 s |
| HEIC   | `DSCF0656.HEIC` | 2000px | Embedded allowed | default |  9.610 s |  9.101 s | 10.106 s |
| HEIC   | `DSCF0656.HEIC` | 2000px | Primary image    | default |  9.287 s |  8.440 s |  9.848 s |
| RAF    | `DSCF0750.RAF`  | 1000px | Embedded allowed | default | 957.7 ms | 854.7 ms | 986.3 ms |
| RAF    | `DSCF0750.RAF`  | 1000px | Primary image    | default | 16.886 s | 15.347 s | 21.660 s |
| RAF    | `DSCF0750.RAF`  | 2000px | Embedded allowed | default | 733.1 ms | 700.5 ms | 815.0 ms |
| RAF    | `DSCF0750.RAF`  | 2000px | Primary image    | default | 13.355 s | 12.862 s | 14.784 s |
