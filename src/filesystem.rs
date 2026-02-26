/// In-memory filesystem (tmpfs).
///
/// Provides a simple hierarchical filesystem living entirely in the kernel heap.
/// Supports files and directories, absolute and relative paths, and basic CRUD.

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

pub static FILESYSTEM: Mutex<Option<FileSystem>> = Mutex::new(None);

pub type InodeId = u64;

#[derive(Debug)]
pub enum FsError {
    NotFound,
    AlreadyExists,
    NotADirectory,
    NotAFile,
    DirectoryNotEmpty,
    InvalidPath,
}

impl core::fmt::Display for FsError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            FsError::NotFound => write!(f, "not found"),
            FsError::AlreadyExists => write!(f, "already exists"),
            FsError::NotADirectory => write!(f, "not a directory"),
            FsError::NotAFile => write!(f, "not a file"),
            FsError::DirectoryNotEmpty => write!(f, "directory not empty"),
            FsError::InvalidPath => write!(f, "invalid path"),
        }
    }
}

#[derive(Debug)]
pub enum InodeKind {
    File(Vec<u8>),
    Directory(BTreeMap<String, InodeId>),
}

#[derive(Debug)]
pub struct Inode {
    pub kind: InodeKind,
    pub parent: InodeId,
}

pub struct FileSystem {
    inodes: BTreeMap<InodeId, Inode>,
    next_inode: InodeId,
}

/// Initialize the global filesystem with an empty root directory.
pub fn init() {
    let mut fs = FileSystem {
        inodes: BTreeMap::new(),
        next_inode: 1,
    };
    // Root directory: inode 0, parent points to itself.
    fs.inodes.insert(
        0,
        Inode {
            kind: InodeKind::Directory(BTreeMap::new()),
            parent: 0,
        },
    );
    *FILESYSTEM.lock() = Some(fs);
}

impl FileSystem {
    fn alloc_inode(&mut self) -> InodeId {
        let id = self.next_inode;
        self.next_inode += 1;
        id
    }

    /// Check whether an inode is a directory.
    pub fn is_directory(&self, inode: InodeId) -> bool {
        self.inodes
            .get(&inode)
            .map(|n| matches!(n.kind, InodeKind::Directory(_)))
            .unwrap_or(false)
    }

    /// Resolve a path string to an inode id, starting from `cwd`.
    pub fn resolve_path(&self, path: &str, cwd: InodeId) -> Result<InodeId, FsError> {
        let path = path.trim();
        if path.is_empty() {
            return Ok(cwd);
        }

        let mut current = if path.starts_with('/') { 0 } else { cwd };

        for component in path.split('/') {
            if component.is_empty() || component == "." {
                continue;
            }
            if component == ".." {
                current = self
                    .inodes
                    .get(&current)
                    .ok_or(FsError::NotFound)?
                    .parent;
                continue;
            }
            let inode = self.inodes.get(&current).ok_or(FsError::NotFound)?;
            match &inode.kind {
                InodeKind::Directory(entries) => {
                    current = *entries.get(component).ok_or(FsError::NotFound)?;
                }
                InodeKind::File(_) => return Err(FsError::NotADirectory),
            }
        }
        Ok(current)
    }

    /// Resolve the parent directory and return `(parent_inode, child_name)`.
    pub fn resolve_parent(
        &self,
        path: &str,
        cwd: InodeId,
    ) -> Result<(InodeId, String), FsError> {
        let path = path.trim();
        if path.is_empty() || path == "/" {
            return Err(FsError::InvalidPath);
        }

        // Strip trailing slashes.
        let path = path.trim_end_matches('/');
        if path.is_empty() {
            return Err(FsError::InvalidPath);
        }

        if let Some(pos) = path.rfind('/') {
            let parent_path = &path[..pos];
            let child_name = &path[pos + 1..];
            if child_name.is_empty() {
                return Err(FsError::InvalidPath);
            }
            let parent_path = if parent_path.is_empty() {
                "/"
            } else {
                parent_path
            };
            let parent_id = self.resolve_path(parent_path, cwd)?;
            Ok((parent_id, String::from(child_name)))
        } else {
            // No slash — child lives directly under cwd.
            Ok((cwd, String::from(path)))
        }
    }

    /// List entries of a directory inode. Returns `(name, is_dir)` pairs.
    pub fn list_dir(&self, inode: InodeId) -> Result<Vec<(String, bool)>, FsError> {
        let node = self.inodes.get(&inode).ok_or(FsError::NotFound)?;
        match &node.kind {
            InodeKind::Directory(entries) => {
                let mut result = Vec::new();
                for (name, &child_id) in entries.iter() {
                    let child = self.inodes.get(&child_id).ok_or(FsError::NotFound)?;
                    let is_dir = matches!(child.kind, InodeKind::Directory(_));
                    result.push((name.clone(), is_dir));
                }
                Ok(result)
            }
            InodeKind::File(_) => Err(FsError::NotADirectory),
        }
    }

    /// Read a file's content.
    pub fn read_file(&self, inode: InodeId) -> Result<&[u8], FsError> {
        let node = self.inodes.get(&inode).ok_or(FsError::NotFound)?;
        match &node.kind {
            InodeKind::File(data) => Ok(data.as_slice()),
            InodeKind::Directory(_) => Err(FsError::NotAFile),
        }
    }

    /// Create an empty file at `path` relative to `cwd`.
    pub fn create_file(&mut self, path: &str, cwd: InodeId) -> Result<InodeId, FsError> {
        let (parent_id, name) = self.resolve_parent(path, cwd)?;

        // Ensure parent is a directory.
        let parent = self.inodes.get(&parent_id).ok_or(FsError::NotFound)?;
        if let InodeKind::Directory(entries) = &parent.kind {
            if entries.contains_key(&name) {
                return Err(FsError::AlreadyExists);
            }
        } else {
            return Err(FsError::NotADirectory);
        }

        let id = self.alloc_inode();
        self.inodes.insert(
            id,
            Inode {
                kind: InodeKind::File(Vec::new()),
                parent: parent_id,
            },
        );

        // Add to parent's entries.
        if let Some(parent) = self.inodes.get_mut(&parent_id) {
            if let InodeKind::Directory(entries) = &mut parent.kind {
                entries.insert(name, id);
            }
        }
        Ok(id)
    }

    /// Create an empty directory at `path` relative to `cwd`.
    pub fn create_dir(&mut self, path: &str, cwd: InodeId) -> Result<InodeId, FsError> {
        let (parent_id, name) = self.resolve_parent(path, cwd)?;

        let parent = self.inodes.get(&parent_id).ok_or(FsError::NotFound)?;
        if let InodeKind::Directory(entries) = &parent.kind {
            if entries.contains_key(&name) {
                return Err(FsError::AlreadyExists);
            }
        } else {
            return Err(FsError::NotADirectory);
        }

        let id = self.alloc_inode();
        self.inodes.insert(
            id,
            Inode {
                kind: InodeKind::Directory(BTreeMap::new()),
                parent: parent_id,
            },
        );

        if let Some(parent) = self.inodes.get_mut(&parent_id) {
            if let InodeKind::Directory(entries) = &mut parent.kind {
                entries.insert(name, id);
            }
        }
        Ok(id)
    }

    /// Remove a file or empty directory at `path` relative to `cwd`.
    pub fn remove(&mut self, path: &str, cwd: InodeId) -> Result<(), FsError> {
        let (parent_id, name) = self.resolve_parent(path, cwd)?;

        // Find child inode id.
        let child_id = {
            let parent = self.inodes.get(&parent_id).ok_or(FsError::NotFound)?;
            match &parent.kind {
                InodeKind::Directory(entries) => {
                    *entries.get(&name).ok_or(FsError::NotFound)?
                }
                InodeKind::File(_) => return Err(FsError::NotADirectory),
            }
        };

        // Check if directory is empty.
        {
            let child = self.inodes.get(&child_id).ok_or(FsError::NotFound)?;
            if let InodeKind::Directory(entries) = &child.kind {
                if !entries.is_empty() {
                    return Err(FsError::DirectoryNotEmpty);
                }
            }
        }

        // Remove from parent's entries and from inode table.
        if let Some(parent) = self.inodes.get_mut(&parent_id) {
            if let InodeKind::Directory(entries) = &mut parent.kind {
                entries.remove(&name);
            }
        }
        self.inodes.remove(&child_id);
        Ok(())
    }

    /// Write content to a file at `path`. Creates the file if it doesn't exist.
    pub fn write_file(
        &mut self,
        path: &str,
        content: &[u8],
        cwd: InodeId,
    ) -> Result<(), FsError> {
        // Try to resolve the file first.
        match self.resolve_path(path, cwd) {
            Ok(inode_id) => {
                let node = self.inodes.get_mut(&inode_id).ok_or(FsError::NotFound)?;
                match &mut node.kind {
                    InodeKind::File(data) => {
                        data.clear();
                        data.extend_from_slice(content);
                        Ok(())
                    }
                    InodeKind::Directory(_) => Err(FsError::NotAFile),
                }
            }
            Err(FsError::NotFound) => {
                // Create the file first.
                let id = self.create_file(path, cwd)?;
                let node = self.inodes.get_mut(&id).ok_or(FsError::NotFound)?;
                if let InodeKind::File(data) = &mut node.kind {
                    data.extend_from_slice(content);
                }
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    /// Build the absolute path string for an inode by walking parents.
    pub fn get_path(&self, inode: InodeId) -> Result<String, FsError> {
        if inode == 0 {
            return Ok(String::from("/"));
        }

        let mut parts = Vec::new();
        let mut current = inode;

        loop {
            let node = self.inodes.get(&current).ok_or(FsError::NotFound)?;
            let parent_id = node.parent;

            if parent_id == current {
                // We've reached root.
                break;
            }

            // Find our name in parent's directory entries.
            let parent = self.inodes.get(&parent_id).ok_or(FsError::NotFound)?;
            if let InodeKind::Directory(entries) = &parent.kind {
                for (name, &child_id) in entries.iter() {
                    if child_id == current {
                        parts.push(name.clone());
                        break;
                    }
                }
            }
            current = parent_id;
        }

        parts.reverse();
        let mut path = String::from("/");
        for (i, part) in parts.iter().enumerate() {
            if i > 0 {
                path.push('/');
            }
            path.push_str(part);
        }
        Ok(path)
    }
}
