/* tslint:disable */
/* eslint-disable */

export class Uno {
    free(): void;
    [Symbol.dispose](): void;
    cycles(): number;
    digital_read(pin: number): boolean;
    digital_write(pin: number, high: boolean): void;
    constructor(image: Uint8Array | number[], freq_hz: number);
    pc(): number;
    pin_mode(pin: number, mode: number): void;
    pin_state(pin: number): number;
    reset(): void;
    run(instructions: number): void;
    set_analog_input(channel: number, volts: number): void;
    take_serial(): Uint8Array;
}

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_uno_free: (a: number, b: number) => void;
    readonly uno_cycles: (a: number) => number;
    readonly uno_digital_read: (a: number, b: number) => [number, number, number];
    readonly uno_digital_write: (a: number, b: number, c: number) => [number, number];
    readonly uno_new: (a: number, b: number, c: number) => number;
    readonly uno_pc: (a: number) => number;
    readonly uno_pin_mode: (a: number, b: number, c: number) => [number, number];
    readonly uno_pin_state: (a: number, b: number) => [number, number, number];
    readonly uno_reset: (a: number) => void;
    readonly uno_run: (a: number, b: number) => void;
    readonly uno_set_analog_input: (a: number, b: number, c: number) => [number, number];
    readonly uno_take_serial: (a: number) => [number, number];
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __externref_table_dealloc: (a: number) => void;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
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
