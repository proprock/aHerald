// Native helper for optional launch checks. Records no hook payload.
use std::{
    ffi::c_void,
    io::{self, Read},
};
#[repr(C)]
struct Entry {
    size: u32,
    usage: u32,
    pid: u32,
    heap: usize,
    module: u32,
    threads: u32,
    parent: u32,
    priority: i32,
    flags: u32,
    name: [u16; 260],
}
#[link(name = "kernel32")]
unsafe extern "system" {
    fn CreateToolhelp32Snapshot(flags: u32, pid: u32) -> *mut c_void;
    fn Process32FirstW(h: *mut c_void, e: *mut Entry) -> i32;
    fn Process32NextW(h: *mut c_void, e: *mut Entry) -> i32;
    fn CloseHandle(h: *mut c_void) -> i32;
}
fn ancestors() -> Vec<String> {
    // SAFETY: snapshot ownership stays here; Entry matches Win32's layout,
    // size is initialized and all pointers refer to live writable storage.
    unsafe {
        let h = CreateToolhelp32Snapshot(2, 0);
        assert_ne!(h as isize, -1);
        let mut e: Entry = std::mem::zeroed();
        e.size = std::mem::size_of::<Entry>() as u32;
        let mut rows = Vec::new();
        let mut more = Process32FirstW(h, &mut e);
        while more != 0 {
            let n = e.name.iter().position(|x| *x == 0).unwrap_or(260);
            rows.push((e.pid, e.parent, String::from_utf16_lossy(&e.name[..n])));
            more = Process32NextW(h, &mut e);
        }
        CloseHandle(h);
        let mut pid = std::process::id();
        let mut out = Vec::new();
        for _ in 0..6 {
            let Some(row) = rows.iter().find(|r| r.0 == pid) else {
                break;
            };
            out.push(row.2.clone());
            pid = row.1;
        }
        out
    }
}
fn main() {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).unwrap();
    let mut text = format!("stdin_bytes={}\n", input.len());
    for (i, arg) in std::env::args().skip(1).enumerate() {
        text.push_str(&format!("arg{i}={arg}\n"));
    }
    for (i, name) in ancestors().iter().enumerate() {
        text.push_str(&format!("parent{i}={name}\n"));
    }
    let file = std::path::PathBuf::from(std::env::var_os("AHERALD_LAUNCH_RECORD").unwrap());
    let tmp = file.with_extension("tmp");
    std::fs::write(&tmp, text).unwrap();
    std::fs::rename(tmp, file).unwrap();
    println!("{{}}");
}
