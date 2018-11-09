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

use std::fs;
use std::path::Path;

use convert::convert;

extern crate gimli;
#[macro_use]
extern crate serde_json;
extern crate vlq;

mod convert;
mod dwarf;
mod to_json;
mod wasm;

const INPUT_FILE: &str = "/Users/yury/Work/old-man-sandbox/rust-wasm-hey/hey.dbg.wasm";
const OUTPUT_FILE: &str = "./test.json";

fn main() {
    let wasm = fs::read(Path::new(INPUT_FILE)).expect("failed to read wasm input");
    let json = convert(&wasm, true).expect("json");
    fs::write(Path::new(OUTPUT_FILE), &json).expect("failed to write JSON");
}
