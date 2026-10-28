//! Version-dependent, read-only normalization of Codex rate-limit snapshots.
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Window {
    pub used_percent: Option<f64>,
    pub window_duration_mins: Option<i64>,
    pub resets_at: Option<i64>,
}
impl Window {
    pub fn remaining_percent(&self) -> Option<f64> {
        self.used_percent.map(|v| (100.0 - v).clamp(0.0, 100.0))
    }
}
#[derive(Clone, Debug, Default, Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Credits {
    pub has_credits: Option<bool>,
    pub unlimited: Option<bool>,
    pub balance: Option<String>,
}
#[derive(Clone, Debug, Default, Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bucket {
    pub limit_id: Option<String>,
    pub limit_name: Option<String>,
    pub primary: Option<Window>,
    pub secondary: Option<Window>,
    pub credits: Option<Credits>,
    pub plan_type: Option<String>,
    pub rate_limit_reached_type: Option<String>,
    /// Claude's optional pay-as-you-go blocks; absent for Codex.
    pub extra_usage: Option<Allowance>,
    pub spend: Option<Allowance>,
}
/// A spending allowance with amounts already converted to decimal strings (`12.34`).
#[derive(Clone, Debug, Default, PartialEq, Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Allowance {
    pub enabled: Option<bool>,
    pub used: Option<String>,
    pub limit: Option<String>,
    /// Short alphabetic code such as `USD`; anything else is dropped before it gets here.
    pub currency: Option<String>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResetCredit {
    pub id: String,
    pub reset_type: Option<String>,
    pub status: Option<String>,
    pub granted_at: Option<i64>,
    pub expires_at: Option<i64>,
    pub title: Option<String>,
    pub description: Option<String>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResetCredits {
    pub available_count: Option<u64>,
    pub credits: Option<Vec<ResetCredit>>,
}
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub buckets: BTreeMap<String, Bucket>,
    pub ordinary_usage_allowed: Option<bool>,
    pub reset_credits: Option<ResetCredits>,
}
impl Snapshot {
    /// A snapshot of one already-typed bucket, validated like parsed data.
    pub fn single(id: &str, bucket: Bucket) -> Result<Self, &'static str> {
        validate_bucket(&bucket)?;
        Ok(Self {
            buckets: BTreeMap::from([(id.to_owned(), bucket)]),
            ..Self::default()
        })
    }
    pub fn parse(value: Value) -> Result<Self, &'static str> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Wire {
            rate_limits: Option<Bucket>,
            rate_limits_by_limit_id: Option<BTreeMap<String, Bucket>>,
            ordinary_usage_allowed: Option<bool>,
            rate_limit_reset_credits: Option<ResetCredits>,
        }
        if !value.is_object()
            || (value.get("rateLimits").is_none() && value.get("rateLimitsByLimitId").is_none())
        {
            return Err("missing usage snapshot");
        }
        let wire: Wire = serde_json::from_value(value).map_err(|_| "invalid usage snapshot")?;
        let buckets = if let Some(map) = wire.rate_limits_by_limit_id {
            map
        } else if let Some(bucket) = wire.rate_limits {
            BTreeMap::from([(
                bucket.limit_id.clone().unwrap_or_else(|| "codex".into()),
                bucket,
            )])
        } else {
            BTreeMap::new()
        };
        for bucket in buckets.values() {
            validate_bucket(bucket)?;
        }
        Ok(Self {
            buckets,
            ordinary_usage_allowed: wire.ordinary_usage_allowed,
            reset_credits: wire.rate_limit_reset_credits,
        })
    }
    /// Merge only present, non-null values. False asks the caller for a full refetch.
    pub fn update(&mut self, value: Value) -> Result<bool, &'static str> {
        let value = value
            .get("rateLimits")
            .ok_or("missing rolling usage snapshot")?;
        let incoming: Bucket =
            serde_json::from_value(value.clone()).map_err(|_| "invalid rolling usage snapshot")?;
        validate_bucket(&incoming)?;
        let id = incoming.limit_id.clone().or_else(|| {
            (self.buckets.len() == 1)
                .then(|| self.buckets.keys().next().cloned())
                .flatten()
        });
        let Some(id) = id else {
            return Ok(false);
        };
        if self.buckets.len() >= 128 && !self.buckets.contains_key(&id) {
            return Ok(false);
        }
        let target = self.buckets.entry(id).or_default();
        let mut original = serde_json::to_value(&*target).map_err(|_| "invalid stored usage")?;
        merge_available(&mut original, value);
        *target = serde_json::from_value(original).map_err(|_| "invalid merged usage")?;
        Ok(true)
    }
}

fn validate_bucket(bucket: &Bucket) -> Result<(), &'static str> {
    for window in [&bucket.primary, &bucket.secondary].into_iter().flatten() {
        if window
            .used_percent
            .is_some_and(|v| !v.is_finite() || v < 0.0)
            || window.window_duration_mins.is_some_and(|v| v < 0)
        {
            return Err("invalid usage window");
        }
    }
    Ok(())
}
fn merge_available(target: &mut Value, update: &Value) {
    if let (Some(target), Some(update)) = (target.as_object_mut(), update.as_object()) {
        for (key, value) in update {
            if !value.is_null() {
                merge_available(target.entry(key.clone()).or_insert(Value::Null), value);
            }
        }
    } else if !update.is_null() {
        *target = update.clone();
    }
}

/// `YYYY-MM-DD HH:MM:SS (UTC+HH:MM)` for `timestamp` in the local zone, with the offset in
/// force at that instant; `None` when the platform cannot represent it.
#[cfg(unix)]
pub fn local_reset(timestamp: i64) -> Option<String> {
    let tm = crate::zone::local_time(timestamp)?;
    let minutes = tm.tm_gmtoff / 60;
    Some(format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02} (UTC{}{:02}:{:02})",
        i64::from(tm.tm_year) + 1900,
        tm.tm_mon + 1,
        tm.tm_mday,
        tm.tm_hour,
        tm.tm_min,
        tm.tm_sec,
        if minutes < 0 { "-" } else { "+" },
        minutes.abs() / 60,
        minutes.abs() % 60
    ))
}

#[cfg(windows)]
pub fn local_reset(timestamp: i64) -> Option<String> {
    local_reset_in(timestamp, None)
}

#[cfg(windows)]
pub fn local_reset_in(
    timestamp: i64,
    zone: Option<&windows::Win32::System::Time::DYNAMIC_TIME_ZONE_INFORMATION>,
) -> Option<String> {
    use windows::Win32::{
        Foundation::{FILETIME, SYSTEMTIME},
        System::Time::*,
    };
    let ticks: u64 = ((timestamp as i128 + 11_644_473_600) * 10_000_000)
        .try_into()
        .ok()?;
    let file = FILETIME {
        dwLowDateTime: ticks as u32,
        dwHighDateTime: (ticks >> 32) as u32,
    };
    let mut utc = SYSTEMTIME::default();
    let mut local = SYSTEMTIME::default();
    let mut local_file = FILETIME::default();
    // SAFETY: all pointers reference initialized structures for synchronous Win32 calls;
    // None selects the OS timezone and its historical DST rules for this instant.
    unsafe {
        FileTimeToSystemTime(&file, &mut utc).ok()?;
        SystemTimeToTzSpecificLocalTimeEx(zone.map(|z| z as *const _), &utc, &mut local).ok()?;
        SystemTimeToFileTime(&local, &mut local_file).ok()?;
    }
    let local_ticks = ((local_file.dwHighDateTime as u64) << 32) | local_file.dwLowDateTime as u64;
    let minutes = (local_ticks as i128 - ticks as i128) / 600_000_000;
    Some(format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02} (UTC{}{:02}:{:02})",
        local.wYear,
        local.wMonth,
        local.wDay,
        local.wHour,
        local.wMinute,
        local.wSecond,
        if minutes < 0 { "-" } else { "+" },
        minutes.abs() / 60,
        minutes.abs() % 60
    ))
}
