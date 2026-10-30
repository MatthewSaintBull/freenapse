//! Update notice. Once a day the tray app asks GitHub which release is the
//! latest and, when it is newer than this build, says so in its menu and in one
//! notification. It is the only network request freenapse makes: one GET to
//! `api.github.com`, with nothing about the user or the PC in it beyond the
//! user agent `freenapse/<version>`. `[tray] check_updates = false` turns it off.

use std::ffi::c_void;
use std::ptr;

use windows_sys::Win32::Networking::WinHttp::*;

/// Where the menu item takes the user.
pub const RELEASES_URL: &str = "https://github.com/MatthewSaintBull/freenapse/releases/latest";

const API_HOST: &str = "api.github.com";
const API_PATH: &str = "/repos/MatthewSaintBull/freenapse/releases/latest";

/// `major.minor.patch` of a version or a `v`-prefixed tag; anything after the
/// patch number (a pre-release suffix) is ignored.
pub fn parse_version(s: &str) -> Option<(u32, u32, u32)> {
    let s = s.trim().trim_start_matches(['v', 'V']);
    let mut parts = s.split('.');
    let num = |p: Option<&str>| p.and_then(|p| p.chars().take_while(|c| c.is_ascii_digit()).collect::<String>().parse::<u32>().ok());
    let major = num(parts.next())?;
    let minor = num(parts.next()).unwrap_or(0);
    let patch = num(parts.next()).unwrap_or(0);
    Some((major, minor, patch))
}

/// True when `latest` is a higher version than `current`.
pub fn is_newer(latest: &str, current: &str) -> bool {
    match (parse_version(latest), parse_version(current)) {
        (Some(l), Some(c)) => l > c,
        _ => false,
    }
}

/// The `tag_name` of a GitHub release JSON object, without a JSON parser:
/// the API writes the key as `"tag_name":"v0.2.0"` and tags carry no escapes.
pub fn tag_from_json(body: &str) -> Option<String> {
    let i = body.find("\"tag_name\"")?;
    let rest = &body[i + "\"tag_name\"".len()..];
    let rest = rest.trim_start().strip_prefix(':')?.trim_start().strip_prefix('"')?;
    let end = rest.find('"')?;
    let tag = &rest[..end];
    (!tag.is_empty() && !tag.contains('\\')).then(|| tag.to_string())
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

struct Handle(*mut c_void);

impl Drop for Handle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { WinHttpCloseHandle(self.0) };
        }
    }
}

/// The latest release tag on GitHub, e.g. `v0.3.0`. Blocks for up to about
/// fifteen seconds; call it from a thread of its own.
pub fn fetch_latest_tag() -> Result<String, String> {
    unsafe {
        let agent = wide(&format!("freenapse/{}", crate::VERSION));
        let session = Handle(WinHttpOpen(agent.as_ptr(), WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, ptr::null(), ptr::null(), 0));
        if session.0.is_null() {
            return Err("WinHttpOpen failed".into());
        }
        WinHttpSetTimeouts(session.0, 5000, 5000, 5000, 10000);
        let host = wide(API_HOST);
        let conn = Handle(WinHttpConnect(session.0, host.as_ptr(), INTERNET_DEFAULT_HTTPS_PORT, 0));
        if conn.0.is_null() {
            return Err("WinHttpConnect failed".into());
        }
        let (verb, path) = (wide("GET"), wide(API_PATH));
        let req = Handle(WinHttpOpenRequest(conn.0, verb.as_ptr(), path.as_ptr(), ptr::null(), ptr::null(), ptr::null(), WINHTTP_FLAG_SECURE));
        if req.0.is_null() {
            return Err("WinHttpOpenRequest failed".into());
        }
        let headers = wide("Accept: application/vnd.github+json\r\nX-GitHub-Api-Version: 2022-11-28\r\n");
        if WinHttpSendRequest(req.0, headers.as_ptr(), u32::MAX, ptr::null(), 0, 0, 0) == 0 {
            return Err("no connection".into());
        }
        if WinHttpReceiveResponse(req.0, ptr::null_mut()) == 0 {
            return Err("no response".into());
        }
        let mut status = 0u32;
        let mut size = std::mem::size_of::<u32>() as u32;
        WinHttpQueryHeaders(
            req.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            ptr::null(),
            &mut status as *mut u32 as *mut c_void,
            &mut size,
            ptr::null_mut(),
        );
        if status != 200 {
            return Err(format!("GitHub answered HTTP {status}"));
        }
        let mut body = Vec::new();
        loop {
            let mut avail = 0u32;
            if WinHttpQueryDataAvailable(req.0, &mut avail) == 0 || avail == 0 {
                break;
            }
            let start = body.len();
            body.resize(start + avail as usize, 0);
            let mut read = 0u32;
            if WinHttpReadData(req.0, body[start..].as_mut_ptr() as *mut c_void, avail, &mut read) == 0 {
                break;
            }
            body.truncate(start + read as usize);
            if body.len() > 1 << 20 {
                break;
            }
        }
        tag_from_json(&String::from_utf8_lossy(&body)).ok_or_else(|| "no tag_name in the answer".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert_eq!(parse_version("v0.2.0"), Some((0, 2, 0)));
        assert_eq!(parse_version("1.10.3-beta"), Some((1, 10, 3)));
        assert_eq!(parse_version("v2"), Some((2, 0, 0)));
        assert_eq!(parse_version("latest"), None);
        assert!(is_newer("v0.3.0", "0.2.0"));
        assert!(is_newer("v0.2.1", "0.2.0"));
        assert!(is_newer("v1.0.0", "0.9.9"));
        assert!(!is_newer("v0.2.0", "0.2.0"));
        assert!(!is_newer("v0.1.9", "0.2.0"));
        assert!(!is_newer("garbage", "0.2.0"));
    }

    #[test]
    fn tag_extraction() {
        let body = r#"{"url":"https://api.github.com/x","tag_name":"v0.2.0","name":"v0.2.0","draft":false}"#;
        assert_eq!(tag_from_json(body).as_deref(), Some("v0.2.0"));
        assert_eq!(tag_from_json(r#"{ "tag_name" : "v1.2.3" }"#).as_deref(), Some("v1.2.3"));
        assert_eq!(tag_from_json(r#"{"message":"Not Found"}"#), None);
        assert_eq!(tag_from_json(r#"{"tag_name":""}"#), None);
    }
}
