use std::collections::HashMap;
use std::fs::File;
use std::io::{prelude::*, Error, SeekFrom};
use std::path::{Path, PathBuf};
use std::result::Result;

use bitreader::BitReader;

use byteorder::{BigEndian, ReadBytesExt};

use flate2::read::ZlibDecoder;

use super::nbt;
use super::sizes::*;
use super::types::*;

pub struct Chunk<'a> {
    pub data: &'a ChunkData,
    pub ndata: Edges<&'a ChunkData>,
}

pub struct ChunkData {
    pub blocks: [u16; BLOCKS_IN_CHUNK_3D],
    pub lights: [u8; BLOCKS_IN_CHUNK_3D],
    pub biomes: [u8; BLOCKS_IN_CHUNK_2D],
}

pub struct Region {
    pub chunks: HashMap<Pair<usize>, ChunkData>,
    pub nchunks: Edges<HashMap<Pair<usize>, ChunkData>>,
}

static EMPTY_CHUNK: ChunkData = ChunkData {
    blocks: [0u16; BLOCKS_IN_CHUNK_3D],
    lights: [0u8; BLOCKS_IN_CHUNK_3D],
    biomes: [0u8; BLOCKS_IN_CHUNK_2D],
};

impl Region {
    pub fn get_chunk(&self, c: &Pair<usize>) -> Chunk {
        Chunk {
            data: &self.chunks[c],
            ndata: Edges {
                n: match c.z {
                    0 => self.nchunks.n.get(&Pair { x: c.x, z: MAX_CHUNK_IN_REGION }),
                    _ => self.chunks.get(&Pair { x: c.x, z: c.z - 1 }),
                }.unwrap_or_else(|| &EMPTY_CHUNK),
                e: match c.x {
                    MAX_CHUNK_IN_REGION => self.nchunks.e.get(&Pair { x: 0, z: c.z }),
                    _ => self.chunks.get(&Pair { x: c.x + 1, z: c.z }),
                }.unwrap_or_else(|| &EMPTY_CHUNK),
                s: match c.z {
                    MAX_CHUNK_IN_REGION => self.nchunks.s.get(&Pair { x: c.x, z: 0 }),
                    _ => self.chunks.get(&Pair { x: c.x, z: c.z + 1 }),
                }.unwrap_or_else(|| &EMPTY_CHUNK),
                w: match c.x {
                    0 => self.nchunks.w.get(&Pair { x: MAX_CHUNK_IN_REGION, z: c.z }),
                    _ => self.chunks.get(&Pair { x: c.x - 1, z: c.z }),
                }.unwrap_or_else(|| &EMPTY_CHUNK),
            }
        }
    }
}

fn get_path_from_coords<'a>(worldpath: &Path, r: &Pair<i32>) -> PathBuf {
    worldpath.join("region").join(format!("r.{}.{}.mca", r.x, r.z))
}

fn get_region_chunk_reader(file: &mut File, cx: usize, cz: usize)
-> Result<Option<ZlibDecoder<&mut File>>, Error> {
    let co = (cz * CHUNKS_IN_REGION + cx) * 4;
    file.seek(SeekFrom::Start(co as u64))?;

    let offset = (file.read_u32::<BigEndian>()? >> 8) as usize * SECTOR_SIZE;
    Ok(if offset > 0 {
        file.seek(SeekFrom::Start(offset as u64))?;
        let size = file.read_u32::<BigEndian>()? as usize;
        file.seek(SeekFrom::Current(1))?;

        let mut reader = ZlibDecoder::new_with_buf(file, vec![0u8; size - 1]);
        nbt::read_tag_header(&mut reader)?;
        Some(reader)
    } else {
        None
    })
}

pub fn read_region_chunk<R>(reader: &mut R, blocknames: &[&str])
-> Result<Option<ChunkData>, Error> where R: Read {
    // println!("Reading chunk {}, {}", cx, cz);

    if nbt::seek_compound_tag_name(reader, "Level")?.is_none() {
        return Ok(None);
    }

    let mut chunk = ChunkData {
        blocks: [0u16; BLOCKS_IN_CHUNK_3D],
        lights: [0u8; BLOCKS_IN_CHUNK_3D],
        biomes: [0u8; BLOCKS_IN_CHUNK_2D],
    };

    while let Some(tag_name) = nbt::seek_compound_tag_names(reader, vec!["Sections", "Biomes"])? {
        if tag_name == "Sections" {
            let slen = nbt::read_list_length(reader)?;

            let light_bytes_default = vec![0u8; BLOCKS_IN_SECTION_3D / 2];

            for _ in 0..slen {
                let section = nbt::read_compound_tag_names(reader,
                    vec!["Y", "Palette", "BlockStates", "BlockLight", "SkyLight"])?;
                let y = *section["Y"].to_u8()? as usize;
                if y > MAX_SECTION_IN_CHUNK_Y {
                    continue;
                }
                let so = y * BLOCKS_IN_SECTION_3D;

                // Read blocks.
                if section.contains_key("BlockStates") {
                    let palette = section["Palette"].to_list()?;
                    let states = section["BlockStates"].to_long_array()?;

                    let mut pblocks = Vec::with_capacity(palette.len());
                    for ptag in palette {
                        let pblock = ptag.to_hashmap()?;
                        let name = pblock["Name"].to_str()?;
                        pblocks.push(blocknames.iter().position(|b| b == &name).unwrap() as u16);
                    }

                    // BlockStates is an array of i64 representing 4096 blocks,
                    // but we have to check the array length to determine the # of bits per block.
                    let len = states.len();
                    let mut bytes = vec![0u8; len * 8];
                    for i in 0..len {
                        let long = states[len - i - 1];
                        for b in 0..8 {
                            bytes[i * 8 + b] = (long >> ((7 - b) * 8)) as u8;
                        }
                    }
                    let bits = (len / 64) as u8;

                    let mut br = BitReader::new(&bytes);
                    for i in (0..BLOCKS_IN_SECTION_3D).rev() {
                        chunk.blocks[so + i] = pblocks[br.read_u16(bits).unwrap() as usize];
                    }
                }

                // Read lights.
                if section.contains_key("BlockLight") || section.contains_key("SkyLight") {
                    let bbytes = section.get("BlockLight")
                        .map_or(&light_bytes_default, |tag| tag.to_u8_array().unwrap());
                    let sbytes = section.get("SkyLight")
                        .map_or(&light_bytes_default, |tag| tag.to_u8_array().unwrap());

                    for i in 0..(BLOCKS_IN_SECTION_3D / 2) {
                        // The bottom half of each byte, moving blocklight to the top.
                        chunk.lights[so + i * 2] = ((bbytes[i] & 0x0f) << 4) | (sbytes[i] & 0x0f);
                        // The top half of each byte, moving skylight to the bottom.
                        chunk.lights[so + i * 2 + 1] = (bbytes[i] & 0xf0) | (sbytes[i] >> 4);
                    }
                }
            }
        } else if tag_name == "Biomes" {
            // Read biomes.
            let cbiomes = nbt::read_u8_array(reader)?;
            if cbiomes.len() == BLOCKS_IN_CHUNK_2D {
                chunk.biomes.copy_from_slice(&cbiomes);
            }
        }
    }

    Ok(Some(chunk))
}

fn read_region_chunk_data(path: &Path, margins: &Edges<usize>, blocknames: &[&str])
-> Result<HashMap<Pair<usize>, ChunkData>, Box<Error>> {
    let mut chunks = HashMap::new();

    if path.exists() {
        let mut file = File::open(path)?;

        for cz in margins.n..(CHUNKS_IN_REGION - margins.s) {
            for cx in margins.w..(CHUNKS_IN_REGION - margins.e) {
                if let Some(mut reader) = get_region_chunk_reader(&mut file, cx, cz)? {
                    if let Some(chunk) = read_region_chunk(&mut reader, blocknames)? {
                        chunks.insert(Pair { x: cx, z: cz }, chunk);
                    }
                }
            }
        }
    }

    Ok(chunks)
}

#[allow(dead_code)]
pub fn read_region_data(worldpath: &Path, r: &Pair<i32>, blocknames: &[&str])
-> Result<Option<Region>, Box<Error>> {
    let regionpath = get_path_from_coords(worldpath, &r);
    if !regionpath.exists() {
        return Ok(None);
    }

    let npaths = Edges {
        n: get_path_from_coords(worldpath, &Pair { x: r.x, z: r.z - 1 }),
        s: get_path_from_coords(worldpath, &Pair { x: r.x, z: r.z + 1 }),
        w: get_path_from_coords(worldpath, &Pair { x: r.x - 1, z: r.z }),
        e: get_path_from_coords(worldpath, &Pair { x: r.x + 1, z: r.z }),
    };
    let nmargins = Edges {
        n: Edges { n: MAX_CHUNK_IN_REGION, s: 0, w: 0, e: 0 },
        s: Edges { n: 0, s: MAX_CHUNK_IN_REGION, w: 0, e: 0 },
        w: Edges { n: 0, s: 0, w: MAX_CHUNK_IN_REGION, e: 0 },
        e: Edges { n: 0, s: 0, w: 0, e: MAX_CHUNK_IN_REGION },
    };

    Ok(Some(Region {
        chunks: read_region_chunk_data(&regionpath, &Edges::default(), blocknames)?,
        nchunks: Edges {
            n: read_region_chunk_data(&npaths.n, &nmargins.n, blocknames)?,
            e: read_region_chunk_data(&npaths.e, &nmargins.e, blocknames)?,
            s: read_region_chunk_data(&npaths.s, &nmargins.s, blocknames)?,
            w: read_region_chunk_data(&npaths.w, &nmargins.w, blocknames)?,
        },
    }))
}
