//! Poison-tolerant locking.
//!
//! A `std::sync` lock is poisoned when a thread panics while holding it. Panicking again on every later `lock()` turns one
//! failure into a cascade that can take down a whole session or the host. Every state guarded by these locks stays valid
//! after a panic (plain maps, counters, queues), so the guard is recovered instead. Use this only where that holds; when a
//! half-updated state would be unsafe to continue with, return an error instead.
use std::sync::{Mutex, MutexGuard, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard};

pub trait LockExt<T> {
    fn lock_or_recover(&self) -> MutexGuard<'_, T>;
}

impl<T> LockExt<T> for Mutex<T> {
    fn lock_or_recover(&self) -> MutexGuard<'_, T> {
        self.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

pub trait RwLockExt<T> {
    fn read_or_recover(&self) -> RwLockReadGuard<'_, T>;
    fn write_or_recover(&self) -> RwLockWriteGuard<'_, T>;
}

impl<T> RwLockExt<T> for RwLock<T> {
    fn read_or_recover(&self) -> RwLockReadGuard<'_, T> {
        self.read().unwrap_or_else(PoisonError::into_inner)
    }
    fn write_or_recover(&self) -> RwLockWriteGuard<'_, T> {
        self.write().unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn mutex_poisoned_by_a_panic_is_recovered_with_its_value() {
        let mutex = Arc::new(Mutex::new(vec![1, 2]));
        let held = Arc::clone(&mutex);
        let _ = std::thread::spawn(move || {
            let mut guard = held.lock_or_recover();
            guard.push(3);
            panic!("poison the lock");
        })
        .join();
        assert!(mutex.is_poisoned());
        assert_eq!(*mutex.lock_or_recover(), vec![1, 2, 3]);
    }

    #[test]
    fn rwlock_poisoned_by_a_panic_is_recovered_for_read_and_write() {
        let lock = Arc::new(RwLock::new(1));
        let held = Arc::clone(&lock);
        let _ = std::thread::spawn(move || {
            let mut guard = held.write_or_recover();
            *guard = 2;
            panic!("poison the lock");
        })
        .join();
        assert!(lock.is_poisoned());
        assert_eq!(*lock.read_or_recover(), 2);
        *lock.write_or_recover() = 3;
        assert_eq!(*lock.read_or_recover(), 3);
    }
}
