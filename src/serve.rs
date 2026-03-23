// Dev server — serves the build/ directory over HTTP.
//
// URL → file mapping:
//   /           → build/index.html
//   /docs/      → build/docs/index.html
//   /style.css  → build/style.css
//
// Trailing-slash paths try index.html. Extensionless paths try <path>/index.html.
// Returns 404 for anything not found.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;
use tiny_http::{Response, Server};

const PID_FILE: &str = "build/site/.server.pid";

pub fn stop() -> Result<()> {
  let pid_str = fs::read_to_string(PID_FILE)
    .map_err(|_| anyhow::anyhow!("No server running (no PID file found)"))?;
  let pid: u32 = pid_str.trim().parse()
    .map_err(|_| anyhow::anyhow!("Invalid PID file"))?;

  // SIGTERM on Unix
  #[cfg(unix)]
  {
    use std::process::Command;
    let status = Command::new("kill").arg(pid.to_string()).status()?;
    if !status.success() {
      anyhow::bail!("Failed to kill process {pid} — already stopped?");
    }
  }

  fs::remove_file(PID_FILE).ok();
  println!("Server (PID {pid}) stopped.");
  Ok(())
}

pub fn run(build_dir: &str, port: u16) -> Result<()> {
  let pid = std::process::id();
  fs::write(PID_FILE, pid.to_string())?;

  let addr = format!("0.0.0.0:{port}");
  let server = Server::http(&addr).map_err(|e| anyhow::anyhow!("{e}"))?;
  println!("Serving {build_dir}/ at http://localhost:{port}/  (run `cargo run -- stop` to stop)");

  // Clean up PID file on exit (via Drop)
  struct PidGuard;
  impl Drop for PidGuard {
    fn drop(&mut self) { fs::remove_file(PID_FILE).ok(); }
  }
  let _guard = PidGuard;

  for request in server.incoming_requests() {
    let url = request.url().to_string();
    let path = resolve_path(build_dir, &url);

    match fs::read(&path) {
      Ok(bytes) => {
        let mime = mime_type(path.extension().and_then(|e| e.to_str()).unwrap_or(""));
        let response = Response::from_data(bytes).with_header(
          tiny_http::Header::from_bytes("Content-Type", mime).unwrap(),
        );
        let _ = request.respond(response);
      }
      Err(_) => {
        let not_found = Path::new(build_dir).join("404.html");
        if let Ok(bytes) = fs::read(&not_found) {
          let response = Response::from_data(bytes)
            .with_status_code(404)
            .with_header(tiny_http::Header::from_bytes("Content-Type", "text/html; charset=utf-8").unwrap());
          let _ = request.respond(response);
        } else {
          let _ = request.respond(Response::from_string(format!("404 Not Found: {url}")).with_status_code(404));
        }
      }
    }
  }

  Ok(())
}

fn resolve_path(build_dir: &str, url: &str) -> PathBuf {
  // Strip query string
  let path_part = url.split('?').next().unwrap_or("/");

  // Decode percent-encoding minimally (just %20 for now; extend if needed)
  let decoded = path_part.replace("%20", " ");

  let relative = decoded.trim_start_matches('/');
  let base = Path::new(build_dir).join(relative);

  // If it's an existing file, serve it directly
  if base.is_file() {
    return base;
  }

  // Otherwise try index.html inside the directory
  let index = base.join("index.html");
  if index.is_file() {
    return index;
  }

  // Also try adding .html (for bare extensionless paths like /docs)
  let with_html = base.with_extension("html");
  if with_html.is_file() {
    return with_html;
  }

  base // will 404
}

fn mime_type(ext: &str) -> &'static str {
  match ext {
    "html" => "text/html; charset=utf-8",
    "css"  => "text/css; charset=utf-8",
    "js"   => "application/javascript",
    "png"  => "image/png",
    "svg"  => "image/svg+xml",
    "ico"  => "image/x-icon",
    "wasm" => "application/wasm",
    "woff2"=> "font/woff2",
    "ttf"  => "font/ttf",
    _      => "application/octet-stream",
  }
}
