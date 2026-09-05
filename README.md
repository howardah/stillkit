# stillkit

A simple Rust CLI tool for classifying files by extension and organizing photos by date.
It also generates previews and provides an interactive image-rating workflow.

## Features

- **Sort by extension** — Files are moved into subdirectories named after their extension.
- **Custom mappings** — Map specific extensions to custom folder names (e.g., `raf:RAW`).
- **Ignore extensions** — Skip moving specific file types or all files.
- **Recursive mode** — Process subdirectories automatically.
- **Interactive rating UI** — Browse images in a terminal UI, preview them, and assign 0-5 star ratings.
- **Cross-platform** — Works on macOS, Linux, and Windows.
- **Version flag** — Automatically shows the version from `Cargo.toml`.

## Installation

### Build from source

```sh
git clone https://github.com/howardah/stillkit.git
cd stillkit
cargo install --path .
```

This installs `still` to your `~/.cargo/bin`, so make sure it’s in your `PATH`.

### Or run without installing

```sh
cargo run -- <directory> [options]
```

## Usage

The explicit subcommands are:

| Command | Description |
| --- | --- |
| `still classify <directory>` | Classify files into extension-based directories. |
| `still organize [directory]` | Organize photos into a date-based hierarchy; defaults to the current directory. |
| `still exposure <inputs...>` | Adjust image exposure in photographic stops. |
| `still previews [directory | images...]` | Generate preview images. |
| `still rate <directory>` | Rate images in the terminal UI. |

The legacy `still <directory>` form remains available for extension classification.

```sh
still <directory> [options]
```

### Options

| Option         | Alias | Description                                                                           |
| -------------- | ----- | ------------------------------------------------------------------------------------- |
| `--extensions` | `-e`  | Map extension to folder name (e.g., `raf:RAW`). Multiple allowed.                     |
| `--ignore`     |       | Ignore specific extensions (e.g., `heic`). Use `all` to ignore all. Multiple allowed. |
| `--recursive`  | `-r`  | Recursively process subdirectories.                                                   |
| `--version`    | `-V`  | Show version from Cargo.toml.                                                         |
| `--help`       | `-h`  | Show help message.                                                                    |

### Examples

**Basic classification**

```sh
still classify ./photos
```

Moves files into folders like `JPG`, `PNG`, `MP4` based on extension.

**Custom mappings**

```sh
still classify ./photos -e raf:RAW -e jpg:JPEGs
```

Moves `.raf` files into `RAW/` and `.jpg` files into `JPEGs/`.

**Ignore some extensions**

```sh
still classify ./photos --ignore heic --ignore all
```

Skips `.heic` files or all files if `all` is specified.

**Recursive classification**

```sh
still classify ./photos -r
```

Classifies all files in `photos/` and its subdirectories.

**Organize photos by date**

```sh
still organize
still organize ./photos
```

Both forms organize photos in the selected directory; the first uses the current directory.

**Rate images in a TUI**

```sh
still rate ./photos
```

Browse images in a terminal UI, preview the selected image on the right, and press `0`-`5` to
rename the file with a star suffix such as `fish_★☆☆☆☆.jpg`.

**Import ratings from another directory**

```sh
still rate import --from ./edited --to ./originals
```

This matches files by basename while ignoring both extension and existing rating suffix, so
`DSCF0655_★☆☆☆☆.webp` in `edited/` will update `DSCF0655.jpg` or `DSCF0655_★★★☆☆.jpg` in
`originals/` to `DSCF0655_★☆☆☆☆.jpg`.

**Generate full-size previews**

```sh
still previews ./photos --full
```

Pass one or more image paths to preview selected files instead of scanning a directory:

```sh
still previews ./photos/one.jpg ./edited/two.CR2
```

For image inputs, the default output is a `preview` directory beside the first image. Files from
other directories are placed in that same output directory. Use `--output` to choose another
location. Directory input continues to default to `<directory>/preview`.

This keeps the original image dimensions and only converts into the selected preview format.
Use `--quality 0..100` (or `-q`) to control JPEG and WebP compression; the default is 75.
By default previews keep the source photo metadata; add `--clear-metadata` to strip it.
Preview generation requires no external programs, including when keeping metadata or using
`--full`. For HEIC/HEIF/HIF and camera RAW, it tries macOS `sips`, ImageMagick 7
`magick`, and ImageMagick 6 `convert`, in that order, before using built-in Rust
decoding. Missing programs and conversion failures continue to the next backend.
This also applies with `--clear-metadata`: decoded pixels pass through the Rust
output encoder, which strips source metadata.
Add `--no-deps` to bypass `sips`, `magick`, `convert`, and `exiftool` entirely,
even when installed. This selects the built-in codec and metadata pipeline for
benchmarking; resized RAW previews may still use their embedded JPEG, so use
`--full` when benchmarking full sensor development.

Metadata copying uses `exiftool` when available, with a Rust fallback for standard
photographic EXIF/GPS fields, supported ICC/XMP profiles, and JPEG/PNG IPTC data.
The fallback updates EXIF dimensions and orientation. Proprietary maker notes,
RAW storage tags, and embedded thumbnails are not copied by the Rust metadata
writer; install ExifTool if preserving proprietary metadata is important.

Camera RAW previews recognize CR2/CR3/CRW, NEF/NRW, ARW/SR2/SRF, RAF, DNG,
ORF, RW2, PEF, SRW, RAW/RWL, 3FR/FFF, IIQ, MOS, MRW, ERF, KDC, and DCR
(case-insensitive). Specific camera models and compression variants must be
supported by at least one available decoder. The built-in RAW decoder is
[`rawler`](https://docs.rs/rawler/0.8.0/rawler/); ImageMagick may additionally use
RAW delegates such as darktable or dcraw, but they are optional.

If external conversion fails, the Rust fallback first uses a sufficiently large
embedded JPEG for resized RAW previews. When none is available, or with `--full`,
it develops sensor pixels with white balance, demosaicing, camera color calibration,
and sRGB conversion. All paths support JPEG, PNG, WebP, quality settings, and the
metadata options. Rust decoding can be slower and use more memory than external
accelerators. Corrupt files and unsupported camera variants can still fail;
absence of external programs alone is not an error.
Batch generation rejects inputs that map to the same output (for example,
`photo.CR2` and `photo.jpg`); process those separately or use distinct names.

**Adjust exposure**

Exposure adjustments use photographic stops: `+1` doubles brightness and `-1` halves it.
No external tools are required. On macOS, HEIC/HEIF/HIF and RAW inputs first try
`sips` decoding followed by built-in pixel adjustment. Otherwise conversion tries
`magick`, then ImageMagick 6 `convert`, then Rust decoding and adjustment. Missing
or failing external programs fall through to the built-in pipeline. `--no-deps`
skips all external programs, including ExifTool for metadata copying.

Exposure multiplies color channel values by `2^stops`, clips them to their output
range, and preserves alpha. The Rust pipeline keeps 16-bit data when the input
and output codecs support it. JPEG output uses quality 90; WebP uses lossless
encoding. Decoder color rendering may differ between backends.

HEIC and camera RAW inputs produce PNG copies (for example, `photo_+1_0.png`),
including with `--original-names`; other supported raster inputs keep their
extensions. The built-in pipeline cannot encode HEIC or camera RAW, so in-place
`--overwrite` rejects those inputs before changing files. This naming policy is
the same with and without external tools. Standard metadata is preserved using
the same optional-ExifTool policy as previews; TIFF also retains standard metadata.
BMP/GIF output does not receive photographic EXIF metadata. Exposure processes
the first frame/image of multi-image containers.

```sh
# Save beside the original as photo_+1_5.jpg
still exposure photo.jpg --adjustment 1.5 --next-to-original

# Overwrite supported raster images in a directory (excluding HEIC/RAW)
still exposure ./photos --adjustment=-0.2 --overwrite

# Apply a ramp from -1.0 to +1.0 across a sorted directory
still exposure ./photos --start=-1 --end=1 --output ./exposed --precision 2

# Keep original names in the output directory instead of adding suffixes
still exposure ./photos --adjustment 0.5 --output ./exposed --original-names

# Compare automatic and built-in pipelines using separate output directories
time still previews photo.CR2 --full --output preview-auto
time still previews photo.CR2 --full --no-deps --output preview-rust
time still exposure photo.HEIC -e 1 --output exposed-auto
time still exposure photo.HEIC -e 1 --no-deps --output exposed-rust
```

Inputs may be individual files, multiple files, or directories. Use `--recursive` for nested
directories. Ramps assign values in sorted input order and include both endpoints. The explicit
`--overwrite` mode replaces inputs; generated files in other modes require `--force` if they already exist.
Colliding output names are rejected before processing. Completed exposure output
is staged before replacing a file, so decoding or metadata failures leave the
original intact.

Rust HEIC decoding is included in the default build. The existing `native-heic`
feature enables the decoder's parallel processing (`cargo install --path . --features native-heic`).
The `heic` dependency is AGPL-or-commercial licensed even without that feature;
`rawler` is LGPL-2.1, and `img-parts` is MIT/Apache-2.0. Review these licenses when
distributing binaries. Rawler adds a camera database and decoding dependencies;
img-parts provides metadata container editing without external libraries.

The Rust decoder uses CPU SIMD, not Apple hardware HEVC decoding. Hardware acceleration for the
Rust path would require a separate VideoToolbox backend with platform FFI and additional codec and
distribution considerations; `sips` is the supported accelerated path on macOS for now.

## Notes

- Extensions are matched case-insensitively.
- Only moves files if there is **more than one unique extension** in the directory (to avoid unnecessary folder creation).
- Uses `std::fs::rename` — moves are instantaneous if on the same filesystem.

## License

MIT License. See [LICENSE](LICENSE) for details.
