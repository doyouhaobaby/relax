//! The OS key holding each thread's thread-local block (jit.rs rt_tls_block), and the fast
//! path compiled code reads it through (`Inst::TlsAddr`: `Env::tls_key`).
//!
//! - `key_create(dtor)`: one key per process; `dtor(block)` runs at thread exit.
//! - `get(key)` / `set(key, block)`: the calling thread's block.
//! - `jit_offset(key, emit)`: the value of `Env::tls_key` for this OS (`emit` places glue code
//!   and returns its address): Linux x86-64 the fs-relative offset of a static-TLS cell, macOS
//!   the TSD slot (checked against pthread_getspecific), Android bionic's TLS_SLOT_APP.
//! - `static_tls_*`: Linux x86-64's reserve of static TLS for programs' `#[thread_local]` statics.

include!("../rt_abi.rs");
pub use imp::*;

/// `relax_tls_fixed`'s question for the answering runtime's own nesting depth under Relax (no cell
/// is at this address): a runtime asks the one that runs it, so a refusal can say how deep it is.
pub const DEPTH_QUERY: u64 = u64::MAX;

#[cfg(unix)]
mod unix {
    extern "C" {
        pub fn pthread_key_create(key: *mut usize, dtor: Option<extern "C" fn(*mut u8)>) -> i32;
        pub fn pthread_getspecific(key: usize) -> *mut u8;
        pub fn pthread_setspecific(key: usize, v: *mut u8) -> i32;
    }

    pub fn key_create(dtor: extern "C" fn(*mut u8)) -> u64 {
        // pthread_key_t: unsigned long on macOS, unsigned int on Linux (zero-extended here)
        let mut key: usize = 0;
        unsafe {
            pthread_key_create(&mut key, Some(dtor));
        }
        #[cfg(any(target_os = "linux", target_os = "android"))]
        let key = key as u32 as usize;
        key as u64
    }

    pub fn get(key: u64) -> *mut u8 {
        unsafe { pthread_getspecific(key as usize) }
    }
}

#[cfg(target_os = "linux")]
mod imp {
    pub use super::unix::{get, key_create};

    thread_local! {
        /// this thread's block (static TLS: JIT code reads it at fs:[env.tls_key])
        static TLS_BLOCK: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    }

    pub fn set(key: u64, b: *mut u8) {
        unsafe {
            super::unix::pthread_setspecific(key as usize, b);
        }
        TLS_BLOCK.with(|c| c.set(b as u64));
    }

    /// Linux x86-64: the block pointer lives in a static-TLS cell of this (executable)
    /// runtime, at a fixed offset from the thread pointer that JIT code reads via fs.
    pub fn jit_offset(_key: u64, emit: &dyn Fn(&[u8]) -> u64) -> u64 {
        #[cfg(target_arch = "x86_64")]
        let reader = emit(&relax_backend::x64::fs_reader());
        #[cfg(not(target_arch = "x86_64"))]
        let reader: u64 = compile_error!("the JIT does not build for Linux on this CPU: its thread-local fast path has only the x86-64 (fs) reader on Linux");
        let fs_base: rt_fn_ty!(fn() -> u64) = unsafe { std::mem::transmute(reader as usize) };
        let cell = TLS_BLOCK.with(|c| c as *const std::cell::Cell<u64> as u64);
        let off = cell.wrapping_sub(fs_base()) as i64;
        if off < i32::MIN as i64 || off > i32::MAX as i64 || !static_cell(cell) {
            // a plain error: nothing this run does moves the cell
            eprintln!("{}", cell_refusal(super::fixed::depth(), off));
            std::process::exit(2);
        }
        off as u64
    }

    /// What a runtime says when JIT code cannot reach its thread-local block cell at a fixed
    /// offset from the thread pointer: `depth` is how deep under Relax it runs (1: the loader,
    /// 2: a runtime the loader runs; 0: the runtime above did not say), `off` the cell's offset.
    pub(super) fn cell_refusal(depth: u32, off: i64) -> String {
        let deep = match depth {
            1 => return format!("relax: this runtime's thread-local block cell is {off:#x} from the thread pointer, out of the reach of JIT code"),
            2 => return "relax: this runtime runs under Relax, and the static TLS the outer runtime hands out was used up before it placed this runtime's thread-local block cell, so JIT code cannot reach the cell".to_string(),
            0 => "more than one level deep".to_string(),
            d => format!("{d} levels deep"),
        };
        format!("relax: this runtime runs {deep} under Relax (the loader is level 1, a runtime it runs level 2). On Linux x86-64 JIT code reaches its thread's block through a cell at a fixed offset from the thread pointer, and only the loader hands such cells out, to the runtime it runs; Relax nested deeper than that is not supported. Run this program under the loader directly.")
    }

    /// Under Relax the cell is the outer program's thread-local: in the outer runtime's static TLS
    /// when it placed it there (`__relax_tls_fixed`), else in a block of this thread only.
    fn static_cell(cell: u64) -> bool {
        #[cfg(relax)]
        {
            extern "C" {
                fn __relax_tls_fixed(addr: u64) -> u32;
            }
            unsafe { __relax_tls_fixed(cell) != 0 }
        }
        #[cfg(not(relax))]
        {
            let _ = cell;
            true
        }
    }
}

/// Linux x86-64: the executable's static TLS that JIT programs' `#[thread_local]` statics share
/// (jit.rs Unit::thread_local_place, `Inst::TlsStatic`): `RESERVE` bytes at one offset from the
/// thread pointer on every thread (the executable's own static TLS, as `TLS_BLOCK`), zero when a
/// thread starts (its TLS image is .tbss). Handed out once per process, never freed: a variable
/// keeps its offset for the process. Not when this runtime itself runs under Relax: its
/// thread-locals are the outer program's, which need not be at a fixed offset.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod fixed {
    use std::cell::Cell;
    use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

    const RESERVE: usize = 16 << 10;
    /// 64-aligned: the thread pointer is aligned to the executable's TLS alignment, so a variable
    /// aligned at most that within the reserve is aligned in memory
    #[repr(C, align(64))]
    struct Reserve([Cell<u64>; RESERVE / 8]);
    thread_local! {
        static PROGRAM_TLS: Reserve = const { Reserve([const { Cell::new(0) }; RESERVE / 8]) };
    }
    static BASE: std::sync::OnceLock<Option<i32>> = std::sync::OnceLock::new();
    static NEXT: AtomicU32 = AtomicU32::new(0);

    fn reserve() -> *const Cell<u64> {
        PROGRAM_TLS.with(|r| r.0.as_ptr())
    }

    pub fn static_tls_init(emit: &dyn Fn(&[u8]) -> u64) {
        BASE.get_or_init(|| {
            if cfg!(relax) {
                return None;
            }
            let reader = emit(&relax_backend::x64::fs_reader());
            let fs_base: rt_fn_ty!(fn() -> u64) = unsafe { std::mem::transmute(reader as usize) };
            FS_BASE.store(reader, Ordering::Release);
            let off = (reserve() as u64).wrapping_sub(fs_base()) as i64;
            (off >= i32::MIN as i64 && off + RESERVE as i64 <= i32::MAX as i64).then_some(off as i32)
        });
    }

    pub fn static_tls_base() -> Option<i32> {
        BASE.get().copied().flatten()
    }

    static FS_BASE: AtomicU64 = AtomicU64::new(0);

    /// For a runtime running under this one (`__relax_tls_fixed`, os/tls.rs static_cell):
    /// whether `addr` is a variable this runtime placed in static TLS (as seen from the calling
    /// thread), i.e. at the same offset from the thread pointer on every thread.
    pub extern "C" fn relax_tls_fixed(addr: u64) -> u32 {
        if addr == super::DEPTH_QUERY {
            return depth();
        }
        let (Some(base), r) = (static_tls_base(), FS_BASE.load(Ordering::Acquire)) else { return 0 };
        if r == 0 {
            return 0;
        }
        let fs_base: rt_fn_ty!(fn() -> u64) = unsafe { std::mem::transmute(r as usize) };
        let off = addr.wrapping_sub(fs_base()) as i64;
        (off >= base as i64 && off < base as i64 + NEXT.load(Ordering::Acquire) as i64) as u32
    }

    /// How deep under Relax this runtime runs: 1 natively (the loader), else one more than the
    /// runtime that runs it says (`relax_tls_fixed(DEPTH_QUERY)`); 0 when that one does not say.
    pub fn depth() -> u32 {
        #[cfg(relax)]
        {
            extern "C" {
                fn __relax_tls_fixed(addr: u64) -> u32;
            }
            match unsafe { __relax_tls_fixed(super::DEPTH_QUERY) } {
                0 => 0,
                d => d + 1,
            }
        }
        #[cfg(not(relax))]
        {
            1
        }
    }

    pub fn static_tls_alloc(size: u32, align: u32) -> Option<i32> {
        let base = static_tls_base()?;
        if align > 64 {
            return None;
        }
        let mut cur = NEXT.load(Ordering::Relaxed);
        loop {
            let at = (cur + align - 1) / align * align;
            if at as usize + size as usize > RESERVE {
                return None;
            }
            match NEXT.compare_exchange(cur, at + size, Ordering::AcqRel, Ordering::Relaxed) {
                Ok(_) => return Some(base + at as i32),
                Err(c) => cur = c,
            }
        }
    }

    /// What this module holds of a run, as a fresh process has it (`run_end`). Nothing of the run is left that could see it.
    pub(crate) unsafe fn reset_statics() {
        use relax_session::reset_static as reset;
        // (the reader is code of the ended run; the offsets start over, jit.rs `fresh_thread` gives a thread that stays a clean reserve)
        reset(&FS_BASE, AtomicU64::new(0));
        reset(&BASE, std::sync::OnceLock::new());
        reset(&NEXT, AtomicU32::new(0));
    }

    /// The calling thread's words are the ended run's: they go.
    pub fn static_tls_clear() {
        let r = reserve();
        for k in 0..RESERVE / 8 {
            unsafe { (*r.add(k)).set(0) };
        }
    }

    /// Exchanges the calling thread's static TLS words in use with `saved` (empty: zeros), so a
    /// scoped thread-local block (jit.rs domain_callback) starts from fresh values and the
    /// thread's own come back after it.
    pub fn static_tls_swap(saved: &mut Vec<u64>) {
        if static_tls_base().is_none() {
            return;
        }
        let n = (NEXT.load(Ordering::Acquire) as usize + 7) / 8;
        saved.resize(n, 0);
        let r = reserve();
        for (k, w) in saved.iter_mut().enumerate() {
            *w = unsafe { (*r.add(k)).replace(*w) };
        }
    }
}
#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
mod fixed {
    pub fn static_tls_init(_emit: &dyn Fn(&[u8]) -> u64) {}
    pub fn static_tls_base() -> Option<i32> {
        None
    }
    pub fn static_tls_alloc(_size: u32, _align: u32) -> Option<i32> {
        None
    }
    pub fn static_tls_swap(_saved: &mut Vec<u64>) {}
    pub fn static_tls_clear() {}
    pub extern "C" fn relax_tls_fixed(_addr: u64) -> u32 {
        0
    }

    pub(crate) unsafe fn reset_statics() {}
}
pub use fixed::*;

#[cfg(target_os = "android")]
mod imp {
    pub use super::unix::{get, key_create};

    /// bionic TLS_SLOT_APP: word 2 of the slot array tpidr_el0 points at, "available for use by
    /// apps in API level 29 and later" (bionic libc/platform/bionic/tls_defines.h; our minSdk is
    /// 30). The process is ours (one makepad run host), so Relax owns the slot. The pthread key
    /// stays for its thread-exit destructor; JIT code reads the slot (codegen arm64 tls_addr).
    const TLS_SLOT_APP: u64 = 2;

    fn slot() -> *mut u64 {
        let tp: u64;
        unsafe { core::arch::asm!("mrs {}, tpidr_el0", out(reg) tp, options(nomem, nostack, preserves_flags)); }
        (tp as *mut u64).wrapping_add(TLS_SLOT_APP as usize)
    }

    pub fn set(key: u64, block: *mut u8) {
        unsafe {
            super::unix::pthread_setspecific(key as usize, block);
            slot().write_volatile(block as u64);
        }
    }

    /// The slot index JIT code reads (`Env::tls_key`), checked through the emitted reader.
    pub fn jit_offset(key: u64, emit: &dyn Fn(&[u8]) -> u64) -> u64 {
        let reader = emit(&relax_backend::arm64::tsd_reader());
        let read: extern "C" fn(u64) -> u64 = unsafe { std::mem::transmute(reader as usize) };
        let old = get(key);
        set(key, 0x5a5a_0000 as *mut u8);
        let seen = read(TLS_SLOT_APP);
        set(key, old);
        if seen != 0x5a5a_0000 {
            panic!("thread-local fast path: bionic TLS_SLOT_APP reads {:#x}", seen);
        }
        TLS_SLOT_APP
    }
}

#[cfg(target_vendor = "apple")]
mod imp {
    pub use super::unix::{get, key_create};
    use super::unix::pthread_setspecific;

    pub fn set(key: u64, b: *mut u8) {
        unsafe {
            pthread_setspecific(key as usize, b);
        }
    }

    /// The inline fast path reads the TSD slot directly: check it sees pthread's value.
    pub fn jit_offset(key: u64, emit: &dyn Fn(&[u8]) -> u64) -> u64 {
        #[cfg(target_arch = "aarch64")]
        let reader = emit(&relax_backend::arm64::tsd_reader());
        #[cfg(not(target_arch = "aarch64"))]
        let reader: u64 = compile_error!("the JIT does not build for x86-64 Apple targets: its thread-local fast path has no x86-64 Apple TLS reader (Apple hosts are AArch64)");
        let read: extern "C" fn(u64) -> u64 = unsafe { std::mem::transmute(reader as usize) };
        let old = get(key);
        set(key, 0x5a5a_0000 as *mut u8);
        let seen = read(key);
        set(key, old);
        if seen != 0x5a5a_0000 {
            panic!("thread-local fast path: TSD slot {} reads {:#x}", key, seen);
        }
        key
    }
}

/// Windows: a TlsAlloc index, any of the 1088; compiled code reads the word TlsGetValue reads
/// with two loads (relax_backend::x64::win_tls_key): the TEB's TlsSlots for indexes below 64,
/// its TlsExpansionSlots array above (a process with GPU drivers loaded has used up the 64 direct
/// slots before Relax creates its key). The OS has no per-key destructor: a host thread-local
/// guard frees the block at thread exit.
#[cfg(windows)]
mod imp {
    use crate::os::win32::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static DTOR: AtomicUsize = AtomicUsize::new(0);

    struct Guard(std::cell::Cell<(u64, usize)>);
    impl Drop for Guard {
        fn drop(&mut self) {
            let (key, b) = self.0.get();
            let d = DTOR.load(Ordering::Acquire);
            if b != 0 && d != 0 {
                let dtor: extern "C" fn(*mut u8) = unsafe { std::mem::transmute(d) };
                unsafe { TlsSetValue(key as u32, std::ptr::null_mut()) };
                dtor(b as *mut u8);
            }
        }
    }
    thread_local! {
        static GUARD: Guard = const { Guard(std::cell::Cell::new((0, 0))) };
    }

    pub fn key_create(dtor: extern "C" fn(*mut u8)) -> u64 {
        DTOR.store(dtor as *const () as usize, Ordering::Release);
        let k = unsafe { TlsAlloc() };
        if k == u32::MAX {
            panic!("TlsAlloc failed");
        }
        k as u64
    }

    pub fn get(key: u64) -> *mut u8 {
        unsafe { TlsGetValue(key as u32) }
    }

    pub fn set(key: u64, b: *mut u8) {
        unsafe {
            TlsSetValue(key as u32, b);
        }
        let _ = GUARD.try_with(|g| g.0.set((key, b as usize)));
    }

    pub fn jit_offset(key: u64, emit: &dyn Fn(&[u8]) -> u64) -> u64 {
        let Some(off) = relax_backend::x64::win_tls_key(key) else {
            panic!("thread-local fast path: TlsAlloc index {} is past the TEB's 1088 slots", key);
        };
        let reader = emit(&relax_backend::x64::win_tls_reader());
        let read: rt_fn_ty!(fn(u64) -> u64) = unsafe { std::mem::transmute(reader as usize) };
        let old = get(key);
        unsafe { TlsSetValue(key as u32, 0x5a5a_0000 as *mut u8) };
        let seen = read(off);
        unsafe { TlsSetValue(key as u32, old) };
        if seen != 0x5a5a_0000 {
            panic!("thread-local fast path: index {} (gs:[{:#x}] + {:#x}) reads {:#x}", key, off >> 32, off as u32, seen);
        }
        off
    }
}

/// What this module holds of a run, as a fresh process has it (`run_end`). Nothing of the run is left that could see it.
pub(crate) unsafe fn reset_statics() {
    fixed::reset_statics();
}

#[cfg(all(test, target_os = "linux", target_arch = "x86_64"))]
mod tests {
    /// A runtime whose thread-local block cell JIT code cannot reach stops with a plain error that
    /// says how deep under Relax it runs, and a runtime learns its depth from the one that runs it:
    /// natively the test is the loader (1); under Relax one more than its loader says (0 while that
    /// loader is a build from before the depth question).
    #[test]
    fn a_nested_runtime_knows_its_depth_and_says_it() {
        let deep = super::imp::cell_refusal(3, 0);
        assert!(deep.starts_with("relax: this runtime runs 3 levels deep under Relax") && deep.contains("not supported"), "the refusal names the depth: {deep}");
        assert!(super::imp::cell_refusal(0, 0).contains("more than one level deep"), "an unknown depth is said as such");
        assert!(super::imp::cell_refusal(2, 0).contains("used up"), "one level deep the cause is the outer runtime's full reserve");
        let d = super::fixed::depth();
        if cfg!(relax) {
            assert!(d == 2 || d == 0, "a runtime under the loader is level 2, got {d}");
        } else {
            assert_eq!(d, 1, "a native runtime is the loader");
        }
    }
}
