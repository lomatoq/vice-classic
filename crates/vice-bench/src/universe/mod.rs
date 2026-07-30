//! Compatibility re-export of the production-owned supported model universe.
//!
//! M3 introduced the declaration in the benchmark crate before inference
//! existed. M7 posterior/confidence now consumes it in production, so
//! `vice-opt` is its single owner and the benchmark judges import that same
//! value rather than maintaining a second copy.

pub use vice_opt::universe::*;

#[cfg(test)]
mod tests;
