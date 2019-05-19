use std::error::Error;
use std::fs::File;
use std::path::Path;

use super::blocktypes::BlockType;
use super::color;
use super::color::RGBA;
use super::image;
use super::region;
use super::sizes::*;
use super::types::*;
use super::world;

fn get_iso_size(csize: &Pair<usize>) -> Pair<usize> {
    Pair {
        x: (csize.x + csize.z) * ISO_CHUNK_X_MARGIN,
        z: (csize.x + csize.z) * ISO_CHUNK_Y_MARGIN + ISO_CHUNK_SIDE_HEIGHT,
    }
}

fn draw_chunk(pixels: &mut [u8], blocktypes: &[BlockType], chunk: &region::Chunk,
    co: &usize, width: &usize) {
    for bz in (0..BLOCKS_IN_CHUNK).rev() {
        for bx in (0..BLOCKS_IN_CHUNK).rev() {
            let bo2 = bz * BLOCKS_IN_CHUNK + bx;

            let bpx = (ISO_CHUNK_X_MARGIN as i16 +
                (bx as i16 - bz as i16 - 1) * ISO_BLOCK_X_MARGIN as i16) as usize;
            let bpy2 = (bx + bz) * ISO_BLOCK_Y_MARGIN;

            let biome = chunk.data.biomes[bo2] as usize;

            for by in (0..BLOCKS_IN_CHUNK_Y).rev() {
                let bo3 = by * BLOCKS_IN_CHUNK_2D + bo2;
                let block = chunk.data.blocks[bo3];
                let blocktype = &blocktypes[block as usize];
                if blocktype.empty {
                    continue;
                }

                let tlight = match by {
                    MAX_BLOCK_IN_CHUNK_Y => MAX_LIGHT_LEVEL,
                    _ => chunk.data.lights[bo3 + BLOCKS_IN_CHUNK_2D]
                };
                let tslight = (tlight & 0x0f) as usize;
                let tblight = ((tlight & 0xf0) >> 4) as usize;
                let tcolor = &blocktype.colors[biome][tslight][tblight][1];
                let tcolor2 = &blocktype.colors[biome][tslight][tblight][4];

                // TODO: once we implement shapes, add hilight/shadow when the adjacent block
                // has skylight and isn't a solid shape (i.e., has gaps).

                let lblock = match bz {
                    MAX_BLOCK_IN_CHUNK => chunk.ndata.s.blocks[bo3 - MAX_BLOCK_IN_CHUNK * BLOCKS_IN_CHUNK],
                    _ => chunk.data.blocks[bo3 + BLOCKS_IN_CHUNK],
                };
                let llight = match bz {
                    MAX_BLOCK_IN_CHUNK => chunk.ndata.s.lights[bo3 - MAX_BLOCK_IN_CHUNK * BLOCKS_IN_CHUNK],
                    _ => chunk.data.lights[bo3 + BLOCKS_IN_CHUNK],
                };
                let lslight = (llight & 0x0f) as usize;
                let lblight = ((llight & 0xf0) >> 4) as usize;
                // Add a hilight if block to the left has skylight and is not the same as this one.
                let lshade = if lslight > 0 && lblock != block { 2 } else { 1 };
                let lcolor = &blocktype.colors[biome][lslight][lblight][lshade];
                let lcolor2 = &blocktype.colors[biome][lslight][lblight][lshade + 3];

                let rblock = match bx {
                    MAX_BLOCK_IN_CHUNK => chunk.ndata.e.blocks[bo3 - MAX_BLOCK_IN_CHUNK],
                    _ => chunk.data.blocks[bo3 + 1],
                };
                let rlight = match bx {
                    MAX_BLOCK_IN_CHUNK => chunk.ndata.e.lights[bo3 - MAX_BLOCK_IN_CHUNK],
                    _ => chunk.data.lights[bo3 + 1],
                };
                let rslight = (rlight & 0x0f) as usize;
                let rblight = ((rlight & 0xf0) >> 4) as usize;
                // Add a shadow if block to the right has skylight and is not the same as this one.
                let rshade = if rslight > 0 && rblock != block { 3 } else { 1 };
                let rcolor = &blocktype.colors[biome][rslight][rblight][rshade];
                let rcolor2 = &blocktype.colors[biome][rslight][rblight][rshade + 3];

                // Create an index of colors corresponding to the digits in the block shape.
                let bcolors = [&RGBA::default(),
                    &tcolor, &lcolor, &rcolor, &tcolor2, &lcolor2, &rcolor2];

                let bpy = bpy2 + (MAX_BLOCK_IN_CHUNK_Y - by) * ISO_BLOCK_SIDE_HEIGHT;

                // Don't draw the top if the block above is the same as this one.
                // This prevents stripes appearing in columns of translucent blocks.
                let skip_top = by < MAX_BLOCK_IN_CHUNK_Y &&
                    chunk.data.blocks[bo3] == chunk.data.blocks[bo3 + BLOCKS_IN_CHUNK_2D];

                for y in (if skip_top { ISO_BLOCK_Y_MARGIN } else { 0 })..ISO_BLOCK_HEIGHT {
                    for x in 0..ISO_BLOCK_WIDTH {
                        let po = (co + (bpy + y) * width + bpx + x) * 4;
                        if pixels[po + 3] == MAX_CHANNEL_VALUE {
                            continue;
                        }

                        let pcolor = color::blend_alpha_color(&RGBA {
                            r: pixels[po],
                            g: pixels[po + 1],
                            b: pixels[po + 2],
                            a: pixels[po + 3],
                        }, bcolors[blocktype.shape[x][y]]);
                        pixels[po] = pcolor.r;
                        pixels[po + 1] = pcolor.g;
                        pixels[po + 2] = pcolor.b;
                        pixels[po + 3] = pcolor.a;
                    }
                }
            }
        }
    }
}

#[allow(dead_code)]
pub fn draw_world_iso_map(worldpath: &Path, outpath: &Path, blocktypes: &[BlockType])
-> Result<(), Box<Error>> {
    println!("Creating block map from world dir {}", worldpath.display());

    let world = world::get_world(worldpath)?;

    let csize = world.get_chunk_size();
    let size = get_iso_size(&csize);
    let mut pixels = vec![0u8; size.x * size.z * 4];

    let blocknames: Vec<&str> = blocktypes.iter().map(|b| &b.name[..]).collect();

    let mut i = 0;
    let len = world.regions.len();

    for rz in (world.rlimits.n..world.rlimits.s + 1).rev() {
        for rx in (world.rlimits.w..world.rlimits.e + 1).rev() {
            let r = Pair { x: rx, z: rz };
            if !world.regions.contains(&r) {
                continue;
            }

            i += 1;
            println!("Reading block data for region {}, {} ({}/{})", r.x, r.z, i, len);
            if let Some(reg) = region::read_region_data(worldpath, &r, &blocknames)? {
                println!("Drawing block map for region {}, {}", r.x, r.z);
                let arx = (r.x - world.rlimits.w) as usize;
                let arz = (r.z - world.rlimits.n) as usize;

                for cz in (0..CHUNKS_IN_REGION).rev() {
                    for cx in (0..CHUNKS_IN_REGION).rev() {
                        let c = &Pair { x: cx, z: cz };
                        if !reg.chunks.contains_key(c) {
                            continue;
                        }

                        // println!("Drawing chunk {}, {}", c.x, c.z);
                        let acx = arx * CHUNKS_IN_REGION + c.x - world.margins.w;
                        let acz = arz * CHUNKS_IN_REGION + c.z - world.margins.n;

                        let cpx = (acx + csize.z - acz - 1) * ISO_CHUNK_X_MARGIN;
                        let cpy = (acx + acz) * ISO_CHUNK_Y_MARGIN;
                        let co = cpy * size.x + cpx;

                        draw_chunk(&mut pixels, &blocktypes, &reg.get_chunk(&c), &co, &size.x);
                    }
                }
            }
        }
    }

    let file = File::create(outpath)?;
    image::draw_block_map(&pixels, size, file, true)?;

    Ok(())
}

#[allow(dead_code)]
pub fn draw_region_iso_map(worldpath: &Path, r: &Pair<i32>, outpath: &Path, blocktypes: &[BlockType])
-> Result<(), Box<Error>> {
    println!("Reading block data for region {}, {}", r.x, r.z);
    let blocknames: Vec<&str> = blocktypes.iter().map(|b| &b.name[..]).collect();
    if let Some(reg) = region::read_region_data(worldpath, &r, &blocknames)? {
        println!("Drawing block map");
        let climits = Edges {
            n: reg.chunks.keys().map(|c| c.z).min().unwrap(),
            e: reg.chunks.keys().map(|c| c.x).max().unwrap(),
            s: reg.chunks.keys().map(|c| c.z).max().unwrap(),
            w: reg.chunks.keys().map(|c| c.x).min().unwrap(),
        };
        let csize = Pair {
            x: climits.e - climits.w + 1,
            z: climits.s - climits.n + 1,
        };
        let size = get_iso_size(&csize);

        let mut pixels = vec![0u8; size.x * size.z * 4];

        for cz in (0..CHUNKS_IN_REGION).rev() {
            for cx in (0..CHUNKS_IN_REGION).rev() {
                let c = &Pair { x: cx, z: cz };
                if !reg.chunks.contains_key(c) {
                    continue;
                }

                // println!("Drawing chunk {}, {}", c.x, c.z);
                let acx = c.x - climits.w;
                let acz = c.z - climits.n;

                let cpx = (acx + csize.z - acz - 1) * ISO_CHUNK_X_MARGIN;
                let cpy = (acx + acz) * ISO_CHUNK_Y_MARGIN;
                let co = cpy * size.x + cpx;

                draw_chunk(&mut pixels, &blocktypes, &reg.get_chunk(&c), &co, &size.x);
            }
        }

        let file = File::create(outpath)?;
        image::draw_block_map(&pixels, size, file, true)?;

        return Ok(())
    }

    println!("No data in region.");
    Ok(())
}
