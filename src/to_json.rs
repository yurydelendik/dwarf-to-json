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

use crate::dwarf::{DebugAttrValue, DebugInfoObj, LocationInfo};
use serde_json::{to_vec_pretty, Map, Value};
use std::fmt::Error;
use std::fmt::Write as FmtWrite;
use std::str;
use vlq::encode;

fn convert_expr(a: &[u8]) -> Result<Value, Error> {
    let mut result = String::new();
    for i in a {
        write!(&mut result, "{:02X}", i)?;
    }
    Ok(json!(result))
}

pub fn convert_scopes(infos: &[DebugInfoObj]) -> Result<Value, Error> {
    let mut result = Vec::new();
    for entry in infos {
        let mut dict = Map::new();
        dict.insert("tag".to_string(), json!(entry.tag));
        for (attr_name, attr_value) in entry.attrs.iter() {
            let value = match attr_value {
                DebugAttrValue::I64(i) => json!(i),
                DebugAttrValue::Bool(b) => json!(b),
                DebugAttrValue::String(s) => json!(s),
                DebugAttrValue::Ranges(ranges) => {
                    let mut r = Vec::new();
                    for range in ranges {
                        r.push(vec![json!(range.0), json!(range.1)]);
                    }
                    json!(r)
                }
                DebugAttrValue::LocationList(list) => {
                    let mut r = Vec::new();
                    for item in list {
                        let mut dict = Map::new();
                        dict.insert(
                            "range".to_string(),
                            json!(vec![json!(item.0), json!(item.1)]),
                        );
                        dict.insert("expr".to_string(), convert_expr(item.2)?);
                        r.push(dict);
                    }
                    json!(r)
                }
                DebugAttrValue::Expression(expr) => convert_expr(expr)?,
                DebugAttrValue::UID(uid) => json!(uid),
                DebugAttrValue::UIDRef(uid, name) => {
                    let mut dict = Map::new();
                    dict.insert("uid".to_string(), json!(uid));
                    if let Some(s) = name {
                        dict.insert("name".to_string(), json!(s));
                    }
                    json!(dict)
                }
                DebugAttrValue::Ignored => json!("<ignored>"),
                DebugAttrValue::Unknown => json!("???"),
            };
            dict.insert(attr_name.to_string(), value);
        }
        if !entry.children.is_empty() {
            dict.insert("children".to_string(), convert_scopes(&entry.children)?);
        }
        result.push(json!(dict));
    }
    Ok(json!(result))
}

pub fn convert_debug_info_to_json(
    di: &LocationInfo,
    infos: Option<Vec<DebugInfoObj>>,
    code_section_offset: i64,
) -> Result<Vec<u8>, Error> {
    let mut buffer = Vec::new();
    let mut last_address = 0;
    let mut last_source_id = 0;
    let mut last_line = 0;
    let mut last_column = 0;
    for loc in di.locations.iter() {
        if loc.line == 0 {
            continue;
        }
        let address = loc.address as i64 + code_section_offset;
        let address_delta = address - last_address;
        encode(address_delta, &mut buffer).unwrap();
        let source_id = i64::from(loc.source_id);
        let source_id_delta = source_id - last_source_id;
        encode(source_id_delta, &mut buffer).unwrap();
        let line = i64::from(loc.line) - 1;
        let line_delta = line - last_line;
        encode(line_delta, &mut buffer).unwrap();
        let column = i64::from(if loc.column == 0 { 0 } else { loc.column - 1 });
        let column_delta = column - last_column;
        encode(column_delta, &mut buffer).unwrap();
        buffer.push(b',');

        last_address = address;
        last_source_id = source_id;
        last_line = line;
        last_column = column;
    }

    if !di.locations.is_empty() {
        buffer.pop();
    }

    let mappings = str::from_utf8(&buffer).unwrap();
    let names: Vec<String> = Vec::new();

    let mut root = Map::new();
    root.insert("version".to_string(), json!(3));
    root.insert("sources".to_string(), json!(di.sources));
    root.insert("names".to_string(), json!(names));
    root.insert("mappings".to_string(), json!(mappings));
    if infos.is_some() {
        let mut x_scopes = Map::new();
        x_scopes.insert("debug_info".to_string(), convert_scopes(&infos.unwrap())?);
        x_scopes.insert(
            "code_section_offset".to_string(),
            json!(code_section_offset),
        );
        root.insert("x-scopes".to_string(), json!(x_scopes));
    }
    to_vec_pretty(&json!(root)).map_err(|_| Error)
}
