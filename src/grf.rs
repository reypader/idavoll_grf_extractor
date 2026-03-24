use anyhow::{bail, Context, Result};
use encoding_rs::EUC_KR;
use std::io::{Read, Seek, SeekFrom};

use crate::decrypt;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const SIGNATURE: &[u8] = b"Master of Magic";
const HEADER_SIZE: u64 = 46; // 15 sig + 15 key + 4 table_offset + 4 skip + 4 filecount + 4 version
const VERSION_200: u32 = 0x0200;

const TYPE_FILE: u8 = 0x01;
const TYPE_ENCRYPT_MIXED: u8 = 0x02;
const TYPE_ENCRYPT_HEADER: u8 = 0x04;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

pub struct GrfEntry {
    /// Internal GRF path decoded from CP949 (uses backslash as separator).
    pub internal_path: String,
    pub pack_size: u32,
    pub length_aligned: u32,
    pub real_size: u32,
    pub entry_type: u8,
    /// Byte offset of the compressed data from the start of the GRF file
    /// (i.e. already includes the 46-byte header).
    pub data_offset: u64,
}

impl GrfEntry {
    pub fn is_encrypted_mixed(&self) -> bool {
        self.entry_type & TYPE_ENCRYPT_MIXED != 0
    }

    pub fn is_encrypted_header(&self) -> bool {
        self.entry_type & TYPE_ENCRYPT_HEADER != 0
    }
}

// ---------------------------------------------------------------------------
// GRF reader
// ---------------------------------------------------------------------------

pub struct Grf<R: Read + Seek> {
    reader: R,
    pub entries: Vec<GrfEntry>,
}

impl<R: Read + Seek> Grf<R> {
    pub fn open(mut reader: R) -> Result<Self> {
        // --- Read header (46 bytes) ---
        let mut sig = [0u8; 15];
        reader.read_exact(&mut sig).context("reading GRF signature")?;
        if sig != SIGNATURE[..15] {
            bail!("not a GRF file: bad signature");
        }

        let mut _key = [0u8; 15];
        reader.read_exact(&mut _key)?;

        let table_offset = read_u32_le(&mut reader)?;
        let skip = read_u32_le(&mut reader)?;
        let raw_count = read_u32_le(&mut reader)?;
        let version = read_u32_le(&mut reader)?;

        if version != VERSION_200 {
            bail!("unsupported GRF version {version:#06x}; only 0x0200 is supported");
        }

        let file_count = raw_count.saturating_sub(skip).saturating_sub(7) as usize;

        // --- Read file table ---
        let table_abs = HEADER_SIZE + table_offset as u64;
        reader.seek(SeekFrom::Start(table_abs)).context("seeking to file table")?;

        let pack_size = read_u32_le(&mut reader)? as usize;
        let real_size = read_u32_le(&mut reader)? as usize;

        let mut compressed = vec![0u8; pack_size];
        reader.read_exact(&mut compressed).context("reading compressed file table")?;

        let table_bytes = inflate(&compressed, real_size).context("decompressing file table")?;

        // --- Parse entries ---
        let entries = parse_entries(&table_bytes, file_count)?;

        Ok(Self { reader, entries })
    }

    /// Read and decompress (and decrypt if needed) a single entry's data.
    pub fn read_entry(&mut self, entry: &GrfEntry) -> Result<Vec<u8>> {
        self.reader
            .seek(SeekFrom::Start(entry.data_offset))
            .with_context(|| format!("seeking to entry {}", entry.internal_path))?;

        let mut buf = vec![0u8; entry.length_aligned as usize];
        self.reader
            .read_exact(&mut buf)
            .with_context(|| format!("reading entry {}", entry.internal_path))?;

        // Decrypt if needed.
        if entry.is_encrypted_mixed() {
            decrypt::decode_full(&mut buf, entry.length_aligned as usize, entry.pack_size as usize);
        } else if entry.is_encrypted_header() {
            decrypt::decode_header(&mut buf, entry.length_aligned as usize);
        }

        // Decompress.
        let data = inflate(&buf[..entry.pack_size as usize], entry.real_size as usize)
            .with_context(|| format!("decompressing entry {}", entry.internal_path))?;

        Ok(data)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn read_u32_le<R: Read>(r: &mut R) -> Result<u32> {
    let mut buf = [0u8; 4];
    r.read_exact(&mut buf)?;
    Ok(u32::from_le_bytes(buf))
}

fn inflate(compressed: &[u8], expected_size: usize) -> Result<Vec<u8>> {
    use flate2::read::ZlibDecoder;
    let mut decoder = ZlibDecoder::new(compressed);
    let mut out = Vec::with_capacity(expected_size);
    decoder.read_to_end(&mut out)?;
    Ok(out)
}

/// Parse the decompressed file table into entry descriptors.
fn parse_entries(table: &[u8], expected_count: usize) -> Result<Vec<GrfEntry>> {
    let mut entries = Vec::with_capacity(expected_count);
    let mut pos = 0usize;

    while pos < table.len() && entries.len() < expected_count {
        // Null-terminated CP949 filename.
        let start = pos;
        while pos < table.len() && table[pos] != 0 {
            pos += 1;
        }
        if pos >= table.len() {
            break;
        }
        let filename_bytes = &table[start..pos];
        pos += 1; // skip null terminator

        if pos + 17 > table.len() {
            break;
        }

        let pack_size = u32::from_le_bytes(table[pos..pos + 4].try_into().unwrap());
        let length_aligned = u32::from_le_bytes(table[pos + 4..pos + 8].try_into().unwrap());
        let real_size = u32::from_le_bytes(table[pos + 8..pos + 12].try_into().unwrap());
        let entry_type = table[pos + 12];
        let raw_offset = u32::from_le_bytes(table[pos + 13..pos + 17].try_into().unwrap());
        pos += 17;

        // Skip directory entries.
        if entry_type & TYPE_FILE == 0 {
            continue;
        }

        let internal_path = decode_cp949(filename_bytes);
        let data_offset = HEADER_SIZE + raw_offset as u64;

        entries.push(GrfEntry {
            internal_path,
            pack_size,
            length_aligned,
            real_size,
            entry_type,
            data_offset,
        });
    }

    Ok(entries)
}

/// Decode CP949 bytes to a Rust String, replacing unmappable bytes with U+FFFD.
fn decode_cp949(bytes: &[u8]) -> String {
    let (cow, _, _) = EUC_KR.decode(bytes);
    cow.into_owned()
}
