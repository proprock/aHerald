//! Reads one generic-password item of the login Keychain through the Security framework.
//!
//! Only `claude_usage` uses it, for the Claude Code OAuth credentials (AH-066): the bytes go
//! straight into a zeroed buffer, nothing is logged, and the system's own access prompt is
//! never bypassed.
use crate::https::Wiped;
use objc2::{rc::Retained, runtime::AnyObject};
use objc2_foundation::{NSData, NSDictionary, NSNumber, NSString};
use std::ffi::c_void;

/// Why the item could not be read. Carries no item data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    /// No such item.
    NotFound,
    /// The user (or the Keychain's access list) refused access.
    Denied,
    /// No Keychain is available or it is locked and cannot be asked to unlock.
    Unavailable,
    /// Any other `OSStatus`.
    Other(i32),
}

/// Maps a Security framework `OSStatus` to a failure.
pub fn classify(status: i32) -> Failure {
    match status {
        // errSecItemNotFound
        -25300 => Failure::NotFound,
        // errSecAuthFailed, errSecUserCanceled
        -25293 | -128 => Failure::Denied,
        // errSecInteractionNotAllowed, errSecNotAvailable
        -25308 | -25291 => Failure::Unavailable,
        other => Failure::Other(other),
    }
}

#[link(name = "Security", kind = "framework")]
unsafe extern "C" {
    static kSecClass: *const c_void;
    static kSecClassGenericPassword: *const c_void;
    static kSecAttrService: *const c_void;
    static kSecReturnData: *const c_void;
    static kSecMatchLimit: *const c_void;
    static kSecMatchLimitOne: *const c_void;
    fn SecItemCopyMatching(query: *const c_void, result: *mut *const c_void) -> i32;
}

/// The password data of the generic-password item whose service is `service`.
pub fn read(service: &str) -> Result<Wiped<Vec<u8>>, Failure> {
    let service = NSString::from_str(service);
    let yes = NSNumber::new_bool(true);
    // SAFETY: the `kSec*` constants are immutable CoreFoundation objects that are toll-free
    // bridged to the Foundation classes used here; the query dictionary and everything in it
    // outlive the call. On success the result is a CFData that this call owns (a "Copy" API),
    // which `Retained` releases.
    let data = unsafe {
        let key = |p: *const c_void| &*(p as *const NSString);
        let value = |p: *const c_void| &*(p as *const AnyObject);
        let query: Retained<NSDictionary<NSString, AnyObject>> = NSDictionary::from_slices(
            &[
                key(kSecClass),
                key(kSecAttrService),
                key(kSecReturnData),
                key(kSecMatchLimit),
            ],
            &[
                value(kSecClassGenericPassword),
                &*service as &AnyObject,
                &*yes as &AnyObject,
                value(kSecMatchLimitOne),
            ],
        );
        let mut result: *const c_void = std::ptr::null();
        let status = SecItemCopyMatching(Retained::as_ptr(&query).cast(), &mut result);
        if status != 0 {
            return Err(classify(status));
        }
        Retained::from_raw(result as *mut NSData).ok_or(Failure::Other(0))?
    };
    Ok(Wiped(data.to_vec()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statuses_map_to_failures_without_item_data() {
        assert_eq!(classify(-25300), Failure::NotFound);
        assert_eq!(classify(-25293), Failure::Denied);
        assert_eq!(classify(-128), Failure::Denied);
        assert_eq!(classify(-25308), Failure::Unavailable);
        assert_eq!(classify(-25291), Failure::Unavailable);
        assert_eq!(classify(-25295), Failure::Other(-25295));
    }

    #[test]
    fn an_item_that_does_not_exist_is_not_found_and_asks_for_nothing() {
        let service = format!("aherald-test-missing-{}", std::process::id());
        assert_eq!(read(&service).err(), Some(Failure::NotFound));
    }

    #[test]
    #[ignore = "manual check: creates, reads and deletes a Keychain item named for this test"]
    fn a_readable_item_comes_back_byte_for_byte_and_is_cleaned_up() {
        let service = format!("aherald-test-item-{}", std::process::id());
        let add = std::process::Command::new("/usr/bin/security")
            .args(["add-generic-password", "-a", "aherald-test", "-s", &service])
            .args(["-w", "{\"k\":\"v\"}", "-A"])
            .status()
            .unwrap();
        assert!(add.success());
        let read_back = read(&service);
        let _ = std::process::Command::new("/usr/bin/security")
            .args(["delete-generic-password", "-s", &service])
            .status();
        assert_eq!(read_back.unwrap().0, b"{\"k\":\"v\"}");
    }
}
