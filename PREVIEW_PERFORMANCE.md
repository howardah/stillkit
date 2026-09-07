# Preview performance

This investigation covers HEIC previews on macOS, Linux, and Windows. This initial
record was written before the portable resizing optimization. Measurements below
are historical unless identified as results of the new experiment.

## Correctness before speed

`demo/DSCF0656.HEIC` is a 7728 × 5152, 10-bit, 4:2:2 camera image. Published
`heic` 0.1.x versions tested during the investigation produced pale, banded output.
Stillkit pins upstream revision `9e7f75617fb1e62e642ee34ee1ac21a5f068ab51`
for the chroma-decoding fix. The faster corrupted output is not a valid target:
the corrected decoder performs work that the broken path did not handle correctly.

The sample contains a 1920 × 1280 embedded JPEG, plus smaller previews. Decoding
that JPEG avoids decoding the approximately 40-megapixel HEVC primary image.

## Historical measurements

The user's 17-photo batch, default 1000-pixel JPEG previews at quality 75:

| Stage / command | User seconds | System seconds | Wall seconds |
| --- | ---: | ---: | ---: |
| Original, corrupted `--no-deps` | 47.01 | 6.88 | 9.267 |
| Original `--tool magick` | 167.65 | 25.69 | 35.879 |
| Original `--tool sips` | 24.88 | 7.37 | 10.796 |
| Corrected decoder `--no-deps` | 128.91 | 6.42 | 22.225 |
| Corrected decoder, automatic tools | 25.04 | 7.80 | 14.794 |
| Embedded/copy improvements, automatic tools | 25.44 | 8.46 | 11.764 |
| Embedded/copy improvements, `--no-deps` | 3.16 | 0.44 | 0.660 |
| Same, `--no-deps --no-embedded-preview` | 125.44 | 4.28 | 20.598 |

These are individual user runs, not controlled repeated trials. The last primary
decode was about 7.3% faster in wall time than the first corrected run. CPU time
is summed across threads and can exceed wall time.

Earlier local single-sample profiling found primary decoding taking several
seconds, RGB conversion around 0.29 seconds, and Lanczos resizing around 0.83
seconds. An RGBA path additionally spent around 0.43 seconds converting to RGBA,
0.054 seconds converting RGBA to RGB, and 0.073 seconds cloning RGB. These are
approximate observations from different experiments, not additive guarantees.
After the previous improvements, a local end-to-end sample run measured 0.21
seconds with the shortcut versus 5.11 seconds with it disabled. Do not compare
these directly with the user's batch or infer Windows/Linux performance.

## Current behavior and completed improvements

- The built-in preview path requests RGB directly and consumes image buffers,
  avoiding full-resolution RGBA intermediates and unnecessary RGB copies.
- Embedded HEIC candidates must be explicitly linked JPEG thumbnails, sufficiently
  large, dimensionally consistent, and compatible in aspect ratio, transforms,
  orientation, and color metadata. Unsupported crops, HDR/color cases, malformed
  JPEGs, and incomplete baseline entropy data fall back to primary decoding.
- The source bytes are reused when the embedded attempt falls back. Metadata
  copying still rereads the source; that is a separate remaining opportunity.
- `--no-embedded-preview` disables HEIC and RAW embedded shortcuts. `--full` and
  `--max-size full` always request the primary image. Smaller sources are not
  upscaled. The shortcut is not a promise that camera-rendered and primary pixels
  are identical.
- Forced external tools bypass the built-in shortcut. Automatic processing keeps
  its existing external-tool order: `sips` on macOS, then ImageMagick, then built-in
  decoding. `--no-deps` selects built-in processing.
- Default HEIC decoding is pure Rust. The existing optional `native-heic` feature
  enables decoder parallelism; it does not select a GPU backend. Batch processing
  already uses Rayon. Feature defaults and distribution assumptions stay unchanged.

## Platform acceleration findings

These are source inspections, not Windows/Linux runtime benchmarks. Hardware,
drivers, bit depth, chroma sampling, and image dimensions all affect eligibility.
An API exposing HEVC support does not establish support for this camera file.

| Platform | Candidate | Constraints |
| --- | --- | --- |
| macOS | Direct ImageIO thumbnail creation | Could avoid launching `sips` and its temporary PNG round-trip. Request primary-derived thumbnails with `kCGImageSourceCreateThumbnailFromImageAlways`; hardware acceleration is not guaranteed. |
| macOS | VideoToolbox backend in pinned `heic` | Explicitly rejects non-4:2:0 source chroma, including this 4:2:2 sample. |
| Windows | D3D11 video acceleration | GPU-driver decoding need not use the Store HEVC extension. The pinned backend selects Main/Main10 profiles and NV12/P010 4:2:0 surfaces, not this sample's 4:2:2 format. |
| Windows | Media Foundation | Codec availability can require HEVC Video Extensions. The pinned backend explicitly rejects non-4:2:0 input; Microsoft's decoder documentation also lists 4:2:0. |
| Linux | VA-API | Requires runtime libva libraries, a working driver and device access. The API defines Main422_10, but the pinned backend selects Main/Main10 and 4:2:0 surfaces. Device support must be queried, not assumed. |

Relevant sources:

- [Pinned HEIC implementation](https://github.com/imazen/heic/tree/9e7f75617fb1e62e642ee34ee1ac21a5f068ab51)
- [VideoToolbox implementation](https://github.com/imazen/heic/blob/9e7f75617fb1e62e642ee34ee1ac21a5f068ab51/heic-backend-videotoolbox/src/imp.rs)
- [D3D11 backend](https://github.com/imazen/heic/blob/9e7f75617fb1e62e642ee34ee1ac21a5f068ab51/heic-backend-d3d11va/src/lib.rs)
- [VA-API profile selection](https://github.com/imazen/heic/blob/9e7f75617fb1e62e642ee34ee1ac21a5f068ab51/heic-backend-vaapi/src/decode.rs)
- [Microsoft HEVC decoder constraints](https://learn.microsoft.com/en-us/windows/win32/medfound/h-265---hevc-video-decoder)
- [VA-API profile definitions](https://github.com/intel/libva/blob/master/va/va.h)
- [Apple primary-image thumbnail option](https://developer.apple.com/documentation/imageio/kcgimagesourcecreatethumbnailfromimagealways)

## Portable optimization experiment

Evaluate `fast_image_resize` 6.x for RGB8 Lanczos3 previews, using borrowed source
pixels, runtime SIMD dispatch, and scalar fallback. Keep the existing image
resizer for other pixel types and recoverable failures. Do not change color space,
encoding quality, alpha handling, dimensions, or tool selection. Do not add nested
Rayon resizing. See [the library documentation](https://docs.rs/fast_image_resize/6.0.0/).

Acceptance: at least 20% faster resizing or 5% faster end-to-end processing, without
a repeatable end-to-end regression greater than 5%. Camera-sample resized RGB must
reach at least 40 dB PSNR against the previous Lanczos implementation and pass
visual inspection. Byte identity between different resamplers is not required.

Use release builds, one warm-up and five measured runs, isolated temporary outputs,
1000/2000-pixel sizes, embedded enabled/disabled, and unchanged metadata policy.
Separate decode, resize, and encode measurements. A repeated-sample batch is a
throughput experiment, not the user's 17 distinct photos. Record toolchain, CPU,
features, thread counts, binary sizes, and medians with final results.

Other candidates remain unimplemented: metadata input/output rereads, upstream
HEVC/SIMD optimization, and workload-dependent concurrency tuning. Compare
`RAYON_NUM_THREADS=1`, `2`, `4`, and default without changing defaults based on one
machine. Hardware backends and architecture-specific build flags are deferred;
portable binaries must not require the development machine's CPU instructions.

## New results

The implementation uses `fast_image_resize` **6.1.0**, locked in Cargo.lock, with
only its `std` feature. This adds one package; its dependencies were already in
the lockfile. Its license is MIT OR Apache-2.0 and its minimum Rust version is
1.87 (local compiler: 1.98.0). Runtime CPU detection chooses SIMD where supported;
no build-machine-specific CPU flags or external libraries are required.

The RGB-specific typed API borrows both buffers and avoids compiling the dynamic
resizer's unused pixel-format implementations. The local default-feature release
binary grew from 19,645,368 to 20,400,792 bytes: 755,424 bytes, approximately 3.85%.
An initial dynamic-API experiment produced 23,843,360 bytes and was replaced before
performance acceptance. Scratch buffers are released after each resize rather
than retained on batch workers. Existing alpha/non-RGB resizing is unchanged.

Measurements on macOS 15.7.7, Intel i7-8569U (4 physical / 8 logical cores), 16 GiB
RAM, default Cargo features, Rust 1.98.0 release builds. The resizer selected AVX2.
Windows, Linux, ARM, and scalar runtime performance were not exercised locally.

Stage medians (one warm-up, five measurements, source bytes already in memory):

| Pixels resized | Old Lanczos seconds | New Lanczos seconds | JPEG encode seconds | PSNR versus old |
| --- | ---: | ---: | ---: | ---: |
| Primary → 1000 px | 0.541978 | 0.041459 | 0.011757 | 59.12 dB |
| Primary → 2000 px | 0.708960 | 0.054757 | 0.045682 | 57.69 dB |
| Embedded → 1000 px | 0.059059 | 0.005651 | 0.012226 | 56.18 dB |

Primary decode including RGB conversion measured 3.956112 seconds; embedded
selection/validation/decoding measured 0.043020 seconds. Resizing is approximately
10–13× faster, but primary HEVC decoding still dominates. All comparisons passed
the 40 dB threshold. Inspection of lossless before/after images at both sizes
showed no visible new banding, color shifts, or edge artifacts.

End-to-end single-image medians, including metadata, process startup, and file I/O:

| Maximum size / embedded policy | Before seconds | After seconds | Wall-time reduction |
| --- | ---: | ---: | ---: |
| 1000 px, embedded enabled | 0.137398 | 0.089904 | 34.6% |
| 1000 px, embedded disabled | 4.439313 | 3.934086 | 11.4% |
| 2000 px, embedded enabled (primary fallback) | 7.099813 | 5.857444 | 17.5% |
| 2000 px, embedded disabled | 7.960447 | 7.525137 | 5.5% |

The later end-to-end measurements were noisy: individual 2000-pixel runs ranged
from 4.96–50.56 seconds before and 4.08–36.01 seconds after with embedded enabled;
disabled runs ranged from 6.39–8.71 and 6.77–23.83 seconds respectively. At 1000
pixels primary-after runs included 5.69/13.28-second outliers (median 3.93).
All five samples contribute to each median; none were discarded. These results
support a gain but should not be read as precise portable speedup predictions.
The isolated resizing measurements and quality checks are the stronger evidence.
No end-to-end median regressed in this comparison.

The concurrency experiment uses four independent copies of the same camera file,
1000-pixel primary previews, the new binary, and five measured batches after a
warm-up per setting. This is not the user's 17-photo collection. It runs after
the single-file experiment, so thermal/background conditions can differ.

| `RAYON_NUM_THREADS` | Four-photo median seconds |
| --- | ---: |
| 1 | 28.434404 |
| 2 | 17.630574 |
| 4 | 12.488926 |
| default (8 logical CPUs) | 13.078134 |

Two/four threads clearly helped relative to one. Four threads and default were
close; this four-image batch cannot fully exercise eight simultaneous file jobs,
and configurations were measured sequentially rather than interleaved. This does
not justify a global thread-count change or predict throughput for 17 distinct
photos. Concurrency defaults are unchanged; decoder parallelism remains opt-in
through `native-heic`. Users can reproduce the sweep for their own workload.

### Acceptance and remaining work

The RGB resizer meets the resizing-speed and PSNR gates, preserves all tested
fallback/metadata behavior, and produced no end-to-end median regression. Keep
the portable optimization. HEVC decoding itself, metadata rereads, and native
platform integration remain unchanged. No claim is made that full decoding now
matches `sips`, or that Windows/Linux have been benchmarked.

Validation completed:

- `cargo fmt --all -- --check`, `cargo check --all-targets`,
  `cargo clippy --all-targets -- -D warnings`, and
  `cargo check --all-targets --features native-heic` passed.
- `cargo test`: 59 passed, five opt-in tests ignored in the normal suite.
- Explicit release runs passed for camera primary-image content, camera embedded
  eligibility, and the new timing/PSNR benchmark. The primary-content regression
  also verifies that the source HEIC bytes are unchanged.
- New unit tests cover constant colors, gradients, sharp edges, odd/portrait/
  landscape dimensions, one-pixel output, no-upscaling/full buffer reuse,
  recoverable invalid accelerator dimensions, and unchanged alpha/grayscale/
  higher-bit-depth resizing. Existing metadata and embedded-corruption fallback
  integration tests passed unchanged.
- The new resizer enables only `std`, not its optional Rayon feature. No generated
  media or benchmark binaries belong in the source changes.

### Reproducing the measurements

Preserve a release binary before changing the implementation, then build the new
version with the same features and toolchain. The comparison script uses only
Python's standard library, copies the source to a temporary directory, creates
fresh outputs for every invocation, checks that all previews succeeded, and
removes its own temporary directory afterward. Original media is never modified.
It preserves metadata and uses JPEG quality 75. Binary order alternates between
runs to reduce systematic cache/thermal bias. Timings include process startup and
filesystem work; compilation and initial input copying are excluded.

```sh
python3 scripts/benchmark_previews.py demo/DSCF0656.HEIC /path/to/still-before target/release/still
python3 scripts/benchmark_previews.py demo/DSCF0656.HEIC target/release/still --copies 4 --sizes 1000 --threads 1 2 4 default --primary-only
cargo test --release --bin still benchmark_camera_preview_stages -- --ignored --nocapture
```

The ignored stage benchmark measures one warm-up plus five runs per stage,
asserts the camera-sample PSNR threshold, and does not write images by default.
Set `STILL_BENCH_OUTPUT` to an existing temporary directory to save lossless
before/after PNGs for visual inspection; existing filenames are rejected.
At 2000 pixels the sample's 1920-pixel embedded JPEG is ineligible, so both CLI
flag variants decode the primary image. The stage benchmark consequently tests
embedded resizing only at 1000 pixels.
