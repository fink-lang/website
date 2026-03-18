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
import { compile } from './compiler.js'
import { run } from './wasi-shim.js'

// ---------------------------------------------------------------------------
// Analysis WASM (semantic tokens, diagnostics)
// Loaded via data-URL dynamic import, same as vscode-fink, so esbuild does
// not try to bundle the wasm-bindgen glue and rewrite its internal URL.
// ---------------------------------------------------------------------------

// eslint-disable-next-line @typescript-eslint/no-explicit-any
let ParsedDocument: any = null

async function loadAnalysisWasm(): Promise<void> {
  // Derive the base URL of this module so assets are found regardless of
  // where the playground is deployed.
  const base = new URL('.', import.meta.url).href

  const [wasmJs, wasmBin] = await Promise.all([
    fetch(`${base}fink_wasm.js`).then(r => {
      if (!r.ok) throw new Error(`fink_wasm.js: ${r.status}`)
      return r.text()
    }),
    fetch(`${base}fink_wasm_bg.wasm`).then(r => {
      if (!r.ok) throw new Error(`fink_wasm_bg.wasm: ${r.status}`)
      return r.arrayBuffer()
    }),
  ])

  const dataUrl = `data:text/javascript;base64,${btoa(wasmJs)}`
  const mod = await import(/* @vite-ignore */ dataUrl)
  await mod.default(wasmBin)
  ParsedDocument = mod.ParsedDocument
}

// ---------------------------------------------------------------------------
// Language registration
// ---------------------------------------------------------------------------

monaco.languages.register({ id: 'fink', extensions: ['.fnk'] })

// Monarch grammar provides basic tokenisation (strings, comments, numbers,
// keywords). Semantic tokens layer on top with full AST-based coloring.
monaco.languages.setMonarchTokensProvider('fink', {
  keywords: [
    'fn', 'let', 'if', 'else', 'match', 'import', 'from', 'export',
    'type', 'as', 'true', 'false', 'and', 'or', 'not', 'xor', 'in',
  ],
  tokenizer: {
    root: [
      [/\/\/.*/, 'comment'],
      [/\/\*/, 'comment', '@block_comment'],
      [/"([^"\\]|\\.)*"/, 'string'],
      [/'([^'\\]|\\.)*'/, 'string'],
      [/\d+(\.\d+)?([eE][+-]?\d+)?/, 'number.float'],
      [/[a-zA-Z_]\w*/, { cases: { '@keywords': 'keyword', '@default': 'identifier' } }],
      [/[+\-*\/=<>!&|^~?:;.,@#$%`\\]/, 'operator'],
      [/[{}()\[\]]/, 'delimiter.bracket'],
    ],
    block_comment: [
      [/[^/*]+/, 'comment'],
      [/\*\//, 'comment', '@pop'],
      [/[/*]/, 'comment'],
    ],
  },
})

// Semantic token legend must match TOKEN_* constants in vscode-fink/src/lib.rs
const TOKEN_TYPES = ['function', 'variable', 'property', 'block-name', 'tag-left', 'tag-right']
const TOKEN_MODIFIERS = ['readonly']

monaco.languages.registerDocumentSemanticTokensProvider('fink', {
  getLegend() {
    return { tokenTypes: TOKEN_TYPES, tokenModifiers: TOKEN_MODIFIERS }
  },
  provideDocumentSemanticTokens(model) {
    if (!ParsedDocument) return { data: new Uint32Array(0) }
    const doc = new ParsedDocument(model.getValue())
    const data = doc.get_semantic_tokens()
    doc.free()
    return { data, resultId: undefined }
  },
  releaseDocumentSemanticTokens() {},
})

// ---------------------------------------------------------------------------
// Theme
// ---------------------------------------------------------------------------

monaco.editor.defineTheme('fink-dark', {
  base: 'vs-dark',
  inherit: true,
  rules: [],
  colors: {},
})

// ---------------------------------------------------------------------------
// Editor
// ---------------------------------------------------------------------------

const INITIAL_CODE = `greet = fn name:
  'Hello, \${name}!'
`

const editorEl = document.getElementById('editor')!
const editor = monaco.editor.create(editorEl, {
  value: INITIAL_CODE,
  language: 'fink',
  theme: 'fink-dark',
  fontSize: 14,
  fontFamily: '"Hack", "Consolas", "Menlo", monospace',
  minimap: { enabled: false },
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
  console.warn('Analysis WASM failed to load — semantic tokens disabled:', err)
})
