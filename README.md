# rmapbash
## Minecraft map renderer

Reads a saved Minecraft world from disk and outputs a rendered .PNG image.

![Isometric day mode](./samples/iso-day.png?raw=true)

![Orthographic night mode](./samples/ortho-night.png?raw=true)

### Features so far

- Orthographic (top-down) or isometric (oblique) viewing angle.
- Day or night lighting mode.
- Nether and End supported; just point to the `DIM-1` or `DIM1` subdir of the save dir.
- Render part of a world by passing two coordinates to use as a bounding box;
  e.g. `-b 10 20 200 400` to render only the area between (10, 20) and (200, 400).

```
USAGE:
    rmapbash [FLAGS] [OPTIONS] <PATH>

FLAGS:
    -h, --help         Prints help information
    -i, --isometric    Isometric view
    -n, --night        Night lighting
    -V, --version      Prints version information

OPTIONS:
    -b, --blocks <b> <b> <b> <b>    Block limits

ARGS:
    <PATH>    Path to either a save directory or a .dat file
```

### About

This is my first project in Rust; I've been using it to learn the language.

It's a reimplementation of my first C project, [cmapbash](https://github.com/saltire/cmapbash),
which is what I used to learn *that* language.