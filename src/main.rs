// fink-site builder
//
// Reads content/*.md, renders each through the markdown processor and a Tera
// template, and writes the result to build/<slug>/index.html (or build/index.html
// for the root page). Static files in static/ are copied verbatim.
//
// Content file frontmatter (YAML between --- markers) supports:
//   title: string    — page <title> and heading
//   template: string — Tera template name without extension (default: "page")

mod highlight;
mod markdown;
mod serve;

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use tera::{Context as TeraCtx, Tera};
use walkdir::WalkDir;

const CONTENT_DIR: &str = "content";
const TEMPLATE_DIR: &str = "templates";
const STATIC_DIR: &str = "static";
const BUILD_DIR: &str = "build";
const DEFAULT_PORT: u16 = 8080;

fn main() -> Result<()> {
  let args: Vec<String> = std::env::args().collect();
  let cmd = args.get(1).map(|s| s.as_str());

  match cmd {
    Some("serve") => {
      let port = args.get(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_PORT);
      build()?;
      serve::run(BUILD_DIR, port)?;
      return Ok(());
    }
    Some("stop") => {
      serve::stop()?;
      return Ok(());
    }
    Some("build") | None => {}
    Some(other) => {
      eprintln!("Unknown command: {other}");
      eprintln!("Usage: fink-site [build | serve [port] | stop]");
      std::process::exit(1);
    }
  }

  build()
}

fn build() -> Result<()> {
  // Clean and create build dir
  if Path::new(BUILD_DIR).exists() {
    fs::remove_dir_all(BUILD_DIR).context("failed to remove build dir")?;
  }
  fs::create_dir_all(BUILD_DIR).context("failed to create build dir")?;

  // Load templates
  let pattern = format!("{}/**/*.html", TEMPLATE_DIR);
  let tera = Tera::new(&pattern).context("failed to load templates")?;

  // Render content pages
  for entry in WalkDir::new(CONTENT_DIR)
    .into_iter()
    .filter_map(|e| e.ok())
    .filter(|e| e.path().extension().map(|x| x == "md").unwrap_or(false))
  {
    render_page(entry.path(), &tera)?;
  }

  // Copy static assets
  copy_static()?;

  println!("Build complete → {}/", BUILD_DIR);
  Ok(())
}

struct Page {
  title: Option<String>,
  template: String,
  body_html: String,
  /// Extra front-matter fields passed through to the template
  meta: HashMap<String, String>,
}

fn parse_frontmatter(source: &str) -> (HashMap<String, String>, &str) {
  let mut meta = HashMap::new();

  let body = if let Some(rest) = source.strip_prefix("---\n") {
    if let Some(end) = rest.find("\n---\n") {
      let fm = &rest[..end];
      let body = &rest[end + 5..]; // skip "\n---\n"
      for line in fm.lines() {
        if let Some((k, v)) = line.split_once(": ") {
          meta.insert(k.trim().to_string(), v.trim().to_string());
        }
      }
      body
    } else {
      source
    }
  } else {
    source
  };

  (meta, body)
}

fn render_page(md_path: &Path, tera: &Tera) -> Result<()> {
  let source = fs::read_to_string(md_path)
    .with_context(|| format!("failed to read {}", md_path.display()))?;

  let (mut meta, body) = parse_frontmatter(&source);

  let page = Page {
    title: meta.remove("title"),
    template: meta.remove("template").unwrap_or_else(|| "page".to_string()),
    body_html: markdown::render(body),
    meta,
  };

  // Determine output path:
  //   content/index.md    → build/index.html
  //   content/foo.md      → build/foo/index.html
  //   content/docs/bar.md → build/docs/bar/index.html
  let rel = md_path.strip_prefix(CONTENT_DIR).unwrap();
  let out_path = output_path(rel);

  fs::create_dir_all(out_path.parent().unwrap())?;

  // Calculate root-relative prefix based on output depth.
  // build/index.html        → "./"
  // build/docs/index.html   → "../"
  // build/a/b/index.html    → "../../"
  let depth = out_path
    .strip_prefix(BUILD_DIR).unwrap()
    .parent().unwrap()
    .components().count();
  let root = if depth == 0 { "./".to_string() } else { "../".repeat(depth) };

  let template_name = format!("{}.html", page.template);
  let mut ctx = TeraCtx::new();
  ctx.insert("title", &page.title);
  ctx.insert("body", &page.body_html);
  ctx.insert("meta", &page.meta);
  ctx.insert("root", &root);

  let rendered = tera
    .render(&template_name, &ctx)
    .with_context(|| format!("template render failed for {}", md_path.display()))?;

  fs::write(&out_path, rendered)
    .with_context(|| format!("failed to write {}", out_path.display()))?;

  println!("  {}", out_path.display());
  Ok(())
}

fn output_path(rel: &Path) -> PathBuf {
  let stem = rel.file_stem().unwrap().to_str().unwrap();
  let parent = rel.parent().unwrap();

  if stem == "index" || stem == "404" {
    // content/index.md → build/index.html
    // content/404.md   → build/404.html  (GitHub Pages 404 handler)
    // content/docs/index.md → build/docs/index.html
    Path::new(BUILD_DIR).join(parent).join(format!("{stem}.html"))
  } else {
    // content/foo.md → build/foo/index.html
    Path::new(BUILD_DIR).join(parent).join(stem).join("index.html")
  }
}

fn copy_static() -> Result<()> {
  if !Path::new(STATIC_DIR).exists() {
    return Ok(());
  }
  for entry in WalkDir::new(STATIC_DIR)
    .into_iter()
    .filter_map(|e| e.ok())
    .filter(|e| e.file_type().is_file())
  {
    let rel = entry.path().strip_prefix(STATIC_DIR).unwrap();
    let dest = Path::new(BUILD_DIR).join(rel);
    fs::create_dir_all(dest.parent().unwrap())?;
    fs::copy(entry.path(), &dest)?;
  }
  Ok(())
}
