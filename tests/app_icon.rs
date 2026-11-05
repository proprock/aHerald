#![cfg(windows)]

use std::{mem::size_of, os::windows::ffi::OsStrExt};
use windows::{
    Win32::{
        Foundation::{FreeLibrary, HMODULE},
        Graphics::Gdi::{BITMAP, DeleteObject, GetObjectW, HGDIOBJ},
        System::LibraryLoader::{LOAD_LIBRARY_AS_DATAFILE, LoadLibraryExW},
        UI::WindowsAndMessaging::{
            DestroyIcon, GetIconInfo, HICON, ICONINFO, IMAGE_ICON, LR_DEFAULTCOLOR, LoadImageW,
        },
    },
    core::PCWSTR,
};

struct Module(HMODULE);
impl Drop for Module {
    fn drop(&mut self) {
        // SAFETY: this guard owns the module returned by LoadLibraryExW.
        unsafe {
            let _ = FreeLibrary(self.0);
        }
    }
}
struct Icon(HICON);
impl Drop for Icon {
    fn drop(&mut self) {
        // SAFETY: LR_SHARED is not used; this guard owns the icon.
        unsafe {
            let _ = DestroyIcon(self.0);
        }
    }
}
struct Bitmap(HGDIOBJ);
impl Drop for Bitmap {
    fn drop(&mut self) {
        // SAFETY: GetIconInfo returns owned bitmap copies.
        unsafe {
            let _ = DeleteObject(self.0);
        }
    }
}

#[test]
fn built_executable_supplies_application_icon_at_common_shell_sizes() {
    let path: Vec<u16> = std::ffi::OsStr::new(env!("CARGO_BIN_EXE_aherald"))
        .encode_wide()
        .chain(Some(0))
        .collect();
    // SAFETY: path is NUL-terminated; loading as data never executes the application.
    let module = Module(unsafe {
        LoadLibraryExW(PCWSTR(path.as_ptr()), None, LOAD_LIBRARY_AS_DATAFILE).unwrap()
    });
    for size in [16, 20, 24, 32, 40, 48, 64, 96, 128, 256] {
        // SAFETY: integer resource ID 1 and live module; the returned icon is owned.
        let icon = Icon(HICON(
            unsafe {
                LoadImageW(
                    Some(module.0.into()),
                    PCWSTR(std::ptr::without_provenance(1)),
                    IMAGE_ICON,
                    size,
                    size,
                    LR_DEFAULTCOLOR,
                )
            }
            .unwrap_or_else(|error| panic!("application icon at {size}px: {error}"))
            .0,
        ));
        let mut info = ICONINFO::default();
        // SAFETY: live icon and writable output structure.
        unsafe {
            GetIconInfo(icon.0, &mut info).unwrap();
        }
        let _color = Bitmap(info.hbmColor.into());
        let _mask = Bitmap(info.hbmMask.into());
        let mut bitmap = BITMAP::default();
        // SAFETY: live bitmap and correctly sized writable output buffer.
        assert_ne!(
            // SAFETY: `info.hbmColor` is the live bitmap of the icon loaded above and `bitmap` is a
            // writable BITMAP of the size passed.
            unsafe {
                GetObjectW(
                    info.hbmColor.into(),
                    size_of::<BITMAP>() as i32,
                    Some(&mut bitmap as *mut _ as *mut _),
                )
            },
            0
        );
        assert_eq!((bitmap.bmWidth, bitmap.bmHeight), (size, size));
        assert_eq!(bitmap.bmBitsPixel, 32);
    }
}
