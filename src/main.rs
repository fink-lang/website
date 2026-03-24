// fink-site builder
//
// Reads content/*.md, renders each through the markdown processor and a Tera
// template, and writes the result to build/site/<slug>/index.html (or
// build/site/index.html for the root page). Static files in static/ are copied
// verbatim.
//
// Downloaded dependencies (e.g. playground, brand) live in .deps/.
// Only build/site/ is wiped on each build; .deps/ is preserved.
//
// Content file frontmatter (YAML between --- markers) supports:
//   title: string       — page <title> and heading
//   template: string    — Tera template name without extension (default: "page")
//   fragment: path      — HTML fragment to inject (relative to .deps/)
//   asset_dir: path     — copies runtime files from .deps/<path> to output

mod highlight;
mod markdown;
mod playground;
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
const DEPS_DIR: &str = ".deps";
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
    Some("install-deps") => {
      install_deps()?;
      return Ok(());
    }
    Some("update-deps") => {
      update_deps()?;
      return Ok(());
    }
    Some("check-deps") => {
      check_deps()?;
      return Ok(());
    }
    Some("clean") => {
      clean()?;
      return Ok(());
    }
    Some("build") | None => {}
    Some(other) => {
      eprintln!("Unknown command: {other}");
      eprintln!("Usage: fink-site [build | serve [port] | stop | install-deps | update-deps | check-deps | clean | svg <file>]");
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

  // Copy brand assets from .deps/brand/ into build/site/
  copy_brand()?;

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
    let full = Path::new(DEPS_DIR).join(frag_path);
    let html = fs::read_to_string(&full)
      .with_context(|| format!("failed to read fragment {}", full.display()))?;
    Some(html)
  } else {
    None
  };

  // Copy runtime asset directory if requested (e.g. JS/WASM/CSS files)
  if let Some(dir) = &asset_dir {
    let src = Path::new(DEPS_DIR).join(dir);
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

// ---- brand assets ----------------------------------------------------------

const BRAND_DIR: &str = ".deps/brand";

/// Copy brand assets from .deps/brand/ into the site output directory.
/// SVGs are copied directly; PNGs are downscaled to the sizes needed for
/// favicons, apple-touch-icon, and social image.
fn copy_brand() -> Result<()> {
  let brand = Path::new(BRAND_DIR);
  if !brand.exists() {
    // Brand dep not fetched — static/ fallbacks will be used
    return Ok(());
  }

  let site = Path::new(SITE_DIR);

  // Logo SVG (nav, hero, SVG favicon)
  let logo_src = brand.join("assets/fink-rect-no-bg.svg");
  anyhow::ensure!(logo_src.exists(), "required brand asset missing: {}", logo_src.display());
  fs::copy(&logo_src, site.join("logo.svg"))?;

  // Favicons downscaled from the no-bg PNG (transparent)
  let icon_src = brand.join("assets/fink-rect-no-bg.png");
  anyhow::ensure!(icon_src.exists(), "required brand asset missing: {}", icon_src.display());
  downscale_png(&icon_src, &site.join("favicon-32.png"), 32)?;
  downscale_png(&icon_src, &site.join("favicon-192.png"), 192)?;

  // Apple touch icon from rounded variant (iOS masks transparent icons to black)
  let apple_src = brand.join("assets/fink-rounded.png");
  anyhow::ensure!(apple_src.exists(), "required brand asset missing: {}", apple_src.display());
  downscale_png(&apple_src, &site.join("apple-touch-icon.png"), 180)?;

  // Social / og:image from wordmark (already 1200×630)
  let social_src = brand.join("assets/fink-wordmark.png");
  anyhow::ensure!(social_src.exists(), "required brand asset missing: {}", social_src.display());
  fs::copy(&social_src, site.join("social.png"))?;

  Ok(())
}

/// Downscale a PNG to a square of the given size using Lanczos3 filtering.
fn downscale_png(src: &Path, dest: &Path, size: u32) -> Result<()> {
  let img = image::open(src)
    .with_context(|| format!("failed to open {}", src.display()))?;
  let resized = img.resize_exact(size, size, image::imageops::FilterType::Lanczos3);
  resized.save(dest)
    .with_context(|| format!("failed to write {}", dest.display()))?;
  Ok(())
}

// ---- dependency management -------------------------------------------------

struct AssetDep {
  name: String,
  url: String,
  version: String,
  dest: String,
}

/// Parse [package.metadata.assets.*] entries from Cargo.toml.
fn parse_asset_deps() -> Result<Vec<AssetDep>> {
  let toml_str = fs::read_to_string("Cargo.toml")
    .context("failed to read Cargo.toml")?;

  let mut in_asset: Option<String> = None;
  let mut assets: Vec<AssetDep> = vec![];
  let mut url = String::new();
  let mut version = String::new();
  let mut dest = String::new();

  for line in toml_str.lines() {
    let trimmed = line.trim();
    if let Some(rest) = trimmed.strip_prefix("[package.metadata.assets.") {
      if let Some(name) = in_asset.take() {
        if !url.is_empty() && !version.is_empty() && !dest.is_empty() {
          assets.push(AssetDep { name, url: url.clone(), version: version.clone(), dest: dest.clone() });
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
      assets.push(AssetDep { name, url, version, dest });
    }
  }

  Ok(assets)
}

/// Extract "owner/repo" from a GitHub release URL.
fn github_repo_from_url(url: &str) -> Option<String> {
  // https://github.com/{owner}/{repo}/releases/download/...
  let rest = url.strip_prefix("https://github.com/")?;
  let parts: Vec<&str> = rest.splitn(4, '/').collect();
  if parts.len() >= 3 && parts[2] == "releases" {
    Some(format!("{}/{}", parts[0], parts[1]))
  } else {
    None
  }
}

/// Fetch pinned asset dependencies into .deps/.
fn fetch_asset_deps() -> Result<()> {
  let assets = parse_asset_deps()?;

  for dep in &assets {
    let resolved_url = dep.url.replace("{version}", &dep.version);
    println!("Fetching {} {}…", dep.name, dep.version);

    if Path::new(&dep.dest).exists() {
      fs::remove_dir_all(&dep.dest)?;
    }
    fs::create_dir_all(&dep.dest)?;

    let status = std::process::Command::new("curl")
      .args(["-sL", &resolved_url])
      .stdout(std::process::Stdio::piped())
      .spawn()
      .context("failed to run curl")?;

    let tar_status = std::process::Command::new("tar")
      .args(["-xzf", "-", "-C", &dep.dest])
      .stdin(status.stdout.unwrap())
      .status()
      .context("failed to run tar")?;

    if !tar_status.success() {
      anyhow::bail!("failed to download/extract {} from {resolved_url}", dep.name);
    }
    println!("  → {}/", dep.dest);
  }

  if assets.is_empty() {
    println!("No asset dependencies found in Cargo.toml");
  }

  Ok(())
}

/// Install pinned dependencies (asset deps only, no cargo update).
fn install_deps() -> Result<()> {
  fetch_asset_deps()
}

/// Update all dependencies: cargo update + re-fetch asset deps.
fn update_deps() -> Result<()> {
  println!("Running cargo update…");
  let status = std::process::Command::new("cargo")
    .arg("update")
    .status()
    .context("failed to run cargo update")?;
  if !status.success() {
    anyhow::bail!("cargo update failed");
  }

  fetch_asset_deps()
}

/// Remove all build output.
fn clean() -> Result<()> {
  let build_dir = "build";
  if Path::new(build_dir).exists() {
    fs::remove_dir_all(build_dir).context("failed to remove build dir")?;
    println!("Removed {build_dir}/");
  } else {
    println!("Nothing to clean.");
  }
  Ok(())
}

/// Query the latest release tag for a GitHub repo. Returns None on failure.
fn github_latest_tag(repo: &str) -> Option<String> {
  let output = std::process::Command::new("curl")
    .args(["-sf", &format!("https://api.github.com/repos/{repo}/releases/latest")])
    .output()
    .ok()?;

  if !output.status.success() { return None; }

  // Extract tag_name from JSON (simple string search to avoid a JSON dep)
  // Looks for: "tag_name": "v1.2.3"
  let body = String::from_utf8_lossy(&output.stdout);
  body.find("\"tag_name\"").and_then(|i| {
    let after_key = &body[i + "\"tag_name\"".len()..];
    let val_start = after_key.find('"')? + 1;
    let val_end = after_key[val_start..].find('"')?;
    Some(after_key[val_start..val_start + val_end].to_string())
  })
}

/// Parse git dependencies with tag pins from Cargo.toml.
/// Returns (name, repo "owner/repo", pinned tag).
fn parse_git_deps() -> Result<Vec<(String, String, String)>> {
  let toml_str = fs::read_to_string("Cargo.toml")
    .context("failed to read Cargo.toml")?;

  let mut deps = vec![];
  for line in toml_str.lines() {
    let trimmed = line.trim();
    // Match: name = { git = "https://github.com/owner/repo.git", tag = "v1.0.0" }
    if let Some((key, value)) = trimmed.split_once('=') {
      let name = key.trim();
      let val = value.trim();
      if val.contains("git =") && val.contains("tag =") {
        let git_url = extract_field(val, "git");
        let tag = extract_field(val, "tag");
        if let (Some(url), Some(tag)) = (git_url, tag) {
          if let Some(repo) = github_repo_from_git_url(&url) {
            deps.push((name.to_string(), repo, tag));
          }
        }
      }
    }
  }
  Ok(deps)
}

/// Extract a quoted field value from an inline TOML table, e.g. `git` from `{ git = "..." }`.
fn extract_field(s: &str, field: &str) -> Option<String> {
  let pattern = format!("{field} = \"");
  let start = s.find(&pattern)? + pattern.len();
  let end = s[start..].find('"')?;
  Some(s[start..start + end].to_string())
}

/// Extract "owner/repo" from a GitHub .git URL.
fn github_repo_from_git_url(url: &str) -> Option<String> {
  // https://github.com/owner/repo.git
  let rest = url.strip_prefix("https://github.com/")?;
  let rest = rest.strip_suffix(".git").unwrap_or(rest);
  let parts: Vec<&str> = rest.splitn(3, '/').collect();
  if parts.len() >= 2 {
    Some(format!("{}/{}", parts[0], parts[1]))
  } else {
    None
  }
}

/// Check for newer releases of all dependencies via the GitHub API.
fn check_deps() -> Result<()> {
  let assets = parse_asset_deps()?;
  let git_deps = parse_git_deps()?;

  if assets.is_empty() && git_deps.is_empty() {
    println!("No dependencies found in Cargo.toml");
    return Ok(());
  }

  let mut all_current = true;

  // Check git dependencies (crates pinned to tags)
  for (name, repo, tag) in &git_deps {
    match github_latest_tag(repo) {
      Some(ref latest) if latest == tag => {
        println!("  {name} {tag} ✓");
      }
      Some(ref latest) => {
        println!("  {name} {tag} → {latest} available");
        all_current = false;
      }
      None => {
        println!("  {name} {tag} (failed to query GitHub API)");
      }
    }
  }

  // Check asset dependencies
  for dep in &assets {
    let repo = match github_repo_from_url(&dep.url) {
      Some(r) => r,
      None => {
        println!("  {} {} (skipped — not a GitHub release URL)", dep.name, dep.version);
        continue;
      }
    };

    match github_latest_tag(&repo) {
      Some(ref tag) if tag == &dep.version => {
        println!("  {} {} ✓", dep.name, dep.version);
      }
      Some(ref tag) => {
        println!("  {} {} → {tag} available", dep.name, dep.version);
        all_current = false;
      }
      None => {
        println!("  {} {} (failed to query GitHub API)", dep.name, dep.version);
      }
    }
  }

  // Check crates.io dependencies via cargo outdated
  println!();
  let outdated = std::process::Command::new("cargo")
    .args(["outdated", "--root-deps-only"])
    .status();

  match outdated {
    Ok(s) if s.success() => {}
    Ok(_) => { all_current = false; }
    Err(_) => {
      println!("  cargo outdated not installed — run: cargo install cargo-outdated");
    }
  }

  if all_current {
    println!("All dependencies are up to date.");
  }

  Ok(())
}
