// Markdown → HTML processor.
//
// Uses pulldown-cmark for parsing. Fink code blocks (``` fink ... ```) are
// intercepted and replaced with highlighted HTML produced by highlight::highlight().
// All other code blocks fall through to pulldown-cmark's default handling
// (no highlighting — plain text in <code>).
//
// h2 headings get id attributes derived from their text so the sidebar TOC
// can link to them.

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd, html};

use crate::highlight;
use crate::playground;

/// A single TOC entry extracted from a ## heading.
pub struct TocEntry {
  pub text: String,
  pub slug: String,
}

/// Slugify a heading: lowercase, spaces/punctuation → hyphens, collapse runs.
pub fn slugify(text: &str) -> String {
  text.chars()
    .map(|c| if c.is_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
    .collect::<String>()
    .split('-')
    .filter(|s| !s.is_empty())
    .collect::<Vec<_>>()
    .join("-")
}

/// Extract TOC entries from ## headings in the markdown source.
pub fn extract_toc(md: &str) -> Vec<TocEntry> {
  let parser = Parser::new_ext(md, Options::empty());

  let mut entries = Vec::new();
  let mut in_h2 = false;
  let mut text_buf = String::new();

  for event in parser {
    match event {
      Event::Start(Tag::Heading { level: HeadingLevel::H2, .. }) => {
        in_h2 = true;
        text_buf.clear();
      }
      Event::Text(ref text) if in_h2 => {
        text_buf.push_str(text);
      }
      Event::End(TagEnd::Heading(HeadingLevel::H2)) if in_h2 => {
        in_h2 = false;
        let text = text_buf.trim().to_string();
        let slug = slugify(&text);
        entries.push(TocEntry { text, slug });
      }
      _ => {}
    }
  }

  entries
}

/// Render a markdown string to an HTML string.
/// Fink code blocks are syntax-highlighted.
/// h2 headings get id attributes for sidebar anchor links.
pub fn render(md: &str) -> String {
  let opts = Options::ENABLE_TABLES
    | Options::ENABLE_FOOTNOTES
    | Options::ENABLE_STRIKETHROUGH
    | Options::ENABLE_HEADING_ATTRIBUTES;

  let parser = Parser::new_ext(md, opts);
  let mut events: Vec<Event> = Vec::new();

  let mut in_fink_block = false;
  let mut fink_buf = String::new();

  // For h2: buffer text events between Start/End so we can wrap with id=
  let mut in_h2 = false;
  let mut h2_buf: Vec<Event<'static>> = Vec::new();
  let mut h2_text = String::new();

  for event in parser {
    match event {
      // --- Fink code blocks ---
      Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(ref lang)))
        if lang.as_ref() == "fink" =>
      {
        in_fink_block = true;
        fink_buf.clear();
      }

      Event::Text(ref text) if in_fink_block => {
        fink_buf.push_str(text);
      }

      Event::End(TagEnd::CodeBlock) if in_fink_block => {
        in_fink_block = false;
        let src = fink_buf.trim_end_matches('\n');
        let highlighted = highlight::highlight(src);
        let hash = playground::encode_source(src);
        // Wrap in container with playground link button (external-link icon)
        let html = format!(
          "<div class=\"code-block-wrap\">\
           <a class=\"playground-link\" href=\"/playground/#{hash}\" \
           title=\"Open in playground\" aria-label=\"Open in playground\">\
           <svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 135 135\">\
           <g transform=\"translate(0,3)\">\
           <path fill=\"currentColor\" d=\"M 7.6,102.4 C 34.3,81.4 46.3,56 57.8,36.3 70.8,14.2 90.6,8.3 111.9,25.6 103.8,25.3 98.5,37.4 84.1,53.9 69.7,70.5 44.6,92.5 7.6,102.4Z\"/>\
           <path fill=\"currentColor\" d=\"M 97.9,42.1 C 105.6,70.6 101.8,110.8 5.7,115.6 64.5,89.8 93.6,50 97.9,42.1Z\"/>\
           <path fill=\"#faa343\" d=\"m 102.5,50.9 c -4.6,-22.7 10.2,-26.9 26.5,-21.5 -8.2,2.8 -20.2,8 -26.5,21.5z\"/>\
           </g></svg></a>\
           <pre class=\"code-block\"><code class=\"language-fink\">{highlighted}</code></pre>\
           </div>\n"
        );
        events.push(Event::Html(html.into()));
      }

      // --- h2 headings: buffer contents, inject id= on close ---
      Event::Start(Tag::Heading { level: HeadingLevel::H2, .. }) => {
        in_h2 = true;
        h2_buf.clear();
        h2_text.clear();
      }

      Event::Text(text) if in_h2 => {
        h2_text.push_str(&text);
        h2_buf.push(Event::Text(text.into_static()));
      }

      Event::End(TagEnd::Heading(HeadingLevel::H2)) if in_h2 => {
        in_h2 = false;
        let slug = slugify(&h2_text);
        events.push(Event::Html(format!("<h2 id=\"{slug}\">").into()));
        events.extend(h2_buf.drain(..));
        events.push(Event::Html("</h2>\n".into()));
      }

      other => {
        if in_h2 {
          h2_buf.push(other.into_static());
        } else {
          events.push(other);
        }
      }
    }
  }

  let mut html_out = String::with_capacity(md.len() * 2);
  html::push_html(&mut html_out, events.into_iter());
  html_out
}
