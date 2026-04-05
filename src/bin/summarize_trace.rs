use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize)]
struct TraceEvent {
    #[serde(default)]
    name: String,
    #[serde(default)]
    ph: String,
    #[serde(default)]
    ts: f64,
    #[serde(default)]
    dur: f64,
    #[serde(default)]
    tid: i64,
}

struct SpanStats {
    durations: Vec<f64>,
}

impl SpanStats {
    fn calls(&self) -> usize {
        self.durations.len()
    }

    fn total(&self) -> f64 {
        self.durations.iter().sum()
    }

    fn mean(&self) -> f64 {
        self.total() / self.calls() as f64
    }

    fn median(&self) -> f64 {
        let mut sorted = self.durations.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let mid = sorted.len() / 2;
        if sorted.len() % 2 == 0 {
            (sorted[mid - 1] + sorted[mid]) / 2.0
        } else {
            sorted[mid]
        }
    }

    fn max(&self) -> f64 {
        self.durations.iter().cloned().fold(0.0, f64::max)
    }

    fn min(&self) -> f64 {
        self.durations.iter().cloned().fold(f64::MAX, f64::min)
    }
}

fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("Usage: summarize-trace <trace.json>");
        std::process::exit(1);
    });

    let data = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        eprintln!("Failed to read {path}: {e}");
        std::process::exit(1);
    });

    // Chrome trace JSON may have a trailing comma or be wrapped in {"traceEvents": [...]}
    let events: Vec<TraceEvent> = if data.trim_start().starts_with('[') {
        serde_json::from_str(&data).unwrap_or_else(|e| {
            eprintln!("Failed to parse JSON array: {e}");
            std::process::exit(1);
        })
    } else {
        #[derive(Deserialize)]
        struct Wrapper {
            #[serde(rename = "traceEvents")]
            trace_events: Vec<TraceEvent>,
        }
        let wrapper: Wrapper = serde_json::from_str(&data).unwrap_or_else(|e| {
            eprintln!("Failed to parse JSON object: {e}");
            std::process::exit(1);
        });
        wrapper.trace_events
    };

    // Match B/E pairs and X events into durations
    let mut spans: HashMap<String, SpanStats> = HashMap::new();
    let mut open_spans: HashMap<(String, i64), f64> = HashMap::new();

    for event in &events {
        match event.ph.as_str() {
            "X" if event.dur > 0.0 => {
                spans
                    .entry(event.name.clone())
                    .or_insert_with(|| SpanStats {
                        durations: Vec::new(),
                    })
                    .durations
                    .push(event.dur);
            }
            "B" => {
                open_spans.insert((event.name.clone(), event.tid), event.ts);
            }
            "E" => {
                if let Some(start) = open_spans.remove(&(event.name.clone(), event.tid)) {
                    let duration = event.ts - start;
                    if duration > 0.0 {
                        spans
                            .entry(event.name.clone())
                            .or_insert_with(|| SpanStats {
                                durations: Vec::new(),
                            })
                            .durations
                            .push(duration);
                    }
                }
            }
            _ => {}
        }
    }

    let mut sorted: Vec<_> = spans.into_iter().collect();
    sorted.sort_by(|a, b| b.1.total().partial_cmp(&a.1.total()).unwrap_or(std::cmp::Ordering::Equal));

    println!("name,calls,total_us,mean_us,median_us,max_us,min_us");
    for (name, stats) in &sorted {
        println!(
            "{},{},{:.0},{:.1},{:.1},{:.1},{:.1}",
            name,
            stats.calls(),
            stats.total(),
            stats.mean(),
            stats.median(),
            stats.max(),
            stats.min(),
        );
    }
}
