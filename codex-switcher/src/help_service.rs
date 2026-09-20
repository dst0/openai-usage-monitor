pub(super) struct HelpService;

impl HelpService {
    pub(super) fn resolve_path() -> std::path::PathBuf {
        let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("/"));
        let candidates = [
            home.join(".codex/helps.html"),
            home.join("Applications/Codex Monitor.app/Contents/Resources/helps.html"),
            std::path::PathBuf::from(
                "/Applications/Codex Monitor.app/Contents/Resources/helps.html",
            ),
            home.join("dev/openai-usage-monitor/resources/helps.html"),
        ];
        for p in &candidates {
            if p.exists() {
                return p.clone();
            }
        }
        home.join(".codex/helps.html")
    }

    pub(super) fn open_in_browser() -> Result<(), String> {
        let path = Self::resolve_path();
        let path_str = path.display().to_string();
        let encoded_path = percent_encode_path(&path_str);
        let target_url = format!("file://{}", encoded_path);
        println!("📖 Opening documentation: {}", target_url);

        #[cfg(target_os = "macos")]
        let status = std::process::Command::new("open").arg(&target_url).status();
        #[cfg(not(target_os = "macos"))]
        let status = std::process::Command::new("xdg-open")
            .arg(&target_url)
            .status();

        status
            .map_err(|e| format!("Failed to open browser: {}", e))
            .map(|_| ())
    }
}

fn percent_encode_path(path: &str) -> String {
    let mut encoded = String::new();
    for byte in path.bytes() {
        match byte {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                encoded.push(byte as char);
            }
            _ => {
                encoded.push_str(&format!("%{:02X}", byte));
            }
        }
    }
    encoded
}
