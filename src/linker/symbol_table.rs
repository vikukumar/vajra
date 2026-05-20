/// Symbol Table — resolves function/data references between object files and runtime
use std::collections::HashMap;
use anyhow::Result;

#[derive(Debug, Clone, PartialEq)]
pub enum SymbolKind {
    Function,
    Data,
    Undefined,
}

#[derive(Debug, Clone)]
pub struct SymbolEntry {
    pub name: String,
    pub kind: SymbolKind,
    /// Virtual address (set after layout)
    pub va: u64,
    /// Section index (0 = undefined, 1 = .text, 2 = .rdata, etc.)
    pub section: u32,
    /// Offset within section
    pub offset: u64,
    pub size: u64,
    pub is_extern: bool,
}

pub struct SymbolTable {
    symbols: HashMap<String, SymbolEntry>,
}

impl SymbolTable {
    pub fn new() -> Self {
        Self { symbols: HashMap::new() }
    }

    pub fn add(&mut self, entry: SymbolEntry) {
        self.symbols.insert(entry.name.clone(), entry);
    }

    pub fn get(&self, name: &str) -> Option<&SymbolEntry> {
        self.symbols.get(name)
    }

    pub fn get_va(&self, name: &str) -> Option<u64> {
        self.symbols.get(name).map(|s| s.va)
    }

    pub fn resolve_all(&self, names: &[String]) -> Result<()> {
        for name in names {
            if self.symbols.get(name).map(|s| s.kind == SymbolKind::Undefined).unwrap_or(true) {
                anyhow::bail!("Undefined symbol: '{}'", name);
            }
        }
        Ok(())
    }

    pub fn iter(&self) -> impl Iterator<Item = &SymbolEntry> {
        self.symbols.values()
    }
}
