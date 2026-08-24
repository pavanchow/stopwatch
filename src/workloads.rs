//! Real, instrumented work for the `stopwatch run` CLI. Each workload
//! is chosen so its self time and total time genuinely differ, which
//! is the whole point of separating the two numbers.

use crate::clock::SystemClock;
use crate::error::ProfilerError;
use crate::profiler::Profiler;

/// A small hand-written linear congruential generator, so the sort
/// workload gets pseudo-random input without pulling in an rng crate.
struct Lcg {
    state: u64,
}

impl Lcg {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        // Constants from Numerical Recipes' minimal standard LCG.
        self.state = self
            .state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.state
    }

    fn next_i32(&mut self, bound: i32) -> i32 {
        (self.next_u64() % bound as u64) as i32
    }
}

/// Naive recursive Fibonacci. Deliberately exponential, so the call
/// tree shows both deep recursion and a call-count explosion.
pub fn fib(profiler: &mut Profiler<SystemClock>, n: u64) -> Result<u64, ProfilerError> {
    profiler.span("fib", |p| -> Result<u64, ProfilerError> {
        if n < 2 {
            Ok(n)
        } else {
            let a = fib(p, n - 1)?;
            let b = fib(p, n - 2)?;
            Ok(a + b)
        }
    })?
}

fn bubble_sort(profiler: &mut Profiler<SystemClock>, arr: &mut [i32]) -> Result<(), ProfilerError> {
    profiler.span("bubble_sort", |_p| {
        let n = arr.len();
        for i in 0..n {
            for j in 0..n.saturating_sub(1).saturating_sub(i) {
                if arr[j] > arr[j + 1] {
                    arr.swap(j, j + 1);
                }
            }
        }
    })
}

fn partition(
    profiler: &mut Profiler<SystemClock>,
    arr: &mut [i32],
    lo: isize,
    hi: isize,
) -> Result<isize, ProfilerError> {
    profiler.span("partition", |_p| {
        let pivot = arr[hi as usize];
        let mut i = lo - 1;
        for j in lo..hi {
            if arr[j as usize] <= pivot {
                i += 1;
                arr.swap(i as usize, j as usize);
            }
        }
        arr.swap((i + 1) as usize, hi as usize);
        i + 1
    })
}

fn quick_sort_range(
    profiler: &mut Profiler<SystemClock>,
    arr: &mut [i32],
    lo: isize,
    hi: isize,
) -> Result<(), ProfilerError> {
    if lo >= hi {
        return Ok(());
    }
    let p = partition(profiler, arr, lo, hi)?;
    quick_sort_range(profiler, arr, lo, p - 1)?;
    quick_sort_range(profiler, arr, p + 1, hi)?;
    Ok(())
}

fn quick_sort(profiler: &mut Profiler<SystemClock>, arr: &mut [i32]) -> Result<(), ProfilerError> {
    let hi = arr.len() as isize - 1;
    profiler.span("quick_sort", |p| quick_sort_range(p, arr, 0, hi))?
}

fn random_array(len: usize, seed: u64) -> Vec<i32> {
    let mut lcg = Lcg::new(seed);
    (0..len).map(|_| lcg.next_i32(1_000_000)).collect()
}

/// Sorts a small array with bubble sort (all self time, no children)
/// and a larger one with quick sort (self time split across many
/// `partition` calls), so the two sorts land very differently in the
/// report.
pub fn sort(profiler: &mut Profiler<SystemClock>) -> Result<(), ProfilerError> {
    let mut small = random_array(400, 1);
    bubble_sort(profiler, &mut small)?;

    let mut big = random_array(4_000, 2);
    quick_sort(profiler, &mut big)?;
    Ok(())
}

fn horizontal_pass(
    profiler: &mut Profiler<SystemClock>,
    buf: &mut [u8],
    width: usize,
    height: usize,
    radius: i32,
) -> Result<(), ProfilerError> {
    profiler.span("horizontal_pass", |_p| {
        let src = buf.to_vec();
        for y in 0..height {
            for x in 0..width {
                let mut sum: u32 = 0;
                let mut count: u32 = 0;
                for dx in -radius..=radius {
                    let nx = x as i32 + dx;
                    if nx >= 0 && (nx as usize) < width {
                        sum += src[y * width + nx as usize] as u32;
                        count += 1;
                    }
                }
                buf[y * width + x] = (sum / count) as u8;
            }
        }
    })
}

fn vertical_pass(
    profiler: &mut Profiler<SystemClock>,
    buf: &mut [u8],
    width: usize,
    height: usize,
    radius: i32,
) -> Result<(), ProfilerError> {
    profiler.span("vertical_pass", |_p| {
        let src = buf.to_vec();
        for y in 0..height {
            for x in 0..width {
                let mut sum: u32 = 0;
                let mut count: u32 = 0;
                for dy in -radius..=radius {
                    let ny = y as i32 + dy;
                    if ny >= 0 && (ny as usize) < height {
                        sum += src[ny as usize * width + x] as u32;
                        count += 1;
                    }
                }
                buf[y * width + x] = (sum / count) as u8;
            }
        }
    })
}

/// A box blur over a small synthetic grayscale buffer, split into a
/// horizontal and a vertical pass, each its own span.
pub fn blur(profiler: &mut Profiler<SystemClock>) -> Result<Vec<u8>, ProfilerError> {
    let width = 128;
    let height = 128;
    let mut buffer: Vec<u8> = (0..(width * height)).map(|i| (i % 256) as u8).collect();

    profiler.span("blur", |p| -> Result<(), ProfilerError> {
        horizontal_pass(p, &mut buffer, width, height, 3)?;
        vertical_pass(p, &mut buffer, width, height, 3)?;
        Ok(())
    })??;

    Ok(buffer)
}

/// Runs all three workloads under one top-level span, so the report
/// shows how they compare side by side.
pub fn mixed(profiler: &mut Profiler<SystemClock>) -> Result<(), ProfilerError> {
    profiler.span("mixed", |p| -> Result<(), ProfilerError> {
        fib(p, 20)?;
        sort(p)?;
        blur(p)?;
        Ok(())
    })?
}
