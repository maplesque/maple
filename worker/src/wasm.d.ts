declare module "../pkg/maple.js" {
  // wasm-pack web target exports an async init
  const init: (module?: WebAssembly.Module | BufferSource | Response | RequestInfo | URL | string) => Promise<any>
  export default init
  export function render_gif(template_zip: BufferSource | number[], inputs: any, options: any): Uint8Array
  export function render_png(template_zip: BufferSource | number[], inputs: any, options: any): Uint8Array
  export function template_info(template_zip: BufferSource | number[]): any
}

declare module "../pkg/maple_bg.wasm" {
  const wasm: WebAssembly.Module | BufferSource
  export default wasm
}
