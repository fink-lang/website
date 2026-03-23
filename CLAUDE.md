# fink-site — project rules

## Dependencies

All dependency versions are declared in one place:

- **Rust crates**: `Cargo.toml` `[dependencies]`
- **Asset dependencies** (e.g. playground artifact): `Cargo.toml` `[package.metadata.assets.<name>]`

### Updating dependencies

Run `cargo run -- update-deps` to update all dependencies:

1. Runs `cargo update` (Rust crates)
2. Downloads asset dependencies from GitHub releases into `build/assets/`

### Build vs fetch

- `cargo run` (or `cargo run -- build`) builds with what is already in `build/assets/`. It will error if a referenced asset is missing.
- `cargo run -- update-deps` fetches/updates dependencies. It does not build.
- A clean build should never fetch dependencies.

## Build output

```
build/
  assets/     # downloaded dependencies — NOT wiped by build
  site/       # generated output — wiped on each build
```

`build/` is gitignored. The dev server (`cargo run -- serve`) serves from `build/site/`.

## Fragment injection

Content pages can inject HTML fragments from `build/assets/` using frontmatter:

```yaml
fragment: playground/fragment.html   # path relative to build/assets/
asset_dir: playground                # copies runtime files to build/site/<path>
```

The fragment HTML is injected at build time via `{{ fragment | safe }}` in the template.
