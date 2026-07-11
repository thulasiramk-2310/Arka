use std::path::Path;

const SANDBOXED: &[&str] = &["firefox", "firefox-esr"];

#[derive(Clone, Debug)]
pub struct AppEntry {
    pub name:      String,
    pub exec:      String,
    pub icon:      String,
    pub sandboxed: bool,
    pub keywords:  String,
}

impl AppEntry {
    pub fn matches(&self, query: &str) -> bool {
        let q = query.to_lowercase();
        self.name.to_lowercase().contains(&q)
            || self.exec.to_lowercase().contains(&q)
            || self.keywords.to_lowercase().contains(&q)
    }
}

pub fn load() -> Vec<AppEntry> {
    let dirs = [
        "/usr/share/applications",
        "/usr/local/share/applications",
    ];
    let mut apps: Vec<AppEntry> = Vec::new();

    for dir in &dirs {
        let path = Path::new(dir);
        if !path.is_dir() { continue; }
        let Ok(entries) = std::fs::read_dir(path) else { continue; };
        for entry in entries.flatten() {
            let p = entry.path();
            if p.extension().and_then(|e| e.to_str()) != Some("desktop") { continue; }
            if let Some(app) = parse_desktop(&p) {
                apps.push(app);
            }
        }
    }

    apps.sort_by(|a, b| a.name.cmp(&b.name));
    apps.dedup_by(|a, b| a.name == b.name);
    apps
}

fn parse_desktop(path: &Path) -> Option<AppEntry> {
    let content = std::fs::read_to_string(path).ok()?;
    let mut name     = String::new();
    let mut exec     = String::new();
    let mut icon     = String::new();
    let mut keywords = String::new();
    let mut hidden = false;
    let mut no_display = false;
    let mut in_desktop_entry = false;

    for line in content.lines() {
        let line = line.trim();
        if line == "[Desktop Entry]" { in_desktop_entry = true; continue; }
        if line.starts_with('[') { in_desktop_entry = false; continue; }
        if !in_desktop_entry { continue; }

        if let Some(v) = line.strip_prefix("Name=") {
            if name.is_empty() { name = v.to_string(); }
        } else if let Some(v) = line.strip_prefix("Exec=") {
            exec = strip_exec_codes(v);
        } else if let Some(v) = line.strip_prefix("Icon=") {
            icon = v.to_string();
        } else if let Some(v) = line.strip_prefix("Keywords=") {
            keywords = v.replace(';', " ");
        } else if line == "Hidden=true" || line == "Hidden=True" {
            hidden = true;
        } else if line == "NoDisplay=true" || line == "NoDisplay=True" {
            no_display = true;
        } else if line.strip_prefix("Type=").map(|t| t != "Application").unwrap_or(false) {
            return None;
        }
    }

    if hidden || no_display || name.is_empty() || exec.is_empty() { return None; }

    let stem = path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase();
    let sandboxed = SANDBOXED.iter().any(|s| stem.contains(s));

    Some(AppEntry { name, exec, icon, sandboxed, keywords })
}

fn strip_exec_codes(exec: &str) -> String {
    let mut out = String::new();
    let mut chars = exec.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '%' {
            chars.next();
        } else {
            out.push(c);
        }
    }
    out.trim().to_string()
}
