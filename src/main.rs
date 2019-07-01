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
use std::io::{self, Write};

use convert::convert;

extern crate gimli;
#[macro_use]
extern crate serde_json;
extern crate vlq;
extern crate clap;

use clap::{Arg, App};

mod convert;
mod dwarf;
mod to_json;
mod wasm;

fn main() {
    let matches = App::new("dwarf-to-json")
                          .version("0.1.10")
                          .author("Yury Delendik <ydelendik@mozilla.com>")
                          .arg(Arg::with_name("output")
                               .short("o")
                               .takes_value(true))
                          .arg(Arg::with_name("INPUT")
                               .required(true))
                          .get_matches();

    let input_path = matches.value_of("INPUT").unwrap();
    let wasm = fs::read(input_path).expect("failed to read wasm input");

    let json = convert(&wasm, true).expect("json");

    match matches.value_of("output") {
        Some(output_path) => fs::write(output_path, &json).expect("failed to write JSON"),
        None => {
            let stdout = io::stdout();
            stdout.lock().write(&json).expect("failed to write JSON");
        }
    }
}
