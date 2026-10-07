//! PLY files: the header, and the properties asked for of each element.
//!
//! PLY holds meshes, point clouds and Gaussian splats alike: elements
//! (`vertex`, `face`, ...) with scalar or list properties, written as text
//! or as binary (either byte order). Only what is asked for is kept, so a
//! splat file's 60-odd properties per splat don't all fill memory.

use anyhow::{bail, Context, Result};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Ascii,
    LittleEndian,
    BigEndian,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    I8,
    U8,
    I16,
    U16,
    I32,
    U32,
    F32,
    F64,
}

impl Kind {
    fn parse(name: &str) -> Result<Kind> {
        Ok(match name {
            "char" | "int8" => Kind::I8,
            "uchar" | "uint8" => Kind::U8,
            "short" | "int16" => Kind::I16,
            "ushort" | "uint16" => Kind::U16,
            "int" | "int32" => Kind::I32,
            "uint" | "uint32" => Kind::U32,
            "float" | "float32" => Kind::F32,
            "double" | "float64" => Kind::F64,
            _ => bail!("unknown PLY type '{name}'"),
        })
    }

    fn size(self) -> usize {
        match self {
            Kind::I8 | Kind::U8 => 1,
            Kind::I16 | Kind::U16 => 2,
            Kind::I32 | Kind::U32 | Kind::F32 => 4,
            Kind::F64 => 8,
        }
    }

    /// The value at the start of `b`.
    fn read(self, b: &[u8], format: Format) -> f64 {
        macro_rules! num {
            ($t:ty, $n:expr) => {{
                let a: [u8; $n] = b[..$n].try_into().unwrap();
                if format == Format::BigEndian {
                    <$t>::from_be_bytes(a) as f64
                } else {
                    <$t>::from_le_bytes(a) as f64
                }
            }};
        }
        match self {
            Kind::I8 => b[0] as i8 as f64,
            Kind::U8 => b[0] as f64,
            Kind::I16 => num!(i16, 2),
            Kind::U16 => num!(u16, 2),
            Kind::I32 => num!(i32, 4),
            Kind::U32 => num!(u32, 4),
            Kind::F32 => num!(f32, 4),
            Kind::F64 => num!(f64, 8),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Property {
    pub name: String,
    pub kind: Kind,
    /// The type of the count, for a list property.
    pub list: Option<Kind>,
}

#[derive(Clone, Debug)]
pub struct Element {
    pub name: String,
    pub count: usize,
    pub properties: Vec<Property>,
}

impl Element {
    pub fn has(&self, name: &str) -> bool {
        self.properties.iter().any(|p| p.name == name)
    }
}

/// A list property's values: every record's values one after another,
/// and where each record's start.
#[derive(Clone, Debug, Default)]
pub struct Lists {
    pub values: Vec<f64>,
    /// `starts[i]..starts[i + 1]` are record `i`'s values (one more than
    /// there are records).
    pub starts: Vec<usize>,
}

impl Lists {
    pub fn get(&self, i: usize) -> &[f64] {
        &self.values[self.starts[i]..self.starts[i + 1]]
    }
}

/// The properties read of one element.
#[derive(Clone, Debug, Default)]
pub struct Values {
    pub scalars: HashMap<String, Vec<f32>>,
    pub lists: HashMap<String, Lists>,
}

pub struct Ply<'a> {
    pub format: Format,
    pub elements: Vec<Element>,
    pub comments: Vec<String>,
    body: &'a [u8],
}

impl<'a> Ply<'a> {
    /// Parse the header of a PLY file held in `bytes`.
    pub fn parse(bytes: &'a [u8]) -> Result<Ply<'a>> {
        if !bytes.starts_with(b"ply") {
            bail!("not a PLY file");
        }
        let end = find(bytes, b"end_header").context("the PLY header has no end_header")?;
        let mut body = end + b"end_header".len();
        // The line ends with \n (or \r\n).
        while body < bytes.len() && bytes[body] != b'\n' {
            body += 1;
        }
        let header = std::str::from_utf8(&bytes[..end]).context("the PLY header isn't text")?;
        let mut format = None;
        let mut elements: Vec<Element> = Vec::new();
        let mut comments = Vec::new();
        for line in header.lines().skip(1) {
            let words: Vec<&str> = line.split_whitespace().collect();
            match words.as_slice() {
                ["format", f, ..] => {
                    format = Some(match *f {
                        "ascii" => Format::Ascii,
                        "binary_little_endian" => Format::LittleEndian,
                        "binary_big_endian" => Format::BigEndian,
                        _ => bail!("unknown PLY format '{f}'"),
                    })
                }
                ["comment", ..] | ["obj_info", ..] => {
                    comments.push(line.trim_start()[words[0].len()..].trim().to_string())
                }
                ["element", name, count] => elements.push(Element {
                    name: name.to_string(),
                    count: count.parse().context("bad PLY element count")?,
                    properties: Vec::new(),
                }),
                ["property", "list", count, kind, name] => {
                    let e = elements
                        .last_mut()
                        .context("PLY property before any element")?;
                    e.properties.push(Property {
                        name: name.to_string(),
                        kind: Kind::parse(kind)?,
                        list: Some(Kind::parse(count)?),
                    });
                }
                ["property", kind, name] => {
                    let e = elements
                        .last_mut()
                        .context("PLY property before any element")?;
                    e.properties.push(Property {
                        name: name.to_string(),
                        kind: Kind::parse(kind)?,
                        list: None,
                    });
                }
                [] => {}
                _ => bail!("bad PLY header line '{line}'"),
            }
        }
        Ok(Ply {
            format: format.context("the PLY header has no format")?,
            elements,
            comments,
            body: &bytes[(body + 1).min(bytes.len())..],
        })
    }

    pub fn element(&self, name: &str) -> Option<&Element> {
        self.elements.iter().find(|e| e.name == name)
    }

    /// Read the properties `wanted` asks for of each element it names
    /// (element name, property names); other data is skipped.
    pub fn read(&self, wanted: &[(&str, &[&str])]) -> Result<HashMap<String, Values>> {
        let mut out = HashMap::new();
        let mut binary = self.body;
        let mut text = std::str::from_utf8(if self.format == Format::Ascii {
            self.body
        } else {
            &[]
        })
        .context("the PLY body isn't text")?
        .split_ascii_whitespace();
        for e in &self.elements {
            let names: &[&str] = wanted
                .iter()
                .find(|(n, _)| *n == e.name)
                .map(|(_, p)| *p)
                .unwrap_or(&[]);
            let keep: Vec<bool> = e
                .properties
                .iter()
                .map(|p| names.contains(&p.name.as_str()))
                .collect();
            let mut v = Values::default();
            for (p, &k) in e.properties.iter().zip(&keep) {
                if !k {
                    continue;
                }
                if p.list.is_some() {
                    v.lists.insert(
                        p.name.clone(),
                        Lists {
                            values: Vec::new(),
                            starts: vec![0],
                        },
                    );
                } else {
                    v.scalars
                        .insert(p.name.clone(), Vec::with_capacity(e.count.min(1 << 26)));
                }
            }
            if self.format == Format::Ascii {
                let mut next = || -> Result<f64> {
                    text.next()
                        .with_context(|| format!("the PLY file ends inside '{}'", e.name))?
                        .parse::<f64>()
                        .with_context(|| format!("a value of '{}' isn't a number", e.name))
                };
                for _ in 0..e.count {
                    for (p, &k) in e.properties.iter().zip(&keep) {
                        match p.list {
                            None => {
                                let x = next()?;
                                if k {
                                    v.scalars.get_mut(&p.name).unwrap().push(x as f32);
                                }
                            }
                            Some(_) => {
                                let n = next()? as usize;
                                let l = v.lists.get_mut(&p.name);
                                match l {
                                    Some(l) => {
                                        for _ in 0..n {
                                            l.values.push(next()?);
                                        }
                                        l.starts.push(l.values.len());
                                    }
                                    None => {
                                        for _ in 0..n {
                                            next()?;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            } else {
                binary = self.read_binary(binary, e, &keep, &mut v)?;
            }
            if !names.is_empty() {
                out.insert(e.name.clone(), v);
            }
        }
        Ok(out)
    }

    fn read_binary<'b>(
        &self,
        mut data: &'b [u8],
        e: &Element,
        keep: &[bool],
        v: &mut Values,
    ) -> Result<&'b [u8]> {
        let short = || format!("the PLY file ends inside '{}'", e.name);
        // Records of scalars only all have the same size: read by offset.
        if e.properties.iter().all(|p| p.list.is_none()) {
            let stride: usize = e.properties.iter().map(|p| p.kind.size()).sum();
            let total = stride.checked_mul(e.count).with_context(short)?;
            if data.len() < total {
                bail!(short());
            }
            let mut at = 0;
            for (p, &k) in e.properties.iter().zip(keep) {
                if k {
                    let col = v.scalars.get_mut(&p.name).unwrap();
                    col.extend(
                        (0..e.count)
                            .map(|i| p.kind.read(&data[i * stride + at..], self.format) as f32),
                    );
                }
                at += p.kind.size();
            }
            return Ok(&data[total..]);
        }
        for _ in 0..e.count {
            for (p, &k) in e.properties.iter().zip(keep) {
                match p.list {
                    None => {
                        let n = p.kind.size();
                        if data.len() < n {
                            bail!(short());
                        }
                        if k {
                            let x = p.kind.read(data, self.format) as f32;
                            v.scalars.get_mut(&p.name).unwrap().push(x);
                        }
                        data = &data[n..];
                    }
                    Some(ck) => {
                        if data.len() < ck.size() {
                            bail!(short());
                        }
                        let count = ck.read(data, self.format) as usize;
                        data = &data[ck.size()..];
                        let n = count * p.kind.size();
                        if data.len() < n {
                            bail!(short());
                        }
                        if k {
                            let l = v.lists.get_mut(&p.name).unwrap();
                            l.values.extend(
                                (0..count)
                                    .map(|i| p.kind.read(&data[i * p.kind.size()..], self.format)),
                            );
                            l.starts.push(l.values.len());
                        }
                        data = &data[n..];
                    }
                }
            }
        }
        Ok(data)
    }
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_ascii_and_both_byte_orders() {
        let text = b"ply\nformat ascii 1.0\ncomment hello\nelement vertex 2\nproperty float x\n\
            property uchar red\nelement face 1\nproperty list uchar int vertex_indices\n\
            end_header\n1.5 200\n-2 3\n3 0 1 1\n";
        let ply = Ply::parse(text).unwrap();
        assert_eq!(ply.comments, ["hello"]);
        let got = ply
            .read(&[("vertex", &["x", "red"]), ("face", &["vertex_indices"])])
            .unwrap();
        assert_eq!(got["vertex"].scalars["x"], [1.5, -2.0]);
        assert_eq!(got["vertex"].scalars["red"], [200.0, 3.0]);
        assert_eq!(got["face"].lists["vertex_indices"].get(0), [0.0, 1.0, 1.0]);

        for (name, be) in [("binary_little_endian", false), ("binary_big_endian", true)] {
            let mut b = format!(
                "ply\r\nformat {name} 1.0\r\nelement vertex 2\r\nproperty double x\r\n\
                 property short y\r\nelement face 2\r\nproperty list uchar uint vertex_indices\r\n\
                 property float q\r\nend_header\r\n"
            )
            .into_bytes();
            let put = |b: &mut Vec<u8>, le: &[u8], bev: &[u8]| {
                b.extend_from_slice(if be { bev } else { le })
            };
            for (x, y) in [(0.25f64, -7i16), (4.0, 9)] {
                put(&mut b, &x.to_le_bytes(), &x.to_be_bytes());
                put(&mut b, &y.to_le_bytes(), &y.to_be_bytes());
            }
            for (idx, q) in [(&[0u32, 1, 0][..], 0.5f32), (&[1, 0, 1, 0][..], 2.0)] {
                b.push(idx.len() as u8);
                for i in idx {
                    put(&mut b, &i.to_le_bytes(), &i.to_be_bytes());
                }
                put(&mut b, &q.to_le_bytes(), &q.to_be_bytes());
            }
            let ply = Ply::parse(&b).unwrap();
            let got = ply
                .read(&[("vertex", &["x", "y"]), ("face", &["vertex_indices"])])
                .unwrap();
            assert_eq!(got["vertex"].scalars["x"], [0.25, 4.0], "{name}");
            assert_eq!(got["vertex"].scalars["y"], [-7.0, 9.0], "{name}");
            assert_eq!(
                got["face"].lists["vertex_indices"].get(1),
                [1.0, 0.0, 1.0, 0.0]
            );
        }
    }

    #[test]
    fn short_files_are_errors() {
        let b = b"ply\nformat binary_little_endian 1.0\nelement vertex 3\nproperty float x\nend_header\n\0\0\0\0";
        assert!(Ply::parse(b).unwrap().read(&[("vertex", &["x"])]).is_err());
        assert!(Ply::parse(b"nope").is_err());
        assert!(Ply::parse(b"ply\nformat ascii 1.0\nelement vertex 1\n").is_err());
    }
}
