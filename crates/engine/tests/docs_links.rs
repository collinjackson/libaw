//! Every relative link in the Markdown docs points at a file that exists, so the docs site and
//! the repo never have dead links. (Web links aren't checked: that would need the network.)
use std::path::{Path, PathBuf};

fn md_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            md_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "md") {
            out.push(p);
        }
    }
}

#[test]
fn relative_links_in_the_docs_resolve() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files: Vec<PathBuf> = ["README.md", "AGENTS.md", "CONTRIBUTING.md"].iter().map(|f| root.join(f)).collect();
    md_files(&root.join("docs"), &mut files);
    let mut bad = vec![];
    for f in &files {
        let text = std::fs::read_to_string(f).unwrap();
        // [text](target): skip web links, mail links and same-page anchors.
        for (i, _) in text.match_indices("](") {
            let rest = &text[i + 2..];
            let Some(end) = rest.find(')') else { continue };
            let target = rest[..end].split('#').next().unwrap().trim();
            if target.is_empty() || target.contains("://") || target.starts_with("mailto:") || target.contains(' ') {
                continue;
            }
            if !f.parent().unwrap().join(target).exists() {
                bad.push(format!("{}: {target}", f.strip_prefix(&root).unwrap_or(f).display()));
            }
        }
    }
    assert!(bad.is_empty(), "dead links:\n{}", bad.join("\n"));
}
