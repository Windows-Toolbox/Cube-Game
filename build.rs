//! The manifest for the Toolbox (`kit::build`), and the list of the game's data files in `content\`, compiled into
//! the exe (`src\content.rs`).

use std::path::Path;

fn main() {
    kit::build::manifest();

    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("content");
    let mut files = vec![];
    collect(&root, &root, &mut files);
    files.sort();
    let mut out = String::from("/// Every data file: its name below `content\\` (with `/`) and its text.\npub static FILES: &[(&str, &str)] = &[\n");
    for name in &files {
        let path = root.join(name).display().to_string().replace('\\', "/");
        out.push_str(&format!("    ({name:?}, include_str!({path:?})),\n"));
    }
    out.push_str("];\n");
    let out_dir = std::env::var("OUT_DIR").unwrap();
    std::fs::write(Path::new(&out_dir).join("content_files.rs"), out).unwrap();
    println!("cargo:rerun-if-changed=content");
}

fn collect(root: &Path, dir: &Path, files: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let path = e.path();
        if path.is_dir() {
            println!("cargo:rerun-if-changed={}", path.display());
            collect(root, &path, files);
        } else if path.extension().is_some_and(|x| x == "json") {
            println!("cargo:rerun-if-changed={}", path.display());
            files.push(path.strip_prefix(root).unwrap().display().to_string().replace('\\', "/"));
        }
    }
}
