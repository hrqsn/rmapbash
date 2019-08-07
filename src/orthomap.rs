use std::error::Error;
use std::fs::File;
use std::path::Path;

use super::blocktypes::BlockType;
use super::color;
use super::image;
use super::options::Options;
use super::region;
use super::sizes::*;
use super::types::*;
use super::world;


fn get_block_color(bx: usize, bz: usize, blocktypes: &[BlockType], chunk: &region::Chunk)
-> color::RGBA {
    let mut color = color::RGBA { r: 0, g: 0, b: 0, a: 0 };

    let bo2 = bz * BLOCKS_IN_CHUNK + bx;
    let biome = chunk.data.biomes[bo2] as usize;

    for by in (0..BLOCKS_IN_CHUNK_Y).rev() {
        let bo3 = by * BLOCKS_IN_CHUNK_2D + bo2;
        let btype = chunk.data.blocks[bo3];
        let blocktype = &blocktypes[btype as usize];
        if blocktype.empty {
            continue;
        }

        let tblock = chunk.get_t_block(&by, &bo3);
        let nblocks = Edges {
            n: chunk.get_n_block(&bz, &bo3),
            e: chunk.get_e_block(&bx, &bo3),
            s: chunk.get_s_block(&bz, &bo3),
            w: chunk.get_w_block(&bx, &bo3),
        };
        let is_edge = Edges {
            n: nblocks.n.slight > 0 && !blocktypes[nblocks.n.btype as usize].solid,
            s: nblocks.s.slight > 0 && !blocktypes[nblocks.s.btype as usize].solid,
            e: nblocks.e.slight > 0 && !blocktypes[nblocks.e.btype as usize].solid,
            w: nblocks.w.slight > 0 && !blocktypes[nblocks.w.btype as usize].solid,
        };
        let shade = match (is_edge.n || is_edge.w, is_edge.e || is_edge.s) {
            (true, false) => 2,
            (false, true) => 3,
            _ => 1,
        };
        let blockcolor = &blocktype.colors[biome][tblock.slight][tblock.blight][shade];

        color = color::blend_alpha_color(&color, blockcolor);
        if color.a == MAX_CHANNEL_VALUE {
            break;
        }
    }

    color
}

fn draw_chunk(pixels: &mut [u8], blocktypes: &[BlockType], chunk: &region::Chunk, co: &isize,
    cblimits: &Edges<usize>, width: &usize) {
    for bz in cblimits.n..(cblimits.s + 1) {
        for bx in cblimits.w..(cblimits.e + 1) {
            let po = (co + (bz * width + bx) as isize) as usize * 4;
            let color = get_block_color(bx, bz, blocktypes, chunk);
            pixels[po] = color.r;
            pixels[po + 1] = color.g;
            pixels[po + 2] = color.b;
            pixels[po + 3] = color.a;
        }
    }
}

pub fn draw_ortho_map(options: &Options, outpath: &Path, blocktypes: &[BlockType])
-> Result<(), Box<Error>> {
    println!("Creating block map from world dir {}", options.inpath.display());

    let world = world::get_world(options.inpath, &options.blimits)?;
    let size = world.bedges.size();
    let cbcrop = match options.blimits {
        Some(blimits) => Pair {
            x: block_pos_in_chunk(blimits.w, None),
            z: block_pos_in_chunk(blimits.n, None),
        },
        None => Pair { x: 0, z: 0 },
    };
    let crop = cbcrop.z * size.x + cbcrop.x;
    let mut pixels = vec![0u8; size.x * size.z * 4];

    let mut i = 0;
    let len = world.regions.len();

    for rz in (world.redges.n..world.redges.s + 1).rev() {
        for rx in (world.redges.w..world.redges.e + 1).rev() {
            let r = &Pair { x: rx, z: rz };
            if !world.regions.contains_key(&r) {
                continue;
            }

            i += 1;
            println!("Reading block data for region {}, {} ({}/{})", r.x, r.z, i, len);
            if let Some(reg) = region::read_region_data(
                options.inpath, r, blocktypes, &options.blimits)? {
                let chunk_count = reg.chunks.len();
                println!("Drawing block map for region {}, {} ({} chunk{})", r.x, r.z,
                    chunk_count, if chunk_count == 1 { "" } else { "s" });

                let arc = Pair {
                    x: r.x * CHUNKS_IN_REGION as isize - world.cedges.w,
                    z: r.z * CHUNKS_IN_REGION as isize - world.cedges.n,
                };

                for cz in (0..CHUNKS_IN_REGION).rev() {
                    for cx in (0..CHUNKS_IN_REGION).rev() {
                        let c = &Pair { x: cx, z: cz };
                        if let Some(chunk) = reg.get_chunk(c) {
                            // println!("Drawing chunk {}, {}", c.x, c.z);
                            let wc = Pair {
                                x: r.x * CHUNKS_IN_REGION as isize + c.x as isize,
                                z: r.z * CHUNKS_IN_REGION as isize + c.z as isize,
                            };
                            let cblimits = match options.blimits {
                                Some(blimits) => Edges {
                                    n: block_pos_in_chunk(blimits.n, Some(wc.z)),
                                    e: block_pos_in_chunk(blimits.e, Some(wc.x)),
                                    s: block_pos_in_chunk(blimits.s, Some(wc.z)),
                                    w: block_pos_in_chunk(blimits.w, Some(wc.x)),
                                },
                                None => Edges::<usize>::full(BLOCKS_IN_CHUNK),
                            };

                            let ac = Pair {
                                x: (arc.x + c.x as isize) as usize,
                                z: (arc.z + c.z as isize) as usize,
                            };
                            let co = ((ac.z * size.x + ac.x) * BLOCKS_IN_CHUNK) as isize -
                                crop as isize;

                            draw_chunk(&mut pixels, blocktypes, &chunk, &co, &cblimits, &size.x);
                        }
                    }
                }
            } else {
                println!("No data in region.");
            }
        }
    }

    let file = File::create(outpath)?;
    image::draw_block_map(&pixels, size, file, true)?;

    Ok(())
}
