/// Vajra ELF64 Linker — Pure Rust Linux executable emitter
/// Produces a statically-linked ELF64 executable.
/// Uses direct Linux syscalls — zero libc dependency.
/// Uses own ELF64 object parser — zero external crate dependency.

use anyhow::Result;
use crate::assembler::elf_writer::parse_elf;

const PT_LOAD: u32 = 1;
#[allow(dead_code)]
const PT_NULL: u32 = 0;
const PF_X: u32 = 0x1;
#[allow(dead_code)]
const PF_W: u32 = 0x2;
const PF_R: u32 = 0x4;

const LOAD_ADDR: u64 = 0x0000_0000_0040_0000; // 4 MB
const PAGE_SIZE: u64 = 0x1000;

fn align_up(val: usize, align: usize) -> usize {
    (val + align - 1) & !(align - 1)
}

fn write_u16(buf: &mut Vec<u8>, v: u16) { buf.extend_from_slice(&v.to_le_bytes()); }
fn write_u32(buf: &mut Vec<u8>, v: u32) { buf.extend_from_slice(&v.to_le_bytes()); }
fn write_u64(buf: &mut Vec<u8>, v: u64) { buf.extend_from_slice(&v.to_le_bytes()); }

pub fn link(obj_bytes: &[u8], runtime_bytes: &[u8], _entry_point: &str) -> Result<Vec<u8>> {
    // Extract .text and .rodata from both object files using our own ELF parser
    let mut text_data: Vec<u8> = Vec::new();
    let mut rodata_data: Vec<u8> = Vec::new();

    for (bytes, file_id) in [(obj_bytes, 0usize), (runtime_bytes, 1usize)] {
        if bytes.is_empty() { continue; }

        // Try our own ELF64 parser first
        if bytes.len() >= 4 && &bytes[0..4] == b"\x7FELF" {
            if let Ok((sections, _syms, _relas)) = parse_elf(bytes, file_id) {
                for section in &sections {
                    match section.name.as_str() {
                        ".text" => text_data.extend_from_slice(&section.data),
                        ".rodata" | ".rdata" => rodata_data.extend_from_slice(&section.data),
                        _ => {}
                    }
                }
                continue;
            }
        }

        // Fallback: raw bytes assumed to be .text
        text_data.extend_from_slice(bytes);
    }

    // Layout:
    //   ELF header (64 bytes)
    //   Program headers (2 entries × 56 bytes = 112 bytes)
    //   .text (execute+read)
    //   .rodata (read)

    let elf_hdr_size = 64usize;
    let phdr_size = 56usize;
    let num_phdrs = 2usize;
    let headers_size = elf_hdr_size + phdr_size * num_phdrs;

    let text_offset = align_up(headers_size, PAGE_SIZE as usize);
    let text_size = align_up(text_data.len(), PAGE_SIZE as usize);

    let rodata_offset = text_offset + text_size;
    let rodata_size = align_up(rodata_data.len().max(1), PAGE_SIZE as usize);

    let text_vaddr = LOAD_ADDR + text_offset as u64;
    let rodata_vaddr = LOAD_ADDR + rodata_offset as u64;
    let entry_vaddr = text_vaddr; // entry = first byte of .text

    // Build ELF header
    let mut out = Vec::new();

    // e_ident
    out.extend_from_slice(&[
        0x7F, b'E', b'L', b'F', // magic
        2,    // EI_CLASS = ELFCLASS64
        1,    // EI_DATA = ELFDATA2LSB (little-endian)
        1,    // EI_VERSION = current
        0,    // EI_OSABI = ELFOSABI_NONE (System V)
        0, 0, 0, 0, 0, 0, 0, 0, // EI_ABIVERSION + padding
    ]);

    write_u16(&mut out, 2);    // e_type = ET_EXEC
    write_u16(&mut out, 0x3E); // e_machine = EM_X86_64
    write_u32(&mut out, 1);    // e_version = EV_CURRENT
    write_u64(&mut out, entry_vaddr); // e_entry
    write_u64(&mut out, elf_hdr_size as u64); // e_phoff (program headers right after elf header)
    write_u64(&mut out, 0);    // e_shoff (no section headers)
    write_u32(&mut out, 0);    // e_flags
    write_u16(&mut out, elf_hdr_size as u16); // e_ehsize
    write_u16(&mut out, phdr_size as u16);    // e_phentsize
    write_u16(&mut out, num_phdrs as u16);    // e_phnum
    write_u16(&mut out, 64);   // e_shentsize
    write_u16(&mut out, 0);    // e_shnum
    write_u16(&mut out, 0);    // e_shstrndx

    assert_eq!(out.len(), elf_hdr_size);

    // Program header 1: LOAD .text (RX)
    write_u32(&mut out, PT_LOAD);
    write_u32(&mut out, PF_R | PF_X);     // p_flags
    write_u64(&mut out, text_offset as u64); // p_offset
    write_u64(&mut out, text_vaddr);          // p_vaddr
    write_u64(&mut out, text_vaddr);          // p_paddr
    write_u64(&mut out, text_data.len() as u64); // p_filesz
    write_u64(&mut out, text_size as u64);       // p_memsz
    write_u64(&mut out, PAGE_SIZE);              // p_align

    // Program header 2: LOAD .rodata (R)
    write_u32(&mut out, PT_LOAD);
    write_u32(&mut out, PF_R);
    write_u64(&mut out, rodata_offset as u64);
    write_u64(&mut out, rodata_vaddr);
    write_u64(&mut out, rodata_vaddr);
    write_u64(&mut out, rodata_data.len() as u64);
    write_u64(&mut out, rodata_size as u64);
    write_u64(&mut out, PAGE_SIZE);

    assert_eq!(out.len(), elf_hdr_size + phdr_size * num_phdrs);

    // Pad to text_offset
    while out.len() < text_offset {
        out.push(0);
    }

    // Write .text
    out.extend_from_slice(&text_data);
    while out.len() < text_offset + text_size {
        out.push(0);
    }

    // Write .rodata
    out.extend_from_slice(&rodata_data);
    while out.len() < rodata_offset + rodata_size {
        out.push(0);
    }

    Ok(out)
}
