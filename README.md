# fink-lang.org

Source for the [fink-lang.org](https://fink-lang.org) website.

## Overview

A custom static site generator written in Rust. Reads Markdown content with YAML
frontmatter, renders it through Tera templates with fink syntax highlighting,
and outputs plain HTML/CSS to `build/`.

The [playground](/playground/) is embedded as a pre-built artifact from
[fink-lang/playground](https://github.com/fink-lang/playground).

## Project structure

```
content/        Markdown pages (frontmatter: title, template)
templates/      Tera HTML templates
static/         Assets copied verbatim to build/
src/            Rust source for the site builder
build/          Generated output (git-ignored)
```

## Development

```sh
# Build the site
cargo run

# Build and serve locally (default port 8080)
cargo run -- serve
cargo run -- serve 3000

# Stop the dev server
cargo run -- stop
```

## Deployment

Handled by GitHub Actions (`.github/workflows/ci.yml`):

- Every push/PR: build and verify output
- Push to `main`: deploy to GitHub Pages
