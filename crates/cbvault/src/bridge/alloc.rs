//! A counting global allocator, so "the hot path allocates nothing" is a
//! measurement rather than a claim.
//!
//! The counter is armed per thread and per window, so a test can measure one
//! call and be sure that nothing another test does on another thread lands in
//! the number. `cargo test` runs tests in parallel; without that, every test in
//! the crate would be counted by whichever one is measuring.
//!
//! The arming flag is a `thread_local` with a `const` initialiser and no
//! destructor, so reading it from inside the allocator cannot allocate — which
//! is the one way an allocator like this normally deadlocks.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

thread_local! {
    /// Whether this thread is counting right now.
    static ARMED: Cell<bool> = const { Cell::new(false) };
    /// How many allocations this thread has made since it was armed.
    static COUNT: Cell<u64> = const { Cell::new(0) };
    /// Set while the allocator is inside itself, so a re-entrant call does not
    /// count itself.
    static INSIDE: Cell<bool> = const { Cell::new(false) };
}

/// The allocator: the system one, counting on the way through.
pub struct Counting;

#[allow(unsafe_code)]
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count();
        // SAFETY: `layout` is handed to the system allocator unchanged.
        #[allow(unsafe_code)]
        unsafe {
            System.alloc(layout)
        }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count();
        // SAFETY: `layout` is handed to the system allocator unchanged.
        #[allow(unsafe_code)]
        unsafe {
            System.alloc_zeroed(layout)
        }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        count();
        // SAFETY: the pointer and layout are handed to the system allocator unchanged.
        #[allow(unsafe_code)]
        unsafe {
            System.realloc(ptr, layout, new_size)
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: the pointer and layout are handed to the system allocator unchanged.
        #[allow(unsafe_code)]
        unsafe {
            System.dealloc(ptr, layout)
        }
    }
}

/// Counts one allocation, if this thread is armed and is not already inside the
/// allocator.
#[inline]
fn count() {
    let armed = ARMED.try_with(Cell::get).unwrap_or(false);
    if !armed || INSIDE.try_with(Cell::get).unwrap_or(true) {
        return;
    }
    let _ = INSIDE.try_with(|inside| inside.set(true));
    let _ = COUNT.try_with(|count| count.set(count.get() + 1));
    let _ = INSIDE.try_with(|inside| inside.set(false));
}

/// Starts counting on this thread.
pub fn arm() {
    let _ = COUNT.try_with(|count| count.set(0));
    let _ = ARMED.try_with(|armed| armed.set(true));
}

/// Stops counting on this thread and answers how many allocations it saw.
pub fn disarm() -> u64 {
    let _ = ARMED.try_with(|armed| armed.set(false));
    COUNT.try_with(Cell::get).unwrap_or(0)
}

/// Runs `f` with the allocator counting, and answers how many allocations it
/// made. Nested calls are not supported: the inner `disarm` would stop the
/// outer count.
pub fn counting<T>(f: impl FnOnce() -> T) -> (T, u64) {
    arm();
    let out = f();
    (out, disarm())
}
