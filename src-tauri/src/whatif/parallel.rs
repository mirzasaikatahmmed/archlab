//! Runs independent jobs on all cores and keeps the results in order.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Mutex, PoisonError};
use std::thread;

use super::Progress;

/// Applies `work` to every item. Returns `None` if `cancel` was set before all items ran.
pub fn run_all<T: Sync, R: Send>(
    items: &[T],
    cancel: &AtomicBool,
    progress: Progress<'_>,
    work: impl Fn(&T) -> R + Sync,
) -> Option<Vec<R>> {
    let total = u32::try_from(items.len()).unwrap_or(u32::MAX);
    let next = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);
    let results: Mutex<Vec<Option<R>>> = Mutex::new((0..items.len()).map(|_| None).collect());
    let workers = thread::available_parallelism()
        .map_or(1, usize::from)
        .min(items.len().max(1));

    thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                while !cancel.load(Ordering::Relaxed) {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(item) = items.get(i) else { break };
                    let result = work(item);
                    results.lock().unwrap_or_else(PoisonError::into_inner)[i] = Some(result);
                    let finished = done.fetch_add(1, Ordering::Relaxed) + 1;
                    progress(u32::try_from(finished).unwrap_or(u32::MAX), total);
                }
            });
        }
    });

    let results = results.into_inner().unwrap_or_else(PoisonError::into_inner);
    results.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicU32;

    use super::*;

    #[test]
    fn results_keep_the_order_of_the_items() {
        let items: Vec<u32> = (0..50).collect();
        let out = run_all(&items, &AtomicBool::new(false), &|_, _| {}, |n| n * 2).expect("ran");
        assert_eq!(out, items.iter().map(|n| n * 2).collect::<Vec<_>>());
    }

    #[test]
    fn progress_counts_every_item() {
        let items = vec![1u32; 12];
        let last = AtomicU32::new(0);
        run_all(
            &items,
            &AtomicBool::new(false),
            &|done, total| {
                assert_eq!(total, 12);
                last.fetch_max(done, Ordering::Relaxed);
            },
            |n| *n,
        );
        assert_eq!(last.load(Ordering::Relaxed), 12);
    }

    #[test]
    fn cancelling_returns_nothing() {
        let items = vec![1u32; 10];
        assert!(run_all(&items, &AtomicBool::new(true), &|_, _| {}, |n| *n).is_none());
    }

    #[test]
    fn no_items_is_an_empty_result() {
        let items: Vec<u32> = Vec::new();
        assert_eq!(
            run_all(&items, &AtomicBool::new(false), &|_, _| {}, |n| *n),
            Some(Vec::new())
        );
    }
}
