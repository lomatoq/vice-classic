//! Factorial oracle harness (spec v1.3 §27.6, §28 M3.5).
//!
//! M3.5 builds the MEASUREMENT frame for causal attribution, before there is
//! an algorithm to attribute anything to (§32 rule 2). What it owes, in the
//! spec's own words: *"O0/G30 renderer ceiling; PF 2×2 where the reference
//! backend supports honest injection; intervention schemas / compatibility
//! keys; later arms `not_yet_applicable`"*, gated by *"no causal deltas
//! across incompatible runs; inverse-crime warning visible"*.
//!
//! This commit is the frame itself — the part that decides what MAY be
//! measured and what may be subtracted from what. Each module makes one
//! thing impossible rather than discouraged:
//!
//! | module | what it makes impossible |
//! |---|---|
//! | [`design`] | an arm that is absent without saying what is missing and who owns it |
//! | [`key`] | a delta assembled from arms of two different runs |
//! | [`crime`] | an aggregate that loses an inverse-crime warning |
//! | [`effects`] | a main effect published from a sequential difference |
//!
//! What M3.5 will be able to measure is ONE arm of the 2×2: `PF11`, GT
//! partition and GT formation, which is also `G30`. The harness has no
//! partitioner (M4.5) and no formation estimator (M4), so `PF00`, `PF01` and
//! `PF10` are typed refusals — and because every factorial effect needs all
//! four arms, so are all three effects. That is the design's own answer, not
//! a gap papered over: publishing `PF11 − PF10` under the name "partition
//! main effect" would be the order-dependent ladder §27.6 abolished.

pub mod crime;
pub mod design;
pub mod effects;
pub mod key;
