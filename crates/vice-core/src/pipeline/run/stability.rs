use super::*;

pub(super) fn fitted_phase_envelope_stable<'a>(
    fitted_classes: impl IntoIterator<Item = &'a str>,
    selected_class: &str,
) -> bool {
    let mut saw_fitted = false;
    for class in fitted_classes {
        saw_fitted = true;
        if class != selected_class {
            return false;
        }
    }
    saw_fitted
}

pub(super) struct RenderStability {
    pub(super) stable: bool,
    pub(super) refusal: Option<String>,
}

pub(super) fn certify_render_stability(
    selected: &crate::candidate::MaterializedCandidate,
    config: &CoreConfig,
    parts: &mut ReportParts,
) -> RenderStability {
    let canonical_binding_check = (|| {
        let scene = vice_ir::parse_scene(&selected.scene_json)
            .map_err(|error| format!("parse selected scene: {error}"))?;
        let scene = vice_ir::ValidatedScene::new(scene)
            .map_err(|error| format!("validate selected scene: {error}"))?;
        let roundtrip_topology = vice_verify::topology_signature_sha256(scene.scene())
            .map_err(|error| format!("roundtrip topology: {error}"))?;
        if roundtrip_topology != selected.summary.post_quantization.topology_signature_sha256 {
            return Err(format!(
                "canonical scene roundtrip changed topology: report={} roundtrip={}",
                selected.summary.post_quantization.topology_signature_sha256, roundtrip_topology
            ));
        }
        if scene.scene().graph.boundaries.len() != selected.bindings.len() {
            return Err(format!(
                "canonical scene roundtrip changed boundary count: scene={} bindings={}",
                scene.scene().graph.boundaries.len(),
                selected.bindings.len()
            ));
        }
        let bindings = vice_verify::rebind_scene_bindings(
            scene.scene(),
            &selected.bindings,
            config.verification,
        )
        .map_err(|error| format!("canonical binding remap: {error}"))?;
        Ok((scene, bindings))
    })();
    if let Ok((_, bindings)) = &canonical_binding_check {
        parts.selected_boundary_bindings = bindings.clone();
    }
    let tighter_tolerance = config
        .verification
        .render_options
        .budget
        .chord_tolerance
        .px()
        / 2.0;
    let result = canonical_binding_check
        .as_ref()
        .map_err(Clone::clone)
        .and_then(|(scene, bindings)| {
            let budget =
                vice_render::TessellationBudget::with_chord_tolerance_px(tighter_tolerance)
                    .ok_or_else(|| "invalid tighter tessellation budget".to_string())?;
            let mut verification = config.verification;
            verification.render_options = verification.render_options.with_budget(budget);
            vice_verify::preseal_scene(scene.scene(), bindings, verification)
                .map(|_| ())
                .map_err(|error| error.to_string())
        });
    RenderStability {
        stable: result.is_ok(),
        refusal: result.err(),
    }
}

#[cfg(test)]
mod tests {
    use super::fitted_phase_envelope_stable;

    #[test]
    fn phase_stability_is_owned_only_by_materialized_fitted_classes() {
        let fitted = ["components=1;holes=0", "components=1;holes=0"];
        assert!(fitted_phase_envelope_stable(fitted, "components=1;holes=0"));
        assert!(!fitted_phase_envelope_stable(
            std::iter::empty(),
            "components=1;holes=0"
        ));
        assert!(!fitted_phase_envelope_stable(
            ["components=1;holes=0", "components=2;holes=0"],
            "components=1;holes=0"
        ));
        // Raw, refused or budget-pruned envelope classes are deliberately not
        // an input to this predicate; entropy and unexplored mass own them.
        let refused_raw_class = "components=3;holes=1";
        assert!(!fitted.contains(&refused_raw_class));
    }
}
