// SVG renderer for syntax-highlighted Fink code.
//
// Reuses the highlight pipeline (lexer + AST annotations) and renders to an
// SVG document with monospace text, using the same colour palette as the
// website's CSS.
//
// Usage: cargo run -- svg <file.fnk>
//        Writes SVG to stdout.

use std::collections::HashMap;
use std::fs;

use crate::highlight;

// ---- colour map (parsed from static/syntax-colors.css) ----------------------

const SYNTAX_CSS_PATH: &str = "static/syntax-colors.css";

/// Parse syntax-colors.css for --fink-* custom properties.
/// Expects lines like: --fink-kw: #C586C0;
/// Returns a map from token class name (e.g. "kw") to hex color.
fn load_colors() -> HashMap<String, String> {
  let css = fs::read_to_string(SYNTAX_CSS_PATH)
    .unwrap_or_else(|e| panic!("failed to read {SYNTAX_CSS_PATH}: {e}"));

  let mut map = HashMap::new();
  for line in css.lines() {
    let line = line.trim();
    let Some(rest) = line.strip_prefix("--fink-") else { continue };
    let Some((name, rest)) = rest.split_once(':') else { continue };
    // Strip trailing comment (/* ... */) and semicolon
    let value = rest.split("/*").next().unwrap_or(rest);
    let color = value.trim().trim_end_matches(';').trim();
    if color.starts_with('#') {
      map.insert(name.to_string(), color.to_string());
    }
  }
  map
}

// ---- SVG rendering ---------------------------------------------------------

const FALLBACK_BG: &str = "#1F1F1F";
const FALLBACK_TEXT: &str = "#D4D4D4";
const FONT_FAMILY: &str = "Hack, Consolas, Menlo, Monaco, monospace";
const FONT_SIZE: f64 = 14.0;
const LINE_HEIGHT: f64 = 1.55;
const CHAR_WIDTH: f64 = 8.4;   // approximate for 14px monospace
const PAD_X: f64 = 16.0;
const PAD_Y: f64 = 16.0;

fn xml_escape(s: &str) -> String {
  s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Render a Fink source string as an SVG document.
pub fn render_svg(src: &str) -> String {
  let colors = load_colors();
  let bg = colors.get("bg").map(|s| s.as_str()).unwrap_or(FALLBACK_BG);
  let default_color = colors.get("text").map(|s| s.as_str()).unwrap_or(FALLBACK_TEXT);
  let anns = highlight::annotate(src);

  let lines: Vec<&str> = src.split('\n').collect();
  let max_cols = lines.iter().map(|l| l.len()).max().unwrap_or(0);
  let num_lines = lines.len();

  let line_h = FONT_SIZE * LINE_HEIGHT;
  let width = PAD_X * 2.0 + max_cols as f64 * CHAR_WIDTH;
  let height = PAD_Y * 2.0 + num_lines as f64 * line_h;
  let corner_r = 6.0;

  let mut out = String::with_capacity(src.len() * 4);

  // SVG header
  out.push_str(&format!(
    "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {w} {h}\" width=\"{w}\" height=\"{h}\">\n",
    w = width.ceil() as usize,
    h = height.ceil() as usize,
  ));
  out.push_str(&format!(
    "<rect width=\"100%\" height=\"100%\" rx=\"{corner_r}\" ry=\"{corner_r}\" fill=\"{bg}\"/>\n"
  ));

  // Build a byte-offset → (line, col) lookup
  let mut line_starts: Vec<usize> = Vec::with_capacity(num_lines);
  let mut offset = 0;
  for line in &lines {
    line_starts.push(offset);
    offset += line.len() + 1; // +1 for \n
  }

  // Walk annotations, grouped by line for tspan output
  let mut ann_idx = 0usize;

  for (line_no, line_text) in lines.iter().enumerate() {
    let line_start = line_starts[line_no];
    let line_end = line_start + line_text.len();
    let y = PAD_Y + (line_no as f64 + 0.8) * line_h;

    out.push_str(&format!(
      "<text x=\"{x}\" y=\"{y}\" fill=\"{default_color}\" font-family=\"{FONT_FAMILY}\" font-size=\"{FONT_SIZE}\" xml:space=\"preserve\">",
      x = PAD_X,
    ));

    let mut cursor = line_start;

    while cursor < line_end {
      // Skip past annotations
      while ann_idx < anns.len() && anns[ann_idx].1 <= cursor {
        ann_idx += 1;
      }

      // Find next annotation that overlaps this line
      let next = anns[ann_idx..].iter().find(|a| a.0 >= cursor && a.0 < line_end);

      match next {
        None => {
          // Rest of line is plain text
          out.push_str(&xml_escape(&src[cursor..line_end]));
          cursor = line_end;
        }
        Some(&(start, _, _)) if start > cursor => {
          // Plain text before annotation
          out.push_str(&xml_escape(&src[cursor..start]));
          cursor = start;
        }
        Some(&(_, end, ref class)) => {
          let seg_end = end.min(line_end);
          // Map highlight class names to CSS variable names where they differ
          let var_name = match class.as_str() {
            "fn"    => "call",
            "str-e" => "str-esc",
            "ph"    => "partial",
            "wc"    => "wldcrd",
            other   => other,
          };
          let color = colors.get(var_name)
            .or_else(|| var_name.rsplit_once('-').and_then(|(base, _)| colors.get(base)))
            .map(|s| s.as_str())
            .unwrap_or(default_color);
          let italic = class == "cmt";
          if italic {
            out.push_str(&format!(
              "<tspan fill=\"{color}\" font-style=\"italic\">{}</tspan>",
              xml_escape(&src[cursor..seg_end])
            ));
          } else {
            out.push_str(&format!(
              "<tspan fill=\"{color}\">{}</tspan>",
              xml_escape(&src[cursor..seg_end])
            ));
          }
          cursor = seg_end;
          ann_idx += 1;
        }
      }
    }

    out.push_str("</text>\n");
  }

  out.push_str("</svg>\n");
  out
}
