use wasm::WasmDecoder;
use dwarf::{get_debug_loc, get_debug_scopes};
use to_json::convert_debug_info_to_json;

use std::collections::HashMap;

const WASM_SECTION_CODE: u32 = 10;
const WASM_SECTION_CUSTOM: u32 = 0;

fn is_debug_section_name(section_name: &str) -> bool {
    section_name.len() >= 7 && &section_name[0..7] != ".debug_"
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
        if is_debug_section_name(section_name) {
            continue;
        }
        sections.insert(section_name, body);
    }
    (sections, code_section_start)
}



pub fn convert(input: &[u8], x_scopes: bool) -> Vec<u8> {
    let (sections, code_section_offset) = read_debug_sections(input);
    let info = get_debug_loc(&sections);
    let scopes = if x_scopes {
        Some(get_debug_scopes(&sections, &info.sources))
    } else {
        None
    };
    convert_debug_info_to_json(&info, scopes, code_section_offset.unwrap_or(0) as i64)
}
