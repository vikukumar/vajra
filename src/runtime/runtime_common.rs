#[no_mangle]
pub unsafe extern "C" fn memset(s: *mut u8, c: i32, n: usize) -> *mut u8 {
    for i in 0..n {
        *s.add(i) = c as u8;
    }
    s
}

#[no_mangle]
pub unsafe extern "C" fn memcpy(dest: *mut u8, src: *const u8, n: usize) -> *mut u8 {
    for i in 0..n {
        *dest.add(i) = *src.add(i);
    }
    dest
}

#[no_mangle]
pub unsafe extern "C" fn memcmp(s1: *const u8, s2: *const u8, n: usize) -> i32 {
    for i in 0..n {
        let a = *s1.add(i);
        let b = *s2.add(i);
        if a != b {
            return if a < b { -1 } else { 1 };
        }
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn strlen(s: *const u8) -> usize {
    let mut len = 0;
    while *s.add(len) != 0 {
        len += 1;
    }
    len
}

const MAX_THREADS: usize = 16;
const HEAP_SIZE: usize = 128 * 1024 * 1024; // 128 MB per thread
const MAX_BIGINT_LIMBS: usize = 1000;

static mut G_THREAD_IDS: [u64; MAX_THREADS] = [0; MAX_THREADS];
static mut G_HEAP_STARTS: [*mut u8; MAX_THREADS] = [core::ptr::null_mut(); MAX_THREADS];
static mut G_HEAP_BUMPS: [usize; MAX_THREADS] = [0; MAX_THREADS];
static mut G_HEAP_LIMITS: [usize; MAX_THREADS] = [0; MAX_THREADS];
static mut G_STACK_BASES: [*mut u8; MAX_THREADS] = [core::ptr::null_mut(); MAX_THREADS];
static mut G_VERBOSE: bool = false;

#[repr(C)]
struct AllocHeader {
    size: u32,
    marked: u32,
}

unsafe fn print_i64_nn(val: i64) {
    let mut buf = [0u8; 32];
    let mut ptr = buf.len();
    let is_neg = val < 0;
    let mut abs_val = if is_neg {
        if val == i64::MIN {
            print_raw(b"-9223372036854775808".as_ptr(), 20);
            return;
        }
        (-val) as u64
    } else {
        val as u64
    };

    let buf_ptr = buf.as_mut_ptr();
    if abs_val == 0 {
        ptr -= 1;
        *buf_ptr.add(ptr) = b'0';
    } else {
        while abs_val > 0 {
            ptr -= 1;
            *buf_ptr.add(ptr) = b'0' + (abs_val % 10) as u8;
            abs_val /= 10;
        }
    }
    if is_neg {
        ptr -= 1;
        *buf_ptr.add(ptr) = b'-';
    }
    print_raw(buf_ptr.add(ptr), buf.len() - ptr);
}

unsafe fn print_bigint_nn(b: *const BigInt) {
    if b.is_null() {
        print_raw(b"null".as_ptr(), 4);
        return;
    }
    if (*b).len == 0 {
        print_raw(b"0".as_ptr(), 1);
        return;
    }
    if (*b).sign < 0 {
        print_raw(b"-".as_ptr(), 1);
    }
    
    // Print the most significant digit (without leading zeros)
    let len = (*b).len as usize;
    print_i64_nn(*(*b).digits.add(len - 1) as i64);
    
    // Print subsequent digits with zero padding (9 digits per limb)
    for i in (0..(len - 1)).rev() {
        let digit = *(*b).digits.add(i);
        let mut buf = [b'0'; 9];
        let buf_ptr = buf.as_mut_ptr();
        let mut temp = digit;
        let mut ptr = 9;
        while temp > 0 && ptr > 0 {
            ptr -= 1;
            *buf_ptr.add(ptr) = b'0' + (temp % 10) as u8;
            temp /= 10;
        }
        print_raw(buf_ptr, 9);
    }
}

#[no_mangle]
pub unsafe extern "C" fn vajra_print_i64(val: i64) {
    let uval = val as u64;
    if is_tagged(uval) {
        print_i64_nn(untag(uval));
    } else {
        let b = uval as *const BigInt;
        print_bigint_nn(b);
    }
}

fn f64_is_nan(v: f64) -> bool {
    let bits = v.to_bits();
    (bits & 0x7FF0000000000000) == 0x7FF0000000000000 && (bits & 0x000FFFFFFFFFFFFF) != 0
}

fn f64_is_infinite(v: f64) -> bool {
    let bits = v.to_bits();
    (bits & 0x7FF0000000000000) == 0x7FF0000000000000 && (bits & 0x000FFFFFFFFFFFFF) == 0
}

#[no_mangle]
pub unsafe extern "C" fn vajra_print_f64(val: f64) {
    let mut v = val;
    if f64_is_nan(v) {
        print_raw(b"NaN".as_ptr(), 3);
        return;
    }
    if f64_is_infinite(v) {
        if v < 0.0 {
            print_raw(b"-inf".as_ptr(), 4);
        } else {
            print_raw(b"inf".as_ptr(), 3);
        }
        return;
    }
    if v < 0.0 {
        print_raw(b"-".as_ptr(), 1);
        v = -v;
    }
    let ipart = v as i64;
    print_i64_nn(ipart);
    print_raw(b".".as_ptr(), 1);
    let mut frac = v - (ipart as f64);
    for _ in 0..6 {
        frac *= 10.0;
        let digit = frac as i64;
        print_i64_nn(digit % 10);
        frac -= digit as f64;
    }
}

#[no_mangle]
pub unsafe extern "C" fn vajra_strlen(s: *const u8) -> usize {
    let mut len = 0;
    while *s.add(len) != 0 {
        len += 1;
    }
    len
}

#[no_mangle]
pub unsafe extern "C" fn vajra_print_str(s: *const u8) {
    if s.is_null() {
        print_raw(b"null".as_ptr(), 4);
        print_raw(EOL.as_ptr(), EOL.len());
        return;
    }
    let len = vajra_strlen(s);
    print_raw(s, len);
    print_raw(EOL.as_ptr(), EOL.len());
}

unsafe fn is_heap_ptr(val: u64) -> bool {
    let ptr = val as *mut u8;
    for i in 0..MAX_THREADS {
        let start = G_HEAP_STARTS[i];
        let bump = G_HEAP_BUMPS[i];
        if !start.is_null() && ptr >= start && ptr < start.add(bump) {
            return true;
        }
    }
    false
}

unsafe fn is_valid_image_ptr(ptr: *const u8) -> bool {
    let ref_addr = vajra_runtime_init as usize as i64;
    let ptr_addr = ptr as usize as i64;
    let diff = ptr_addr - ref_addr;
    diff >= -16_777_216 && diff <= 16_777_216
}

unsafe fn is_valid_class_name_ptr(ptr: *const u8) -> bool {
    if ptr.is_null() || is_heap_ptr(ptr as u64) || !is_valid_image_ptr(ptr) {
        return false;
    }
    let mut len = 0;
    while len < 128 {
        let c = *ptr.add(len);
        if c == 0 {
            return len > 0;
        }
        if !((c >= b'a' && c <= b'z') || (c >= b'A' && c <= b'Z') || (c >= b'0' && c <= b'9') || c == b'_') {
            return false;
        }
        len += 1;
    }
    false
}


#[no_mangle]
pub unsafe extern "C" fn vajra_print_auto(val: u64) {
    if G_VERBOSE {
        print_raw(b"DBG print_auto: val=".as_ptr(), 20);
        print_i64_nn(val as i64);
        print_raw(EOL.as_ptr(), EOL.len());
    }

    if val == 0 {
        print_raw(b"null".as_ptr(), 4);
        return;
    }
    if is_tagged(val) {
        print_i64_nn(untag(val));
        return;
    }
    
    // If it's not on the heap, it's a string literal or global variable
    if !is_heap_ptr(val) {
        let len = vajra_strlen(val as *const u8);
        print_raw(val as *const u8, len);
        return;
    }
    
    let p = val as *const u64;
    let first = *p;
    if first == 1 || first == 1_i64 as u64 || first == -1_i64 as u64 {
        let b = val as *const BigInt;
        print_bigint_nn(b);
        return;
    }
    
    if is_valid_class_name_ptr(first as *const u8) {
        print_raw(b"[Object ".as_ptr(), 8);
        let len = vajra_strlen(first as *const u8);
        print_raw(first as *const u8, len);
        print_raw(b"]".as_ptr(), 1);
        return;
    }
    
    let len = vajra_strlen(val as *const u8);
    print_raw(val as *const u8, len);
}

#[no_mangle]
pub unsafe extern "C" fn vajra_throw(msg: *const u8) -> ! {
    print_raw(b"Exception: ".as_ptr(), 11);
    vajra_print_str(msg);
    vajra_exit(1);
}

#[no_mangle]
pub unsafe extern "C" fn vajra_spawn(
    _func: extern "C" fn(*mut c_void) -> u32,
    _arg: *mut c_void,
) -> *mut c_void {
    core::ptr::null_mut()
}

#[no_mangle]
pub unsafe extern "C" fn vajra_spawn_val(
    _func: extern "C" fn(*mut c_void) -> u32,
    _arg: *mut c_void,
) -> *mut c_void {
    core::ptr::null_mut()
}

#[no_mangle]
pub unsafe extern "C" fn vajra_readline() -> *mut u8 {
    const MAX_LINE: usize = 4096;
    let buf = vajra_alloc(MAX_LINE + 1);
    let mut n = read_raw(buf, MAX_LINE);
    while n > 0 {
        let c = *buf.add(n - 1);
        if c == b'\n' || c == b'\r' {
            n -= 1;
        } else {
            break;
        }
    }
    *buf.add(n) = 0;
    buf
}

#[no_mangle]
pub unsafe extern "C" fn vajra_alloc(size: usize) -> *mut u8 {
    if G_VERBOSE {
        print_raw(b"DBG: vajra_alloc size=\0".as_ptr(), 22);
        print_i64_nn(size as i64);
        print_raw(EOL.as_ptr(), EOL.len());
    }
    let Some(size) = size.checked_add(7) else {
        vajra_throw(b"Allocation size overflow\0".as_ptr());
    };
    let size = size & !7;
    let Some(total_size) = size.checked_add(8) else {
        vajra_throw(b"Allocation size overflow\0".as_ptr());
    };
    if total_size > HEAP_SIZE {
        vajra_throw(b"Allocation exceeds heap limit\0".as_ptr());
    }

    let idx = get_thread_idx();
    let bump = *G_HEAP_BUMPS.as_ptr().add(idx);
    let limit = *G_HEAP_LIMITS.as_ptr().add(idx);
    let start = *G_HEAP_STARTS.as_ptr().add(idx);

    if start.is_null() {
        vajra_throw(b"Heap allocation failure\0".as_ptr());
    }

    if bump + total_size <= limit {
        let ptr = start.add(bump);
        let header = ptr as *mut AllocHeader;
        (*header).size = size as u32;
        (*header).marked = 0;
        *G_HEAP_BUMPS.as_mut_ptr().add(idx) = bump + total_size;
        return ptr.add(8);
    }

    gc_collect(idx);

    let bump = *G_HEAP_BUMPS.as_ptr().add(idx);
    if bump + total_size <= limit {
        let ptr = start.add(bump);
        let header = ptr as *mut AllocHeader;
        (*header).size = size as u32;
        (*header).marked = 0;
        *G_HEAP_BUMPS.as_mut_ptr().add(idx) = bump + total_size;
        return ptr.add(8);
    }

    vajra_throw(b"Out of memory\0".as_ptr());
}

#[no_mangle]
pub unsafe extern "C" fn vajra_free(_ptr: *mut u8) {
    // GC is automatic
}

unsafe fn mark_block(idx: usize, ptr: *mut u8) -> bool {
    let heap_start = *G_HEAP_STARTS.as_ptr().add(idx);
    let heap_bump = *G_HEAP_BUMPS.as_ptr().add(idx);

    let mut offset = 0;
    while offset < heap_bump {
        let header = heap_start.add(offset) as *mut AllocHeader;
        let block_size = (*header).size as usize;
        let block_start = heap_start.add(offset + 8);
        let block_end = block_start.add(block_size);

        if ptr >= block_start && ptr < block_end {
            if (*header).marked == 0 {
                (*header).marked = 1;
                return true;
            }
            return false;
        }
        offset += block_size + 8;
    }
    false
}

unsafe fn gc_collect(idx: usize) {
    let heap_start = *G_HEAP_STARTS.as_ptr().add(idx);
    let heap_bump = *G_HEAP_BUMPS.as_ptr().add(idx);

    // 1. Clear marks
    let mut offset = 0;
    while offset < heap_bump {
        let header = heap_start.add(offset) as *mut AllocHeader;
        (*header).marked = 0;
        offset += (*header).size as usize + 8;
    }

    // Spill non-volatile registers to stack before scanning
    let mut regs = [0usize; 7];
    #[cfg(target_arch = "x86_64")]
    core::arch::asm!(
        "mov [{0}], rbx",
        "mov [{0} + 8], rsi",
        "mov [{0} + 16], rdi",
        "mov [{0} + 24], r12",
        "mov [{0} + 32], r13",
        "mov [{0} + 40], r14",
        "mov [{0} + 48], r15",
        in(reg) regs.as_mut_ptr(),
    );

    // 2. Scan stack
    let mut rsp: *mut usize = core::ptr::null_mut();
    #[cfg(target_arch = "x86_64")]
    core::arch::asm!("mov {}, rsp", out(reg) rsp);
    let stack_base = *G_STACK_BASES.as_ptr().add(idx) as *mut usize;

    if !stack_base.is_null() && rsp < stack_base {
        let mut cur = rsp;
        while cur < stack_base {
            let val = *cur;
            let ptr_val = val as *mut u8;
            if ptr_val >= heap_start && ptr_val < heap_start.add(heap_bump) {
                mark_block(idx, ptr_val);
            }
            cur = cur.add(1);
        }
    }

    // 3. Transitive mark
    let mut changed = true;
    while changed {
        changed = false;
        let mut offset = 0;
        while offset < heap_bump {
            let header = heap_start.add(offset) as *mut AllocHeader;
            if (*header).marked == 1 {
                let size = (*header).size as usize;
                let payload = heap_start.add(offset + 8) as *mut usize;
                let num_words = size / 8;
                for i in 0..num_words {
                    let val = *payload.add(i);
                    let ptr_val = val as *mut u8;
                    if ptr_val >= heap_start && ptr_val < heap_start.add(heap_bump) {
                        if mark_block(idx, ptr_val) {
                            changed = true;
                        }
                    }
                }
            }
            offset += (*header).size as usize + 8;
        }
    }

    // 4. Sweep (high-watermark)
    let mut max_live_end = 0;
    let mut offset = 0;
    while offset < heap_bump {
        let header = heap_start.add(offset) as *mut AllocHeader;
        let block_size = (*header).size as usize + 8;
        if (*header).marked == 1 {
            max_live_end = offset + block_size;
        }
        offset += block_size;
    }

    *G_HEAP_BUMPS.as_mut_ptr().add(idx) = max_live_end;
}

struct ThreadArg {
    start: i64,
    end: i64,
    context: *mut c_void,
    loop_body: extern "C" fn(i64, i64, *mut c_void),
    counter: *const AtomicI32,
}

// --- BigInt Implementation ---

#[repr(C)]
struct BigInt {
    sign: i64,
    len: i64,
    digits: *mut u64,
}

unsafe fn alloc_bigint(len: usize) -> *mut BigInt {
    if len > MAX_BIGINT_LIMBS {
        vajra_throw(b"BigInt too large\0".as_ptr());
    }
    let b = vajra_alloc(core::mem::size_of::<BigInt>()) as *mut BigInt;
    if len > 0 {
        let d = vajra_alloc(len * core::mem::size_of::<u64>()) as *mut u64;
        (*b).digits = d;
    } else {
        (*b).digits = core::ptr::null_mut();
    }
    (*b).len = len as i64;
    (*b).sign = 1;
    b
}

unsafe fn bigint_normalize(b: *mut BigInt) {
    let mut len = (*b).len as usize;
    while len > 0 && *(*b).digits.add(len - 1) == 0 {
        len -= 1;
    }
    (*b).len = len as i64;
    if len == 0 {
        (*b).sign = 1;
    }
}

unsafe fn bigint_from_i64(val: i64) -> *mut BigInt {
    if val == 0 {
        let b = alloc_bigint(0);
        (*b).sign = 1;
        return b;
    }
    let sign = if val < 0 { -1 } else { 1 };
    let abs_val = if val < 0 {
        if val == i64::MIN {
            9223372036854775808u64
        } else {
            (-val) as u64
        }
    } else {
        val as u64
    };
    
    let mut temp = abs_val;
    let mut len = 0;
    while temp > 0 {
        len += 1;
        temp /= 1_000_000_000;
    }
    
    let b = alloc_bigint(len);
    (*b).sign = sign;
    let mut temp = abs_val;
    for i in 0..len {
        *((*b).digits.add(i)) = temp % 1_000_000_000;
        temp /= 1_000_000_000;
    }
    b
}

unsafe fn bigint_to_i64(b: *const BigInt) -> Option<i64> {
    if (*b).len == 0 {
        return Some(0);
    }
    if (*b).len > 3 {
        return None;
    }
    let mut val = 0u64;
    let mut mul = 1u64;
    for i in 0..((*b).len as usize) {
        let digit = *(*b).digits.add(i);
        if i > 0 {
            mul = mul.checked_mul(1_000_000_000)?;
        }
        let term = digit.checked_mul(mul)?;
        val = val.checked_add(term)?;
    }
    if (*b).sign < 0 {
        if val > (i64::MIN as u64).wrapping_neg() {
            None
        } else {
            Some(-(val as i64))
        }
    } else {
        if val > i64::MAX as u64 {
            None
        } else {
            Some(val as i64)
        }
    }
}

unsafe fn bigint_cmp_abs(a: *const BigInt, b: *const BigInt) -> i32 {
    if (*a).len != (*b).len {
        return if (*a).len < (*b).len { -1 } else { 1 };
    }
    let len = (*a).len as usize;
    for i in (0..len).rev() {
        let da = *(*a).digits.add(i);
        let db = *(*b).digits.add(i);
        if da != db {
            return if da < db { -1 } else { 1 };
        }
    }
    0
}

unsafe fn bigint_add_abs(a: *const BigInt, b: *const BigInt) -> *mut BigInt {
    let len_a = (*a).len as usize;
    let len_b = (*b).len as usize;
    let max_len = if len_a > len_b { len_a } else { len_b };
    if max_len >= MAX_BIGINT_LIMBS {
        vajra_throw(b"BigInt addition exceeds limit\0".as_ptr());
    }
    let res = alloc_bigint(max_len + 1);
    
    let mut carry = 0u64;
    for i in 0..max_len {
        let da = if i < len_a { *(*a).digits.add(i) } else { 0 };
        let db = if i < len_b { *(*b).digits.add(i) } else { 0 };
        let sum = da + db + carry;
        *(*res).digits.add(i) = sum % 1_000_000_000;
        carry = sum / 1_000_000_000;
    }
    if carry > 0 {
        *(*res).digits.add(max_len) = carry;
    }
    bigint_normalize(res);
    res
}

unsafe fn bigint_sub_abs(a: *const BigInt, b: *const BigInt) -> *mut BigInt {
    let len_a = (*a).len as usize;
    let len_b = (*b).len as usize;
    let res = alloc_bigint(len_a);
    
    let mut borrow = 0i64;
    for i in 0..len_a {
        let da = *(*a).digits.add(i) as i64;
        let db = if i < len_b { *(*b).digits.add(i) as i64 } else { 0 };
        let mut diff = da - db - borrow;
        if diff < 0 {
            diff += 1_000_000_000;
            borrow = 1;
        } else {
            borrow = 0;
        }
        *(*res).digits.add(i) = diff as u64;
    }
    bigint_normalize(res);
    res
}

unsafe fn bigint_add(a: *const BigInt, b: *const BigInt) -> *mut BigInt {
    if (*a).sign == (*b).sign {
        let res = bigint_add_abs(a, b);
        (*res).sign = (*a).sign;
        res
    } else {
        let cmp = bigint_cmp_abs(a, b);
        if cmp == 0 {
            alloc_bigint(0)
        } else if cmp > 0 {
            let res = bigint_sub_abs(a, b);
            (*res).sign = (*a).sign;
            res
        } else {
            let res = bigint_sub_abs(b, a);
            (*res).sign = (*b).sign;
            res
        }
    }
}

unsafe fn bigint_sub(a: *const BigInt, b: *const BigInt) -> *mut BigInt {
    if (*a).sign != (*b).sign {
        let res = bigint_add_abs(a, b);
        (*res).sign = (*a).sign;
        res
    } else {
        let cmp = bigint_cmp_abs(a, b);
        if cmp == 0 {
            alloc_bigint(0)
        } else if cmp > 0 {
            let res = bigint_sub_abs(a, b);
            (*res).sign = (*a).sign;
            res
        } else {
            let res = bigint_sub_abs(b, a);
            (*res).sign = -(*a).sign;
            res
        }
    }
}

unsafe fn bigint_mul(a: *const BigInt, b: *const BigInt) -> *mut BigInt {
    let len_a = (*a).len as usize;
    let len_b = (*b).len as usize;
    if len_a == 0 || len_b == 0 {
        return alloc_bigint(0);
    }
    let Some(result_len) = len_a.checked_add(len_b) else {
        vajra_throw(b"BigInt multiplication size overflow\0".as_ptr());
    };
    if result_len > MAX_BIGINT_LIMBS {
        vajra_throw(b"BigInt multiplication exceeds limit\0".as_ptr());
    }
    let res = alloc_bigint(result_len);
    (*res).sign = if (*a).sign == (*b).sign { 1 } else { -1 };
    
    // Clear digits
    for i in 0..result_len {
        *(*res).digits.add(i) = 0;
    }
    
    for i in 0..len_a {
        let da = *(*a).digits.add(i);
        let mut carry = 0u64;
        for j in 0..len_b {
            let db = *(*b).digits.add(j);
            let prod = da.checked_mul(db).unwrap_or(0);
            let slot = (*res).digits.add(i + j);
            let sum = *slot + prod + carry;
            *slot = sum % 1_000_000_000;
            carry = sum / 1_000_000_000;
        }
        if carry > 0 {
            *(*res).digits.add(i + len_b) += carry;
        }
    }
    bigint_normalize(res);
    res
}

unsafe fn bigint_div_rem(a: *const BigInt, b: *const BigInt) -> (*mut BigInt, *mut BigInt) {
    let len_b = (*b).len as usize;
    if len_b == 0 {
        vajra_throw(b"Division by zero\0".as_ptr());
    }
    let cmp = bigint_cmp_abs(a, b);
    if cmp < 0 {
        let q = alloc_bigint(0);
        let r = alloc_bigint((*a).len as usize);
        (*r).sign = (*a).sign;
        for i in 0..((*a).len as usize) {
            *(*r).digits.add(i) = *(*a).digits.add(i);
        }
        bigint_normalize(r);
        return (q, r);
    }
    if cmp == 0 {
        let q = bigint_from_i64(1);
        (*q).sign = if (*a).sign == (*b).sign { 1 } else { -1 };
        return (q, alloc_bigint(0));
    }
    
    // Schoolbook division: simplified for now by single digit trial if simple,
    // otherwise fallback to simple trial subtraction.
    let len_a = (*a).len as usize;
    let q = alloc_bigint(len_a);
    (*q).sign = if (*a).sign == (*b).sign { 1 } else { -1 };
    
    let mut temp = alloc_bigint(len_a);
    (*temp).sign = 1;
    for i in 0..len_a {
        *(*temp).digits.add(i) = *(*a).digits.add(i);
    }
    bigint_normalize(temp);
    
    let mut r = temp;
    let b_abs = alloc_bigint(len_b);
    for i in 0..len_b {
        *(*b_abs).digits.add(i) = *(*b).digits.add(i);
    }
    bigint_normalize(b_abs);
    
    let mut q_count = 0u64;
    while bigint_cmp_abs(r, b_abs) >= 0 {
        let next_r = bigint_sub_abs(r, b_abs);
        r = next_r;
        q_count += 1;
    }
    
    let q_res = bigint_from_i64(q_count as i64);
    (*q_res).sign = (*q).sign;
    (*r).sign = (*a).sign;
    
    (q_res, r)
}

unsafe fn bigint_cmp(a: *const BigInt, b: *const BigInt) -> i64 {
    if (*a).sign != (*b).sign {
        return if (*a).sign < (*b).sign { -1 } else { 1 };
    }
    let cmp = bigint_cmp_abs(a, b);
    if (*a).sign < 0 {
        -cmp as i64
    } else {
        cmp as i64
    }
}

unsafe fn cmp_i64_bigint(a: i64, b: *const BigInt) -> i64 {
    if (*b).len == 0 {
        return if a < 0 { -1 } else if a > 0 { 1 } else { 0 };
    }
    if a < 0 && (*b).sign > 0 {
        return -1;
    }
    if a >= 0 && (*b).sign < 0 {
        return 1;
    }
    if (*b).len > 3 {
        return if (*b).sign > 0 { -1 } else { 1 };
    }
    if let Some(b_val) = bigint_to_i64(b) {
        return if a < b_val { -1 } else if a > b_val { 1 } else { 0 };
    }
    if (*b).sign > 0 { -1 } else { 1 }
}

// --- Runtime Helper Functions ---

#[no_mangle]
pub unsafe extern "C" fn vajra_bigint_from_digits(
    digits_ptr: *const u64,
    len: i64,
    sign: i64,
) -> *mut BigInt {
    if G_VERBOSE {
        print_raw(b"DBG: vajra_bigint_from_digits len=\0".as_ptr(), 33);
        print_i64_nn(len);
        print_raw(b" sign=\0".as_ptr(), 6);
        print_i64_nn(sign);
        print_raw(EOL.as_ptr(), EOL.len());
    }
    if len < 0 || len as usize > MAX_BIGINT_LIMBS {
        vajra_throw(b"BigInt literal exceeds limit\0".as_ptr());
    }
    let b = alloc_bigint(len as usize);
    (*b).sign = sign;
    for i in 0..(len as usize) {
        *((*b).digits.add(i)) = *digits_ptr.add(i);
    }
    b
}

#[no_mangle]
pub unsafe extern "C" fn vajra_pow(a: u64, b: u64) -> u64 {
    if is_tagged(a) && is_tagged(b) {
        let va = untag(a);
        let vb = untag(b);
        if vb >= 0 {
            if let Some(res) = va.checked_pow(vb as u32) {
                if res >= -4611686018427387904 && res <= 4611686018427387903 {
                    return tag(res);
                }
            }
        }
    }
    
    let bigint_a = if is_tagged(a) {
        bigint_from_i64(untag(a))
    } else {
        a as *mut BigInt
    };
    
    let power = if is_tagged(b) {
        untag(b)
    } else {
        bigint_to_i64(b as *const BigInt).unwrap_or(0)
    };
    
    if power < 0 {
        return tag(0);
    }
    
    let mut res = bigint_from_i64(1);
    let mut base = bigint_a;
    let mut exp = power;
    while exp > 0 {
        if exp & 1 == 1 {
            res = bigint_mul(res, base);
        }
        base = bigint_mul(base, base);
        exp >>= 1;
    }
    res as u64
}

#[no_mangle]
pub unsafe extern "C" fn vajra_add(a: u64, b: u64) -> u64 {
    if is_tagged(a) && is_tagged(b) {
        let va = untag(a);
        let vb = untag(b);
        if let Some(res) = va.checked_add(vb) {
            if res >= -4611686018427387904 && res <= 4611686018427387903 {
                return tag(res);
            }
        }
    }
    let bigint_a = if is_tagged(a) {
        bigint_from_i64(untag(a))
    } else {
        a as *mut BigInt
    };
    let bigint_b = if is_tagged(b) {
        bigint_from_i64(untag(b))
    } else {
        b as *mut BigInt
    };
    let res = bigint_add(bigint_a, bigint_b);
    if let Some(val) = bigint_to_i64(res) {
        if val >= -4611686018427387904 && val <= 4611686018427387903 {
            return tag(val);
        }
    }
    res as u64
}

#[no_mangle]
pub unsafe extern "C" fn vajra_sub(a: u64, b: u64) -> u64 {
    if is_tagged(a) && is_tagged(b) {
        let va = untag(a);
        let vb = untag(b);
        if let Some(res) = va.checked_sub(vb) {
            if res >= -4611686018427387904 && res <= 4611686018427387903 {
                return tag(res);
            }
        }
    }
    let bigint_a = if is_tagged(a) {
        bigint_from_i64(untag(a))
    } else {
        a as *mut BigInt
    };
    let bigint_b = if is_tagged(b) {
        bigint_from_i64(untag(b))
    } else {
        b as *mut BigInt
    };
    let res = bigint_sub(bigint_a, bigint_b);
    if let Some(val) = bigint_to_i64(res) {
        if val >= -4611686018427387904 && val <= 4611686018427387903 {
            return tag(val);
        }
    }
    res as u64
}

#[no_mangle]
pub unsafe extern "C" fn vajra_mul(a: u64, b: u64) -> u64 {
    if is_tagged(a) && is_tagged(b) {
        let va = untag(a);
        let vb = untag(b);
        if let Some(res) = va.checked_mul(vb) {
            if res >= -4611686018427387904 && res <= 4611686018427387903 {
                return tag(res);
            }
        }
    }
    let bigint_a = if is_tagged(a) {
        bigint_from_i64(untag(a))
    } else {
        a as *mut BigInt
    };
    let bigint_b = if is_tagged(b) {
        bigint_from_i64(untag(b))
    } else {
        b as *mut BigInt
    };
    let res = bigint_mul(bigint_a, bigint_b);
    if let Some(val) = bigint_to_i64(res) {
        if val >= -4611686018427387904 && val <= 4611686018427387903 {
            return tag(val);
        }
    }
    res as u64
}

#[no_mangle]
pub unsafe extern "C" fn vajra_div(a: u64, b: u64) -> u64 {
    if is_tagged(a) && is_tagged(b) {
        let va = untag(a);
        let vb = untag(b);
        if vb != 0 {
            if let Some(res) = va.checked_div(vb) {
                if res >= -4611686018427387904 && res <= 4611686018427387903 {
                    return tag(res);
                }
            }
        }
    }
    let bigint_a = if is_tagged(a) {
        bigint_from_i64(untag(a))
    } else {
        a as *mut BigInt
    };
    let bigint_b = if is_tagged(b) {
        bigint_from_i64(untag(b))
    } else {
        b as *mut BigInt
    };
    let (q, _r) = bigint_div_rem(bigint_a, bigint_b);
    if let Some(val) = bigint_to_i64(q) {
        if val >= -4611686018427387904 && val <= 4611686018427387903 {
            return tag(val);
        }
    }
    q as u64
}

#[no_mangle]
pub unsafe extern "C" fn vajra_rem(a: u64, b: u64) -> u64 {
    if is_tagged(a) && is_tagged(b) {
        let va = untag(a);
        let vb = untag(b);
        if vb != 0 {
            if let Some(res) = va.checked_rem(vb) {
                if res >= -4611686018427387904 && res <= 4611686018427387903 {
                    return tag(res);
                }
            }
        }
    }
    let bigint_a = if is_tagged(a) {
        bigint_from_i64(untag(a))
    } else {
        a as *mut BigInt
    };
    let bigint_b = if is_tagged(b) {
        bigint_from_i64(untag(b))
    } else {
        b as *mut BigInt
    };
    let (_q, r) = bigint_div_rem(bigint_a, bigint_b);
    if let Some(val) = bigint_to_i64(r) {
        if val >= -4611686018427387904 && val <= 4611686018427387903 {
            return tag(val);
        }
    }
    r as u64
}

unsafe fn is_valid_bigint(val: u64) -> bool {
    if !is_heap_ptr(val) {
        return false;
    }
    if (val & 7) != 0 {
        return false;
    }
    let header = (val - 8) as *const AllocHeader;
    if (*header).size < core::mem::size_of::<BigInt>() as u32 {
        return false;
    }
    let first = *(val as *const u64) as i64;
    first == 1 || first == -1
}

unsafe fn get_string_ptr(val: u64) -> *const u8 {
    if val == 0 {
        return core::ptr::null();
    }
    if is_heap_ptr(val) {
        let header = (val - 8) as *const AllocHeader;
        if (*header).size >= 8 {
            let first = *(val as *const u64);
            if is_valid_class_name_ptr(first as *const u8) {
                return core::ptr::null();
            }
        }
        return val as *const u8;
    }
    val as *const u8
}

#[no_mangle]
pub unsafe extern "C" fn vajra_cmp(a: u64, b: u64) -> i64 {
    if a == b {
        return 0;
    }
    if is_tagged(a) && is_tagged(b) {
        let va = untag(a);
        let vb = untag(b);
        if va < vb { -1 } else if va > vb { 1 } else { 0 }
    } else {
        let is_a_num = is_tagged(a) || is_valid_bigint(a);
        let is_b_num = is_tagged(b) || is_valid_bigint(b);
        
        if is_a_num && is_b_num {
            if is_tagged(a) && is_tagged(b) {
                let va = untag(a);
                let vb = untag(b);
                if va < vb { -1 } else if va > vb { 1 } else { 0 }
            } else if is_tagged(a) {
                cmp_i64_bigint(untag(a), b as *const BigInt)
            } else if is_tagged(b) {
                -cmp_i64_bigint(untag(b), a as *const BigInt)
            } else {
                bigint_cmp(a as *const BigInt, b as *const BigInt)
            }
        } else {
            if a == 0 || b == 0 {
                return if a == 0 { -1 } else { 1 };
            }
            let str_a = get_string_ptr(a);
            let str_b = get_string_ptr(b);
            if str_a.is_null() || str_b.is_null() {
                if a < b { -1 } else { 1 }
            } else {
                let mut i = 0;
                loop {
                    let ca = *str_a.add(i);
                    let cb = *str_b.add(i);
                    if ca != cb {
                        return if ca < cb { -1 } else { 1 };
                    }
                    if ca == 0 {
                        break;
                    }
                    i += 1;
                }
                0
            }
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn abs(n: i64) -> i64 {
    if is_tagged(n as u64) {
        let val = untag(n as u64);
        let abs_val = if val < 0 {
            if val == i64::MIN {
                i64::MAX
            } else {
                -val
            }
        } else {
            val
        };
        tag(abs_val) as i64
    } else {
        if n < 0 {
            if n == i64::MIN {
                i64::MAX
            } else {
                -n
            }
        } else {
            n
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn vajra_not(a: u64) -> u64 {
    if is_tagged(a) {
        let val = untag(a);
        if val == 0 { 3 } else { 1 }
    } else {
        1
    }
}

#[no_mangle]
pub unsafe extern "C" fn vajra_and(a: u64, b: u64) -> u64 {
    let val_a = if is_tagged(a) { untag(a) != 0 } else { true };
    let val_b = if is_tagged(b) { untag(b) != 0 } else { true };
    if val_a && val_b { 3 } else { 1 }
}

#[no_mangle]
pub unsafe extern "C" fn vajra_or(a: u64, b: u64) -> u64 {
    let val_a = if is_tagged(a) { untag(a) != 0 } else { true };
    let val_b = if is_tagged(b) { untag(b) != 0 } else { true };
    if val_a || val_b { 3 } else { 1 }
}

#[no_mangle]
pub unsafe extern "C" fn vajra_untag(a: u64) -> i64 {
    if is_tagged(a) {
        untag(a)
    } else {
        let b = a as *const BigInt;
        if b.is_null() || (*b).len == 0 {
            return 0;
        }
        let mut val = 0u64;
        let mut mul = 1u64;
        for i in 0..core::cmp::min((*b).len as usize, 3) {
            let digit = *(*b).digits.add(i);
            val = val.wrapping_add(digit.wrapping_mul(mul));
            mul = mul.wrapping_mul(1_000_000_000);
        }
        if (*b).sign < 0 {
            -(val as i64)
        } else {
            val as i64
        }
    }
}

#[repr(C)]
struct timespec {
    tv_sec: i64,
    tv_nsec: i64,
}

unsafe fn parse_ipv4_to_bytes(ip_str: *const u8) -> [u8; 4] {
    let mut bytes = [0u8; 4];
    let bytes_ptr = bytes.as_mut_ptr();
    let mut current_val = 0u8;
    let mut byte_idx = 0;
    let mut i = 0;
    loop {
        let c = *ip_str.add(i);
        if c == 0 {
            if byte_idx < 4 {
                *bytes_ptr.add(byte_idx) = current_val;
            }
            break;
        } else if c == b'.' {
            if byte_idx < 4 {
                *bytes_ptr.add(byte_idx) = current_val;
                byte_idx += 1;
                current_val = 0;
            }
        } else if c >= b'0' && c <= b'9' {
            current_val = current_val.wrapping_mul(10).wrapping_add(c - b'0');
        }
        i += 1;
    }
    bytes
}

#[no_mangle]
pub unsafe extern "C" fn vajra_socket_create() -> u64 {
    #[cfg(target_os = "windows")]
    {
        ensure_winsock();
        if let Some(socket_fn) = G_SOCKET {
            let sock = socket_fn(2, 1, 6);
            if sock == usize::MAX {
                return tag(-1);
            }
            return tag(sock as i64);
        }
        tag(-1)
    }
    #[cfg(not(target_os = "windows"))]
    {
        #[cfg(target_arch = "x86_64")]
        {
            let sock = sys_socket(2, 1, 6);
            tag(sock as i64)
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            tag(-1)
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn vajra_socket_connect(sock_val: u64, ip_val: u64, port_val: u64) -> u64 {
    let sock = untag(sock_val) as usize;
    let ip_ptr = ip_val as *const u8;
    let port = untag(port_val) as u32;

    let bytes = parse_ipv4_to_bytes(ip_ptr);

    let mut addr = [0u8; 16];
    let family_val = 2u16;
    core::ptr::copy_nonoverlapping(&family_val as *const u16 as *const u8, addr.as_mut_ptr(), 2);
    let port_bytes = [(port >> 8) as u8, (port & 0xFF) as u8];
    core::ptr::copy_nonoverlapping(port_bytes.as_ptr(), addr.as_mut_ptr().add(2), 2);
    core::ptr::copy_nonoverlapping(bytes.as_ptr(), addr.as_mut_ptr().add(4), 4);

    #[cfg(target_os = "windows")]
    {
        ensure_winsock();
        if let Some(connect_fn) = G_CONNECT {
            let res = connect_fn(sock, addr.as_ptr(), 16);
            return tag(res as i64);
        }
        tag(-1)
    }
    #[cfg(not(target_os = "windows"))]
    {
        #[cfg(target_arch = "x86_64")]
        {
            let res = sys_connect(sock as isize, addr.as_ptr(), 16);
            tag(res as i64)
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            tag(-1)
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn vajra_socket_send(sock_val: u64, data_val: u64) -> u64 {
    let sock = untag(sock_val) as usize;
    let data_ptr = data_val as *const u8;
    let len = vajra_strlen(data_ptr);

    #[cfg(target_os = "windows")]
    {
        ensure_winsock();
        if let Some(send_fn) = G_SEND {
            let res = send_fn(sock, data_ptr, len as i32, 0);
            return tag(res as i64);
        }
        tag(-1)
    }
    #[cfg(not(target_os = "windows"))]
    {
        #[cfg(target_arch = "x86_64")]
        {
            let res = sys_send(sock as isize, data_ptr, len, 0);
            tag(res as i64)
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            tag(-1)
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn vajra_socket_recv(sock_val: u64, len_val: u64) -> u64 {
    let sock = untag(sock_val) as usize;
    let len = untag(len_val) as usize;

    let buf = vajra_alloc(len + 1);
    let mut bytes_received: isize = -1;

    #[cfg(target_os = "windows")]
    {
        ensure_winsock();
        if let Some(recv_fn) = G_RECV {
            let res = recv_fn(sock, buf, len as i32, 0);
            bytes_received = res as isize;
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        #[cfg(target_arch = "x86_64")]
        {
            bytes_received = sys_recv(sock as isize, buf, len, 0);
        }
    }

    if bytes_received <= 0 {
        *buf = 0;
    } else {
        *buf.add(bytes_received as usize) = 0;
    }
    buf as u64
}

#[no_mangle]
pub unsafe extern "C" fn vajra_socket_close(sock_val: u64) -> u64 {
    let sock = untag(sock_val) as usize;

    #[cfg(target_os = "windows")]
    {
        ensure_winsock();
        if let Some(close_fn) = G_CLOSESOCKET {
            let res = close_fn(sock);
            return tag(res as i64);
        }
        tag(-1)
    }
    #[cfg(not(target_os = "windows"))]
    {
        #[cfg(target_arch = "x86_64")]
        {
            let res = sys_close(sock as isize);
            tag(res as i64)
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            tag(-1)
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn vajra_datetime_now() -> u64 {
    #[cfg(target_os = "windows")]
    {
        ensure_datetime();
        if let Some(sys_time_fn) = G_GET_SYSTEM_TIME_AS_FILE_TIME {
            let mut file_time = 0u64;
            sys_time_fn(&mut file_time);
            let unix_seconds = (file_time - 116444736000000000) / 10000000;
            return tag(unix_seconds as i64);
        }
        tag(0)
    }
    #[cfg(not(target_os = "windows"))]
    {
        #[cfg(target_arch = "x86_64")]
        {
            let mut tp = timespec { tv_sec: 0, tv_nsec: 0 };
            sys_clock_gettime(0, &mut tp);
            tag(tp.tv_sec)
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            tag(0)
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn vajra_datetime_epoch() -> u64 {
    vajra_datetime_now()
}

// --- Math and Random ---

#[no_mangle]
pub unsafe extern "C" fn vajra_math_sin(x: f64) -> f64 {
    let pi = 3.141592653589793;
    let mut z = x % (2.0 * pi);
    if z < -pi { z += 2.0 * pi; }
    if z > pi { z -= 2.0 * pi; }
    if z < -pi / 2.0 {
        z = -pi - z;
    } else if z > pi / 2.0 {
        z = pi - z;
    }
    let z2 = z * z;
    z * (1.0 - z2 * (1.0 / 6.0 - z2 * (1.0 / 120.0 - z2 * (1.0 / 5040.0 - z2 / 362880.0))))
}

#[no_mangle]
pub unsafe extern "C" fn vajra_math_cos(x: f64) -> f64 {
    let pi = 3.141592653589793;
    vajra_math_sin(x + pi / 2.0)
}

#[no_mangle]
pub unsafe extern "C" fn vajra_math_tan(x: f64) -> f64 {
    let s = vajra_math_sin(x);
    let c = vajra_math_cos(x);
    if c.abs() < 1e-15 { 0.0 } else { s / c }
}

#[no_mangle]
pub unsafe extern "C" fn vajra_math_sqrt(x: f64) -> f64 {
    let ret: f64;
    #[cfg(target_arch = "x86_64")]
    core::arch::asm!(
        "sqrtsd {}, {}",
        out(xmm_reg) ret,
        in(xmm_reg) x
    );
    #[cfg(not(target_arch = "x86_64"))]
    { ret = x; }
    ret
}

#[no_mangle]
pub unsafe extern "C" fn vajra_math_abs(x: f64) -> f64 {
    if x < 0.0 { -x } else { x }
}

unsafe fn vajra_math_ln(x: f64) -> f64 {
    if x <= 0.0 { return -1e300; }
    let mut val = x;
    let mut k = 0.0;
    while val > 1.5 {
        val /= 2.718281828459045;
        k += 1.0;
    }
    while val < 0.5 {
        val *= 2.718281828459045;
        k -= 1.0;
    }
    let z = val - 1.0;
    let z2 = z * z;
    let z3 = z2 * z;
    let z4 = z3 * z;
    let z5 = z4 * z;
    let approx = z - z2/2.0 + z3/3.0 - z4/4.0 + z5/5.0;
    approx + k
}

unsafe fn vajra_math_exp(x: f64) -> f64 {
    let mut sum = 1.0;
    let mut term = 1.0;
    for i in 1..20 {
        term *= x / i as f64;
        sum += term;
    }
    sum
}

#[no_mangle]
pub unsafe extern "C" fn vajra_math_log(x: f64) -> f64 {
    vajra_math_ln(x)
}

#[no_mangle]
pub unsafe extern "C" fn vajra_math_pow(x: f64, y: f64) -> f64 {
    let y_int = y as i32;
    if (y - y_int as f64).abs() < 1e-9 {
        let mut res = 1.0;
        let mut base = x;
        let mut exp = if y_int < 0 { -y_int } else { y_int };
        while exp > 0 {
            if exp & 1 == 1 {
                res *= base;
            }
            base *= base;
            exp >>= 1;
        }
        if y_int < 0 { 1.0 / res } else { res }
    } else {
        if x <= 0.0 { return 0.0; }
        let lnx = vajra_math_ln(x);
        vajra_math_exp(y * lnx)
    }
}

static mut G_RANDOM_STATE: u64 = 0x123456789abcdef;

#[no_mangle]
pub unsafe extern "C" fn vajra_random_int(min: i64, max: i64) -> i64 {
    let mut x = G_RANDOM_STATE;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    G_RANDOM_STATE = x;
    if min >= max { return min; }
    let range = (max - min) as u64;
    let val = x % range;
    min + val as i64
}

#[no_mangle]
pub unsafe extern "C" fn vajra_random_float() -> f64 {
    let mut x = G_RANDOM_STATE;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    G_RANDOM_STATE = x;
    (x & 0xFFFFFFFFFFFF) as f64 / 281474976710655.0
}

#[no_mangle]
pub unsafe extern "C" fn fmod(x: f64, y: f64) -> f64 {
    if y == 0.0 {
        return f64::NAN;
    }
    let quot = (x / y) as i64;
    x - (quot as f64) * y
}

// --- Tagging Helpers ---

#[inline(always)]
fn is_tagged(val: u64) -> bool {
    (val & 1) == 1
}

#[inline(always)]
fn tag(val: i64) -> u64 {
    ((val as u64) << 1) | 1
}

#[inline(always)]
fn untag(val: u64) -> i64 {
    (val as i64) >> 1
}
