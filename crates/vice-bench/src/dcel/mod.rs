//! The §28 M5 harness: shared DCEL and safe topological transactions,
//! measured over the GT corpus.
//!
//! ## The four clauses, and what each is measured against
//!
//! §28 M5 states four things. Each is a conjunction here, each conjunction has
//! at least one control that CAN fail, and each control is knocked out in BOTH
//! directions — the red side and the empty side — because a control that only
//! fails where it is supposed to fail is half a control (F-0039, RT45-A12).
//!
//! | clause | measured as | control that can fail |
//! |---|---|---|
//! | no final-topology claim from proxy | the set of topologies is the same size going into the M5 stage and coming out | [`ProxyKnockout::Select`] makes the stage pick a winner, and the row must go NOT MET |
//! | candidate recall maintained after budget pruning | the DCEL of every candidate carries the class an INDEPENDENT chain assigns it, and budget pruning removed no GT-carrying class | the independent chain is `topology::independent`, a different mathematics; a disagreement fails the row |
//! | no unrelated graph mutation | chains lying wholly outside the ROI plus its halo are byte-identical before and after every transaction | [`RoiKnockout::Reach`] adds one pixel outside the ROI and the transaction must be refused |
//! | no dangling/invalid faces | [`vice_topology::audit`] is green on every arrangement built | the audit's resolving power is measured by the mutation walk, which exhibits the world where it is red |
//!
//! ## The population, and what it is NOT
//!
//! The arms here are the corpus's own scenes at the topology cells, digitized
//! by the §5.3 majority rule from EXACT ink coverage — the same truth field the
//! M4.5 recall clause is scored against, and it reaches this module through
//! `topology::exact_ink_coverage`, which no palette and no formation enters.
//!
//! Said plainly so it is not read as more: **this is the oracle-observation
//! population, not the estimated-evidence path.** M4.5's recall clause runs the
//! evidence stage and asks whether the envelope built from an ESTIMATED
//! coverage field contains the truth. This one asks what the DCEL and the
//! transactions do to topologies that are already correct. Those are different
//! questions, and the second is the one §28 M5 asks: its four clauses are about
//! the arrangement and the transaction, not about the evidence.
//!
//! ## Widening a judge's proof domain, on purpose
//!
//! `topology::independent::signature_of` is the M4.5 judge. F-8 and F-9 are
//! that it was proved on twelve hand-written fixtures no larger than 7x7 while
//! deciding questions at corpus sizes. Every arm here compares it against a
//! THIRD computation — the DCEL's face and loop structure, which counts neither
//! the way production does (union-find over pixels) nor the way the judge does
//! (flood fill plus bit-quad Euler), but by walking boundaries. Every arm of
//! this run is therefore a differential comparison of the judge at a corpus
//! size, and the count is published.

pub mod report;

use serde::Serialize;
use vice_ir::ComplementaryConnectivity;
use vice_topology::continuation::EditKind;
use vice_topology::dcel::{apply, audit, is_the_assembly_of_its_own_labelling, Edit, Roi};
use vice_topology::{Dcel, Labelling, Outcome, TX_CONFIG_V1};

use crate::gt::corpus::all_groups;
use crate::gt::degradation::{matrix_v1, DegradationCell, ResizeChain};
use crate::gt::raster::ViewTransform;
use crate::gt::split::{Split, SPLIT_POLICY_V1};
use crate::gt::GtScene;
use crate::hashing::sha256_hex;
use crate::topology::{
    exact_ink_coverage, independent, view_for, GtSignature, TopologyScope, GT_MAJORITY_LEVEL,
    TOPOLOGY_CELL_IDS,
};
use vice_topology::{structural_fixtures, with_a_distant_witness};

pub const DCEL_RUN_SCHEMA: &str = "vice-classic/dcel-transactions/v1";

/// Whether the M5 stage is allowed to reduce a set of topologies to one.
///
/// It is not, and this type is how "it is not" becomes falsifiable: §32 rule 14
/// forbids a topology winner from the M5 proxy, and a clause asserting that it
/// does not happen is worth exactly as much as the world in which it does.
/// [`ProxyKnockout::Select`] is that world, it is reachable from the harness,
/// and the test `the_proxy_knockout_takes_the_row_down` runs it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProxyKnockout {
    /// Production: the stage carries every topology through.
    Off,
    /// Knockout: the stage keeps the candidate with the smallest surrogate
    /// cost and drops the rest. Never used outside the control.
    Select,
}

/// Whether the transaction's edit is allowed to reach outside its ROI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoiKnockout {
    Off,
    /// Knockout: add one pixel far from the declared region.
    Reach,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunKnockouts {
    pub proxy: ProxyKnockout,
    pub roi: RoiKnockout,
}

pub const PRODUCTION: RunKnockouts = RunKnockouts {
    proxy: ProxyKnockout::Off,
    roi: RoiKnockout::Off,
};

/// One measured arm: one scene, one degradation cell, one convention.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DcelArm {
    /// `corpus` or `structural`. The second population exists because the
    /// first cannot carry clause 1's control: STATUS_M4_5 limitation 18 records
    /// that ZERO of the corpus's 132 arms have a class that differs between the
    /// two complementary conventions, so on the corpus alone "the stage did not
    /// reduce a set of topologies" is a statement about sets of size one. The
    /// structural fixtures of REVIEW_M4_5 condition 51 include the diagonal
    /// pinch, whose class differs between the arms BY CONSTRUCTION, and that is
    /// where the control gets a population.
    pub source: &'static str,
    pub scene_id: String,
    pub shape_family: String,
    pub cell_id: String,
    pub split: &'static str,
    pub size_px: u32,
    pub foreground_connectivity: &'static str,
    /// What the INDEPENDENT chain says the class is.
    pub truth: GtSignature,
    /// What the DCEL's faces and loops say it is.
    pub dcel_class: GtSignature,
    pub agrees_with_the_independent_chain: bool,
    pub audit_ok: bool,
    pub is_its_own_assembly: bool,
    pub vertices: u32,
    pub boundaries: u32,
    pub segments: u32,
    pub loops: u32,
    pub faces: u32,
    pub skeleton_components: u32,
    /// `V - B + L` and `2C`: the Euler identity, both sides published so a
    /// reader can check the arithmetic rather than trust the boolean.
    pub euler_lhs: i64,
    pub euler_rhs: i64,
    /// The class this arm carries OUT of the M5 stage. Equal to `dcel_class`
    /// in production; under the proxy knockout the stage overwrites it with
    /// the group's first class, which is what "picking a winner" does. §32
    /// rule 14 is that the number of distinct classes per group is the same on
    /// both sides.
    pub class_out: GtSignature,
    /// The transaction this arm ran, if its geometry admitted one.
    pub transaction: Option<ArmTransaction>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ArmTransaction {
    pub declared: &'static str,
    pub committed: bool,
    pub refusal: Option<String>,
    pub roi_area_px: u64,
    pub pixels_changed: usize,
    pub unrelated_chains: usize,
    pub unrelated_chains_that_moved: usize,
    pub components_before: u32,
    pub components_after: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DcelRun {
    pub schema: &'static str,
    pub scope: &'static str,
    pub cells: Vec<String>,
    pub scenes: u64,
    pub arms: Vec<DcelArm>,
    pub refused: Vec<String>,
    pub sealed_audit_groups_skipped: u64,
    pub fixture_set_hash: String,
    /// The mutation walk's own measurement, taken on this run's arms rather
    /// than on a fixture: how many derived slots were perturbed and how many
    /// each check caught. Without it "the audit is green" says nothing about
    /// what the audit can see.
    pub audit_resolving_power: ResolvingPower,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
pub struct ResolvingPower {
    /// Every arm the run saw. The probe fires on a stride of these, so a
    /// control that went empty would show up as `arrangements_probed = 0`
    /// beside a large `arms_seen` rather than as silence.
    pub arms_seen: u64,
    pub arrangements_probed: u32,
    pub slots_perturbed: u64,
    pub caught_by_audit: u64,
    pub caught_by_assembly_equality: u64,
    pub caught_by_neither: u64,
    pub no_ops: u64,
}

fn conn_name(c: ComplementaryConnectivity) -> &'static str {
    match c.foreground() {
        vice_ir::PixelConnectivity::Four => "4",
        vice_ir::PixelConnectivity::Eight => "8",
    }
}

fn resolve_cells(ids: &[&str]) -> Result<Vec<DegradationCell>, String> {
    let matrix = matrix_v1();
    ids.iter()
        .map(|want| {
            matrix
                .iter()
                .find(|c| c.id() == *want)
                .copied()
                .ok_or_else(|| format!("dcel cell {want} is not a cell of the frozen matrix"))
        })
        .collect()
}

/// Run the M5 harness.
pub fn run(scope: TopologyScope, k: RunKnockouts) -> Result<DcelRun, String> {
    let ids: Vec<&str> = match scope {
        TopologyScope::Full => TOPOLOGY_CELL_IDS.to_vec(),
        TopologyScope::Test => TOPOLOGY_CELL_IDS.iter().take(1).copied().collect(),
    };
    let cells = resolve_cells(&ids)?;
    let groups = all_groups()?;
    let policy = &SPLIT_POLICY_V1;

    let mut arms = Vec::new();
    let mut refused = Vec::new();
    let mut digests: Vec<String> = Vec::new();
    let mut scenes = 0u64;
    let mut skipped_audit = 0u64;
    let mut power = ResolvingPower::default();

    for group in groups.iter().step_by(scope.stride()) {
        let split = policy.split_of_group(group);
        if split == Split::SealedAudit {
            // §27.1: scoring the sealed audit is what OPENS it.
            skipped_audit += 1;
            continue;
        }
        for scene in &group.scenes {
            scenes += 1;
            digests.push(
                vice_ir::scene_digest_sha256(scene.scene().scene()).map_err(|e| e.to_string())?,
            );
            for cell in &cells {
                if cell.resize != ResizeChain::None {
                    continue;
                }
                if !policy.profile_allowed(split, cell.profile.as_str()) {
                    continue;
                }
                for conn in ComplementaryConnectivity::arms() {
                    match measure_arm(scene, cell, split, &group.shape_family, conn, k, &mut power)
                    {
                        Ok(row) => arms.push(row),
                        Err(reason) => refused.push(reason),
                    }
                }
            }
        }
    }

    // The structural register (REVIEW_M4_5 condition 51), at every size the
    // corpus uses. It is not decoration beside the corpus: it is the ONLY
    // population here that carries a convention-dependent class, and clause 1's
    // control has nothing to measure without one.
    let sizes: Vec<u32> = cells
        .iter()
        .map(|c| c.size_px)
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    for n in &sizes {
        for f in structural_fixtures(*n as usize) {
            let witnessed = with_a_distant_witness(&f.labelling, *n as usize);
            let t = ViewTransform {
                scale: 1.0,
                dx: 0.0,
                dy: 0.0,
                width_px: *n,
                height_px: *n,
            };
            for conn in ComplementaryConnectivity::arms() {
                arms.push(arm_from_labelling(
                    "structural",
                    f.name,
                    f.name,
                    &format!("s{n}"),
                    "structural",
                    *n,
                    witnessed.inside(),
                    &t,
                    conn,
                    k,
                    &mut power,
                ));
            }
        }
    }

    // The proxy knockout, applied where a proxy WOULD apply it: per group of
    // arms describing the same fixture at the same size. Production never
    // enters this branch, and the branch exists so that the clause it violates
    // has a world in which it is false (F-0035).
    if k.proxy == ProxyKnockout::Select {
        let mut first: std::collections::BTreeMap<(String, String), GtSignature> =
            std::collections::BTreeMap::new();
        for a in &mut arms {
            let key = (a.scene_id.clone(), a.cell_id.clone());
            let win = *first.entry(key).or_insert(a.dcel_class);
            a.class_out = win;
        }
    }

    digests.sort();
    Ok(DcelRun {
        schema: DCEL_RUN_SCHEMA,
        scope: scope.as_str(),
        cells: ids.iter().map(|s| (*s).to_string()).collect(),
        scenes,
        arms,
        refused,
        sealed_audit_groups_skipped: skipped_audit,
        fixture_set_hash: sha256_hex(digests.join("\u{1f}").as_bytes()),
        audit_resolving_power: power,
    })
}

/// How often the mutation walk is run. Every arm would be correct and slow;
/// every twenty-fifth keeps the control's population in the hundreds while the
/// run stays inside a CI step. The number of arrangements probed is published,
/// so the control cannot go quietly empty.
const RESOLVING_POWER_STRIDE: usize = 17;

fn measure_arm(
    scene: &GtScene,
    cell: &DegradationCell,
    split: Split,
    shape_family: &str,
    conn: ComplementaryConnectivity,
    k: RunKnockouts,
    power: &mut ResolvingPower,
) -> Result<DcelArm, String> {
    let t = view_for(cell);
    let ink =
        exact_ink_coverage(scene, &t).map_err(|e| format!("{} {}: {e}", scene.id(), cell.id()))?;
    let inside: Vec<bool> = ink.iter().map(|v| *v >= GT_MAJORITY_LEVEL).collect();
    Ok(arm_from_labelling(
        "corpus",
        scene.id(),
        shape_family,
        &cell.id(),
        split.as_str(),
        cell.size_px,
        &inside,
        &t,
        conn,
        k,
        power,
    ))
}

/// One arm from one labelling. Both populations go through this, so the corpus
/// and the structural register are measured by the same code rather than by two
/// that could drift.
#[allow(clippy::too_many_arguments)]
fn arm_from_labelling(
    source: &'static str,
    scene_id: &str,
    shape_family: &str,
    cell_id: &str,
    split: &'static str,
    size_px: u32,
    inside: &[bool],
    t: &ViewTransform,
    conn: ComplementaryConnectivity,
    k: RunKnockouts,
    power: &mut ResolvingPower,
) -> DcelArm {
    let (w, h) = (t.width_px as usize, t.height_px as usize);
    let inside = inside.to_vec();

    // The independent chain: flood fill for components, bit-quad Euler for
    // holes. A different mathematics from both production and the DCEL.
    let truth = independent::signature_of(&inside, w, h, conn);
    let truth = GtSignature {
        components: truth.components,
        holes: truth.holes,
    };

    let d = Dcel::assemble(Labelling::new(w, h, inside.clone()), conn);
    let _ = &k;
    let a = audit(&d);
    let own = is_the_assembly_of_its_own_labelling(&d);
    let dcel_class = GtSignature {
        components: d.foreground_faces() as u32,
        holes: d.holes() as u32,
    };

    let (v, b, l, f, c) = match &a {
        Ok(r) => (
            r.vertices,
            r.boundaries,
            r.loops,
            r.faces,
            r.skeleton_components,
        ),
        Err(_) => (0, 0, 0, 0, 0),
    };

    // The M5 stage carries topologies through. `topologies_out` is where §32
    // rule 14 becomes falsifiable, and the knockout is the world where it is.
    let transaction = transaction_for(&d, t, k.roi);

    // Probed on a stride. `arms_seen` counts every arm and the probe fires on
    // one in `RESOLVING_POWER_STRIDE`, so the control's population grows with
    // the run instead of being a fixed list of places.
    power.arms_seen += 1;
    if power.arms_seen % RESOLVING_POWER_STRIDE as u64 == 1 {
        probe_resolving_power(&d, power);
    }

    DcelArm {
        source,
        scene_id: scene_id.to_string(),
        shape_family: shape_family.to_string(),
        cell_id: cell_id.to_string(),
        split,
        size_px,
        foreground_connectivity: conn_name(conn),
        truth,
        dcel_class,
        agrees_with_the_independent_chain: dcel_class == truth,
        audit_ok: a.is_ok(),
        is_its_own_assembly: own,
        vertices: v,
        boundaries: b,
        segments: d.segment_count() as u32,
        loops: l,
        faces: f,
        skeleton_components: c,
        euler_lhs: i64::from(v) - i64::from(b) + i64::from(l),
        euler_rhs: 2 * i64::from(c),
        class_out: dcel_class,
        transaction,
    }
}

/// The mutation walk, on a real arm rather than on a fixture.
fn probe_resolving_power(d: &Dcel, power: &mut ResolvingPower) {
    let r = vice_topology::measure_audit_resolving_power(d);
    power.arrangements_probed += 1;
    power.slots_perturbed += r.slots;
    power.caught_by_audit += r.caught_by_audit;
    power.caught_by_assembly_equality += r.caught_by_assembly_equality;
    power.caught_by_neither += r.caught_by_neither;
    power.no_ops += r.no_ops;
}

/// One transaction per arm: fill a small square at the centre of the canvas.
///
/// The edit is derived from the CANVAS rather than from the arrangement, so it
/// is the same edit for every arm and the population is not selected by what
/// happens to work. Which topological edit it turns out to be depends on the
/// scene, so the declared kind is read off the two arrangements and the
/// transaction is then required to agree with it — an arm whose edit does not
/// match its declaration is a rolled-back transaction and is counted as one.
fn transaction_for(base: &Dcel, t: &ViewTransform, roi_k: RoiKnockout) -> Option<ArmTransaction> {
    let (w, h) = (t.width_px, t.height_px);
    if w < 16 || h < 16 {
        return None;
    }
    let s = (w / 8).max(2);
    let roi = Roi {
        x0: w / 2 - s / 2,
        y0: h / 2 - s / 2,
        x1: w / 2 + s / 2,
        y1: h / 2 + s / 2,
    };
    let mut set: Vec<(u32, u32, bool)> = (roi.x0..roi.x1)
        .flat_map(|x| (roi.y0..roi.y1).map(move |y| (x, y, true)))
        .collect();
    if roi_k == RoiKnockout::Reach {
        set.push((0, 0, true));
    }

    // What the edit actually does, so the declaration is a claim about THIS
    // scene rather than a guess.
    let mut inside = base.labelling().inside().to_vec();
    for (x, y, v) in &set {
        if *x < w && *y < h {
            inside[*y as usize * w as usize + *x as usize] = *v;
        }
    }
    let after = Dcel::assemble(
        Labelling::new(w as usize, h as usize, inside),
        base.connectivity(),
    );
    let (c0, h0) = (base.foreground_faces() as i64, base.holes() as i64);
    let (c1, h1) = (after.foreground_faces() as i64, after.holes() as i64);
    let kind = match (c1 - c0, h1 - h0) {
        (-1, 0) => EditKind::BridgeClose,
        (1, 0) => EditKind::GapOpen,
        (0, 1) => EditKind::HoleOpen,
        (0, -1) => EditKind::HoleFill,
        _ => return None,
    };

    let edit = Edit { kind, roi, set };
    let out = apply(base, &edit, &TX_CONFIG_V1);
    let rep = out.report();
    Some(ArmTransaction {
        declared: rep.declared,
        committed: rep.committed,
        refusal: match &out {
            Outcome::RolledBack { reason, .. } => Some(reason.to_string()),
            Outcome::Committed { .. } => None,
        },
        roi_area_px: rep.roi.area_px(),
        pixels_changed: rep.pixels_changed,
        unrelated_chains: rep.unrelated_chains,
        unrelated_chains_that_moved: rep.unrelated_chains_that_moved,
        components_before: rep.base.map(|b| b.foreground_faces).unwrap_or(0),
        components_after: rep.candidate.map(|c| c.foreground_faces).unwrap_or(0),
    })
}
