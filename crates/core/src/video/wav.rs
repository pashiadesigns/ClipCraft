//! Stream retained recording audio to disk without growing memory with duration.
use std::{
    io::{Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};
pub struct Writer {
    file: tempfile::NamedTempFile,
    bytes: u32,
}
impl Writer {
    pub fn new(folder: &Path) -> std::io::Result<Self> {
        let mut file = tempfile::Builder::new()
            .prefix(".clipcraft-audio-")
            .suffix(".wav")
            .tempfile_in(folder)?;
        file.write_all(&[0; 44])?;
        Ok(Self { file, bytes: 0 })
    }
    pub fn append(&mut self, values: &[i16]) -> std::io::Result<()> {
        let bytes: Vec<u8> = values.iter().flat_map(|v| v.to_le_bytes()).collect();
        self.bytes = self
            .bytes
            .checked_add(bytes.len() as u32)
            .ok_or_else(|| std::io::Error::other("Audio track is too large"))?;
        self.file.write_all(&bytes)
    }
    pub fn finish(mut self) -> std::io::Result<PathBuf> {
        let mut header = Vec::with_capacity(44);
        header.extend(b"RIFF");
        header.extend((self.bytes + 36).to_le_bytes());
        header.extend(b"WAVEfmt ");
        header.extend(16u32.to_le_bytes());
        header.extend(1u16.to_le_bytes());
        header.extend(2u16.to_le_bytes());
        header.extend(48000u32.to_le_bytes());
        header.extend(192000u32.to_le_bytes());
        header.extend(4u16.to_le_bytes());
        header.extend(16u16.to_le_bytes());
        header.extend(b"data");
        header.extend(self.bytes.to_le_bytes());
        self.file.seek(SeekFrom::Start(0))?;
        self.file.write_all(&header)?;
        self.file.as_file().sync_all()?;
        self.file.keep().map(|(_, path)| path).map_err(|e| e.error)
    }
}
