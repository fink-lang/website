// Fink playground — Monaco editor + semantic tokens + WASI run pipeline.
//
// Analysis (semantic tokens, diagnostics) uses the same WASM that powers
// the vscode-fink extension, loaded via the data-URL dynamic-import trick
// so it can be served as plain static files without a bundler touching it.
//
// Code execution uses the WASI shim (wasi-shim.ts) running in a sandboxed
// iframe. The compiler slot is a placeholder for now (see compiler.ts).

// MonacoEnvironment must be set before the editor creates its workers.
;(window as any).MonacoEnvironment = {
  getWorkerUrl(_moduleId: string, _label: string): string {
    return new URL('./editor.worker.js', import.meta.url).href
  },
}

import * as monaco from 'monaco-editor/esm/vs/editor/editor.api'
import 'monaco-editor/esm/vs/editor/contrib/semanticTokens/browser/documentSemanticTokens.js'
import { loadWASM, createOnigScanner, createOnigString } from 'vscode-oniguruma'
import { Registry, INITIAL, parseRawGrammar } from 'vscode-textmate'
import { compile } from './compiler.js'
import { run } from './wasi-shim.js'

// ---------------------------------------------------------------------------
// Analysis WASM (semantic tokens, diagnostics)
// ---------------------------------------------------------------------------

// eslint-disable-next-line @typescript-eslint/no-explicit-any
let ParsedDocument: any = null

// Resolved once the WASM is ready. The semantic token provider awaits this
// so it doesn't race with the async WASM load on first page render.
let resolveWasmReady!: () => void
const wasmReady = new Promise<void>(resolve => { resolveWasmReady = resolve })

async function loadAnalysisWasm(): Promise<void> {
  // Derive the base URL of this module so assets are found regardless of
  // where the playground is deployed.
  const base = new URL('.', import.meta.url).href
  console.log('[fink] loading analysis WASM from', base)

  console.log('[fink] fetching wasm binary...')
  const wasmBin = await fetch(`${base}fink_wasm_bg.wasm`).then(r => {
    if (!r.ok) throw new Error(`fink_wasm_bg.wasm: ${r.status}`)
    return r.arrayBuffer()
  })
  console.log('[fink] wasm binary fetched, size:', wasmBin.byteLength)

  console.log('[fink] importing glue module...')
  const mod = await import(/* @vite-ignore */ `${base}fink_wasm.js`)
  console.log('[fink] glue module imported, calling init...')
  await mod.default(wasmBin)
  ParsedDocument = mod.ParsedDocument
  resolveWasmReady()
  console.log('[fink] analysis WASM ready')
}

// ---------------------------------------------------------------------------
// Language registration + TM grammar
// ---------------------------------------------------------------------------

monaco.languages.register({ id: 'fink', extensions: ['.fnk'] })

monaco.languages.setLanguageConfiguration('fink', {
  comments: {
    lineComment: '#',
    blockComment: ['---', '---'],
  },
  brackets: [
    ['{', '}'],
    ['[', ']'],
    ['(', ')'],
  ],
  autoClosingPairs: [
    { open: '{', close: '}' },
    { open: '[', close: ']' },
    { open: '(', close: ')' },
    { open: "'", close: "'", notIn: ['string'] },
    { open: '---', close: '---', notIn: ['string', 'comment'] },
  ],
  autoCloseBefore: ';:.,=}])> \n\t',
  surroundingPairs: [
    { open: '{', close: '}' },
    { open: '[', close: ']' },
    { open: '(', close: ')' },
    { open: "'", close: "'" },
  ],
  indentationRules: {
    increaseIndentPattern: /^(\s*).*:\s*$/,
    decreaseIndentPattern: /^\s*$/,
  },
  onEnterRules: [
    // Increase indent after a line ending with ':'
    {
      beforeText: /:\s*$/,
      action: { indentAction: monaco.languages.IndentAction.Indent },
    },
  ],
  folding: {
    offSide: true,
  },
})

// TMToMonacoToken: maps TM scope array → Monaco theme token string.
// Tries progressively shorter scope prefixes until a themed color is found.
// eslint-disable-next-line @typescript-eslint/no-explicit-any
// TMToMonacoToken: maps TM scope array → Monaco theme token string.
// Iterates from innermost scope (scopes[0]) outward, trying progressively
// shorter dot-prefixes, returning the first that has a themed foreground color.
// Innermost-first ensures more-specific rules (e.g. entity.name.tag.numeric)
// win over outer container scopes (e.g. constant.numeric).
// Scopes array from vscode-textmate is outermost-first; innermost (most
// specific) is scopes[scopes.length - 1]. Iterate innermost-first so that
// capture-level scopes (e.g. constant.character.escape inside constant.numeric)
// win over their container scope.
function tmToMonacoToken(editor: any, scopes: string[]): string {
  const result = (() => {
    for (let i = scopes.length - 1; i >= 0; i--) {
      const scope = scopes[i]
      for (let j = scope.length - 1; j >= 0; j--) {
        if (scope[j] === '.') {
          const token = scope.slice(0, j)
          if (editor._themeService._theme._tokenTheme._match(token)._foreground > 1)
            return token
        }
      }
    }
    return ''
  })()
  // Monaco collapses keyword.operator → keyword at theme-build time, losing
  // the distinct color. Remap operator scopes to a custom token name that
  // has its own entry in the theme rules.
  // keyword.operator.* scopes resolve to the `keyword` color via Monaco's
  // prefix matching. Intercept them before the fallback and return our custom
  // token name that has a distinct white color entry.
  if (scopes.some(s => s.startsWith('keyword.operator')))
    return 'fink-operator'
  return result
}

// Loaded once; grammar wiring needs to run after the editor is created.
async function loadTmGrammar(editor: monaco.editor.IStandaloneCodeEditor): Promise<void> {
  const base = new URL('.', import.meta.url).href

  // vscode-oniguruma — fetch as ArrayBuffer so MIME type doesn't matter.
  const onigBuffer = await fetch(`${base}onig.wasm`).then(r => r.arrayBuffer())
  await loadWASM({ data: onigBuffer })

  const finkGrammarContent = await fetch(`${base}fink.tmLanguage.json`).then(r => r.text())

  // vscode-textmate Registry — handles $self/$base recursion correctly.
  const registry = new Registry({
    onigLib: Promise.resolve({ createOnigScanner, createOnigString }),
    async loadGrammar(scopeName) {
      if (scopeName === 'source.fink') {
        return parseRawGrammar(finkGrammarContent, 'fink.tmLanguage.json')
      }
      return null
    },
  })

  const grammar = await registry.loadGrammar('source.fink')
  if (!grammar) throw new Error('Failed to load fink grammar')

  monaco.languages.setTokensProvider('fink', {
    getInitialState: () => ({ ruleStack: INITIAL, clone() { return this }, equals(o: unknown) { return o === this } }),
    tokenize(line, state: { ruleStack: typeof INITIAL }) {
      const result = grammar.tokenizeLine(line, state.ruleStack)
      return {
        endState: { ruleStack: result.ruleStack, clone() { return this }, equals(o: unknown) { return o === this } },
        tokens: result.tokens.map((t, i) => {
          let scopes = tmToMonacoToken(editor, t.scopes)
          if (scopes === '') {
            const end = i + 1 < result.tokens.length ? result.tokens[i + 1].startIndex : line.length
            const ch = line.slice(t.startIndex, end).trim()
            if ('[]{}()'.includes(ch)) scopes = 'fink-bracket'
          }
          return { startIndex: t.startIndex, scopes }
        }),
      }
    },
  })
  console.log('[fink] TM grammar loaded')
}

// Semantic token legend must match TOKEN_* constants in vscode-fink/src/lib.rs
const TOKEN_TYPES = ['function', 'variable', 'property', 'block-name', 'tag-left', 'tag-right']
const TOKEN_MODIFIERS = ['readonly']

monaco.languages.registerDocumentSemanticTokensProvider('fink', {
  getLegend() {
    return { tokenTypes: TOKEN_TYPES, tokenModifiers: TOKEN_MODIFIERS }
  },
  async provideDocumentSemanticTokens(model) {
    await wasmReady
    const src = model.getValue()
    const doc = new ParsedDocument(src)
    const data = doc.get_semantic_tokens()
    const diag = doc.get_diagnostics()
    doc.free()
    const parsed = JSON.parse(diag) as Array<{
      line: number, col: number, endLine: number, endCol: number,
      message: string, source: string, severity: string
    }>
    monaco.editor.setModelMarkers(model, 'fink', parsed.map(d => ({
      startLineNumber: d.line + 1,
      startColumn: d.col + 1,
      endLineNumber: d.endLine + 1,
      endColumn: Math.max(d.endCol + 1, d.col + 2),
      message: d.message,
      severity: d.severity === 'error'
        ? monaco.MarkerSeverity.Error
        : d.severity === 'warning'
          ? monaco.MarkerSeverity.Warning
          : monaco.MarkerSeverity.Info,
      source: d.source,
    })))
    return { data, resultId: undefined }
  },
  releaseDocumentSemanticTokens() {},
})

// ---------------------------------------------------------------------------
// Theme
// ---------------------------------------------------------------------------

// Monaco standalone's getTokenStyleMetadata() resolves semantic token colors
// via the TextMate rule matcher (_match), not a separate semanticTokenColors
// map. We add rules whose `token` field matches the semantic token type names
// (and optional dot-separated modifiers) returned by the provider.
// Theme mirrors the colors in static/style.css (the static-site highlighter).
// Rules here cover:
//   a) semantic token types returned by the WASM provider (function, variable, …)
//   b) TM scopes not already handled by the vs-dark base theme
monaco.editor.defineTheme('fink-dark', {
  base: 'vs-dark',
  inherit: true,
  rules: [
    // Semantic tokens (must match TOKEN_* constants in vscode-fink/src/lib.rs)
    { token: 'function',          foreground: 'DCDCAA' },  // .fn
    { token: 'variable',          foreground: '9CDCFE' },  // .rec-key
    { token: 'variable.readonly', foreground: '4FC1FF' },  // .ident / .prop
    { token: 'property',          foreground: '9CDCFE' },  // .rec-key
    { token: 'block-name',        foreground: '4FC1FF' },  // .blk (bright blue)
    { token: 'tag-left',          foreground: '569CD6' },  // .tag
    { token: 'tag-right',         foreground: '569CD6' },  // .tag

    // TM token rules — explicit entries matching the strings returned by
    // tmToMonacoToken. Monaco standalone's `inherit: true` pulls in vs-dark
    // base colors for standard scopes, but we add them explicitly here so
    // they are guaranteed to be present.
    { token: 'comment',                      foreground: '6A9955' },
    { token: 'comment.line',                 foreground: '6A9955' },
    { token: 'constant.language',            foreground: '569CD6' },
    { token: 'constant.numeric',             foreground: 'B5CEA8' },
    { token: 'constant.character.escape',    foreground: 'D7BA7D' },  // \n \t \x \u etc + 0x/0b prefix
    { token: 'entity.name.function',         foreground: 'DCDCAA' },
    { token: 'entity.name.tag',              foreground: '569CD6' },  // .tag
    { token: 'entity.name.tag.numeric',      foreground: '569CD6' },  // numeric tag suffix (10sec, 1.5min)
    { token: 'entity.name.tag.postfix',      foreground: '569CD6' },  // postfix tag 10sec
    { token: 'entity.name.tag.string',       foreground: '569CD6' },  // template tag fmt, raw
    { token: 'entity.name.type',             foreground: '4EC9B0' },
    { token: 'entity.other.attribute-name',  foreground: '9CDCFE' },
    { token: 'invalid',                      foreground: 'F44747' },
    { token: 'keyword',                      foreground: '569CD6' },
    { token: 'keyword.control',              foreground: 'C586C0' },
    { token: 'fink-operator',                foreground: 'D4D4D4' },  // operators (remapped in tmToMonacoToken)
    { token: 'fink-bracket',                 foreground: 'DCDCAA' },  // brackets [] {} () (.br-1 gold)
    { token: 'punctuation.section.embedded', foreground: '569CD6' },  // ${ }
    { token: 'storage',                      foreground: '569CD6' },
    { token: 'storage.modifier',             foreground: '569CD6' },
    { token: 'storage.type',                 foreground: '4EC9B0' },
    { token: 'string',                       foreground: 'CE9178' },
    { token: 'variable',                     foreground: '9CDCFE' },
    { token: 'variable.language',            foreground: '569CD6' },
    { token: 'variable.other.constant',      foreground: '4FC1FF' },
    { token: 'variable.other.property',      foreground: '4FC1FF' },
    { token: 'entity.name.label',            foreground: '4FC1FF' },
  ],
  colors: {
    // Bracket pair colorization — matched to static site .br-1/2/3 palette
    'editorBracketHighlight.foreground1': '#BDBB85',
    'editorBracketHighlight.foreground2': '#CC76D1',
    'editorBracketHighlight.foreground3': '#4A9DF8',
    'editorBracketHighlight.foreground4': '#BDBB85',
    'editorBracketHighlight.foreground5': '#CC76D1',
    'editorBracketHighlight.foreground6': '#4A9DF8',
    'editorBracketHighlight.unexpectedBracket.foreground': '#FF000066',
  },
})

// ---------------------------------------------------------------------------
// Editor
// ---------------------------------------------------------------------------

const INITIAL_CODE = `1_234_567               # u32
+1                      # i8
-1                      # i8
0xFF                    # u8
+0xFF                   # i8
0xFfFf                  # u16
0xFFFF_FFFF             # u32
0xFFFF_FFFF_FFFF_FFFF   # u64
0o_1234_5670            # octal
0b_0101_1111            # binary

1.0             # f32
1.0e100_000     # f64

1.0d            # decimal — cannot mix with floats
1.0d-100        # decimal with negative exponent

'hello \${1 + 2}'
":
  foo \${bar}

'\\n'        # newline
'\\t'        # tab
'\\\\'        # backslash
'\\''        # single quote
'\\x0f'      # hex code point
'\\u{10_ff_ff}'  # Unicode code point
fmt'hello \${name}'
raw":
  foo \\n
  bar

10sec       # == sec 10
10.5min     # == min 10.5
(foo)min    # == min foo

{foo: 1, bar: 2}
{foo: 1, 'ni na': 2, (key): 3}  # computed keys

-a          # unary minus
a + b
a - b
a * b
a / b
a // b      # integer divide
a ** b      # power
a % b       # remainder (sign follows dividend)
a %% b      # true modulus (sign follows divisor)
a /% b      # divmod — returns [quotient, remainder]

not a
a and b
a or b
a xor b

~a          # not
a & b       # and
a ^ b       # xor
a >> b      # shift right
a << b      # shift left
a >>> b     # rotate right
a <<< b     # rotate left

a == b
a != b
a > b
a >= b
a < b
a <= b
a > b > c       # chained
a in b
a not in b
a >< b          # disjoint

0..10           # exclusive end
0...10          # inclusive end
'a'...'z'       # char range
start..end
(1 + 2)..(3 + 4)

[head, ..tail]
[..seq1, ..seq2]    # concat

{foo: bar, ..rest}
{..rec1, ..rec2}    # merge

foo
  arg1
  arg2
|= result


match foo:
  1: 'one'
  2: 'two'
  _: 'other'


add = fn a, b:
  result = a + b
  result

log 'hello'
add 1, 2

# nested — right to left
foo bar spam ham
# == foo (bar (spam ham))

# multiline — indented args
add
  mul 2, 3
  mul 3, 4


add5 = add 5, ?
add5 = ? + 5


1..10
| filter ? % 2 == 0
| map ? * 2
| [..?]
|= even_nums


a = try bar a
`

const editorEl = document.getElementById('editor')!
const editor = monaco.editor.create(editorEl, {
  value: INITIAL_CODE,
  language: 'fink',
  theme: 'fink-dark',
  fontSize: 14,
  fontFamily: '"Hack", "Consolas", "Menlo", monospace',
  minimap: { enabled: false },
  bracketPairColorization: { enabled: true },
  scrollBeyondLastLine: false,
  'semanticHighlighting.enabled': true,
  padding: { top: 16, bottom: 16 },
  lineNumbers: 'on',
  automaticLayout: true,
})

// ---------------------------------------------------------------------------
// Run
// ---------------------------------------------------------------------------

const runBtn = document.getElementById('run-btn') as HTMLButtonElement
const outputEl = document.getElementById('output')!

runBtn.addEventListener('click', async () => {
  runBtn.disabled = true
  outputEl.textContent = '…'
  outputEl.className = 'running'

  try {
    const src = editor.getValue()
    const wasm = await compile(src)
    const result = await run(wasm)

    const text = result.stdout + result.stderr
    outputEl.textContent = text || '(no output)'
    outputEl.className = result.exitCode === 0 ? 'ok' : 'error'
  } catch (err) {
    outputEl.textContent = `Error: ${err}`
    outputEl.className = 'error'
  } finally {
    runBtn.disabled = false
  }
})

// ---------------------------------------------------------------------------
// Init
// ---------------------------------------------------------------------------

loadAnalysisWasm().catch(err => {
  console.error('Analysis WASM failed to load — semantic tokens disabled:', err)
})

loadTmGrammar(editor).catch(err => {
  console.error('TM grammar failed to load — falling back to plain tokenization:', err)
})
