use std::str;
use std::collections::HashMap;

use gimli;

use gimli::{
    DebugAbbrev, DebugInfo, DebugLine, DebugStr, 
    DebugRanges, DebugRngLists, DebugLoc, DebugLocLists, 
    RangeLists, LocationLists, LittleEndian, AttributeValue
};

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
    Unknown
}
pub struct DebugInfoObj<'a> {
    pub tag: &'static str,
    pub attrs: Vec<(&'static str, DebugAttrValue<'a>)>,
    pub children: Vec<DebugInfoObj<'a>>,
}

fn remove_dead_functions(items: &mut Vec<DebugInfoObj>) {
    // TODO
}

fn enum_to_str(s: Option<&'static str>) -> DebugAttrValue {
    let s1 = s.unwrap();
    let (_dw, s2) = s1.split_at(s1.find('_').unwrap() + 1);
    let (_dw, s3) = s2.split_at(s2.find('_').unwrap() + 1);
    DebugAttrValue::String(s3)
}

pub fn get_debug_scopes<'b>(debug_sections: &'b HashMap<&str, &[u8]>, sources: &Vec<String>) -> Vec<DebugInfoObj<'b>> {
    // see https://gist.github.com/yurydelendik/802f36983d50cedb05f984d784dc5159
    let ref debug_str = DebugStr::new(&debug_sections[".debug_str"], LittleEndian);
    let ref debug_abbrev = DebugAbbrev::new(&debug_sections[".debug_abbrev"], LittleEndian);
    let ref debug_info = DebugInfo::new(&debug_sections[".debug_info"], LittleEndian);    

    let debug_ranges = DebugRanges::new(&debug_sections[".debug_ranges"], LittleEndian);
    let debug_rnglists = DebugRngLists::new(&[], LittleEndian);
    let rnglists = RangeLists::new(debug_ranges, debug_rnglists).expect("Should parse rnglists");

    // let debug_loc = DebugLoc::new(&debug_sections[".debug_loc"], LittleEndian);
    // let debug_loclists = DebugLocLists::new(&[], LittleEndian);
    // let loclists = LocationLists::new(debug_loc, debug_loclists).expect("Should parse loclists");

    let mut iter = debug_info.units();
    let mut info = Vec::new();
    while let Some(unit) = iter.next().unwrap_or(None) {
        let abbrevs = unit.abbreviations(debug_abbrev).unwrap();

        let mut stack: Vec<DebugInfoObj> = Vec::new();
        stack.push(DebugInfoObj {
            tag: &"",
            attrs: Vec::new(),
            children: Vec::new(),
        });
        // Iterate over all of this compilation unit's entries.
        let mut entries = unit.entries(&abbrevs);
        while let Some((depth_delta, entry)) = entries.next_dfs().expect("???") {
           let tag_value = &entry.tag().static_string().unwrap()[ /*DW_TAG_*/ 7..];
           let mut attrs_values = Vec::new();
            let mut attrs = entry.attrs();
            while let Some(attr) = attrs.next().unwrap() {
                let attr_name = &attr.name().static_string().unwrap()[ /*DW_AT_*/ 6 ..];
                let attr_value = match attr.value() {
                    AttributeValue::Addr(u) => DebugAttrValue::I64(u as i64),
                    AttributeValue::Udata(u) => DebugAttrValue::I64(u as i64),
                    AttributeValue::Data1(u) => DebugAttrValue::I64(u[0] as i64),
                    AttributeValue::Sdata(i) => DebugAttrValue::I64(i),
                    AttributeValue::DebugLineRef(o) => DebugAttrValue::I64(o.0 as i64),
                    AttributeValue::Flag(f) => DebugAttrValue::Bool(f),
                    AttributeValue::FileIndex(i) => DebugAttrValue::String("sdfasdf"),
                    AttributeValue::DebugStrRef(r) => DebugAttrValue::String(
                        debug_str.get_str(r).expect("string").to_string().expect("???")
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
                    },
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
                    AttributeValue::Exprloc(expr) => {
                        DebugAttrValue::Expression
                    },
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
                    AttributeValue::UnitRef(_) | AttributeValue::DebugInfoRef(_) => {
                        // Types and stuff
                        DebugAttrValue::Ignored
                    },
                    _ => { 
                        println!("{:?}", attr.value());
                        DebugAttrValue::Unknown
                    },
                };
                attrs_values.push((attr_name, attr_value));
            }
            if depth_delta <= 0 && stack.len() > 1 {
                for i in 0..1 - depth_delta {
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
    let mut locations = Vec::new();
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
                    let index = sources.len();
                    sources.push(file_path);
                    source_to_id_map.insert(file_index, index);
                    index
                } else {
                    *source_to_id_map.get(&file_index).unwrap() as usize
                };
                let loc = LocationRecord {
                    address: pc,
                    source_id: source_id as u32,
                    line: line as u32,
                    column: column as u32,
                };
                locations.push(loc);
                if row.end_sequence() {
                    // Heuristic to remove dead functions.
                    let block_end_loc = locations.len() - 1;
                    let fn_size = locations[block_end_loc].address -
                        locations[block_start_loc].address + 1;
                    let fn_size_field_len = ((fn_size + 1).next_power_of_two().trailing_zeros() +
                                                 6) / 7;
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
