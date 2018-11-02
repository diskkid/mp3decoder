#[macro_use]
extern crate lazy_static;

use std::fs::{File};
use std::io::{Result, BufReader, Read};
use std::collections::HashMap;

lazy_static! {
    static ref FRAME_SIZE_MAP: HashMap<u16, [usize;3]> = {
        let mut m = HashMap::new();
        // bitrate -> [32kHz, 44.1kHz, 48kHz]
        m.insert(32, [144, 104, 96]);
        m.insert(40, [180, 130, 120]);
        m.insert(48, [216, 156, 120]);
        m.insert(56, [252, 182, 168]);
        m.insert(64, [288, 208, 192]);
        m.insert(80, [360, 261, 240]);
        m.insert(96, [432, 313, 288]);
        m.insert(112, [504, 365, 336]);
        m.insert(128, [576, 417, 384]);
        m.insert(160, [720, 522, 480]);
        m.insert(192, [864, 626, 576]);
        m.insert(224, [1008, 731, 672]);
        m.insert(256, [1152, 835, 768]);
        m.insert(320, [1440, 1044, 960]);
        m
    };
}

static BITRATE_MAP: [[u16;3];15] = [
    // Layer 1, 2, 3
    [0, 0, 0],
    [32, 32, 32],
    [64, 48, 40],
    [96, 56, 48],
    [128, 64, 56],
    [160, 80, 64],
    [192, 96, 80],
    [224, 112, 96],
    [256, 128, 112],
    [288, 160, 128],
    [320, 192, 160],
    [352, 224, 192],
    [384, 256, 224],
    [416, 320, 256],
    [448, 384, 320],
];

static SAMPLING_FREQ_MAP: [u16;3] = [
    44100,
    48000,
    32000,
];

#[derive(Debug)]
enum Mode {
    Stereo,
    JointStereo,
    DualMonaural,
    SingleChannel,
}

#[derive(Debug)]
enum Layer {
    Reserved,
    L1,
    L2,
    L3,
}

#[derive(Debug)]
enum MpegVersion {
    V1,
    V2,
    V2_5,
}

#[derive(Debug)]
struct Mp3 {
}

#[derive(Debug)]
struct FrameHeader {
    id            : MpegVersion,
    layer         : Layer,
    protection    : bool,
    bitrate       : u16,
    sampling_freq : u16,
    padding       : bool,
    mode          : Mode,
    i_stereo      : bool,
    ms_stereo     : bool,
    copyright     : bool,
    original      : bool,
    emphasis      : u8,
    size          : usize,
}

impl FrameHeader {
    fn single_channel(&self) -> bool {
        match self.mode {
            Mode::SingleChannel => true,
            _ => false,
        }
    }
}

#[derive(Debug)]
struct SideInfo {
    main_data_begin: usize,
    scfsi: u8,
    // granule: [Granule; 2],
}

#[derive(Debug)]
struct Granule {
    channels: [Channel; 2],
}

#[derive(Debug)]
struct Channel {
    part2_3_length: u16,
    big_values: u16,
    global_gain: u8,
    scalefac_compress: u8,
    preemphasis: bool,
    scalefac_scale: bool,
    count1table_select: bool,
    // windows_switching_flag == 0
    table_select: [u8; 3],
    region_0_count: u8,
    region_1_count: u8,
    // windows_switching_flag == 1
    block_type: BlockType,
    subblock_gain: [u8; 3],
}

#[derive(Debug)]
enum BlockType {
    Normal,
    Start,
    Short,
    Mixed,
    End,
}

#[derive(Debug)]
struct Frame {
    header: FrameHeader,
    body: Vec<u8>,
}

fn has_sync_word(frame_header: &[u8;4]) -> bool {
    frame_header[0] == 0b11111111 && frame_header[1] & 0b11100000 == 0b11100000
}

fn new_frame_header(reader: &mut Read) -> Result<FrameHeader> {
    let mut frame_header = [0u8;4];
    reader.read_exact(&mut frame_header)?;
    let id = match (frame_header[1] & 0b00011000) >> 3 {
        0 => MpegVersion::V2_5,
        2 => MpegVersion::V2,
        3 => MpegVersion::V1,
        x => panic!("{} is not supported MPEG version ID", x),
    };
    let layer = match (frame_header[1] & 0b00000110) >> 1 {
        0 => Layer::Reserved,
        1 => Layer::L3,
        2 => Layer::L2,
        3 => Layer::L1,
        x => panic!("{} is not supported layer", x),
    };
    // 0: CRC
    // 1: No CRC
    let protection = frame_header[1] & 0b00000001 != 0b00000001;

    let bitrate_index = ((frame_header[2] & 0b11110000) >> 4) as usize;
    let layer_index = match layer {
        Layer::L3 => 2,
        Layer::L2 => 1,
        Layer::L1 => 0,
        _ => panic!("Layer::Reserved is not supported"),
    };
    let bitrate = BITRATE_MAP[bitrate_index][layer_index];

    let sampling_freq_index = (frame_header[2] & 0b00001100 >> 2) as usize;
    let sampling_freq = SAMPLING_FREQ_MAP[sampling_freq_index];

    let padding = frame_header[2] & 0b00000010 == 0b00000010;
    let mode = match (frame_header[3] & 0b11000000) >> 6 {
        0 => Mode::Stereo,
        1 => Mode::JointStereo,
        2 => Mode::DualMonaural,
        3 => Mode::SingleChannel,
        x => panic!("{} is not supported mode", x),
    };
    let i_stereo = frame_header[3] & 0b00010000 == 0b00010000;
    let ms_stereo = frame_header[3] & 0b00100000 == 0b00100000;
    let copyright = frame_header[3] & 0b00001000 == 0b00001000;
    let original = frame_header[3] & 0b00000100 == 0b00000100;
    let emphasis = frame_header[3] & 0b00000011;
    let i = match sampling_freq {
        44100 => 1,
        48000 => 2,
        32000 => 0,
        x => panic!("{} is not supported sampling frequency", x),
    };
    let size = FRAME_SIZE_MAP.get(&bitrate).unwrap()[i];
    Ok(FrameHeader {
        id,
        layer,
        protection,
        bitrate,
        sampling_freq,
        padding,
        mode,
        i_stereo,
        ms_stereo,
        copyright,
        original,
        emphasis,
        size,
    })
}

fn new_side_info(reader: &mut Read, header: &FrameHeader) -> Result<SideInfo> {
    if header.single_channel() {
        let mut side = [0u8;17];
        reader.read_exact(&mut side)?;
        Ok(new_side_info_mono(&side))
    } else {
        let mut side = [0u8;32];
        reader.read_exact(&mut side)?;
        Ok(new_side_info_stereo(&side))
    }
}

fn new_side_info_mono(side: &[u8;17]) -> SideInfo {
    let main_data_begin = ((side[0] << 1) | ((side[1] & 0b10000000) >> 7)) as usize;
    let scfsi = ((side[1] & 0b00000011) << 2) | ((side[2] & 0b11000000) >> 6);
    SideInfo{
        main_data_begin,
        scfsi,
        // granule: [granule1, granule2],
    }
}

fn new_side_info_stereo(side: &[u8;32]) -> SideInfo {
    let main_data_begin = ((side[0] << 1) | ((side[1] & 0b10000000) >> 7)) as usize;
    let scfsi = ((side[1] & 0b00111111) << 2) | ((side[2] & 0b11000000) >> 6);
    SideInfo{
        main_data_begin,
        scfsi,
        // granule: [granule1, granule2],
    }
}

// 59bit per channel, assume bits are aligned to highest bit
fn new_channel(bytes: &[u8;8]) -> Channel {
    // 12bits
    let part2_3_length = ((bytes[0] as u16) << 4) | ((bytes[1] & 0b11110000) as u16 >> 4);

    // 9bits
    let big_values = (((bytes[1] & 0b00001111) as u16) << 5) | ((bytes[2] & 0b11111000) as u16 >> 3);

    // 8bits
    let global_gain = ((bytes[2] & 0b00000111) << 5) | ((bytes[3] & 0b11111000) >> 3);

    // 4bits
    let scalefac_compress = ((bytes[3] & 0b00000111) << 1) | ((bytes[4] & 0b10000000) >> 7);

    let preemphasis = bytes[7] & 0b10000000 == 0b10000000;
    let scalefac_scale = bytes[7] & 0b01000000 == 0b01000000;
    let count1table_select = bytes[7] & 0b00100000 == 0b00100000;

    // 1bit
    let windows_witching_flag = (bytes[4] & 0b01000000) >> 6;
    if windows_witching_flag == 0 {
        let block_type = BlockType::Normal;
        // 5bits
        let table_select_1 = (bytes[4] & 0b00111110) >> 1;
        // 5bits
        let table_select_2 = ((bytes[4] & 0b00000001) << 4) | ((bytes[5] & 0b11110000) >> 4);
        // 5bits
        let table_select_3 = ((bytes[5] & 0b00001111) << 1) | ((bytes[6] & 0b10000000) >> 7);
        // 4bits
        let region_0_count = (bytes[6] & 0b01111000) >> 3;
        // 3bits
        let region_1_count = bytes[6] & 0b00000111;
        Channel {
            part2_3_length,
            big_values,
            global_gain,
            scalefac_compress,
            block_type,
            table_select: [table_select_1, table_select_2, table_select_3],
            region_0_count,
            region_1_count,
            preemphasis,
            scalefac_scale,
            count1table_select,
            subblock_gain: [0;3],
        }
    } else {
        let block_type = match (bytes[4] & 0b00110000) >> 4 {
            1 => BlockType::Start,
            2 => if (bytes[4] & 0b00001000) == 0b00001000 {
                BlockType::Mixed
            } else {
                BlockType::Short
            },
            3 => BlockType::End,
            x => panic!("{} is not supported block type", x),
        };
        let table_select_1 = ((bytes[4] & 0b00000111) << 2) | ((bytes[5] & 0b11000000) >> 6);
        let table_select_2 = (bytes[5] & 0b00111110) >> 1;
        let subblock_gain_1 = ((bytes[5] & 0b00000001) << 2) | ((bytes[6] & 0b11000000) >> 6);
        let subblock_gain_2 = (bytes[6] & 0b00111000) >> 3;
        let subblock_gain_3 = bytes[6] & 0b00000111;
        Channel {
            part2_3_length,
            big_values,
            global_gain,
            scalefac_compress,
            block_type,
            table_select: [table_select_1, table_select_2, 0],
            region_0_count: 0,
            region_1_count: 0,
            preemphasis,
            scalefac_scale,
            count1table_select,
            subblock_gain: [subblock_gain_1, subblock_gain_2, subblock_gain_3],
        }
    }
}

fn open(file_path: &str) -> Result<Mp3> {
    let mut file = BufReader::new(File::open(file_path)?);
    // Search sync word
    let mut frame_header = [0u8;4];
    file.read_exact(&mut frame_header)?;
    if has_sync_word(&frame_header) {
        let header = new_frame_header(&mut file)?;
        if header.protection {
            // CRC
        }
        let mut side = [0u8;4];
        file.read_exact(&mut side)?;
        let side = new_side_info(&mut file, &header)?;
        println!("{:?}", side);
        let mut body = vec![0; header.size];
        file.read_exact(body.as_mut_slice())?;
        let frame = Frame{header, body};
        println!("{:?}", frame);
    }
    file.read_exact(&mut frame_header)?;
    if has_sync_word(&frame_header) {
        let header = new_frame_header(&mut file)?;
        let mut body = vec![0; header.size];
        file.read_exact(body.as_mut_slice())?;
        let frame = Frame{header, body};
        println!("{:?}", frame);
    }
    Ok(Mp3{})
}

fn main() {
    let mut args = std::env::args();
    if let Some(file_path) = args.nth(1) {
        let mp3 = open(&file_path);
        println!("{:?}", mp3);
    } else {
        println!("Usage: mp3decoder <file>");
    }
}
