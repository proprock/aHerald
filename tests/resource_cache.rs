use aherald::{
    animation::{PreparedAnimation, PreparedFrame},
    resource_cache::AssetCache,
};
use std::sync::Arc;
fn art(bytes: usize) -> PreparedAnimation {
    PreparedAnimation {
        frames: vec![PreparedFrame {
            width: (bytes / 4) as u32,
            height: 1,
            bgra: vec![0; bytes],
        }],
        durations_ms: vec![100],
        looping: true,
    }
}
#[test]
fn repeated_keys_share_decoded_storage_and_failures_are_cached() {
    let mut cache = AssetCache::new(2, 16);
    let first = cache
        .get_or_load(("codex", "error"), || Some(art(8)))
        .unwrap();
    let again = cache
        .get_or_load(("codex", "error"), || panic!("decoded twice"))
        .unwrap();
    assert!(Arc::ptr_eq(&first, &again));
    assert!(cache.get_or_load(("demo", "error"), || None).is_none());
    assert!(
        cache
            .get_or_load(("demo", "error"), || panic!("retried corrupt resource"))
            .is_none()
    );
    assert_eq!(cache.retained_bytes(), 8);
}
#[test]
fn byte_and_entry_limits_evict_lru_and_oversized_values_are_not_retained() {
    let mut cache = AssetCache::new(2, 16);
    let held = cache
        .get_or_load(("codex", "one"), || Some(art(12)))
        .unwrap();
    cache.get_or_load(("codex", "two"), || Some(art(8)));
    assert_eq!(cache.retained_bytes(), 8);
    assert_eq!(held.frames[0].bgra.len(), 12);
    assert!(
        cache
            .get_or_load(("codex", "huge"), || Some(art(20)))
            .is_some()
    );
    assert_eq!(cache.retained_bytes(), 8);
    cache.get_or_load(("codex", "missing"), || None);
    cache.get_or_load(("codex", "another"), || None);
    assert_eq!(cache.retained_bytes(), 0);
    cache.get_or_load(("codex", "two"), || Some(art(4)));
    assert_eq!(cache.retained_bytes(), 4);
}

#[test]
fn hits_refresh_recency_and_a_zero_capacity_cache_retains_nothing() {
    let mut cache = AssetCache::new(2, 16);
    cache.get_or_load(("codex", "one"), || Some(art(4)));
    cache.get_or_load(("codex", "two"), || Some(art(4)));
    cache.get_or_load(("codex", "one"), || panic!("cache miss"));
    cache.get_or_load(("codex", "three"), || Some(art(4)));
    cache.get_or_load(("codex", "one"), || panic!("recent entry evicted"));
    let reloaded = cache
        .get_or_load(("codex", "two"), || Some(art(8)))
        .unwrap();
    assert_eq!(reloaded.frames[0].bgra.len(), 8);
    let mut disabled = AssetCache::new(0, 16);
    disabled.get_or_load(("codex", "one"), || Some(art(4)));
    assert_eq!(disabled.retained_bytes(), 0);
    assert!(disabled.get_or_load(("codex", "one"), || None).is_none());
}
