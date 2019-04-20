use std::collections::HashMap;
use std::fs::File;
use std::io::{prelude::*, Error, ErrorKind, SeekFrom};
use std::path::Path;
use std::result::Result;

use bitreader::BitReader;

use flate2::read::GzDecoder;
use flate2::read::ZlibDecoder;

use ::nbt::Blob;

use regex::Regex;

use super::nbt;

fn read_u32(file: &mut File) -> Result<u32, Error> {
    let mut buf = [0; 4];
    file.read(&mut buf)?;
    Ok(((buf[0] as u32) << 24) | ((buf[1] as u32) << 16) | ((buf[2] as u32) << 8) | buf[3] as u32)
}

pub fn read_world_regions(path: &Path) -> Result<Vec<(i32, i32)>, Error> {
    if !path.is_dir() {
        return Err(Error::new(ErrorKind::NotFound, "Directory not found."));
    }

    let region_path = path.join("region");
    if !region_path.is_dir() {
        return Err(Error::new(ErrorKind::NotFound, "No region subdirectory found in path."));
    }

    let mut regions = Vec::new();
    let re = Regex::new(r"^r\.([-\d]+)\.([-\d]+)\.mca$").unwrap();

    for entry in std::fs::read_dir(region_path)? {
        if let Some(filename) = entry?.file_name().to_str() {
            if let Some(caps) = re.captures(filename) {
                let rx = caps.get(1).unwrap().as_str().parse::<i32>().unwrap();
                let rz = caps.get(2).unwrap().as_str().parse::<i32>().unwrap();
                regions.push((rx, rz));
            }
        }
    }

    Ok(regions)
}

pub fn read_region_chunks(path: &Path) -> Result<[bool; 1024], Error> {
    let mut file = File::open(path)?;
    let mut chunks = [false; 1024];

    for p in 0..1024 {
        if read_u32(&mut file)? > 0 {
            chunks[p] = true;
        }
    }

    Ok(chunks)
}

pub fn read_region_chunk_coords(path: &Path) -> Result<Vec<(u8, u8)>, Error> {
    let mut file = File::open(path)?;
    let mut chunks: Vec<(u8, u8)> = vec![];

    for cz in 0..32 {
        for cx in 0..32 {
            if read_u32(&mut file)? > 0 {
                chunks.push((cx, cz));
            }
        }
    }

    Ok(chunks)
}

fn read_region_chunk(file: &mut File, cx: u8, cz: u8) -> Result<Option<Blob>, Error> {
    let co = (cz as u64 * 32 + cx as u64) * 4;
    file.seek(SeekFrom::Start(co))?;

    let offset = (read_u32(file)? >> 8) * 4096;
    if offset > 0 {
        file.seek(SeekFrom::Start(offset as u64))?;
        let size = read_u32(file)? as usize;
        file.seek(SeekFrom::Current(1))?;

        let mut reader = ZlibDecoder::new_with_buf(file, vec![0u8; size - 1]);
        Ok(Some(Blob::from_reader(&mut reader)?))
    }
    else {
        Ok(None)
    }
}

fn get_region_chunk_reader(file: &mut File, cx: u8, cz: u8)
-> Result<Option<ZlibDecoder<&mut File>>, Error> {
    let co = (cz as u64 * 32 + cx as u64) * 4;
    file.seek(SeekFrom::Start(co))?;

    let offset = (read_u32(file)? >> 8) * 4096;
    if offset > 0 {
        file.seek(SeekFrom::Start(offset as u64))?;
        let size = read_u32(file)? as usize;
        file.seek(SeekFrom::Current(1))?;

        let mut reader = ZlibDecoder::new_with_buf(file, vec![0u8; size - 1]);
        nbt::read_tag_header(&mut reader)?;
        Ok(Some(reader))
    }
    else {
        Ok(None)
    }
}

pub fn read_region_chunk_block_maps(path: &Path, block_names: &[&str])
-> Result<HashMap<(u8, u8), [u16; 65536]>, Error> {
    let mut file = File::open(path)?;
    let mut blockmaps = HashMap::new();

    for cz in 0..32 {
        for cx in 0..32 {
            if let Some(chunk) = read_region_chunk(&mut file, cx, cz)? {
                // println!("Reading chunk {}, {}", cx, cz);
                let value: serde_json::Value = serde_json::to_value(&chunk)?;

                let mut blocks = [0u16; 65536];

                for section in value["Level"]["Sections"].as_array().unwrap() {
                    let so = (section["Y"].as_u64().unwrap() * 4096) as usize;

                    let palette: Vec<u16> = section["Palette"].as_array().unwrap().iter()
                        .map(|obj| obj["Name"].as_str().unwrap().trim_start_matches("minecraft:"))
                        .map(|block| block_names.iter().position(|b| b == &block).unwrap() as u16)
                        .collect();

                    let states = section["BlockStates"].as_array().unwrap();
                    let len = states.len();

                    let mut bytes = vec![0u8; len * 8];
                    for i in 0..len {
                        if let Some(long) = states[len - i - 1].as_i64() {
                            for b in 0..8 {
                                bytes[i * 8 + b] = (long >> ((7 - b) * 8)) as u8;
                            }
                        }
                    }

                    // BlockStates is an array of i64 representing 4096 blocks,
                    // but we have to check the array length to determine the # of bits per block.
                    let bits = (len / 64) as u8;

                    let mut br = BitReader::new(&bytes);
                    for i in (0..4096).rev() {
                        blocks[so + i] = palette[br.read_u16(bits).unwrap() as usize];
                    }
                }

                blockmaps.insert((cx as u8, cz as u8), blocks);
            }
        }
    }

    Ok(blockmaps)
}

pub fn read_region_chunk_biomes(path: &Path) -> Result<HashMap<(u8, u8), [u8; 256]>, Error> {
    let mut file = File::open(path)?;
    let mut biomes = HashMap::new();

    for cz in 0..32 {
        for cx in 0..32 {
            if let Some(chunk) = read_region_chunk(&mut file, cx, cz)? {
                let value: serde_json::Value = serde_json::to_value(&chunk)?;

                let mut bytes = [0u8; 256];
                for i in 0..256 {
                    bytes[i] = value["Level"]["Biomes"][i].as_u64().unwrap_or(0) as u8;
                }

                biomes.insert((cx as u8, cz as u8), bytes);
            }
        }
    }

    Ok(biomes)
}

pub fn read_region_chunk_heightmaps(path: &Path) -> Result<HashMap<(u8, u8), [u8; 256]>, Error> {
    let mut file = File::open(path)?;
    let mut heightmaps = HashMap::new();

    for cz in 0..32 {
        for cx in 0..32 {
            if let Some(mut reader) = get_region_chunk_reader(&mut file, cx, cz)? {
                nbt::scan_compound_tag(&mut reader, "Level")?;
                nbt::scan_compound_tag(&mut reader, "Heightmaps")?;
                nbt::scan_compound_tag(&mut reader, "WORLD_SURFACE")?;

                let longs = nbt::read_long_array(&mut reader)?;
                let mut bytes = [0u8; 288];
                for i in 0..36 {
                    let long = longs[35 - i];
                    for b in 0..8 {
                        bytes[i * 8 + b] = (long >> ((7 - b) * 8)) as u8;
                    }
                }

                let mut br = BitReader::new(&bytes);
                let mut heights = [0u8; 256];
                for i in (0..256).rev() {
                    heights[i] = br.read_u16(9).unwrap() as u8;
                }

                heightmaps.insert((cx as u8, cz as u8), heights);
            }
        }
    }

    Ok(heightmaps)
}

pub fn read_dat_file(path: &Path) -> Result<(), Error> {
    let file = File::open(path)?;
    let mut reader = GzDecoder::new(file);

    println!("================================= NBT Contents =================================");
    let blob = match Blob::from_reader(&mut reader) {
        Ok(blob) => blob,
        Err(err) => return Err(Error::new(ErrorKind::InvalidData,
            format!("Error reading NBT: {}", err))),
    };
    println!("{}", blob);

    println!("============================== JSON Representation =============================");
    let json = match serde_json::to_string_pretty(&blob) {
        Ok(json) => json,
        Err(err) => return Err(Error::new(ErrorKind::InvalidData,
            format!("Error formatting NBT as JSON: {}", err))),
    };
    println!("{}", json);

    Ok(())
}
