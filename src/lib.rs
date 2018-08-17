use std::slice;
use std::str;
use std::mem;

use convert::convert;

extern crate gimli;
#[macro_use]
extern crate serde_json;
extern crate vlq;

mod convert;
mod wasm;
mod dwarf;
mod to_json;

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
pub extern "C" fn convert_dwarf(wasm: *const u8, wasm_len: usize, output: *mut *const u8, output_len: *mut usize) {
  let wasm_bytes = unsafe {
    slice::from_raw_parts(wasm, wasm_len)
  };
  let json = convert(&wasm_bytes, false);
  unsafe {
    *output = alloc_mem(json.len()) as *const u8;
    *output_len = json.len();
    slice::from_raw_parts_mut(*output as *mut u8, *output_len).clone_from_slice(json.as_slice());
  };
}