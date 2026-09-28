//! Small, bounded groups of blocking archive reads. These run on OS threads,
//! never the renderer's Rayon pool or a Tokio runtime worker. `try_join` is
//! copied from VectorEffects' `ve-zarr`.

use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::error::Result;

/// Complete both reads before returning, including when either fails.
pub fn try_join<A: Send, B>(
    left: impl FnOnce() -> Result<A> + Send,
    right: impl FnOnce() -> Result<B>,
) -> Result<(A, B)> {
    std::thread::scope(|scope| {
        let left = scope.spawn(left);
        let right = right();
        let left = left
            .join()
            .unwrap_or_else(|panic| std::panic::resume_unwind(panic));
        Ok((left?, right?))
    })
}

/// Runs `f` over `items` on at most `concurrency` threads and returns the
/// results in input order. The first error is returned once every started
/// call has finished; no new call starts after it.
pub fn map_bounded<T: Sync, R: Send>(
    items: &[T],
    concurrency: usize,
    f: impl Fn(&T) -> Result<R> + Sync,
) -> Result<Vec<R>> {
    let next = AtomicUsize::new(0);
    let failed = std::sync::atomic::AtomicBool::new(false);
    let results: Mutex<Vec<Option<Result<R>>>> =
        Mutex::new((0..items.len()).map(|_| None).collect());
    let workers = concurrency.clamp(1, items.len().max(1));
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                loop {
                    if failed.load(Ordering::SeqCst) {
                        break;
                    }
                    let i = next.fetch_add(1, Ordering::SeqCst);
                    let Some(item) = items.get(i) else {
                        break;
                    };
                    let r = f(item);
                    if r.is_err() {
                        failed.store(true, Ordering::SeqCst);
                    }
                    if let Ok(mut slots) = results.lock() {
                        slots[i] = Some(r);
                    }
                }
            });
        }
    });
    let slots = results
        .into_inner()
        .map_err(|_| crate::EnvError::Cancelled)?;
    let mut out = Vec::with_capacity(slots.len());
    let mut first_error = None;
    for slot in slots {
        match slot {
            Some(Ok(r)) => out.push(r),
            Some(Err(e)) => {
                first_error.get_or_insert(e);
            }
            None => {}
        }
    }
    match first_error {
        Some(e) => Err(e),
        None => Ok(out),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EnvError;
    use std::sync::mpsc::channel;
    use std::time::Duration;

    #[test]
    fn independent_reads_can_make_progress_together() {
        let (tx, rx) = channel();
        let pair = try_join(
            move || {
                rx.recv_timeout(Duration::from_secs(5))
                    .map_err(|e| EnvError::Open(e.to_string()))?;
                Ok("eastward")
            },
            || {
                tx.send(()).expect("reader is waiting");
                Ok("northward")
            },
        )
        .expect("both complete");
        assert_eq!(pair, ("eastward", "northward"));
    }

    #[test]
    fn a_bounded_map_keeps_order_and_limits_threads() {
        let live = AtomicUsize::new(0);
        let peak = AtomicUsize::new(0);
        let items: Vec<u32> = (0..40).collect();
        let out = map_bounded(&items, 3, |&i| {
            let now = live.fetch_add(1, Ordering::SeqCst) + 1;
            peak.fetch_max(now, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(2));
            live.fetch_sub(1, Ordering::SeqCst);
            Ok(i * 2)
        })
        .expect("all succeed");
        assert_eq!(out, items.iter().map(|i| i * 2).collect::<Vec<_>>());
        assert!(peak.load(Ordering::SeqCst) <= 3);
    }

    #[test]
    fn a_bounded_map_reports_an_error() {
        let items: Vec<u32> = (0..10).collect();
        let err = map_bounded(&items, 4, |&i| {
            if i == 5 {
                Err(EnvError::Open("chunk 5".into()))
            } else {
                Ok(i)
            }
        })
        .expect_err("fails");
        assert!(err.to_string().contains("chunk 5"));
    }
}
