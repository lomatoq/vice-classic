//! Rules about the SOURCE SURFACE of this module, and their prices.
//!
//! Three judges live here rather than beside the mechanisms they guard,
//! because they share a subject — the text of the crate — and a limit: each
//! reads source, so each is defeated by moving or renaming what it reads. That
//! is stated once, here, instead of three times in three places:
//!
//! - **no serde attribute on a field of `Parts`**, because the leaf count that
//!   guards the mutation walk's completeness is keyed on the serialization
//!   (RT5-A12, M5B-N14);
//! - **every field of `Dcel` is in `Parts` or declared**, because the walk and
//!   the leaf count both measure `d.parts()`, so a field one level above is
//!   outside the ruler entirely (REVIEW_M5_A D6-N2);
//! - **every judge branch has a distinct literal label**, because a branch
//!   reusing a name hides from the probe (N17, RT5-A18, RT5-A22, M5B-E18b).
//!
//! **The shared residual, at its cheapest known price:** all three read text.
//! Renaming a struct, moving it out of `src/dcel`, or producing a label through
//! a macro defeats them — **one line** in each case. The closure for all three
//! is the same and is owned by M6: a derive that emits the subject set and its
//! proof together, so that the set comes from the definition rather than from a
//! reader of the definition.

#[cfg(test)]
mod tests {

    /// **D6-N2: every field of `Dcel` is in `Parts`, or is a declared
    /// exception.**
    ///
    /// The clause-4 row named the cheapest bypass as a field whose TYPE
    /// serialises to nothing. Reviewer A found a cheaper one and ran it: an
    /// ordinary `u32` added to `Dcel` itself — one level ABOVE `Parts` — behind
    /// a public accessor, wrong above 16 px. Every judge stayed green, four
    /// clauses MET, and the artifact was byte-identical, because the leaf count
    /// is taken over `d.parts()`: a field on `Dcel` is outside the ruler
    /// entirely, and the proc-macro that would close the TYPE axis would not
    /// close this one.
    ///
    /// F-0048's last paragraph requires the boundary to be named at the
    /// CHEAPEST known bypass price, and the row named a higher one. That is the
    /// second overstated residual of this milestone.
    ///
    /// Closed in the shape the project already uses: the subject set is taken
    /// from the side the attacker does not silently edit. Every field of `Dcel`
    /// must be `parts`, or appear below with a reason — so a new field is a
    /// failing test until somebody decides, rather than a silent hole.
    ///
    /// **Residual at its own cheapest price:** this reads the struct's source,
    /// so renaming the struct or moving it defeats it — one line, the same
    /// class as the serde scan and the branch-label scan, and the same closure.
    #[test]
    fn every_field_of_dcel_is_in_parts_or_declared() {
        /// (field, why it is not in `Parts`).
        const OUTSIDE_PARTS: &[(&str, &str)] = &[
            (
                "labelling",
                "the INPUT. It is what every anchor compares against, so it cannot be part of the \
                 derived structure the walk perturbs",
            ),
            (
                "conn",
                "the other input. Two values, both swept; a non-complementary pair is \
                 unrepresentable",
            ),
            (
                "foreground_connectivity",
                "a printable projection of `conn`, carried for the artifact; it derives from an \
                 input rather than from the arrangement",
            ),
            (
                "parts",
                "the derived structure itself — this is the field the rule exists to funnel                  everything else into, and the one the mutation walk and the leaf count both                  measure",
            ),
        ];

        let src = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/dcel/mod.rs"),
        )
        .expect("mod.rs");
        let start = src
            .find("pub struct Dcel {")
            .expect("the struct this rule is about must be findable");
        let end = start + src[start..].find("\n}\n").expect("closed");
        let body = &src[start..end];
        assert!(
            body.lines().count() > 3,
            "the scan read a {}-line body; it is not reading the struct",
            body.lines().count()
        );

        let mut found: Vec<&str> = Vec::new();
        for line in body.lines() {
            let t = line.trim();
            if t.starts_with("//") || t.starts_with("#[") || t.ends_with('{') {
                continue;
            }
            if let Some((name, _)) = t.split_once(':') {
                let n = name.trim();
                if !n.is_empty() && n.chars().all(|c| c.is_alphanumeric() || c == '_') {
                    found.push(n);
                }
            }
        }
        assert!(
            found.len() >= 4,
            "found {} fields on `Dcel`: {found:?}",
            found.len()
        );
        for f in &found {
            assert!(
                OUTSIDE_PARTS.iter().any(|(n, _)| n == f),
                "`Dcel` has a field `{f}` that is neither `parts` nor a declared exception. The \
                 mutation walk perturbs `Parts`, and the leaf count that guards the walk's \
                 completeness is taken over `d.parts()`, so a field HERE is outside both — an \
                 ordinary `u32` behind a public accessor, wrong above 16 px, kept every judge \
                 green and the artifact byte-identical (REVIEW_M5_A D6-N2). Put it in `Parts`, or \
                 declare it here with the reason it does not belong there"
            );
        }
        // And the exception list may not rot.
        for (n, why) in OUTSIDE_PARTS {
            assert!(
                found.contains(n),
                "`{n}` is declared as outside `Parts` and no longer exists on `Dcel`"
            );
            assert!(why.len() > 30, "the exception for `{n}` does not say why");
        }
    }

    /// **RT5-A18 / N17: labels unique by construction, without source position.**
    ///
    /// N17 wanted a new branch reusing an existing label to be impossible.
    /// Delta-4 bought that with `line!()`, which put source line numbers into a
    /// signed Tier A artifact and made its bytes a function of one file's
    /// layout. Uniqueness does not need a position: the labels are literals and
    /// this requires them to be pairwise distinct.
    ///
    /// Both directions: the scan must FIND the labels (an empty scan is
    /// vacuously distinct, F-0039), and duplicates must fail.
    ///
    /// **Residual, named where the strength is claimed, with its price.** Two
    /// holes were found separately and both are closed here: the scan read ONE
    /// hardcoded file, so a branch elsewhere was invisible for ZERO lines
    /// (RT5-A22), and it matched literals, so `const R: &str = "empty";` hid a
    /// duplicate for TWO (REVIEW_M5_B E18b). It now walks the whole module tree
    /// and REFUSES a non-literal rather than skipping it.
    ///
    /// What remains: a label produced by a macro that expands to a literal, or
    /// a branch in a file outside `src/dcel`. Cheapest known bypass: **one
    /// line**, a `branch:` written through a macro. Same class as the
    /// serde-attribute scan and the same closure — a derive that emits the
    /// labels and their distinctness proof together.
    #[test]
    fn every_judge_branch_has_a_distinct_label() {
        // EVERY file of the dcel module tree, not one hardcoded path.
        // REDTEAM_M5 RT5-A22: the scan read `audit.rs` alone, so a branch in
        // `loops.rs`, `crossing.rs` or a new file was invisible to it at a cost
        // of ZERO lines. The walk is over the directory now, and it asserts it
        // found more than one file so an empty walk cannot pass.
        // RECURSIVE. REDTEAM_M5 RT5-A23 = REVIEW_M5_B E22, found independently:
        // "walks the module tree" was `read_dir` of ONE level, and a
        // subdirectory has no `.rs` extension so it was skipped whole. §4.1's
        // 800-line rule makes subdirectories likely — `audit.rs` has already
        // been split once — so this was a live door, not a hypothetical one.
        //
        // The red team offered recording the depth as a limitation instead, and
        // warned that doing it silently would be F-0048 for the ninth time. One
        // line of recursion is cheaper than a limitation, so the depth is
        // closed rather than declared; F-0078 records the class.
        fn walk(dir: &std::path::Path, src: &mut String, files: &mut usize) {
            let Ok(rd) = std::fs::read_dir(dir) else {
                return;
            };
            let mut entries: Vec<_> = rd.flatten().map(|e| e.path()).collect();
            entries.sort();
            for p in entries {
                if p.is_dir() {
                    walk(&p, src, files);
                } else if p.extension().is_some_and(|x| x == "rs") {
                    src.push_str(&std::fs::read_to_string(&p).expect("read"));
                    *files += 1;
                }
            }
        }
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/dcel");
        let mut files = 0usize;
        let mut src = String::new();
        walk(&dir, &mut src, &mut files);
        assert!(
            files > 4,
            "the scan found {files} files; it is not covering the module"
        );
        let mut labels: Vec<String> = Vec::new();
        for line in src.lines() {
            let t = line.trim();
            if let Some(rest) = t.strip_prefix("branch: ") {
                let v = rest.trim_end_matches(',').trim();
                // A LITERAL is required. `branch: SOME_CONST` hides a duplicate
                // for two lines (REVIEW_M5_B E18b), so an indirection is a
                // failure here rather than something the scan quietly skips.
                assert!(
                    v.starts_with('"') && v.ends_with('"'),
                    "branch label {v:?} is not a string literal. An indirection -                      `const X: &str = \"empty\";` - hides a duplicate from this scan for two                      lines, so it is refused rather than skipped"
                );
                labels.push(v.trim_matches('"').to_string());
            }
        }
        assert!(
            labels.len() >= 2,
            "found {} branch labels; the scan is not reading the judge",
            labels.len()
        );
        let mut sorted: Vec<&str> = labels.iter().map(|s| s.as_str()).collect();
        sorted.sort_unstable();
        let before = sorted.len();
        sorted.dedup();
        assert_eq!(
            sorted.len(),
            before,
            "two branches of the judge share a label: {labels:?}. A branch reusing an existing \
             name hides from the probe for zero lines, which is what N17 was about"
        );
        assert!(
            !labels.iter().any(|l| l.contains('@')),
            "a branch label carries a source position: {labels:?}. That lands in the signed \
             artifact and makes its bytes a function of source layout (RT5-A18)"
        );
    }
}
