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

use std::collections::HashMap;

use gimli;

use gimli::{DebugAbbrev, DebugInfo, DebugLine, DebugStr, DebugRanges, DebugRngLists, DebugLoc,
            DebugLocLists, RangeLists, LocationLists, LittleEndian, AttributeValue};

trait Reader: gimli::Reader<Offset = usize> {}

impl<'input, Endian> Reader for gimli::EndianSlice<'input, Endian>
where
    Endian: gimli::Endianity,
{
}

pub enum DebugAttrValue<'a> {
    I64(i64),
    Bool(bool),
    String(&'a str),
    Ranges(Vec<(i64, i64)>),
    Expression,
    Ignored,
    Unknown,
}
pub struct DebugInfoObj<'a> {
    pub tag: &'static str,
    pub attrs: HashMap<&'static str, DebugAttrValue<'a>>,
    pub children: Vec<DebugInfoObj<'a>>,
}

fn is_out_of_range(low_pc: i64, high_pc: i64) -> bool {
    let fn_size = (high_pc - low_pc) as u32;
    let fn_size_field_len = ((fn_size + 1).next_power_of_two().trailing_zeros() + 6) / 7;
    low_pc < 1 + fn_size_field_len as i64
}

fn is_subprogram(item: &DebugInfoObj) -> bool {
    if let Some(DebugAttrValue::String("subprogram")) = item.attrs.get("tag") {
        true
    } else {
        false
    }
}

fn is_inlined_subprogram(item: &DebugInfoObj) -> bool {
    item.attrs.get("inline").is_some()
}

fn remove_dead_functions(items: &mut Vec<DebugInfoObj>) {
    let mut dead = Vec::new();
    for (i, mut item) in items.iter_mut().enumerate() {
        if is_subprogram(&item) {
            let low_and_high_pc = {
                let low_pc = item.attrs.get("low_pc");
                if low_pc.is_some() {
                    let high_pc = item.attrs.get("high_pc");
                    if let (DebugAttrValue::I64(low_pc_val), DebugAttrValue::I64(high_pc_val)) =
                        (low_pc.unwrap(), high_pc.unwrap())
                    {
                        Some((*low_pc_val, *high_pc_val))
                    } else {
                        None
                    }
                } else {
                    None
                }
            };
            if let Some((low_pc_val, high_pc_val)) = low_and_high_pc {
                if is_out_of_range(low_pc_val, high_pc_val) {
                    if is_inlined_subprogram(&item) {
                        item.attrs.remove("low_pc");
                        item.attrs.remove("high_pc");
                    } else {
                        dead.push(i);
                    }
                    continue;
                }
            }
        }

        let present_ranges_are_empty =
            if let Some(DebugAttrValue::Ranges(ref mut ranges)) = item.attrs.get_mut("ranges") {
                let mut i = 0;
                while i != ranges.len() {
                    if is_out_of_range(ranges[i].0, ranges[i].1) {
                        ranges.remove(i);
                    } else {
                        i += 1;
                    }
                }
                ranges.len() == 0
            } else {
                false
            };
        if present_ranges_are_empty && is_subprogram(&item) {
            if is_inlined_subprogram(&item) {
                item.attrs.remove("ranges");
            } else {
                dead.push(i);
            }
            continue;
        }

        if item.children.len() > 0 {
            remove_dead_functions(&mut item.children);
        }
    }
    for i in dead.iter().rev() {
        items.remove(*i);
    }
}

fn enum_to_str(s: Option<&'static str>) -> DebugAttrValue {
    let s1 = s.unwrap();
    let (_dw, s2) = s1.split_at(s1.find('_').unwrap() + 1);
    let (_dw, s3) = s2.split_at(s2.find('_').unwrap() + 1);
    DebugAttrValue::String(s3)
}

struct UnitInfos<R: Reader> {
    address_size: u8,
    base_address: u64,
    line_program: Option<gimli::IncompleteLineNumberProgram<R>>,
    comp_dir: Option<R>,
    comp_name: Option<R>,
}

fn get_source_id<R: Reader>(
    sources: &mut Vec<String>,
    unit: &UnitInfos<R>,
    file_index: u64,
) -> i64 {
    const INVALID_INDEX: i64 = -1;
    if file_index == 0 {
        return INVALID_INDEX;
    }
    let header = match unit.line_program {
        Some(ref program) => program.header(),
        None => return INVALID_INDEX,
    };
    let file = match header.file(file_index) {
        Some(header) => header,
        None => return INVALID_INDEX,
    };

    let mut file_name: String = String::from(file.path_name().to_string_lossy().unwrap());
    if let Some(directory) = file.directory(header) {
        let directory = directory.to_string_lossy().unwrap();
        let prefix = if !directory.starts_with('/') {
            if let Some(ref comp_dir) = unit.comp_dir {
                format!("{}/", comp_dir.to_string_lossy().unwrap())
            } else {
                String::from("")
            }
        } else {
            String::from("")
        };
        file_name = format!("{}{}/{}", prefix, directory, &file_name);
    }
    (if let Some(position) = sources.iter().position(|&ref x| *x == file_name) {
         position
     } else {
         let id = sources.len();
         sources.push(file_name);
         id
     }) as i64
}

fn decode_data2(d: &[u8]) -> i64 {
    (d[0] as i64) | ((d[1] as i64) << 8)
}

fn decode_data4(d: &[u8]) -> i64 {
    (d[0] as i64) | ((d[1] as i64) << 8) | ((d[2] as i64) << 16) | ((d[3] as i64) << 24)
}

pub fn get_debug_scopes<'b>(
    debug_sections: &'b HashMap<&str, &[u8]>,
    sources: &mut Vec<String>,
) -> Vec<DebugInfoObj<'b>> {
    // see https://gist.github.com/yurydelendik/802f36983d50cedb05f984d784dc5159
    let ref debug_str = DebugStr::new(&debug_sections[".debug_str"], LittleEndian);
    let ref debug_abbrev = DebugAbbrev::new(&debug_sections[".debug_abbrev"], LittleEndian);
    let ref debug_info = DebugInfo::new(&debug_sections[".debug_info"], LittleEndian);
    let ref debug_line = DebugLine::new(&debug_sections[".debug_line"], LittleEndian);

    let debug_ranges = DebugRanges::new(&debug_sections[".debug_ranges"], LittleEndian);
    let debug_rnglists = DebugRngLists::new(&[], LittleEndian);
    let rnglists = RangeLists::new(debug_ranges, debug_rnglists).expect("Should parse rnglists");

    // let debug_loc = DebugLoc::new(&debug_sections[".debug_loc"], LittleEndian);
    // let debug_loclists = DebugLocLists::new(&[], LittleEndian);
    // let loclists = LocationLists::new(debug_loc, debug_loclists).expect("Should parse loclists");

    let mut iter = debug_info.units();
    let mut info = Vec::new();
    while let Some(unit) = iter.next().unwrap_or(None) {
        let mut unit_infos = UnitInfos {
            address_size: unit.address_size(),
            base_address: 0,
            comp_dir: None,
            comp_name: None,
            line_program: None,
        };
        let abbrevs = unit.abbreviations(debug_abbrev).unwrap();

        let mut stack: Vec<DebugInfoObj> = Vec::new();
        stack.push(DebugInfoObj {
            tag: &"",
            attrs: HashMap::new(),
            children: Vec::new(),
        });
        // Iterate over all of this compilation unit's entries.
        let mut entries = unit.entries(&abbrevs);
        while let Some((depth_delta, entry)) = entries.next_dfs().expect("entry") {
            if entry.tag() == gimli::DW_TAG_compile_unit || entry.tag() == gimli::DW_TAG_type_unit {
                unit_infos.base_address =
                    match entry.attr_value(gimli::DW_AT_low_pc).expect("low_pc") {
                        Some(AttributeValue::Addr(address)) => address,
                        _ => 0,
                    };
                unit_infos.comp_dir = entry
                    .attr(gimli::DW_AT_comp_dir)
                    .expect("comp_dir")
                    .and_then(|attr| attr.string_value(debug_str));
                unit_infos.comp_name = entry.attr(gimli::DW_AT_name).expect("name").and_then(
                    |attr| {
                        attr.string_value(debug_str)
                    },
                );
                unit_infos.line_program =
                    match entry.attr_value(gimli::DW_AT_stmt_list).expect("stmt_list") {
                        Some(AttributeValue::DebugLineRef(offset)) => {
                            debug_line
                                .program(
                                    offset,
                                    unit_infos.address_size,
                                    unit_infos.comp_dir.clone(),
                                    unit_infos.comp_name.clone(),
                                )
                                .ok()
                        }
                        _ => None,
                    }
            }

            let tag_value = &entry.tag().static_string().unwrap()[ /*DW_TAG_*/ 7..];
            let mut attrs_values = HashMap::new();
            let mut attrs = entry.attrs();
            while let Some(attr) = attrs.next().unwrap() {
                let attr_name = &attr.name().static_string().unwrap()[ /*DW_AT_*/ 6 ..];
                let attr_value = match attr.value() {
                    AttributeValue::Addr(u) => DebugAttrValue::I64(u as i64),
                    AttributeValue::Udata(u) => {
                        if attr_name != "high_pc" {
                            DebugAttrValue::I64(u as i64)
                        } else {
                            DebugAttrValue::I64(
                                u as i64 +
                                    (if let Some(DebugAttrValue::I64(low_pc)) =
                                        attrs_values.get("low_pc")
                                    {
                                         *low_pc
                                     } else {
                                         0
                                     }),
                            )
                        }
                    }
                    AttributeValue::Data1(u) => DebugAttrValue::I64(u[0] as i64),
                    AttributeValue::Data2(u) => DebugAttrValue::I64(decode_data2(&u.0)),
                    AttributeValue::Data4(u) => DebugAttrValue::I64(decode_data4(&u.0)),
                    AttributeValue::Sdata(i) => DebugAttrValue::I64(i),
                    AttributeValue::DebugLineRef(o) => DebugAttrValue::I64(o.0 as i64),
                    AttributeValue::Flag(f) => DebugAttrValue::Bool(f),
                    AttributeValue::FileIndex(i) => DebugAttrValue::I64(
                        get_source_id(sources, &unit_infos, i),
                    ),
                    AttributeValue::DebugStrRef(str_offset) => DebugAttrValue::String(
                        debug_str
                            .get_str(str_offset)
                            .expect("string")
                            .to_string()
                            .expect("???"),
                    ),
                    AttributeValue::RangeListsRef(r) => {
                        let low_pc = 0;
                        let mut ranges = rnglists
                            .ranges(r, unit.version(), unit.address_size(), low_pc)
                            .expect("Should parse ranges OK");
                        let mut result = Vec::new();
                        while let Some(range) = ranges.next().expect("Should parse next range") {
                            assert!(range.begin <= range.end);
                            result.push((range.begin as i64, range.end as i64));
                        }
                        DebugAttrValue::Ranges(result)
                    }
                    // AttributeValue::LocationListsRef(r) => {
                    //     let low_pc = 0;
                    //     let mut locs = loclists
                    //       .locations(r, unit.version(), unit.address_size(), low_pc)
                    //       .expect("Should parse locations OK");
                    //     while let Some(loc) = locs.next().expect("Should parse next location") {
                    //         assert!(loc.range.begin <= loc.range.end);
                    //     }
                    //     DebugAttrValue::Ignored
                    // },
                    AttributeValue::Exprloc(_expr) => DebugAttrValue::Expression,
                    AttributeValue::Encoding(e) => enum_to_str(e.static_string()),
                    AttributeValue::DecimalSign(e) => enum_to_str(e.static_string()),
                    AttributeValue::Endianity(e) => enum_to_str(e.static_string()),
                    AttributeValue::Accessibility(e) => enum_to_str(e.static_string()),
                    AttributeValue::Visibility(e) => enum_to_str(e.static_string()),
                    AttributeValue::Virtuality(e) => enum_to_str(e.static_string()),
                    AttributeValue::Language(e) => enum_to_str(e.static_string()),
                    AttributeValue::AddressClass(e) => enum_to_str(e.static_string()),
                    AttributeValue::IdentifierCase(e) => enum_to_str(e.static_string()),
                    AttributeValue::CallingConvention(e) => enum_to_str(e.static_string()),
                    AttributeValue::Inline(e) => enum_to_str(e.static_string()),
                    AttributeValue::Ordering(e) => enum_to_str(e.static_string()),
                    AttributeValue::UnitRef(offset) => {
                        let mut unit_entries =
                            unit.entries_at_offset(&abbrevs, offset).expect("unitref");
                        unit_entries.next_entry().unwrap();
                        let entry = unit_entries.current().expect("unitentry");
                        let name = if let Some(AttributeValue::DebugStrRef(str_offset)) =
                            entry.attr_value(gimli::DW_AT_linkage_name).expect("unitref attr")
                        {
                            debug_str
                                .get_str(str_offset)
                                .expect("string")
                                .to_string()
                                .expect("???")
                        } else {
                            ""
                        };
                        DebugAttrValue::String(name)
                    }
                    AttributeValue::DebugInfoRef(_) => {
                        // Types and stuff
                        DebugAttrValue::Ignored
                    }
                    _ => DebugAttrValue::Unknown,
                };
                attrs_values.insert(attr_name, attr_value);
            }
            if depth_delta <= 0 && stack.len() > 1 {
                for _ in 0..1 - depth_delta {
                    let past = stack.pop().unwrap();
                    stack.last_mut().unwrap().children.push(past);
                }
            }
            let new_info = DebugInfoObj {
                tag: tag_value,
                attrs: attrs_values,
                children: Vec::new(),
            };
            stack.push(new_info);
        }
        while stack.len() > 1 {
            let past = stack.pop().unwrap();
            stack.last_mut().unwrap().children.push(past);
        }
        info.append(&mut stack.pop().unwrap().children);
    }
    remove_dead_functions(&mut info);
    info
}

pub struct LocationRecord {
    pub address: u64,
    pub source_id: u32,
    pub line: u32,
    pub column: u32,
}

pub struct LocationInfo {
    pub sources: Vec<String>,
    pub locations: Vec<LocationRecord>,
}

pub fn get_debug_loc(debug_sections: &HashMap<&str, &[u8]>) -> LocationInfo {
    let mut sources = Vec::new();
    let mut locations: Vec<LocationRecord> = Vec::new();
    let mut source_to_id_map: HashMap<u64, usize> = HashMap::new();

    let ref debug_str = DebugStr::new(&debug_sections[".debug_str"], LittleEndian);
    let ref debug_abbrev = DebugAbbrev::new(&debug_sections[".debug_abbrev"], LittleEndian);
    let ref debug_info = DebugInfo::new(&debug_sections[".debug_info"], LittleEndian);
    let ref debug_line = DebugLine::new(&debug_sections[".debug_line"], LittleEndian);

    let mut iter = debug_info.units();
    while let Some(unit) = iter.next().unwrap_or(None) {
        let abbrevs = unit.abbreviations(debug_abbrev).unwrap();
        let mut cursor = unit.entries(&abbrevs);
        cursor.next_dfs().expect("???");
        let root = cursor.current().expect("missing die");
        let offset = match root.attr_value(gimli::DW_AT_stmt_list).unwrap() {
            Some(gimli::AttributeValue::DebugLineRef(offset)) => offset,
            _ => continue,
        };
        let comp_dir = root.attr(gimli::DW_AT_comp_dir).unwrap().and_then(|attr| {
            attr.string_value(debug_str)
        });
        let comp_name = root.attr(gimli::DW_AT_name).unwrap().and_then(|attr| {
            attr.string_value(debug_str)
        });
        let program = debug_line.program(offset, unit.address_size(), comp_dir, comp_name);
        let mut block_start_loc = locations.len();
        if let Ok(program) = program {
            let mut rows = program.rows();
            while let Some((header, row)) = rows.next_row().unwrap() {
                let pc = row.address();
                let line = row.line().unwrap_or(0);
                let column = match row.column() {
                    gimli::ColumnType::Column(column) => column,
                    gimli::ColumnType::LeftEdge => 0,
                };
                let file_index = row.file_index();
                let source_id = if !source_to_id_map.contains_key(&file_index) {
                    let file_path: String = if let Some(file) = row.file(header) {
                        if let Some(directory) = file.directory(header) {
                            format!(
                                "{}/{}",
                                directory.to_string_lossy(),
                                file.path_name().to_string_lossy()
                            )
                        } else {
                            String::from(file.path_name().to_string_lossy())
                        }
                    } else {
                        String::from("<unknown>")
                    };
                    sources
                        .iter()
                        .position(|&ref p| *p == file_path)
                        .unwrap_or_else(|| {
                            let index = sources.len();
                            sources.push(file_path);
                            source_to_id_map.insert(file_index, index);
                            index
                        })
                } else {
                    *source_to_id_map.get(&file_index).unwrap() as usize
                };
                let mut loc = LocationRecord {
                    address: pc,
                    source_id: source_id as u32,
                    line: line as u32,
                    column: column as u32,
                };
                let end_sequence = if row.end_sequence() {
                    // end_sequence falls on the byte after function's end --
                    // moving address one step back.
                    loc.address -= 1;
                    // Compacting duplicate records.
                    if locations[locations.len() - 1].address < loc.address {
                        locations.push(loc);
                    }
                    true
                } else {
                    locations.push(loc);
                    false
                };
                if end_sequence {
                    // Heuristic to remove dead functions.
                    let block_end_loc = locations.len() - 1;
                    let fn_size = locations[block_end_loc].address -
                        locations[block_start_loc].address + 1;
                    let fn_size_field_len = ((fn_size + 1).next_power_of_two().trailing_zeros() +
                                                 6) / 7;
                    // Remove function if it starts at its size field location.
                    if locations[block_start_loc].address <= fn_size_field_len as u64 {
                        locations.drain(block_start_loc..);
                    }
                    block_start_loc = locations.len();
                }
            }
        }

        // new unit, new sources
        source_to_id_map.clear();
    }

    locations.sort_by(|a, b| a.address.cmp(&b.address));

    LocationInfo { sources, locations }
}
