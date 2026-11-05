//! Windows Firewall check and fix (see `phonemirror_core::firewall`).

use crate::state::State;
use phonemirror_core::Problem;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, Default)]
pub struct FirewallStatus {
    /// False on systems where PhoneMirror doesn't manage the firewall.
    pub supported: bool,
    pub receiver_allowed: bool,
    pub allowed_on_public: bool,
    pub blocked: bool,
    pub on_public_network: bool,
    pub networks: Vec<String>,
}

#[cfg(windows)]
async fn powershell(script: &str) -> Result<String, String> {
    let mut c = crate::proc::command(std::path::Path::new("powershell.exe"));
    c.args([
        "-NoProfile",
        "-NonInteractive",
        "-ExecutionPolicy",
        "Bypass",
        "-Command",
        script,
    ]);
    let out = tokio::time::timeout(std::time::Duration::from_secs(20), c.output())
        .await
        .map_err(|_| "timed out".to_string())?
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).to_string())
    }
}

#[cfg(windows)]
pub async fn status(st: &State) -> FirewallStatus {
    use phonemirror_core::firewall::*;
    let uxplay = st.paths.uxplay.display().to_string();
    let rules = powershell(&rules_for_program_script(&uxplay))
        .await
        .unwrap_or_default();
    let summary = summarize_rules(&rules);
    let profiles =
        parse_network_profiles(&powershell(NETWORK_PROFILE_SCRIPT).await.unwrap_or_default());
    let on_public = profiles
        .iter()
        .any(|p| p.category == NetworkCategory::Public);
    FirewallStatus {
        supported: true,
        receiver_allowed: if on_public {
            summary.allows_public
        } else {
            summary.allows_private
        },
        allowed_on_public: summary.allows_public,
        blocked: summary.blocked,
        on_public_network: on_public,
        networks: profiles
            .iter()
            .map(|p| {
                let cat = match p.category {
                    NetworkCategory::Public => "Public",
                    NetworkCategory::Private => "Private",
                    NetworkCategory::Domain => "Work (domain)",
                    NetworkCategory::Unknown => "Unknown",
                };
                format!("{} ({}): {cat}", p.name, p.interface)
            })
            .collect(),
    }
}

#[cfg(not(windows))]
pub async fn status(_st: &State) -> FirewallStatus {
    FirewallStatus::default()
}

/// Runs the fix script elevated; Windows shows one permission prompt.
#[cfg(windows)]
pub async fn fix(st: &State, include_public: bool) -> Result<FirewallStatus, Problem> {
    use phonemirror_core::firewall::{fix_script, ps_quote};
    let script = fix_script(
        &st.paths.uxplay.display().to_string(),
        &st.paths.app_exe.display().to_string(),
        include_public,
    );
    let file = st.paths.data.join("fix-firewall.ps1");
    // UTF-8 with BOM so Windows PowerShell 5 reads non-ASCII paths correctly.
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(script.as_bytes());
    std::fs::write(&file, bytes).map_err(|e| Problem::internal(e.to_string()))?;
    let launcher = format!(
        "$p = Start-Process -FilePath powershell.exe -Verb RunAs -Wait -PassThru -WindowStyle Hidden -ArgumentList @('-NoProfile','-ExecutionPolicy','Bypass','-File',{}); exit $p.ExitCode",
        ps_quote(&format!("\"{}\"", file.display()))
    );
    let result = powershell(&launcher).await;
    let _ = std::fs::remove_file(&file);
    if let Err(e) = result {
        return Err(Problem::new("firewall_not_changed", "The firewall wasn't changed.")
            .hint("Windows asks for permission to change firewall settings. Press \"Fix firewall\" again and choose Yes.")
            .details(e));
    }
    st.toast(
        "success",
        "Firewall updated. iPhones on this network can now find your PC.",
        None,
    );
    Ok(status(st).await)
}

#[cfg(not(windows))]
pub async fn fix(_st: &State, _include_public: bool) -> Result<FirewallStatus, Problem> {
    Err(Problem::new(
        "unsupported",
        "PhoneMirror doesn't manage the firewall on this system.",
    )
    .hint("Allow UDP 5353 and TCP/UDP 7100–7102 for uxplay in your firewall."))
}
