# stillkit

A simple Rust CLI tool for sorting files into directories based on their file extensions.
Supports custom folder names, ignored extensions, and recursive sorting.

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

**Basic sorting**

```sh
still ./photos
```

Moves files into folders like `JPG`, `PNG`, `MP4` based on extension.

**Custom mappings**

```sh
still ./photos -e raf:RAW -e jpg:JPEGs
```

Moves `.raf` files into `RAW/` and `.jpg` files into `JPEGs/`.

**Ignore some extensions**

```sh
still ./photos --ignore heic --ignore all
```

Skips `.heic` files or all files if `all` is specified.

**Recursive sorting**

```sh
still ./photos -r
```

Sorts all files in `photos/` and its subdirectories.

**Rate images in a TUI**

```sh
pht rate ./photos
```

Browse images in a terminal UI, preview the selected image on the right, and press `0`-`5` to
rename the file with a star suffix such as `fish_★☆☆☆☆.jpg`.

**Import ratings from another directory**

```sh
pht rate import --from ./edited --to ./originals
```

This matches files by basename while ignoring both extension and existing rating suffix, so
`DSCF0655_★☆☆☆☆.webp` in `edited/` will update `DSCF0655.jpg` or `DSCF0655_★★★☆☆.jpg` in
`originals/` to `DSCF0655_★☆☆☆☆.jpg`.

**Generate full-size previews**

```sh
pht preview ./photos --full
```

This keeps the original image dimensions and only converts into the selected preview format.
By default previews keep the source photo metadata; add `--clear-metadata` to strip it.
Metadata-preserving previews use `exiftool`, and HEIC/HEIF/HIF previews use ImageMagick's `magick` when available.

## Notes

- Extensions are matched case-insensitively.
- Only moves files if there is **more than one unique extension** in the directory (to avoid unnecessary folder creation).
- Uses `std::fs::rename` — moves are instantaneous if on the same filesystem.

## License

MIT License. See [LICENSE](LICENSE) for details.
