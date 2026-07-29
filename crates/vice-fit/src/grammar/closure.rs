//! The final transition of the cyclic grammar.
//!
//! A canonical cut is only a search device: the repeated endpoint is still a
//! grammar node. Its join feasibility and parameter sharing therefore have to
//! be applied before the global K truncation, exactly like every interior node.

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ClosureMode {
    Open,
    Corner,
    Smooth,
}

impl ClosureMode {
    pub(super) fn is_closed(self) -> bool {
        self != Self::Open
    }

    pub(super) fn is_smooth(self) -> bool {
        self == Self::Smooth
    }

    pub(super) fn state_for_seed(self, edge: GrammarEdge) -> Option<ClosureState> {
        self.is_smooth().then_some(ClosureState {
            first_entry_class: edge.entry_class,
            first_family_ord: family_ord(edge.family),
            first_tail_shared: false,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct ClosureState {
    first_entry_class: usize,
    first_family_ord: usize,
    first_tail_shared: bool,
}

impl ClosureState {
    pub(super) fn after_join(mut self, follows_first: bool, smooth: bool) -> Self {
        if follows_first {
            self.first_tail_shared = smooth;
        }
        self
    }
}

fn family_is_representable(family: SpanFamily, head: bool, tail: bool) -> bool {
    match family {
        SpanFamily::Quad => !tail,
        SpanFamily::CircularArc => !(head && tail),
        SpanFamily::Line | SpanFamily::Cubic => true,
    }
}

pub(super) fn path_is_representable(path: &GrammarPath, families: &[SpanFamily]) -> bool {
    let closure_smooth = path.closure_smooth;
    if path.candidates.len() != families.len()
        || path.smooth.len() != families.len().saturating_sub(1)
        || (path.closed && families.len() < 2)
        || (closure_smooth && !path.closed)
    {
        return false;
    }
    for (i, &family) in families.iter().enumerate() {
        let head = if i == 0 {
            closure_smooth
        } else {
            path.smooth[i - 1]
        };
        let tail = if i + 1 == families.len() {
            closure_smooth
        } else {
            path.smooth[i]
        };
        if !family_is_representable(family, head, tail) {
            return false;
        }
        if tail {
            let Some(&outgoing) = families.get((i + 1) % families.len()) else {
                return false;
            };
            if !smooth_transition_is_representable(family, head, outgoing) {
                return false;
            }
        }
    }
    true
}

pub(super) fn close_finished_path(
    path: &mut Partial,
    edges: &[GrammarEdge],
    coordinate_bits: f64,
    join_bits: f64,
    smooth: bool,
) -> bool {
    if path.prev.is_none() {
        return false;
    }
    if !smooth {
        path.topology += join_bits;
        path.bits += join_bits;
        return true;
    }
    let Some(first) = path.closure else {
        return false;
    };
    let last = edges[path.edge];
    let first_family = FAMILY_BY_ORD[first.first_family_ord];
    if !jet_compatible(last.exit_class, first.first_entry_class)
        || !smooth_transition_is_representable(last.family, path.smooth_here, first_family)
        || !family_is_representable(first_family, true, first.first_tail_shared)
    {
        return false;
    }

    let first_head_saving = free_scalars(first_family, false, first.first_tail_shared)
        - free_scalars(first_family, true, first.first_tail_shared);
    let last_tail_saving = free_scalars(last.family, path.smooth_here, false)
        - free_scalars(last.family, path.smooth_here, true);
    let angle_free = tangent_is_free(last.family) && tangent_is_free(first_family);
    let scalar_saving = (first_head_saving + last_tail_saving) as f64 * coordinate_bits;
    let seam_bits = join_bits + f64::from(u8::from(angle_free)) * coordinate_bits;
    path.geometry -= scalar_saving;
    path.topology += seam_bits;
    path.bits += seam_bits - scalar_saving;
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use vice_geom::Pt;

    fn samples() -> Vec<BoundarySample> {
        (0..3)
            .map(|i| BoundarySample {
                p: Pt::new(i as f64, 0.0),
                normal: Pt::new(0.0, 1.0),
                halfwidth: 0.35,
                confidence: 1.0,
                weight_ds: 1.0,
                corr_length_px: 1.0,
            })
            .collect()
    }

    fn edge(candidate: usize, from: usize, to: usize, family: SpanFamily) -> GrammarEdge {
        GrammarEdge {
            candidate,
            from,
            to,
            family,
            entry_class: 0,
            exit_class: 0,
            entry_rad: 0.0,
            exit_rad: 0.0,
            residual_bits: 0.0,
            proposal_cost_px: 0.0,
        }
    }

    #[test]
    fn more_than_k_cheaper_seam_invalid_paths_do_not_hide_a_valid_path() {
        let mut edges = (0..8)
            .map(|i| edge(i, 0, 1, SpanFamily::Line))
            .collect::<Vec<_>>();
        edges.push(edge(8, 1, 2, SpanFamily::Quad));
        edges.push(edge(9, 1, 2, SpanFamily::Cubic));
        edges[9].residual_bits = 100.0;
        let paths = k_best_paths_for_objective(
            &edges,
            &samples(),
            &crate::GEOMETRY_CODE_TABLE_V1,
            256.0,
            8,
            (PathObjective::PhysicalCode, ClosureMode::Smooth),
            crate::code::first_sample_residual_bits(
                &samples(),
                &crate::GEOMETRY_CODE_TABLE_V1,
                256.0,
            )
            .expect("valid samples"),
        );
        assert_eq!(paths.len(), 8);
        assert!(
            paths.iter().all(|path| path.candidates[1] == 9),
            "all cheaper quad-ending paths are invalid at the smooth seam"
        );
    }

    #[test]
    fn smooth_seam_rederives_both_endpoint_scalar_counts() {
        let edges = [
            edge(0, 0, 1, SpanFamily::Cubic),
            edge(1, 1, 2, SpanFamily::Cubic),
        ];
        let table = &crate::GEOMETRY_CODE_TABLE_V1;
        let open = k_best_paths(&edges, &samples(), table, 256.0, 1)
            .expect("valid samples")
            .pop()
            .expect("open path");
        let closed = k_best_paths_for_objective(
            &edges,
            &samples(),
            table,
            256.0,
            1,
            (PathObjective::PhysicalCode, ClosureMode::Smooth),
            crate::code::first_sample_residual_bits(&samples(), table, 256.0)
                .expect("valid samples"),
        )
        .pop()
        .expect("closed path");
        let cb = table.coordinate_bits(256.0);
        let join = (crate::code::JOIN_KINDS as f64).log2();
        assert_eq!(
            closed.code.geometry_bits,
            open.code.geometry_bits - 2.0 * cb
        );
        assert_eq!(
            closed.code.topology_bits,
            open.code.topology_bits + join + cb
        );
        assert_eq!(closed.code.residual_bits, open.code.residual_bits);
    }

    #[test]
    fn a_single_span_corner_loop_cannot_consume_the_global_k_slot() {
        let mut edges = vec![edge(0, 0, 2, SpanFamily::Cubic)];
        edges.push(edge(1, 0, 1, SpanFamily::Cubic));
        edges.push(edge(2, 1, 2, SpanFamily::Cubic));
        edges[1].residual_bits = 10.0;
        edges[2].residual_bits = 10.0;
        let path = k_best_paths_for_objective(
            &edges,
            &samples(),
            &crate::GEOMETRY_CODE_TABLE_V1,
            256.0,
            1,
            (PathObjective::PhysicalCode, ClosureMode::Corner),
            crate::code::first_sample_residual_bits(
                &samples(),
                &crate::GEOMETRY_CODE_TABLE_V1,
                256.0,
            )
            .expect("valid samples"),
        )
        .pop()
        .expect("the valid two-span loop survives K=1");
        assert_eq!(path.candidates, vec![1, 2]);
        assert!(path.closed);
        assert!(!path.closure_smooth);
    }
}
