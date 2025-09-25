use std::io;

#[derive(Debug, Clone)]
pub(crate) enum DirectoryEntry {
    Directory(Directory),
    File(u64),
}

#[derive(Default, Debug, Clone)]
pub(crate) struct Directory(Vec<(String, DirectoryEntry)>);
impl Directory {
    fn get_mut(&mut self, path: &str) -> Option<&mut DirectoryEntry> {
        let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        let mut current = self;
        if path.is_empty() {
            panic!("path cannot be empty");
        }
        for (i, part) in parts.iter().enumerate() {
            let (_, entry) = current.0.iter_mut().find(|(name, _)| name == part)?;
            if i == parts.len() - 1 {
                return Some(entry);
            }
            match entry {
                DirectoryEntry::Directory(d) => current = d,
                DirectoryEntry::File(_) => return None,
            }
        }
        unreachable!();
    }

    fn get_parent_directory_mut(&mut self, path: &str) -> io::Result<&mut Directory> {
        let path = match path.rsplit_once('/') {
            Some((p, _)) => p,
            None => "",
        };
        if path.is_empty() {
            return Ok(self);
        }
        match self.get_mut(path) {
            Some(DirectoryEntry::Directory(d)) => Ok(d),
            Some(DirectoryEntry::File(_)) => Err(io::Error::other(format!(
                "parent '{}' is a file, not a directory",
                path
            ))),
            None => Err(io::Error::other(format!(
                "parent directory '{}' does not exist",
                path
            ))),
        }
    }
    fn get_name(path: &str) -> &str {
        match path.rsplit_once('/') {
            Some((_, n)) => n,
            None => path,
        }
    }

    pub(crate) fn entries(&self) -> &[(String, DirectoryEntry)] {
        &self.0
    }

    pub(crate) fn create_file(&mut self, path: &str, inode: u64) -> io::Result<()> {
        let parent = self.get_parent_directory_mut(path)?;
        let name = Self::get_name(path);
        if parent.0.iter_mut().any(|(n, _)| n == name) {
            return Err(io::Error::other(format!("path '{}' already exists", path)));
        } else {
            parent
                .0
                .push((name.to_string(), DirectoryEntry::File(inode)));
        }
        Ok(())
    }

    pub(crate) fn mkdir(&mut self, path: &str) -> io::Result<&mut Directory> {
        let parent = self.get_parent_directory_mut(path)?;
        let name = Self::get_name(path);
        if parent.0.iter_mut().any(|(n, _)| n == name) {
            return Err(io::Error::other(format!("path '{}' already exists", path)));
        } else {
            parent.0.push((
                name.to_string(),
                DirectoryEntry::Directory(Directory::default()),
            ));
        }
        match parent.0.iter_mut().find(|(n, _)| n == name) {
            Some((_, DirectoryEntry::Directory(d))) => Ok(d),
            _ => unreachable!(),
        }
    }
    pub(crate) fn mkdir_p(&mut self, path: &str) -> io::Result<&mut Directory> {
        let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        for i in 0..(parts.len() - 1) {
            let sub_path = parts[..=i].join("/");
            if self.get_mut(&sub_path).is_none() {
                self.mkdir(&sub_path)?;
            }
        }
        self.mkdir(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mkdir_and_create_file() {
        let mut root = Directory::default();
        // Create directory
        let dir = root.mkdir("foo").unwrap();
        assert!(matches!(dir, Directory { .. }));
        // Create file in directory
        root.create_file("foo/bar.txt", 42).unwrap();
        // Check file exists
        match root.get_mut("foo/bar.txt") {
            Some(DirectoryEntry::File(inode)) => assert_eq!(*inode, 42),
            _ => panic!("File not found or wrong type"),
        }
    }

    #[test]
    fn test_mkdir_existing_should_fail() {
        let mut root = Directory::default();
        root.mkdir("foo").unwrap();
        let res = root.mkdir("foo");
        assert!(res.is_err());
    }

    #[test]
    fn test_create_file_existing_should_fail() {
        let mut root = Directory::default();
        root.mkdir("foo").unwrap();
        root.create_file("foo/bar.txt", 1).unwrap();
        let res = root.create_file("foo/bar.txt", 2);
        assert!(res.is_err());
    }

    #[test]
    fn test_get_parent_directory_mut_nonexistent() {
        let mut root = Directory::default();
        let res = root.get_parent_directory_mut("foo/bar.txt");
        assert!(res.is_err());
    }

    #[test]
    fn test_get_parent_directory_mut_file_as_parent() {
        let mut root = Directory::default();
        root.mkdir("foo").unwrap();
        root.create_file("foo/bar", 1).unwrap();
        let res = root.get_parent_directory_mut("foo/bar/baz.txt");
        assert!(res.is_err());
    }

    #[test]
    fn test_get_name() {
        assert_eq!(Directory::get_name("foo/bar.txt"), "bar.txt");
        assert_eq!(Directory::get_name("bar.txt"), "bar.txt");
        assert_eq!(Directory::get_name("foo/bar/baz"), "baz");
        assert_eq!(Directory::get_name("foo/"), "");
    }

    #[test]
    fn test_mkdir_p_creates_all() {
        let mut root = Directory::default();
        root.mkdir_p("a/b/c").unwrap();
        assert!(matches!(
            root.get_mut("a/b/c"),
            Some(DirectoryEntry::Directory(_))
        ));
    }

    #[test]
    fn test_mkdir_p_existing_path() {
        let mut root = Directory::default();
        root.mkdir("a").unwrap();
        root.mkdir_p("a/b/c").unwrap();
        assert!(matches!(
            root.get_mut("a/b/c"),
            Some(DirectoryEntry::Directory(_))
        ));
    }

    #[test]
    fn test_get_mut_file_and_directory() {
        let mut root = Directory::default();
        root.mkdir_p("dir1/dir2").unwrap();
        root.create_file("dir1/dir2/file.txt", 99).unwrap();
        // Directory
        match root.get_mut("dir1/dir2") {
            Some(DirectoryEntry::Directory(_)) => {}
            _ => panic!("Expected directory"),
        }
        // File
        match root.get_mut("dir1/dir2/file.txt") {
            Some(DirectoryEntry::File(inode)) => assert_eq!(*inode, 99),
            _ => panic!("Expected file"),
        }
    }

    #[test]
    fn test_get_mut_nonexistent() {
        let mut root = Directory::default();
        assert!(root.get_mut("no/such/path").is_none());
    }

    #[test]
    fn test_create_file_in_root() {
        let mut root = Directory::default();
        root.create_file("file.txt", 123).unwrap();
        match root.get_mut("file.txt") {
            Some(DirectoryEntry::File(inode)) => assert_eq!(*inode, 123),
            _ => panic!("Expected file"),
        }
    }
}
