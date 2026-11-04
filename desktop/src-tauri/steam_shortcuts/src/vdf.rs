//! Lossless binary KeyValues. Unknown scalar fields remain raw bytes.
use std::io::{Error, Result};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    Object(Vec<Entry>),
    String(Vec<u8>),
    Scalar(u8, Vec<u8>),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub key: Vec<u8>,
    pub value: Value,
}
impl Entry {
    pub fn new(key: &str, value: Value) -> Self {
        Self {
            key: key.as_bytes().to_vec(),
            value,
        }
    }
}
pub fn text(value: &str) -> Value {
    Value::String(value.as_bytes().to_vec())
}
pub fn int(value: u32) -> Value {
    Value::Scalar(2, value.to_le_bytes().to_vec())
}
pub fn get<'a>(entries: &'a [Entry], key: &str) -> Option<&'a Value> {
    entries
        .iter()
        .find(|e| e.key.eq_ignore_ascii_case(key.as_bytes()))
        .map(|e| &e.value)
}
pub fn set(entries: &mut Vec<Entry>, key: &str, value: Value) {
    if let Some(entry) = entries
        .iter_mut()
        .find(|e| e.key.eq_ignore_ascii_case(key.as_bytes()))
    {
        entry.value = value;
    } else {
        entries.push(Entry::new(key, value));
    }
}
pub fn string(entries: &[Entry], key: &str) -> Option<String> {
    match get(entries, key)? {
        Value::String(v) => String::from_utf8(v.clone()).ok(),
        _ => None,
    }
}
pub fn number(entries: &[Entry], key: &str) -> Option<u32> {
    match get(entries, key)? {
        Value::Scalar(2, v) => Some(u32::from_le_bytes(v.as_slice().try_into().ok()?)),
        _ => None,
    }
}
pub fn decode(bytes: &[u8]) -> Result<Vec<Entry>> {
    if bytes.len() > 32 * 1024 * 1024 {
        return Err(Error::other("Steam shortcut file is too large"));
    }
    let mut parser = Parser {
        bytes,
        cursor: 0,
        count: 0,
    };
    let entries = parser.object(0)?;
    if parser.cursor != bytes.len() {
        return Err(Error::other("Trailing bytes in Steam shortcut file"));
    }
    Ok(entries)
}
struct Parser<'a> {
    bytes: &'a [u8],
    cursor: usize,
    count: usize,
}
impl Parser<'_> {
    fn take(&mut self, length: usize) -> Result<Vec<u8>> {
        let end = self
            .cursor
            .checked_add(length)
            .ok_or_else(|| Error::other("Invalid VDF length"))?;
        let value = self
            .bytes
            .get(self.cursor..end)
            .ok_or_else(|| Error::other("Truncated Steam shortcut file"))?
            .to_vec();
        self.cursor = end;
        Ok(value)
    }
    fn cstring(&mut self, wide: bool) -> Result<Vec<u8>> {
        let start = self.cursor;
        loop {
            let bytes = self.take(if wide { 2 } else { 1 })?;
            if bytes.iter().all(|b| *b == 0) {
                return Ok(self.bytes[start..self.cursor - bytes.len()].to_vec());
            }
        }
    }
    fn object(&mut self, depth: usize) -> Result<Vec<Entry>> {
        if depth > 32 {
            return Err(Error::other("Steam shortcut nesting is too deep"));
        }
        let mut entries: Vec<Entry> = vec![];
        let mut keys = std::collections::HashSet::new();
        loop {
            let kind = self.take(1)?[0];
            if kind == 8 {
                return Ok(entries);
            }
            self.count += 1;
            if self.count > 200_000 {
                return Err(Error::other("Too many Steam shortcut fields"));
            }
            let key = self.cstring(false)?;
            if !keys.insert(key.to_ascii_lowercase()) {
                return Err(Error::other("Duplicate Steam shortcut field"));
            }
            let value = match kind {
                0 => Value::Object(self.object(depth + 1)?),
                1 => Value::String(self.cstring(false)?),
                2 | 3 | 4 | 6 => Value::Scalar(kind, self.take(4)?),
                5 => Value::Scalar(kind, self.cstring(true)?),
                7 | 10 => Value::Scalar(kind, self.take(8)?),
                _ => {
                    return Err(Error::other(
                        "Unsupported Steam VDF type; original file left untouched",
                    ));
                }
            };
            entries.push(Entry { key, value });
        }
    }
}
pub fn encode(entries: &[Entry]) -> Vec<u8> {
    fn object(entries: &[Entry], bytes: &mut Vec<u8>) {
        for entry in entries {
            let kind = match &entry.value {
                Value::Object(_) => 0,
                Value::String(_) => 1,
                Value::Scalar(k, _) => *k,
            };
            bytes.push(kind);
            bytes.extend(&entry.key);
            bytes.push(0);
            match &entry.value {
                Value::Object(children) => object(children, bytes),
                Value::String(value) => {
                    bytes.extend(value);
                    bytes.push(0);
                }
                Value::Scalar(kind, value) => {
                    bytes.extend(value);
                    if *kind == 5 {
                        bytes.extend([0, 0]);
                    }
                }
            }
        }
        bytes.push(8);
    }
    let mut bytes = vec![];
    object(entries, &mut bytes);
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn round_trips_unknown_fields_and_non_utf8() {
        let entries = vec![Entry::new(
            "shortcuts",
            Value::Object(vec![
                Entry::new("future", Value::String(vec![255, 128])),
                Entry::new("wide", Value::Scalar(5, vec![65, 0, 255, 9])),
                Entry::new("float", Value::Scalar(3, 1.5f32.to_le_bytes().to_vec())),
                Entry::new("uint64", Value::Scalar(7, u64::MAX.to_le_bytes().to_vec())),
            ]),
        )];
        let bytes = encode(&entries);
        assert_eq!(encode(&decode(&bytes).unwrap()), bytes);
    }
    #[test]
    fn rejects_malformed_or_ambiguous_files() {
        for bytes in [
            vec![],
            vec![0],
            vec![1, b'a', 0],
            vec![255, 0, 8],
            vec![8, 8],
            vec![1, b'a', 0, 0, 1, b'A', 0, 0, 8],
        ] {
            assert!(decode(&bytes).is_err(), "{bytes:?}");
        }
        assert!(decode(&[8]).unwrap().is_empty());
    }
}
