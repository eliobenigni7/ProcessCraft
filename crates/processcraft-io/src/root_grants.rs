//! Roots explicitly authorized by a trusted entry point, not by client requests.
use crate::{Access, FileGrant};
use std::{
    io,
    path::{Path, PathBuf},
};

/// Keeps directory handles alive for the lifetime of the authorization.
#[derive(Debug)]
pub struct RootGrant {
    root: PathBuf,
    access: Access,
    _pin: FileGrant,
}

impl RootGrant {
    pub fn new(root: impl AsRef<Path>, access: Access) -> io::Result<Self> {
        let root = root.as_ref();
        if !root.is_absolute() || !root.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "absolute existing directory required",
            ));
        }
        // FileGrant validates and pins every ancestor, including the root itself.
        let pin = FileGrant::new(
            root.join("processcraft-root-grant-sentinel"),
            Access::ReadOnly,
        )?;
        let canonical = std::fs::canonicalize(root)?;
        Ok(Self {
            root: canonical,
            access,
            _pin: pin,
        })
    }

    pub fn grant(&self, path: impl AsRef<Path>, access: Access) -> io::Result<FileGrant> {
        if access == Access::Write && self.access != Access::Write {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "read-only root",
            ));
        }
        let grant = FileGrant::new(path, access)?;
        if !grant.canonical_path.starts_with(&self.root) || grant.canonical_path == self.root {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "path outside authorized root",
            ));
        }
        Ok(grant)
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    #[test]
    fn only_files_within_explicit_root_are_granted() {
        let base =
            std::env::temp_dir().join(format!("processcraft-root-test-{}", uuid::Uuid::new_v4()));
        let allowed = base.join("allowed");
        let sibling = base.join("allowed-other");
        std::fs::create_dir_all(&allowed).unwrap();
        std::fs::create_dir_all(&sibling).unwrap();
        let root = RootGrant::new(&allowed, Access::ReadOnly).unwrap();
        assert!(root.grant(allowed.join("ok.txt"), Access::ReadOnly).is_ok());
        assert!(root.grant(allowed.join("ok.txt"), Access::Write).is_err());
        assert!(
            root.grant(sibling.join("escape.txt"), Access::ReadOnly)
                .is_err()
        );
        assert!(
            root.grant(
                allowed.join("..").join("allowed-other").join("escape.txt"),
                Access::ReadOnly
            )
            .is_err()
        );
        drop(root);
        std::fs::remove_dir_all(base).unwrap();
    }
}
