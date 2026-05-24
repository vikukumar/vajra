/// Vajra PE32+ Linker — Pure Rust Windows EXE emitter
/// Produces a complete, runnable PE32+ executable from COFF object files.
/// Only imports kernel32.dll (ExitProcess, WriteFile, GetStdHandle, CreateThread, VirtualAlloc).
/// NO MSVC link.exe required.

use anyhow::Result;
use std::collections::HashMap;
use object::{Object, ObjectSection, ObjectSymbol};

/// Image base for the PE (standard 64-bit default)
const IMAGE_BASE: u64 = 0x0000_0001_4000_0000;
/// Section alignment in virtual memory (must be >= file alignment)
const SECTION_ALIGN: u32 = 0x1000; // 4 KB
/// File alignment for sections
const FILE_ALIGN: u32 = 0x200; // 512 bytes

/// Known kernel32.dll imports Vajra needs
static KERNEL32_IMPORTS: &[&str] = &[
    "ExitProcess",
    "WriteFile",
    "ReadFile",
    "GetStdHandle",
    "CreateThread",
    "WaitForSingleObject",
    "VirtualAlloc",
    "VirtualFree",
    "GetSystemInfo",
    "SetConsoleOutputCP",
    "GetConsoleOutputCP",
    "CreateFileW",
    "CloseHandle",
    "GetLastError",
    "LoadLibraryA",
    "GetProcAddress",
    "GetCommandLineA",
];

fn align_up(val: u32, align: u32) -> u32 {
    (val + align - 1) & !(align - 1)
}

fn align_offset(offset: usize, align: usize) -> usize {
    (offset + align - 1) & !(align - 1)
}

/// Write a u16 little-endian
fn write_u16(buf: &mut Vec<u8>, v: u16) {
    buf.extend_from_slice(&v.to_le_bytes());
}
/// Write a u32 little-endian
fn write_u32(buf: &mut Vec<u8>, v: u32) {
    buf.extend_from_slice(&v.to_le_bytes());
}
/// Write a u64 little-endian
fn write_u64(buf: &mut Vec<u8>, v: u64) {
    buf.extend_from_slice(&v.to_le_bytes());
}

struct SectionChunk {
    file_id: usize, // 0 = user, 1 = runtime
    section_index: object::SectionIndex,
    name: String,
    data: Vec<u8>,
    start_offset: usize, // within the merged section
}

struct ImportTableLayout {
    import_dir_rva_in_rdata: u32,
    import_dir_size: u32,
    iat_rva_in_rdata: u32,
    iat_size: u32,
}

fn build_import_table_bytes(
    rdata_rva: u32,
    start_offset: u32,
) -> (Vec<u8>, ImportTableLayout, HashMap<String, u32>) {
    let mut import_data = Vec::new();
    let mut iat_slot_rvas = HashMap::new();

    // 1. IAT
    let iat_offset_in_import = import_data.len() as u32;
    for &name in KERNEL32_IMPORTS {
        let slot_rva = rdata_rva + start_offset + iat_offset_in_import + (iat_slot_rvas.len() as u32 * 8);
        iat_slot_rvas.insert(name.to_string(), slot_rva);
        write_u64(&mut import_data, 0); // Placeholder
    }
    write_u64(&mut import_data, 0); // null terminator

    // 2. Hint/Name table
    let mut hint_name_offsets = Vec::new();
    for &name in KERNEL32_IMPORTS {
        hint_name_offsets.push(import_data.len() as u32);
        write_u16(&mut import_data, 0); // Hint
        import_data.extend_from_slice(name.as_bytes());
        import_data.push(0); // null terminator
        if import_data.len() % 2 != 0 {
            import_data.push(0); // pad
        }
    }

    // 3. INT (Import Name Table)
    let int_offset_in_import = import_data.len() as u32;
    for &offset in &hint_name_offsets {
        let hint_name_rva = rdata_rva + start_offset + offset;
        write_u64(&mut import_data, hint_name_rva as u64);
    }
    write_u64(&mut import_data, 0); // null terminator

    // Now, patch the IAT to also point to the Hint/Name entries initially
    for (i, &offset) in hint_name_offsets.iter().enumerate() {
        let hint_name_rva = rdata_rva + start_offset + offset;
        let slot_offset = iat_offset_in_import as usize + i * 8;
        import_data[slot_offset..slot_offset + 8].copy_from_slice(&(hint_name_rva as u64).to_le_bytes());
    }

    // 4. DLL Name
    let dll_name_offset = import_data.len() as u32;
    import_data.extend_from_slice(b"KERNEL32.DLL\0");
    if import_data.len() % 2 != 0 {
        import_data.push(0);
    }

    // 5. Import Directory Entry
    let import_dir_offset = import_data.len() as u32;
    write_u32(&mut import_data, rdata_rva + start_offset + int_offset_in_import); // INT RVA
    write_u32(&mut import_data, 0); // TimeDateStamp
    write_u32(&mut import_data, 0); // ForwarderChain
    write_u32(&mut import_data, rdata_rva + start_offset + dll_name_offset); // DLL Name RVA
    write_u32(&mut import_data, rdata_rva + start_offset + iat_offset_in_import); // IAT RVA

    // Null descriptor
    for _ in 0..5 {
        write_u32(&mut import_data, 0);
    }

    let import_dir_size = import_data.len() as u32 - import_dir_offset;

    (
        import_data,
        ImportTableLayout {
            import_dir_rva_in_rdata: start_offset + import_dir_offset,
            import_dir_size,
            iat_rva_in_rdata: start_offset + iat_offset_in_import,
            iat_size: (KERNEL32_IMPORTS.len() + 1) as u32 * 8,
        },
        iat_slot_rvas,
    )
}

fn build_dos_stub() -> Vec<u8> {
    let stub = vec![
        0x4D, 0x5A, 0x90, 0x00, 0x03, 0x00, 0x00, 0x00,
        0x04, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0x00, 0x00,
        0xB8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x40, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x40, 0x00, 0x00, 0x00,
    ];
    stub
}

pub fn link(obj_bytes: &[u8], runtime_bytes: &[u8], entry_point: &str) -> Result<Vec<u8>> {
    let user_obj = object::read::File::parse(obj_bytes)?;
    let rt_obj = object::read::File::parse(runtime_bytes)?;

    let mut text_chunks = Vec::new();
    let mut rdata_chunks = Vec::new();
    let mut data_chunks = Vec::new();

    let mut text_len = 0;
    let mut rdata_len = 0;
    let mut data_len = 0;

    for (obj, file_id) in [(&user_obj, 0), (&rt_obj, 1)] {
        for sec in obj.sections() {
            let name = sec.name().unwrap_or("").to_string();
            let mut data = sec.data().unwrap_or(&[]).to_vec();
            let sec_idx = sec.index();

            if name == ".text" || name == "text" || sec.kind() == object::SectionKind::Text {
                let aligned = align_offset(text_len, 16);
                text_chunks.push(SectionChunk {
                    file_id,
                    section_index: sec_idx,
                    name,
                    data,
                    start_offset: aligned,
                });
                text_len = aligned + sec.size() as usize;
            } else if name == ".rdata" || name == "rdata" || name == ".rodata" || sec.kind() == object::SectionKind::ReadOnlyData {
                let aligned = align_offset(rdata_len, 16);
                rdata_chunks.push(SectionChunk {
                    file_id,
                    section_index: sec_idx,
                    name,
                    data,
                    start_offset: aligned,
                });
                rdata_len = aligned + sec.size() as usize;
            } else if name == ".data" || name == "data" || name == ".bss" || sec.kind() == object::SectionKind::Data || sec.kind() == object::SectionKind::UninitializedData {
                let aligned = align_offset(data_len, 16);
                if name == ".bss" || sec.kind() == object::SectionKind::UninitializedData {
                    data.resize(sec.size() as usize, 0);
                }
                data_chunks.push(SectionChunk {
                    file_id,
                    section_index: sec_idx,
                    name,
                    data,
                    start_offset: aligned,
                });
                data_len = aligned + sec.size() as usize;
            }
        }
    }

    // ── Generate Stubs and Import Table ──────────────────────────────────────
    let text_rva = SECTION_ALIGN;
    let stubs_start_offset = align_offset(text_len, 16);
    let stubs_len = KERNEL32_IMPORTS.len() * 6;
    text_len = stubs_start_offset + stubs_len;

    let rdata_rva = align_up(text_rva + text_len as u32, SECTION_ALIGN);
    let iat_start_offset = align_offset(rdata_len, 16);
    let (import_table_data, iat, iat_slot_rvas) = build_import_table_bytes(rdata_rva, iat_start_offset as u32);

    // Merge buffers
    let mut text_merged = Vec::new();
    for chunk in &text_chunks {
        text_merged.resize(chunk.start_offset, 0);
        text_merged.extend_from_slice(&chunk.data);
    }
    
    // Generate actual stub code
    let mut stubs_data = Vec::new();
    let mut import_stub_rvas = HashMap::new();
    for (i, &name) in KERNEL32_IMPORTS.iter().enumerate() {
        let stub_rva = text_rva + stubs_start_offset as u32 + (i as u32 * 6);
        import_stub_rvas.insert(name.to_string(), stub_rva);

        let iat_rva = iat_slot_rvas[name];
        let rip_rel = iat_rva as i32 - (stub_rva as i32 + 6);

        stubs_data.push(0xFF);
        stubs_data.push(0x25);
        stubs_data.extend_from_slice(&rip_rel.to_le_bytes());
    }
    text_merged.resize(stubs_start_offset, 0);
    text_merged.extend_from_slice(&stubs_data);

    let mut rdata_merged = Vec::new();
    for chunk in &rdata_chunks {
        rdata_merged.resize(chunk.start_offset, 0);
        rdata_merged.extend_from_slice(&chunk.data);
    }
    rdata_merged.resize(iat_start_offset, 0);
    rdata_merged.extend_from_slice(&import_table_data);
    rdata_len = rdata_merged.len();

    let data_rva = align_up(rdata_rva + rdata_len as u32, SECTION_ALIGN);
    let mut data_merged = Vec::new();
    for chunk in &data_chunks {
        data_merged.resize(chunk.start_offset, 0);
        data_merged.extend_from_slice(&chunk.data);
    }
    data_len = data_merged.len();

    let data_vsize = if data_len == 0 { SECTION_ALIGN } else { data_len as u32 };
    let image_size = align_up(data_rva + data_vsize, SECTION_ALIGN);

    // ── Build Symbol VA Map ──────────────────────────────────────────────────
    let mut symbol_vas = HashMap::new();
    for (obj, file_id) in [(&user_obj, 0), (&rt_obj, 1)] {
        for sym in obj.symbols() {
            if let Ok(name) = sym.name() {
                if name.is_empty() {
                    continue;
                }
                if let object::SymbolSection::Section(sec_idx) = sym.section() {
                    let mut found_chunk = None;
                    for chunk in text_chunks.iter().chain(rdata_chunks.iter()).chain(data_chunks.iter()) {
                        if chunk.file_id == file_id && chunk.section_index == sec_idx {
                            found_chunk = Some(chunk);
                            break;
                        }
                    }
                    if let Some(chunk) = found_chunk {
                        let sec_rva = match chunk.name.as_str() {
                            ".text" | "text" => text_rva,
                            ".rdata" | "rdata" | ".rodata" => rdata_rva,
                            ".data" | "data" | ".bss" => data_rva,
                            _ => {
                                if chunk.name.starts_with(".bss") { data_rva } else { text_rva }
                            }
                        };
                        let final_rva = sec_rva + chunk.start_offset as u32 + sym.address() as u32;
                        symbol_vas.insert(name.to_string(), final_rva);
                    }
                }
            }
        }
    }

    // Map extern symbols to their stubs/IAT entries
    for &name in KERNEL32_IMPORTS {
        if let Some(&stub_rva) = import_stub_rvas.get(name) {
            symbol_vas.insert(name.to_string(), stub_rva);
            symbol_vas.insert(format!("__imp_{}", name), iat_slot_rvas[name]);
        }
    }

    // ── Resolve Relocations ──────────────────────────────────────────────────
    for chunk in text_chunks.iter().chain(rdata_chunks.iter()).chain(data_chunks.iter()) {
        let obj = if chunk.file_id == 0 { &user_obj } else { &rt_obj };
        let sec = obj.section_by_index(chunk.section_index)?;

        let (merged_buf, sec_rva) = match chunk.name.as_str() {
            ".text" | "text" => (&mut text_merged, text_rva),
            ".rdata" | "rdata" | ".rodata" => (&mut rdata_merged, rdata_rva),
            ".data" | "data" | ".bss" => (&mut data_merged, data_rva),
            _ => {
                if chunk.name.starts_with(".bss") {
                    (&mut data_merged, data_rva)
                } else {
                    continue;
                }
            }
        };

        for (reloc_offset, reloc) in sec.relocations() {
            let target_rva = match reloc.target() {
                object::RelocationTarget::Symbol(sym_idx) => {
                    if let Ok(sym) = obj.symbol_by_index(sym_idx) {
                        if let Ok(name) = sym.name() {
                            if let Some(&va) = symbol_vas.get(name) {
                                va
                            } else {
                                if let object::SymbolSection::Section(s_idx) = sym.section() {
                                    let mut found_va = 0;
                                    for c in text_chunks.iter().chain(rdata_chunks.iter()).chain(data_chunks.iter()) {
                                        if c.file_id == chunk.file_id && c.section_index == s_idx {
                                            let c_sec_rva = match c.name.as_str() {
                                                ".text" | "text" => text_rva,
                                                ".rdata" | "rdata" | ".rodata" => rdata_rva,
                                                ".data" | "data" | ".bss" => data_rva,
                                                _ => {
                                                    if c.name.starts_with(".bss") { data_rva } else { text_rva }
                                                }
                                            };
                                            found_va = c_sec_rva + c.start_offset as u32 + sym.address() as u32;
                                            break;
                                        }
                                    }
                                    found_va
                                } else {
                                    if !name.is_empty() {
                                        println!("Warning: Unresolved symbol: {}", name);
                                    }
                                    0
                                }
                            }
                        } else {
                            0
                        }
                    } else {
                        0
                    }
                }
                object::RelocationTarget::Section(s_idx) => {
                    let mut found_va = 0;
                    for c in text_chunks.iter().chain(rdata_chunks.iter()).chain(data_chunks.iter()) {
                        if c.file_id == chunk.file_id && c.section_index == s_idx {
                            let c_sec_rva = match c.name.as_str() {
                                ".text" | "text" => text_rva,
                                ".rdata" | "rdata" | ".rodata" => rdata_rva,
                                ".data" | "data" | ".bss" => data_rva,
                                _ => {
                                    if c.name.starts_with(".bss") { data_rva } else { text_rva }
                                }
                            };
                            found_va = c_sec_rva + c.start_offset as u32;
                            break;
                        }
                    }
                    found_va
                }
                _ => 0,
            };

            let offset_in_merged = chunk.start_offset + reloc_offset as usize;
            let reloc_rva = sec_rva + offset_in_merged as u32;
            let addend = reloc.addend();

            match reloc.kind() {
                object::RelocationKind::Relative => {
                    let size = reloc.size();
                    let rel_val = target_rva as i64 - reloc_rva as i64 + addend;
                    if size == 32 {
                        merged_buf[offset_in_merged..offset_in_merged+4].copy_from_slice(&(rel_val as i32).to_le_bytes());
                    } else if size == 64 {
                        merged_buf[offset_in_merged..offset_in_merged+8].copy_from_slice(&rel_val.to_le_bytes());
                    }
                }
                object::RelocationKind::Absolute => {
                    let size = reloc.size();
                    let abs_val = target_rva as i64 + IMAGE_BASE as i64 + addend;
                    if size == 32 {
                        merged_buf[offset_in_merged..offset_in_merged+4].copy_from_slice(&(abs_val as i32).to_le_bytes());
                    } else if size == 64 {
                        merged_buf[offset_in_merged..offset_in_merged+8].copy_from_slice(&abs_val.to_le_bytes());
                    }
                }
                object::RelocationKind::ImageOffset => {
                    let size = reloc.size();
                    let rva_val = target_rva as i64 + addend;
                    if size == 32 {
                        merged_buf[offset_in_merged..offset_in_merged+4].copy_from_slice(&(rva_val as i32).to_le_bytes());
                    } else if size == 64 {
                        merged_buf[offset_in_merged..offset_in_merged+8].copy_from_slice(&rva_val.to_le_bytes());
                    }
                }
                _ => {}
            }
        }
    }

    // ── Build PE headers ─────────────────────────────────────────────────────
    let entry_point_rva = symbol_vas.get(entry_point).cloned().unwrap_or(text_rva);

    let mut pe: Vec<u8> = Vec::new();
    let dos_stub = build_dos_stub();
    pe.extend_from_slice(&dos_stub);

    let num_sections: u16 = 3;
    let headers_size = align_up(
        dos_stub.len() as u32
            + 4   // PE sig
            + 20  // COFF header
            + 240 // Optional header (PE32+)
            + num_sections as u32 * 40, // section headers
        FILE_ALIGN,
    );

    let text_raw_size = align_up(text_merged.len() as u32, FILE_ALIGN);
    let rdata_raw_size = align_up(rdata_merged.len() as u32, FILE_ALIGN);
    let data_raw_size = align_up(data_merged.len() as u32, FILE_ALIGN);

    let text_file_offset = headers_size;
    let rdata_file_offset = text_file_offset + text_raw_size;
    let data_file_offset = rdata_file_offset + rdata_raw_size;

    let mut pe_hdr = Vec::new();
    pe_hdr.extend_from_slice(b"PE\0\0");

    // COFF File Header
    write_u16(&mut pe_hdr, 0x8664); // Machine: AMD64
    write_u16(&mut pe_hdr, num_sections);
    write_u32(&mut pe_hdr, 0); // Timestamp
    write_u32(&mut pe_hdr, 0); // Symbol table ptr
    write_u32(&mut pe_hdr, 0); // Symbol table count
    write_u16(&mut pe_hdr, 240); // Optional header size
    write_u16(&mut pe_hdr, 0x0022); // exe, large-address-aware

    // Optional Header PE32+
    write_u16(&mut pe_hdr, 0x020B); // Magic: PE32+
    pe_hdr.push(14); // MajorLinkerVersion
    pe_hdr.push(0);  // MinorLinkerVersion
    write_u32(&mut pe_hdr, text_raw_size);
    write_u32(&mut pe_hdr, rdata_raw_size + data_raw_size);
    write_u32(&mut pe_hdr, 0);
    write_u32(&mut pe_hdr, entry_point_rva);
    write_u32(&mut pe_hdr, text_rva);
    write_u64(&mut pe_hdr, IMAGE_BASE);
    write_u32(&mut pe_hdr, SECTION_ALIGN);
    write_u32(&mut pe_hdr, FILE_ALIGN);
    write_u16(&mut pe_hdr, 6); // MajorOS
    write_u16(&mut pe_hdr, 0);
    write_u16(&mut pe_hdr, 0);
    write_u16(&mut pe_hdr, 0);
    write_u16(&mut pe_hdr, 6); // MajorSubsystem
    write_u16(&mut pe_hdr, 0);
    write_u32(&mut pe_hdr, 0);
    write_u32(&mut pe_hdr, image_size);
    write_u32(&mut pe_hdr, headers_size);
    write_u32(&mut pe_hdr, 0);
    write_u16(&mut pe_hdr, 3); // Subsystem: WINDOWS_CUI
    write_u16(&mut pe_hdr, 0x8100); // DLLCharacteristics: NX-compat, Terminal Server Aware (ASLR disabled)
    write_u64(&mut pe_hdr, 0x0010_0000); // StackReserve
    write_u64(&mut pe_hdr, 0x0001_0000); // StackCommit
    write_u64(&mut pe_hdr, 0x0010_0000); // HeapReserve
    write_u64(&mut pe_hdr, 0x0001_0000); // HeapCommit
    write_u32(&mut pe_hdr, 0);
    write_u32(&mut pe_hdr, 16);

    let import_dir_rva = rdata_rva + iat.import_dir_rva_in_rdata;
    let import_dir_size = iat.import_dir_size;
    for i in 0..16u32 {
        if i == 1 {
            write_u32(&mut pe_hdr, import_dir_rva);
            write_u32(&mut pe_hdr, import_dir_size);
        } else if i == 12 {
            write_u32(&mut pe_hdr, rdata_rva + iat.iat_rva_in_rdata);
            write_u32(&mut pe_hdr, iat.iat_size);
        } else {
            write_u64(&mut pe_hdr, 0);
        }
    }

    // Section Table
    let mut write_section = |name: &[u8; 8], vsize: u32, rva: u32, rawsize: u32, rawoff: u32, chars: u32| {
        pe_hdr.extend_from_slice(name);
        write_u32(&mut pe_hdr, vsize);
        write_u32(&mut pe_hdr, rva);
        write_u32(&mut pe_hdr, rawsize);
        write_u32(&mut pe_hdr, rawoff);
        pe_hdr.extend_from_slice(&[0u8; 12]);
        write_u32(&mut pe_hdr, chars);
    };

    write_section(b".text\0\0\0", text_merged.len() as u32, text_rva, text_raw_size, text_file_offset, 0x6000_0020);
    write_section(b".rdata\0\0", rdata_merged.len() as u32, rdata_rva, rdata_raw_size, rdata_file_offset, 0x4000_0040);
    write_section(b".data\0\0\0", data_vsize, data_rva, data_raw_size, data_file_offset, 0xC000_0040);

    let mut out = Vec::new();
    out.extend_from_slice(&dos_stub);
    assert_eq!(out.len(), 0x40);
    out.extend_from_slice(&pe_hdr);

    while out.len() < headers_size as usize {
        out.push(0);
    }

    out.extend_from_slice(&text_merged);
    while out.len() < (text_file_offset + text_raw_size) as usize {
        out.push(0);
    }

    out.extend_from_slice(&rdata_merged);
    while out.len() < (rdata_file_offset + rdata_raw_size) as usize {
        out.push(0);
    }

    out.extend_from_slice(&data_merged);
    while out.len() < (data_file_offset + data_raw_size) as usize {
        out.push(0);
    }

    Ok(out)
}
