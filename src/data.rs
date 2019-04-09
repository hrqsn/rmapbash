use std::fs::File;
use std::io::{prelude::*, Error, ErrorKind};
use std::path::Path;
use std::result::Result;

use flate2::read::GzDecoder;

use nbt::Blob;

use regex::Regex;

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
    let mut f = File::open(path)?;
    let mut buf = [0; 4];
    let mut chunks = [false; 1024];

    for p in 0..1024 {
        f.read(&mut buf)?;
        let val = ((buf[0] as u32) << 24) | ((buf[1] as u32) << 16) |
            ((buf[2] as u32) << 8) | buf[3] as u32;
        if val > 0 {
            chunks[p] = true;
        }
    }

    Ok(chunks)
}

pub fn read_dat_file(path: &Path) -> Result<(), Error> {
    let file = File::open(path)?;
    let mut level_reader = GzDecoder::new(file);

    println!("================================= NBT Contents =================================");
    let blob = match Blob::from_reader(&mut level_reader) {
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
