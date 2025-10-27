use std::fs::File;
use std::io::{self, Read};

// ---- core structs ----

#[derive(Debug)]
struct WavFile {
    header: Header,
    fmt:   Option<FmtChunk>,
    list:  Option<ListChunk>,
    data:  Option<DataChunk>,
}

impl WavFile {
    fn parse(file: &mut File) -> io::Result<Self> {
        let header = Header::read(file)?;

        if !(&header.chunk_id == b"RIFF" || &header.format == b"WAVE") {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Not RIFF/WAVE",
            ));
        }

        let mut fmt  = None;
        let mut list = None;
        let mut data = None;

        while let Some(hdr) = read_chunk_header(file)? {
            let body = read_chunk_body(file, hdr.size)?;

            match &hdr.id {
                b"fmt " => { fmt = Some(FmtChunk::parse(&body, &hdr)); }
                b"LIST" => { list = Some(ListChunk::parse(&body, &hdr)); }
                b"data" => { data = Some(DataChunk::parse(&body, &hdr)); }
                _ => { /* ignore unknown chunks */ }
            }
        }

        Ok(WavFile { header, fmt, list, data })
    }
}

#[derive(Debug)]
struct Header {
    chunk_id: [u8;4], // "RIFF"
    chunk_size: u32,  // file bytes after this field, minus 8 overall header bytes
    format:   [u8;4], // "WAVE"
}

impl Header {
    fn read(file: &mut File) -> io::Result<Self> {
        let mut buf = [0u8; 12];
        file.read_exact(&mut buf)?;

        Ok(Header {
            chunk_id: buf[0..4].try_into().unwrap(),
            chunk_size: u32::from_le_bytes(buf[4..8].try_into().unwrap()),
            format: buf[8..12].try_into().unwrap(),
        })
    }
}

#[derive(Debug)]
struct FmtChunk {
    chunk_id: [u8;4],    // "fmt "
    chunk_size: u32,     // usually 16 for PCM
    audio_format: u16,   // PCM = 1
    num_channels: u16,   // 1=mono,2=stereo,...
    sample_rate: u32,    // 44100, 48000, ...
    byte_rate: u32,      // sample_rate * block_align
    block_align: u16,    // num_channels * bits_per_sample/8
    bits_per_sample: u16 // 8, 16, 24, ...
}

impl FmtChunk {
    fn parse(body: &[u8], hdr: &ChunkHeader) -> Self {
        FmtChunk {
            chunk_id: hdr.id,
            chunk_size: hdr.size,
            audio_format: u16::from_le_bytes(body[0..2].try_into().unwrap()),
            num_channels: u16::from_le_bytes(body[2..4].try_into().unwrap()),
            sample_rate: u32::from_le_bytes(body[4..8].try_into().unwrap()),
            byte_rate:   u32::from_le_bytes(body[8..12].try_into().unwrap()),
            block_align: u16::from_le_bytes(body[12..14].try_into().unwrap()),
            bits_per_sample: u16::from_le_bytes(body[14..16].try_into().unwrap()),
        }
    }
}

#[derive(Debug)]
struct ListDataChunk {
    info_id:   [u8;4],
    info_size: u32,
    info:      String,
}

#[derive(Debug)]
struct ListChunk {
    chunk_id:     [u8;4],   // "LIST"
    chunk_size:   u32,
    list_type_id: [u8;4],   // "INFO"
    data:         Vec<ListDataChunk>,
}

impl ListChunk {
    fn parse(body: &[u8], hdr: &ChunkHeader) -> Self {
        // first 4 bytes of body = list_type_id
        let list_type_id: [u8;4] = body[0..4].try_into().unwrap();
        let mut data_chunks = Vec::new();
        let mut offset = 4;

        // walk subchunks inside LIST
        while offset + 8 <= body.len() {
            let info_id: [u8;4] = body[offset..offset+4].try_into().unwrap();
            let info_size = u32::from_le_bytes(body[offset+4..offset+8].try_into().unwrap());

            let start = offset + 8;
            let end   = start + info_size as usize;
            if end > body.len() { break; }

            // interpret text payload
            let raw = &body[start..end];
            if let Ok(s) = std::str::from_utf8(raw) {
                let cleaned = s.trim_end_matches('\0').to_string();
                data_chunks.push(ListDataChunk {
                    info_id,
                    info_size,
                    info: cleaned,
                });
            }

            // advance offset, respecting word alignment
            let pad = (info_size as usize) % 2;
            offset = end + pad;
        }

        ListChunk {
            chunk_id: hdr.id,
            chunk_size: hdr.size,
            list_type_id,
            data: data_chunks,
        }
    }
}

#[derive(Debug)]
struct DataChunk {
    chunk_id:   [u8;4],   // "data"
    chunk_size: u32,      // number of bytes in PCM payload
    data:       Vec<i16>, // PCM16 samples
}

impl DataChunk {
    fn parse(body: &[u8], hdr: &ChunkHeader) -> Self {
        // interpret body as little-endian i16 frames
        let mut samples = Vec::with_capacity((hdr.size / 2) as usize);
        for i in (0..hdr.size as usize).step_by(2) {
            let s = i16::from_le_bytes(body[i..i+2].try_into().unwrap());
            samples.push(s);
        }

        DataChunk {
            chunk_id: hdr.id,
            chunk_size: hdr.size,
            data: samples,
        }
    }
}

// tiny struct for generic chunk header
#[derive(Clone, Copy, Debug)]
struct ChunkHeader {
    id:   [u8;4],
    size: u32,
}

// read 8-byte chunk header; return None on EOF
fn read_chunk_header(file: &mut File) -> io::Result<Option<ChunkHeader>> {
    let mut buf = [0u8; 8];
    match file.read_exact(&mut buf) {
        Ok(_) => {
            let id   = buf[0..4].try_into().unwrap();
            let size = u32::from_le_bytes(buf[4..8].try_into().unwrap());
            Ok(Some(ChunkHeader { id, size }))
        }
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => Ok(None),
        Err(e) => Err(e),
    }
}

// read exactly `size` bytes of body, plus eat the 1-byte pad if size is odd
fn read_chunk_body(file: &mut File, size: u32) -> io::Result<Vec<u8>> {
    let mut body = vec![0u8; size as usize];
    file.read_exact(&mut body)?;
    if (size % 2) == 1 {
        let mut pad = [0u8;1];
        // ignore errors on pad? we'll just try to read it
        let _ = file.read_exact(&mut pad);
    }
    Ok(body)
}

fn fourcc_to_string(id: [u8;4]) -> String {
    String::from_utf8_lossy(&id).to_string()
}

fn main() -> io::Result<()> {
    let mut file = File::open("knchoe.wav")?;
    let wav = WavFile::parse(&mut file)?;

    println!("RIFF Header Chunk ID: {}", fourcc_to_string(wav.header.chunk_id));
    println!("File Size (Minus 8 bytes): {}", wav.header.chunk_size);
    println!("RIFF File Format: {}", fourcc_to_string(wav.header.format));

    if let Some(fmt) = &wav.fmt {
        println!("\nFMT Subchunk:");
        println!("  Chunk ID: {}", fourcc_to_string(fmt.chunk_id));
        println!("  Subchunk1 Size: {}", fmt.chunk_size);
        println!("  Audio Format: {}", fmt.audio_format);
        println!("  Number of Channels: {}", fmt.num_channels);
        println!("  Sample Rate: {}", fmt.sample_rate);
        println!("  Byte Rate: {}", fmt.byte_rate);
        println!("  Block Align: {}", fmt.block_align);
        println!("  Bits Per Sample: {}", fmt.bits_per_sample);
    }

    if let Some(list) = &wav.list {
        println!("\nLIST Subchunk:");
        println!("  Chunk ID: {}", fourcc_to_string(list.chunk_id));
        println!("  Chunk Size: {}", list.chunk_size);
        println!("  List Type ID: {}", fourcc_to_string(list.list_type_id));
        println!("  Data Subchunks Length: {}", list.data.len());

        for (i, sub) in list.data.iter().enumerate() {
            println!("\n  Subchunk {}:", i + 1);
            println!("    Info type: {}", fourcc_to_string(sub.info_id));
            println!("    Size: {}", sub.info_size);
            println!("    Info: {}", sub.info);
        }
    }

    if let Some(data) = &wav.data {
        println!("\nDATA Subchunk:");
        println!("  Chunk ID: {}", fourcc_to_string(data.chunk_id));
        println!("  Chunk Size: {}", data.chunk_size);
        println!("  Data Length: {} samples", data.data.len());
    }

    Ok(())
}
