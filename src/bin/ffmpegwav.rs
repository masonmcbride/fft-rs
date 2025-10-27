use std::fs::File;
use std::io::{self, Read, Seek};
use std::collections::HashMap;

pub const HEADER_SPEC: &[(&str, &str)] = &[
    ("riff_id",   "[u8;4]"), // should be "RIFF"
    ("riff_size", "u32"),    // file size - numbytes([u8;4]) - numbytes(u32)
    ("wave_id",   "[u8;4]"), // should be "WAVE"
];

pub const FMT_CHUNK_SPEC: &[(&str, &str)] = &[
    ("chunk_id",         "[u8;4]"),
    ("chunk_size",       "u32"),
    ("audio_format",     "u16"),
    ("num_channels",     "u16"),
    ("sample_rate",      "u32"),
    ("byte_rate",        "u32"),
    ("block_align",      "u16"),
    ("bits_per_sample",  "u16"),

    // Remaining bytes in this chunk body after the first 16 bytes:
    ("extra_bytes",      "Vec<u8>"), // variable tail, length = chunk_size - 16
];

pub const DATA_CHUNK_SPEC: &[(&str, &str)] = &[
    ("chunk_id",   "[u8;4]"), 
    ("chunk_size", "u32"),

    // read PCM samples from body until we've consumed chunk_size bytes.
    // for PCM16, that's chunk_size / 2 samples of i16 LE (possibly interleaved channels)
    ("samples",    "Vec<i16>"),
];

pub const LIST_ENTRY_SPEC: &[(&str, &str)] = &[
    ("tag_id",    "[u8;4]"),
    ("text_size", "u32"),
    ("text",      "Vec<String>"), // read text_size bytes, then skip pad byte if text_size is odd
];

pub const LIST_CHUNK_SPEC: &[(&str, &str)] = &[
    ("chunk_id",   "[u8;4]"),      // expect "LIST"
    ("chunk_size", "u32"),
    ("list_type",  "[u8;4]"),      // "INFO", etc.
    ("entries",    "Vec<LIST_ENTRY_SPEC>"), // repeated LIST_ENTRY_SPEC structures
];

pub const UNKNOWN_CHUNK_SPEC: &[(&str, &str)] = &[
    ("chunk_id",    "[u8;4]"),
    ("chunk_size",  "u32"),
    ("raw_payload", "Vec<u8>"),
];

#[derive(Debug)]

pub enum ParsedValue {
    Bytes4([u8;4]),
    U16(u16),
    U32(u32),
    Bytes(Vec<u8>),
    I16Vec(Vec<i16>),
}


fn read_simple_field<R: Read>(
    ftype: &str,
    r: &mut R,
    remaining_in_chunk: u32,
    chunk_id_already: [u8;4],
    chunk_size_already: u32,
    field_idx: usize,
) -> io::Result<(ParsedValue, u32)> {

    // chunk header virutal fields (index 0 and 1 in every spec)
    if field_idx == 0 && ftype == "[u8;4]" {
        // "chunk_id" (already known, costs 0 body bytes)
        return Ok((ParsedValue::Bytes4(chunk_id_already), remaining_in_chunk));
    }

    if field_idx == 1 && ftype == "u32" {
        // "chunk_size" (already known, costs 0 body bytes)
        return Ok((ParsedValue::U32(chunk_size_already), remaining_in_chunk));
    }

    // Now actual body consumption.
    match ftype {
        "[u8;4]" => {
            if remaining_in_chunk < 4 {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "not enough bytes left in chunk for [u8;4]",
                ));
            }
            let mut buf = [0u8;4];
            r.read_exact(&mut buf)?;
            Ok((ParsedValue::Bytes4(buf), remaining_in_chunk - 4))
        }

        "u16" => {
            if remaining_in_chunk < 2 {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "not enough bytes left in chunk for u16",
                ));
            }
            let mut b = [0u8;2];
            r.read_exact(&mut b)?;
            Ok((
                ParsedValue::U16(u16::from_le_bytes(b)),
                remaining_in_chunk - 2,
            ))
        }

        "u32" => {
            if remaining_in_chunk < 4 {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "not enough bytes left in chunk for u32",
                ));
            }
            let mut b = [0u8;4];
            r.read_exact(&mut b)?;
            Ok((
                ParsedValue::U32(u32::from_le_bytes(b)),
                remaining_in_chunk - 4,
            ))
        }

        "Vec<u8>" => {
            // slurp whatever's left in this chunk body
            let take_len = remaining_in_chunk as usize;
            let mut buf = vec![0u8; take_len];
            r.read_exact(&mut buf)?;
            Ok((ParsedValue::Bytes(buf), 0))
        }

        "Vec<i16>" => {
            let take_len = remaining_in_chunk as usize;
            let mut buf = vec![0u8; take_len];
            r.read_exact(&mut buf)?;

            if buf.len() % 2 != 0 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "odd number of bytes for Vec<i16>",
                ));
            }

            let mut samples = Vec::with_capacity(buf.len() / 2);
            for pair in buf.chunks_exact(2) {
                samples.push(i16::from_le_bytes([pair[0], pair[1]]));
            }

            Ok((ParsedValue::I16Vec(samples), 0))
        }

        other => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("unsupported field type {}", other),
        )),
    }
}

pub fn parse_wav(
    f: &mut (impl Read + Seek),
) -> io::Result<HashMap<String, ParsedValue>> {
    let mut out: HashMap<String, ParsedValue> = HashMap::new();

    let mut current_chunk_id = [0u8;4];
    f.read_exact(&mut current_chunk_id)?;

    let mut sz_buf = [0u8;4];
    f.read_exact(&mut sz_buf)?;
    let mut current_chunk_size = u32::from_le_bytes(sz_buf);

    let mut remaining_global: u32 = current_chunk_size;
    let mut unknown_i: usize = 0;

    while remaining_global > 0 {
        let (prefix, spec): (String, &[(&str, &str)]) = match &current_chunk_id {
            b"RIFF" => ("header".to_string(), HEADER_SPEC),
            b"fmt " => ("fmt".to_string(),   FMT_CHUNK_SPEC),
            b"data" => ("data".to_string(),  DATA_CHUNK_SPEC),
            //b"LIST" => ("list".to_string(),  LIST_CHUNK_SPEC),
            _ => {
                let pfx = format!("unknown{}", unknown_i);
                unknown_i += 1;
                (pfx, UNKNOWN_CHUNK_SPEC)
            }
        };

        let mut remaining_in_chunk: u32 = current_chunk_size;

        for (field_idx, (fname, ftype)) in spec.iter().enumerate() {
            let before = remaining_in_chunk;

            let (val, after_remaining_in_chunk) = read_simple_field(
                ftype,
                f,
                remaining_in_chunk,
                current_chunk_id,
                current_chunk_size,
                field_idx,
            )?;

            out.insert(format!("{}.{}", prefix, fname), val);
            
            let used = before - after_remaining_in_chunk;
            remaining_in_chunk = after_remaining_in_chunk;

            if used > remaining_global {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "field overran RIFF remaining bytes",
                ));
            }
            remaining_global -= used;
        }

        if prefix != "header" {
            if remaining_in_chunk != 0 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "chunk {}: {} leftover bytes not parsed",
                        prefix, remaining_in_chunk
                    ),
                ));
            }

            if (current_chunk_size % 2) == 1 {
                let mut pad = [0u8;1];
                f.read_exact(&mut pad)?;

                if remaining_global == 0 {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "ran out of RIFF budget reading padding byte",
                    ));
                }
                remaining_global -= 1;
            }
        }
        if remaining_global == 0 {
            break;
        }

        let mut next_id = [0u8;4];
        f.read_exact(&mut next_id)?;

        let mut next_sz_buf = [0u8;4];
        f.read_exact(&mut next_sz_buf)?;
        let next_size = u32::from_le_bytes(next_sz_buf);

        if remaining_global < 8 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "chunk header overruns RIFF remaining bytes",
            ));
        }
        remaining_global -= 8;

        current_chunk_id = next_id;
        current_chunk_size = next_size;
    }

    // ----- Final sanity check that this really was a RIFF/WAVE file -----
    if !(
        matches!(out.get("header.riff_id"),  Some(ParsedValue::Bytes4(b)) if b == b"RIFF") &&
        matches!(out.get("header.wave_id"),  Some(ParsedValue::Bytes4(b)) if b == b"WAVE")
    ) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Not a RIFF/WAVE file",
        ));
    }

    Ok(out)
}

// ---------- main: just parse and dump ----------

fn main() -> io::Result<()> {
    let mut file = File::open("knchoe.wav")?;
    let wav = parse_wav(&mut file)?;

    println!("{:#?}", wav);

    Ok(())
}
