use std::path::{Path, PathBuf};

fn collect_rust_sources(root: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(root)
        .unwrap_or_else(|error| panic!("read {}: {error}", root.display()))
        .map(|entry| entry.expect("source entry").path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect_rust_sources(&path, out);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            out.push(path);
        }
    }
}

fn main() {
    let manifest = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let workspace = manifest
        .parent()
        .and_then(Path::parent)
        .expect("workspace root");
    let mut sources = Vec::new();
    collect_rust_sources(&workspace.join("crates/vice-fit/src"), &mut sources);
    collect_rust_sources(&manifest.join("src/geometry"), &mut sources);
    sources.sort();

    let mut generated = String::from("const BACKEND_SOURCE_PATHS: &[(&str, &str)] = &[\n");
    for source in sources {
        println!("cargo:rerun-if-changed={}", source.display());
        let relative = source
            .strip_prefix(workspace)
            .expect("backend source is in the workspace")
            .to_string_lossy()
            .replace('\\', "/");
        let absolute = source
            .canonicalize()
            .unwrap_or_else(|error| panic!("canonicalize {}: {error}", source.display()));
        generated.push_str(&format!(
            "    ({relative:?}, include_str!({:?})),\n",
            absolute.to_string_lossy()
        ));
    }
    generated.push_str(
        "    (\n        \"crates/vice-bench/src/geometry/source_manifest_v1\",\n        \
         \"all Rust sources under vice-fit/src and vice-bench/src/geometry\",\n    ),\n];\n",
    );

    let out = PathBuf::from(std::env::var_os("OUT_DIR").expect("out dir"))
        .join("geometry_backend_sources.rs");
    std::fs::write(&out, generated)
        .unwrap_or_else(|error| panic!("write {}: {error}", out.display()));
}
