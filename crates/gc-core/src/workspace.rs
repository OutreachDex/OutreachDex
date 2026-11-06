use table_sdk::{KitDocument, TableDocument};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Kit(String),
    #[error("{0}")]
    Invalid(String),
    #[error("The file was changed elsewhere. Reopen the workspace before editing.")]
    Conflict,
}

#[derive(Serialize)]
pub struct FileRead {
    pub bytes: Vec<u8>,
    pub version: String,
    pub size: usize,
}

/// Workspace files are always resolved relative to a checked canonical root.
#[derive(Clone)]
pub struct Workspace {
    root: PathBuf,
}

impl Workspace {
    pub fn create(path: &Path) -> Result<Self, Error> {
        Self::create_with(path, "blank")
    }

    /// Creates a workspace from a bundled template (`crate::config::TEMPLATES`).
    pub fn create_with(path: &Path, template: &str) -> Result<Self, Error> {
        let config = crate::config::template(template)?.config();
        fs::create_dir_all(path)?;
        let root = fs::canonicalize(path)?;
        let target = root.join("contacts.ktable");
        if target.symlink_metadata().is_ok() {
            return Err(Error::Invalid(
                "contacts.ktable already exists. Use Open workspace instead.".into(),
            ));
        }
        let doc = crate::schema::contacts_document(&config).map_err(Error::Kit)?;
        let bytes = doc.save(None).map_err(Error::Kit)?;
        for folder in ["people", "intake", "drafts", "mail", "campaigns"] {
            let dir = root.join(folder);
            if dir
                .symlink_metadata()
                .is_ok_and(|m| m.file_type().is_symlink())
            {
                return Err(Error::Invalid(format!(
                    "Workspace folders can't be symbolic links: {folder}"
                )));
            }
            fs::create_dir_all(dir)?;
        }
        let mut tmp = tempfile::NamedTempFile::new_in(&root)?;
        tmp.write_all(&bytes)?;
        tmp.as_file().sync_all()?;
        tmp.persist_noclobber(target)
            .map_err(|e| Error::Io(e.error))?;
        let workspace = Self { root };
        crate::config::install(&workspace, template)?;
        Ok(workspace)
    }

    pub fn open(path: &Path) -> Result<Self, Error> {
        let workspace = Self {
            root: fs::canonicalize(path)?,
        };
        let file = workspace.read("contacts.ktable", None, None)?;
        TableDocument::open("contacts.ktable", &file.bytes).map_err(Error::Kit)?;
        Ok(workspace)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn file(&self, path: &str) -> Result<PathBuf, Error> {
        use std::path::Component;
        let relative = Path::new(path);
        if path.is_empty()
            || path.contains('\\')
            || relative
                .components()
                .any(|c| !matches!(c, Component::Normal(_)))
            || relative
                .components()
                .any(|c| c.as_os_str().to_string_lossy().starts_with('.'))
        {
            return Err(Error::Invalid(
                "The path must be inside the workspace and can't access hidden files.".into(),
            ));
        }
        let mut target = self.root.clone();
        for component in relative.components() {
            target.push(component);
            if target
                .symlink_metadata()
                .is_ok_and(|m| m.file_type().is_symlink())
            {
                return Err(Error::Invalid("The workspace path can't contain symbolic links.".into()));
            }
        }
        Ok(target)
    }

    pub fn put_new(&self, path: &str, bytes: &[u8]) -> Result<String, Error> {
        self.write(path, bytes, "create", None)
    }

    pub fn stamp(&self, path: &str) -> Result<Option<String>, Error> {
        match fs::read(self.file(path)?) {
            Ok(bytes) => Ok(Some(version(&bytes))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn read(
        &self,
        path: &str,
        offset: Option<usize>,
        length: Option<usize>,
    ) -> Result<FileRead, Error> {
        let bytes = fs::read(self.file(path)?)?;
        let size = bytes.len();
        let version = version(&bytes);
        let start = offset.unwrap_or(0).min(size);
        let end = length.map_or(size, |n| start.saturating_add(n).min(size));
        Ok(FileRead {
            bytes: bytes[start..end].to_vec(),
            version,
            size,
        })
    }

    pub fn write(
        &self,
        path: &str,
        bytes: &[u8],
        mode: &str,
        expected: Option<&str>,
    ) -> Result<String, Error> {
        if !matches!(mode, "save" | "create") {
            return Err(Error::Invalid("Only saving and creating workspace documents is supported.".into()));
        }
        let target = self.file(path)?;
        if (mode == "save" && expected.is_none())
            || (mode == "create" && self.stamp(path)?.is_some())
            || self.stamp(path)?.as_deref() != expected
        {
            return Err(Error::Conflict);
        }
        if bytes.len() > 64 * 1024 * 1024 {
            return Err(Error::Invalid("The document is over 64 MB.".into()));
        }
        match target.extension().and_then(|s| s.to_str()) {
            Some("ktable") => {
                TableDocument::open(path, bytes).map_err(Error::Kit)?;
            }
            Some("md") => {
                use note_sdk::{KitDocument, NoteDocument};
                NoteDocument::open(path, bytes).map_err(Error::Kit)?;
            }
            _ => {
                return Err(Error::Invalid(
                    "The editor can only write .ktable or .md documents.".into(),
                ));
            }
        }
        let parent = target
            .parent()
            .ok_or_else(|| Error::Invalid("Invalid path.".into()))?;
        fs::create_dir_all(parent)?;
        let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
        tmp.write_all(bytes)?;
        tmp.as_file().sync_all()?;
        // Recheck after serialization/validation, immediately before replacing the file.
        if self.stamp(path)?.as_deref() != expected {
            return Err(Error::Conflict);
        }
        if mode == "create" {
            tmp.persist_noclobber(target)
                .map_err(|e| Error::Io(e.error))?;
        } else {
            tmp.persist(target).map_err(|e| Error::Io(e.error))?;
        }
        Ok(version(bytes))
    }
}

fn version(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
