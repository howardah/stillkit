# HEIC regression fixture

`gradient-422-10bit.heic` is a synthetic 64×64 red-to-blue gradient, encoded
as 10-bit HEVC with 4:2:2 chroma. It exercises the camera-image decoding path
that returned corrupted pixels with `heic` 0.1.x. It contains no photographic content
or source photo metadata. Its color regression runs in the normal test suite.

Generated with ImageMagick/libheif:

```sh
magick -size 64x64 gradient:red-blue -depth 10 -define heic:chroma=422 -quality 90 tests/fixtures/gradient-422-10bit.heic
```

The full-size camera regression uses the local, untracked `demo/DSCF0656.HEIC`:

```sh
cargo test --release --test previews_without_tools heic_no_deps_preserves_image_content -- --ignored
```

It checks regional colors against an independent ImageMagick/libheif decode,
with external programs disabled during preview generation. Add
`--features native-heic` before `--test` to exercise parallel decoding.
