use std::fs;
use std::path::{Path, PathBuf};

/// Maximum number of lines allowed in a single `.rs` source file.
const MAX_FILE_LINES: usize = 300;

#[test]
fn test_enforce_one_class_per_file() {
    let src_root = find_src_root();
    let mut violations = Vec::new();

    walk_rs_files(&src_root, &mut |path| {
        if should_skip_file(path) {
            return;
        }

        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                violations.push(format!("{}: failed to read: {}", path.display(), e));
                return;
            }
        };

        let class_count = count_classes(&content);
        if class_count > 1 {
            violations.push(format!(
                "{} contains {} classes/traits (expected <= 1)",
                path.display(),
                class_count
            ));
        }
    });

    report_violations("one pub struct / pub trait per file", &violations);
}

#[test]
fn test_enforce_max_file_length() {
    let src_root = find_src_root();
    let mut violations = Vec::new();

    walk_rs_files(&src_root, &mut |path| {
        if should_skip_file(path) {
            return;
        }

        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                violations.push(format!("{}: failed to read: {}", path.display(), e));
                return;
            }
        };

        let line_count = content.lines().count();
        if line_count > MAX_FILE_LINES {
            violations.push(format!(
                "{} has {} lines (limit {})",
                path.display(),
                line_count,
                MAX_FILE_LINES
            ));
        }
    });

    report_violations(
        &format!("max {} lines per file", MAX_FILE_LINES),
        &violations,
    );
}

fn find_src_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src")
}

fn walk_rs_files(root: &Path, callback: &mut dyn FnMut(&Path)) {
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.is_dir() {
                    if !should_skip_dir(&path) {
                        stack.push(path);
                    }
                } else if path.extension().is_some_and(|ext| ext == "rs") {
                    callback(&path);
                }
            }
        }
    }
}

fn should_skip_dir(path: &Path) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    matches!(name, "tests" | "target" | ".git" | ".build")
}

fn should_skip_file(path: &Path) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if name == "mod.rs" || name == "lib.rs" {
        return true;
    }
    if name.ends_with(".test.rs")
        || name == "tests.rs"
        || name.starts_with("test_")
        || name.ends_with("_test.rs")
    {
        return true;
    }
    false
}

fn count_classes(content: &str) -> usize {
    content
        .lines()
        .filter(|line| {
            let t = line.trim();
            if t.starts_with("//") || t.starts_with("/*") || t.starts_with('*') {
                return false;
            }
            t.starts_with("pub struct ") || t.starts_with("pub trait ")
        })
        .count()
}

fn report_violations(rule: &str, violations: &[String]) {
    if violations.is_empty() {
        return;
    }
    let count = violations.len();
    let display_limit = 50;
    let displayed = if count > display_limit {
        &violations[..display_limit]
    } else {
        violations
    };

    panic!(
        "Codebase standard violation: '{}' rule failed ({} violations):\n{}{}",
        rule,
        count,
        displayed.join("\n"),
        if count > display_limit {
            "\n... and more"
        } else {
            ""
        }
    );
}
