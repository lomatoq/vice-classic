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
// The corpus seal, and the reason it is a compiler flag rather than a test.
//
// `GtSourceGroup`, `GtScene` and `AmbiguityPair` are `pub(crate)`. These two
// lints are warn-by-default, which would let a public item hand one out with
// only a warning; denied here, EVERY syntax that would leak one - return type,
// type alias, public field, trait item, `impl Trait`, closure, `static`,
// `Deref` - is a hard error. Three previous attempts closed this class with a
// source scan and were walked around three times, the last one reaching all 22
// sealed-audit groups (REDTEAM RT45-A9, REVIEW M45-N14). A scan enumerates
// syntaxes; the type system does not have to.
#![deny(private_interfaces, private_bounds)]

pub mod artifact;
pub mod assets;
pub mod config;
pub mod corpus;
pub mod correlation;
pub mod corridor;
pub mod dcel;
pub mod envinfo;
pub mod error;
pub mod exec;
pub mod fit;
pub mod fsutil;
pub mod gates;
pub mod gt;
pub mod hashing;
pub mod limits;
pub mod oracle;
pub mod prereg;
pub mod reliability;
pub mod report;
pub mod runner;
pub mod scorecard;
pub mod topology;
pub mod universe;
