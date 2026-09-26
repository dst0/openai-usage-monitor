//! Cache keys must hash files that a clean checkout contains. `hashFiles`
//! returns an empty string for a missing or ignored file, so a key built from
//! one never changes and `actions/cache`, which never overwrites a key,
//! restores the first saved entry forever. Each `hashFiles` argument must be
//! a single-quoted literal path committed at `HEAD`; patterns are rejected
//! rather than evaluated. GitHub expression function names are
//! case-insensitive, so any spelling of `hashFiles(` is checked.

use crate::git_repo::GitRepo;

/// Characters that make a `hashFiles` argument a pattern (`!` negates).
const PATTERN_CHARS: [char; 4] = ['*', '?', '[', '!'];

pub fn cache_key_input_violations(text: &str, repo: &GitRepo) -> Vec<String> {
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        for call in hash_files_calls(line) {
            let paths = match call {
                Ok(paths) => paths,
                Err(problem) => {
                    out.push(format!("line {}: {problem}", i + 1));
                    continue;
                }
            };
            for path in paths {
                match repo.committed_file(&path) {
                    Ok(true) => {}
                    Ok(false) => out.push(format!(
                        "line {}: `hashFiles('{path}')` names a file that is not committed; a \
                         clean checkout hashes nothing and the cache key never changes",
                        i + 1
                    )),
                    Err(e) => out.push(format!(
                        "line {}: cannot verify that `{path}` is committed: {e}",
                        i + 1
                    )),
                }
            }
        }
    }
    out
}

/// Literal paths of each `hashFiles(...)` call on `line`, or why a call
/// cannot be read.
pub fn hash_files_calls(line: &str) -> Vec<Result<Vec<String>, String>> {
    let lower = line.to_ascii_lowercase();
    lower
        .match_indices("hashfiles(")
        .map(|(at, call)| {
            let (args, _) = line[at + call.len()..]
                .split_once(')')
                .ok_or("`hashFiles(` must close on its own line")?;
            args.split(',').map(literal_path).collect()
        })
        .collect()
}

fn literal_path(arg: &str) -> Result<String, String> {
    let arg = arg.trim();
    let path = arg
        .strip_prefix('\'')
        .and_then(|a| a.strip_suffix('\''))
        .filter(|p| !p.is_empty() && !p.contains('\''))
        .ok_or_else(|| format!("`hashFiles` argument `{arg}` is not a single-quoted path"))?;
    if path.contains(PATTERN_CHARS) {
        return Err(format!(
            "`hashFiles('{path}')` is a pattern; name each committed file literally"
        ));
    }
    Ok(path.to_string())
}

#[cfg(test)]
#[path = "cache_keys.test.rs"]
mod tests;
