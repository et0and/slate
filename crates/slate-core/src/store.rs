//! Local SQLite catalogue for projects and recent files.
//!
//! `.contactsheet` files remain portable documents. This catalogue stores only
//! local metadata: project name, path, photo count and last update time.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context as _, Result};
use rusqlite::{Connection, params};

use crate::model::Project;

#[derive(Debug, Clone)]
pub struct RecentProject {
    pub name: String,
    pub path: PathBuf,
}

pub struct LocalStore {
    path: PathBuf,
}

impl LocalStore {
    pub fn default_location() -> Self {
        Self {
            path: default_database_path(),
        }
    }

    #[cfg(test)]
    pub fn at(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn recent_projects(&self, limit: usize) -> Result<Vec<RecentProject>> {
        let connection = self.connect()?;
        let mut statement = connection
            .prepare("SELECT name, path FROM projects ORDER BY updated_at DESC LIMIT ?1")?;
        let rows = statement.query_map(params![limit as i64], |row| {
            Ok(RecentProject {
                name: row.get(0)?,
                path: PathBuf::from(row.get::<_, String>(1)?),
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("read recent projects")
    }

    pub fn remember_project(&self, path: &Path, project: &Project) -> Result<()> {
        let connection = self.connect()?;
        let updated_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis() as i64)
            .unwrap_or_default();
        connection.execute(
            "INSERT INTO projects(path, name, photo_count, updated_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(path) DO UPDATE SET
               name = excluded.name,
               photo_count = excluded.photo_count,
               updated_at = excluded.updated_at",
            params![
                path.to_string_lossy(),
                project.name,
                project.photos.len() as i64,
                updated_at
            ],
        )?;
        Ok(())
    }

    fn connect(&self) -> Result<Connection> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create {}", parent.display()))?;
        }
        let connection = Connection::open(&self.path)
            .with_context(|| format!("open {}", self.path.display()))?;
        connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS projects (
               path TEXT PRIMARY KEY,
               name TEXT NOT NULL,
               photo_count INTEGER NOT NULL,
               updated_at INTEGER NOT NULL
             );",
        )?;
        Ok(connection)
    }
}

fn default_database_path() -> PathBuf {
    if let Some(directory) = std::env::var_os("SLATE_DATA_DIR") {
        return PathBuf::from(directory).join("metadata.sqlite3");
    }
    #[cfg(target_os = "macos")]
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home)
            .join("Library/Application Support/slate-desktop/metadata.sqlite3");
    }
    #[cfg(target_os = "windows")]
    if let Some(app_data) = std::env::var_os("APPDATA") {
        return PathBuf::from(app_data).join("slate-desktop/metadata.sqlite3");
    }
    if let Some(data_home) = std::env::var_os("XDG_DATA_HOME") {
        return PathBuf::from(data_home).join("slate-desktop/metadata.sqlite3");
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join(".local/share/slate-desktop/metadata.sqlite3");
    }
    PathBuf::from("metadata.sqlite3")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_projects_are_ordered_and_deduplicated() {
        let directory = tempfile::tempdir().unwrap();
        let store = LocalStore::at(directory.path().join("metadata.sqlite3"));
        let mut first = Project::new("First");
        first.add_photo(PathBuf::from("/tmp/first.jpg"), 10, 10, 1);
        let mut second = Project::new("Second");
        second.add_photo(PathBuf::from("/tmp/second.jpg"), 10, 10, 1);

        store
            .remember_project(Path::new("/tmp/first.contactsheet"), &first)
            .unwrap();
        store
            .remember_project(Path::new("/tmp/second.contactsheet"), &second)
            .unwrap();
        store
            .remember_project(Path::new("/tmp/first.contactsheet"), &first)
            .unwrap();

        let recent = store.recent_projects(4).unwrap();
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].name, "First");
        assert_eq!(recent[0].path, Path::new("/tmp/first.contactsheet"));
        assert_eq!(recent[1].name, "Second");
    }
}
