
use std::fs;
use std::path::Path;

use convert::convert;

extern crate gimli;
extern crate rustc_serialize;
extern crate vlq;

mod convert;
mod wasm;
mod dwarf;
mod to_json;

const INPUT_FILE: &str = "/Users/yury/Work/old-man-sandbox/rust-wasm-hey/hey.wasm";
const OUTPUT_FILE: &str = "./test.json";

fn main() {
    let wasm = fs::read(Path::new(INPUT_FILE)).expect("failed to read wasm input");
    let json = convert(&wasm);
    fs::write(Path::new(OUTPUT_FILE), &json).expect("failed to write JSON");
}
