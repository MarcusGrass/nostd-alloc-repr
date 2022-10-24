#![no_std]
#![no_main]
#![feature(naked_functions)]
#![feature(default_alloc_error_handler)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use core::alloc::{GlobalAlloc, Layout};
use core::cell::UnsafeCell;
use dlmalloc::Dlmalloc;
use unix_print::{unix_println};

#[global_allocator]
static ALLOCATOR: SingleThreadedAlloc = SingleThreadedAlloc::new();

struct SingleThreadedAlloc {
    inner: UnsafeCell<Dlmalloc>,
}

impl SingleThreadedAlloc {
    pub(crate) const fn new() -> Self {
        SingleThreadedAlloc {
            inner: UnsafeCell::new(Dlmalloc::new())
        }
    }
}

unsafe impl GlobalAlloc for SingleThreadedAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        (*self.inner.get()).malloc(layout.size(), layout.align())
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        (*self.inner.get()).free(ptr, layout.size(), layout.align())
    }
}

/// Extremely unsafe, this program is not thread safe at all will immediately segfault on more threads
unsafe impl Sync for SingleThreadedAlloc {}
unsafe impl Send for SingleThreadedAlloc {}

#[panic_handler]
fn on_panic(_info: &core::panic::PanicInfo) -> !{
    unsafe {exit(1)}
}

#[no_mangle]
#[naked]
unsafe extern "C" fn _start() {
    core::arch::asm!("mov rdi, rsp", "call main", options(noreturn))
}

#[derive(Debug, Copy, Clone)]
pub struct ArgIter<'a> {
    args: &'a [*const u8],
    it: usize,
}

impl<'a> Iterator for ArgIter<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        if self.it < self.args.len() {
            let arg = &self.args[self.it];
            // Safety: Assumes all args are terminated by a null-byte, or else we'll start reading out of bounds
            let len = unsafe {strlen(*arg)};

            // Safety: Assumes arg is a correct pointer and that the len we got from strlen is fine
            let v: Vec<u8> = unsafe {Vec::from_raw_parts(*arg as _, len, len)};
            let s = String::from_utf8(v)
                .expect("Args not utf8");
            self.it += 1;
            // Just extending the lifetime
            let with_lifetime: &'a str = unsafe {core::mem::transmute(s.as_str())};
            let out = Some(with_lifetime);
            core::mem::forget(s);
            out

        } else {
            None
        }
    }
}

unsafe fn strlen(mut s: *const u8) -> usize {
    let mut count = 0;
    while *s != b'\0' {
        count += 1;
        s = s.add(1);
    }
    count
}

#[no_mangle]
unsafe fn main(stack: *const u8) {
    let argc = *(stack as *const u64);
    let argv = stack.add(8) as *const *const u8;
    let args = core::slice::from_raw_parts(argv, argc as usize);
    let envp = stack.add(8 + argc as usize);
    let mut env_sz = 0;
    let mut v: Vec<u8> = Vec::with_capacity(256);
    for i in 0..8 {
        v.push(i);
        unix_println!("{}", envp.add(i as usize).read());
        env_sz += 1;
    }

    unix_println!("Env len {env_sz}");
    let arg_iter = ArgIter { args, it: 0 };
    run_with_args(arg_iter);
    exit(0);
}

#[inline]
fn run_with_args(args: ArgIter) {
    for arg in args.skip(1) {
        unix_println!("{arg}");
    }
}

#[inline]
pub unsafe fn exit(code: i32) -> ! {
    let syscall_number: u64 = 60;
    core::arch::asm!(
        "syscall",
        in("rax") syscall_number,
        in("rdi") code,
        options(noreturn)
    )
}