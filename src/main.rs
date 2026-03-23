// fink-site builder
//
// Reads content/*.md, renders each through the markdown processor and a Tera
// template, and writes the result to build/site/<slug>/index.html (or
// build/site/index.html for the root page). Static files in static/ are copied
// verbatim.
//
// Downloaded dependencies (e.g. playground artifact) live in build/assets/.
// Only build/site/ is wiped on each build; build/assets/ is preserved.
//
// Content file frontmatter (YAML between --- markers) supports:
//   title: string       — page <title> and heading
//   template: string    — Tera template name without extension (default: "page")
//   fragment: path      — HTML fragment to inject (relative to ASSETS_DIR)
//   asset_dir: path     — copies runtime files from ASSETS_DIR/<path> to output

mod highlight;
mod markdown;
mod serve;
mod svg;

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use tera::{Context as TeraCtx, Tera};
use walkdir::WalkDir;

const CONTENT_DIR: &str = "content";
const TEMPLATE_DIR: &str = "templates";
const STATIC_DIR: &str = "static";
const SITE_DIR: &str = "build/site";
const ASSETS_DIR: &str = "build/assets";
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
      serve::run(SITE_DIR, port)?;
      return Ok(());
    }
    Some("stop") => {
      serve::stop()?;
      return Ok(());
    }
    Some("svg") => {
      let path = args.get(2).expect("Usage: fink-site svg <file.fnk>");
      let src = fs::read_to_string(path)
        .with_context(|| format!("failed to read {path}"))?;
      print!("{}", svg::render_svg(&src));
      return Ok(());
    }
    Some("update-deps") => {
      update_deps()?;
      return Ok(());
    }
    Some("build") | None => {}
    Some(other) => {
      eprintln!("Unknown command: {other}");
      eprintln!("Usage: fink-site [build | serve [port] | stop | update-deps | svg <file>]");
      std::process::exit(1);
    }
  }

  build()
}

fn build() -> Result<()> {
  // Clean and create site dir (preserve assets dir)
  if Path::new(SITE_DIR).exists() {
    fs::remove_dir_all(SITE_DIR).context("failed to remove site dir")?;
  }
  fs::create_dir_all(SITE_DIR).context("failed to create site dir")?;

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

  println!("Build complete → {}/", SITE_DIR);
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

  let toc = markdown::extract_toc(body);
  let fragment_path = meta.remove("fragment");
  let asset_dir = meta.remove("asset_dir");
  let page = Page {
    title: meta.remove("title"),
    template: meta.remove("template").unwrap_or_else(|| "page".to_string()),
    body_html: markdown::render(body),
    meta,
  };

  // Determine output path:
  //   content/index.md    → build/site/index.html
  //   content/foo.md      → build/site/foo/index.html
  //   content/docs/bar.md → build/site/docs/bar/index.html
  let rel = md_path.strip_prefix(CONTENT_DIR).unwrap();
  let out_path = output_path(rel);

  fs::create_dir_all(out_path.parent().unwrap())?;

  // Calculate root-relative prefix based on output depth.
  // build/site/index.html        → "./"
  // build/site/docs/index.html   → "../"
  let depth = out_path
    .strip_prefix(SITE_DIR).unwrap()
    .parent().unwrap()
    .components().count();
  let root = if depth == 0 { "./".to_string() } else { "../".repeat(depth) };

  // Load HTML fragment from assets if requested
  let fragment = if let Some(frag_path) = &fragment_path {
    let full = Path::new(ASSETS_DIR).join(frag_path);
    let html = fs::read_to_string(&full)
      .with_context(|| format!("failed to read fragment {}", full.display()))?;
    Some(html)
  } else {
    None
  };

  // Copy runtime asset directory if requested (e.g. JS/WASM/CSS files)
  if let Some(dir) = &asset_dir {
    let src = Path::new(ASSETS_DIR).join(dir);
    let dest_dir = Path::new(SITE_DIR).join(dir);
    if src.is_dir() {
      copy_dir(&src, &dest_dir)
        .with_context(|| format!("failed to copy asset dir {}", src.display()))?;
    }
  }

  let template_name = format!("{}.html", page.template);
  let mut ctx = TeraCtx::new();
  ctx.insert("title", &page.title);
  ctx.insert("body", &page.body_html);
  ctx.insert("meta", &page.meta);
  ctx.insert("root", &root);
  if let Some(ref frag) = fragment {
    ctx.insert("fragment", frag);
  }
  let toc_items: Vec<HashMap<String, String>> = toc.iter()
    .map(|e| {
      let mut m = HashMap::new();
      m.insert("text".to_string(), e.text.clone());
      m.insert("slug".to_string(), e.slug.clone());
      m
    })
    .collect();
  ctx.insert("toc", &toc_items);

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
    // content/index.md → build/site/index.html
    // content/404.md   → build/site/404.html  (GitHub Pages 404 handler)
    // content/docs/index.md → build/site/docs/index.html
    Path::new(SITE_DIR).join(parent).join(format!("{stem}.html"))
  } else {
    // content/foo.md → build/site/foo/index.html
    Path::new(SITE_DIR).join(parent).join(stem).join("index.html")
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
    let dest = Path::new(SITE_DIR).join(rel);
    fs::create_dir_all(dest.parent().unwrap())?;
    fs::copy(entry.path(), &dest)?;
  }
  Ok(())
}

/// Recursively copy a directory tree, excluding index.html (generated by the builder).
fn copy_dir(src: &Path, dest: &Path) -> Result<()> {
  for entry in WalkDir::new(src)
    .into_iter()
    .filter_map(|e| e.ok())
    .filter(|e| e.file_type().is_file())
  {
    let name = entry.file_name().to_str().unwrap_or("");
    if name == "index.html" { continue; }
    let rel = entry.path().strip_prefix(src).unwrap();
    let dest_file = dest.join(rel);
    fs::create_dir_all(dest_file.parent().unwrap())?;
    fs::copy(entry.path(), &dest_file)?;
  }
  Ok(())
}

// ---- dependency management -------------------------------------------------

/// Run `cargo update` and download asset dependencies listed in Cargo.toml
/// under [package.metadata.assets.<name>].
fn update_deps() -> Result<()> {
  // 1. Run cargo update for crate dependencies
  println!("Running cargo update…");
  let status = std::process::Command::new("cargo")
    .arg("update")
    .status()
    .context("failed to run cargo update")?;
  if !status.success() {
    anyhow::bail!("cargo update failed");
  }

  // 2. Download asset dependencies from Cargo.toml metadata
  let toml_str = fs::read_to_string("Cargo.toml")
    .context("failed to read Cargo.toml")?;

  // Parse [package.metadata.assets.*] entries.
  // Each has: url (with {version} placeholder), version, dest
  let mut in_asset: Option<String> = None;
  let mut assets: Vec<(String, String, String, String)> = vec![]; // (name, url, version, dest)
  let mut url = String::new();
  let mut version = String::new();
  let mut dest = String::new();

  for line in toml_str.lines() {
    let trimmed = line.trim();
    if let Some(rest) = trimmed.strip_prefix("[package.metadata.assets.") {
      // Save previous asset if any
      if let Some(name) = in_asset.take() {
        if !url.is_empty() && !version.is_empty() && !dest.is_empty() {
          assets.push((name, url.clone(), version.clone(), dest.clone()));
        }
      }
      in_asset = Some(rest.trim_end_matches(']').to_string());
      url.clear();
      version.clear();
      dest.clear();
    } else if in_asset.is_some() {
      if let Some((k, v)) = trimmed.split_once('=') {
        let k = k.trim();
        let v = v.trim().trim_matches('"');
        match k {
          "url" => url = v.to_string(),
          "version" => version = v.to_string(),
          "dest" => dest = v.to_string(),
          _ => {}
        }
      }
    }
  }
  if let Some(name) = in_asset {
    if !url.is_empty() && !version.is_empty() && !dest.is_empty() {
      assets.push((name, url, version, dest));
    }
  }

  for (name, url_template, ver, dest_dir) in &assets {
    let resolved_url = url_template.replace("{version}", ver);
    println!("Fetching {name} {ver}…");

    // Clean and recreate dest
    if Path::new(dest_dir).exists() {
      fs::remove_dir_all(dest_dir)?;
    }
    fs::create_dir_all(dest_dir)?;

    // Download and extract tarball
    let status = std::process::Command::new("curl")
      .args(["-sL", &resolved_url])
      .stdout(std::process::Stdio::piped())
      .spawn()
      .context("failed to run curl")?;

    let tar_status = std::process::Command::new("tar")
      .args(["-xzf", "-", "-C", dest_dir])
      .stdin(status.stdout.unwrap())
      .status()
      .context("failed to run tar")?;

    if !tar_status.success() {
      anyhow::bail!("failed to download/extract {name} from {resolved_url}");
    }
    println!("  → {dest_dir}/");
  }

  if assets.is_empty() {
    println!("No asset dependencies found in Cargo.toml");
  }

  Ok(())
}
