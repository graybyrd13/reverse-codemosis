//! Writing inside the project, and only there.
//!
//! Every file the engine writes (corrections, recorded answers, baselines,
//! proof certificates, the audit log) goes through [`write_confined`]. A
//! path is accepted only if it is relative, has no `..`, and no existing
//! component from the project root down is a symbolic link, so neither a
//! crafted path nor a link planted in the repository can make the engine
//! write outside the project.

use std::path::{Component, Path, PathBuf};

/// `root/rel`, if writing there cannot leave `root`.
pub fn confined(root: &Path, rel: &Path) -> Result<PathBuf, String> {
    let mut at = root.to_path_buf();
    let mut any = false;
    for c in rel.components() {
        match c {
            Component::Normal(part) => {
                at.push(part);
                any = true;
                if std::fs::symlink_metadata(&at).is_ok_and(|m| m.file_type().is_symlink()) {
                    return Err(format!("refusing to write through the symbolic link {}", at.display()));
                }
            }
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(format!(
                    "refusing to write {}: the path must stay inside the project",
                    rel.display()
                ));
            }
        }
    }
    if !any {
        return Err("refusing to write: empty path".to_string());
    }
    Ok(at)
}

/// Write `contents` to `root/rel`, creating parent directories, after
/// checking the path with [`confined`].
pub fn write_confined(root: &Path, rel: &Path, contents: &[u8]) -> std::io::Result<()> {
    let path = confined(root, rel).map_err(|e| std::io::Error::new(std::io::ErrorKind::PermissionDenied, e))?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
        // create_dir_all may have created components; check again so a link
        // swapped in meanwhile is still caught.
        confined(root, rel).map_err(|e| std::io::Error::new(std::io::ErrorKind::PermissionDenied, e))?;
    }
    std::fs::write(path, contents)
}

/// Append a line to `root/rel` under the same rules.
pub fn append_confined(root: &Path, rel: &Path, line: &str) -> std::io::Result<()> {
    use std::io::Write as _;
    let path = confined(root, rel).map_err(|e| std::io::Error::new(std::io::ErrorKind::PermissionDenied, e))?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(f, "{line}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_escapes() {
        let root = tempfile::tempdir().unwrap();
        assert!(confined(root.path(), Path::new("../x")).is_err());
        assert!(confined(root.path(), Path::new("a/../../x")).is_err());
        assert!(confined(root.path(), Path::new("/etc/passwd")).is_err());
        assert!(confined(root.path(), Path::new("")).is_err());
        assert!(confined(root.path(), Path::new("src/./a.py")).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinks() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join("link")).unwrap();
        std::os::unix::fs::symlink(outside.path().join("f"), root.path().join("f.py")).unwrap();
        assert!(write_confined(root.path(), Path::new("link/x.txt"), b"x").is_err());
        assert!(write_confined(root.path(), Path::new("f.py"), b"x").is_err());
        assert!(!outside.path().join("x.txt").exists());
        assert!(!outside.path().join("f").exists());
        write_confined(root.path(), Path::new("ok/y.txt"), b"y").unwrap();
        assert_eq!(std::fs::read(root.path().join("ok/y.txt")).unwrap(), b"y");
    }
}
