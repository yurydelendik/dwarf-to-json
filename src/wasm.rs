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

use std::result;
use std::str;

pub struct WasmFormatError;

pub type Result<T> = result::Result<T, WasmFormatError>;

fn read_u32_leb128(slice: &[u8]) -> Result<(u32, usize)> {
    let mut result: u32 = 0;
    let mut shift = 0;
    let mut position = 0;

    for _ in 0..5 {
        let byte = unsafe { *slice.get_unchecked(position) };
        position += 1;
        result |= ((byte & 0x7F) as u32) << shift;
        if (byte & 0x80) == 0 {
            break;
        }
        shift += 7;
    }

    // Do a single bounds check at the end instead of for every byte.
    if position > slice.len() {
        return Err(WasmFormatError);
    }
    Ok((result, position))
}

pub struct WasmDecoder<'a> {
    data: &'a [u8],
}

impl<'a> WasmDecoder<'a> {
    pub fn new(data: &'a [u8]) -> WasmDecoder<'a> {
        WasmDecoder { data }
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn eof(&self) -> bool {
        self.data.len() == 0
    }

    pub fn u32(&mut self) -> Result<u32> {
        let (n, l1) = read_u32_leb128(self.data)?;
        self.data = &self.data[l1..];
        Ok(n)
    }

    pub fn skip(&mut self, amt: usize) -> Result<&'a [u8]> {
        if amt > self.data.len() {
            return Err(WasmFormatError);
        }
        let (data, rest) = self.data.split_at(amt);
        self.data = rest;
        Ok(data)
    }

    pub fn str(&mut self) -> Result<&'a str> {
        let len = self.u32()?;
        str::from_utf8(self.skip(len as usize)?).map_err(|_| WasmFormatError)
    }
}
