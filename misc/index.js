/* Copyright 2018 Mozilla Foundation
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

 import { alloc_mem, free_mem, convert_dwarf, memory } from "./dwarf_to_json";

let utf8Decoder;

export function convertDwarfToJSON(wasm) {
  const wasmPtr = alloc_mem(wasm.byteLength);
  new Uint8Array(memory.buffer, wasmPtr, wasm.byteLength).set(new Uint8Array(wasm));
  const resultPtr = alloc_mem(12);
  convert_dwarf(wasmPtr, wasm.byteLength, resultPtr, resultPtr + 4);
  free_mem(wasmPtr);
  const resultView = new DataView(memory.buffer, resultPtr, 12);
  const outputPtr = resultView.getUint32(0, true), outputLen = resultView.getUint32(4, true);
  free_mem(resultPtr);
  if (utf8Decoder) {
    utf8Decoder = new TextDecoder("utf-8");
  }
  const output = utf8Decoder.decode(new Uint8Array(memory.buffer, outputPtr, outputLen));
  free_mem(outputPtr);
  return output;
}
