# fink-site — project rules

## Commands

All commands are available via `make`:

| Target | Description |
|---|---|
| `make deps-check` | Check for outdated dependencies (asset releases + cargo crates) |
| `make deps-update` | Update all dependencies (`cargo update` + re-fetch assets) |
| `make deps-install` | Fetch pinned asset dependencies to `.deps/` (no `cargo update`) |
| `make clean` | Remove `build/` |
| `make build` | Build the site (`--release`) |
| `make test` | Verify expected output files exist (assumes prior `build`) |
| `make serve` | Build + start dev server at http://localhost:8080/ |

## Dependencies

All dependency versions are declared in one place:

- **Rust crates**: `Cargo.toml` `[dependencies]`
- **Asset dependencies** (e.g. playground, brand): `Cargo.toml` `[package.metadata.assets.<name>]`

### Build vs fetch

- `make build` builds with what is already in `.deps/`. It will error if a referenced asset is missing.
- `make deps-install` fetches pinned assets. `make deps-update` also runs `cargo update`.
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

The dev server (`make serve`) serves from `build/site/`.

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

## Session wrap-up

Before wrapping up, always kill the dev server if it's running (e.g. `lsof -ti:8080 | xargs kill`).
