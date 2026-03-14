// Markdown → HTML processor.
//
// Uses pulldown-cmark for parsing. Fink code blocks (``` fink ... ```) are
// intercepted and replaced with highlighted HTML produced by highlight::highlight().
// All other code blocks fall through to pulldown-cmark's default handling
// (no highlighting — plain text in <code>).

use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd, html};

use crate::highlight;

/// Render a markdown string to an HTML string.
/// Fink code blocks are syntax-highlighted.
pub fn render(md: &str) -> String {
  let opts = Options::ENABLE_TABLES
    | Options::ENABLE_FOOTNOTES
    | Options::ENABLE_STRIKETHROUGH
    | Options::ENABLE_HEADING_ATTRIBUTES;

  let parser = Parser::new_ext(md, opts);
  let mut events: Vec<Event> = Vec::new();

  let mut in_fink_block = false;
  let mut fink_buf = String::new();

  for event in parser {
    match event {
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
        // Trim trailing newline that pulldown-cmark adds
        let src = fink_buf.trim_end_matches('\n');
        let highlighted = highlight::highlight(src);
        let html = format!(
          "<pre class=\"code-block\"><code class=\"language-fink\">{}</code></pre>\n",
          highlighted
        );
        events.push(Event::Html(html.into()));
      }

      other => {
        events.push(other);
      }
    }
  }

  let mut html_out = String::with_capacity(md.len() * 2);
  html::push_html(&mut html_out, events.into_iter());
  html_out
}
