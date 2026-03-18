// Placeholder compiler: ignores the source text and returns a pre-compiled
// "Hello from Fink!" WASM module so the full run pipeline can be exercised
// end-to-end while the real Fink codegen WASM is being developed.
//
// TODO: replace with a call to the real Fink compiler WASM once it exposes
//       a compile(src: string) -> Uint8Array entry point.
//
// placeholder.wasm is inlined by esbuild's `binary` loader at build time
// (it's only 145 bytes so there's no reason to serve it as a separate file).
// eslint-disable-next-line @typescript-eslint/ban-ts-comment
// @ts-ignore: esbuild binary loader provides a Uint8Array default export
import placeholderWasm from './placeholder.wasm'

export async function compile(_src: string): Promise<Uint8Array> {
  return placeholderWasm as Uint8Array
}
