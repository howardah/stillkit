# `photos` Subcommand — Implementation Plan

## Overview

The `photos` subcommand automates organizing camera photos into a date-based directory hierarchy. It has two modes depending on whether an input (`-i`) is supplied:

- **Sort mode** (no input): reorganize photos already sitting in the root of the output directory.
- **Import mode** (with input): move photos from an input directory into the output, merging with any existing structure.

---

## CLI Shape

```
photool photos [OUTPUT] [-o OUTPUT] [-i INPUT] [-r] [--dry-run]
```

| Argument              | Description                                                                                                 |
| --------------------- | ----------------------------------------------------------------------------------------------------------- |
| `OUTPUT` (positional) | Destination directory. Defaults to current directory.                                                       |
| `-o` / `--output`     | Alternative way to specify destination. Conflicts with positional arg.                                      |
| `-i` / `--input`      | Source directory. When absent the tool runs in sort mode.                                                   |
| `-r` / `--recursive`  | Recurse into subdirectories of the input. Without this flag only the top level of the input dir is scanned. |
| `--dry-run`           | Print intended actions; do not move or create anything.                                                     |

The existing extension-based sorter remains the default `photool <directory>` invocation for backwards compatibility. A `sort` subcommand alias is also added so `photool sort <directory>` works identically.

---

## Output Directory Structure

```
<output>/
  2026 - 04 April/          ← month dir  (YYYY - MM MMMM)
    2026-04-15/             ← day dir (single day)
      IMG_001.jpg
      IMG_002.heic
      IMG_003.hif
      RAW/
        IMG_004.raf
    2026-04-20 - 2026-04-21/  ← day-range dir (date range)
      IMG_010.jpg
      RAW/
        IMG_011.raf
```

### Month directory format

`YYYY - MM MMMM` — zero-padded two-digit month number followed by the full English month name.  
Example: `2026 - 04 April`, `2025 - 12 December`.

### Day subdirectory format

- **Single day**: `YYYY-MM-DD`
- **Date range**: `YYYY-MM-DD - YYYY-MM-DD`

A group is formed from photos whose dates are no more than 2 calendar days apart from the adjacent date in the sorted sequence. If the gap between two adjacent dates exceeds 2 days, a new group begins. Days are grouped per month — a sequence spanning a month boundary produces a separate dir in each month.

### File placement

| Extension                                                                                                                  | Destination within the day dir                                                   |
| -------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------- |
| `.jpg`, `.jpeg`, `.heic`, `.heif`, `.hif`                                                                                   | Day dir root                                                                     |
| `.mp4`, `.mov`                                                                                                             | Day dir root                                                                     |
| `.raf`, `.cr2`, `.nef`, `.arw`, `.dng`, `.rw2`, and other non-listed types _when JPG/HEIC/HEIF/HIF/MP4/MOV also exist in the group_ | `RAW/` sub-subdirectory                                                          |
| Any non-listed types _when no JPG/HEIC/MP4/MOV exist in the group_                                                         | Day dir root                                                                     |
| Multiple differing non-listed extensions in the same group                                                                 | Sorted into per-extension subdirectories using the existing extension-sort logic |

The RAW subdir only exists when there are "primary" files (JPG/HEIC/HEIF/HIF/MP4/MOV) to distinguish from. If a group contains only raw files, they are placed in the day dir root. If a group contains a mix of unlisted types (e.g. `.raf` and `.xmp`), those types each get their own subdir via the existing extension-sort logic.

---

## Date Extraction

1. Parse **EXIF `DateTimeOriginal`** (tag 0x9003) — most reliable for camera photos.
2. Fall back to **EXIF `DateTime`** (tag 0x0132).
3. Fall back to **file system modification time** (useful for RAW files whose EXIF is embedded differently).
4. If no date can be determined, print a warning and skip the file.

### Dependencies to add

```toml
exif = "0.5"      # kamadak-exif — pure-Rust EXIF reader, handles JPEG/HEIF/TIFF-based RAW
chrono = "0.4"    # date arithmetic and formatting
```

---

## Algorithm Details

### Sort mode (no `-i`)

1. Scan files in the **root** of the output directory only (non-recursive); ignore existing subdirectories.
2. Extract the date for each file.
3. Group files by month.
4. Within each month, sort files by date and compute clusters (gap ≤ 2 days between adjacent dates).
5. Determine the day-dir name (single date or range) for each cluster.
6. Determine file placement within the day dir (root vs `RAW/` vs per-extension subdir) based on the mix of types present in the cluster.
7. In dry-run: print each planned move. Otherwise:
   - Create `<output>/<month-dir>/<day-dir>/` (and subdirs as needed).
   - Move each file to its destination.

### Import mode (with `-i`)

1. Scan files in the input directory. If `-r` is passed, walk recursively; otherwise scan the top level only.
2. Extract the date for each file.
3. For each file, determine its target month dir.
4. **Check for an existing day dir** in `<output>/<month-dir>/`. A dir matches if either:
   - it contains at least one photo with the **same calendar date**, or
   - the incoming photo's date falls **strictly between** the earliest and latest dates of photos already in that dir (i.e. the photo slots into an existing range even if no photo shares its exact date).

   If a match is found, use that dir regardless of its name.

5. If no matching existing dir is found, batch newly-imported files by clusters (same gap ≤ 2 days logic) to derive a new day-dir name.
6. **Duplicate detection**: if a file with the **same name** already exists in the target dir and its EXIF date matches, skip it and leave it in the input directory. Print a notice.
7. In dry-run: print each planned move/skip. Otherwise move files.

#### Finding the matching existing day dir

For each candidate subdirectory under `<output>/<month-dir>/`:

- Scan its root and any `RAW/` subdir for photo files.
- Collect their EXIF dates and derive a `(min_date, max_date)` range.
- The dir **matches** the incoming file if:
  - the incoming date equals any date already present in the dir, **or**
  - `min_date < incoming_date < max_date` (the date slots into the existing span).

This scan is done lazily per month, building a `(min_date, max_date, path) → existing_dir` map once per month, then reused for all files targeting that month.

---

## Dry-Run Output Format

```
[DRY RUN] Would move:
  /Volumes/Camera/DCIM/IMG_001.jpg
    → /Photos/2026 - 04 April/2026-04-15/IMG_001.jpg

  /Volumes/Camera/DCIM/IMG_002.raf
    → /Photos/2026 - 04 April/2026-04-15/RAW/IMG_002.raf

[DRY RUN] Would skip (already exists):
  /Volumes/Camera/DCIM/IMG_003.jpg  (same name and date found in Hiking Trip/)
```

---

## Example Scenarios

### Scenario 1 — Sort mode, single day

**Before** (`/Photos/` root):

```
IMG_001.jpg   (EXIF: 2026-04-15)
IMG_002.heic  (EXIF: 2026-04-15)
IMG_003.raf   (EXIF: 2026-04-15)
```

**After**:

```
2026 - 04 April/
  2026-04-15/
    IMG_001.jpg
    IMG_002.heic
    RAW/
      IMG_003.raf
```

---

### Scenario 2 — Sort mode, gap tolerance grouping

Photos within 2 days of each other are grouped; a gap of 3+ days starts a new group.

**Before** (`/Photos/` root):

```
IMG_001.jpg   (EXIF: 2026-04-15)
IMG_002.jpg   (EXIF: 2026-04-17)   ← 2 days after 15th → same group
IMG_003.jpg   (EXIF: 2026-04-20)   ← 3 days after 17th → new group
IMG_004.jpg   (EXIF: 2026-04-21)
```

**After**:

```
2026 - 04 April/
  2026-04-15 - 2026-04-17/
    IMG_001.jpg
    IMG_002.jpg
  2026-04-20 - 2026-04-21/
    IMG_003.jpg
    IMG_004.jpg
```

---

### Scenario 3 — Sort mode, RAW-only group

When a group contains no JPG/HEIC/MP4/MOV files, raw files are placed in the day dir root rather than a `RAW/` subdir.

**Before** (`/Photos/` root):

```
IMG_001.raf  (EXIF: 2026-04-15)
IMG_002.raf  (EXIF: 2026-04-15)
```

**After**:

```
2026 - 04 April/
  2026-04-15/
    IMG_001.raf
    IMG_002.raf
```

---

### Scenario 4 — Sort mode, mixed non-listed types

When a group has multiple different unlisted extension types alongside primary files, non-primary types each get a per-extension subdir.

**Before** (`/Photos/` root):

```
IMG_001.jpg  (EXIF: 2026-04-15)
IMG_001.raf  (EXIF: 2026-04-15)
IMG_001.xmp  (EXIF: 2026-04-15)
VID_001.mp4  (EXIF: 2026-04-15)
```

**After**:

```
2026 - 04 April/
  2026-04-15/
    IMG_001.jpg
    VID_001.mp4
    RAW/
      IMG_001.raf
    XMP/
      IMG_001.xmp
```

---

### Scenario 5 — Sort mode, month boundary

Photos from March 31 and April 1 are never grouped across a month boundary.

**Before** (`/Photos/` root):

```
IMG_001.jpg  (EXIF: 2026-03-31)
IMG_002.jpg  (EXIF: 2026-04-01)
```

**After**:

```
2026 - 03 March/
  2026-03-31/
    IMG_001.jpg
2026 - 04 April/
  2026-04-01/
    IMG_002.jpg
```

---

### Scenario 6 — Import mode, merging into a named existing dir

**Existing output** (`/Photos/`):

```
2026 - 04 April/
  Hiking Trip/
    IMG_001.jpg  (EXIF: 2026-04-15)
    IMG_002.heic (EXIF: 2026-04-16)
    RAW/
      IMG_003.raf (EXIF: 2026-04-15)
```

**Input** (`/Volumes/Camera/`):

```
IMG_010.jpg  (EXIF: 2026-04-15)
IMG_011.raf  (EXIF: 2026-04-16)
```

**After** (files moved into the existing named dir):

```
2026 - 04 April/
  Hiking Trip/
    IMG_001.jpg
    IMG_002.heic
    IMG_010.jpg   ← imported
    RAW/
      IMG_003.raf
      IMG_011.raf  ← imported
```

`Hiking Trip/` is matched because it already contains photos from 2026-04-15 and 2026-04-16.

---

### Scenario 7 — Import mode, date falls within existing dir's span

**Existing output** (`/Photos/`):

```
2026 - 04 April/
  Hiking Trip/
    IMG_001.jpg  (EXIF: 2026-04-14)
    IMG_003.jpg  (EXIF: 2026-04-16)   ← note: no photo on the 15th
```

**Input**:

```
IMG_002.jpg  (EXIF: 2026-04-15)   ← no exact match, but 14th < 15th < 16th
```

**After** (photo placed in `Hiking Trip/` because its date falls within the existing span):

```
2026 - 04 April/
  Hiking Trip/
    IMG_001.jpg
    IMG_002.jpg   ← imported
    IMG_003.jpg
```

---

### Scenario 8 — Import mode, new photos alongside an existing dir

**Existing output** (`/Photos/`):

```
2026 - 04 April/
  Hiking Trip/
    IMG_001.jpg  (EXIF: 2026-04-15)
```

**Input**:

```
IMG_020.jpg  (EXIF: 2026-04-20)
IMG_021.jpg  (EXIF: 2026-04-21)
```

No existing dir covers the 20th–21st, so a new range dir is created:

**After**:

```
2026 - 04 April/
  Hiking Trip/
    IMG_001.jpg
  2026-04-20 - 2026-04-21/
    IMG_020.jpg
    IMG_021.jpg
```

---

### Scenario 9 — Import mode, duplicate skip

**Existing output** (`/Photos/`):

```
2026 - 04 April/
  2026-04-15/
    IMG_001.jpg  (EXIF: 2026-04-15)
```

**Input**:

```
IMG_001.jpg  (EXIF: 2026-04-15)   ← same name, same date → skip
IMG_002.jpg  (EXIF: 2026-04-15)   ← same date, different name → import
```

**After**:

- `IMG_001.jpg` in input is **left in place** — name and date already present.
- `IMG_002.jpg` is moved into `2026-04-15/`.

---

### Scenario 10 — Import mode, cross-month batch

**Input**:

```
IMG_001.jpg  (EXIF: 2026-03-30)
IMG_002.jpg  (EXIF: 2026-03-31)
IMG_003.jpg  (EXIF: 2026-04-01)
IMG_004.jpg  (EXIF: 2026-04-02)
```

The gap from March 31 → April 1 crosses a month boundary, so they produce separate dirs even though they are only 1 day apart:

**After**:

```
2026 - 03 March/
  2026-03-30 - 2026-03-31/
    IMG_001.jpg
    IMG_002.jpg
2026 - 04 April/
  2026-04-01 - 2026-04-02/
    IMG_003.jpg
    IMG_004.jpg
```

---

## Implementation Structure

```
src/
  main.rs          — CLI definition and dispatch (adds `photos` + `sort` alias)
  sort.rs          — existing extension-sorter logic (extracted from main.rs)
  photos/
    mod.rs         — entry point: parse args, call sort or import
    date.rs        — EXIF date extraction helpers
    group.rs       — date cluster logic (gap ≤ 2 days, per-month)
    layout.rs      — month/day dir naming, file placement routing
    scan.rs        — scanning output for existing date→dir mapping
    ops.rs         — file move / dry-run print operations
```
