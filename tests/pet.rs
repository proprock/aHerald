use aherald::{config::Config, events::Kind, pet};
use image::{Rgba, RgbaImage};
use std::{
    fs,
    path::PathBuf,
    sync::{
        LazyLock,
        atomic::{AtomicU32, Ordering},
    },
};

#[test]
fn visual_sources_cover_defaults_full_pet_and_both_hybrid_directions() {
    use aherald::{artwork, events::Event};
    let fixture = Fixture::new();
    let builtin = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets");
    let resolve = |text: &str, event: &Event| {
        let config = Config::parse(text, &fixture.0).unwrap();
        artwork::resolve(&config, event, &builtin).unwrap().unwrap()
    };
    for kind in [
        "turn_complete",
        "permission_required",
        "input_required",
        "error",
        "interrupted",
    ] {
        let event = Event::demo(kind).unwrap();
        assert_eq!(resolve("", &event).frames[0].dimensions(), (64, 64));
        assert_eq!(
            resolve("[notifications]\npet='pet.json'", &event).frames[0].dimensions(),
            (192, 208)
        );
    }
    let event = Event::demo("error").unwrap();
    assert_eq!(
        resolve(
            "[notifications]\npet='pet.json'\n[events.error]\nanimation='missing'",
            &event
        )
        .frames[0]
            .dimensions(),
        (192, 208),
        "an old animation path cannot silently override a pet"
    );
    assert_eq!(
        resolve(
            "[notifications]\npet='pet.json'\n[events.error]\nvisual='builtin'",
            &event
        )
        .frames[0]
            .dimensions(),
        (64, 64)
    );
    assert_eq!(
        resolve(
            "[notifications]\nvisual='builtin'\npet='pet.json'\n[events.error]\nvisual='pet'",
            &event
        )
        .frames[0]
            .dimensions(),
        (192, 208)
    );
    assert_eq!(
        resolve("[notifications]\nvisual='builtin'\npet='pet.json'", &event).frames[0].dimensions(),
        (64, 64)
    );
    let custom = format!(
        "[notifications]\npet='pet.json'\n[events.error]\nvisual='custom'\nanimation='{}'",
        builtin.join("turn_complete").display()
    );
    let selected = resolve(&custom, &event);
    let expected = aherald::animation::Animation::load(&builtin.join("turn_complete")).unwrap();
    assert_eq!(
        selected.frames, expected.frames,
        "custom selection must load that event's explicit path"
    );
}

#[test]
fn unavailable_artwork_recovers_to_builtin_then_text() {
    use aherald::{artwork, events::Event};
    let fixture = Fixture::new();
    let builtin = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets");
    let event = Event::demo("error").unwrap();
    for text in [
        "[notifications]\npet='missing.json'",
        "[events.error]\nvisual='pet'",
        "[events.error]\nvisual='custom'\nanimation='missing'",
    ] {
        let config = Config::parse(text, &fixture.0).unwrap();
        assert_eq!(
            artwork::resolve(&config, &event, &builtin)
                .unwrap()
                .unwrap()
                .frames[0]
                .dimensions(),
            (64, 64)
        );
        assert!(
            artwork::resolve(&config, &event, &fixture.0.join("missing-assets"))
                .unwrap()
                .is_none()
        );
    }
    let config = Config::default();
    assert!(
        artwork::resolve(&config, &event, &fixture.0.join("missing-assets"))
            .unwrap()
            .is_none()
    );
}

#[test]
fn custom_artwork_that_exceeds_scaled_limits_recovers_to_builtin() {
    let fixture = Fixture::new();
    RgbaImage::new(1024, 1024)
        .save(fixture.0.join("large.png"))
        .unwrap();
    fs::write(
        fixture.0.join("animation.toml"),
        "type='spritesheet'\nimage='large.png'\nframe_width=1024\nframe_height=1024\nframes=1",
    )
    .unwrap();
    let config = Config::parse(
        "[notifications]\nscale=4.0\n[events.error]\nvisual='custom'\nanimation='.'",
        &fixture.0,
    )
    .unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets");
    let event = aherald::events::Event::demo("error").unwrap();
    let prepared = aherald::artwork::prepare(&config, &event, &root)
        .unwrap()
        .unwrap();
    assert_eq!(prepared.frames[0].width, 256);
    assert!(
        aherald::artwork::prepare(&config, &event, &fixture.0.join("missing"))
            .unwrap()
            .is_none()
    );
}
#[test]
fn artwork_too_large_at_the_monitor_dpi_keeps_the_configured_scale() {
    let fixture = Fixture::new();
    RgbaImage::new(1024, 1024)
        .save(fixture.0.join("large.png"))
        .unwrap();
    fs::write(
        fixture.0.join("animation.toml"),
        "type='spritesheet'\nimage='large.png'\nframe_width=1024\nframe_height=1024\nframes=1",
    )
    .unwrap();
    let config =
        Config::parse("[events.error]\nvisual='custom'\nanimation='.'", &fixture.0).unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets");
    let event = aherald::events::Event::demo("error").unwrap();
    let width = |scale| {
        aherald::artwork::prepare_at(&config, &event, &root, scale)
            .unwrap()
            .unwrap()
            .frames[0]
            .width
    };
    assert_eq!(width(2.0), 2048, "fits the budget at the DPI scale");
    assert_eq!(
        width(4.0),
        1024,
        "over budget: configured scale, still custom"
    );
}
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!(
            "aherald-pet-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        static ATLAS: LazyLock<Vec<u8>> = LazyLock::new(|| {
            let image = RgbaImage::from_fn(1536, 1872, |x, y| {
                Rgba([(y / 208) as u8, (x / 192) as u8, 80, 255])
            });
            let mut out = std::io::Cursor::new(Vec::new());
            image.write_to(&mut out, image::ImageFormat::WebP).unwrap();
            out.into_inner()
        });
        fs::write(path.join("sheet.webp"), &*ATLAS).unwrap();
        fs::write(
            path.join("pet.json"),
            r#"{"id":"test","displayName":"Original test fixture","spritesheetPath":"sheet.webp"}"#,
        )
        .unwrap();
        Self(path)
    }
    fn load(&self, kind: Kind) -> Result<aherald::animation::Animation, String> {
        pet::load(&self.0.join("pet.json"), kind)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn configured_pet_states_override_defaults_and_stay_agent_scoped() {
    use aherald::events::{Event, Source};
    let fixture = Fixture::new();
    let config = Config::parse(
        "[notifications]\npet='pet.json'\n[notifications.pet_animations]\nturn_complete='failed'\n[agents.codex]\npet='pet.json'\n[agents.codex.pet_animations]\nturn_complete='waiting'",
        &fixture.0,
    ).unwrap();
    let mut event = Event::demo("turn_complete").unwrap();
    let shared = pet::for_event(&config, &event).unwrap().unwrap();
    assert_eq!(shared.frames.len(), 8);
    assert_eq!(shared.frames[0].get_pixel(0, 0).0, [5, 0, 80, 255]);
    event.source = Source::CodexHook;
    let agent = pet::for_event(&config, &event).unwrap().unwrap();
    assert_eq!(agent.frames.len(), 6);
    assert_eq!(agent.frames[0].get_pixel(0, 0).0, [6, 0, 80, 255]);
    assert_eq!(agent.durations_ms, [150, 150, 150, 150, 150, 260]);
}

#[test]
fn all_standard_pet_states_can_be_selected_by_event() {
    use aherald::events::Event;
    let fixture = Fixture::new();
    for (state, row, count, last_ms) in [
        ("running-right", 1, 8, 220),
        ("running-left", 2, 8, 220),
        ("jumping", 4, 5, 280),
        ("running", 7, 6, 220),
    ] {
        let config = Config::parse(
            &format!(
                "[notifications]\npet='pet.json'\n[notifications.pet_animations]\nerror='{state}'"
            ),
            &fixture.0,
        )
        .unwrap();
        let animation = pet::for_event(&config, &Event::demo("error").unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(animation.frames.len(), count);
        assert_eq!(animation.frames[0].get_pixel(0, 0).0, [row, 0, 80, 255]);
        assert_eq!(animation.durations_ms.last(), Some(&last_ms));
    }
}
#[test]
fn pet_config_is_optional_and_resolves_relative_to_config() {
    let dir = PathBuf::from(r"C:\settings");
    let config = Config::parse("[notifications]\npet='characters/becky/pet.json'", &dir).unwrap();
    assert_eq!(
        config.notifications.pet,
        Some(dir.join("characters/becky/pet.json"))
    );
    assert!(Config::default().notifications.pet.is_none());
}
#[test]
fn webp_tracks_select_event_rows_and_skip_unused_cells() {
    let fixture = Fixture::new();
    for (kind, row, count) in [
        (Kind::TurnComplete, 3, 4),
        (Kind::PermissionRequired, 8, 6),
        (Kind::InputRequired, 6, 6),
        (Kind::Error, 5, 8),
        (Kind::Interrupted, 0, 6),
    ] {
        let animation = fixture.load(kind).unwrap();
        assert_eq!(animation.frames.len(), count);
        assert_eq!(animation.frames[0].dimensions(), (192, 208));
        for (col, frame) in animation.frames.iter().enumerate() {
            assert_eq!(frame.get_pixel(0, 0).0, [row, col as u8, 80, 255]);
        }
        assert!(animation.looping);
        assert_eq!(animation.index_at(animation.durations_ms.iter().sum()), 0);
    }
}
#[test]
fn invalid_manifest_asset_and_escape_fail_safely() {
    let fixture = Fixture::new();
    for text in [
        "{",
        r#"{"spritesheetPath":"../escape.webp"}"#,
        r#"{"spritesheetPath":"C:\\escape.webp"}"#,
        r#"{"spritesheetPath":"missing.webp"}"#,
        r#"{"spriteVersionNumber":3,"spritesheetPath":"sheet.webp"}"#,
    ] {
        fs::write(fixture.0.join("pet.json"), text).unwrap();
        assert!(fixture.load(Kind::Error).is_err());
    }
    fs::write(
        fixture.0.join("pet.json"),
        r#"{"spritesheetPath":"sheet.webp"}"#,
    )
    .unwrap();
    fs::write(fixture.0.join("sheet.webp"), b"corrupt").unwrap();
    assert!(fixture.load(Kind::Error).is_err());
}

#[test]
fn v2_png_atlas_keeps_standard_rows_and_alpha() {
    let fixture = Fixture::new();
    let sheet = RgbaImage::from_fn(1536, 2288, |x, y| {
        Rgba([(y / 208) as u8, (x / 192) as u8, 80, 128])
    });
    sheet.save(fixture.0.join("sheet.png")).unwrap();
    fs::write(
        fixture.0.join("pet.json"),
        r#"{"spriteVersionNumber":2,"spritesheetPath":"sheet.png","unknownMetadata":true}"#,
    )
    .unwrap();
    let animation = fixture.load(Kind::PermissionRequired).unwrap();
    assert_eq!(animation.frames.len(), 6);
    assert_eq!(animation.frames[0].get_pixel(0, 0).0, [8, 0, 80, 128]);
    assert_eq!(animation.durations_ms, [150, 150, 150, 150, 150, 280]);
    let prepared = animation.prepare(1.0, false).unwrap();
    assert_eq!(&prepared.frames[0].bgra[..4], &[40, 0, 4, 128]);
}
#[test]
fn atlas_size_metadata_limits_and_custom_tracks_are_checked() {
    let fixture = Fixture::new();
    for text in [
        r#"{"spritesheetPath":"sheet.webp","spriteVersionNumber":2}"#,
        r#"{"spritesheetPath":"sheet.webp","animations":{"waiting":{"frames":[48]}}}"#,
        r#"{"spritesheetPath":"sheet.webp","frame":{"width":1}}"#,
        r#"{"spritesheetPath":""}"#,
        r#"{"spritesheetPath":"https://example.com/sheet.webp"}"#,
        r#"{"spritesheetPath":"sheet.webp:stream"}"#,
    ] {
        fs::write(fixture.0.join("pet.json"), text).unwrap();
        assert!(fixture.load(Kind::Error).is_err());
    }
    fs::write(fixture.0.join("pet.json"), vec![b' '; 16385]).unwrap();
    assert!(fixture.load(Kind::Error).is_err());
    fs::write(
        fixture.0.join("pet.json"),
        r#"{"spritesheetPath":"sheet.webp"}"#,
    )
    .unwrap();
    RgbaImage::new(64, 64)
        .save(fixture.0.join("sheet.webp"))
        .unwrap();
    assert!(fixture.load(Kind::Error).is_err());
    let oversized = fs::File::create(fixture.0.join("sheet.webp")).unwrap();
    oversized.set_len(20 * 1024 * 1024 + 1).unwrap();
    assert!(fixture.load(Kind::Error).is_err());
}
#[test]
fn custom_requires_explicit_selection_and_missing_pet_reaches_text_without_builtins() {
    let fixture = Fixture::new();
    let mut config = Config::default();
    assert!(
        pet::for_notification(&config, Kind::Error)
            .unwrap()
            .is_none()
    );
    config.notifications.pet = Some(fixture.0.join("pet.json"));
    assert_eq!(
        pet::for_notification(&config, Kind::Error)
            .unwrap()
            .unwrap()
            .frames[0]
            .get_pixel(0, 0)
            .0[0],
        5
    );
    config.notifications.pet = Some(fixture.0.join("missing.json"));
    assert!(
        pet::for_notification(&config, Kind::Error)
            .unwrap()
            .is_none()
    );
    config.events[Kind::Error].animation =
        Some(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/error"));
    assert!(
        pet::for_notification(&config, Kind::Error)
            .unwrap()
            .is_none(),
        "unused custom paths do not select artwork"
    );
    config.events[Kind::Error].visual = aherald::config::EventVisual::Custom;
    assert_eq!(
        pet::for_notification(&config, Kind::Error)
            .unwrap()
            .unwrap()
            .frames[0]
            .dimensions(),
        (64, 64)
    );
}

#[cfg(windows)]
#[test]
fn native_pet_window_keeps_source_and_signal_with_or_without_artwork() {
    use aherald::{events::Event, process::deliver};
    use std::time::{Duration, Instant};
    use windows::Win32::{Foundation::*, UI::WindowsAndMessaging::*};
    struct Owned(aherald::process::Child);
    impl Drop for Owned {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    struct Search {
        pid: u32,
        window: HWND,
    }
    unsafe extern "system" fn find(hwnd: HWND, p: LPARAM) -> windows::core::BOOL {
        // SAFETY: synchronous EnumWindows receives a live Search; no pointer escapes the call.
        unsafe {
            let state = &mut *(p.0 as *mut Search);
            let mut pid = 0;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            if pid == state.pid && IsWindowVisible(hwnd).as_bool() {
                state.window = hwnd;
                return false.into();
            }
            true.into()
        }
    }
    let fixture = Fixture::new();
    for valid in [true, false] {
        if !valid {
            fs::write(fixture.0.join("sheet.webp"), b"broken").unwrap();
        }
        let config = fixture.0.join("config.toml");
        fs::write(&config,"[notifications]\npet='pet.json'\nfade_in_ms=0\nfade_out_ms=0\n[sounds]\nenabled=false\n[events.permission_required]\nduration_ms=0").unwrap();
        let mut child = Owned(
            deliver(
                &PathBuf::from(env!("CARGO_BIN_EXE_aherald")),
                &config,
                &Event::demo("permission_required").unwrap(),
            )
            .unwrap(),
        );
        let mut search = Search {
            pid: child.0.id(),
            window: HWND::default(),
        };
        let deadline = Instant::now() + Duration::from_secs(3);
        while search.window.is_invalid() && Instant::now() < deadline {
            // SAFETY: only a live stack object is passed to synchronous enumeration.
            unsafe {
                let _ = EnumWindows(Some(find), LPARAM(&mut search as *mut Search as isize));
            }
            std::thread::yield_now();
        }
        assert!(
            !search.window.is_invalid(),
            "pet/fallback window was not created"
        );
        // SAFETY: only the test-owned process's window is inspected and dismissed.
        unsafe {
            let mut rect = RECT::default();
            GetWindowRect(search.window, &mut rect).unwrap();
            let (w, h) = (rect.right - rect.left, rect.bottom - rect.top);
            let mut title = [0u16; 128];
            let len = GetWindowTextW(search.window, &mut title);
            assert_eq!(
                String::from_utf16_lossy(&title[..len as usize]),
                "aHerald demo - Permission required"
            );
            if valid {
                // The new contract reserves text space beside the 192x208 artwork.
                assert!(w > h * 2, "text space must remain beside the pet");
            } else {
                assert!(w > h * 2);
            }
            PostMessageW(Some(search.window), WM_LBUTTONUP, WPARAM(0), LPARAM(0)).unwrap();
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(status) = child.0.try_wait().unwrap() {
                assert!(status.success());
                break;
            }
            assert!(Instant::now() < deadline, "pet window failed to dismiss");
            std::thread::yield_now();
        }
    }
}

#[test]
fn agent_pet_settings_resolve_relative_paths_and_select_only_the_event_source() {
    use aherald::events::{Event, Source};
    let fixture = Fixture::new();
    let config = Config::parse(
        "[agents.codex]\npet='pet.json'\n[agents.claude]\npet='missing.json'",
        &fixture.0,
    )
    .unwrap();
    let mut event = Event::demo("error").unwrap();
    event.source = Source::CodexHook;
    assert!(pet::for_event(&config, &event).unwrap().is_some());
    event.source = Source::Demo;
    assert!(pet::for_event(&config, &event).unwrap().is_none());
    let mut config = config;
    config.notifications.pet = Some(fixture.0.join("missing.json"));
    event.source = Source::CodexHook;
    assert!(pet::for_event(&config, &event).unwrap().is_some());
    config.agents.get_mut("codex").unwrap().pet = Some(fixture.0.join("missing.json"));
    assert!(pet::for_event(&config, &event).unwrap().is_none());
    config.events[Kind::Error].animation =
        Some(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/error"));
    config.events[Kind::Error].visual = aherald::config::EventVisual::Custom;
    assert_eq!(
        pet::for_event(&config, &event).unwrap().unwrap().frames[0].dimensions(),
        (64, 64)
    );
}
