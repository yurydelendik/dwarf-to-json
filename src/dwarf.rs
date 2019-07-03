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
use std::result::Result;

use gimli;

use gimli::{
    AttributeValue, DebugAbbrev, DebugInfo, DebugLine, DebugLoc, DebugLocLists, DebugRanges,
    DebugRngLists, DebugStr, LittleEndian, LocationLists, RangeLists
};

trait Reader: gimli::Reader<Offset = usize> {}

impl<'input, Endian> Reader for gimli::EndianSlice<'input, Endian> where Endian: gimli::Endianity {}

#[derive(Debug)]
pub enum Error {
    GimliError(gimli::Error),
    MissingDwarfEntry,
    MissingSection,
    DataFormat,
}

impl From<gimli::Error> for Error {
    fn from(err: gimli::Error) -> Self {
        Error::GimliError(err)
    }
}

pub enum DebugAttrValue<'a> {
    I64(i64),
    Bool(bool),
    String(&'a str),
    Ranges(Vec<(i64, i64)>),
    Expression(&'a [u8]),
    LocationList(Vec<(i64, i64, &'a [u8])>),
    UID(usize),
    UIDRef(usize, Option<&'a str>),
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
    low_pc < i64::from(1 + fn_size_field_len)
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
    for (i, item) in items.iter_mut().enumerate() {
        if is_subprogram(&item) {
            let low_and_high_pc = {
                let low_pc = item.attrs.get("low_pc");
                if low_pc.is_some() {
                    let high_pc = item.attrs.get("high_pc");
                    if let (
                        Some(DebugAttrValue::I64(low_pc_val)),
                        Some(DebugAttrValue::I64(high_pc_val)),
                    ) = (low_pc, high_pc)
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
                ranges.is_empty()
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

        if !item.children.is_empty() {
            remove_dead_functions(&mut item.children);
        }
    }
    for i in dead.iter().rev() {
        items.remove(*i);
    }
}

fn enum_to_str(s: Option<&'static str>) -> Result<DebugAttrValue, Error> {
    let s1 = s.ok_or(Error::DataFormat)?;
    let (_dw, s2) = s1.split_at(s1.find('_').ok_or(Error::DataFormat)? + 1);
    let (_dw, s3) = s2.split_at(s2.find('_').ok_or(Error::DataFormat)? + 1);
    Ok(DebugAttrValue::String(s3))
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
) -> Result<Option<i64>, Error> {
    if file_index == 0 {
        return Ok(None);
    }
    let header = match unit.line_program {
        Some(ref program) => program.header(),
        None => return Err(Error::MissingDwarfEntry),
    };
    let file = match header.file(file_index) {
        Some(header) => header,
        None => return Err(Error::MissingDwarfEntry),
    };

    let mut file_name: String = String::from(file.path_name().to_string_lossy()?);
    if let Some(directory) = file.directory(header) {
        let directory = directory.to_string_lossy()?;
        let prefix = if !directory.starts_with('/') {
            if let Some(ref comp_dir) = unit.comp_dir {
                format!("{}/", comp_dir.to_string_lossy()?)
            } else {
                String::from("")
            }
        } else {
            String::from("")
        };
        file_name = format!("{}{}/{}", prefix, directory, &file_name);
    }
    let id = (if let Some(position) = sources.iter().position(|x| *x == file_name) {
        position
    } else {
        let id = sources.len();
        sources.push(file_name);
        id
    }) as i64;
    Ok(Some(id))
}

fn decode_data2(d: &[u8]) -> i64 {
    (i64::from(d[0]) | i64::from(d[1]) << 8)
}

fn decode_data4(d: &[u8]) -> i64 {
    i64::from(d[0]) | (i64::from(d[1]) << 8) | (i64::from(d[2]) << 16) | (i64::from(d[3]) << 24)
}

pub fn get_debug_scopes<'b>(
    debug_sections: &'b HashMap<&str, &[u8]>,
    sources: &mut Vec<String>,
) -> Result<Vec<DebugInfoObj<'b>>, Error> {
    // see https://gist.github.com/yurydelendik/802f36983d50cedb05f984d784dc5159
    let debug_str = &DebugStr::new(&debug_sections[".debug_str"], LittleEndian);
    let debug_abbrev = &DebugAbbrev::new(&debug_sections[".debug_abbrev"], LittleEndian);
    let debug_info = &DebugInfo::new(&debug_sections[".debug_info"], LittleEndian);
    let debug_line = &DebugLine::new(&debug_sections[".debug_line"], LittleEndian);

    let debug_ranges = match debug_sections.get(".debug_ranges") {
        Some(section) => DebugRanges::new(section, LittleEndian),
        None => DebugRanges::new(&[], LittleEndian),
    };
    let debug_rnglists = DebugRngLists::new(&[], LittleEndian);
    let rnglists = RangeLists::new(debug_ranges, debug_rnglists)?;

    let debug_loc = match debug_sections.get(".debug_loc") {
        Some(section) => DebugLoc::new(section, LittleEndian),
        None => DebugLoc::new(&[], LittleEndian),
    };
    let debug_loclists = DebugLocLists::new(&[], LittleEndian);
    let loclists = LocationLists::new(debug_loc, debug_loclists)?;

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
        let abbrevs = unit.abbreviations(debug_abbrev)?;

        let mut stack: Vec<DebugInfoObj> = Vec::new();
        stack.push(DebugInfoObj {
            tag: &"",
            attrs: HashMap::new(),
            children: Vec::new(),
        });
        // Iterate over all of this compilation unit's entries.
        let mut entries = unit.entries(&abbrevs);
        while let Some((depth_delta, entry)) = entries.next_dfs()? {
            if entry.tag() == gimli::DW_TAG_compile_unit || entry.tag() == gimli::DW_TAG_type_unit {
                unit_infos.base_address = match entry.attr_value(gimli::DW_AT_low_pc)? {
                    Some(AttributeValue::Addr(address)) => address,
                    _ => 0,
                };
                unit_infos.comp_dir = entry
                    .attr(gimli::DW_AT_comp_dir)?
                    .and_then(|attr| attr.string_value(debug_str));
                unit_infos.comp_name = entry
                    .attr(gimli::DW_AT_name)?
                    .and_then(|attr| attr.string_value(debug_str));
                unit_infos.line_program = match entry.attr_value(gimli::DW_AT_stmt_list)? {
                    Some(AttributeValue::DebugLineRef(offset)) => debug_line
                        .program(
                            offset,
                            unit_infos.address_size,
                            unit_infos.comp_dir,
                            unit_infos.comp_name,
                        ).ok(),
                    _ => None,
                }
            }

            let mut attrs_values = HashMap::new();
            attrs_values.insert("uid", DebugAttrValue::UID(entry.offset().0));

            let tag_value = &entry.tag().static_string().unwrap()[ /*DW_TAG_*/ 7..];
            let mut attrs = entry.attrs();
            while let Some(attr) = attrs.next()? {
                let attr_name = &attr.name().static_string().unwrap()[ /*DW_AT_*/ 6 ..];
                let attr_value = match attr.value() {
                    AttributeValue::Addr(u) => DebugAttrValue::I64(u as i64),
                    AttributeValue::Udata(u) => {
                        if attr_name != "high_pc" {
                            DebugAttrValue::I64(u as i64)
                        } else {
                            DebugAttrValue::I64(
                                u as i64
                                    + (if let Some(DebugAttrValue::I64(low_pc)) =
                                        attrs_values.get("low_pc")
                                    {
                                        *low_pc
                                    } else {
                                        0
                                    }),
                            )
                        }
                    }
                    AttributeValue::Data1(u) => DebugAttrValue::I64(i64::from(u[0])),
                    AttributeValue::Data2(u) => DebugAttrValue::I64(decode_data2(&u.0)),
                    AttributeValue::Data4(u) => DebugAttrValue::I64(decode_data4(&u.0)),
                    AttributeValue::Sdata(i) => DebugAttrValue::I64(i),
                    AttributeValue::DebugLineRef(o) => DebugAttrValue::I64(o.0 as i64),
                    AttributeValue::Flag(f) => DebugAttrValue::Bool(f),
                    AttributeValue::FileIndex(i) => DebugAttrValue::I64(
                        get_source_id(sources, &unit_infos, i)?.unwrap_or(-1), // FIXME do we need -1?
                    ),
                    AttributeValue::DebugStrRef(str_offset) => {
                        DebugAttrValue::String(debug_str.get_str(str_offset)?.to_string()?)
                    }
                    AttributeValue::RangeListsRef(r) => {
                        let low_pc = 0;
                        let mut ranges =
                            rnglists.ranges(r, unit.version(), unit.address_size(), low_pc)?;
                        let mut result = Vec::new();
                        while let Some(range) = ranges.next()? {
                            assert!(range.begin <= range.end);
                            result.push((range.begin as i64, range.end as i64));
                        }
                        DebugAttrValue::Ranges(result)
                    }
                    AttributeValue::LocationListsRef(r) => {
                        let low_pc = 0;
                        let mut locs =
                            loclists.locations(r, unit.version(), unit.address_size(), low_pc)?;
                        let mut result = Vec::new();
                        while let Some(loc) = locs.next()? {
                            result.push((
                                loc.range.begin as i64,
                                loc.range.end as i64,
                                loc.data.0.slice(),
                            ));
                        }
                        DebugAttrValue::LocationList(result)
                    }
                    AttributeValue::Exprloc(ref expr) => {
                        DebugAttrValue::Expression(&expr.0.slice())
                    }
                    AttributeValue::Encoding(e) => enum_to_str(e.static_string())?,
                    AttributeValue::DecimalSign(e) => enum_to_str(e.static_string())?,
                    AttributeValue::Endianity(e) => enum_to_str(e.static_string())?,
                    AttributeValue::Accessibility(e) => enum_to_str(e.static_string())?,
                    AttributeValue::Visibility(e) => enum_to_str(e.static_string())?,
                    AttributeValue::Virtuality(e) => enum_to_str(e.static_string())?,
                    AttributeValue::Language(e) => enum_to_str(e.static_string())?,
                    AttributeValue::AddressClass(e) => enum_to_str(e.static_string())?,
                    AttributeValue::IdentifierCase(e) => enum_to_str(e.static_string())?,
                    AttributeValue::CallingConvention(e) => enum_to_str(e.static_string())?,
                    AttributeValue::Inline(e) => enum_to_str(e.static_string())?,
                    AttributeValue::Ordering(e) => enum_to_str(e.static_string())?,
                    AttributeValue::UnitRef(offset) => {
                        let mut unit_entries = unit.entries_at_offset(&abbrevs, offset)?;
                        unit_entries.next_entry()?;
                        let entry = unit_entries.current().ok_or(Error::MissingDwarfEntry)?;
                        let name = if let Some(AttributeValue::DebugStrRef(str_offset)) =
                            entry.attr_value(gimli::DW_AT_linkage_name)?
                        {
                            Some(debug_str.get_str(str_offset)?.to_string()?)
                        } else if let Some(AttributeValue::DebugStrRef(str_offset)) =
                            entry.attr_value(gimli::DW_AT_name)?
                        {
                            Some(debug_str.get_str(str_offset)?.to_string()?)
                        } else {
                            None
                        };
                        DebugAttrValue::UIDRef(offset.0, name)
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
    Ok(info)
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

pub fn get_debug_loc(debug_sections: &HashMap<&str, &[u8]>) -> Result<LocationInfo, Error> {
    let mut sources = Vec::new();
    let mut locations: Vec<LocationRecord> = Vec::new();
    let mut source_to_id_map: HashMap<u64, usize> = HashMap::new();

    let debug_str = &DebugStr::new(&debug_sections.get(".debug_str").ok_or(Error::MissingSection)?, LittleEndian);
    let debug_abbrev = &DebugAbbrev::new(&debug_sections.get(".debug_abbrev").ok_or(Error::MissingSection)?, LittleEndian);
    let debug_info = &DebugInfo::new(&debug_sections.get(".debug_info").ok_or(Error::MissingSection)?, LittleEndian);
    let debug_line = &DebugLine::new(&debug_sections.get(".debug_line").ok_or(Error::MissingSection)?, LittleEndian);

    let mut iter = debug_info.units();
    while let Some(unit) = iter.next().unwrap_or(None) {
        let abbrevs = unit.abbreviations(debug_abbrev)?;
        let mut cursor = unit.entries(&abbrevs);
        cursor.next_dfs()?;
        let root = cursor.current().ok_or(Error::MissingDwarfEntry)?;
        let offset = match root.attr_value(gimli::DW_AT_stmt_list)? {
            Some(gimli::AttributeValue::DebugLineRef(offset)) => offset,
            _ => continue,
        };
        let comp_dir = root
            .attr(gimli::DW_AT_comp_dir)?
            .and_then(|attr| attr.string_value(debug_str));
        let comp_name = root
            .attr(gimli::DW_AT_name)?
            .and_then(|attr| attr.string_value(debug_str));
        let program = debug_line.program(offset, unit.address_size(), comp_dir, comp_name);
        let mut block_start_loc = locations.len();
        if let Ok(program) = program {
            let mut rows = program.rows();
            while let Some((header, row)) = rows.next_row()? {
                let pc = row.address();
                let line = row.line().unwrap_or(0);
                let column = match row.column() {
                    gimli::ColumnType::Column(column) => column,
                    gimli::ColumnType::LeftEdge => 0,
                };
                let file_index = row.file_index();
                let source_id = if !source_to_id_map.contains_key(&file_index) {
                    let mut file_path: String = if let Some(file) = row.file(header) {
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
                    if !file_path.starts_with('/') && comp_dir.is_some() {
                        file_path = format!("{}/{}", comp_dir.unwrap().to_string_lossy(), file_path);
                    }
                    sources
                        .iter()
                        .position(|p| *p == file_path)
                        .unwrap_or_else(|| {
                            let index = sources.len();
                            sources.push(file_path);
                            source_to_id_map.insert(file_index, index);
                            index
                        })
                } else {
                    *source_to_id_map.get(&file_index).ok_or(Error::DataFormat)? as usize
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
                    let fn_size =
                        locations[block_end_loc].address - locations[block_start_loc].address + 1;
                    let fn_size_field_len =
                        ((fn_size + 1).next_power_of_two().trailing_zeros() + 6) / 7;
                    // Remove function if it starts at its size field location.
                    if locations[block_start_loc].address <= u64::from(fn_size_field_len) {
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

    Ok(LocationInfo { sources, locations })
}
