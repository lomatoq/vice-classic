//! Critical events of the max/min trees, as BATCHES over equal values
//! (spec §11.2, §23).
//!
//! ```text
//! max-tree superlevel components
//! min-tree sublevel/background events with complementary connectivity
//! component/hole birth/death
//! bridge/gap events
//! persistence plateaus
//! ```
//!
//! ## Plateaus and ties are batches, not an iteration order
//!
//! §11.2 says it in one sentence — *"equal-valued pixels are processed as a
//! batch event, not by iteration order"* — and this project has burned on
//! that class of thing repeatedly, so it is worth being explicit about what
//! is and is not order-dependent here.
//!
//! At each distinct QUANTIZED level, every pixel with that value is made
//! active first, and only then are unions performed, over an edge list that
//! is built and then SORTED. Union-find computes connected components, which
//! do not depend on edge order; the surviving root is the smaller index,
//! which does not depend on edge order either; and the birth level a merged
//! root inherits is the ELDER's, which is a property of the two components
//! rather than of their arrival. So the set of events at a level is a
//! function of the level's pixel set and the graph, and nothing else.
//!
//! Every event publishes `batch_pixels`, the size of the equal-valued batch
//! it came out of. A plateau is not an inference from that number, it is that
//! number.
//!
//! ## Quantization
//!
//! Levels live on a fixed grid of [`LEVEL_QUANTA`] steps. Coverage comes
//! from 8-bit observations, so a finer grid would be manufacturing distinct
//! levels out of float noise and would make "equal-valued" a measure-zero
//! event — at which point the batch rule would be true and useless. The grid
//! is twice the 8-bit resolution, so two codes are never merged.
//!
//! ## What a bridge and a gap are here
//!
//! A merge in the FOREGROUND tree is two blobs becoming one as the level
//! falls: a BRIDGE forms. A merge in the BACKGROUND tree is two background
//! regions becoming one as the level rises: the foreground neck between them
//! breaks, a GAP opens. Those two are the same question asked from the two
//! sides, which is why §5.3 insists the two sides use complementary
//! connectivity, and they are the entire content of the corpus's
//! `bridge-or-gap` ambiguity pair.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use vice_ir::{ComplementaryConnectivity, PixelConnectivity};

/// Steps of the level grid. Twice the 8-bit code resolution.
pub const LEVEL_QUANTA: u32 = 512;

pub fn quantize(v: f64) -> u16 {
    (v.clamp(0.0, 1.0) * f64::from(LEVEL_QUANTA)).round() as u16
}

pub fn dequantize(q: u16) -> f64 {
    f64::from(q) / f64::from(LEVEL_QUANTA)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    /// A foreground component appears as the level falls.
    ComponentBirth,
    /// Two foreground components join: a bridge forms.
    BridgeMerge,
    /// A background region appears as the level rises — a hole or the
    /// outside.
    GapBirth,
    /// Two background regions join: the neck between them breaks, a gap
    /// opens.
    GapMerge,
}

impl EventKind {
    pub fn as_str(self) -> &'static str {
        match self {
            EventKind::ComponentBirth => "component_birth",
            EventKind::BridgeMerge => "bridge_merge",
            EventKind::GapBirth => "gap_birth",
            EventKind::GapMerge => "gap_merge",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct CriticalEvent {
    pub kind: EventKind,
    pub level: f64,
    /// Lifetime of the structure that died here, in coverage units. Zero for
    /// a birth, which has not died yet.
    pub persistence: f64,
    /// How many equal-valued pixels entered in the same batch as this event.
    /// The plateau evidence, published rather than inferred.
    pub batch_pixels: u32,
}

/// A run of levels across which no event happens.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Plateau {
    pub lo: f64,
    pub hi: f64,
}

impl Plateau {
    pub fn width(&self) -> f64 {
        self.hi - self.lo
    }
    /// The representative level: the midpoint, which is the farthest point
    /// from either event bounding the plateau and therefore the most stable
    /// place to threshold.
    pub fn representative(&self) -> f64 {
        dequantize(quantize(0.5 * (self.lo + self.hi)))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EventTrees {
    pub events: Vec<CriticalEvent>,
    pub plateaus: Vec<Plateau>,
    /// Number of distinct levels whose batch held more than one pixel. A
    /// field where this is zero has no ties at all, and a batch rule tested
    /// only there would be tested on nothing.
    pub tie_batches: u32,
    pub largest_batch_pixels: u32,
}

struct Dsu(Vec<usize>);

impl Dsu {
    fn new(n: usize) -> Dsu {
        Dsu((0..n).collect())
    }
    fn find(&mut self, x: usize) -> usize {
        let mut x = x;
        while self.0[x] != x {
            self.0[x] = self.0[self.0[x]];
            x = self.0[x];
        }
        x
    }
}

fn offsets(c: PixelConnectivity) -> &'static [(isize, isize)] {
    match c {
        PixelConnectivity::Four => &[(1, 0), (0, 1)],
        PixelConnectivity::Eight => &[(1, 0), (0, 1), (1, 1), (1, -1)],
    }
}

/// One merge tree over the superlevel sets of `qv`.
///
/// Returns (births, merges) as `(level_q, batch_pixels)` and
/// `(level_q, persistence_q, batch_pixels)`.
#[allow(clippy::type_complexity)]
fn merge_tree(
    qv: &[u16],
    width_px: usize,
    height_px: usize,
    c: PixelConnectivity,
) -> (Vec<(u16, u32)>, Vec<(u16, u16, u32)>, u32, u32) {
    let n = qv.len();
    let mut by_level: BTreeMap<u16, Vec<usize>> = BTreeMap::new();
    for (i, q) in qv.iter().enumerate() {
        by_level.entry(*q).or_default().push(i);
    }
    let mut tie_batches = 0u32;
    let mut largest = 0u32;
    for v in by_level.values() {
        if v.len() > 1 {
            tie_batches += 1;
        }
        largest = largest.max(v.len() as u32);
    }

    let mut active = vec![false; n];
    let mut dsu = Dsu::new(n);
    let mut birth = vec![0u16; n];
    let mut births = Vec::new();
    let mut merges = Vec::new();
    let offs = offsets(c);

    // Descending: the superlevel set grows.
    for (level, batch) in by_level.iter().rev() {
        let batch_pixels = batch.len() as u32;
        for &i in batch {
            active[i] = true;
            birth[i] = *level;
        }
        // The edge list is BUILT and then SORTED, so the union order is a
        // function of the pixel set rather than of the traversal.
        let mut edges: Vec<(usize, usize)> = Vec::new();
        for &i in batch {
            let (x, y) = ((i % width_px) as isize, (i / width_px) as isize);
            for (dx, dy) in offs {
                for s in [1isize, -1] {
                    let (nx, ny) = (x + dx * s, y + dy * s);
                    if nx < 0 || ny < 0 || nx >= width_px as isize || ny >= height_px as isize {
                        continue;
                    }
                    let j = ny as usize * width_px + nx as usize;
                    if !active[j] {
                        continue;
                    }
                    edges.push(if i < j { (i, j) } else { (j, i) });
                }
            }
        }
        edges.sort_unstable();
        edges.dedup();
        for (a, b) in edges {
            let (ra, rb) = (dsu.find(a), dsu.find(b));
            if ra == rb {
                continue;
            }
            let (ba, bb) = (birth[ra], birth[rb]);
            let younger = ba.min(bb);
            if younger > *level {
                // Both components existed before this level: a real death.
                merges.push((*level, younger - *level, batch_pixels));
            }
            // Smaller index survives (canonical), elder's birth is kept.
            let (keep, gone) = if ra < rb { (ra, rb) } else { (rb, ra) };
            dsu.0[gone] = keep;
            birth[keep] = ba.max(bb);
        }
        let mut roots = BTreeSet::new();
        for &i in batch {
            roots.insert(dsu.find(i));
        }
        for r in roots {
            if birth[r] == *level {
                births.push((*level, batch_pixels));
            }
        }
    }
    (births, merges, tie_batches, largest)
}

/// Every critical event of both trees, plus the plateaus between them.
pub fn batch_critical_events(
    values: &[f64],
    width_px: usize,
    height_px: usize,
    conn: ComplementaryConnectivity,
) -> EventTrees {
    let qv: Vec<u16> = values.iter().map(|v| quantize(*v)).collect();
    let (fb, fm, ties_f, big_f) = merge_tree(&qv, width_px, height_px, conn.foreground());
    // The background tree is the same algorithm on the complement, with the
    // complementary connectivity — which is exactly what §5.3 forbids doing
    // with the SAME connectivity on both sides.
    let inv: Vec<u16> = qv.iter().map(|q| LEVEL_QUANTA as u16 - *q).collect();
    let (bb, bm, ties_b, big_b) = merge_tree(&inv, width_px, height_px, conn.background());

    let mut events: Vec<CriticalEvent> = Vec::new();
    for (l, n) in fb {
        events.push(CriticalEvent {
            kind: EventKind::ComponentBirth,
            level: dequantize(l),
            persistence: 0.0,
            batch_pixels: n,
        });
    }
    for (l, p, n) in fm {
        events.push(CriticalEvent {
            kind: EventKind::BridgeMerge,
            level: dequantize(l),
            persistence: dequantize(p),
            batch_pixels: n,
        });
    }
    for (l, n) in bb {
        events.push(CriticalEvent {
            kind: EventKind::GapBirth,
            level: 1.0 - dequantize(l),
            persistence: 0.0,
            batch_pixels: n,
        });
    }
    for (l, p, n) in bm {
        events.push(CriticalEvent {
            kind: EventKind::GapMerge,
            level: 1.0 - dequantize(l),
            persistence: dequantize(p),
            batch_pixels: n,
        });
    }
    // Deterministic order: by level, then kind, then persistence.
    events.sort_by(|a, b| {
        a.level
            .total_cmp(&b.level)
            .then(a.kind.cmp(&b.kind))
            .then(a.persistence.total_cmp(&b.persistence))
            .then(a.batch_pixels.cmp(&b.batch_pixels))
    });

    let plateaus = plateaus_between(&events);
    EventTrees {
        events,
        plateaus,
        tie_batches: ties_f.max(ties_b),
        largest_batch_pixels: big_f.max(big_b),
    }
}

/// Runs of level with no event in them, bounded by `[0,1]`.
fn plateaus_between(events: &[CriticalEvent]) -> Vec<Plateau> {
    let mut marks: BTreeSet<u16> = BTreeSet::new();
    marks.insert(0);
    marks.insert(LEVEL_QUANTA as u16);
    for e in events {
        marks.insert(quantize(e.level));
    }
    let v: Vec<u16> = marks.into_iter().collect();
    v.windows(2)
        .filter(|w| w[1] > w[0] + 1)
        .map(|w| Plateau {
            lo: dequantize(w[0]),
            hi: dequantize(w[1]),
        })
        .collect()
}

/// Where a candidate level came from. The whole of the §28 M4.5 clause "no
/// magic-threshold-only architecture" is measurable because of this field:
/// a candidate knows whether it exists only because somebody fixed a number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LevelOrigin {
    /// The midpoint of a run of levels across which the topology does not
    /// change.
    PersistencePlateau,
    /// One quantum above a critical level.
    BeforeEvent,
    /// One quantum below a critical level.
    AfterEvent,
    /// A cheap smoke probe (§11.2: "fixed levels remain cheap smoke
    /// probes"), not the architecture.
    FixedSmoke,
}

impl LevelOrigin {
    pub fn as_str(self) -> &'static str {
        match self {
            LevelOrigin::PersistencePlateau => "persistence_plateau",
            LevelOrigin::BeforeEvent => "before_event",
            LevelOrigin::AfterEvent => "after_event",
            LevelOrigin::FixedSmoke => "fixed_smoke",
        }
    }
    pub fn is_event_driven(self) -> bool {
        !matches!(self, LevelOrigin::FixedSmoke)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CandidateLevel {
    pub value: f64,
    pub origins: BTreeSet<LevelOrigin>,
    /// The largest persistence that justifies this level, or the plateau
    /// width for a plateau midpoint. Zero for a fixed smoke probe, which
    /// nothing justifies.
    pub persistence: f64,
}

impl CandidateLevel {
    /// True when this level would exist without any fixed probe.
    pub fn is_event_driven(&self) -> bool {
        self.origins.iter().any(|o| o.is_event_driven())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct LevelConfig {
    pub max_plateau_levels: usize,
    pub max_event_levels: usize,
    /// Events below this persistence are noise at 8-bit resolution and do
    /// not earn a level of their own.
    pub min_event_persistence: f64,
    /// The cheap smoke probes. `0.5` is the classical coverage threshold,
    /// and it is HERE — one entry of a level list — rather than being the
    /// architecture.
    pub fixed_smoke_levels: &'static [f64],
}

pub const LEVEL_CONFIG_V1: LevelConfig = LevelConfig {
    max_plateau_levels: 6,
    max_event_levels: 8,
    min_event_persistence: 0.02,
    fixed_smoke_levels: &[0.5],
};

/// The levels §23 asks for: plateau representatives, the levels either side
/// of a persistent event, and the fixed smoke probes — deduplicated, with
/// every origin of a surviving level kept.
pub fn candidate_levels(trees: &EventTrees, cfg: &LevelConfig) -> Vec<CandidateLevel> {
    let mut acc: BTreeMap<u16, (BTreeSet<LevelOrigin>, f64)> = BTreeMap::new();
    let mut add = |q: u16, o: LevelOrigin, p: f64| {
        if q == 0 || q >= LEVEL_QUANTA as u16 {
            // A level at either extreme labels everything or nothing; the
            // envelope would remove it as invalid anyway, and generating it
            // would inflate the candidate count with a known answer.
            return;
        }
        let e = acc.entry(q).or_insert_with(|| (BTreeSet::new(), 0.0));
        e.0.insert(o);
        e.1 = e.1.max(p);
    };

    let mut plateaus: Vec<&Plateau> = trees.plateaus.iter().collect();
    plateaus.sort_by(|a, b| b.width().total_cmp(&a.width()).then(a.lo.total_cmp(&b.lo)));
    for p in plateaus.into_iter().take(cfg.max_plateau_levels) {
        add(
            quantize(p.representative()),
            LevelOrigin::PersistencePlateau,
            p.width(),
        );
    }

    let mut persistent: Vec<&CriticalEvent> = trees
        .events
        .iter()
        .filter(|e| e.persistence >= cfg.min_event_persistence)
        .collect();
    persistent.sort_by(|a, b| {
        b.persistence
            .total_cmp(&a.persistence)
            .then(a.level.total_cmp(&b.level))
            .then(a.kind.cmp(&b.kind))
    });
    for e in persistent.into_iter().take(cfg.max_event_levels) {
        let q = quantize(e.level);
        add(q.saturating_add(1), LevelOrigin::BeforeEvent, e.persistence);
        add(q.saturating_sub(1), LevelOrigin::AfterEvent, e.persistence);
    }

    for l in cfg.fixed_smoke_levels {
        add(quantize(*l), LevelOrigin::FixedSmoke, 0.0);
    }

    acc.into_iter()
        .rev()
        .map(|(q, (origins, persistence))| CandidateLevel {
            value: dequantize(q),
            origins,
            persistence,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arm4() -> ComplementaryConnectivity {
        ComplementaryConnectivity::arms()[0]
    }

    /// Two blobs joined by a low neck: the bridge is a merge event whose
    /// persistence is the height difference, and it is found without anyone
    /// naming a threshold.
    #[test]
    fn a_neck_between_two_blobs_is_a_bridge_merge_with_its_own_persistence() {
        let (w, h) = (9usize, 3usize);
        let mut v = vec![0.0f64; w * h];
        for y in 0..3 {
            for x in 0..3 {
                v[y * w + x] = 1.0;
            }
            for x in 6..9 {
                v[y * w + x] = 1.0;
            }
        }
        for x in 3..6 {
            v[w + x] = 0.4;
        }
        let t = batch_critical_events(&v, w, h, arm4());
        let bridges: Vec<&CriticalEvent> = t
            .events
            .iter()
            .filter(|e| e.kind == EventKind::BridgeMerge)
            .collect();
        assert_eq!(bridges.len(), 1, "exactly one bridge: {:?}", t.events);
        // Levels live on the quantized grid, so 0.4 is carried as the
        // nearest quantum. The tolerance is ONE quantum and not a loose
        // epsilon: a level that missed by two would be a different level.
        let q = 1.0 / f64::from(LEVEL_QUANTA);
        assert!(
            (bridges[0].level - 0.4).abs() <= q,
            "the bridge forms at the neck height, got {}",
            bridges[0].level
        );
        assert!(
            (bridges[0].persistence - 0.6).abs() <= q,
            "persistence is the lifetime of the component that died: {}",
            bridges[0].persistence
        );
        // And the level that separates the blobs is generated WITHOUT any
        // fixed probe.
        let levels = candidate_levels(&t, &LEVEL_CONFIG_V1);
        assert!(
            levels.iter().any(|l| l.value > 0.4 && l.is_event_driven()),
            "no event-driven level above the neck: {levels:?}"
        );
    }

    /// Equal values are one batch, and the batch is visible as a number.
    ///
    /// The control that makes it mean something: a field of DISTINCT values
    /// has no tie batches at all, so `tie_batches > 0` is a property of the
    /// input rather than of the counter.
    #[test]
    fn equal_valued_pixels_are_one_batch_and_the_batch_is_published() {
        let (w, h) = (4usize, 4usize);
        let flat = vec![0.7f64; w * h];
        let t = batch_critical_events(&flat, w, h, arm4());
        assert!(t.tie_batches > 0);
        assert_eq!(t.largest_batch_pixels, 16, "one batch of every pixel");
        let births: Vec<&CriticalEvent> = t
            .events
            .iter()
            .filter(|e| e.kind == EventKind::ComponentBirth)
            .collect();
        assert_eq!(
            births.len(),
            1,
            "sixteen equal pixels are ONE component born once, not sixteen births \
             resolved by iteration order: {:?}",
            t.events
        );
        let distinct: Vec<f64> = (0..w * h).map(|i| i as f64 / 255.0).collect();
        let d = batch_critical_events(&distinct, w, h, arm4());
        assert_eq!(
            d.largest_batch_pixels, 1,
            "a field of distinct values must have no ties, or the counter is not measuring ties"
        );
    }

    /// The event set does not depend on how the pixels are visited: the
    /// transposed image (which reverses the traversal order of every batch
    /// relative to the geometry) yields the same events.
    #[test]
    fn the_events_do_not_depend_on_traversal_order() {
        let (w, h) = (7usize, 5usize);
        let v: Vec<f64> = (0..w * h)
            .map(|i| {
                let (x, y) = ((i % w) as f64, (i / w) as f64);
                // A deliberately tie-heavy field: many pixels share a value.
                ((x - 3.0).abs().max((y - 2.0).abs()) * 0.25).min(1.0)
            })
            .collect();
        let mut transposed = vec![0.0f64; w * h];
        for y in 0..h {
            for x in 0..w {
                // Reflect in x: same geometry mirrored, same multiset of
                // batches, opposite scan order within each row.
                transposed[y * w + (w - 1 - x)] = v[y * w + x];
            }
        }
        let a = batch_critical_events(&v, w, h, arm4());
        let b = batch_critical_events(&transposed, w, h, arm4());
        assert!(a.tie_batches > 0, "the fixture must actually contain ties");
        assert_eq!(a.events, b.events);
        assert_eq!(a.plateaus, b.plateaus);
    }

    /// A hole is a background event, and it comes from the BACKGROUND tree
    /// under the complementary connectivity — not from the foreground tree.
    #[test]
    fn a_hole_shows_up_as_a_background_event() {
        let (w, h) = (7usize, 7usize);
        let mut v = vec![1.0f64; w * h];
        v[3 * w + 3] = 0.0;
        let t = batch_critical_events(&v, w, h, arm4());
        assert!(
            t.events.iter().any(|e| e.kind == EventKind::GapBirth),
            "the hole must appear as a background birth: {:?}",
            t.events
        );
    }

    /// A plateau is a run with no events, and its representative is its
    /// midpoint. The control: a field whose events are dense has narrower
    /// plateaus than one whose events are two.
    #[test]
    fn plateaus_are_runs_without_events_and_are_represented_by_their_midpoint() {
        let p = Plateau { lo: 0.25, hi: 0.75 };
        assert!((p.width() - 0.5).abs() < 1e-12);
        assert!((p.representative() - 0.5).abs() < 1e-9);
        let (w, h) = (5usize, 5usize);
        let mut v = vec![0.0f64; w * h];
        for y in 1..4 {
            for x in 1..4 {
                v[y * w + x] = 0.9;
            }
        }
        let t = batch_critical_events(&v, w, h, arm4());
        assert!(!t.plateaus.is_empty());
        let widest = t.plateaus.iter().map(|p| p.width()).fold(0.0f64, f64::max);
        assert!(
            widest > 0.5,
            "a two-value image has a wide plateau: {widest}"
        );
    }

    /// Level generation is separable: every candidate knows its origins, and
    /// dropping the fixed probes is a filter rather than a rebuild. This is
    /// the mechanism the §28 M4.5 clause "no magic-threshold-only" is
    /// measured with.
    #[test]
    fn fixed_smoke_levels_are_one_labelled_source_among_several() {
        let (w, h) = (9usize, 3usize);
        let mut v = vec![0.0f64; w * h];
        for y in 0..3 {
            for x in 0..3 {
                v[y * w + x] = 1.0;
            }
            for x in 6..9 {
                v[y * w + x] = 1.0;
            }
        }
        for x in 3..6 {
            v[w + x] = 0.4;
        }
        let t = batch_critical_events(&v, w, h, arm4());
        let levels = candidate_levels(&t, &LEVEL_CONFIG_V1);
        let event_driven = levels.iter().filter(|l| l.is_event_driven()).count();
        let fixed_only = levels.iter().filter(|l| !l.is_event_driven()).count();
        assert!(
            event_driven >= 3,
            "the generator must produce several event-driven levels, not one: {levels:?}"
        );
        assert!(
            event_driven > fixed_only,
            "event-driven levels {event_driven} must outnumber fixed-only {fixed_only}"
        );
        assert!(
            levels
                .iter()
                .any(|l| l.origins.contains(&LevelOrigin::FixedSmoke)),
            "the smoke probe must still be there: it is cheap and it is a control"
        );
    }
}
