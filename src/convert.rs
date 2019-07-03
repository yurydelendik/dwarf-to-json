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

use crate::dwarf;
use crate::dwarf::{get_debug_loc, get_debug_scopes, LocationInfo};
use gimli;
use serde_json;
use crate::to_json::convert_debug_info_to_json;
use crate::wasm::{WasmDecoder, WasmFormatError};

use std::collections::HashMap;
use std::str;

const WASM_SECTION_CODE: u32 = 10;
const WASM_SECTION_CUSTOM: u32 = 0;

#[derive(Debug)]
pub enum Error {
    GimliError(gimli::Error),
    DataFormat,
    WasmError,
    OutputError,
}

impl From<dwarf::Error> for Error {
    fn from(err: dwarf::Error) -> Self {
        match err {
            dwarf::Error::GimliError(e) => Error::GimliError(e),
            dwarf::Error::MissingDwarfEntry | dwarf::Error::MissingSection
                                            | dwarf::Error::DataFormat => Error::DataFormat,
        }
    }
}

impl From<WasmFormatError> for Error {
    fn from(_: WasmFormatError) -> Self {
        Error::WasmError
    }
}

impl From<std::fmt::Error> for Error {
    fn from(_: std::fmt::Error) -> Self {
        Error::OutputError
    }
}

fn is_debug_section_name(section_name: &str) -> bool {
    section_name.len() >= 7 && &section_name[0..7] == ".debug_"
}

fn is_url_prefixes_name(section_name: &str) -> bool {
    section_name == "sourceURLPrefixes"
}

fn read_debug_sections(
    input: &[u8],
) -> Result<(HashMap<&str, &[u8]>, Option<usize>), WasmFormatError> {
    let (header, sections) = input.split_at(8);
    if header != b"\x00asm\x01\x00\x00\x00" {
        return Err(WasmFormatError);
    }
    let mut decoder = WasmDecoder::new(sections);
    let mut sections = HashMap::new();
    let mut code_section_start = None;
    while !decoder.eof() {
        let section_id = decoder.u32()?;
        let section_len = decoder.u32()?;
        if section_id != WASM_SECTION_CUSTOM {
            if section_id == WASM_SECTION_CODE {
                let offset_from_start = input.len() - decoder.len();
                code_section_start = Some(offset_from_start);
            }

            decoder.skip(section_len as usize)?;
            continue;
        }
        let pos = decoder.len();
        let section_name = decoder.str()?;
        let section_name_len = pos - decoder.len();
        let body = decoder.skip(section_len as usize - section_name_len)?;
        if !is_debug_section_name(section_name) && !is_url_prefixes_name(section_name) {
            continue;
        }
        sections.insert(section_name, body);
    }
    Ok((sections, code_section_start))
}

fn fix_source_urls(info: &mut LocationInfo, prefixes_bytes: &[u8]) -> Result<(), WasmFormatError> {
    let mut prefixes_decoder = WasmDecoder::new(prefixes_bytes);
    let prefixes_pairs: Vec<Vec<String>> =
        serde_json::from_str(prefixes_decoder.str()?).unwrap_or(vec![]);
    if prefixes_pairs.is_empty() {
        return Ok(());
    }
    for i in 0..info.sources.len() {
        let url = &mut info.sources[i];
        if let Some(found) = prefixes_pairs
            .iter()
            .find(|&x| url.starts_with(x[0].as_str()))
        {
            *url = {
                let (_, tail) = url.split_at(found[0].len());
                let mut result_url = String::from(found[1].as_str());
                result_url.push_str(tail);
                result_url
            };
        }
    }
    Ok(())
}

pub fn convert(input: &[u8], x_scopes: bool) -> Result<Vec<u8>, Error> {
    let (sections, code_section_offset) = read_debug_sections(input)?;
    let mut info = get_debug_loc(&sections)?;
    let scopes = if x_scopes {
        Some(get_debug_scopes(&sections, &mut info.sources)?)
    } else {
        None
    };
    if let Some(ref prefixes) = sections.get("sourceURLPrefixes") {
        fix_source_urls(&mut info, prefixes)?;
    }
    let json = convert_debug_info_to_json(&info, scopes, code_section_offset.unwrap_or(0) as i64)?;
    Ok(json)
}
