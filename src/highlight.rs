// Syntax highlighter for Fink code blocks.
//
// Strategy:
//   1. Run the Fink lexer for token-level annotations (keywords, operators,
//      strings, numbers, comments). These have priority 5.
//   2. Try to parse the snippet. If it succeeds, walk the AST for semantic
//      annotations (function calls, property names, record keys, brackets).
//      These have priority 10 and win over lexer annotations where they overlap.
//   3. Render the source using the merged annotations.
//
// On parse failure (incomplete snippets etc.) lexer-only mode still provides
// keywords, operators, strings, numbers, and comments.
//
// CSS classes:
//   .kw      — control/structural keywords (fn, match, import, …)
//   .kw-b    — boolean literals (true, false)
//   .ty      — built-in primitive types (u8, f32, str, …)
//   .fn      — function call site (AST)
//   .prop    — property name after . (AST)
//   .rec-key — record key in key:value pair (AST)
//   .blk     — block name: filter, map, fold, … (AST)
//   .tag     — tagged literal / template tag (AST)
//   .ident   — plain identifier (lexer fallback)
//   .op      — arithmetic / comparison / general operators
//   .op-asgn — = assignment
//   .op-pipe — | pipe and |= binding
//   .op-rng  — .. and ... ranges
//   .op-dot  — . accessor dot
//   .br      — brackets: { } [ ] ( )
//   .str     — string content and delimiters
//   .str-e   — string interpolation ${ }
//   .num     — numeric literals
//   .cmt     — comments
//   .ph      — ? partial application placeholder

use fink::ast::{CmpPart, Node, NodeKind};
use fink::lexer::{TokenKind, tokenize_with_seps};
use fink::parser;

// ---- token classification lists ----------------------------------------

const KEYWORDS: &[&str] = &[
  "fn", "match", "import", "type", "variant", "try", "else",
  "dict", "set", "ordered_set", "await", "yield",
];

const WORD_OPS: &[&str] = &["not", "and", "or", "xor", "in"];

const BUILTIN_LITERALS: &[&str] = &["true", "false"];

const BUILTIN_TYPES: &[&str] = &[
  "u8", "u16", "u32", "u64",
  "i8", "i16", "i32", "i64",
  "f16", "f32", "f64",
  "num", "int", "uint", "float", "dec", "str",
];

const OPERATORS: &[&[u8]] = &[
  b"|=", b"==", b"!=", b"<=", b">=",
  b"//", b"**", b"%%", b"/%",
  b"...", b"..", b"->",
  b">>>", b"<<<", b">>", b"<<", b"><",
  b"+", b"-", b"*", b"/", b"%",
  b"=", b"<", b">",
  b"|", b".",
];

// ---- annotation map -------------------------------------------------------

#[derive(Clone)]
struct Ann {
  start: usize,
  end: usize,
  class: &'static str,
  priority: u8,
}

struct Annotations {
  anns: Vec<Ann>,
}

impl Annotations {
  fn new() -> Self { Self { anns: Vec::new() } }

  fn add(&mut self, start: usize, end: usize, class: &'static str, priority: u8) {
    if start < end {
      self.anns.push(Ann { start, end, class, priority });
    }
  }

  fn sort(&mut self) {
    // Sort by start position; higher priority wins for same-start annotations
    self.anns.sort_by(|a, b| a.start.cmp(&b.start).then(b.priority.cmp(&a.priority)));
  }
}

// ---- AST annotation pass --------------------------------------------------

fn ann_node(anns: &mut Annotations, node: &Node, class: &'static str) {
  anns.add(node.loc.start.idx as usize, node.loc.end.idx as usize, class, 10);
}

fn ann_range(anns: &mut Annotations, start: usize, end: usize, class: &'static str) {
  anns.add(start, end, class, 10);
}

fn resolve_callee<'a>(node: &'a Node<'a>) -> &'a Node<'a> {
  match &node.kind {
    NodeKind::Member { rhs, .. } => resolve_callee(rhs),
    _ => node,
  }
}

// Return the CSS class for a bracket at the given nesting depth (1-based, cycling 1–3).
fn br_class(depth: usize) -> &'static str {
  match (depth - 1) % 3 {
    0 => "br-1",
    1 => "br-2",
    _ => "br-3",
  }
}

fn collect_ast_anns<'src>(node: &'src Node<'src>, anns: &mut Annotations, depth: usize) {
  match &node.kind {

    NodeKind::Apply { func, args } => {
      let callee = resolve_callee(func);
      match &callee.kind {
        NodeKind::Ident(_) => {
          // Detect tagged literals: callee immediately adjacent to first arg
          let tag_kind = args.items.first().and_then(|first_arg| {
            if callee.loc.end.idx == first_arg.loc.start.idx {
              Some("tag")   // prefix: fmt'...'
            } else if first_arg.loc.end.idx == callee.loc.start.idx {
              Some("tag")   // postfix: 10sec
            } else {
              None
            }
          });
          ann_node(anns, callee, tag_kind.unwrap_or("fn"));
        }
        NodeKind::Group { open, close, .. } => {
          // (expr)(args) — colour opening/closing parens as function
          ann_range(anns, open.loc.start.idx as usize, open.loc.end.idx as usize, "fn");
          ann_range(anns, close.loc.start.idx as usize, close.loc.end.idx as usize, "fn");
        }
        _ => {}
      }
      collect_ast_anns(func, anns, depth);
      for arg in &args.items { collect_ast_anns(arg, anns, depth); }
    }

    NodeKind::Pipe(children) => {
      for child in &children.items {
        // Bare ident directly in pipe position is a function
        if matches!(&child.kind, NodeKind::Ident(_)) {
          ann_node(anns, child, "fn");
        }
        collect_ast_anns(child, anns, depth);
      }
    }

    NodeKind::Member { op, lhs, rhs } => {
      // Annotate the dot as op-dot
      ann_range(anns, op.loc.start.idx as usize, op.loc.end.idx as usize, "op-dot");
      collect_ast_anns(lhs, anns, depth);
      if matches!(&rhs.kind, NodeKind::Ident(_)) {
        ann_node(anns, rhs, "prop");
      } else {
        collect_ast_anns(rhs, anns, depth);
      }
    }

    NodeKind::LitRec { open, close, items } => {
      let br = br_class(depth);
      ann_range(anns, open.loc.start.idx as usize, open.loc.end.idx as usize, br);
      ann_range(anns, close.loc.start.idx as usize, close.loc.end.idx as usize, br);
      for child in &items.items {
        if let NodeKind::Arm { lhs, body, .. } = &child.kind {
          if matches!(&lhs.kind, NodeKind::Ident(_)) {
            // shorthand {foo} = variable ref; key:val = record key
            ann_node(anns, lhs, if body.items.is_empty() { "ident" } else { "rec-key" });
          }
          collect_ast_anns(lhs, anns, depth + 1);
          for expr in &body.items { collect_ast_anns(expr, anns, depth + 1); }
        } else {
          collect_ast_anns(child, anns, depth + 1);
        }
      }
    }

    NodeKind::LitSeq { open, close, items } => {
      let br = br_class(depth);
      ann_range(anns, open.loc.start.idx as usize, open.loc.end.idx as usize, br);
      ann_range(anns, close.loc.start.idx as usize, close.loc.end.idx as usize, br);
      for child in &items.items { collect_ast_anns(child, anns, depth + 1); }
    }

    NodeKind::Group { open, close, inner } => {
      let br = br_class(depth);
      ann_range(anns, open.loc.start.idx as usize, open.loc.end.idx as usize, br);
      ann_range(anns, close.loc.start.idx as usize, close.loc.end.idx as usize, br);
      collect_ast_anns(inner, anns, depth + 1);
    }

    NodeKind::Block { name, params, body, .. } => {
      if matches!(&name.kind, NodeKind::Ident(_)) {
        ann_node(anns, name, "blk");
      }
      collect_ast_anns(params, anns, depth);
      for expr in &body.items { collect_ast_anns(expr, anns, depth); }
    }

    // --- recurse-only nodes ---

    NodeKind::StrTempl { children, .. } | NodeKind::StrRawTempl { children, .. } => {
      for child in children { collect_ast_anns(child, anns, depth); }
    }

    NodeKind::Module(children) | NodeKind::Patterns(children) => {
      for child in &children.items { collect_ast_anns(child, anns, depth); }
    }

    NodeKind::InfixOp { lhs, rhs, .. }
    | NodeKind::Bind { lhs, rhs, .. }
    | NodeKind::BindRight { lhs, rhs, .. } => {
      collect_ast_anns(lhs, anns, depth);
      collect_ast_anns(rhs, anns, depth);
    }

    NodeKind::ChainedCmp(parts) => {
      for part in parts {
        if let CmpPart::Operand(n) = part { collect_ast_anns(n, anns, depth); }
      }
    }

    NodeKind::UnaryOp { operand, .. }
    | NodeKind::Try(operand)
    | NodeKind::Yield(operand) => { collect_ast_anns(operand, anns, depth); }

    NodeKind::Spread { inner: Some(inner), .. } => { collect_ast_anns(inner, anns, depth); }

    NodeKind::Fn { params, body, .. } => {
      collect_ast_anns(params, anns, depth);
      for expr in &body.items { collect_ast_anns(expr, anns, depth); }
    }

    NodeKind::Match { subjects, arms, .. } => {
      for subj in &subjects.items { collect_ast_anns(subj, anns, depth); }
      for arm in &arms.items { collect_ast_anns(arm, anns, depth); }
    }

    NodeKind::Arm { lhs, body, .. } => {
      collect_ast_anns(lhs, anns, depth);
      for expr in &body.items { collect_ast_anns(expr, anns, depth); }
    }

    // Leaf nodes
    NodeKind::Ident(_)
    | NodeKind::LitBool(_) | NodeKind::LitInt(_)
    | NodeKind::LitFloat(_) | NodeKind::LitDecimal(_)
    | NodeKind::LitStr { .. }
    | NodeKind::Partial | NodeKind::Wildcard
    | NodeKind::Spread { inner: None, .. } => {}
  }
}

// ---- lexer annotation pass ------------------------------------------------

fn classify_sep(src: &str) -> &'static str {
  match src {
    "|=" | "|" => "op-pipe",
    "="        => "op-asgn",
    "..." | ".." => "op-rng",
    "."        => "op-dot",
    _          => "op",
  }
}

fn classify_ident(src: &str) -> &'static str {
  if KEYWORDS.contains(&src)             { "kw"    }
  else if BUILTIN_LITERALS.contains(&src) { "kw-b"  }
  else if BUILTIN_TYPES.contains(&src)    { "ty"    }
  else if WORD_OPS.contains(&src)         { "op"    }
  else if src == "_"                      { "wc"    }
  else                                    { "ident" }
}

// Emit sub-token annotations for a numeric literal.
//
// Highlights the base prefix (0x/0o/0b), exponent marker (e/E), and decimal
// suffix (d) in num-b; digits and separators (_) in num.
fn ann_number(base: usize, tok_src: &str, kind: TokenKind, anns: &mut Annotations, p: u8) {
  match kind {
    TokenKind::Int => {
      // Check for base prefix: 0x 0o 0b (case-insensitive, may have _ after)
      let lower = tok_src.to_ascii_lowercase();
      if lower.starts_with("0x") || lower.starts_with("0o") || lower.starts_with("0b") {
        // 0 = num, x/o/b = num-b, rest = num
        anns.add(base, base + 1, "num", p);
        anns.add(base + 1, base + 2, "num-b", p);
        if tok_src.len() > 2 { anns.add(base + 2, base + tok_src.len(), "num", p); }
      } else {
        anns.add(base, base + tok_src.len(), "num", p);
      }
    }
    TokenKind::Float => {
      // Split on 'e'/'E': digits before = num, e = num-b, exponent = num
      if let Some(pos) = tok_src.find(|c| c == 'e' || c == 'E') {
        anns.add(base, base + pos, "num", p);
        anns.add(base + pos, base + pos + 1, "num-b", p);
        if tok_src.len() > pos + 1 {
          anns.add(base + pos + 1, base + tok_src.len(), "num", p);
        }
      } else {
        anns.add(base, base + tok_src.len(), "num", p);
      }
    }
    TokenKind::Decimal => {
      // Trailing 'd' suffix: digits = num, d = num-b
      if tok_src.ends_with('d') || tok_src.ends_with('D') {
        let body_end = tok_src.len() - 1;
        anns.add(base, base + body_end, "num", p);
        anns.add(base + body_end, base + tok_src.len(), "num-b", p);
      } else {
        anns.add(base, base + tok_src.len(), "num", p);
      }
    }
    _ => { anns.add(base, base + tok_src.len(), "num", p); }
  }
}

// Emit sub-token annotations for a StrText token.
// Escape sequences (\n, \t, \u{XXXX}, \${, etc.) are highlighted in str-e;
// plain text in str.
fn ann_str_text(base: usize, tok_src: &str, anns: &mut Annotations, p: u8) {
  let bytes = tok_src.as_bytes();
  let mut i = 0;
  let mut seg_start = 0;

  while i < bytes.len() {
    if bytes[i] == b'\\' {
      // Flush plain text before this escape
      if seg_start < i { anns.add(base + seg_start, base + i, "str", p); }

      // Determine escape length
      let esc_len = if i + 1 < bytes.len() {
        match bytes[i + 1] {
          b'u' if i + 2 < bytes.len() && bytes[i + 2] == b'{' => {
            // \u{XXXXXX} — find closing }
            let close = bytes[i+2..].iter().position(|&b| b == b'}')
              .map(|p| p + 3).unwrap_or(2);
            close + 1
          }
          b'$' => 2, // \${
          _    => 2, // \n \t \r \\ \' etc.
        }
      } else {
        1
      };

      let esc_end = (i + esc_len).min(bytes.len());
      anns.add(base + i, base + esc_end, "str-e", p);
      i = esc_end;
      seg_start = i;
    } else {
      i += 1;
    }
  }
  // Flush remaining plain text
  if seg_start < bytes.len() {
    anns.add(base + seg_start, base + bytes.len(), "str", p);
  }
}

fn collect_lexer_anns(src: &str, anns: &mut Annotations) {
  let mut lexer = tokenize_with_seps(src, OPERATORS);
  loop {
    let tok = lexer.next_token();
    if tok.kind == TokenKind::EOF { break; }
    let s = tok.loc.start.idx as usize;
    let e = tok.loc.end.idx as usize;
    let p = 5u8; // lexer priority — AST (10) wins
    match tok.kind {
      TokenKind::Ident => { anns.add(s, e, classify_ident(tok.src), p); }
      TokenKind::Int | TokenKind::Float | TokenKind::Decimal => {
        ann_number(s, tok.src, tok.kind, anns, p);
      }
      TokenKind::Sep  => { anns.add(s, e, classify_sep(tok.src), p); }
      TokenKind::Comma | TokenKind::Semicolon | TokenKind::Colon => { anns.add(s, e, "op", p); }
      TokenKind::Partial  => { anns.add(s, e, "ph", p); }
      TokenKind::BracketOpen | TokenKind::BracketClose => { anns.add(s, e, "br", p); }
      TokenKind::StrStart | TokenKind::StrEnd => { anns.add(s, e, "str", p); }
      TokenKind::StrText => { ann_str_text(s, tok.src, anns, p); }
      TokenKind::StrExprStart | TokenKind::StrExprEnd => { anns.add(s, e, "str-e", p); }
      TokenKind::Comment
      | TokenKind::CommentStart | TokenKind::CommentText | TokenKind::CommentEnd => {
        anns.add(s, e, "cmt", p);
      }
      TokenKind::BlockStart | TokenKind::BlockCont | TokenKind::BlockEnd => {}
      TokenKind::Err | TokenKind::EOF => {}
    }
  }
}

// ---- rendering ------------------------------------------------------------

fn html_escape(s: &str) -> String {
  s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn render(src: &str, anns: &mut Annotations) -> String {
  anns.sort();

  let mut out = String::with_capacity(src.len() * 3);
  let mut cursor = 0usize;
  let mut ann_idx = 0usize;

  while cursor < src.len() {
    // Skip annotations that are fully behind cursor
    while ann_idx < anns.anns.len() && anns.anns[ann_idx].end <= cursor {
      ann_idx += 1;
    }

    // Find the next annotation starting at or after cursor
    let next = anns.anns[ann_idx..]
      .iter()
      .find(|a| a.start >= cursor);

    match next {
      None => {
        out.push_str(&html_escape(&src[cursor..]));
        break;
      }
      Some(ann) if ann.start > cursor => {
        out.push_str(&html_escape(&src[cursor..ann.start]));
        cursor = ann.start;
      }
      Some(ann) => {
        let end = ann.end.min(src.len());
        let class = ann.class;
        out.push_str(&format!(
          "<span class=\"{}\">{}</span>",
          class,
          html_escape(&src[cursor..end])
        ));
        cursor = end;
        ann_idx += 1;
      }
    }
  }

  out
}

// ---- public entry points --------------------------------------------------

fn collect(src: &str) -> Annotations {
  let mut anns = Annotations::new();
  collect_lexer_anns(src, &mut anns);
  if let Ok(parse_result) = parser::parse(src) {
    collect_ast_anns(&parse_result.root, &mut anns, 1);
  }
  anns
}

/// Highlight a Fink source snippet, returning an HTML fragment.
/// Suitable for wrapping in `<pre><code>...</code></pre>`.
pub fn highlight(src: &str) -> String {
  let mut anns = collect(src);
  render(src, &mut anns)
}

/// Return resolved annotation spans as (start, end, class) tuples.
/// Spans are non-overlapping and sorted by position; gaps between spans
/// are plain (unstyled) text.
pub fn annotate(src: &str) -> Vec<(usize, usize, String)> {
  let mut anns = collect(src);
  anns.sort();

  let mut result = Vec::new();
  let mut cursor = 0usize;
  let mut ann_idx = 0usize;

  while cursor < src.len() {
    while ann_idx < anns.anns.len() && anns.anns[ann_idx].end <= cursor {
      ann_idx += 1;
    }
    let next = anns.anns[ann_idx..].iter().find(|a| a.start >= cursor);
    match next {
      None => break,
      Some(ann) if ann.start > cursor => { cursor = ann.start; }
      Some(ann) => {
        let end = ann.end.min(src.len());
        result.push((cursor, end, ann.class.to_string()));
        cursor = end;
        ann_idx += 1;
      }
    }
  }

  result
}
