//! Flathub's own catalogue (the appstream file Flatpak downloads with the remote): names,
//! publishers, summaries, icons, licences, releases. Nothing here is typed in by ArkaOS
//! except the short list of picks in /usr/share/arkaos/apps.json, which `arka` uses too.

use std::path::{Path, PathBuf};

use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

#[derive(Debug, Clone, PartialEq)]
pub enum Para {
    P(String),
    Li(String),
}

#[derive(Debug, Clone, Default)]
pub struct App {
    pub id: String,
    pub name: String,
    pub summary: String,
    pub developer: String,
    /// Flathub checked that the publisher is who the app says it is.
    pub verified: bool,
    pub categories: Vec<String>,
    pub keywords: Vec<String>,
    pub license: String,
    pub homepage: String,
    pub description: Vec<Para>,
    pub version: String,
    /// Unix time of the newest release the app lists, 0 when it lists none.
    pub released: i64,
    pub icon: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct Pick {
    pub id: String,
    pub name: String,
    pub about: String,
}

/// Where Flatpak keeps the Flathub catalogue: the user's copy first (what `arka`
/// sets up), then the system one. Returns the folder and the installation flag.
pub fn locate() -> Option<(PathBuf, &'static str)> {
    let arch = std::env::consts::ARCH;
    let tail = format!("flatpak/appstream/flathub/{arch}/active");
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    let user = home.join(".local/share").join(&tail);
    let system = Path::new("/var/lib").join(&tail);
    [(user, "--user"), (system, "--system")]
        .into_iter()
        .find(|(d, _)| d.join("appstream.xml").is_file())
}

/// The user's copy, whether or not it exists yet.
pub fn user_dir() -> PathBuf {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    home.join(".local/share/flatpak/appstream/flathub")
        .join(std::env::consts::ARCH)
        .join("active")
}

pub fn load(dir: &Path) -> Result<Vec<App>, String> {
    let xml = std::fs::read_to_string(dir.join("appstream.xml")).map_err(|e| e.to_string())?;
    let mut apps = parse(&xml)?;
    for a in &mut apps {
        if let Some(name) = a.icon.take() {
            a.icon = ["128x128", "64x64"]
                .iter()
                .map(|s| dir.join("icons").join(s).join(&name))
                .find(|p| p.is_file());
        }
    }
    apps.sort_by_key(|a| a.name.to_lowercase());
    Ok(apps)
}

pub fn load_picks() -> Vec<Pick> {
    let path = std::env::var("ARKA_APPS_CATALOG").unwrap_or_else(|_| "/usr/share/arkaos/apps.json".into());
    let Ok(text) = std::fs::read_to_string(path) else { return Vec::new() };
    let Ok(serde_json::Value::Array(items)) = serde_json::from_str(&text) else { return Vec::new() };
    items
        .iter()
        .filter_map(|v| {
            Some(Pick {
                id: v.get("id")?.as_str()?.to_string(),
                name: v.get("name")?.as_str()?.to_string(),
                about: v.get("about").and_then(|s| s.as_str()).unwrap_or("").to_string(),
            })
        })
        .collect()
}

fn attr(e: &BytesStart, key: &str) -> Option<String> {
    e.try_get_attribute(key).ok().flatten().and_then(|a| a.normalized_value(quick_xml::XmlVersion::Implicit1_0).ok().map(|v| v.into_owned()))
}

fn squash(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Days since 1970-01-01 for a civil date (Howard Hinnant's algorithm).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn release_time(e: &BytesStart) -> i64 {
    if let Some(t) = attr(e, "timestamp").and_then(|t| t.parse().ok()) {
        return t;
    }
    let Some(d) = attr(e, "date") else { return 0 };
    let p: Vec<i64> = d.get(..10).unwrap_or("").split('-').filter_map(|x| x.parse().ok()).collect();
    if p.len() == 3 { days_from_civil(p[0], p[1], p[2]) * 86400 } else { 0 }
}

const CAPTURE: &[&str] = &[
    "id", "name", "summary", "developer_name", "project_license", "url", "category", "keyword", "p", "li", "icon", "value",
    "bundle",
];

pub fn parse(xml: &str) -> Result<Vec<App>, String> {
    let mut r = Reader::from_str(xml);
    r.config_mut().expand_empty_elements = true;
    let mut out = Vec::new();
    // (element name, has xml:lang) from <component> down.
    let mut stack: Vec<(String, bool)> = Vec::new();
    let mut cur: Option<App> = None;
    let mut buf = String::new();
    let mut icon_w = 0u32;
    let mut url_home = false;
    let mut value_verified = false;
    let mut first_release = true;
    loop {
        match r.read_event().map_err(|e| format!("Flathub's list is damaged: {e}"))? {
            Event::Start(e) => {
                let name = String::from_utf8_lossy(e.local_name().as_ref()).into_owned();
                let lang = e.try_get_attribute("xml:lang").ok().flatten().is_some();
                // <components> is the root, so a component sits at depth 1.
                if name == "component" && stack.len() == 1 {
                    let kind = attr(&e, "type").unwrap_or_default();
                    if kind == "desktop-application" || kind == "desktop" {
                        cur = Some(App::default());
                        first_release = true;
                    }
                }
                if cur.is_some() {
                    if CAPTURE.contains(&name.as_str()) {
                        buf.clear();
                    }
                    match name.as_str() {
                        "icon" => {
                            icon_w = if attr(&e, "type").as_deref() == Some("cached") {
                                attr(&e, "width").and_then(|w| w.parse().ok()).unwrap_or(0)
                            } else {
                                0
                            };
                        }
                        "url" => url_home = attr(&e, "type").as_deref() == Some("homepage"),
                        "value" => value_verified = attr(&e, "key").as_deref() == Some("flathub::verification::verified"),
                        "release" if stack.last().map(|s| s.0.as_str()) == Some("releases") => {
                            if let Some(a) = cur.as_mut() {
                                if first_release {
                                    a.version = attr(&e, "version").unwrap_or_default();
                                    a.released = release_time(&e);
                                    first_release = false;
                                }
                            }
                        }
                        _ => {}
                    }
                }
                stack.push((name, lang));
            }
            Event::Text(t) => {
                if cur.is_some() {
                    buf.push_str(&t.decode().map_err(|e| e.to_string())?);
                }
            }
            Event::CData(t) => {
                if cur.is_some() {
                    buf.push_str(&t.decode().map_err(|e| e.to_string())?);
                }
            }
            Event::GeneralRef(g) => {
                if cur.is_some() {
                    if let Ok(Some(c)) = g.resolve_char_ref() {
                        buf.push(c);
                    } else if let Ok(n) = g.decode() {
                        buf.push_str(quick_xml::escape::resolve_predefined_entity(&n).unwrap_or(""));
                    }
                }
            }
            Event::End(_) => {
                let Some((name, own_lang)) = stack.pop() else { continue };
                // Elements that carry xml:lang (or sit inside one) are translations; we show the untranslated text.
                let translated = own_lang || stack.iter().any(|s| s.1);
                if name == "component" && stack.len() == 1 {
                    if let Some(a) = cur.take() {
                        if !a.id.is_empty() && !a.name.is_empty() {
                            out.push(a);
                        }
                    }
                    continue;
                }
                let Some(a) = cur.as_mut() else { continue };
                let parent = stack.last().map(|s| s.0.as_str()).unwrap_or("");
                if translated {
                    continue;
                }
                let text = squash(&buf);
                match (name.as_str(), parent) {
                    // Older entries name the .desktop file; <bundle> below has the real Flatpak id.
                    ("id", "component") => a.id = text.trim_end_matches(".desktop").to_string(),
                    ("bundle", "component") => {
                        if let Some(id) = text.strip_prefix("app/").and_then(|r| r.split('/').next()) {
                            a.id = id.to_string();
                        }
                    }
                    ("name", "component") => a.name = text,
                    ("summary", "component") => a.summary = text,
                    ("name", "developer") | ("developer_name", "component") if a.developer.is_empty() => a.developer = text,
                    ("project_license", "component") => a.license = text,
                    ("url", "component") if url_home => a.homepage = text,
                    ("category", "categories") => a.categories.push(text),
                    ("keyword", "keywords") => a.keywords.push(text.to_lowercase()),
                    ("p", "description") if !text.is_empty() => a.description.push(Para::P(text)),
                    ("li", _) if stack.iter().any(|s| s.0 == "description") && !text.is_empty() => a.description.push(Para::Li(text)),
                    ("icon", "component") if icon_w >= 64 => {
                        if a.icon.is_none() || icon_w == 128 {
                            a.icon = Some(PathBuf::from(text));
                        }
                    }
                    ("value", "custom") if value_verified => a.verified = text == "true",
                    _ => {}
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<components origin="flathub">
  <component type="desktop-application">
    <id>org.signal.Signal</id>
    <name xml:lang="de">Signal Schreibtisch</name>
    <name>Signal Desktop</name>
    <summary>Private messenger</summary>
    <summary xml:lang="fr">Messagerie privée</summary>
    <project_license>AGPL-3.0-only</project_license>
    <description><p>Signal is a <em>private</em> messenger &amp; more&#33;</p><p xml:lang="de">Nein</p><ul><li>Calls</li></ul></description>
    <developer><name>Signal Foundation</name></developer>
    <icon type="cached" width="64" height="64">org.signal.Signal.png</icon>
    <icon type="cached" width="128" height="128">org.signal.Signal.png</icon>
    <url type="bugtracker">https://example.org/bugs</url>
    <url type="homepage">https://signal.org/</url>
    <categories><category>Network</category><category>Chat</category></categories>
    <releases><release version="8.35.0" timestamp="1763000000"/><release version="8.34.0" timestamp="1762000000"/></releases>
    <custom><value key="flathub::verification::verified">false</value></custom>
  </component>
  <component type="desktop-application"><id>org.kde.kdenlive.desktop</id><name>Kdenlive</name>
    <bundle type="flatpak" runtime="org.kde.Platform/x86_64/6.11">app/org.kde.kdenlive/x86_64/stable</bundle></component>
  <component type="runtime"><id>org.freedesktop.Platform</id><name>Platform</name></component>
  <component type="desktop">
    <id>org.gimp.GIMP</id><name>GNU Image Manipulation Program</name><summary>High-end image creation</summary>
    <developer_name>The GIMP team</developer_name>
    <releases><release version="3.0.4" date="2025-06-01"/></releases>
    <custom><value key="flathub::verification::verified">true</value></custom>
  </component>
</components>"#;

    #[test]
    fn reads_untranslated_fields() {
        let apps = parse(XML).unwrap();
        assert_eq!(apps.len(), 3, "runtimes are not apps");
        assert_eq!(apps[1].id, "org.kde.kdenlive", "the Flatpak id, not the .desktop name");
        let s = &apps[0];
        assert_eq!(s.name, "Signal Desktop");
        assert_eq!(s.summary, "Private messenger");
        assert_eq!(s.developer, "Signal Foundation");
        assert_eq!(s.license, "AGPL-3.0-only");
        assert_eq!(s.homepage, "https://signal.org/");
        assert_eq!(s.version, "8.35.0");
        assert_eq!(s.released, 1763000000);
        assert!(!s.verified);
        assert_eq!(s.categories, ["Network", "Chat"]);
        assert_eq!(s.icon.as_deref(), Some(Path::new("org.signal.Signal.png")));
        assert_eq!(s.description, [Para::P("Signal is a private messenger & more!".into()), Para::Li("Calls".into())]);
    }

    #[test]
    fn reads_older_fields_and_dates() {
        let g = &parse(XML).unwrap()[2];
        assert_eq!(g.developer, "The GIMP team");
        assert!(g.verified);
        assert_eq!(g.released, 1748736000); // 2025-06-01 00:00 UTC
    }
}

#[cfg(test)]
mod real {
    /// cargo test --release -p arka-capsule real_catalogue -- --ignored --nocapture
    #[test]
    #[ignore]
    fn real_catalogue() {
        let (dir, scope) = super::locate().expect("no Flathub catalogue on this computer");
        let t = std::time::Instant::now();
        let apps = super::load(&dir).unwrap();
        println!("{scope}: {} apps in {:?}; icons {}, verified {}", apps.len(), t.elapsed(),
            apps.iter().filter(|a| a.icon.is_some()).count(), apps.iter().filter(|a| a.verified).count());
        for id in ["org.signal.Signal", "org.gimp.GIMP", "org.libreoffice.LibreOffice"] {
            let a = apps.iter().find(|a| a.id == id).unwrap();
            println!("{} | {} | {} | v{} | {} | {:?} | desc {} | {}", a.name, a.developer, a.summary, a.version, a.license, a.icon.as_ref().map(|p| p.is_file()), a.description.len(), a.homepage);
        }
    }
}
