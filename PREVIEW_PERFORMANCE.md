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

Pending implementation and measurement. No additional speedup is claimed yet.
