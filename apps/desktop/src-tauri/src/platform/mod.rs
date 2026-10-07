//! OS-specific glue. Code outside this module stays platform-neutral, so a macOS backend can
//! be added later by implementing the same functions.

#[cfg(windows)]
mod win;
#[cfg(windows)]
pub use win::*;

#[cfg(not(windows))]
mod unsupported;
#[cfg(not(windows))]
pub use unsupported::*;
