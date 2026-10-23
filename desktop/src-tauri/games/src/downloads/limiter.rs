use std::{
    sync::{LazyLock, Mutex},
    time::Duration,
};

use database::borrow_db_checked;
use download_manager::util::download_thread_control_flag::{
    DownloadThreadControl, DownloadThreadControlFlag,
};
use tokio::time::{Instant, sleep};

const MAX_THROTTLE_SLEEP: Duration = Duration::from_millis(100);
const THROTTLE_WINDOW_SECONDS: f64 = 0.1;
const MAX_THROTTLED_READ: usize = 64 * 1024;

fn burst_capacity(limit: u64) -> f64 {
    (limit as f64 * THROTTLE_WINDOW_SECONDS).max(1.0)
}

fn throttled_read_size(limit: u64, max_download_threads: usize, max_read_len: usize) -> usize {
    if limit == 0 {
        return max_read_len;
    }

    let threads = max_download_threads.max(1) as u64;
    (limit / 10 / threads)
        .max(1)
        .min(max_read_len as u64)
        .min(MAX_THROTTLED_READ as u64) as usize
}

struct LimiterState {
    limit: u64,
    tokens: f64,
    last_refill: Instant,
}

impl LimiterState {
    fn new() -> Self {
        Self {
            limit: 0,
            tokens: 0.0,
            last_refill: Instant::now(),
        }
    }

    fn refill(&mut self, now: Instant) {
        let elapsed = now
            .saturating_duration_since(self.last_refill)
            .as_secs_f64();
        self.last_refill = now;

        if self.limit == 0 {
            return;
        }

        self.tokens = (self.tokens + elapsed * self.limit as f64).min(burst_capacity(self.limit));
    }
}

/// Shared aggregate download limiter. The limit is in bytes per second, with 0
/// meaning unlimited. All download threads share the same token bucket.
struct DownloadLimiter {
    state: Mutex<LimiterState>,
}

impl DownloadLimiter {
    fn new() -> Self {
        Self {
            state: Mutex::new(LimiterState::new()),
        }
    }

    fn limit(&self) -> u64 {
        self.lock_state().limit
    }

    fn set_limit(&self, limit: u64) {
        let mut state = self.lock_state();
        let now = Instant::now();
        state.refill(now);
        state.limit = limit;
        state.last_refill = now;

        if limit == 0 {
            state.tokens = 0.0;
        } else {
            state.tokens = state.tokens.min(burst_capacity(limit));
        }
    }

    fn refund(&self, amount: u64) {
        if amount == 0 {
            return;
        }

        let mut state = self.lock_state();
        if state.limit == 0 {
            return;
        }

        state.refill(Instant::now());
        state.tokens = (state.tokens + amount as f64).min(burst_capacity(state.limit));
    }

    /// Waits until `amount` bytes may be read under the current aggregate
    /// limit. Returns false if the download is stopped while waiting.
    async fn acquire(&self, amount: u64, control_flag: &DownloadThreadControl) -> bool {
        if amount == 0 {
            return control_flag.get() != DownloadThreadControlFlag::Stop;
        }

        let mut remaining = amount;
        let mut acquired = 0u64;

        loop {
            if control_flag.get() == DownloadThreadControlFlag::Stop {
                self.refund(acquired);
                return false;
            }

            let wait = {
                let mut state = self.lock_state();
                if state.limit == 0 {
                    return control_flag.get() != DownloadThreadControlFlag::Stop;
                }

                state.refill(Instant::now());

                let chunk = remaining.min(burst_capacity(state.limit) as u64);
                let chunk_bytes = chunk as f64;
                if state.tokens >= chunk_bytes {
                    state.tokens -= chunk_bytes;
                    remaining -= chunk;
                    acquired += chunk;

                    if remaining == 0 {
                        drop(state);
                        if control_flag.get() == DownloadThreadControlFlag::Stop {
                            self.refund(acquired);
                            return false;
                        }
                        return true;
                    }

                    Duration::ZERO
                } else {
                    let missing = chunk_bytes - state.tokens;
                    Duration::from_secs_f64(missing / state.limit as f64)
                }
            };

            if wait.is_zero() {
                continue;
            }

            sleep(wait.min(MAX_THROTTLE_SLEEP)).await;
        }
    }

    fn lock_state(&self) -> std::sync::MutexGuard<'_, LimiterState> {
        self.state.lock().unwrap_or_else(|error| error.into_inner())
    }
}

impl Default for DownloadLimiter {
    fn default() -> Self {
        Self::new()
    }
}

static DOWNLOAD_LIMITER: LazyLock<DownloadLimiter> = LazyLock::new(DownloadLimiter::new);

/// Updates the aggregate limit used by all active and future downloads.
pub fn set_download_speed_limit(limit: u64) {
    DOWNLOAD_LIMITER.set_limit(limit);
}

/// Reads the persisted limit while holding the DB read guard, so a concurrent
/// settings save can only be applied before or after this update, never lost.
pub fn initialize_download_speed_limit_from_db() {
    let db = borrow_db_checked();
    set_download_speed_limit(db.settings.max_download_speed);
}

pub(crate) async fn throttle_bytes(amount: u64, control_flag: &DownloadThreadControl) -> bool {
    DOWNLOAD_LIMITER.acquire(amount, control_flag).await
}

pub(crate) fn refund(amount: u64) {
    DOWNLOAD_LIMITER.refund(amount);
}

pub(crate) fn suggested_read_size(max_download_threads: usize, max_read_len: usize) -> usize {
    throttled_read_size(DOWNLOAD_LIMITER.limit(), max_download_threads, max_read_len)
}

#[cfg(test)]
mod tests {
    use futures_util::future::join_all;
    use tokio::time::sleep;

    use super::*;

    fn go() -> DownloadThreadControl {
        DownloadThreadControl::new(DownloadThreadControlFlag::Go)
    }

    #[test]
    fn adaptive_read_size_tracks_limit_and_threads() {
        assert_eq!(throttled_read_size(0, 4, 1024 * 1024), 1024 * 1024);
        assert_eq!(throttled_read_size(100_000, 4, 1024 * 1024), 2_500);
        assert_eq!(throttled_read_size(100_000_000, 4, 1024 * 1024), 64 * 1024);
        assert_eq!(throttled_read_size(1, 4, 1024 * 1024), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn aggregate_limit_is_shared_between_tasks() {
        let limiter = DownloadLimiter::new();
        limiter.set_limit(100_000);
        let control = go();

        let start = Instant::now();
        let tasks = (0..4).map(|_| limiter.acquire(25_000, &control));
        let results = join_all(tasks).await;
        let elapsed = start.elapsed();

        assert!(results.into_iter().all(|result| result));
        assert!(
            elapsed >= Duration::from_millis(800),
            "aggregate limiter was bypassed: {elapsed:?}"
        );
        assert!(
            elapsed < Duration::from_secs(5),
            "aggregate limiter was much too slow: {elapsed:?}"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn zero_limit_is_unlimited() {
        let limiter = DownloadLimiter::new();
        limiter.set_limit(0);
        let control = go();

        let start = Instant::now();
        assert!(limiter.acquire(10_000_000, &control).await);
        assert_eq!(start.elapsed(), Duration::ZERO);
    }

    #[tokio::test(start_paused = true)]
    async fn changing_limit_updates_active_wait() {
        let limiter = std::sync::Arc::new(DownloadLimiter::new());
        limiter.set_limit(100_000);
        let control = go();

        let waiter_limiter = limiter.clone();
        let waiter_control = control.clone();
        let wait =
            tokio::spawn(async move { waiter_limiter.acquire(10_000_000, &waiter_control).await });

        sleep(Duration::from_millis(150)).await;
        limiter.set_limit(0);

        let start = Instant::now();
        assert!(wait.await.unwrap());
        assert!(start.elapsed() < Duration::from_millis(500));
    }

    #[tokio::test(start_paused = true)]
    async fn lowering_limit_applies_to_new_acquires() {
        let limiter = DownloadLimiter::new();
        limiter.set_limit(0);
        limiter.set_limit(100_000);
        let control = go();

        let start = Instant::now();
        assert!(limiter.acquire(100_000, &control).await);
        assert!(start.elapsed() >= Duration::from_millis(800));
    }

    #[tokio::test(start_paused = true)]
    async fn stop_interrupts_throttle_wait() {
        let limiter = std::sync::Arc::new(DownloadLimiter::new());
        limiter.set_limit(1_000);
        let control = go();

        let waiter_limiter = limiter.clone();
        let waiter_control = control.clone();
        let wait =
            tokio::spawn(async move { waiter_limiter.acquire(10_000_000, &waiter_control).await });

        sleep(Duration::from_millis(50)).await;
        control.set(DownloadThreadControlFlag::Stop);
        let stopped_at = Instant::now();

        assert!(!wait.await.unwrap());
        assert!(
            stopped_at.elapsed() < Duration::from_millis(300),
            "stop waited for the throttle sleep: {:?}",
            stopped_at.elapsed()
        );
    }

    #[tokio::test(start_paused = true)]
    async fn cancelled_wait_does_not_leak_budget() {
        let limiter = std::sync::Arc::new(DownloadLimiter::new());
        limiter.set_limit(100_000);
        let control = go();

        let waiter_limiter = limiter.clone();
        let waiter_control = control.clone();
        let wait =
            tokio::spawn(async move { waiter_limiter.acquire(10_000_000, &waiter_control).await });

        sleep(Duration::from_millis(150)).await;
        control.set(DownloadThreadControlFlag::Stop);
        assert!(!wait.await.unwrap());

        let next = go();
        let start = Instant::now();
        assert!(limiter.acquire(1_000, &next).await);
        assert!(start.elapsed() < Duration::from_millis(100));
    }

    #[tokio::test(start_paused = true)]
    async fn stopped_control_does_not_throttle() {
        let limiter = DownloadLimiter::new();
        limiter.set_limit(1_000);
        let control = DownloadThreadControl::new(DownloadThreadControlFlag::Stop);

        let start = Instant::now();
        assert!(!limiter.acquire(10_000_000, &control).await);
        assert_eq!(start.elapsed(), Duration::ZERO);
    }
}
