//! Vajra COFF Object File Writer
//! Writes x86-64 COFF object files (.obj) without any external crates.
//! Implements COFF as documented in the Microsoft PE/COFF Specification.
//!
//! Layout:
//!   COFF File Header (20 bytes)
//!   Section Headers (40 bytes each)
//!   Section Data (.text, .rdata, .data)
//!   Relocation tables
//!   Symbol Table
//!   String Table

use std::collections::HashMap;

// ─── COFF constants ───────────────────────────────────────────────────────────

const IMAGE_FILE_MACHINE_AMD64: u16 = 0x8664;
const IMAGE_SCN_CNT_CODE: u32             = 0x0000_0020;
const IMAGE_SCN_CNT_INITIALIZED_DATA: u32 = 0x0000_0040;
const IMAGE_SCN_MEM_EXECUTE: u32          = 0x2000_0000;
const IMAGE_SCN_MEM_READ: u32             = 0x4000_0000;
const IMAGE_SCN_MEM_WRITE: u32            = 0x8000_0000;
const IMAGE_SCN_ALIGN_16BYTES: u32        = 0x0050_0000;
const IMAGE_SCN_ALIGN_8BYTES: u32         = 0x0040_0000;

const IMAGE_SYM_CLASS_EXTERNAL: u8 = 2;
const IMAGE_SYM_CLASS_STATIC: u8   = 3;
const IMAGE_SYM_TYPE_FUNCTION: u16 = 0x0020;
const IMAGE_SYM_TYPE_NULL: u16     = 0x0000;
const IMAGE_SYM_ABSOLUTE: i16      = -1;

const IMAGE_REL_AMD64_REL32: u16 = 0x0004;
// const IMAGE_REL_AMD64_ADDR64: u16 = 0x0001;
// const IMAGE_REL_AMD64_ADDR32NB: u16 = 0x0003;

// ─── Writer helpers ───────────────────────────────────────────────────────────

fn w16(buf: &mut Vec<u8>, v: u16) { buf.extend_from_slice(&v.to_le_bytes()); }
fn w32(buf: &mut Vec<u8>, v: u32) { buf.extend_from_slice(&v.to_le_bytes()); }
fn w16i(buf: &mut Vec<u8>, v: i16) { buf.extend_from_slice(&v.to_le_bytes()); }

// ─── Public types ─────────────────────────────────────────────────────────────

/// A relocation entry to emit in the COFF object
#[derive(Debug, Clone)]
pub struct CoffReloc {
    /// Byte offset within the section where the relocation applies
    pub offset: u32,
    /// Name of the symbol being referenced
    pub symbol: String,
    /// Addend baked into the code (usually -4 for RIP-relative calls)
    pub addend: i32,
}

/// A symbol defined in this object file
#[derive(Debug, Clone)]
pub struct CoffSymbol {
    pub name: String,
    /// 1-based section index (1=.text, 2=.rdata, 3=.data)
    pub section: u16,
    pub value: u32,
    pub is_function: bool,
    pub is_external: bool,
}

/// Complete COFF object to emit
pub struct CoffObject {
    pub text: Vec<u8>,
    pub rdata: Vec<u8>,
    pub data: Vec<u8>,
    pub text_relocs: Vec<CoffReloc>,
    pub rdata_relocs: Vec<CoffReloc>,
    pub defined_symbols: Vec<CoffSymbol>,
    /// External (undefined) symbols referenced but not defined here
    pub extern_symbols: Vec<String>,
}

impl CoffObject {
    pub fn new() -> Self {
        Self {
            text: Vec::new(),
            rdata: Vec::new(),
            data: Vec::new(),
            text_relocs: Vec::new(),
            rdata_relocs: Vec::new(),
            defined_symbols: Vec::new(),
            extern_symbols: Vec::new(),
        }
    }

    /// Emit the COFF object as raw bytes
    pub fn write(&self) -> Vec<u8> {
        // ── Build string table ────────────────────────────────────────────────
        // String table starts with 4-byte size field, then null-terminated strings
        // for names longer than 8 bytes.
        let mut strtab: Vec<u8> = Vec::new();
        strtab.extend_from_slice(&[0u8; 4]); // placeholder for size

        let mut name_offsets: HashMap<&str, u32> = HashMap::new();

        let all_names: Vec<&str> = self.defined_symbols.iter().map(|s| s.name.as_str())
            .chain(self.extern_symbols.iter().map(|s| s.as_str()))
            .collect();

        for name in &all_names {
            if name.len() > 8 {
                if !name_offsets.contains_key(name) {
                    let off = strtab.len() as u32;
                    name_offsets.insert(name, off);
                    strtab.extend_from_slice(name.as_bytes());
                    strtab.push(0);
                }
            }
        }
        // Write string table size into first 4 bytes
        let strtab_size = strtab.len() as u32;
        strtab[0..4].copy_from_slice(&strtab_size.to_le_bytes());

        // ── Symbol table ──────────────────────────────────────────────────────
        // Each COFF symbol is 18 bytes.
        // Section symbols (.text, .rdata, .data) + defined + extern symbols.

        // Build all symbols:
        // 0: .text section symbol
        // 1: .rdata section symbol  
        // 2: .data section symbol
        // 3+: defined symbols
        // N+: extern symbols

        let mut sym_table: Vec<u8> = Vec::new();
        let mut sym_indices: HashMap<String, u32> = HashMap::new();

        let mut emit_sym = |sym_table: &mut Vec<u8>,
                            name: &str,
                            value: u32,
                            section: i16,
                            typ: u16,
                            storage: u8,
                            name_offsets: &HashMap<&str, u32>| {
            // Name: 8 bytes — either padded name or zeros + 4-byte string table offset
            if name.len() <= 8 {
                let mut name_bytes = [0u8; 8];
                name_bytes[..name.len()].copy_from_slice(name.as_bytes());
                sym_table.extend_from_slice(&name_bytes);
            } else {
                sym_table.extend_from_slice(&[0u8; 4]); // zeros
                let off = name_offsets[name];
                sym_table.extend_from_slice(&off.to_le_bytes());
            }
            w32(sym_table, value);
            w16i(sym_table, section);
            w16(sym_table, typ);
            sym_table.push(storage);
            sym_table.push(0); // NumberOfAuxSymbols
        };

        // Section symbols
        emit_sym(&mut sym_table, ".text",  0, 1, IMAGE_SYM_TYPE_NULL, IMAGE_SYM_CLASS_STATIC, &name_offsets);
        emit_sym(&mut sym_table, ".rdata", 0, 2, IMAGE_SYM_TYPE_NULL, IMAGE_SYM_CLASS_STATIC, &name_offsets);
        emit_sym(&mut sym_table, ".data",  0, 3, IMAGE_SYM_TYPE_NULL, IMAGE_SYM_CLASS_STATIC, &name_offsets);

        let mut sym_count = 3u32;

        for sym in &self.defined_symbols {
            sym_indices.insert(sym.name.clone(), sym_count);
            sym_count += 1;
            let typ = if sym.is_function { IMAGE_SYM_TYPE_FUNCTION } else { IMAGE_SYM_TYPE_NULL };
            let storage = if sym.is_external { IMAGE_SYM_CLASS_EXTERNAL } else { IMAGE_SYM_CLASS_STATIC };
            emit_sym(&mut sym_table, &sym.name, sym.value, sym.section as i16, typ, storage, &name_offsets);
        }

        for name in &self.extern_symbols {
            sym_indices.insert(name.clone(), sym_count);
            sym_count += 1;
            // Section = IMAGE_SYM_ABSOLUTE means undefined external
            emit_sym(&mut sym_table, name, 0, IMAGE_SYM_ABSOLUTE, IMAGE_SYM_TYPE_FUNCTION, IMAGE_SYM_CLASS_EXTERNAL, &name_offsets);
        }

        // ── Relocation tables ─────────────────────────────────────────────────
        // Each relocation: 10 bytes (offset u32, sym_idx u32, type u16)

        let build_relocs = |relocs: &[CoffReloc]| -> Vec<u8> {
            let mut buf = Vec::new();
            for r in relocs {
                let sym_idx = sym_indices.get(&r.symbol).copied().unwrap_or(0);
                w32(&mut buf, r.offset);
                w32(&mut buf, sym_idx);
                w16(&mut buf, IMAGE_REL_AMD64_REL32);
            }
            buf
        };

        let text_reloc_bytes = build_relocs(&self.text_relocs);
        let rdata_reloc_bytes = build_relocs(&self.rdata_relocs);

        // ── Layout ───────────────────────────────────────────────────────────
        // File header: 20 bytes
        // 3 section headers: 3 × 40 = 120 bytes
        // Total headers: 140 bytes

        let header_size = 20 + 3 * 40;

        let text_raw_ptr = header_size as u32;
        let text_raw_size = self.text.len() as u32;
        let text_reloc_ptr = if self.text_relocs.is_empty() {
            0
        } else {
            text_raw_ptr + text_raw_size
        };
        let text_reloc_count = self.text_relocs.len() as u16;

        let rdata_raw_ptr = text_raw_ptr + text_raw_size + text_reloc_bytes.len() as u32;
        let rdata_raw_size = self.rdata.len() as u32;
        let rdata_reloc_ptr = if self.rdata_relocs.is_empty() {
            0
        } else {
            rdata_raw_ptr + rdata_raw_size
        };
        let rdata_reloc_count = self.rdata_relocs.len() as u16;

        let data_raw_ptr = rdata_raw_ptr + rdata_raw_size + rdata_reloc_bytes.len() as u32;
        let data_raw_size = self.data.len() as u32;

        let sym_table_ptr = data_raw_ptr + data_raw_size;
        let strtab_ptr = sym_table_ptr + sym_count * 18;

        // ── COFF File Header ──────────────────────────────────────────────────
        let mut out = Vec::new();
        w16(&mut out, IMAGE_FILE_MACHINE_AMD64); // Machine
        w16(&mut out, 3);                         // NumberOfSections
        w32(&mut out, 0);                         // TimeDateStamp
        w32(&mut out, sym_table_ptr);             // PointerToSymbolTable
        w32(&mut out, sym_count);                 // NumberOfSymbols
        w16(&mut out, 0);                         // SizeOfOptionalHeader
        w16(&mut out, 0);                         // Characteristics

        // ── Section headers ───────────────────────────────────────────────────
        let write_section_hdr = |out: &mut Vec<u8>,
                                 name: &[u8; 8],
                                 vsize: u32,
                                 raw_ptr: u32,
                                 raw_size: u32,
                                 reloc_ptr: u32,
                                 reloc_count: u16,
                                 chars: u32| {
            out.extend_from_slice(name);
            w32(out, vsize);          // VirtualSize
            w32(out, 0);              // VirtualAddress (0 for obj files)
            w32(out, raw_size);       // SizeOfRawData
            w32(out, raw_ptr);        // PointerToRawData
            w32(out, reloc_ptr);      // PointerToRelocations
            w32(out, 0);              // PointerToLinenumbers
            w16(out, reloc_count);    // NumberOfRelocations
            w16(out, 0);              // NumberOfLinenumbers
            w32(out, chars);          // Characteristics
        };

        write_section_hdr(
            &mut out, b".text\0\0\0",
            text_raw_size, text_raw_ptr, text_raw_size,
            text_reloc_ptr, text_reloc_count,
            IMAGE_SCN_CNT_CODE | IMAGE_SCN_MEM_EXECUTE | IMAGE_SCN_MEM_READ | IMAGE_SCN_ALIGN_16BYTES,
        );
        write_section_hdr(
            &mut out, b".rdata\0\0",
            rdata_raw_size, rdata_raw_ptr, rdata_raw_size,
            rdata_reloc_ptr, rdata_reloc_count,
            IMAGE_SCN_CNT_INITIALIZED_DATA | IMAGE_SCN_MEM_READ | IMAGE_SCN_ALIGN_8BYTES,
        );
        write_section_hdr(
            &mut out, b".data\0\0\0",
            data_raw_size, data_raw_ptr, data_raw_size,
            0, 0,
            IMAGE_SCN_CNT_INITIALIZED_DATA | IMAGE_SCN_MEM_READ | IMAGE_SCN_MEM_WRITE | IMAGE_SCN_ALIGN_8BYTES,
        );

        // ── Section data ──────────────────────────────────────────────────────
        out.extend_from_slice(&self.text);
        out.extend_from_slice(&text_reloc_bytes);
        out.extend_from_slice(&self.rdata);
        out.extend_from_slice(&rdata_reloc_bytes);
        out.extend_from_slice(&self.data);

        // ── Symbol table ──────────────────────────────────────────────────────
        assert_eq!(out.len(), sym_table_ptr as usize,
            "Symbol table offset mismatch: expected {}, got {}", sym_table_ptr, out.len());
        out.extend_from_slice(&sym_table);

        // ── String table ──────────────────────────────────────────────────────
        assert_eq!(out.len(), strtab_ptr as usize,
            "String table offset mismatch");
        out.extend_from_slice(&strtab);

        out
    }
}

// ─── COFF Object Reader (for linker) ─────────────────────────────────────────

/// Minimal COFF section read from object bytes
#[derive(Debug, Clone)]
pub struct CoffSection {
    pub name: String,
    pub data: Vec<u8>,
    pub file_id: usize,
    pub section_index: usize,
}

/// Minimal COFF symbol read from object bytes
#[derive(Debug, Clone)]
pub struct CoffSym {
    pub name: String,
    pub value: u32,
    pub section_idx: i16, // 1-based section, -1 = absolute, 0 = undefined
    pub is_external: bool,
}

/// A relocation entry read from a COFF object
#[derive(Debug, Clone)]
pub struct CoffRelocEntry {
    pub section_idx: usize,   // which section (0-based) this relocation belongs to
    pub offset: u32,
    pub sym_idx: u32,
    pub reloc_type: u16,
}

/// Parse a COFF object file and return sections, symbols, relocations
pub fn parse_coff(
    bytes: &[u8],
    file_id: usize,
) -> Result<(Vec<CoffSection>, Vec<CoffSym>, Vec<CoffRelocEntry>), String> {
    if bytes.len() < 20 {
        return Err("COFF too small".into());
    }

    let machine = u16::from_le_bytes([bytes[0], bytes[1]]);
    if machine != IMAGE_FILE_MACHINE_AMD64 {
        return Err(format!("Not an AMD64 COFF (machine={:#x})", machine));
    }

    let num_sections = u16::from_le_bytes([bytes[2], bytes[3]]) as usize;
    let sym_table_ptr = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]) as usize;
    let num_symbols = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]) as usize;

    // String table is immediately after the symbol table (each symbol = 18 bytes)
    let strtab_off = sym_table_ptr + num_symbols * 18;
    let strtab = if strtab_off + 4 <= bytes.len() {
        let size = u32::from_le_bytes([
            bytes[strtab_off], bytes[strtab_off+1],
            bytes[strtab_off+2], bytes[strtab_off+3]
        ]) as usize;
        if strtab_off + size <= bytes.len() {
            &bytes[strtab_off..strtab_off + size]
        } else {
            &bytes[strtab_off..]
        }
    } else {
        b""
    };

    let read_coff_name = |raw: &[u8; 8]| -> String {
        if raw[0] == 0 && raw[1] == 0 && raw[2] == 0 && raw[3] == 0 {
            // Offset into string table
            let off = u32::from_le_bytes([raw[4], raw[5], raw[6], raw[7]]) as usize;
            if off < strtab.len() {
                let end = strtab[off..].iter().position(|&b| b == 0).unwrap_or(strtab.len() - off);
                String::from_utf8_lossy(&strtab[off..off + end]).into_owned()
            } else {
                String::new()
            }
        } else {
            // Inline name (up to 8 chars, null-padded)
            let end = raw.iter().position(|&b| b == 0).unwrap_or(8);
            String::from_utf8_lossy(&raw[..end]).into_owned()
        }
    };

    // Read section headers (offset 20, each 40 bytes)
    let mut sections = Vec::new();
    let mut reloc_entries = Vec::new();

    for i in 0..num_sections {
        let sh_off = 20 + i * 40;
        if sh_off + 40 > bytes.len() { break; }
        let sh = &bytes[sh_off..sh_off + 40];

        let mut name_raw = [0u8; 8];
        name_raw.copy_from_slice(&sh[0..8]);
        let name = read_coff_name(&name_raw);

        let raw_size = u32::from_le_bytes([sh[16], sh[17], sh[18], sh[19]]) as usize;
        let raw_ptr  = u32::from_le_bytes([sh[20], sh[21], sh[22], sh[23]]) as usize;
        let reloc_ptr   = u32::from_le_bytes([sh[24], sh[25], sh[26], sh[27]]) as usize;
        let num_relocs  = u16::from_le_bytes([sh[32], sh[33]]) as usize;

        let data = if raw_ptr > 0 && raw_ptr + raw_size <= bytes.len() {
            bytes[raw_ptr..raw_ptr + raw_size].to_vec()
        } else {
            vec![0u8; raw_size]
        };

        sections.push(CoffSection { name, data, file_id, section_index: i });

        // Read relocations for this section
        for j in 0..num_relocs {
            let r_off = reloc_ptr + j * 10;
            if r_off + 10 > bytes.len() { break; }
            let r = &bytes[r_off..r_off + 10];
            let offset   = u32::from_le_bytes([r[0], r[1], r[2], r[3]]);
            let sym_idx  = u32::from_le_bytes([r[4], r[5], r[6], r[7]]);
            let reloc_type = u16::from_le_bytes([r[8], r[9]]);
            reloc_entries.push(CoffRelocEntry { section_idx: i, offset, sym_idx, reloc_type });
        }
    }

    // Read symbols — maintain ALL at their absolute symbol-table index
    // (sym_idx in relocations is a direct 0-based index into this array)
    let mut symbols: Vec<CoffSym> = Vec::new();
    let mut si = 0;
    while si < num_symbols {
        let sym_off = sym_table_ptr + si * 18;
        if sym_off + 18 > bytes.len() { break; }
        let s = &bytes[sym_off..sym_off + 18];

        let mut name_raw = [0u8; 8];
        name_raw.copy_from_slice(&s[0..8]);
        let name = read_coff_name(&name_raw);

        let value = u32::from_le_bytes([s[8], s[9], s[10], s[11]]);
        let section_idx = i16::from_le_bytes([s[12], s[13]]);
        // COFF symbol layout: name(8) value(4) section(2) type(2) storage_class(1) num_aux(1) = 18
        let storage_class = s[16];
        let num_aux = s[17] as usize;

        let is_external = storage_class == IMAGE_SYM_CLASS_EXTERNAL;
        // Push symbol at this index (even if unnamed) so sym_idx lookup is always correct
        symbols.push(CoffSym { name, value, section_idx, is_external });
        si += 1;

        // Skip auxiliary symbol records — push placeholder entries so indices stay aligned
        for _ in 0..num_aux {
            symbols.push(CoffSym {
                name: String::new(),
                value: 0,
                section_idx: 0,
                is_external: false,
            });
            si += 1;
        }
    }

    Ok((sections, symbols, reloc_entries))
}
