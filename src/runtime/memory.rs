/// Vajra Runtime Memory — Bump allocator + simple GC
/// For compiled programs, actual implementation is in runtime/mod.rs as x86-64 machine code.

/// Simple bump allocator (Rust-side, used for REPL and tests)
pub struct BumpAllocator {
    heap: Vec<u8>,
    offset: usize,
}

impl BumpAllocator {
    pub fn new(size: usize) -> Self {
        Self { heap: vec![0u8; size], offset: 0 }
    }

    pub fn alloc(&mut self, size: usize) -> Option<*mut u8> {
        let aligned = (size + 7) & !7; // 8-byte align
        if self.offset + aligned > self.heap.len() {
            return None;
        }
        let ptr = unsafe { self.heap.as_mut_ptr().add(self.offset) };
        self.offset += aligned;
        Some(ptr)
    }

    pub fn reset(&mut self) {
        self.offset = 0;
    }
}
