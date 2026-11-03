//! Turning other password managers' exports into PassVault accounts.
//!
//! One tolerant CSV reader covers Firefox, Chrome/Edge/Brave/Opera, Google
//! Password Manager, Samsung Internet (via Google), Bitwarden, 1Password,
//! LastPass, KeePass/KeePassXC, Apple Passwords, Proton Pass, Dashlane and
//! most others: we match header names instead of hard-coding layouts. On
//! Linux the `browsers` module can also read Firefox / Chromium profiles
//! directly, producing the same `Candidate`s.

use crate::vault::Account;
use serde::Serialize;

#[derive(Clone, Debug, Default)]
pub struct Candidate {
    pub name: String,
    pub username: String,
    pub password: String,
    pub url: String,
    pub notes: String,
}

#[derive(Serialize, Clone, Copy, PartialEq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    New,
    Duplicate,
    Changed,
}

#[derive(Serialize)]
pub struct PreviewItem {
    pub index: usize,
    pub name: String,
    pub username: String,
    pub url: String,
    pub status: Status,
}

#[derive(Serialize)]
pub struct Preview {
    pub source: String,
    pub items: Vec<PreviewItem>,
    pub new_count: usize,
    pub duplicate_count: usize,
    pub changed_count: usize,
}

const NAME: &[&str] = &["name", "title", "item name", "account", "account name"];
const URL: &[&str] = &[
    "url", "login_uri", "website", "web site", "uri", "login url", "hostname", "origin", "origin_url",
    "urls", "web address",
];
const USER: &[&str] = &[
    "username", "login_username", "user name", "login", "login name", "user", "email", "e-mail",
    "username1", "email address",
];
const PASS: &[&str] = &["password", "login_password", "pass", "passwd"];
const NOTES: &[&str] = &["notes", "note", "extra", "comments", "comment", "description"];
const TOTP: &[&str] = &["totp", "login_totp", "otpauth", "otp", "otpsecret", "one-time password", "otp secret"];
const TYPE: &[&str] = &["type", "item type"];

fn norm_header(h: &str) -> String {
    h.trim().trim_start_matches('\u{feff}').to_lowercase()
}

pub fn parse_csv(bytes: &[u8]) -> Result<Vec<Candidate>, String> {
    let text = String::from_utf8_lossy(bytes);
    let mut rdr = csv::ReaderBuilder::new()
        .flexible(true)
        .has_headers(true)
        .from_reader(text.as_bytes());
    let headers: Vec<String> = rdr
        .headers()
        .map_err(|_| "That file isn't a readable CSV.".to_string())?
        .iter()
        .map(norm_header)
        .collect();
    let cols = |names: &[&str]| -> Vec<usize> {
        // Preference order follows `names`, not column order.
        names
            .iter()
            .filter_map(|n| headers.iter().position(|h| h == n))
            .collect()
    };
    let (c_name, c_url, c_user, c_pass, c_notes, c_totp, c_type) =
        (cols(NAME), cols(URL), cols(USER), cols(PASS), cols(NOTES), cols(TOTP), cols(TYPE));
    if c_pass.is_empty() {
        return Err(
            "Couldn't find a password column. Export \"Passwords (CSV)\" from your browser or password manager and pick that file."
                .into(),
        );
    }

    let mut out = Vec::new();
    for rec in rdr.records() {
        let Ok(rec) = rec else { continue };
        let pick = |cs: &[usize]| -> String {
            cs.iter()
                .filter_map(|&i| rec.get(i))
                .map(|v| v.trim())
                .find(|v| !v.is_empty())
                .unwrap_or("")
                .to_string()
        };
        let kind = pick(&c_type).to_lowercase();
        if !kind.is_empty() && kind != "login" && kind != "password" && kind != "1" {
            continue; // Bitwarden/Proton cards, notes, identities...
        }
        let password = pick(&c_pass);
        let username = pick(&c_user);
        if password.is_empty() && username.is_empty() {
            continue;
        }
        let mut url = pick(&c_url);
        // 1Password / Bitwarden can pack several URLs in one cell.
        if let Some(first) = url.split([',', '\n']).next() {
            url = first.trim().to_string();
        }
        let mut notes = pick(&c_notes);
        let totp = pick(&c_totp);
        if !totp.is_empty() {
            if !notes.is_empty() {
                notes.push_str("\n\n");
            }
            notes.push_str("2FA / TOTP: ");
            notes.push_str(&totp);
        }
        let mut name = pick(&c_name);
        if name.is_empty() {
            name = display_name_for(&url);
        }
        if name.is_empty() {
            name = username.clone();
        }
        out.push(Candidate {
            name,
            username,
            password,
            url,
            notes,
        });
    }
    if out.is_empty() {
        return Err("No logins found in that file.".into());
    }
    Ok(out)
}

/// "https://www.github.com/login" -> "github.com"; Android app entries
/// ("android://hash@com.instagram.android/") -> "com.instagram.android".
pub fn host_of(url: &str) -> String {
    let u = url.trim();
    if u.is_empty() {
        return String::new();
    }
    let parsed = url::Url::parse(u).or_else(|_| url::Url::parse(&format!("https://{u}")));
    match parsed {
        Ok(p) => p
            .host_str()
            .unwrap_or("")
            .trim_start_matches("www.")
            .trim_start_matches("m.")
            .to_lowercase(),
        Err(_) => u.to_lowercase(),
    }
}

pub fn display_name_for(url: &str) -> String {
    let host = host_of(url);
    if host.starts_with("com.") || host.starts_with("org.") || host.starts_with("net.") {
        // Android package: show the most meaningful segment.
        return host
            .split('.')
            .filter(|s| !matches!(*s, "com" | "org" | "net" | "android" | "app" | "mobile"))
            .next()
            .map(capitalize)
            .unwrap_or(host);
    }
    host
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

fn key_of(name: &str, url: &str, username: &str) -> (String, String) {
    let site = {
        let h = host_of(url);
        if h.is_empty() {
            name.trim().to_lowercase()
        } else {
            h
        }
    };
    (site, username.trim().to_lowercase())
}

pub fn classify(cands: &[Candidate], existing: &[Account]) -> Vec<(Status, Option<String>)> {
    use std::collections::HashMap;
    let mut by_key: HashMap<(String, String), &Account> = HashMap::new();
    for a in existing {
        by_key.insert(key_of(&a.name, &a.url, &a.username), a);
    }
    let mut seen: HashMap<(String, String), String> = HashMap::new();
    cands
        .iter()
        .map(|c| {
            let k = key_of(&c.name, &c.url, &c.username);
            if let Some(a) = by_key.get(&k) {
                if a.password == c.password {
                    (Status::Duplicate, Some(a.id.clone()))
                } else {
                    (Status::Changed, Some(a.id.clone()))
                }
            } else if let Some(prev_pw) = seen.get(&k) {
                // Same login twice in one export (e.g. Firefox + http/https).
                if *prev_pw == c.password {
                    (Status::Duplicate, None)
                } else {
                    (Status::New, None)
                }
            } else {
                seen.insert(k, c.password.clone());
                (Status::New, None)
            }
        })
        .collect()
}

pub fn preview(source: &str, cands: &[Candidate], existing: &[Account]) -> Preview {
    let cls = classify(cands, existing);
    let items: Vec<PreviewItem> = cands
        .iter()
        .zip(cls.iter())
        .enumerate()
        .map(|(index, (c, (st, _)))| PreviewItem {
            index,
            name: c.name.clone(),
            username: c.username.clone(),
            url: c.url.clone(),
            status: *st,
        })
        .collect();
    let count = |s: Status| items.iter().filter(|i| i.status == s).count();
    Preview {
        source: source.to_string(),
        new_count: count(Status::New),
        duplicate_count: count(Status::Duplicate),
        changed_count: count(Status::Changed),
        items,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn firefox_csv() {
        let csv = "\"url\",\"username\",\"password\",\"httpRealm\",\"formActionOrigin\",\"guid\",\"timeCreated\",\"timeLastUsed\",\"timePasswordChanged\"\n\"https://www.github.com\",\"leo\",\"pw1\",,\"https://github.com\",\"{x}\",\"1\",\"1\",\"1\"\n";
        let c = parse_csv(csv.as_bytes()).unwrap();
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].name, "github.com");
        assert_eq!(c[0].username, "leo");
        assert_eq!(c[0].password, "pw1");
    }

    #[test]
    fn chrome_and_bitwarden_csv() {
        let chrome = "\u{feff}name,url,username,password,note\nGitHub,https://github.com/,a,b,hi\n";
        let c = parse_csv(chrome.as_bytes()).unwrap();
        assert_eq!((c[0].name.as_str(), c[0].notes.as_str()), ("GitHub", "hi"));

        let bw = "folder,favorite,type,name,notes,fields,reprompt,login_uri,login_username,login_password,login_totp\n,,login,Insta,,,0,https://instagram.com,me,pw,JBSWY3DP\n,,note,Secret note,text,,0,,,,\n";
        let c = parse_csv(bw.as_bytes()).unwrap();
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].url, "https://instagram.com");
        assert!(c[0].notes.contains("JBSWY3DP"));
    }

    #[test]
    fn proton_email_fallback_and_android_names() {
        let csv = "type,name,url,email,username,password,note,totp\nlogin,,android://x@com.instagram.android/,me@x.com,,pw,,\n";
        let c = parse_csv(csv.as_bytes()).unwrap();
        assert_eq!(c[0].username, "me@x.com");
        assert_eq!(c[0].name, "Instagram");
    }

    #[test]
    fn dedupe() {
        let existing = vec![Account {
            id: "1".into(),
            name: "GitHub".into(),
            username: "Leo".into(),
            password: "same".into(),
            url: "https://github.com".into(),
            notes: String::new(),
            created_at: 0,
            updated_at: 0,
            favorite: false,
            tags: vec![],
        }];
        let c = |u: &str, p: &str| Candidate {
            name: "x".into(),
            username: "leo".into(),
            password: p.into(),
            url: u.into(),
            notes: String::new(),
        };
        let cls = classify(
            &[c("https://www.github.com/", "same"), c("github.com", "other"), c("https://gitlab.com", "z"), c("gitlab.com", "z")],
            &existing,
        );
        let st: Vec<Status> = cls.iter().map(|x| x.0).collect();
        assert_eq!(st, vec![Status::Duplicate, Status::Changed, Status::New, Status::Duplicate]);
    }
}
