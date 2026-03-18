// Build script for the Fink playground.
//
// Outputs to ../build/playground/ so it can be run after `cargo run` (which
// builds the rest of the site into ../build/) without interfering with it.
//
// Steps:
//   1. Compile src/hello-world.wat → src/placeholder.wasm  (via wabt)
//   2. Bundle node_modules Monaco worker → editor.worker.js  (iife)
//   3. Bundle src/main.ts → playground.js  (esm)
//   4. Copy Monaco CSS + codicon font (fix relative font path)
//   5. Copy the analysis WASM files (served as plain static assets)
//   6. Copy index.html

import * as esbuild from 'esbuild'
import wabt from 'wabt'
import fs from 'fs'
import path from 'path'
import { createRequire } from 'module'

const require = createRequire(import.meta.url)
const OUT = '../build/playground'

fs.mkdirSync(OUT, { recursive: true })

// ---------------------------------------------------------------------------
// 1. Compile placeholder WAT → WASM
// ---------------------------------------------------------------------------

const wabtMod = await wabt()
const wat = fs.readFileSync('src/hello-world.wat', 'utf8')
const parsed = wabtMod.parseWat('hello-world.wat', wat)
const { buffer: wasmBytes } = parsed.toBinary({})
fs.writeFileSync('src/placeholder.wasm', Buffer.from(wasmBytes))
parsed.destroy()
console.log('  compiled hello-world.wat → src/placeholder.wasm')

// ---------------------------------------------------------------------------
// 2. Monaco editor worker (iife — workers don't use ES modules by default)
// ---------------------------------------------------------------------------

await esbuild.build({
  entryPoints: ['node_modules/monaco-editor/esm/vs/editor/editor.worker.js'],
  bundle: true,
  format: 'iife',
  outfile: `${OUT}/editor.worker.js`,
  minify: true,
})
console.log('  bundled editor.worker.js')

// ---------------------------------------------------------------------------
// 3. Main playground bundle (esm — keeps import.meta.url for asset URLs)
// ---------------------------------------------------------------------------

await esbuild.build({
  entryPoints: ['src/main.ts'],
  bundle: true,
  format: 'esm',
  outfile: `${OUT}/playground.js`,
  loader: {
    // Inline .wasm files as Uint8Array (binary loader). The placeholder is
    // only 145 bytes so inlining is fine; larger files should be fetched.
    '.wasm': 'binary',
    '.ttf': 'file',
  },
  // monaco-textmate uses Node's 'path' only for a debug basename call.
  alias: { path: './src/path-stub.js' },
  minify: false, // keep readable during development
})
console.log('  bundled playground.js')

// ---------------------------------------------------------------------------
// 4. Monaco CSS + codicon font
// ---------------------------------------------------------------------------

const monacoDir = path.dirname(require.resolve('monaco-editor/package.json'))
const cssPath = path.join(monacoDir, 'min/vs/editor/editor.main.css')
const codiconPath = path.join(
  monacoDir,
  'min/vs/base/browser/ui/codicons/codicon/codicon.ttf',
)

let css = fs.readFileSync(cssPath, 'utf8')

if (fs.existsSync(codiconPath)) {
  fs.copyFileSync(codiconPath, `${OUT}/codicon.ttf`)
  // Rewrite the relative font URL in the CSS to a flat same-directory path.
  css = css.replace(/url\([^)]*codicon\.ttf[^)]*\)/g, 'url(./codicon.ttf)')
  console.log('  copied codicon.ttf')
}

fs.writeFileSync(`${OUT}/monaco.css`, css)
console.log('  copied monaco.css')

// ---------------------------------------------------------------------------
// 5. TM grammar assets (onig.wasm + fink.tmLanguage.json)
//    vscode-textmate uses vscode-oniguruma.
//
//    Grammar is sourced from the project root (../fink.tmLanguage.json) and
//    transformed for playground use:
//      - Remove top-level include of source.jsx.fink (no JSX support needed)
//      - Replace all "include": "source.fink" self-references with "$self"
//        (avoids re-entrant grammar loads in vscode-textmate)
//      - Strip meta.scope-example.* rules (grammar documentation scaffolding
//        with empty begin/while patterns that cause infinite recursion)
// ---------------------------------------------------------------------------

const onigWasmSrc = path.join(
  path.dirname(require.resolve('vscode-oniguruma/package.json')),
  'release/onig.wasm',
)
fs.copyFileSync(onigWasmSrc, `${OUT}/onig.wasm`)
console.log('  copied onig.wasm')

{
  const grammarSrc = path.resolve('..', 'fink.tmLanguage.json')
  const grammar = JSON.parse(fs.readFileSync(grammarSrc, 'utf8'))

  // 1. Remove top-level source.jsx.fink include
  if (grammar.patterns) {
    grammar.patterns = grammar.patterns.filter(
      p => p.include !== 'source.jsx.fink',
    )
  }

  // 2. Walk the entire grammar tree and apply transforms to every node
  function transformNode(node) {
    if (Array.isArray(node)) {
      return node
        .filter(item => {
          // Strip meta.scope-example.* rules
          const name = item.name ?? item.scopeName
          return !name?.startsWith('meta.scope-example.')
        })
        .map(transformNode)
    }
    if (node && typeof node === 'object') {
      const out = {}
      for (const [k, v] of Object.entries(node)) {
        if (k === 'include' && v === 'source.fink') {
          out[k] = '$self'
        } else if (k === 'patterns' || k === 'repository' || k === 'captures' || k === 'beginCaptures' || k === 'endCaptures') {
          out[k] = transformNode(v)
        } else {
          out[k] = (v && typeof v === 'object') ? transformNode(v) : v
        }
      }
      return out
    }
    return node
  }

  const transformed = transformNode(grammar)
  fs.writeFileSync(`${OUT}/fink.tmLanguage.json`, JSON.stringify(transformed))
  console.log('  transformed + wrote fink.tmLanguage.json from ../fink.tmLanguage.json')
}

// ---------------------------------------------------------------------------
// 7. Analysis WASM (fink_wasm.js + fink_wasm_bg.wasm)
//    Served as plain static files; loaded at runtime via fetch + data-URL
//    dynamic import (same technique as vscode-fink extension).
// ---------------------------------------------------------------------------

for (const file of ['fink_wasm.js', 'fink_wasm_bg.wasm']) {
  const src = path.join('lib', file)
  if (!fs.existsSync(src)) {
    console.warn(`  WARNING: ${src} not found — semantic tokens will be disabled`)
    continue
  }
  fs.copyFileSync(src, `${OUT}/${file}`)
  console.log(`  copied lib/${file}`)
}

// ---------------------------------------------------------------------------
// 8. index.html
// ---------------------------------------------------------------------------

fs.copyFileSync('index.html', `${OUT}/index.html`)
console.log('  copied index.html')

console.log(`\nPlayground → ${OUT}/`)
