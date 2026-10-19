//! A small trailing-edge debouncer.
//!
//! Closing a laptop lid or plugging in a monitor makes the OS fire a burst of display
//! notifications. We only want to re-layout once, after things have settled.

use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::thread;
use std::time::Duration;

/// Calls a function once, `delay` after the **last** of a burst of [`Debouncer::trigger`]
/// calls. The function runs on a background thread.
pub struct Debouncer {
    tx: Sender<()>,
}

impl Debouncer {
    /// Creates the debouncer and its background thread. The thread ends when the
    /// debouncer is dropped (any pending call is still delivered).
    pub fn new(delay: Duration, action: impl Fn() + Send + 'static) -> Self {
        let (tx, rx) = mpsc::channel::<()>();
        thread::Builder::new()
            .name("venecita-debounce".into())
            .spawn(move || {
                // Wait for the first trigger of a burst…
                while rx.recv().is_ok() {
                    // …then keep waiting while triggers keep coming.
                    loop {
                        match rx.recv_timeout(delay) {
                            Ok(()) => continue,
                            Err(RecvTimeoutError::Timeout) => {
                                action();
                                break;
                            }
                            Err(RecvTimeoutError::Disconnected) => {
                                action();
                                return;
                            }
                        }
                    }
                }
            })
            .expect("failed to spawn debounce thread");
        Self { tx }
    }

    /// Registers an occurrence. Cheap and callable from any thread.
    pub fn trigger(&self) {
        // The receiver only disappears if the thread panicked; nothing useful to do then.
        let _ = self.tx.send(());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Instant;

    #[test]
    fn burst_produces_a_single_call() {
        let count = Arc::new(AtomicUsize::new(0));
        let c = count.clone();
        let d = Debouncer::new(Duration::from_millis(50), move || {
            c.fetch_add(1, Ordering::SeqCst);
        });
        for _ in 0..10 {
            d.trigger();
            thread::sleep(Duration::from_millis(5));
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        while count.load(Ordering::SeqCst) == 0 && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        thread::sleep(Duration::from_millis(100));
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn separate_bursts_produce_separate_calls() {
        let count = Arc::new(AtomicUsize::new(0));
        let c = count.clone();
        let d = Debouncer::new(Duration::from_millis(20), move || {
            c.fetch_add(1, Ordering::SeqCst);
        });
        d.trigger();
        thread::sleep(Duration::from_millis(150));
        d.trigger();
        thread::sleep(Duration::from_millis(150));
        assert_eq!(count.load(Ordering::SeqCst), 2);
    }
}
