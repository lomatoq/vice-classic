//! vice-bench — M0 deterministic baseline runner.
//!
//! Scope (spec v1.3 §28 M0): execute the three pinned donor systems as
//! external black-box baselines over a small fixed smoke corpus, recording
//! binary/source/config/input/toolchain/environment hashes, runtime, exit
//! status and output artifact hashes. Typed errors per baseline; one
//! baseline's failure must never corrupt the report of the others.
//!
//! Explicitly NOT here (M0 non-goals): canonical IR, renderer, evidence,
//! topology, fitting, optimization, placeholder APIs for future milestones.

#![forbid(unsafe_code)]

pub mod assets;
pub mod config;
pub mod corpus;
pub mod envinfo;
pub mod error;
pub mod exec;
pub mod fsutil;
pub mod gt;
pub mod hashing;
pub mod limits;
pub mod report;
pub mod runner;
pub mod universe;
