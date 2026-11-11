//! A small ZIP writer and a strict reader, for ClipDesigner's own packages:
//! Office files, kits (.cdkit) and packed projects (.cdpack).
//!
//! The writer streams to any `Write`: bytes are deflated, files are stored
//! as they are (media is already compressed) with a data descriptor, so a
//! large video never has to fit in memory. Archives stay under 4 GB (no
//! ZIP64).
//!
//! The reader treats every archive as hostile: names must be plain relative
//! paths, entry counts and sizes are capped before anything is unpacked, and
//! each entry's length and CRC are checked as it is read.
use crate::{Error, Result};
use std::io::{Read, Seek, SeekFrom, Write};

const LOCAL: u32 = 0x0403_4b50;
const CENTRAL: u32 = 0x0201_4b50;
const END: u32 = 0x0605_4b50;
const DESCRIPTOR: u32 = 0x0807_4b50;
const LIMIT: u64 = u32::MAX as u64 - 1;

fn too_large() -> Error {
    Error::Invalid("This package would be over 4 GB, which is more than one file can hold.".into())
}

struct Record {
    name: String,
    method: u16,
    flags: u16,
    crc: u32,
    packed: u32,
    size: u32,
    offset: u32,
}

/// Writes a ZIP archive to `out`, entry by entry.
pub struct Writer<W: Write> {
    out: W,
    at: u64,
    records: Vec<Record>,
}
impl<W: Write> Writer<W> {
    pub fn new(out: W) -> Self {
        Self {
            out,
            at: 0,
            records: vec![],
        }
    }
    fn put(&mut self, bytes: &[u8]) -> Result<()> {
        self.out.write_all(bytes)?;
        self.at += bytes.len() as u64;
        if self.at > LIMIT {
            return Err(too_large());
        }
        Ok(())
    }
    fn header(record: &Record, central: bool) -> Vec<u8> {
        let mut h = vec![];
        h.extend(if central { CENTRAL } else { LOCAL }.to_le_bytes());
        if central {
            h.extend(20u16.to_le_bytes()); // made by
        }
        h.extend(20u16.to_le_bytes()); // needed
        h.extend(record.flags.to_le_bytes());
        h.extend(record.method.to_le_bytes());
        h.extend(0u16.to_le_bytes()); // time
        h.extend(0x21u16.to_le_bytes()); // date 1980-01-01
        // A local header with a data descriptor leaves these zero.
        let described = !central && record.flags & 8 != 0;
        for v in [record.crc, record.packed, record.size] {
            h.extend(if described { 0 } else { v }.to_le_bytes());
        }
        h.extend((record.name.len() as u16).to_le_bytes());
        h.extend(0u16.to_le_bytes()); // extra
        if central {
            h.extend(0u16.to_le_bytes()); // comment
            h.extend(0u16.to_le_bytes()); // disk
            h.extend(0u16.to_le_bytes()); // internal attributes
            h.extend(0u32.to_le_bytes()); // external attributes
            h.extend(record.offset.to_le_bytes());
        }
        h.extend(record.name.as_bytes());
        h
    }
    fn check_name(&self, name: &str) -> Result<()> {
        check_name(name)?;
        if self.records.iter().any(|r| r.name == name) {
            return Err(Error::Invalid(format!("{name} is in the package twice.")));
        }
        if self.records.len() >= MAX_ENTRIES {
            return Err(Error::Invalid("The package has too many files.".into()));
        }
        Ok(())
    }
    /// Adds `data` as `name`, deflated.
    pub fn bytes(&mut self, name: &str, data: &[u8]) -> Result<()> {
        self.check_name(name)?;
        if data.len() as u64 > LIMIT {
            return Err(too_large());
        }
        let mut crc = flate2::Crc::new();
        crc.update(data);
        let mut enc =
            flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
        enc.write_all(data)?;
        let packed = enc.finish()?;
        let record = Record {
            name: name.to_owned(),
            method: 8,
            flags: 0,
            crc: crc.sum(),
            packed: packed.len() as u32,
            size: data.len() as u32,
            offset: self.at as u32,
        };
        self.put(&Self::header(&record, false))?;
        self.put(&packed)?;
        self.records.push(record);
        Ok(())
    }
    /// Adds everything `source` reads as `name`, stored, streaming it.
    /// `stop` is checked between chunks; `progress` gets the bytes copied.
    pub fn stream(
        &mut self,
        name: &str,
        mut source: impl Read,
        stop: &dyn Fn() -> bool,
        progress: &mut dyn FnMut(u64),
    ) -> Result<()> {
        self.check_name(name)?;
        let mut record = Record {
            name: name.to_owned(),
            method: 0,
            flags: 8,
            crc: 0,
            packed: 0,
            size: 0,
            offset: self.at as u32,
        };
        self.put(&Self::header(&record, false))?;
        let mut crc = flate2::Crc::new();
        let mut buffer = vec![0; 1 << 20];
        let mut total = 0u64;
        loop {
            if stop() {
                return Err(Error::Invalid("Cancelled.".into()));
            }
            let n = source.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            crc.update(&buffer[..n]);
            self.put(&buffer[..n])?;
            total += n as u64;
            progress(n as u64);
        }
        record.crc = crc.sum();
        record.packed = total as u32;
        record.size = total as u32;
        let mut d = vec![];
        d.extend(DESCRIPTOR.to_le_bytes());
        d.extend(record.crc.to_le_bytes());
        d.extend(record.packed.to_le_bytes());
        d.extend(record.size.to_le_bytes());
        self.put(&d)?;
        self.records.push(record);
        Ok(())
    }
    /// Writes the central directory and returns the output.
    pub fn finish(mut self) -> Result<W> {
        let start = self.at;
        let mut central = vec![];
        for r in &self.records {
            central.extend(Self::header(r, true));
        }
        self.put(&central)?;
        let mut end = vec![];
        end.extend(END.to_le_bytes());
        end.extend([0; 4]);
        let count = self.records.len() as u16;
        end.extend(count.to_le_bytes());
        end.extend(count.to_le_bytes());
        end.extend((central.len() as u32).to_le_bytes());
        end.extend((start as u32).to_le_bytes());
        end.extend(0u16.to_le_bytes());
        self.put(&end)?;
        self.out.flush()?;
        Ok(self.out)
    }
}

/// Deflated entries, ready to collect into a `Vec`.
pub fn to_vec(entries: &[(&str, &[u8])]) -> Result<Vec<u8>> {
    let mut w = Writer::new(Vec::new());
    for (name, data) in entries {
        w.bytes(name, data)?;
    }
    w.finish()
}

pub const MAX_ENTRIES: usize = 20_000;

/// A name inside a package: a plain relative path with `/` separators, no
/// drive, no `..`, no empty or dot parts, nothing Windows can't name.
pub fn check_name(name: &str) -> Result<()> {
    let bad = name.is_empty()
        || name.len() > 400
        || name.starts_with('/')
        || name.contains('\\')
        || name.contains(':')
        || name
            .chars()
            .any(|c| c.is_control() || "<>\"|?*".contains(c))
        || name.split('/').any(|part| {
            part.is_empty()
                || part == "."
                || part == ".."
                || part.ends_with('.')
                || part.ends_with(' ')
                || {
                    let stem = part.split('.').next().unwrap_or("").to_ascii_uppercase();
                    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
                        || ((stem.starts_with("COM") || stem.starts_with("LPT"))
                            && stem.len() == 4
                            && stem.as_bytes()[3].is_ascii_digit())
                }
        });
    if bad {
        return Err(Error::Invalid(format!(
            "The package has a file named {name:?}, which isn't allowed."
        )));
    }
    Ok(())
}

/// One entry of an archive being read.
#[derive(Clone, Debug)]
pub struct Entry {
    pub name: String,
    pub size: u64,
    method: u16,
    crc: u32,
    packed: u64,
    offset: u64,
}

/// Reads an archive's directory, refusing anything over `max_total` bytes
/// unpacked or with unsafe names.
pub struct Reader<R: Read + Seek> {
    input: R,
    pub entries: Vec<Entry>,
}
impl<R: Read + Seek> Reader<R> {
    pub fn new(mut input: R, max_total: u64) -> Result<Self> {
        let bad = || Error::Invalid("This isn't a package ClipDesigner can open.".into());
        let len = input.seek(SeekFrom::End(0))?;
        if len < 22 {
            return Err(bad());
        }
        let tail = len.min(22 + 65_535);
        input.seek(SeekFrom::Start(len - tail))?;
        let mut buf = vec![0; tail as usize];
        input.read_exact(&mut buf)?;
        let at = (0..=buf.len() - 22)
            .rev()
            .find(|&i| buf[i..i + 4] == END.to_le_bytes())
            .ok_or_else(bad)?;
        let u16_at = |b: &[u8], i: usize| u16::from_le_bytes([b[i], b[i + 1]]);
        let u32_at = |b: &[u8], i: usize| u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
        let count = u16_at(&buf, at + 10) as usize;
        let size = u32_at(&buf, at + 12) as u64;
        let start = u32_at(&buf, at + 16) as u64;
        if count > MAX_ENTRIES || start + size > len || size > 64 << 20 {
            return Err(bad());
        }
        input.seek(SeekFrom::Start(start))?;
        let mut dir = vec![0; size as usize];
        input.read_exact(&mut dir)?;
        let mut entries = vec![];
        let mut i = 0;
        let mut total = 0u64;
        for _ in 0..count {
            if i + 46 > dir.len() || u32_at(&dir, i) != CENTRAL {
                return Err(bad());
            }
            let flags = u16_at(&dir, i + 8);
            let method = u16_at(&dir, i + 10);
            let crc = u32_at(&dir, i + 16);
            let packed = u32_at(&dir, i + 20) as u64;
            let size = u32_at(&dir, i + 24) as u64;
            let name_len = u16_at(&dir, i + 28) as usize;
            let extra = u16_at(&dir, i + 30) as usize;
            let comment = u16_at(&dir, i + 32) as usize;
            let offset = u32_at(&dir, i + 42) as u64;
            let end = i + 46 + name_len;
            if end > dir.len() || flags & 1 != 0 || !matches!(method, 0 | 8) {
                return Err(bad());
            }
            let name = String::from_utf8(dir[i + 46..end].to_vec()).map_err(|_| bad())?;
            i = end + extra + comment;
            // Folders are implied by the files in them.
            if name.ends_with('/') && size == 0 {
                continue;
            }
            check_name(&name)?;
            if entries
                .iter()
                .any(|e: &Entry| e.name.eq_ignore_ascii_case(&name))
            {
                return Err(Error::Invalid(format!("{name} is in the package twice.")));
            }
            if offset >= start || (method == 0 && packed != size) {
                return Err(bad());
            }
            total += size;
            if total > max_total {
                return Err(Error::Invalid(
                    "This package unpacks to more than ClipDesigner allows.".into(),
                ));
            }
            entries.push(Entry {
                name,
                size,
                method,
                crc,
                packed,
                offset,
            });
        }
        Ok(Self { input, entries })
    }
    pub fn find(&self, name: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.name == name)
    }
    /// Copies one entry into `out`, checking its size and CRC.
    pub fn copy(
        &mut self,
        entry: &Entry,
        out: &mut dyn Write,
        stop: &dyn Fn() -> bool,
    ) -> Result<()> {
        let bad = || Error::Invalid(format!("{} in the package is damaged.", entry.name));
        self.input.seek(SeekFrom::Start(entry.offset))?;
        let mut h = [0; 30];
        self.input.read_exact(&mut h)?;
        if h[..4] != LOCAL.to_le_bytes() {
            return Err(bad());
        }
        let skip =
            u16::from_le_bytes([h[26], h[27]]) as i64 + u16::from_le_bytes([h[28], h[29]]) as i64;
        self.input.seek(SeekFrom::Current(skip))?;
        let raw = (&mut self.input).take(entry.packed);
        let mut reader: Box<dyn Read + '_> = if entry.method == 8 {
            Box::new(flate2::read::DeflateDecoder::new(raw))
        } else {
            Box::new(raw)
        };
        let mut crc = flate2::Crc::new();
        let mut buffer = vec![0; 1 << 20];
        let mut total = 0u64;
        loop {
            if stop() {
                return Err(Error::Invalid("Cancelled.".into()));
            }
            let n = reader.read(&mut buffer).map_err(|_| bad())?;
            if n == 0 {
                break;
            }
            total += n as u64;
            if total > entry.size {
                return Err(bad());
            }
            crc.update(&buffer[..n]);
            out.write_all(&buffer[..n])?;
        }
        if total != entry.size || crc.sum() != entry.crc {
            return Err(bad());
        }
        Ok(())
    }
    /// One entry in memory; for small parts such as a manifest.
    pub fn read(&mut self, name: &str, max: u64) -> Result<Vec<u8>> {
        let entry = self
            .find(name)
            .cloned()
            .ok_or_else(|| Error::Invalid(format!("The package has no {name}.")))?;
        if entry.size > max {
            return Err(Error::Invalid(format!(
                "{name} in the package is too large."
            )));
        }
        let mut out = Vec::with_capacity(entry.size as usize);
        self.copy(&entry, &mut out, &|| false)?;
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn written_entries_read_back() {
        let mut w = Writer::new(Cursor::new(vec![]));
        w.bytes("kit.json", b"{\"a\":1}").unwrap();
        let big: Vec<u8> = (0..3_000_000u32).map(|i| (i * 7) as u8).collect();
        let mut copied = 0;
        w.stream("media/clip.mp4", &big[..], &|| false, &mut |n| copied += n)
            .unwrap();
        assert_eq!(copied, big.len() as u64);
        let bytes = w.finish().unwrap().into_inner();
        let mut r = Reader::new(Cursor::new(bytes), 10 << 20).unwrap();
        assert_eq!(r.entries.len(), 2);
        assert_eq!(r.read("kit.json", 100).unwrap(), b"{\"a\":1}");
        let entry = r.find("media/clip.mp4").unwrap().clone();
        let mut out = vec![];
        r.copy(&entry, &mut out, &|| false).unwrap();
        assert_eq!(out, big);
    }

    #[test]
    fn unsafe_names_and_sizes_are_refused() {
        for name in [
            "../evil", "/abs", "C:/x", "a\\b", "a//b", "con.txt", "a/LPT1", "dot./x", "",
        ] {
            assert!(check_name(name).is_err(), "{name}");
        }
        for name in ["kit.json", "media/My clip (2).mp4", "Ünïcode/файл.png"] {
            check_name(name).unwrap();
        }
        let bytes = to_vec(&[("a.bin", &[0u8; 5000][..])]).unwrap();
        assert!(Reader::new(Cursor::new(bytes.clone()), 4000).is_err());
        // A flipped byte in the data fails the CRC check.
        let mut damaged = bytes;
        let at = 30 + "a.bin".len() + 1;
        damaged[at] ^= 0xff;
        let mut r = Reader::new(Cursor::new(damaged), 10_000).unwrap();
        assert!(r.read("a.bin", 10_000).is_err());
        assert!(Reader::new(Cursor::new(b"not a zip at all, sorry".to_vec()), 100).is_err());
    }
}
