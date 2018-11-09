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

use std::mem;
use std::slice;

use convert::convert;

extern crate gimli;
#[macro_use]
extern crate serde_json;
extern crate vlq;

mod convert;
mod dwarf;
mod to_json;
mod wasm;

#[no_mangle]
pub extern "C" fn alloc_mem(size: usize) -> *mut u8 {
    let mut m = Vec::with_capacity(mem::size_of::<usize>() + size);
    unsafe {
        let p: *mut u8 = m.as_mut_ptr();
        *(p as *mut usize) = size;
        mem::forget(m);
        return p.offset(mem::size_of::<usize>() as isize);
    }
}

#[no_mangle]
pub extern "C" fn free_mem(p: *mut u8) {
    unsafe {
        let v = p.offset(-(mem::size_of::<usize>() as isize));
        let size = *(v as *mut usize);
        Vec::from_raw_parts(v, 0, size);
    }
}

#[no_mangle]
pub extern "C" fn convert_dwarf(
    wasm: *const u8,
    wasm_len: usize,
    output: *mut *const u8,
    output_len: *mut usize,
    enabled_x_scopes: bool,
) -> bool {
    let wasm_bytes = unsafe { slice::from_raw_parts(wasm, wasm_len) };
    match convert(&wasm_bytes, enabled_x_scopes) {
        Ok(json) => unsafe {
            *output = alloc_mem(json.len()) as *const u8;
            *output_len = json.len();
            slice::from_raw_parts_mut(*output as *mut u8, *output_len)
                .clone_from_slice(json.as_slice());
            true
        },
        Err(_) => {
            unsafe {
                *output_len = 0;
            }
            false
        }
    }
}
