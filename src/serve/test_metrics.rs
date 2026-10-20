//! Shared server test metrics.

use super::test_support::*;
use super::*;
#[cfg(target_os = "macos")]
pub(super) fn allocated_bytes() -> usize {
    #[repr(C)]
    #[derive(Default)]
    struct Statistics {
        blocks_in_use: u32,
        size_in_use: usize,
        max_size_in_use: usize,
        size_allocated: usize,
    }
    unsafe extern "C" {
        fn malloc_zone_statistics(zone: *mut std::ffi::c_void, statistics: *mut Statistics);
    }
    let mut statistics = Statistics::default();
    // Apple's malloc.h specifies that a null zone sums all allocation zones.
    unsafe {
        malloc_zone_statistics(std::ptr::null_mut(), &mut statistics);
    }
    statistics.size_in_use
}
pub(super) fn percentile(values: &mut [u128]) -> u128 {
    values.sort_unstable();
    values[values.len() * 95 / 100]
}
#[cfg(target_os = "macos")]
pub(super) fn live_soak(keys: usize, tools_only: bool) {
    let mut start_times = Vec::with_capacity(1000 * keys);
    let mut end_times = Vec::with_capacity(1000 * keys);
    let mut start_bytes = Vec::with_capacity(1000 * keys);
    let mut end_bytes = Vec::with_capacity(1000 * keys);
    let baseline = allocated_bytes();
    let mut states = (0..keys)
        .map(|_| LiveRows::empty(0, false))
        .collect::<Vec<_>>();
    let mut peak = 0;
    let mut warmed = 0;
    for index in 0..30_000 {
        for state in &mut states {
            let event = synthetic_event(index, tools_only);
            let timer = std::time::Instant::now();
            state.ingest(index as u64, index as u64 + 1, &[event]);
            let frame = state.delta("chat");
            let elapsed = timer.elapsed().as_nanos();
            if (1000..2000).contains(&index) {
                start_times.push(elapsed);
                start_bytes.push(frame.data.len() as u128);
            }
            if index >= 29_000 {
                end_times.push(elapsed);
                end_bytes.push(frame.data.len() as u128);
            }
            if index % 4 == 3 {
                let idle = state.delta("chat");
                let payload: Value = serde_json::from_str(&idle.data).unwrap();
                assert_eq!(payload["upsert"], json!([]));
            }
        }
        if index % 50 == 49 {
            peak = peak.max(allocated_bytes().saturating_sub(baseline));
        }
        if index == 9999 {
            warmed = allocated_bytes().saturating_sub(baseline);
        }
    }
    let retained = allocated_bytes().saturating_sub(baseline);
    println!(
        "soak keys={keys} tools_only={tools_only} simulated_seconds=600 lines_per_second=50 start_p95_ns={} end_p95_ns={} start_frame_p95_bytes={} end_frame_p95_bytes={} peak_retained_heap_bytes={peak} warmed_retained_heap_bytes={warmed} final_retained_heap_bytes={retained} records={:?}",
        percentile(&mut start_times),
        percentile(&mut end_times),
        percentile(&mut start_bytes),
        percentile(&mut end_bytes),
        states
            .iter()
            .map(|state| state.records.len())
            .collect::<Vec<_>>()
    );
    if tools_only {
        let state = &mut states[0];
        let timer = std::time::Instant::now();
        state.ingest(
            30_000,
            30_001,
            &[json!({"type":"turn_ended","reason":"completed"})],
        );
        let frame = state.delta("chat");
        println!(
            "turn_ended_ms={} frame_bytes={} retained_heap_bytes={} records={}",
            timer.elapsed().as_secs_f64() * 1000.,
            frame.data.len(),
            allocated_bytes().saturating_sub(baseline),
            state.records.len()
        );
    } else {
        assert!(
            states
                .iter()
                .all(|state| state.records.len() <= ROW_RECORD_LIMIT)
        );
        assert!(
            retained < warmed * 5 / 4,
            "closed-turn heap grew after the cache filled"
        );
    }
    assert!(
        percentile(&mut end_times) < percentile(&mut start_times) * 3 + 100_000,
        "live flush cost grew with history"
    );
    assert!(
        percentile(&mut end_bytes) < percentile(&mut start_bytes) * 2,
        "live frame bytes grew with history"
    );
}
