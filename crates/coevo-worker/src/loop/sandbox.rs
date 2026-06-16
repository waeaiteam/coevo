use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SandboxTier {
    ReadOnly,
    WorkspaceWrite,
    FullAccess,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum NetworkPolicy {
    Blocked,
    Proxied { allowed_endpoints: Vec<String> },
    Open,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SandboxProfile {
    pub tier: SandboxTier,
    pub workspace_root: Option<PathBuf>,
    pub network: NetworkPolicy,
    pub readonly_guards: Vec<PathBuf>,
}

impl SandboxProfile {
    pub fn from_tier(tier: SandboxTier, workspace_root: Option<PathBuf>) -> Self {
        match tier {
            SandboxTier::ReadOnly => Self::read_only(workspace_root),
            SandboxTier::WorkspaceWrite => Self::workspace_write(workspace_root),
            SandboxTier::FullAccess => Self {
                tier: SandboxTier::FullAccess,
                workspace_root,
                network: NetworkPolicy::Open,
                readonly_guards: vec![],
            },
        }
    }

    pub fn from_track(track: &str, workspace_root: Option<PathBuf>) -> Self {
        match track {
            "yellow" => Self::workspace_write(workspace_root),
            "red" => Self::read_only(workspace_root),
            _ => Self::read_only(workspace_root),
        }
    }

    pub fn read_only(workspace_root: Option<PathBuf>) -> Self {
        Self {
            tier: SandboxTier::ReadOnly,
            workspace_root,
            network: NetworkPolicy::Blocked,
            readonly_guards: default_readonly_guards(),
        }
    }

    pub fn workspace_write(workspace_root: Option<PathBuf>) -> Self {
        Self {
            tier: SandboxTier::WorkspaceWrite,
            workspace_root,
            network: NetworkPolicy::Proxied {
                allowed_endpoints: vec![],
            },
            readonly_guards: default_readonly_guards(),
        }
    }
}

fn default_readonly_guards() -> Vec<PathBuf> {
    vec![PathBuf::from(".git"), PathBuf::from(".env")]
}

#[cfg(unix)]
type PermissionSnapshot = u32;
#[cfg(not(unix))]
type PermissionSnapshot = bool;

#[derive(Debug)]
pub struct SandboxFilesystemGuard {
    restored: Vec<(PathBuf, PermissionSnapshot)>,
    acl_root: Option<PathBuf>,
}

impl SandboxFilesystemGuard {
    pub fn enter(profile: &SandboxProfile) -> std::io::Result<Self> {
        if profile.tier != SandboxTier::ReadOnly {
            return Ok(Self {
                restored: vec![],
                acl_root: None,
            });
        }
        let Some(root) = profile.workspace_root.as_ref() else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "read-only sandbox requires a workspace_root",
            ));
        };
        let root = std::fs::canonicalize(root)?;
        let mut guard = Self {
            restored: vec![],
            acl_root: None,
        };
        guard.protect_tree(&root)?;
        guard.apply_os_write_deny(&root)?;
        Ok(guard)
    }

    fn protect_tree(&mut self, path: &Path) -> std::io::Result<()> {
        if path.is_file() {
            let perms = std::fs::metadata(path)?.permissions();
            self.restored
                .push((path.to_path_buf(), snapshot_permissions(&perms)));
            return Ok(());
        }
        if path.is_dir() {
            for entry in std::fs::read_dir(path)? {
                self.protect_tree(&entry?.path())?;
            }
            let perms = std::fs::metadata(path)?.permissions();
            self.restored
                .push((path.to_path_buf(), snapshot_permissions(&perms)));
        }
        Ok(())
    }

    fn apply_os_write_deny(&mut self, root: &Path) -> std::io::Result<()> {
        apply_readonly_tree(root)?;
        apply_os_write_deny(root)?;
        self.acl_root = Some(root.to_path_buf());
        Ok(())
    }
}

impl Drop for SandboxFilesystemGuard {
    fn drop(&mut self) {
        if let Some(root) = self.acl_root.take() {
            let _ = remove_os_write_deny(&root);
        }
        for (path, was_readonly) in self.restored.drain(..).rev() {
            if let Ok(metadata) = std::fs::metadata(&path) {
                let mut perms = metadata.permissions();
                restore_permissions(&mut perms, was_readonly);
                let _ = std::fs::set_permissions(path, perms);
            }
        }
    }
}

#[cfg(unix)]
fn snapshot_permissions(perms: &std::fs::Permissions) -> PermissionSnapshot {
    use std::os::unix::fs::PermissionsExt;

    perms.mode()
}

#[cfg(not(unix))]
fn snapshot_permissions(perms: &std::fs::Permissions) -> PermissionSnapshot {
    perms.readonly()
}

#[cfg(unix)]
fn restore_permissions(perms: &mut std::fs::Permissions, snapshot: PermissionSnapshot) {
    use std::os::unix::fs::PermissionsExt;

    perms.set_mode(snapshot);
}

#[cfg(not(unix))]
fn restore_permissions(perms: &mut std::fs::Permissions, snapshot: PermissionSnapshot) {
    perms.set_readonly(snapshot);
}

#[cfg(unix)]
fn apply_readonly_tree(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    if path.is_file() {
        let metadata = std::fs::metadata(path)?;
        let mut perms = metadata.permissions();
        let readonly_mode = perms.mode() & !0o222;
        if readonly_mode != perms.mode() {
            perms.set_mode(readonly_mode);
            std::fs::set_permissions(path, perms)?;
        }
        return Ok(());
    }
    if path.is_dir() {
        for entry in std::fs::read_dir(path)? {
            apply_readonly_tree(&entry?.path())?;
        }
        let metadata = std::fs::metadata(path)?;
        let mut perms = metadata.permissions();
        let readonly_mode = perms.mode() & !0o222;
        if readonly_mode != perms.mode() {
            perms.set_mode(readonly_mode);
            std::fs::set_permissions(path, perms)?;
        }
    }
    Ok(())
}

#[cfg(not(unix))]
fn apply_readonly_tree(path: &Path) -> std::io::Result<()> {
    if path.is_file() {
        let mut perms = std::fs::metadata(path)?.permissions();
        if !perms.readonly() {
            perms.set_readonly(true);
            std::fs::set_permissions(path, perms)?;
        }
        return Ok(());
    }
    if path.is_dir() {
        for entry in std::fs::read_dir(path)? {
            apply_readonly_tree(&entry?.path())?;
        }
        let mut perms = std::fs::metadata(path)?.permissions();
        if !perms.readonly() {
            perms.set_readonly(true);
            std::fs::set_permissions(path, perms)?;
        }
    }
    Ok(())
}

#[cfg(windows)]
fn apply_os_write_deny(root: &Path) -> std::io::Result<()> {
    let status = std::process::Command::new("icacls")
        .arg(root)
        .arg("/deny")
        .arg("*S-1-1-0:(OI)(CI)(WD,AD,DC)")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            format!("icacls deny write failed for {}", root.display()),
        ))
    }
}

#[cfg(windows)]
fn remove_os_write_deny(root: &Path) -> std::io::Result<()> {
    let status = std::process::Command::new("icacls")
        .arg(root)
        .arg("/remove:d")
        .arg("*S-1-1-0")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            format!("icacls remove deny failed for {}", root.display()),
        ))
    }
}

#[cfg(not(windows))]
fn apply_os_write_deny(_root: &Path) -> std::io::Result<()> {
    Ok(())
}

#[cfg(not(windows))]
fn remove_os_write_deny(_root: &Path) -> std::io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn green_maps_to_readonly_sandbox() {
        let profile = SandboxProfile::from_track("green", Some(PathBuf::from("workspace")));
        assert_eq!(profile.tier, SandboxTier::ReadOnly);
        assert_eq!(profile.network, NetworkPolicy::Blocked);
    }

    #[test]
    fn yellow_maps_to_workspace_write_with_empty_proxy_allowlist() {
        let profile = SandboxProfile::from_track("yellow", Some(PathBuf::from("workspace")));
        assert_eq!(profile.tier, SandboxTier::WorkspaceWrite);
        assert_eq!(
            profile.network,
            NetworkPolicy::Proxied {
                allowed_endpoints: vec![]
            }
        );
    }

    #[test]
    fn red_alpha_falls_back_to_readonly_sandbox() {
        let profile = SandboxProfile::from_track("red", None);
        assert_eq!(profile.tier, SandboxTier::ReadOnly);
        assert_eq!(profile.network, NetworkPolicy::Blocked);
    }

    #[test]
    fn explicit_full_access_tier_opens_network_only_when_server_verdict_allows_it() {
        let profile =
            SandboxProfile::from_tier(SandboxTier::FullAccess, Some(PathBuf::from("workspace")));
        assert_eq!(profile.tier, SandboxTier::FullAccess);
        assert_eq!(profile.network, NetworkPolicy::Open);
    }

    #[test]
    fn readonly_blocks_write_at_os_layer() {
        let root =
            std::env::temp_dir().join(format!("coevo-readonly-sandbox-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let protected = root.join("protected.txt");
        std::fs::write(&protected, "original").unwrap();
        let profile = SandboxProfile::read_only(Some(root.clone()));

        let guard = SandboxFilesystemGuard::enter(&profile).unwrap();
        let write_result = std::fs::write(&protected, "mutated");
        let create_result = std::fs::write(root.join("new.txt"), "new");
        drop(guard);
        std::fs::remove_dir_all(&root).ok();

        assert!(write_result.is_err());
        assert!(create_result.is_err());
    }

    #[cfg(unix)]
    #[test]
    fn readonly_guard_restores_exact_unix_modes() {
        use std::os::unix::fs::PermissionsExt;

        let root =
            std::env::temp_dir().join(format!("coevo-readonly-sandbox-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let protected = root.join("protected.sh");
        std::fs::write(&protected, "#!/bin/sh\necho hello\n").unwrap();

        let root_mode = 0o751;
        let file_mode = 0o764;
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(root_mode)).unwrap();
        std::fs::set_permissions(&protected, std::fs::Permissions::from_mode(file_mode)).unwrap();

        let profile = SandboxProfile::read_only(Some(root.clone()));
        let guard = SandboxFilesystemGuard::enter(&profile).unwrap();

        let protected_during = std::fs::metadata(&protected).unwrap().permissions().mode() & 0o777;
        assert_eq!(
            protected_during & 0o222,
            0,
            "guard should remove write bits"
        );

        drop(guard);

        let restored_root = std::fs::metadata(&root).unwrap().permissions().mode() & 0o777;
        let restored_file = std::fs::metadata(&protected).unwrap().permissions().mode() & 0o777;
        std::fs::remove_dir_all(&root).ok();

        assert_eq!(
            restored_root, root_mode,
            "directory mode should restore exactly"
        );
        assert_eq!(restored_file, file_mode, "file mode should restore exactly");
    }
}
