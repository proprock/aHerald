//! The computer's name for a messenger header, when a sink asks for it (`include_host`).
//!
//! The OS computer name, cleaned of control characters and cut to 64 characters; if it cannot be
//! read, the address of the interface that the default route uses. Neither lookup does network
//! I/O: the address comes from a UDP socket that is "connected" without sending a packet. No
//! fully-qualified-name trimming, hashing or other obfuscation.
use std::net::{IpAddr, UdpSocket};

/// The longest name sent, in characters.
const MAX_LEN: usize = 64;

/// Removes control characters and surrounding whitespace and cuts the name to `MAX_LEN` characters;
/// nothing left means no name.
fn clean(raw: &str) -> Option<String> {
    let kept: String = raw.chars().filter(|c| !c.is_control()).collect();
    let kept: String = kept.trim().chars().take(MAX_LEN).collect();
    (!kept.is_empty()).then_some(kept)
}

/// The usable name, else the address (asked for only then); nothing if neither is usable.
fn choose(name: Option<String>, address: impl FnOnce() -> Option<String>) -> Option<String> {
    name.and_then(|name| clean(&name))
        .or_else(|| address().and_then(|address| clean(&address)))
}

/// A local address worth showing: not the unspecified address and not loopback.
fn usable(address: IpAddr) -> bool {
    !address.is_unspecified() && !address.is_loopback()
}

/// The local address that the default route would use, IPv4 first. `connect` on a UDP socket only
/// picks the route; no packet is sent and nothing is resolved.
fn outbound_address() -> Option<String> {
    [
        ("0.0.0.0:0", "8.8.8.8:80"),
        ("[::]:0", "[2001:4860:4860::8888]:80"),
    ]
    .into_iter()
    .find_map(|(bind, target)| {
        let socket = UdpSocket::bind(bind).ok()?;
        socket.connect(target).ok()?;
        let address = socket.local_addr().ok()?.ip();
        usable(address).then(|| address.to_string())
    })
}

#[cfg(unix)]
fn os_name() -> Option<String> {
    let mut buffer = [0u8; 256];
    // SAFETY: the buffer is valid for `buffer.len()` bytes, and `gethostname` writes at most that
    // many (it may leave the name unterminated when it is cut, which `take_while` handles).
    let status = unsafe { libc::gethostname(buffer.as_mut_ptr().cast(), buffer.len()) };
    if status != 0 {
        return None;
    }
    let end = buffer.iter().position(|&b| b == 0).unwrap_or(buffer.len());
    Some(String::from_utf8_lossy(&buffer[..end]).into_owned())
}

#[cfg(windows)]
fn os_name() -> Option<String> {
    // Windows sets this for every session; it is the NetBIOS computer name.
    std::env::var("COMPUTERNAME").ok()
}

/// The name to show for this computer, or none if there is neither a name nor an address.
pub fn local() -> Option<String> {
    choose(os_name(), outbound_address)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_characters_go_and_the_name_is_cut_to_64_characters() {
        assert_eq!(
            clean("work\nstation\t-1\u{7}\r").as_deref(),
            Some("workstation-1")
        );
        assert_eq!(clean("  box  ").as_deref(), Some("box"));
        assert_eq!(clean(""), None);
        assert_eq!(clean(" \n\t"), None);
        let long = "é".repeat(100);
        assert_eq!(clean(&long).unwrap().chars().count(), 64);
        // The domain part of a fully qualified name stays; nothing is trimmed or hashed.
        assert_eq!(
            clean("host.example.org").as_deref(),
            Some("host.example.org")
        );
    }

    #[test]
    fn the_name_wins_and_the_address_is_only_asked_for_when_it_is_missing() {
        let asked = std::cell::Cell::new(false);
        let address = || {
            asked.set(true);
            Some("192.0.2.10".to_owned())
        };
        assert_eq!(choose(Some("box".into()), address).as_deref(), Some("box"));
        assert!(
            !asked.get(),
            "the address was looked up although the name was usable"
        );
        assert_eq!(choose(None, address).as_deref(), Some("192.0.2.10"));
        asked.set(false);
        assert_eq!(
            choose(Some("\n ".to_owned()), address).as_deref(),
            Some("192.0.2.10")
        );
        assert_eq!(choose(None, || None), None);
        assert_eq!(choose(Some(String::new()), || Some("\n".into())), None);
    }

    #[test]
    fn only_a_routable_looking_address_is_used() {
        use std::net::IpAddr;
        let usable = |text: &str| usable(text.parse::<IpAddr>().unwrap());
        assert!(usable("192.0.2.10"));
        assert!(usable("2001:db8::1"));
        assert!(!usable("127.0.0.1"));
        assert!(!usable("0.0.0.0"));
        assert!(!usable("::1"));
        assert!(!usable("::"));
    }

    #[test]
    fn the_real_lookups_return_clean_text_or_nothing_and_never_panic() {
        for value in [local(), outbound_address()].into_iter().flatten() {
            assert_eq!(clean(&value).as_deref(), Some(value.as_str()));
        }
        if let Some(address) = outbound_address() {
            assert!(usable(address.parse().unwrap()), "{address}");
        }
    }
}
