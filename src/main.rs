use std::fs::File;
use std::io::Read;
use std::fmt;

struct TextField([u8; 4]);

impl TextField {
    fn to_string(&self) -> String {
        String::from_utf8_lossy(&self.0).to_string()
    }
}

impl From<&[u8]> for TextField {
    fn from(bytes: &[u8]) -> Self {
        let four_bytes: &[u8; 4] = bytes.try_into().unwrap();
        TextField(*four_bytes)
    }
}

impl fmt::Display for TextField {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_string())
    }
}

fn buffer_to_textfield(buffer: &[u8]) -> TextField {
    buffer[0..4].try_into().unwrap()
}

fn buffer_to_u16(buffer: &[u8]) -> u16 {
    u16::from_le_bytes(buffer[0..2].try_into().unwrap())
}

fn buffer_to_u32(buffer: &[u8]) -> u32 {
    u32::from_le_bytes(buffer[0..4].try_into().unwrap())
}

struct Header {
    chunk_id: TextField, // "RIFF" -- RIFF Format
    chunk_size: u32,     //  Number of bytes minus 8 -- the first 8 bytes
    format: TextField,   // "WAVE" -- it's a wave file
}

impl Header {
    fn read(file: &mut File) -> Self {
        let mut buffer = [0u8; 12];
        file.read_exact(&mut buffer).expect("Is file missing header?");
        Header {
            chunk_id: buffer_to_textfield(&buffer[0..4]),
            chunk_size: buffer_to_u32(&buffer[4..8]),
            format: buffer_to_textfield(&buffer[8..12])
        }
    }
}

struct FmtChunk {
    chunk_id: TextField, // "fmt "
    chunk_size: u32,    // Chunk size: 16, 18 or 40
    audio_format: u16,  // Format code: PCM = 1
    num_channels: u16,  // Mono = 1, Stereo = 2, etc.
    sample_rate: u32,   // Samples per second 44100, 48000, etc.
    byte_rate: u32,     // sample_rate * block_align
    block_align: u16,    // data block size: num_channels * bits_per_sample/8
    bits_per_sample: u16, // 8 bits, 16 bits, etc.
}

impl FmtChunk{
    fn parse(buffer: &[u8], chunk_id: TextField, chunk_size: u32) -> Option<Self> {
        Some(FmtChunk {
            chunk_id: chunk_id,
            chunk_size: chunk_size,
            audio_format: buffer_to_u16(&buffer[0..2]),
            num_channels: buffer_to_u16(&buffer[2..4]),
            sample_rate: buffer_to_u32(&buffer[4..8]),
            byte_rate: buffer_to_u32(&buffer[8..12]),
            block_align: buffer_to_u16(&buffer[12..14]),
            bits_per_sample: buffer_to_u16(&buffer[14..16])
        })
    }
}
struct ListDataChunk {
    info_id: TextField,
    info_size: u32,
    info: String,
}

impl ListDataChunk {
    fn parse(buffer: &[u8], offset: usize) -> Option<(ListDataChunk, usize)> {
        let info_id = buffer_to_textfield(&buffer[offset..offset+4]);
        let info_size = buffer_to_u32(&buffer[offset+4..offset+8]);

        let subchunk_data = &buffer[offset+8..offset+8+info_size as usize];
        let info = match std::str::from_utf8(subchunk_data) {
            Ok(s) => s.trim_end_matches('\0').to_string(),
            Err(_) => { return Option::None; }
        };

        let new_offset = offset + 8 + info_size as usize + info_size as usize % 2;

        Some((
            ListDataChunk {
                info_id,
                info_size,
                info,
            },
            new_offset,
        ))
    }
}

impl fmt::Display for ListDataChunk {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Format: ID: <info_id>, Size: <info_size>, Info: <info>
        write!(
            f,
            "Info type: {}\n    Size: {}\n    Info: {}",
            self.info_id, self.info_size, self.info
        )
    }
}

struct ListChunk {
    chunk_id: TextField,
    chunk_size: u32,
    list_type_id: TextField,
    data: Vec<ListDataChunk>

}

impl ListChunk {

    fn parse(buffer: &[u8], chunk_id: TextField, chunk_size: u32) -> Option<Self> {
        let list_type_id = buffer_to_textfield(&buffer[0..4]);
        let mut data = Vec::new();
        let mut offset = 4;

        while offset + 8 <= buffer.len() {
            match ListDataChunk::parse(buffer, offset) {
                Some((subchunk, new_offset)) => {
                    data.push(subchunk);
                    offset = new_offset;
                },
                None => { break; }
            }
        }


        Some(ListChunk {
            chunk_id,
            chunk_size,
            list_type_id,
            data
        })
    }
}

struct DataChunk {
    chunk_id: TextField,    // "data"
    chunk_size: u32,    // Number of bytes in data
    data: Vec<i16>,          // The actual audio data
}

impl DataChunk {
    fn parse(buffer: &[u8], chunk_id: TextField, chunk_size: u32) -> Option<Self> {
        let mut samples = Vec::with_capacity((chunk_size / 2) as usize);
        for i in (0..chunk_size as usize).step_by(2) {
            let sample = i16::from_le_bytes(buffer[i..i+2].try_into().unwrap());
            samples.push(sample);
        }

        Some(DataChunk {
            chunk_id,
            chunk_size,
            data: samples
        })
    }
}

struct FfmpegWavFile {
    header: Header,
    fmt: FmtChunk,
    list: ListChunk,
    data: DataChunk
}

impl FfmpegWavFile {
    fn parse(file: &mut File) -> Option<Self> {
        let header = Header::read(file);

        if header.chunk_id.to_string() != "RIFF" || header.format.to_string() != "WAVE" {
            println!("Not the correct RIFF/WAVE header format");
            return None
        }

        let mut fmt: Option<FmtChunk> = Option::None;
        let mut list: Option<ListChunk> = Option::None;
        let mut data: Option<DataChunk> = Option::None;

        loop {
            let mut chunk_header = [0u8; 8];
            match file.read_exact(&mut chunk_header) {
                Ok(_) => {}, 
                Err(_) => { break; }
            }
            let chunk_id = buffer_to_textfield(&chunk_header[0..4]);
            let chunk_size = buffer_to_u32(&chunk_header[4..8]);

            let mut chunk_data = vec![0u8; chunk_size as usize];
            match file.read_exact(&mut chunk_data) {
                Ok(_) => {}, 
                Err(_) => { break; }
            }

            match chunk_id.to_string().as_str() {
                "fmt " => {
                    fmt = FmtChunk::parse(&chunk_data, chunk_id, chunk_size);
                },
                "LIST" => {
                    list = ListChunk::parse(&chunk_data, chunk_id, chunk_size);
                },
                "data" => {
                    data = DataChunk::parse(&chunk_data, chunk_id, chunk_size);
                },
                _ => {
                }
            };
        }
        Some(FfmpegWavFile {
            header,
            fmt: fmt.unwrap(),
            list: list.unwrap(),
            data: data.unwrap()
        })

    }

}
fn main() {
    let mut file = File::open("knchoe.wav").expect("File could not be opened");
    let wav_file = FfmpegWavFile::parse(&mut file).expect("Failed to parse WAV file");

    println!("RIFF Header Chunk ID: {}", wav_file.header.chunk_id);
    println!("File Size (Minus 8 bytes): {}", wav_file.header.chunk_size);
    println!("RIFF File Format: {}", wav_file.header.format);

    println!("\nFMT Subchunk:");
    println!("  Chunk ID: {}", wav_file.fmt.chunk_id);
    println!("  Subchunk1 Size: {}", wav_file.fmt.chunk_size);
    println!("  Audio Format: {}", wav_file.fmt.audio_format);
    println!("  Number of Channels: {}", wav_file.fmt.num_channels);
    println!("  Sample Rate: {}", wav_file.fmt.sample_rate);
    println!("  Byte Rate: {}", wav_file.fmt.byte_rate);
    println!("  Block Align: {}", wav_file.fmt.block_align);
    println!("  Bits Per Sample: {}", wav_file.fmt.bits_per_sample);

    println!("\nLIST Subchunk:");
    println!("  Chunk ID: {}", wav_file.list.chunk_id);
    println!("  Chunk Size: {}", wav_file.list.chunk_size);
    println!("  List Type ID: {}", wav_file.list.list_type_id);
    println!("  Data Subchunks Length: {}", wav_file.list.data.len());
    for (i, subchunk) in wav_file.list.data.iter().enumerate() {
        println!("\n  Subchunk {}:", i + 1);
        println!("    {}", subchunk);
    }
    println!("\nDATA Subchunk:");
    println!("  Chunk ID: {}", wav_file.data.chunk_id);
    println!("  Chunk Size: {}", wav_file.data.chunk_size);
    println!("  Data Length: {} samples", wav_file.data.data.len());
}
