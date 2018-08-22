use wasm::WasmDecoder;
use dwarf::{get_debug_loc, get_debug_scopes, LocationInfo};
use to_json::convert_debug_info_to_json;
use serde_json;

use std::str;
use std::collections::HashMap;

const WASM_SECTION_CODE: u32 = 10;
const WASM_SECTION_CUSTOM: u32 = 0;

fn is_debug_section_name(section_name: &str) -> bool {
    section_name.len() >= 7 && &section_name[0..7] == ".debug_"
}

fn is_url_prefixes_name(section_name: &str) -> bool {
    section_name == "sourceURLPrefixes"
}

fn read_debug_sections(input: &[u8]) -> (HashMap<&str, &[u8]>, Option<usize>) {
    let (_header, sections) = input.split_at(8);
    // TODO check header
    let mut decoder = WasmDecoder::new(sections);
    let mut sections = HashMap::new();
    let mut code_section_start = None;
    while !decoder.eof() {
        let section_id = decoder.u32();
        let section_len = decoder.u32();
        if section_id != WASM_SECTION_CUSTOM {
            if section_id == WASM_SECTION_CODE {
                let offset_from_start = input.len() - decoder.len();
                code_section_start = Some(offset_from_start);
            }

            decoder.skip(section_len as usize);
            continue;
        }
        let pos = decoder.len();
        let section_name = decoder.str();
        let section_name_len = pos - decoder.len();
        let body = decoder.skip(section_len as usize - section_name_len);
        if !is_debug_section_name(section_name) && !is_url_prefixes_name(section_name) {
            continue;
        }
        sections.insert(section_name, body);
    }
    (sections, code_section_start)
}

fn fix_source_urls(info: &mut LocationInfo, prefixes_bytes: &[u8]) {
    let mut prefixes_decoder = WasmDecoder::new(prefixes_bytes);
    let prefixes_pairs: Vec<Vec<String>> =
        serde_json::from_str(prefixes_decoder.str()).unwrap_or(vec![]);
    if prefixes_pairs.len() == 0 {
        return;
    }
    for i in 0..info.sources.len() {
        let url = &mut info.sources[i];
        if let Some(found) = prefixes_pairs.iter().find(
            |&x| url.starts_with(x[0].as_str()),
        )
        {
            *url = {
                let (_, tail) = url.split_at(found[0].len());
                let mut result_url = String::from(found[1].as_str());
                result_url.push_str(tail);
                result_url
            };
        }
    }
}

pub fn convert(input: &[u8], x_scopes: bool) -> Vec<u8> {
    let (sections, code_section_offset) = read_debug_sections(input);
    let mut info = get_debug_loc(&sections);
    let scopes = if x_scopes {
        Some(get_debug_scopes(&sections, &info.sources))
    } else {
        None
    };
    if let Some(ref prefixes) = sections.get("sourceURLPrefixes") {
        fix_source_urls(&mut info, prefixes);
    }
    convert_debug_info_to_json(&info, scopes, code_section_offset.unwrap_or(0) as i64)
}
