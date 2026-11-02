//! The built-in Provider list: what it holds, its schema checks, the picker
//! labels and the search.

use agent_mail::mandate::TlsMode;
use agent_mail::provider::{self, CredentialKind, NOTE_MAX, Provider, REASON_MAX};
use agent_mail::terminal::SearchOption;
use serde_json::{Value, json};

fn repo_file(path: &str) -> String {
    let full = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(path);
    std::fs::read_to_string(&full).unwrap_or_else(|e| panic!("{}: {e}", full.display()))
}

#[test]
fn ac2_the_list_is_built_in_and_says_why() {
    let source = repo_file("src/provider.rs");
    assert!(source.contains("include_str!(\"../data/providers.json\")"));
    for read in ["std::fs", "fs::read", "File::open", "env::var", "TcpStream"] {
        assert!(
            !source.contains(read),
            "provider.rs reads at runtime: {read}"
        );
    }
    let comment: String = source
        .lines()
        .take_while(|l| !l.contains("include_str!"))
        .filter(|l| l.trim_start().starts_with("///"))
        .collect::<Vec<_>>()
        .join(" ");
    assert!(comment.contains("read"), "{comment}");
    assert!(comment.contains("runtime"), "{comment}");
    assert!(
        comment.contains("Credential") && comment.contains("another server"),
        "{comment}"
    );
}

#[test]
fn ac2_all_returns_the_30_supported_providers_in_file_order() {
    let starting: Value =
        serde_json::from_str(&repo_file(".scratch/provider-list/providers.json")).unwrap();
    let supported: Vec<&str> = starting["providers"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| p.get("unsupported").is_none())
        .map(|p| p["id"].as_str().unwrap())
        .collect();
    assert_eq!(supported.len(), 30);
    let ids: Vec<&str> = provider::all()
        .iter()
        .filter(|p| p.unsupported.is_none())
        .map(|p| p.id.as_str())
        .collect();
    assert_eq!(ids, supported);
}

const UNSUPPORTED: [&str; 5] = ["outlook", "microsoft365", "proton", "tuta", "hey"];

fn starting_list() -> Vec<Value> {
    let starting: Value =
        serde_json::from_str(&repo_file(".scratch/provider-list/providers.json")).unwrap();
    starting["providers"].as_array().unwrap().clone()
}

#[test]
fn job032_ac1_all_returns_the_35_providers_in_file_order() {
    let starting = starting_list();
    let expected: Vec<&str> = starting.iter().map(|p| p["id"].as_str().unwrap()).collect();
    let ids: Vec<&str> = provider::all().iter().map(|p| p.id.as_str()).collect();
    assert_eq!(ids.len(), 35);
    assert_eq!(ids, expected);
    assert_eq!(ids[30..], UNSUPPORTED);
}

#[test]
fn job032_ac1_unsupported_providers_carry_the_starting_reason_only() {
    let starting = starting_list();
    for id in UNSUPPORTED {
        let raw = starting.iter().find(|p| p["id"] == id).unwrap();
        let p = by_id(id);
        assert_eq!(
            p.unsupported.as_deref(),
            Some(raw["unsupported"].as_str().unwrap()),
            "{id}"
        );
        assert_eq!(p.imap, None, "{id}");
        assert_eq!(p.credential, None, "{id}");
        assert!(p.connection().is_none(), "{id}");
    }
    assert_eq!(
        by_id("hey").unsupported.as_deref(),
        Some("HEY provides no IMAP access.")
    );
    let supported: Vec<&Provider> = provider::all()
        .iter()
        .filter(|p| !UNSUPPORTED.contains(&p.id.as_str()))
        .collect();
    assert_eq!(supported.len(), 30);
    for p in supported {
        assert_eq!(p.unsupported, None, "{}", p.id);
    }
}

fn is_slug(id: &str) -> bool {
    !id.is_empty()
        && id.split('-').all(|part| {
            !part.is_empty()
                && part
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        })
}

#[test]
fn ac3_the_built_in_list_follows_the_schema() {
    let all = provider::all();
    let mut ids = std::collections::HashSet::new();
    let mut domains = std::collections::HashSet::new();
    for p in all {
        assert!(is_slug(&p.id), "{}", p.id);
        assert!(ids.insert(p.id.clone()), "duplicate id {}", p.id);
        for d in &p.domains {
            assert!(domains.insert(d.to_lowercase()), "duplicate domain {d}");
        }
        match &p.unsupported {
            None => {
                let imap = p
                    .imap
                    .as_ref()
                    .expect("supported Provider has IMAP settings");
                assert!(!imap.host.is_empty());
                assert_ne!(imap.port, 0);
                assert!(matches!(imap.tls, TlsMode::Implicit | TlsMode::Starttls));
                assert!(matches!(
                    p.credential,
                    Some(CredentialKind::Password | CredentialKind::AppPassword)
                ));
            }
            Some(reason) => {
                assert!(p.imap.is_none(), "{}", p.id);
                assert!(p.credential.is_none(), "{}", p.id);
                assert!(!reason.trim().is_empty(), "{}", p.id);
                assert!(reason.chars().count() <= REASON_MAX, "{}", p.id);
            }
        }
        if let Some(note) = &p.note {
            assert!(note.chars().count() <= NOTE_MAX, "{}", p.id);
        }
    }
    let raw = repo_file("data/providers.json");
    assert_eq!(provider::parse(&raw).unwrap(), all);
}

fn entry(id: &str, domains: &[&str]) -> Value {
    json!({
        "id": id,
        "name": id,
        "domains": domains,
        "imap": { "host": "imap.example.test", "port": 993, "tls": "implicit" },
        "credential": "password",
    })
}

fn list(entries: Vec<Value>) -> String {
    json!({ "providers": entries }).to_string()
}

#[test]
fn ac3_the_parser_rejects_each_broken_rule() {
    let good = list(vec![entry("a", &["a.test"]), entry("b", &["b.test"])]);
    assert_eq!(provider::parse(&good).unwrap().len(), 2);

    let with = |change: &dyn Fn(&mut Value)| {
        let mut second = entry("b", &["b.test"]);
        change(&mut second);
        list(vec![entry("a", &["a.test"]), second])
    };
    let bad = [
        ("duplicate id", with(&|e| e["id"] = json!("a"))),
        (
            "domain twice",
            list(vec![entry("a", &["gmx.de"]), entry("b", &["GMX.de"])]),
        ),
        ("port 0", with(&|e| e["imap"]["port"] = json!(0))),
        (
            "missing imap",
            with(&|e| {
                e.as_object_mut().unwrap().remove("imap");
            }),
        ),
        (
            "missing credential",
            with(&|e| {
                e.as_object_mut().unwrap().remove("credential");
            }),
        ),
        (
            "long note",
            with(&|e| e["note"] = json!("n".repeat(NOTE_MAX + 1))),
        ),
        ("plain tls", with(&|e| e["imap"]["tls"] = json!("plain"))),
        ("uppercase id", with(&|e| e["id"] = json!("GMX"))),
    ];
    for (rule, text) in bad {
        assert!(provider::parse(&text).is_err(), "{rule} was accepted");
    }
    let note = with(&|e| e["note"] = json!("n".repeat(NOTE_MAX)));
    assert!(provider::parse(&note).is_ok());
}

/// `entry` turned unsupported: `reason` set, `imap` and `credential` gone.
fn unsupported(id: &str, reason: &str) -> Value {
    let mut e = entry(id, &[]);
    let object = e.as_object_mut().unwrap();
    object.remove("imap");
    object.remove("credential");
    object.insert("unsupported".to_string(), json!(reason));
    e
}

#[test]
fn job032_ac2_the_parser_enforces_the_unsupported_rules() {
    assert_eq!(REASON_MAX, 300);
    let with = |second: Value| list(vec![entry("a", &["a.test"]), second]);
    let bad = [
        ("unsupported with imap", {
            let mut e = unsupported("b", "No IMAP.");
            e["imap"] = json!({ "host": "imap.example.test", "port": 993, "tls": "implicit" });
            with(e)
        }),
        ("unsupported with credential", {
            let mut e = unsupported("b", "No IMAP.");
            e["credential"] = json!("password");
            with(e)
        }),
        (
            "reason of 301 characters",
            with(unsupported("b", &"r".repeat(REASON_MAX + 1))),
        ),
        ("neither unsupported nor imap", {
            let mut e = entry("b", &["b.test"]);
            e.as_object_mut().unwrap().remove("imap");
            with(e)
        }),
        ("neither unsupported nor credential", {
            let mut e = entry("b", &["b.test"]);
            e.as_object_mut().unwrap().remove("credential");
            with(e)
        }),
    ];
    for (rule, text) in bad {
        assert!(provider::parse(&text).is_err(), "{rule} was accepted");
    }
    let longest = with(unsupported("b", &"r".repeat(REASON_MAX)));
    let parsed = provider::parse(&longest).unwrap();
    assert_eq!(parsed[1].unsupported.as_deref().map(str::len), Some(300));
    assert!(parsed[1].domains.is_empty());
}

fn by_id(id: &str) -> &'static Provider {
    provider::all().iter().find(|p| p.id == id).unwrap()
}

#[test]
fn ac4_labels_show_at_most_three_domains() {
    assert_eq!(by_id("gmail").label(), "Gmail (gmail.com, googlemail.com)");
    assert_eq!(by_id("gmx").label(), "GMX (gmx.de, gmx.net, gmx.at, …)");
    assert_eq!(by_id("webde").label(), "WEB.DE (web.de)");
    assert_eq!(by_id("ionos").label(), "IONOS Mail");
    let three = provider::all()
        .iter()
        .find(|p| p.domains.len() == 3)
        .unwrap();
    assert!(!three.label().contains('…'), "{}", three.label());
}

#[test]
fn ac4_sorted_is_case_insensitive_by_name() {
    let names: Vec<String> = provider::sorted()
        .iter()
        .map(|p| p.name.to_lowercase())
        .collect();
    let mut expected = names.clone();
    expected.sort();
    assert_eq!(names, expected);
    assert_eq!(names.len(), 35);
    let supported = provider::sorted()
        .iter()
        .filter(|p| p.unsupported.is_none())
        .count();
    assert_eq!(supported, 30);
}

fn found(query: &str) -> Vec<&'static str> {
    provider::sorted()
        .into_iter()
        .filter(|p| p.search_option().matches(query))
        .map(|p| p.id.as_str())
        .collect()
}

#[test]
fn ac4_search_finds_by_name_and_every_domain() {
    assert!(found("web.de").contains(&"webde"));
    assert!(found("TELEKOM").contains(&"t-online"));
    assert!(found("posteo.us").contains(&"posteo"));
    assert!(!by_id("posteo").label().contains("posteo.us"));
    assert!(found("ameritech.net").contains(&"att"));
    assert!(!by_id("att").label().contains("ameritech.net"));
    let mut gmx = found("gmx");
    gmx.sort_unstable();
    assert_eq!(gmx, ["gmx", "gmx-com"]);
    assert_eq!(found("").len(), 35);
    let option: SearchOption = by_id("gmx").search_option();
    assert_eq!(option.keys[0], "GMX");
    assert_eq!(option.keys.len(), 1 + by_id("gmx").domains.len());
}

#[test]
fn job032_ac3_unsupported_labels_carry_the_suffix() {
    let expected = [
        (
            "outlook",
            "Outlook.com / Hotmail (outlook.com, hotmail.com, live.com, \u{2026}) \u{2013} not supported",
        ),
        (
            "microsoft365",
            "Microsoft 365 / Exchange Online (work) \u{2013} not supported",
        ),
        (
            "proton",
            "Proton Mail (proton.me, protonmail.com, protonmail.ch, \u{2026}) \u{2013} not supported",
        ),
        (
            "tuta",
            "Tuta (Tutanota) (tuta.com, tutanota.com, tutanota.de, \u{2026}) \u{2013} not supported",
        ),
        ("hey", "HEY (hey.com) \u{2013} not supported"),
    ];
    for (id, label) in expected {
        assert_eq!(by_id(id).label(), label);
        assert_eq!(by_id(id).search_option().label, label);
    }
    assert_eq!(by_id("gmail").label(), "Gmail (gmail.com, googlemail.com)");
    for p in provider::all().iter().filter(|p| p.unsupported.is_none()) {
        assert!(!p.label().contains("not supported"), "{}", p.label());
        assert!(!p.label().contains('\u{2013}'), "{}", p.label());
    }
}

#[test]
fn job032_ac3_search_finds_unsupported_providers_by_name_and_domain() {
    for (query, id) in [
        ("hotmail.de", "outlook"),
        ("microsoft", "microsoft365"),
        ("pm.me", "proton"),
        ("keemail.me", "tuta"),
        ("HEY", "hey"),
    ] {
        assert!(found(query).contains(&id), "{query} does not find {id}");
    }
    assert!(!by_id("outlook").label().contains("hotmail.de"));
    assert!(!by_id("proton").label().contains("pm.me"));
    assert!(!by_id("tuta").label().contains("keemail.me"));
    assert_eq!(found("").len(), 35);
}
