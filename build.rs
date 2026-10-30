//! Embeds assets/freenapse.ico as the executables' icon.
//!
//! The GNU toolchain used here has no `windres`, so the resource object is
//! written by hand: a COFF object with one `.rsrc` section holding the
//! RT_ICON images and the RT_GROUP_ICON directory, plus the ADDR32NB
//! relocations the linker needs to turn data offsets into RVAs.

#![allow(clippy::collapsible_if)]

use std::env;
use std::fs;
use std::path::Path;

const RT_ICON: u32 = 3;
const RT_GROUP_ICON: u32 = 14;
const LANG_NEUTRAL: u32 = 0x0409;

/// (width, height, bit count, image bytes) of one entry of the .ico file.
type IcoImage = (u8, u8, u16, Vec<u8>);

fn main() {
    let ico = Path::new("assets/freenapse.ico");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={}", ico.display());
    let Ok(data) = fs::read(ico) else { return };
    let Some(images) = parse_ico(&data) else { return };
    let obj = rsrc_object(&images);
    let out = Path::new(&env::var("OUT_DIR").unwrap()).join("freenapse-icon.o");
    fs::write(&out, obj).unwrap();
    println!("cargo:rustc-link-arg-bins={}", out.display());
}

/// (width, height, bit count, image bytes) for each entry of the .ico file.
fn parse_ico(d: &[u8]) -> Option<Vec<IcoImage>> {
    let u16le = |o: usize| u16::from_le_bytes([d[o], d[o + 1]]);
    let u32le = |o: usize| u32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]]);
    if d.len() < 6 || u16le(0) != 0 || u16le(2) != 1 {
        return None;
    }
    let n = u16le(4) as usize;
    let mut out = Vec::new();
    for i in 0..n {
        let e = 6 + 16 * i;
        let (w, h, bits) = (d[e], d[e + 1], u16le(e + 6));
        let (size, off) = (u32le(e + 8) as usize, u32le(e + 12) as usize);
        out.push((w, h, bits, d.get(off..off + size)?.to_vec()));
    }
    Some(out)
}

struct Dir {
    entries: Vec<(u32, Node)>,
}

enum Node {
    Dir(Dir),
    Data(Vec<u8>),
}

/// Lay the resource tree out and produce the COFF object.
fn rsrc_object(images: &[IcoImage]) -> Vec<u8> {
    // GRPICONDIR: the icon group pointing at RT_ICON ids 1..n.
    let mut group = Vec::new();
    group.extend_from_slice(&0u16.to_le_bytes());
    group.extend_from_slice(&1u16.to_le_bytes());
    group.extend_from_slice(&(images.len() as u16).to_le_bytes());
    for (i, (w, h, bits, bytes)) in images.iter().enumerate() {
        group.extend_from_slice(&[*w, *h, 0, 0]);
        group.extend_from_slice(&1u16.to_le_bytes());
        group.extend_from_slice(&bits.to_le_bytes());
        group.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        group.extend_from_slice(&((i + 1) as u16).to_le_bytes());
    }
    let icons = Dir {
        entries: images
            .iter()
            .enumerate()
            .map(|(i, im)| ((i + 1) as u32, Node::Dir(Dir { entries: vec![(LANG_NEUTRAL, Node::Data(im.3.clone()))] })))
            .collect(),
    };
    let groups = Dir { entries: vec![(1, Node::Dir(Dir { entries: vec![(LANG_NEUTRAL, Node::Data(group))] }))] };
    let root = Dir { entries: vec![(RT_ICON, Node::Dir(icons)), (RT_GROUP_ICON, Node::Dir(groups))] };

    // Pass 1: sizes. Directories first, then data entries, then the blobs.
    let mut dirs: Vec<&Dir> = Vec::new();
    collect(&root, &mut dirs);
    let dir_bytes: usize = dirs.iter().map(|d| 16 + 8 * d.entries.len()).sum();
    let n_data = count_data(&root);
    let data_entries_at = dir_bytes;
    let blobs_at = data_entries_at + 16 * n_data;

    // Pass 2: emit. Each directory gets its offset in order of `dirs`.
    let mut dir_offsets = Vec::new();
    let mut o = 0;
    for d in &dirs {
        dir_offsets.push(o);
        o += 16 + 8 * d.entries.len();
    }
    let mut section = vec![0u8; dir_bytes];
    let mut data_entries = Vec::new();
    let mut blobs = Vec::new();
    let mut relocs = Vec::new(); // offsets of OffsetToData fields
    let mut dir_i = 0;
    let mut data_i = 0;
    emit(&root, &dirs, &dir_offsets, &mut dir_i, &mut data_i, &mut section, &mut data_entries, &mut blobs, &mut relocs, data_entries_at, blobs_at);
    section.extend_from_slice(&data_entries);
    section.extend_from_slice(&blobs);
    while !section.len().is_multiple_of(4) {
        section.push(0);
    }

    // COFF object: header, one section, raw data, relocations, one symbol, empty string table.
    let raw_at = 20 + 40;
    let reloc_at = raw_at + section.len();
    let sym_at = reloc_at + 10 * relocs.len();
    let mut obj = Vec::new();
    obj.extend_from_slice(&0x8664u16.to_le_bytes()); // AMD64
    obj.extend_from_slice(&1u16.to_le_bytes()); // sections
    obj.extend_from_slice(&0u32.to_le_bytes()); // timestamp
    obj.extend_from_slice(&(sym_at as u32).to_le_bytes());
    obj.extend_from_slice(&1u32.to_le_bytes()); // symbols
    obj.extend_from_slice(&0u16.to_le_bytes()); // optional header
    obj.extend_from_slice(&0u16.to_le_bytes()); // characteristics
    obj.extend_from_slice(b".rsrc\0\0\0");
    obj.extend_from_slice(&0u32.to_le_bytes()); // virtual size
    obj.extend_from_slice(&0u32.to_le_bytes()); // virtual address
    obj.extend_from_slice(&(section.len() as u32).to_le_bytes());
    obj.extend_from_slice(&(raw_at as u32).to_le_bytes());
    obj.extend_from_slice(&(reloc_at as u32).to_le_bytes());
    obj.extend_from_slice(&0u32.to_le_bytes()); // line numbers
    obj.extend_from_slice(&(relocs.len() as u16).to_le_bytes());
    obj.extend_from_slice(&0u16.to_le_bytes());
    obj.extend_from_slice(&0xC000_0040u32.to_le_bytes()); // initialized data, read, write
    obj.extend_from_slice(&section);
    for r in &relocs {
        obj.extend_from_slice(&(*r as u32).to_le_bytes());
        obj.extend_from_slice(&0u32.to_le_bytes()); // symbol 0: the section
        obj.extend_from_slice(&3u16.to_le_bytes()); // IMAGE_REL_AMD64_ADDR32NB
    }
    obj.extend_from_slice(b".rsrc\0\0\0");
    obj.extend_from_slice(&0u32.to_le_bytes()); // value
    obj.extend_from_slice(&1u16.to_le_bytes()); // section number
    obj.extend_from_slice(&0u16.to_le_bytes()); // type
    obj.push(3); // IMAGE_SYM_CLASS_STATIC
    obj.push(0); // aux symbols
    obj.extend_from_slice(&4u32.to_le_bytes()); // string table: just its size
    obj
}

fn collect<'a>(d: &'a Dir, out: &mut Vec<&'a Dir>) {
    out.push(d);
    for (_, n) in &d.entries {
        if let Node::Dir(sub) = n {
            collect(sub, out);
        }
    }
}

fn count_data(d: &Dir) -> usize {
    d.entries
        .iter()
        .map(|(_, n)| match n {
            Node::Dir(s) => count_data(s),
            Node::Data(_) => 1,
        })
        .sum()
}

#[allow(clippy::too_many_arguments)]
fn emit(
    d: &Dir,
    dirs: &[&Dir],
    dir_offsets: &[usize],
    dir_i: &mut usize,
    data_i: &mut usize,
    section: &mut [u8],
    data_entries: &mut Vec<u8>,
    blobs: &mut Vec<u8>,
    relocs: &mut Vec<usize>,
    data_entries_at: usize,
    blobs_at: usize,
) {
    let me = *dir_i;
    *dir_i += 1;
    let base = dir_offsets[me];
    let mut entries: Vec<&(u32, Node)> = d.entries.iter().collect();
    entries.sort_by_key(|(id, _)| *id);
    // IMAGE_RESOURCE_DIRECTORY: characteristics, timestamp, version, named entries, id entries.
    section[base + 12..base + 14].copy_from_slice(&0u16.to_le_bytes());
    section[base + 14..base + 16].copy_from_slice(&(entries.len() as u16).to_le_bytes());
    for (k, (id, node)) in entries.iter().enumerate() {
        let e = base + 16 + 8 * k;
        section[e..e + 4].copy_from_slice(&id.to_le_bytes());
        match node {
            Node::Dir(_) => {
                // The next directory emitted is this child (pre-order matches `collect`).
                let child = dir_offsets[*dir_i] as u32 | 0x8000_0000;
                section[e + 4..e + 8].copy_from_slice(&child.to_le_bytes());
                let _ = dirs;
                if let Node::Dir(sub) = node {
                    emit(sub, dirs, dir_offsets, dir_i, data_i, section, data_entries, blobs, relocs, data_entries_at, blobs_at);
                }
            }
            Node::Data(bytes) => {
                let entry_at = data_entries_at + 16 * *data_i;
                *data_i += 1;
                section[e + 4..e + 8].copy_from_slice(&(entry_at as u32).to_le_bytes());
                let blob_at = blobs_at + blobs.len();
                data_entries.extend_from_slice(&(blob_at as u32).to_le_bytes()); // RVA after relocation
                data_entries.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
                data_entries.extend_from_slice(&0u32.to_le_bytes());
                data_entries.extend_from_slice(&0u32.to_le_bytes());
                relocs.push(entry_at);
                blobs.extend_from_slice(bytes);
                while !blobs.len().is_multiple_of(8) {
                    blobs.push(0);
                }
            }
        }
    }
}
