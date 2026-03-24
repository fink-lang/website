# fink-site — project rules

## Dependencies

All dependency versions are declared in one place:

- **Rust crates**: `Cargo.toml` `[dependencies]`
- **Asset dependencies** (e.g. playground, brand): `Cargo.toml` `[package.metadata.assets.<name>]`

### Updating dependencies

Run `cargo run -- update-deps` to update all dependencies:

1. Runs `cargo update` (Rust crates)
2. Downloads asset dependencies from GitHub releases into `.deps/`

### Build vs fetch

- `cargo run` (or `cargo run -- build`) builds with what is already in `.deps/`. It will error if a referenced asset is missing.
- `cargo run -- update-deps` fetches/updates dependencies. It does not build.
- A clean build should never fetch dependencies.

## Project layout

```
.deps/              # downloaded dependencies — NOT wiped by build, gitignored
  playground/       # playground artifact (JS/WASM/CSS)
  brand/            # brand assets (logo SVGs, favicon PNGs, social image)
build/
  site/             # generated output — wiped on each build
static/             # static files copied verbatim to build/site/
                    # (brand assets here are fallbacks, overridden by .deps/brand/)
```

The dev server (`cargo run -- serve`) serves from `build/site/`.

## Brand assets

Brand assets (logo, favicons, social image) come from the `brand` dependency.
The build copies them from `.deps/brand/assets/` into `build/site/`, overriding
any fallback files in `static/`. Once the brand release is live, the fallback
copies in `static/` can be removed.

## Fragment injection

Content pages can inject HTML fragments from `.deps/` using frontmatter:

```yaml
fragment: playground/fragment.html   # path relative to .deps/
asset_dir: playground                # copies runtime files to build/site/<path>
```

The fragment HTML is injected at build time via `{{ fragment | safe }}` in the template.
