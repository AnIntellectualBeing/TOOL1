//! Windows Firewall: what PhoneMirror needs, how to check it, and the script
//! that fixes it. The checks use PowerShell's NetSecurity cmdlets because
//! their JSON output isn't translated (netsh's text output is).
//!
//! What we allow, in plain words:
//! * `uxplay.exe` may receive connections from devices on your local network
//!   (iPhones find it via mDNS on UDP 5353, then send the picture and sound to
//!   TCP/UDP 7100–7102). Only on Private and Domain networks unless you choose
//!   "Allow on this network" for a Public one.
//! * `phonemirror.exe` may receive mDNS answers (UDP 5353) so it can find
//!   Android phones for wireless pairing.
//!
//! Nothing is opened to the internet: home routers don't forward these ports.

use serde::Deserialize;

pub const UXPLAY_RULE: &str = "PhoneMirror - iPhone receiver (UxPlay)";
pub const APP_RULE: &str = "PhoneMirror - find Android phones (mDNS)";

const UXPLAY_DESC: &str = "Lets iPhones and iPads on your local network find this PC and send their screen and sound to PhoneMirror.";
const APP_DESC: &str =
    "Lets PhoneMirror find Android phones on your local network for wireless pairing.";

/// Quote a string for a PowerShell single-quoted literal.
pub fn ps_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkCategory {
    Public,
    Private,
    Domain,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkProfile {
    pub interface: String,
    pub name: String,
    pub category: NetworkCategory,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RawProfile {
    #[serde(default)]
    interface_alias: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    network_category: serde_json::Value,
}

/// PowerShell 5 serialises enums as numbers, PowerShell 7 may use names.
fn category(v: &serde_json::Value) -> NetworkCategory {
    match v {
        serde_json::Value::Number(n) => match n.as_i64() {
            Some(0) => NetworkCategory::Public,
            Some(1) => NetworkCategory::Private,
            Some(2) => NetworkCategory::Domain,
            _ => NetworkCategory::Unknown,
        },
        serde_json::Value::String(s) => match s.as_str() {
            "Public" => NetworkCategory::Public,
            "Private" => NetworkCategory::Private,
            "DomainAuthenticated" => NetworkCategory::Domain,
            _ => NetworkCategory::Unknown,
        },
        _ => NetworkCategory::Unknown,
    }
}

/// `ConvertTo-Json` gives an object for one item and an array for several.
fn one_or_many<T: for<'de> Deserialize<'de>>(json: &str) -> Vec<T> {
    let json = json.trim();
    if json.is_empty() || json == "null" {
        return vec![];
    }
    if let Ok(v) = serde_json::from_str::<Vec<T>>(json) {
        return v;
    }
    serde_json::from_str::<T>(json)
        .map(|x| vec![x])
        .unwrap_or_default()
}

pub const NETWORK_PROFILE_SCRIPT: &str =
    "Get-NetConnectionProfile | Select-Object InterfaceAlias,Name,NetworkCategory | ConvertTo-Json -Compress";

pub fn parse_network_profiles(json: &str) -> Vec<NetworkProfile> {
    one_or_many::<RawProfile>(json)
        .into_iter()
        .map(|r| NetworkProfile {
            interface: r.interface_alias.unwrap_or_default(),
            name: r.name.unwrap_or_default(),
            category: category(&r.network_category),
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RuleSummary {
    /// An enabled inbound Allow rule covers Private networks.
    pub allows_private: bool,
    pub allows_public: bool,
    /// Enabled inbound Block rules exist (Windows makes these when someone
    /// clicks "Cancel" on its firewall prompt). Block beats Allow.
    pub blocked: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RawRule {
    #[serde(default)]
    enabled: serde_json::Value,
    #[serde(default)]
    profile: serde_json::Value,
    #[serde(default)]
    direction: serde_json::Value,
    #[serde(default)]
    action: serde_json::Value,
}

/// Script printing the firewall rules that apply to `program` as JSON.
pub fn rules_for_program_script(program: &str) -> String {
    format!(
        "$r = @(Get-NetFirewallApplicationFilter -Program {} -ErrorAction SilentlyContinue | Get-NetFirewallRule -ErrorAction SilentlyContinue | Select-Object Enabled,Profile,Direction,Action); ConvertTo-Json -InputObject $r -Compress",
        ps_quote(program)
    )
}

fn is_true(v: &serde_json::Value) -> bool {
    // GpoBoolean: True = 1, False = 2.
    matches!(v, serde_json::Value::Number(n) if n.as_i64() == Some(1))
        || matches!(v, serde_json::Value::String(s) if s == "True")
        || matches!(v, serde_json::Value::Bool(true))
}

fn profile_bits(v: &serde_json::Value) -> i64 {
    match v {
        serde_json::Value::Number(n) => n.as_i64().unwrap_or(0),
        serde_json::Value::String(s) => {
            if s == "Any" {
                return 0;
            }
            s.split(',').fold(0, |acc, p| {
                acc | match p.trim() {
                    "Domain" => 1,
                    "Private" => 2,
                    "Public" => 4,
                    _ => 0,
                }
            })
        }
        _ => 0,
    }
}

fn eq_enum(v: &serde_json::Value, num: i64, name: &str) -> bool {
    match v {
        serde_json::Value::Number(n) => n.as_i64() == Some(num),
        serde_json::Value::String(s) => s == name,
        _ => false,
    }
}

pub fn summarize_rules(json: &str) -> RuleSummary {
    let mut s = RuleSummary::default();
    for r in one_or_many::<RawRule>(json) {
        if !is_true(&r.enabled) || !eq_enum(&r.direction, 1, "Inbound") {
            continue;
        }
        let bits = profile_bits(&r.profile);
        let covers = |b: i64| bits == 0 || bits & b != 0;
        if eq_enum(&r.action, 2, "Allow") {
            s.allows_private |= covers(2);
            s.allows_public |= covers(4);
        } else if eq_enum(&r.action, 4, "Block") {
            s.blocked = true;
        }
    }
    s
}

/// The elevated script behind "Fix firewall". Idempotent: removes Windows'
/// automatic Block rules for our programs, then (re)creates our Allow rules.
pub fn fix_script(uxplay: &str, app: &str, include_public: bool) -> String {
    let profiles = if include_public {
        "Private,Domain,Public"
    } else {
        "Private,Domain"
    };
    let mut s = String::from("$ErrorActionPreference = 'Continue'\n");
    for program in [uxplay, app] {
        s += &format!(
            "Get-NetFirewallApplicationFilter -Program {} -ErrorAction SilentlyContinue | Get-NetFirewallRule -ErrorAction SilentlyContinue | Where-Object {{ $_.Action -eq 'Block' }} | Remove-NetFirewallRule\n",
            ps_quote(program)
        );
    }
    s += &format!(
        "Remove-NetFirewallRule -DisplayName {} -ErrorAction SilentlyContinue\n",
        ps_quote(UXPLAY_RULE)
    );
    s += &format!(
        "Remove-NetFirewallRule -DisplayName {} -ErrorAction SilentlyContinue\n",
        ps_quote(APP_RULE)
    );
    s += &format!(
        "New-NetFirewallRule -DisplayName {} -Description {} -Direction Inbound -Action Allow -Program {} -Profile {profiles} | Out-Null\n",
        ps_quote(UXPLAY_RULE),
        ps_quote(UXPLAY_DESC),
        ps_quote(uxplay)
    );
    s += &format!(
        "New-NetFirewallRule -DisplayName {} -Description {} -Direction Inbound -Action Allow -Program {} -Protocol UDP -LocalPort 5353 -Profile {profiles} | Out-Null\n",
        ps_quote(APP_RULE),
        ps_quote(APP_DESC),
        ps_quote(app)
    );
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoting() {
        assert_eq!(
            ps_quote(r"C:\Program Files\PhoneMirror\uxplay.exe"),
            r"'C:\Program Files\PhoneMirror\uxplay.exe'"
        );
        assert_eq!(ps_quote("O'Brien"), "'O''Brien'");
    }

    #[test]
    fn profiles_ps5_numbers() {
        let p = parse_network_profiles(
            r#"{"InterfaceAlias":"Wi-Fi","Name":"HomeNet","NetworkCategory":0}"#,
        );
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].category, NetworkCategory::Public);
        assert_eq!(p[0].name, "HomeNet");
    }

    #[test]
    fn profiles_ps7_strings_array() {
        let p = parse_network_profiles(
            r#"[{"InterfaceAlias":"Wi-Fi","Name":"A","NetworkCategory":"Private"},{"InterfaceAlias":"Ethernet","Name":"corp","NetworkCategory":"DomainAuthenticated"}]"#,
        );
        assert_eq!(p[0].category, NetworkCategory::Private);
        assert_eq!(p[1].category, NetworkCategory::Domain);
        assert!(parse_network_profiles("").is_empty());
    }

    #[test]
    fn rule_summary() {
        // Private+Domain allow (bits 3), enabled.
        let s = summarize_rules(r#"[{"Enabled":1,"Profile":3,"Direction":1,"Action":2}]"#);
        assert!(s.allows_private && !s.allows_public && !s.blocked);
        // Windows' auto "Cancel" block rule on Public.
        let s = summarize_rules(
            r#"[{"Enabled":1,"Profile":4,"Direction":1,"Action":4},{"Enabled":1,"Profile":0,"Direction":1,"Action":2}]"#,
        );
        assert!(s.blocked && s.allows_public && s.allows_private);
        // Disabled and outbound rules don't count.
        let s = summarize_rules(
            r#"[{"Enabled":2,"Profile":2,"Direction":1,"Action":2},{"Enabled":1,"Profile":2,"Direction":2,"Action":2}]"#,
        );
        assert_eq!(s, RuleSummary::default());
        // PS7 strings, single object.
        let s = summarize_rules(
            r#"{"Enabled":"True","Profile":"Private, Public","Direction":"Inbound","Action":"Allow"}"#,
        );
        assert!(s.allows_private && s.allows_public);
        assert_eq!(summarize_rules("null"), RuleSummary::default());
    }

    #[test]
    fn fix_script_contents() {
        let s = fix_script(r"C:\P\uxplay.exe", r"C:\P\phonemirror.exe", false);
        assert!(s.contains("-Program 'C:\\P\\uxplay.exe' -Profile Private,Domain"));
        assert!(s.contains("-Protocol UDP -LocalPort 5353"));
        assert!(s.contains("Where-Object { $_.Action -eq 'Block' }"));
        assert!(!s.contains("Public"));
        assert!(fix_script("a", "b", true).contains("Private,Domain,Public"));
    }
}
