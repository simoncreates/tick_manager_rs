pub mod tickmanager;
pub use tickmanager::*;

pub mod tick_hook;
pub use tick_hook::*;

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    #[test]
    fn register_test() {
        let (_manager, handle) = TickManager::new(Speed::Fps(60));

        for i in 0..100 {
            let hook = TickMember::new(handle.clone());
            assert_eq!(hook.id, i);
        }
    }

    #[test]
    fn tick_test() {
        let (_manager, handle) = TickManager::new(Speed::Fps(60));
        let hook1 = Arc::new(TickMember::new(handle.clone()));
        let hook2 = Arc::new(TickMember::new(handle.clone()));

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
}
