pub mod tickmanager;
pub use tickmanager::*;

pub mod tick_hook;
pub use tick_hook::*;

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use std::time::{Duration, Instant};

    use super::*;

    #[test]
    fn register_test() {
        let (_manager, handle) = TickManager::new(Speed::Fps(60));

        for i in 0..100 {
            let hook = TickMember::new(handle.clone(), 1);
            assert_eq!(hook.id, i);
        }
    }

    #[test]
    fn tick_test() {
        let (_manager, handle) = TickManager::new(Speed::Fps(60));
        let hook1 = Arc::new(TickMember::new(handle.clone(), 1));
        let hook2 = Arc::new(TickMember::new(handle.clone(), 1));

        let join1 = {
            let hook1 = hook1.clone();
            std::thread::spawn(move || {
                for _ in 0..10 {
                    hook1.wait_for_tick();
                }
            })
        };
        let join2 = {
            let hook2 = hook2.clone();
            std::thread::spawn(move || {
                for _ in 0..10 {
                    hook2.wait_for_tick();
                }
            })
        };

        join1.join().unwrap();
        join2.join().unwrap();

        assert_eq!(hook1.id, 0);
        assert_eq!(hook2.id, 1);
    }

    /// Ensure ids are increasing properly
    #[test]
    fn id_monotonic_after_drop() {
        let (_manager, handle) = TickManager::new(Speed::Fps(60));

        let mut ids = Vec::new();
        for _ in 0..10 {
            let hook = TickMember::new(handle.clone(), 1);
            ids.push(hook.id);
        }
        assert_eq!(ids.len(), 10);
        ids.drain(0..5);

        let mut new_ids = Vec::new();
        for _ in 0..5 {
            let hook = TickMember::new(handle.clone(), 1);
            new_ids.push(hook.id);
        }

        let last_old = ids.last().copied().unwrap_or(4);
        assert!(
            new_ids.first().copied().unwrap() > last_old,
            "expected new ids to continue after previous ids"
        );
    }

    #[test]
    fn speed_factor_counts() {
        let (_manager, handle) = TickManager::new(Speed::Fps(120));

        let fast_ticks = 12;
        let half_ticks = fast_ticks / 2;

        let fast = Arc::new(TickMember::new(handle.clone(), 1));
        let half = Arc::new(TickMember::new(handle.clone(), 2));

        let fast_count = Arc::new(AtomicUsize::new(0));
        let half_count = Arc::new(AtomicUsize::new(0));

        let j1 = {
            let fast = fast.clone();
            let c = fast_count.clone();
            std::thread::spawn(move || {
                for _ in 0..fast_ticks {
                    fast.wait_for_tick();
                    c.fetch_add(1, Ordering::SeqCst);
                }
            })
        };

        let j2 = {
            let half = half.clone();
            let c = half_count.clone();
            std::thread::spawn(move || {
                for _ in 0..half_ticks {
                    half.wait_for_tick();
                    c.fetch_add(1, Ordering::SeqCst);
                }
            })
        };

        j1.join().unwrap();
        j2.join().unwrap();

        assert_eq!(fast_count.load(Ordering::SeqCst), fast_ticks);
        assert_eq!(half_count.load(Ordering::SeqCst), half_ticks);
    }

    /// Ensure that a slow member does not block a fast member indefinitely.
    #[test]
    fn nonblocking_slow_member() {
        let (_manager, handle) = TickManager::new(Speed::Fps(120));

        let _slow = Arc::new(TickMember::new(handle.clone(), 100));
        let fast = Arc::new(TickMember::new(handle.clone(), 1));

        let fast_count = Arc::new(AtomicUsize::new(0));
        let j_fast = {
            let fast = fast.clone();
            let c = fast_count.clone();
            std::thread::spawn(move || {
                for _ in 0..8 {
                    fast.wait_for_tick();
                    c.fetch_add(1, Ordering::SeqCst);
                }
            })
        };

        j_fast.join().unwrap();

        assert_eq!(
            fast_count.load(Ordering::SeqCst),
            8,
            "fast member should not be blocked by slow member"
        );
    }

    /// Time-sensitive test for Interval speed
    #[test]
    fn interval_timing_approximation() {
        let (_manager, handle) = TickManager::new(Speed::Interval(Duration::from_millis(50)));

        let member = Arc::new(TickMember::new(handle.clone(), 1));
        member.wait_for_tick();
        let t0 = Instant::now();
        member.wait_for_tick();
        let dt = t0.elapsed();

        assert!(
            dt >= Duration::from_millis(40),
            "interval too small: {:?}",
            dt
        );
    }
}
