/* tslint:disable */
/* eslint-disable */

/**
 * A novadb instance held entirely in memory and handed to JavaScript, so a
 * browser can run shutup with no server and no filesystem — one database
 * per page load.
 */
export class NovaDb {
    free(): void;
    [Symbol.dispose](): void;
    /**
     * The names of the collections that exist, as a JSON array.
     */
    collections(): string;
    constructor();
    /**
     * Runs shutup source and returns JSON shaped like the HTTP server's
     * reply: `[{"status":"OK","result":…}, …]`, or a single `ERR` carrying
     * the message, the fix, and where in the source to point.
     */
    run(source: string): string;
}

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_novadb_free: (a: number, b: number) => void;
    readonly novadb_collections: (a: number) => [number, number];
    readonly novadb_new: () => [number, number, number];
    readonly novadb_run: (a: number, b: number, c: number) => [number, number];
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __externref_table_dealloc: (a: number) => void;
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
