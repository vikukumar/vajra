/// Vajra PE32+ Linker — Pure Rust Windows EXE emitter
/// Produces a complete, runnable PE32+ executable from COFF object files.
/// Only imports kernel32.dll (ExitProcess, WriteFile, GetStdHandle, CreateThread, VirtualAlloc).
/// NO MSVC link.exe required.
/// Uses own COFF parser — zero external crate dependency.

use anyhow::Result;
use std::collections::HashMap;
use crate::assembler::coff_writer::parse_coff;

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
    section_index: usize,
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
    // Parse both COFF objects using our own parser
    let (user_sections, user_symbols, user_relocs) =
        parse_coff(obj_bytes, 0).map_err(|e| anyhow::anyhow!("User object parse error: {}", e))?;
    let (rt_sections, rt_symbols, rt_relocs) =
        parse_coff(runtime_bytes, 1).map_err(|e| anyhow::anyhow!("Runtime object parse error: {}", e))?;

    let mut text_chunks: Vec<SectionChunk> = Vec::new();
    let mut rdata_chunks: Vec<SectionChunk> = Vec::new();
    let mut data_chunks: Vec<SectionChunk> = Vec::new();

    let mut text_len = 0usize;
    let mut rdata_len = 0usize;
    let mut data_len = 0usize;

    let all_sections: Vec<_> = user_sections.iter().chain(rt_sections.iter()).collect();

    for sec in &all_sections {
        let name = &sec.name;
        let data = sec.data.clone();

        if name == ".text" || name == "text" {
            let aligned = align_offset(text_len, 16);
            text_len = aligned + data.len();
            text_chunks.push(SectionChunk {
                file_id: sec.file_id,
                section_index: sec.section_index,
                name: name.clone(),
                data,
                start_offset: aligned,
            });
        } else if name == ".rdata" || name == "rdata" || name == ".rodata" {
            let aligned = align_offset(rdata_len, 16);
            rdata_len = aligned + data.len();
            rdata_chunks.push(SectionChunk {
                file_id: sec.file_id,
                section_index: sec.section_index,
                name: name.clone(),
                data,
                start_offset: aligned,
            });
        } else if name == ".data" || name == "data" || name == ".bss" {
            let aligned = align_offset(data_len, 16);
            data_len = aligned + data.len();
            data_chunks.push(SectionChunk {
                file_id: sec.file_id,
                section_index: sec.section_index,
                name: name.clone(),
                data,
                start_offset: aligned,
            });
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

    // Build symbol VA map — we iterate user and runtime separately to know file_id
    let mut symbol_vas: HashMap<String, u32> = HashMap::new();

    for (file_id, sym_list) in [(0usize, &user_symbols), (1usize, &rt_symbols)] {
        for sym in sym_list {
            if sym.name.is_empty() || sym.section_idx <= 0 { continue; }
            let sec_idx = (sym.section_idx - 1) as usize; // convert 1-based COFF to 0-based

            let found = text_chunks.iter()
                .chain(rdata_chunks.iter())
                .chain(data_chunks.iter())
                .find(|c| c.file_id == file_id && c.section_index == sec_idx);

            if let Some(chunk) = found {
                let sec_rva = match chunk.name.as_str() {
                    ".text" | "text" => text_rva,
                    ".rdata" | "rdata" | ".rodata" => rdata_rva,
                    ".data" | "data" | ".bss" => data_rva,
                    _ => text_rva,
                };
                let final_rva = sec_rva + chunk.start_offset as u32 + sym.value;
                symbol_vas.insert(sym.name.clone(), final_rva);
            }
        }
    }
    for &name in KERNEL32_IMPORTS {
        if let Some(&stub_rva) = import_stub_rvas.get(name) {
            symbol_vas.insert(name.to_string(), stub_rva);
            symbol_vas.insert(format!("__imp_{}", name), iat_slot_rvas[name]);
        }
    }

    // ── Resolve Relocations ──────────────────────────────────────────────────
    // Build a combined reloc list with resolved symbol names
    // Each CoffRelocEntry has: section_idx, offset, sym_idx, reloc_type
    // We need to look up the symbol by index to get its name

    let all_relocs: Vec<_> = user_relocs.iter().map(|r| (r, 0usize))
        .chain(rt_relocs.iter().map(|r| (r, 1usize)))
        .collect();

    let user_sym_vec: Vec<_> = user_symbols.iter().collect();
    let rt_sym_vec: Vec<_> = rt_symbols.iter().collect();

    for (reloc, file_id) in &all_relocs {
        // Look up symbol name
        let sym_vec = if *file_id == 0 { &user_sym_vec } else { &rt_sym_vec };
        let sym_name = sym_vec.get(reloc.sym_idx as usize)
            .map(|s| s.name.as_str())
            .unwrap_or("");


        // Find the merged buffer + chunk for this section (inline to avoid lifetime issues)
        let text_match  = text_chunks .iter().find(|c| c.file_id == *file_id && c.section_index == reloc.section_idx);
        let rdata_match = rdata_chunks.iter().find(|c| c.file_id == *file_id && c.section_index == reloc.section_idx);
        let data_match  = data_chunks .iter().find(|c| c.file_id == *file_id && c.section_index == reloc.section_idx);

        let (sec_rva, chunk_start) =
            if let Some(c) = text_match  { (text_rva,  c.start_offset) }
            else if let Some(c) = rdata_match { (rdata_rva, c.start_offset) }
            else if let Some(c) = data_match  { (data_rva,  c.start_offset) }
            else { continue };

        let merged_buf = if text_match.is_some()  { &mut text_merged  }
            else if rdata_match.is_some() { &mut rdata_merged }
            else                          { &mut data_merged  };

        let target_rva = if let Some(&va) = symbol_vas.get(sym_name) {
            va
        } else if !sym_name.is_empty() {
            // Section-reference symbols (.pdata, .xdata, .debug$S) are exception/debug metadata
            // — they don't need to be resolved in our PE for normal execution.
            if sym_name.starts_with('.') {
                continue; // silently skip section-reference relocations
            }
            anyhow::bail!("Linker Error: Unresolved symbol '{}'", sym_name);
        } else {
            0
        };

        let offset_in_merged = chunk_start + reloc.offset as usize;
        let reloc_rva = sec_rva + offset_in_merged as u32;

        // Read value already baked into the code at the reloc site (MSVC COFF may pre-bake addend)
        let implicit_addend = if offset_in_merged + 4 <= merged_buf.len() {
            i32::from_le_bytes(merged_buf[offset_in_merged..offset_in_merged + 4].try_into().unwrap()) as i64
        } else {
            0
        };

        // IMAGE_REL_AMD64_REL32 (type 4) and IMAGE_REL_AMD64_REL32_1..4 (types 5-8):
        // Spec: "The 32-bit relative address from the byte following the relocation."
        // Formula: patch = target_rva - (reloc_site_rva + 4) + implicit_addend
        // The -4 is mandatory: CPU reads RIP from the next byte after the 4-byte field.
        if reloc.reloc_type >= 4 && reloc.reloc_type <= 8 {
            let size_adj = (reloc.reloc_type - 3) as i64; // type4→1byte_adj, type5→2, etc.
            let _ = size_adj; // always 4-byte field; size_adj only matters for REL32_1..4
            let rel_val = target_rva as i64 - reloc_rva as i64 - 4 + implicit_addend;
            if offset_in_merged + 4 <= merged_buf.len() {
                merged_buf[offset_in_merged..offset_in_merged + 4]
                    .copy_from_slice(&(rel_val as i32).to_le_bytes());
            }
        } else if reloc.reloc_type == 1 {
            // IMAGE_REL_AMD64_ADDR64: absolute 64-bit VA
            let abs_val = target_rva as i64 + IMAGE_BASE as i64 + implicit_addend;
            if offset_in_merged + 8 <= merged_buf.len() {
                merged_buf[offset_in_merged..offset_in_merged + 8]
                    .copy_from_slice(&abs_val.to_le_bytes());
            }
        } else if reloc.reloc_type == 2 {
            // IMAGE_REL_AMD64_ADDR32NB: 32-bit without image base (RVA)
            let rva_val = target_rva as i64 + implicit_addend;
            if offset_in_merged + 4 <= merged_buf.len() {
                merged_buf[offset_in_merged..offset_in_merged + 4]
                    .copy_from_slice(&(rva_val as i32).to_le_bytes());
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
