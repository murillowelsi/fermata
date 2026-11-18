//! A clock that works on the desktop and in the browser: `std::time::Instant` panics on wasm32-unknown-unknown, so
//! the web build uses `web_time::Instant` (performance.now()), which has the same API.
#[cfg(not(target_arch = "wasm32"))]
pub use std::time::Instant;
#[cfg(target_arch = "wasm32")]
pub use web_time::Instant;

/// Nanoseconds within the current second, for a cheap random number (Humanize).
pub fn subsec_nanos() -> u32 {
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|t| t.subsec_nanos()).unwrap_or(0)
    }
    #[cfg(target_arch = "wasm32")]
    {
        web_time::SystemTime::now().duration_since(web_time::UNIX_EPOCH).map(|t| t.subsec_nanos()).unwrap_or(0)
    }
}
