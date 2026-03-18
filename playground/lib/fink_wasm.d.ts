/* tslint:disable */
/* eslint-disable */

/**
 * Stateful parsed document - parse once, query many times.
 * Stores only owned data: no borrows, no lifetimes.
 */
export class ParsedDocument {
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Look up the definition site for the identifier at (line, col).
     * Returns [def_line, def_col, def_end_line, def_end_col] or empty.
     */
    get_definition(line: number, col: number): Uint32Array;
    /**
     * Return JSON diagnostics string.
     */
    get_diagnostics(): string;
    /**
     * Find all references to the identifier at (line, col), including the binding site.
     * Returns [line, col, end_line, end_col, ...] (4 u32s per location) or empty.
     * First entry is always the binding site.
     */
    get_references(line: number, col: number): Uint32Array;
    /**
     * Return delta-encoded semantic tokens.
     */
    get_semantic_tokens(): Uint32Array;
    /**
     * Parse source code and pre-compute all provider data.
     */
    constructor(src: string);
}

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_parseddocument_free: (a: number, b: number) => void;
    readonly parseddocument_get_definition: (a: number, b: number, c: number) => [number, number];
    readonly parseddocument_get_diagnostics: (a: number) => [number, number];
    readonly parseddocument_get_references: (a: number, b: number, c: number) => [number, number];
    readonly parseddocument_get_semantic_tokens: (a: number) => [number, number];
    readonly parseddocument_new: (a: number, b: number) => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
