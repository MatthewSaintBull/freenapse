//! Installed games, read from the launchers' own records (no network, no
//! Razer game database): Steam library manifests, Epic Games manifests, GOG
//! Galaxy's registry keys, the uninstall entries of EA app, Battle.net, Riot,
//! Ubisoft Connect and Rockstar games, and Xbox app installs in `X:\XboxGames`.
//!
//! A found game is matched by its install folder: any program running from
//! inside it (the game, its launcher, a 64-bit sub-binary) counts as the game.

#![allow(unsafe_op_in_unsafe_fn)]

use std::path::{Path, PathBuf};
use std::ptr;

use windows_sys::Win32::Foundation::ERROR_SUCCESS;
use windows_sys::Win32::System::Registry::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Game {
    pub name: String,
    /// Launcher it was found in, e.g. "Steam".
    pub source: &'static str,
    /// Install folder.
    pub dir: String,
    /// Main executable file name when the launcher records it (Epic, GOG, Xbox), else empty.
    pub exe: String,
    /// Cover or logo image the launcher keeps for the game, when one exists.
    pub art: Option<String>,
}

/// Every installed game found, sorted by name, one entry per install folder.
pub fn scan() -> Vec<Game> {
    let mut all = Vec::new();
    all.extend(steam());
    all.extend(epic());
    all.extend(gog());
    all.extend(xbox());
    all.extend(uninstall_entries());
    let mut out: Vec<Game> = Vec::new();
    for g in all {
        // The first launcher wins: an EA game bought on Steam is also in the uninstall list.
        let d = norm_dir(&g.dir);
        if d.is_empty() || out.iter().any(|o| { let od = norm_dir(&o.dir); od == d || is_inside(&d, &od) || is_inside(&od, &d) }) {
            continue;
        }
        out.push(g);
    }
    out.sort_by_key(|g| g.name.to_lowercase());
    out
}

/// Lower-case folder path with backslashes and no trailing separator.
pub fn norm_dir(dir: &str) -> String {
    dir.trim().replace('/', "\\").trim_end_matches('\\').to_lowercase()
}

/// True when `path` (normalized) lies inside the folder `dir` (normalized).
pub fn is_inside(path: &str, dir: &str) -> bool {
    !dir.is_empty() && path.len() > dir.len() && path.starts_with(dir) && path.as_bytes()[dir.len()] == b'\\'
}

fn file_name(p: &str) -> String {
    p.rsplit(['\\', '/']).next().unwrap_or(p).to_string()
}

// ---------------------------------------------------------------------------
// Steam: libraryfolders.vdf lists the libraries, appmanifest_<id>.acf each game.
// ---------------------------------------------------------------------------

/// Steam's tools and runtimes, which are "apps" but not games.
fn steam_skip(appid: &str, name: &str) -> bool {
    const IDS: [&str; 4] = ["228980", "1070560", "1391110", "1628350"];
    const PREFIXES: [&str; 5] = ["Steamworks", "Steam Linux Runtime", "Proton", "SteamVR", "Steam Audio"];
    IDS.contains(&appid) || PREFIXES.iter().any(|p| name.starts_with(p))
}

fn steam() -> Vec<Game> {
    let root = reg_string(HKEY_CURRENT_USER, "Software\\Valve\\Steam", "SteamPath")
        .or_else(|| reg_string(HKEY_LOCAL_MACHINE, "SOFTWARE\\WOW6432Node\\Valve\\Steam", "InstallPath"));
    let Some(root) = root else { return Vec::new() };
    let root = PathBuf::from(root.replace('/', "\\"));
    // The library list spells the paths properly (the registry has the root in lower case).
    let mut libs: Vec<PathBuf> = std::fs::read_to_string(root.join("steamapps").join("libraryfolders.vdf"))
        .map(|text| vdf_values(&text, "path").into_iter().map(PathBuf::from).collect())
        .unwrap_or_default();
    if !libs.iter().any(|l| norm_dir(&l.to_string_lossy()) == norm_dir(&root.to_string_lossy())) {
        libs.push(root.clone());
    }
    let mut out = Vec::new();
    for lib in libs {
        let apps = lib.join("steamapps");
        let Ok(rd) = std::fs::read_dir(&apps) else { continue };
        for e in rd.flatten() {
            let fname = e.file_name().to_string_lossy().to_string();
            if !(fname.starts_with("appmanifest_") && fname.ends_with(".acf")) {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(e.path()) else { continue };
            let first = |k: &str| vdf_values(&text, k).into_iter().next().unwrap_or_default();
            let (appid, name, installdir) = (first("appid"), first("name"), first("installdir"));
            if name.is_empty() || installdir.is_empty() || steam_skip(&appid, &name) {
                continue;
            }
            let dir = apps.join("common").join(&installdir);
            if dir.is_dir() {
                let art = steam_art(&root.join("appcache").join("librarycache"), &appid);
                out.push(Game { name, source: "Steam", dir: dir.to_string_lossy().to_string(), exe: String::new(), art });
            }
        }
    }
    out
}

/// The game's cover in Steam's library cache. Layouts seen: `<appid>_library_600x900.jpg`,
/// `<appid>/library_600x900.jpg`, and `<appid>/<hash>/library_600x900.jpg` (one hash folder
/// per asset); games without a portrait get the capsule or the header picture.
fn steam_art(cache: &Path, appid: &str) -> Option<String> {
    const NAMES: [&str; 4] = ["library_600x900.jpg", "library_capsule.jpg", "library_header.jpg", "header.jpg"];
    let dir = cache.join(appid);
    let mut dirs = vec![dir.clone()];
    if let Ok(rd) = std::fs::read_dir(&dir) {
        dirs.extend(rd.flatten().map(|e| e.path()).filter(|p| p.is_dir()));
    }
    for name in NAMES {
        if let Some(p) = dirs.iter().map(|d| d.join(name)).find(|p| p.is_file()) {
            return Some(p.to_string_lossy().to_string());
        }
    }
    let flat = cache.join(format!("{appid}_library_600x900.jpg"));
    flat.is_file().then(|| flat.to_string_lossy().to_string())
}

/// All values of `"key" "value"` pairs in a Valve KeyValues text, at any depth.
pub fn vdf_values(text: &str, key: &str) -> Vec<String> {
    let toks = vdf_tokens(text);
    let mut out = Vec::new();
    for w in toks.windows(2) {
        if let (Some(k), Some(v)) = (&w[0], &w[1]) {
            if k.eq_ignore_ascii_case(key) {
                out.push(v.clone());
            }
        }
    }
    out
}

/// Quoted strings (unescaped) as `Some`, braces as `None`.
fn vdf_tokens(text: &str) -> Vec<Option<String>> {
    let mut toks = Vec::new();
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                let mut s = String::new();
                while let Some(c) = chars.next() {
                    match c {
                        '"' => break,
                        '\\' => match chars.next() {
                            Some('n') => s.push('\n'),
                            Some('t') => s.push('\t'),
                            Some(o) => s.push(o),
                            None => break,
                        },
                        o => s.push(o),
                    }
                }
                toks.push(Some(s));
            }
            '{' | '}' => toks.push(None),
            _ => {}
        }
    }
    toks
}

// ---------------------------------------------------------------------------
// Epic Games: one JSON .item manifest per installed app.
// ---------------------------------------------------------------------------

fn epic() -> Vec<Game> {
    let data = reg_string(HKEY_LOCAL_MACHINE, "SOFTWARE\\WOW6432Node\\Epic Games\\EpicGamesLauncher", "AppDataPath").map(PathBuf::from).unwrap_or_else(|| {
        let pd = std::env::var_os("PROGRAMDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("C:\\ProgramData"));
        pd.join("Epic").join("EpicGamesLauncher").join("Data")
    });
    let Ok(rd) = std::fs::read_dir(data.join("Manifests")) else { return Vec::new() };
    let mut out = Vec::new();
    for e in rd.flatten() {
        if e.path().extension().is_none_or(|x| !x.eq_ignore_ascii_case("item")) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(e.path()) else { continue };
        if let Some(g) = epic_item(&text) {
            if Path::new(&g.dir).is_dir() {
                out.push(g);
            }
        }
    }
    out
}

/// A game from an Epic manifest; `None` for apps, DLC and unfinished installs.
pub fn epic_item(text: &str) -> Option<Game> {
    if json_bool(text, "bIsIncompleteInstall") == Some(true) {
        return None;
    }
    if !json_array(text, "AppCategories").is_some_and(|a| a.iter().any(|c| c == "games")) {
        return None;
    }
    let app = json_str(text, "AppName").unwrap_or_default();
    let main = json_str(text, "MainGameAppName").unwrap_or_default();
    if !main.is_empty() && main != app {
        return None;
    }
    let name = json_str(text, "DisplayName")?;
    let dir = json_str(text, "InstallLocation")?;
    let exe = json_str(text, "LaunchExecutable").map(|x| file_name(&x)).unwrap_or_default();
    Some(Game { name, source: "Epic Games", dir, exe, art: None })
}

/// Position just after `"key"` and the following colon.
fn json_after_key(text: &str, key: &str) -> Option<usize> {
    let pat = format!("\"{key}\"");
    let i = text.find(&pat)? + pat.len();
    let rest = &text[i..];
    let colon = rest.find(':')?;
    if !rest[..colon].trim().is_empty() {
        return None;
    }
    Some(i + colon + 1)
}

/// Read a JSON string starting at the opening quote; returns the value and the index after it.
fn json_string_at(text: &str, start: usize) -> Option<(String, usize)> {
    let rest = &text[start..];
    let q = rest.find('"')?;
    if !rest[..q].trim().is_empty() {
        return None;
    }
    let mut s = String::new();
    let mut it = rest[q + 1..].char_indices();
    while let Some((i, c)) = it.next() {
        match c {
            '"' => return Some((s, start + q + 1 + i + 1)),
            '\\' => match it.next().map(|(_, c)| c) {
                Some('n') => s.push('\n'),
                Some('t') => s.push('\t'),
                Some('u') => {
                    let hex: String = (0..4).filter_map(|_| it.next().map(|(_, c)| c)).collect();
                    s.push(u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32).unwrap_or('?'));
                }
                Some(o) => s.push(o),
                None => return None,
            },
            o => s.push(o),
        }
    }
    None
}

pub fn json_str(text: &str, key: &str) -> Option<String> {
    json_string_at(text, json_after_key(text, key)?).map(|(s, _)| s)
}

fn json_bool(text: &str, key: &str) -> Option<bool> {
    let v = text[json_after_key(text, key)?..].trim_start();
    if v.starts_with("true") {
        Some(true)
    } else if v.starts_with("false") {
        Some(false)
    } else {
        None
    }
}

fn json_array(text: &str, key: &str) -> Option<Vec<String>> {
    let start = json_after_key(text, key)?;
    let rest = &text[start..];
    let open = rest.find('[')?;
    if !rest[..open].trim().is_empty() {
        return None;
    }
    let mut pos = start + open + 1;
    let mut out = Vec::new();
    loop {
        let r = text[pos..].trim_start();
        if r.starts_with(']') {
            return Some(out);
        }
        let (s, next) = json_string_at(text, pos)?;
        out.push(s);
        pos = next;
        let r = text[pos..].trim_start();
        if let Some(stripped) = r.strip_prefix(',') {
            pos = text.len() - stripped.len();
        }
    }
}

// ---------------------------------------------------------------------------
// GOG Galaxy: HKLM\SOFTWARE\WOW6432Node\GOG.com\Games\<id>.
// ---------------------------------------------------------------------------

fn gog() -> Vec<Game> {
    const KEY: &str = "SOFTWARE\\WOW6432Node\\GOG.com\\Games";
    let mut out = Vec::new();
    for id in reg_subkeys(HKEY_LOCAL_MACHINE, KEY) {
        let sub = format!("{KEY}\\{id}");
        let (Some(name), Some(dir)) = (reg_string(HKEY_LOCAL_MACHINE, &sub, "gameName"), reg_string(HKEY_LOCAL_MACHINE, &sub, "path")) else { continue };
        if Path::new(&dir).is_dir() {
            let exe = reg_string(HKEY_LOCAL_MACHINE, &sub, "exe").map(|x| file_name(&x)).unwrap_or_default();
            out.push(Game { name, source: "GOG", dir, exe, art: None });
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Xbox app (PC Game Pass): X:\XboxGames\<game>\Content on fixed drives.
// ---------------------------------------------------------------------------

fn xbox() -> Vec<Game> {
    use windows_sys::Win32::Storage::FileSystem::{GetDriveTypeW, GetLogicalDrives};
    const DRIVE_FIXED: u32 = 3;
    let mut out = Vec::new();
    let mask = unsafe { GetLogicalDrives() };
    for i in 0..26u32 {
        if mask & (1 << i) == 0 {
            continue;
        }
        let letter = (b'A' + i as u8) as char;
        let root: Vec<u16> = format!("{letter}:\\\0").encode_utf16().collect();
        if unsafe { GetDriveTypeW(root.as_ptr()) } != DRIVE_FIXED {
            continue;
        }
        let Ok(rd) = std::fs::read_dir(format!("{letter}:\\XboxGames")) else { continue };
        for e in rd.flatten() {
            let dir = e.path();
            if !dir.join("Content").is_dir() {
                continue;
            }
            let config = std::fs::read_to_string(dir.join("Content").join("MicrosoftGame.config")).unwrap_or_default();
            let attr = |tag: &str, a: &str| {
                let t = config.find(&format!("<{tag}"))?;
                let rest = &config[t..];
                let rest = &rest[..rest.find('>').unwrap_or(rest.len())];
                let i = rest.find(&format!("{a}=\""))? + a.len() + 2;
                Some(rest[i..].split('"').next()?.to_string())
            };
            let name = attr("ShellVisuals", "DefaultDisplayName").filter(|n| !n.is_empty() && !n.starts_with("ms-resource")).unwrap_or_else(|| e.file_name().to_string_lossy().to_string());
            let exe = attr("Executable", "Name").map(|x| file_name(&x)).unwrap_or_default();
            // The largest logo the package ships (paths are relative to Content).
            let art = ["Square480x480Logo", "Square150x150Logo", "StoreLogo"]
                .iter()
                .filter_map(|a| attr("ShellVisuals", a))
                .map(|p| dir.join("Content").join(p.replace('/', "\\")))
                .find(|p| p.is_file())
                .map(|p| p.to_string_lossy().to_string());
            out.push(Game { name, source: "Xbox", dir: dir.to_string_lossy().to_string(), exe, art });
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Uninstall entries: EA app, Battle.net, Riot, Ubisoft Connect and Rockstar
// register each game there with its install folder.
// ---------------------------------------------------------------------------

/// Launcher name for a game publisher's uninstall entry.
fn publisher_source(publisher: &str) -> Option<&'static str> {
    let p = publisher.to_lowercase();
    [("electronic arts", "EA app"), ("blizzard", "Battle.net"), ("riot games", "Riot"), ("ubisoft", "Ubisoft Connect"), ("rockstar games", "Rockstar")]
        .iter()
        .find(|(k, _)| p.contains(k))
        .map(|(_, s)| *s)
}

/// The launchers and services themselves, which share the publishers' names.
fn is_launcher(name: &str) -> bool {
    const LAUNCHERS: [&str; 9] = ["ea app", "origin", "battle.net", "ubisoft connect", "uplay", "riot client", "rockstar games launcher", "rockstar games social club", "ea anticheat"];
    let n = name.to_lowercase();
    LAUNCHERS.iter().any(|l| n == *l || n.starts_with(&format!("{l} ")))
}

fn uninstall_entries() -> Vec<Game> {
    const KEY: &str = "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall";
    const KEY32: &str = "SOFTWARE\\WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\Uninstall";
    let mut out = Vec::new();
    for (root, key) in [(HKEY_LOCAL_MACHINE, KEY), (HKEY_LOCAL_MACHINE, KEY32), (HKEY_CURRENT_USER, KEY)] {
        for id in reg_subkeys(root, key) {
            let sub = format!("{key}\\{id}");
            let Some(source) = reg_string(root, &sub, "Publisher").as_deref().and_then(publisher_source) else { continue };
            let (Some(name), Some(dir)) = (reg_string(root, &sub, "DisplayName"), reg_string(root, &sub, "InstallLocation")) else { continue };
            let dir = dir.trim().trim_matches('"').trim_end_matches('\\').to_string();
            if is_launcher(&name) || dir.is_empty() || !Path::new(&dir).is_dir() {
                continue;
            }
            out.push(Game { name, source, dir, exe: String::new(), art: None });
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Registry helpers.
// ---------------------------------------------------------------------------

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn reg_string(root: HKEY, key: &str, value: &str) -> Option<String> {
    unsafe {
        let mut buf = vec![0u16; 2048];
        let mut len = (buf.len() * 2) as u32;
        let rc = RegGetValueW(root, wide(key).as_ptr(), wide(value).as_ptr(), RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ, ptr::null_mut(), buf.as_mut_ptr() as *mut _, &mut len);
        if rc != ERROR_SUCCESS {
            return None;
        }
        let n = (len as usize / 2).min(buf.len());
        let s = String::from_utf16_lossy(&buf[..n]).trim_end_matches('\0').to_string();
        if s.is_empty() { None } else { Some(s) }
    }
}

fn reg_subkeys(root: HKEY, key: &str) -> Vec<String> {
    let mut out = Vec::new();
    unsafe {
        let mut h: HKEY = 0;
        if RegOpenKeyExW(root, wide(key).as_ptr(), 0, KEY_READ, &mut h) != ERROR_SUCCESS {
            return out;
        }
        let mut i = 0;
        loop {
            let mut buf = [0u16; 256];
            let mut len = buf.len() as u32;
            let rc = RegEnumKeyExW(h, i, buf.as_mut_ptr(), &mut len, ptr::null(), ptr::null_mut(), ptr::null_mut(), ptr::null_mut());
            if rc != ERROR_SUCCESS {
                break;
            }
            out.push(String::from_utf16_lossy(&buf[..len as usize]));
            i += 1;
        }
        RegCloseKey(h);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vdf() {
        let text = "\"libraryfolders\"\n{\n\t\"0\"\n\t{\n\t\t\"path\"\t\t\"C:\\\\Program Files (x86)\\\\Steam\"\n\t\t\"apps\" { \"3910\" \"1\" }\n\t}\n\t\"1\" { \"path\" \"D:\\\\SteamLibrary\" }\n}";
        assert_eq!(vdf_values(text, "path"), vec!["C:\\Program Files (x86)\\Steam", "D:\\SteamLibrary"]);
        let acf = "\"AppState\"\n{\n\t\"appid\"\t\t\"252950\"\n\t\"name\"\t\t\"Rocket League\"\n\t\"installdir\"\t\t\"rocketleague\"\n}";
        assert_eq!(vdf_values(acf, "name"), vec!["Rocket League"]);
        assert_eq!(vdf_values(acf, "installdir"), vec!["rocketleague"]);
        assert!(steam_skip("228980", "Steamworks Common Redistributables"));
        assert!(!steam_skip("252950", "Rocket League"));
    }

    #[test]
    fn epic_manifest() {
        let item = r#"{
            "bIsIncompleteInstall": false,
            "LaunchExecutable": "Binaries/Win64/Breach.exe",
            "AppCategories": [
                "public",
                "games",
                "applications"
            ],
            "DisplayName": "Into The Breach \u2122",
            "InstallLocation": "C:\\Program Files\\Epic Games\\IntoTheBreach",
            "AppName": "Blobfish",
            "MainGameAppName": ""
        }"#;
        let g = epic_item(item).unwrap();
        assert_eq!(g.name, "Into The Breach ™");
        assert_eq!(g.dir, "C:\\Program Files\\Epic Games\\IntoTheBreach");
        assert_eq!(g.exe, "Breach.exe");
        let dlc = item.replace("\"MainGameAppName\": \"\"", "\"MainGameAppName\": \"Other\"");
        assert!(epic_item(&dlc).is_none());
        let app = item.replace("\"games\",", "");
        assert!(epic_item(&app).is_none());
        let partial = item.replace("\"bIsIncompleteInstall\": false", "\"bIsIncompleteInstall\": true");
        assert!(epic_item(&partial).is_none());
    }

    #[test]
    fn folders() {
        assert_eq!(norm_dir("C:/Games/Foo/"), "c:\\games\\foo");
        assert!(is_inside("c:\\games\\foo\\bin\\foo.exe", "c:\\games\\foo"));
        assert!(!is_inside("c:\\games\\foobar\\x.exe", "c:\\games\\foo"));
        assert!(!is_inside("c:\\games\\foo", "c:\\games\\foo"));
        assert_eq!(publisher_source("Electronic Arts, Inc."), Some("EA app"));
        assert!(is_launcher("EA app"));
        assert!(!is_launcher("Hearthstone"));
    }
}
