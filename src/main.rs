use std::fs::File;
use std::io::Read;

struct RiffHeader {
    chunk_id: String,    // "RIFF"
    chunk_size: u32,     // File size minus 8 bytes
    format: String,      // "WAVE"
}

struct FmtSubchunk {
    subchunk1_id: String,   // "fmt "
    subchunk1_size: u32,    // 16 for PCM
    audio_format: u16,      // PCM = 1
    num_channels: u16,      // Mono = 1, Stereo = 2, etc.
    sample_rate: u32,       // 44100, 48000, etc.
    byte_rate: u32,         // SampleRate * NumChannels * BitsPerSample/8
    block_align: u16,       // NumChannels * BitsPerSample/8
    bits_per_sample: u16,   // 8 bits, 16 bits, etc.
}

struct DataSubchunk {
    subchunk2_id: String,   // "data"
    subchunk2_size: u32,    // Number of bytes in data
    data: Vec<u8>,          // The actual audio data
}
fn main() {

    let mut file = File::open("output.wav").expect("File could not be opened");

    const RIFF_HEADER_NUM_BYTES: usize = 4 + 4 + 4;
    const FMT_HEADER_NUM_BYTES: usize = 4 + 4 + 2 + 2 + 4 + 4 + 2 + 2;
    const DATA_HEADER_NUM_BYTES: usize = 4 + 4;
    const WAV_HEADER_NUM_BYTES: usize = RIFF_HEADER_NUM_BYTES
                                        + FMT_HEADER_NUM_BYTES
                                        + DATA_HEADER_NUM_BYTES; 
    let mut buffer = [0u8; WAV_HEADER_NUM_BYTES];

    file.read_exact(&mut buffer).expect("could not read to buffer");

    let riff_buffer = &buffer[0..RIFF_HEADER_NUM_BYTES];
    let fmt_buffer = &buffer[RIFF_HEADER_NUM_BYTES..RIFF_HEADER_NUM_BYTES+FMT_HEADER_NUM_BYTES];
    let data_buffer = &buffer[RIFF_HEADER_NUM_BYTES+FMT_HEADER_NUM_BYTES..WAV_HEADER_NUM_BYTES];

        // Extract and print the first 4 bytes of RIFF Header (chunk_id)
    let chunk_id_bytes = &riff_buffer[0..4];
    let chunk_id = String::from_utf8_lossy(chunk_id_bytes);
    println!("RIFF Chunk ID: {}", chunk_id);

    // Extract and print the last 4 bytes of RIFF Header (format)
    let format_bytes = &riff_buffer[8..12];
    let format = String::from_utf8_lossy(format_bytes);
    println!("RIFF Format: {}", format);

    // Extract and print the first 4 bytes of fmt Subchunk (subchunk1_id)
    let subchunk1_id_bytes = &fmt_buffer[0..4];
    let subchunk1_id = String::from_utf8_lossy(subchunk1_id_bytes);
    println!("Fmt Subchunk ID: {}", subchunk1_id);

    // Extract and print the first 4 bytes of data Subchunk (subchunk2_id)
    let subchunk2_id_bytes = &data_buffer[0..4];
    let subchunk2_id = String::from_utf8_lossy(subchunk2_id_bytes);
    println!("Data Subchunk ID: {}", subchunk2_id);

}
