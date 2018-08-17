use dwarf::{LocationInfo, DebugInfoObj, DebugAttrValue};
use rustc_serialize::json::{Json, ToJson};
use std::collections::BTreeMap;
use std::str;
use std::io::Write;
use vlq::encode;

pub fn convert_scopes(infos: &Vec<DebugInfoObj>) -> Json {
    let mut result = Vec::new();
    for entry in infos {
        let mut dict = BTreeMap::new();
        dict.insert("tag".to_string(), entry.tag.to_json());
        for (attr_name, attr_value) in entry.attrs.iter() {
            let value = match attr_value {
                DebugAttrValue::I64(i) => i.to_json(),
                DebugAttrValue::Bool(b) => b.to_json(),
                DebugAttrValue::String(s) => s.to_json(),
                DebugAttrValue::Ranges(ranges) => {
                    let mut r = Vec::new();
                    for range in ranges {
                        r.push(vec![range.0, range.1].to_json());
                    }
                    r.to_json()
                },
                DebugAttrValue::Expression => "<expr>".to_json(),
                DebugAttrValue::Ignored => "<ignored>".to_json(),
                DebugAttrValue::Unknown => "???".to_json(),
            };
            dict.insert(attr_name.to_string(), value);
        }
        if entry.children.len() > 0 {
            dict.insert("children".to_string(), convert_scopes(&entry.children));
        }
        result.push(Json::Object(dict));
    }
    result.to_json()
}

pub fn convert_debug_info_to_json(di: &LocationInfo, infos: Option<Vec<DebugInfoObj>>, code_section_offset: i64) -> Vec<u8> {
    let mut buffer = Vec::new();
    let mut last_address = 0;
    let mut last_source_id = 0;
    let mut last_line = 1;
    let mut last_column = 1;
    for loc in di.locations.iter() {
        if loc.line == 0 {
            continue;
        }
        let address_delta = loc.address as i64 + code_section_offset - last_address;
        encode(address_delta, &mut buffer).unwrap();
        let source_id_delta = loc.source_id as i64 - last_source_id;
        encode(source_id_delta, &mut buffer).unwrap();
        let line_delta = loc.line as i64 - last_line;
        encode(line_delta, &mut buffer).unwrap();
        let column = if loc.column == 0 { 1 } else { loc.column } as i64;
        let column_delta = column - last_column;
        encode(column_delta, &mut buffer).unwrap();
        buffer.push(b',');

        last_address = loc.address as i64 + code_section_offset;
        last_source_id = loc.source_id as i64;
        last_line = loc.line as i64;
        last_column = loc.column as i64;
    }

    if di.locations.len() > 0 {
        buffer.pop();
    }

    let mappings = str::from_utf8(&buffer).unwrap();
    let names: Vec<String> = Vec::new();

    let mut root = BTreeMap::new();
    root.insert("version".to_string(), 3.to_json());
    root.insert("sources".to_string(), di.sources.to_json());
    root.insert("names".to_string(), names.to_json());
    root.insert("mappings".to_string(), mappings.to_json());
    if infos.is_some() {
        let mut x_scopes = BTreeMap::new();
        x_scopes.insert("debug_info".to_string(), convert_scopes(&infos.unwrap()));
        x_scopes.insert("code_section_offset".to_string(), code_section_offset.to_json());
        root.insert("x-scopes".to_string(), Json::Object(x_scopes));
    }
    let mut result = Vec::new();
    result
        .write(&Json::Object(root).to_string().as_bytes())
        .expect("???");
    result
}
