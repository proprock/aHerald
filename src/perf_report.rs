//! Summarizes the `perf`, `slow` and `count` lines of `aherald.log` into a table ranked
//! by total time, so the costliest span is the first row. Lines that do not parse are
//! skipped: the log is rotated mid-line and may come from an older version.
use crate::operational_log::format_millis;
use std::{collections::BTreeMap, path::Path};

#[derive(Debug, Default)]
pub struct SpanRow {
    pub name: String,
    pub samples: u64,
    pub total_us: u64,
    pub mean_us: u64,
    /// The worst p95 of any window: a lower bound for the true p95.
    pub p95_us: u64,
    pub max_us: u64,
    /// Over-threshold samples, including those the logger folded into a later line.
    pub slow: u64,
}
#[derive(Debug, Default)]
pub struct Summary {
    pub spans: Vec<SpanRow>,
    pub counters: Vec<(String, u64)>,
}

/// `12.34ms` -> microseconds.
fn parse_ms(text: &str) -> Option<u64> {
    let number = text.strip_suffix("ms")?;
    let (whole, fraction) = number.split_once('.').unwrap_or((number, "0"));
    let fraction = format!("{fraction:0<2}");
    Some(whole.parse::<u64>().ok()? * 1000 + fraction.get(..2)?.parse::<u64>().ok()? * 10)
}
fn field<'a>(fields: &[&'a str], key: &str) -> Option<&'a str> {
    fields
        .iter()
        .find_map(|f| f.split_once('=').filter(|(k, _)| *k == key).map(|(_, v)| v))
}

pub fn summarize(text: &str) -> Summary {
    let mut spans: BTreeMap<String, SpanRow> = BTreeMap::new();
    let mut counters: BTreeMap<String, u64> = BTreeMap::new();
    for line in text.lines() {
        let mut parts = line.splitn(3, ' ');
        let (Some(_stamp), Some(_level), Some(message)) =
            (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        let mut words = message.split_whitespace();
        let kind = words.next();
        let Some(name) = words.next() else { continue };
        let fields: Vec<&str> = words.collect();
        match kind {
            Some("perf") => {
                let (Some(n), Some(avg), Some(p95), Some(max)) = (
                    field(&fields, "n").and_then(|v| v.parse::<u64>().ok()),
                    field(&fields, "avg").and_then(parse_ms),
                    field(&fields, "p95").and_then(parse_ms),
                    field(&fields, "max").and_then(parse_ms),
                ) else {
                    continue;
                };
                let row = spans.entry(name.to_owned()).or_default();
                row.samples += n;
                row.total_us += n * avg;
                row.p95_us = row.p95_us.max(p95);
                row.max_us = row.max_us.max(max);
            }
            Some("slow") => {
                let suppressed = field(&fields, "suppressed")
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(0);
                spans.entry(name.to_owned()).or_default().slow += 1 + suppressed;
            }
            Some("count") => {
                if let Some(n) = field(&fields, "n").and_then(|v| v.parse::<u64>().ok()) {
                    *counters.entry(name.to_owned()).or_default() += n;
                }
            }
            _ => {}
        }
    }
    let mut spans: Vec<SpanRow> = spans
        .into_iter()
        .map(|(name, mut row)| {
            row.name = name;
            row.mean_us = row.total_us.checked_div(row.samples).unwrap_or(0);
            row
        })
        .collect();
    spans.sort_by(|a, b| {
        b.total_us
            .cmp(&a.total_us)
            .then_with(|| a.name.cmp(&b.name))
    });
    Summary {
        spans,
        counters: counters.into_iter().collect(),
    }
}

pub fn render(summary: &Summary) -> String {
    let mut out = format!(
        "{:<20} {:>9} {:>10} {:>9} {:>8} {:>8} {:>5}\n",
        "span", "samples", "total_ms", "mean_ms", "p95_ms", "max_ms", "slow"
    );
    for r in &summary.spans {
        out.push_str(&format!(
            "{:<20} {:>9} {:>10} {:>9} {:>8} {:>8} {:>5}\n",
            r.name,
            r.samples,
            format_millis(r.total_us),
            format_millis(r.mean_us),
            format_millis(r.p95_us),
            format_millis(r.max_us),
            r.slow
        ));
    }
    if !summary.counters.is_empty() {
        out.push_str(&format!("\n{:<28} {:>9}\n", "counter", "n"));
        for (name, n) in &summary.counters {
            out.push_str(&format!("{name:<28} {n:>9}\n"));
        }
    }
    out
}

/// The report for the log files in `directory`: the backup first, then the current log.
pub fn report(directory: &Path) -> Result<String, String> {
    let read = |name: &str| std::fs::read_to_string(directory.join(name)).ok();
    let (previous, current) = (read("aherald.previous.log"), read("aherald.log"));
    if previous.is_none() && current.is_none() {
        return Err(
            "no aherald.log beside the configuration; set general.logging = \"debug\" and run the resident"
                .into(),
        );
    }
    let text = format!(
        "{}\n{}",
        previous.unwrap_or_default(),
        current.unwrap_or_default()
    );
    Ok(render(&summarize(&text)))
}
