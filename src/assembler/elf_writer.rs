//! Vajra ELF64 Object File Writer + Reader
//! Produces ELF64 object files without any external crates.
//! Implements ELF64 as documented in the System V ABI.

// ─── ELF constants ────────────────────────────────────────────────────────────

const ELFMAG: &[u8] = b"\x7FELF";
const ELFCLASS64: u8      = 2;
const ELFDATA2LSB: u8     = 1;
const ET_REL: u16         = 1;    // Relocatable
const EM_X86_64: u16      = 62;
const EV_CURRENT: u32     = 1;
const SHT_NULL: u32       = 0;
const SHT_PROGBITS: u32   = 1;
const SHT_SYMTAB: u32     = 2;
const SHT_STRTAB: u32     = 3;
const SHT_RELA: u32       = 4;
const SHF_ALLOC: u64      = 0x2;
const SHF_EXECINSTR: u64  = 0x4;
const SHF_WRITE: u64      = 0x1;
const STB_LOCAL: u8       = 0;
const STB_GLOBAL: u8      = 1;
const STT_NOTYPE: u8      = 0;
const STT_FUNC: u8        = 2;
const STV_DEFAULT: u8     = 0;
const SHN_UNDEF: u16      = 0;
// const SHN_ABS: u16        = 0xFFF1;

/// R_X86_64_PLT32: 32-bit PC-relative (used for function calls)
pub const R_X86_64_PLT32: u32 = 4;
/// R_X86_64_PC32: 32-bit PC-relative
pub const R_X86_64_PC32: u32  = 2;

// ─── Writer helpers ───────────────────────────────────────────────────────────

fn w16(buf: &mut Vec<u8>, v: u16) { buf.extend_from_slice(&v.to_le_bytes()); }
fn w32(buf: &mut Vec<u8>, v: u32) { buf.extend_from_slice(&v.to_le_bytes()); }
fn w64(buf: &mut Vec<u8>, v: u64) { buf.extend_from_slice(&v.to_le_bytes()); }
fn w64i(buf: &mut Vec<u8>, v: i64) { buf.extend_from_slice(&v.to_le_bytes()); }

// ─── Public types ─────────────────────────────────────────────────────────────

/// A relocation for the ELF .rela.text section
#[derive(Debug, Clone)]
pub struct ElfReloc {
    pub offset: u64,  // offset within the section
    pub symbol: String,
    pub rela_type: u32,
    pub addend: i64,
}

/// A symbol defined in this object
#[derive(Debug, Clone)]
pub struct ElfSymbol {
    pub name: String,
    pub section: usize,  // 0-based section index into our sections list
    pub value: u64,
    pub size: u64,
    pub is_function: bool,
    pub is_global: bool,
}

/// Complete ELF64 relocatable object
pub struct ElfObject {
    pub text: Vec<u8>,
    pub rodata: Vec<u8>,
    pub data: Vec<u8>,
    pub text_relocs: Vec<ElfReloc>,
    pub defined_symbols: Vec<ElfSymbol>,
    pub extern_symbols: Vec<String>,
}

impl ElfObject {
    pub fn new() -> Self {
        Self {
            text: Vec::new(),
            rodata: Vec::new(),
            data: Vec::new(),
            text_relocs: Vec::new(),
            defined_symbols: Vec::new(),
            extern_symbols: Vec::new(),
        }
    }

    pub fn write(&self) -> Vec<u8> {
        // Sections in order:
        // 0: NULL
        // 1: .text
        // 2: .rodata
        // 3: .data
        // 4: .rela.text
        // 5: .symtab
        // 6: .strtab
        // 7: .shstrtab

        // ── String tables ─────────────────────────────────────────────────────

        // .strtab: symbol name strings
        let mut strtab = vec![0u8]; // index 0 is always empty string
        let mut sym_name_idx: std::collections::HashMap<String, u32> = std::collections::HashMap::new();

        for sym in &self.defined_symbols {
            if !sym_name_idx.contains_key(&sym.name) {
                sym_name_idx.insert(sym.name.clone(), strtab.len() as u32);
                strtab.extend_from_slice(sym.name.as_bytes());
                strtab.push(0);
            }
        }
        for name in &self.extern_symbols {
            if !sym_name_idx.contains_key(name) {
                sym_name_idx.insert(name.clone(), strtab.len() as u32);
                strtab.extend_from_slice(name.as_bytes());
                strtab.push(0);
            }
        }

        // .shstrtab: section name strings
        let mut shstrtab = vec![0u8];
        let mut sh_name = |shstrtab: &mut Vec<u8>, name: &str| -> u32 {
            let off = shstrtab.len() as u32;
            shstrtab.extend_from_slice(name.as_bytes());
            shstrtab.push(0);
            off
        };
        let sh_null_name   = sh_name(&mut shstrtab, "");
        let sh_text_name   = sh_name(&mut shstrtab, ".text");
        let sh_rodata_name = sh_name(&mut shstrtab, ".rodata");
        let sh_data_name   = sh_name(&mut shstrtab, ".data");
        let sh_rela_name   = sh_name(&mut shstrtab, ".rela.text");
        let sh_symtab_name = sh_name(&mut shstrtab, ".symtab");
        let sh_strtab_name = sh_name(&mut shstrtab, ".strtab");
        let sh_shstrtab_name = sh_name(&mut shstrtab, ".shstrtab");

        // ── Symbol table ──────────────────────────────────────────────────────
        // Elf64_Sym: st_name(4) st_info(1) st_other(1) st_shndx(2) st_value(8) st_size(8) = 24 bytes
        // Local symbols must come before global symbols.

        let mut symtab: Vec<u8> = Vec::new();
        let mut sym_indices: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
        let mut sym_count = 0u32;

        // Null symbol
        symtab.extend_from_slice(&[0u8; 24]);
        sym_count += 1;

        // Section symbols (local)
        let emit_section_sym = |symtab: &mut Vec<u8>, section: u16| {
            w32(symtab, 0); // st_name = 0
            let info = (STB_LOCAL << 4) | STT_NOTYPE;
            symtab.push(info); // st_info
            symtab.push(STV_DEFAULT); // st_other
            w16(symtab, section); // st_shndx
            w64(symtab, 0); // st_value
            w64(symtab, 0); // st_size
        };
        emit_section_sym(&mut symtab, 1); sym_count += 1; // .text
        emit_section_sym(&mut symtab, 2); sym_count += 1; // .rodata
        emit_section_sym(&mut symtab, 3); sym_count += 1; // .data

        let num_locals = sym_count; // all locals done before globals

        // Global defined symbols
        for sym in &self.defined_symbols {
            sym_indices.insert(sym.name.clone(), sym_count);
            sym_count += 1;
            let name_idx = sym_name_idx.get(&sym.name).copied().unwrap_or(0);
            let typ = if sym.is_function { STT_FUNC } else { STT_NOTYPE };
            let bind = if sym.is_global { STB_GLOBAL } else { STB_LOCAL };
            let info = (bind << 4) | typ;
            let shndx = (sym.section + 1) as u16; // 1-based (1=.text, 2=.rodata, 3=.data)
            w32(&mut symtab, name_idx);
            symtab.push(info);
            symtab.push(STV_DEFAULT);
            w16(&mut symtab, shndx);
            w64(&mut symtab, sym.value);
            w64(&mut symtab, sym.size);
        }

        // External (undefined) symbols
        for name in &self.extern_symbols {
            sym_indices.insert(name.clone(), sym_count);
            sym_count += 1;
            let name_idx = sym_name_idx.get(name).copied().unwrap_or(0);
            let info = (STB_GLOBAL << 4) | STT_FUNC;
            w32(&mut symtab, name_idx);
            symtab.push(info);
            symtab.push(STV_DEFAULT);
            w16(&mut symtab, SHN_UNDEF); // undefined
            w64(&mut symtab, 0);
            w64(&mut symtab, 0);
        }

        // ── Rela.text ─────────────────────────────────────────────────────────
        // Elf64_Rela: r_offset(8) r_info(8) r_addend(8) = 24 bytes
        let mut rela_text: Vec<u8> = Vec::new();
        for r in &self.text_relocs {
            let sym_idx = sym_indices.get(&r.symbol).copied().unwrap_or(0) as u64;
            let r_info = (sym_idx << 32) | r.rela_type as u64;
            w64(&mut rela_text, r.offset);
            w64(&mut rela_text, r_info);
            w64i(&mut rela_text, r.addend);
        }

        // ── Layout ───────────────────────────────────────────────────────────
        // ELF header: 64 bytes
        // 8 section headers × 64 bytes = 512 bytes
        // Section data follows

        let elf_hdr_size = 64usize;
        let num_shdrs = 8usize;
        let shdr_size = 64usize;
        let data_start = elf_hdr_size + num_shdrs * shdr_size; // 576

        let text_off   = data_start;
        let rodata_off = text_off + self.text.len();
        let data_off   = rodata_off + self.rodata.len();
        let rela_off   = data_off + self.data.len();
        let symtab_off = rela_off + rela_text.len();
        let strtab_off = symtab_off + symtab.len();
        let shstrtab_off = strtab_off + strtab.len();

        // ── ELF Header ───────────────────────────────────────────────────────
        let mut out = Vec::new();
        out.extend_from_slice(ELFMAG);
        out.push(ELFCLASS64);    // EI_CLASS
        out.push(ELFDATA2LSB);   // EI_DATA
        out.push(EV_CURRENT as u8); // EI_VERSION
        out.push(0);             // EI_OSABI = System V
        out.extend_from_slice(&[0u8; 8]); // EI_ABIVERSION + padding
        w16(&mut out, ET_REL);          // e_type
        w16(&mut out, EM_X86_64);       // e_machine
        w32(&mut out, EV_CURRENT);      // e_version
        w64(&mut out, 0);               // e_entry
        w64(&mut out, 0);               // e_phoff (no program headers)
        w64(&mut out, elf_hdr_size as u64); // e_shoff
        w32(&mut out, 0);               // e_flags
        w16(&mut out, elf_hdr_size as u16); // e_ehsize
        w16(&mut out, 56);              // e_phentsize (not used)
        w16(&mut out, 0);               // e_phnum
        w16(&mut out, shdr_size as u16); // e_shentsize
        w16(&mut out, num_shdrs as u16); // e_shnum
        w16(&mut out, 7);               // e_shstrndx = index of .shstrtab

        assert_eq!(out.len(), elf_hdr_size);

        // ── Section headers ───────────────────────────────────────────────────
        // Elf64_Shdr: sh_name(4) sh_type(4) sh_flags(8) sh_addr(8) sh_offset(8)
        //             sh_size(8) sh_link(4) sh_info(4) sh_addralign(8) sh_entsize(8) = 64 bytes

        let write_shdr = |out: &mut Vec<u8>,
                          name: u32, typ: u32, flags: u64,
                          offset: usize, size: usize,
                          link: u32, info: u32,
                          align: u64, entsize: u64| {
            w32(out, name);
            w32(out, typ);
            w64(out, flags);
            w64(out, 0);  // sh_addr (0 for relocatable)
            w64(out, offset as u64);
            w64(out, size as u64);
            w32(out, link);
            w32(out, info);
            w64(out, align);
            w64(out, entsize);
        };

        // 0: NULL
        write_shdr(&mut out, sh_null_name, SHT_NULL, 0, 0, 0, 0, 0, 0, 0);
        // 1: .text
        write_shdr(&mut out, sh_text_name, SHT_PROGBITS,
            SHF_ALLOC | SHF_EXECINSTR,
            text_off, self.text.len(), 0, 0, 16, 0);
        // 2: .rodata
        write_shdr(&mut out, sh_rodata_name, SHT_PROGBITS,
            SHF_ALLOC,
            rodata_off, self.rodata.len(), 0, 0, 8, 0);
        // 3: .data
        write_shdr(&mut out, sh_data_name, SHT_PROGBITS,
            SHF_ALLOC | SHF_WRITE,
            data_off, self.data.len(), 0, 0, 8, 0);
        // 4: .rela.text (link=.symtab idx=5, info=.text idx=1)
        write_shdr(&mut out, sh_rela_name, SHT_RELA,
            SHF_ALLOC,
            rela_off, rela_text.len(), 5, 1, 8, 24);
        // 5: .symtab (link=.strtab idx=6, info=first_global)
        write_shdr(&mut out, sh_symtab_name, SHT_SYMTAB,
            0,
            symtab_off, symtab.len(), 6, num_locals, 8, 24);
        // 6: .strtab
        write_shdr(&mut out, sh_strtab_name, SHT_STRTAB,
            0,
            strtab_off, strtab.len(), 0, 0, 1, 0);
        // 7: .shstrtab
        write_shdr(&mut out, sh_shstrtab_name, SHT_STRTAB,
            0,
            shstrtab_off, shstrtab.len(), 0, 0, 1, 0);

        assert_eq!(out.len(), data_start);

        // ── Section data ──────────────────────────────────────────────────────
        out.extend_from_slice(&self.text);
        out.extend_from_slice(&self.rodata);
        out.extend_from_slice(&self.data);
        out.extend_from_slice(&rela_text);
        out.extend_from_slice(&symtab);
        out.extend_from_slice(&strtab);
        out.extend_from_slice(&shstrtab);

        out
    }
}

// ─── ELF64 Object Reader (for linker) ────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ElfSection {
    pub name: String,
    pub data: Vec<u8>,
    pub file_id: usize,
    pub section_index: usize,
}

#[derive(Debug, Clone)]
pub struct ElfSym {
    pub name: String,
    pub section_idx: usize, // 0 = undefined
    pub value: u64,
    pub is_global: bool,
}

#[derive(Debug, Clone)]
pub struct ElfRelaEntry {
    pub section_idx: usize,
    pub offset: u64,
    pub sym_idx: u32,
    pub rela_type: u32,
    pub addend: i64,
}

/// Parse an ELF64 object file
pub fn parse_elf(
    bytes: &[u8],
    file_id: usize,
) -> Result<(Vec<ElfSection>, Vec<ElfSym>, Vec<ElfRelaEntry>), String> {
    if bytes.len() < 64 { return Err("ELF too small".into()); }
    if &bytes[0..4] != ELFMAG { return Err("Not an ELF file".into()); }
    if bytes[4] != ELFCLASS64 { return Err("Not ELF64".into()); }

    let num_shdrs    = u16::from_le_bytes([bytes[60], bytes[61]]) as usize;
    let shoff        = u64::from_le_bytes(bytes[40..48].try_into().unwrap()) as usize;
    let shstrndx     = u16::from_le_bytes([bytes[62], bytes[63]]) as usize;

    if shoff == 0 || num_shdrs == 0 { return Ok((vec![], vec![], vec![])); }

    // Read all section headers
    let shdr_size = 64usize;
    let read_shdr = |i: usize| -> &[u8] {
        let off = shoff + i * shdr_size;
        &bytes[off..off + shdr_size]
    };

    // .shstrtab
    let shstrtab = {
        let sh = read_shdr(shstrndx);
        let off = u64::from_le_bytes(sh[24..32].try_into().unwrap()) as usize;
        let sz  = u64::from_le_bytes(sh[32..40].try_into().unwrap()) as usize;
        &bytes[off..off + sz]
    };

    let sh_name_str = |sh: &[u8]| -> String {
        let name_off = u32::from_le_bytes([sh[0], sh[1], sh[2], sh[3]]) as usize;
        let end = shstrtab[name_off..].iter().position(|&b| b == 0).unwrap_or(shstrtab.len() - name_off);
        String::from_utf8_lossy(&shstrtab[name_off..name_off + end]).into_owned()
    };

    // Find .symtab and .strtab
    let mut symtab_off = 0usize;
    let mut symtab_sz  = 0usize;
    let mut strtab_off = 0usize;
    let mut symtab_link = 0usize;

    for i in 0..num_shdrs {
        let sh = read_shdr(i);
        let typ = u32::from_le_bytes([sh[4], sh[5], sh[6], sh[7]]);
        if typ == SHT_SYMTAB {
            symtab_off  = u64::from_le_bytes(sh[24..32].try_into().unwrap()) as usize;
            symtab_sz   = u64::from_le_bytes(sh[32..40].try_into().unwrap()) as usize;
            symtab_link = u32::from_le_bytes([sh[40], sh[41], sh[42], sh[43]]) as usize;
        }
    }
    if symtab_link > 0 && symtab_link < num_shdrs {
        let sh = read_shdr(symtab_link);
        strtab_off = u64::from_le_bytes(sh[24..32].try_into().unwrap()) as usize;
    }

    let strtab_bytes = if strtab_off > 0 { &bytes[strtab_off..] } else { b"" };
    let read_sym_name = |off: u32| -> String {
        let off = off as usize;
        if off >= strtab_bytes.len() { return String::new(); }
        let end = strtab_bytes[off..].iter().position(|&b| b == 0).unwrap_or(0);
        String::from_utf8_lossy(&strtab_bytes[off..off + end]).into_owned()
    };

    // Read symbols
    let mut symbols: Vec<ElfSym> = Vec::new();
    let sym_ent = 24usize;
    let sym_count = symtab_sz / sym_ent;
    for i in 0..sym_count {
        let s = &bytes[symtab_off + i * sym_ent..symtab_off + (i+1) * sym_ent];
        let st_name   = u32::from_le_bytes([s[0], s[1], s[2], s[3]]);
        let st_info   = s[4];
        let st_shndx  = u16::from_le_bytes([s[6], s[7]]) as usize;
        let st_value  = u64::from_le_bytes(s[8..16].try_into().unwrap());
        let bind = st_info >> 4;
        let name = read_sym_name(st_name);
        if !name.is_empty() {
            symbols.push(ElfSym {
                name,
                section_idx: st_shndx,
                value: st_value,
                is_global: bind == STB_GLOBAL,
            });
        }
    }

    // Read sections and rela entries
    let mut sections = Vec::new();
    let mut rela_entries = Vec::new();

    for i in 0..num_shdrs {
        let sh = read_shdr(i);
        let typ = u32::from_le_bytes([sh[4], sh[5], sh[6], sh[7]]);
        let off = u64::from_le_bytes(sh[24..32].try_into().unwrap()) as usize;
        let sz  = u64::from_le_bytes(sh[32..40].try_into().unwrap()) as usize;
        let name = sh_name_str(sh);

        if typ == SHT_PROGBITS && sz > 0 {
            let data = if off + sz <= bytes.len() {
                bytes[off..off + sz].to_vec()
            } else {
                vec![0u8; sz]
            };
            sections.push(ElfSection { name, data, file_id, section_index: i });
        } else if typ == SHT_RELA && sz > 0 {
            // Find which section this rela applies to (sh_info)
            let applied_to = u32::from_le_bytes([sh[44], sh[45], sh[46], sh[47]]) as usize;
            let rela_ent = 24usize;
            let count = sz / rela_ent;
            for j in 0..count {
                let r = &bytes[off + j * rela_ent..off + (j+1) * rela_ent];
                let r_offset = u64::from_le_bytes(r[0..8].try_into().unwrap());
                let r_info   = u64::from_le_bytes(r[8..16].try_into().unwrap());
                let r_addend = i64::from_le_bytes(r[16..24].try_into().unwrap());
                let sym_idx  = (r_info >> 32) as u32;
                let rela_type = (r_info & 0xFFFF_FFFF) as u32;
                rela_entries.push(ElfRelaEntry {
                    section_idx: applied_to,
                    offset: r_offset,
                    sym_idx,
                    rela_type,
                    addend: r_addend,
                });
            }
        }
    }

    Ok((sections, symbols, rela_entries))
}
