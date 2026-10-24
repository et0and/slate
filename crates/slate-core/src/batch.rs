//! Batch operations: pattern renames on disk and tags in the project.

use std::collections::HashSet;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::model::{Photo, PhotoId, Project};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RenameOptions {
    pub prefix: String,
    pub suffix: String,
    pub start: u32,
    pub padding: u8,
}

impl Default for RenameOptions {
    fn default() -> Self {
        Self {
            prefix: String::new(),
            suffix: String::new(),
            start: 1,
            padding: 4,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RenameEntry {
    pub photo_id: PhotoId,
    pub from: PathBuf,
    pub to: PathBuf,
    pub conflict: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct RenamePlan {
    pub entries: Vec<RenameEntry>,
    pub skipped: usize,
}

impl RenamePlan {
    pub fn can_apply(&self) -> bool {
        !self.entries.is_empty() && self.entries.iter().all(|entry| entry.conflict.is_none())
    }

    pub fn conflicts(&self) -> impl Iterator<Item = &RenameEntry> {
        self.entries.iter().filter(|entry| entry.conflict.is_some())
    }
}

#[derive(Debug, Clone, Default)]
pub struct RenameReport {
    pub renamed: Vec<(PhotoId, PathBuf, String)>,
    pub errors: Vec<(PhotoId, String)>,
}

impl RenameReport {
    pub fn renamed_count(&self) -> usize {
        self.renamed.len()
    }
}

pub fn sanitize_fragment(input: &str) -> String {
    input
        .chars()
        .map(|character| match character {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            character if character.is_control() => '-',
            character => character,
        })
        .collect::<String>()
        .trim()
        .trim_end_matches('.')
        .to_string()
}

fn target_name(photo: &Photo, number: u32, options: &RenameOptions) -> String {
    let prefix = sanitize_fragment(&options.prefix);
    let suffix = sanitize_fragment(&options.suffix);
    let padded = format!(
        "{:0width$}",
        number,
        width = usize::from(options.padding.clamp(1, 8))
    );
    let extension = photo.extension();
    let stem = format!("{prefix}{padded}{suffix}");
    if extension.is_empty() {
        stem
    } else {
        format!("{stem}.{extension}")
    }
}

/// Filename a single photo would receive; used for live previews in the UI.
pub fn preview_name(photo: &Photo, number: u32, options: &RenameOptions) -> String {
    target_name(photo, number, options)
}

pub fn plan_rename(project: &Project, selected: &[PhotoId], options: &RenameOptions) -> RenamePlan {
    let selected: HashSet<PhotoId> = selected.iter().copied().collect();
    let mut plan = RenamePlan::default();

    for photo in project
        .photos
        .iter()
        .filter(|photo| selected.contains(&photo.id))
    {
        let name = target_name(
            photo,
            options.start.saturating_add(plan.entries.len() as u32),
            options,
        );
        let to = photo
            .path
            .parent()
            .map(|parent| parent.join(&name))
            .unwrap_or_else(|| PathBuf::from(&name));

        if to == photo.path {
            plan.skipped += 1;
            continue;
        }

        plan.entries.push(RenameEntry {
            photo_id: photo.id,
            from: photo.path.clone(),
            to,
            conflict: None,
        });
    }

    let targets: Vec<String> = plan
        .entries
        .iter()
        .map(|entry| entry.to.to_string_lossy().to_lowercase())
        .collect();

    for (index, entry) in plan.entries.iter_mut().enumerate() {
        let target = &targets[index];
        let duplicate = targets
            .iter()
            .enumerate()
            .any(|(other, other_target)| other != index && other_target == target);
        if duplicate {
            entry.conflict = Some("duplicate name in batch".into());
            continue;
        }
        if entry.to.exists() {
            entry.conflict = Some("a file with this name already exists".into());
        }
    }

    plan
}

pub fn apply_rename(project: &mut Project, plan: &RenamePlan) -> RenameReport {
    let mut report = RenameReport::default();
    for entry in &plan.entries {
        if entry.conflict.is_some() {
            continue;
        }
        // Re-check at apply time: a file may have appeared since the preview.
        if entry.to.exists() && entry.to != entry.from {
            report.errors.push((
                entry.photo_id,
                "target appeared after the preview; skipped".to_string(),
            ));
            continue;
        }
        match std::fs::rename(&entry.from, &entry.to) {
            Ok(()) => {
                let name = entry
                    .to
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default();
                if let Some(photo) = project.photo_mut(entry.photo_id) {
                    photo.path = entry.to.clone();
                }
                report
                    .renamed
                    .push((entry.photo_id, entry.to.clone(), name));
            }
            Err(error) => report.errors.push((entry.photo_id, error.to_string())),
        }
    }
    report
}

pub fn sanitize_tag(input: &str) -> Option<String> {
    let mut tag = String::new();
    let mut last_dash = false;
    for character in input.trim().trim_start_matches('#').to_lowercase().chars() {
        let character = if character == '_' || character.is_whitespace() {
            '-'
        } else {
            character
        };
        if character.is_alphanumeric() || character == '-' || character == '.' {
            if character == '-' {
                if last_dash {
                    continue;
                }
                last_dash = true;
            } else {
                last_dash = false;
            }
            tag.push(character);
        }
    }
    let tag = tag.trim_matches('-').to_string();
    if tag.is_empty() {
        None
    } else {
        Some(tag.chars().take(40).collect())
    }
}

pub fn add_tag(project: &mut Project, ids: &[PhotoId], tag: &str) -> usize {
    let Some(tag) = sanitize_tag(tag) else {
        return 0;
    };
    let ids: HashSet<PhotoId> = ids.iter().copied().collect();
    let mut changed = 0;
    for photo in &mut project.photos {
        if ids.contains(&photo.id) && !photo.has_tag(&tag) {
            photo.tags.push(tag.clone());
            photo.tags.sort();
            changed += 1;
        }
    }
    changed
}

pub fn remove_tag(project: &mut Project, ids: &[PhotoId], tag: &str) -> usize {
    let tag = sanitize_tag(tag).unwrap_or_else(|| tag.trim().to_lowercase());
    let ids: HashSet<PhotoId> = ids.iter().copied().collect();
    let mut changed = 0;
    for photo in &mut project.photos {
        if ids.contains(&photo.id) {
            let before = photo.tags.len();
            photo.tags.retain(|existing| existing != &tag);
            if photo.tags.len() != before {
                changed += 1;
            }
        }
    }
    changed
}

pub fn clear_tags(project: &mut Project, ids: &[PhotoId]) -> usize {
    let ids: HashSet<PhotoId> = ids.iter().copied().collect();
    let mut changed = 0;
    for photo in &mut project.photos {
        if ids.contains(&photo.id) && !photo.tags.is_empty() {
            photo.tags.clear();
            changed += 1;
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn photo(id: u64, name: &str) -> Photo {
        Photo {
            id: PhotoId(id),
            path: PathBuf::from("/tmp").join(name),
            width: 100,
            height: 100,
            bytes: 0,
            tags: vec![],
            rotation: 0,
        }
    }

    fn project(names: &[&str]) -> Project {
        let mut project = Project::new("Test");
        project.photos = names
            .iter()
            .enumerate()
            .map(|(index, name)| photo(index as u64 + 1, name))
            .collect();
        project
    }

    #[test]
    fn builds_padded_names_and_keeps_extensions() {
        let project = project(&["a.JPG", "b.tiff"]);
        let options = RenameOptions {
            prefix: "SF-".into(),
            suffix: "-WEB".into(),
            start: 7,
            padding: 3,
        };
        let plan = plan_rename(&project, &project.photo_ids(), &options);
        assert_eq!(plan.entries[0].to.file_name().unwrap(), "SF-007-WEB.jpg");
        assert_eq!(plan.entries[1].to.file_name().unwrap(), "SF-008-WEB.tiff");
        assert!(plan.can_apply());
    }

    #[test]
    fn catches_duplicate_targets() {
        let mut project = project(&["a.jpg"]);
        project.photos.push(project.photos[0].clone());
        let options = RenameOptions {
            prefix: "SAME".into(),
            start: u32::MAX,
            padding: 1,
            suffix: String::new(),
        };
        let plan = plan_rename(&project, &project.photo_ids(), &options);
        assert!(!plan.can_apply());
        assert_eq!(plan.conflicts().count(), 2);
    }

    #[test]
    fn same_file_name_in_different_folders_is_not_a_conflict() {
        let mut project = project(&["a.jpg"]);
        project.photos.push(Photo {
            id: PhotoId(2),
            path: PathBuf::from("/other/a.jpg"),
            width: 10,
            height: 10,
            bytes: 1,
            tags: vec![],
            rotation: 0,
        });
        let options = RenameOptions {
            prefix: "same".into(),
            start: 1,
            padding: 1,
            suffix: String::new(),
        };
        let plan = plan_rename(&project, &project.photo_ids(), &options);
        assert_eq!(plan.entries.len(), 2);
        assert!(!plan.entries[0].conflict.is_some());
        assert!(!plan.entries[1].conflict.is_some());
    }

    #[test]
    fn existing_file_is_a_conflict_but_same_path_is_skipped() {
        let directory = tempfile::tempdir().unwrap();
        let original = directory.path().join("one1.jpg");
        let occupied = directory.path().join("TARGET1.jpg");
        std::fs::write(&original, "x").unwrap();
        std::fs::write(&occupied, "x").unwrap();

        let mut project = Project::new("Test");
        project.photos = vec![Photo {
            id: PhotoId(1),
            path: original.clone(),
            width: 10,
            height: 10,
            bytes: 1,
            tags: vec![],
            rotation: 0,
        }];
        let options = RenameOptions {
            prefix: "TARGET".into(),
            start: 1,
            padding: 1,
            suffix: String::new(),
        };
        let plan = plan_rename(&project, &[PhotoId(1)], &options);
        assert!(!plan.can_apply());
        assert!(plan.entries[0].conflict.is_some());

        let options = RenameOptions {
            prefix: "one".into(),
            start: 1,
            padding: 1,
            suffix: String::new(),
        };
        let plan = plan_rename(&project, &[PhotoId(1)], &options);
        assert_eq!(plan.skipped, 1);
        assert!(plan.entries.is_empty());
    }

    #[test]
    fn applies_renames_and_updates_project_paths() {
        let directory = tempfile::tempdir().unwrap();
        let first = directory.path().join("one.jpg");
        let second = directory.path().join("two.jpg");
        std::fs::write(&first, "1").unwrap();
        std::fs::write(&second, "2").unwrap();

        let mut project = Project::new("Test");
        project.photos = vec![
            Photo {
                id: PhotoId(1),
                path: first.clone(),
                width: 10,
                height: 10,
                bytes: 1,
                tags: vec![],
                rotation: 0,
            },
            Photo {
                id: PhotoId(2),
                path: second.clone(),
                width: 10,
                height: 10,
                bytes: 1,
                tags: vec![],
                rotation: 0,
            },
        ];

        let options = RenameOptions {
            prefix: "SHOOT".into(),
            start: 1,
            padding: 3,
            suffix: String::new(),
        };
        let plan = plan_rename(&project, &project.photo_ids(), &options);
        let report = apply_rename(&mut project, &plan);
        assert_eq!(report.renamed_count(), 2);
        assert!(report.errors.is_empty());
        assert!(directory.path().join("SHOOT001.jpg").exists());
        assert_eq!(project.photos[0].path.file_name().unwrap(), "SHOOT001.jpg");
    }

    #[test]
    fn apply_refuses_a_target_created_after_the_plan() {
        let directory = tempfile::tempdir().unwrap();
        let original = directory.path().join("one.jpg");
        std::fs::write(&original, "1").unwrap();

        let mut project = Project::new("Test");
        project.photos = vec![Photo {
            id: PhotoId(1),
            path: original.clone(),
            width: 10,
            height: 10,
            bytes: 1,
            tags: vec![],
            rotation: 0,
        }];
        let options = RenameOptions {
            prefix: "SHOT".into(),
            start: 1,
            padding: 2,
            suffix: String::new(),
        };
        let plan = plan_rename(&project, &[PhotoId(1)], &options);
        assert!(plan.can_apply());

        let target = directory.path().join("SHOT01.jpg");
        std::fs::write(&target, "do not overwrite").unwrap();

        let report = apply_rename(&mut project, &plan);
        assert_eq!(report.renamed_count(), 0);
        assert_eq!(report.errors.len(), 1);
        assert_eq!(
            std::fs::read_to_string(&target).unwrap(),
            "do not overwrite"
        );
        assert!(original.exists());
    }

    #[test]
    fn tags_are_normalised_and_deduplicated() {
        assert_eq!(
            sanitize_tag("  #Street Photo  ").as_deref(),
            Some("street-photo")
        );
        assert_eq!(sanitize_tag("a___b").as_deref(), Some("a-b"));
        assert_eq!(sanitize_tag("!!!"), None);

        let mut project = project(&["a.jpg"]);
        let ids = project.photo_ids();
        assert_eq!(add_tag(&mut project, &ids, "Street"), 1);
        assert_eq!(add_tag(&mut project, &ids, "street"), 0);
        assert_eq!(project.photos[0].tags, vec!["street"]);
        assert_eq!(remove_tag(&mut project, &ids, "#street"), 1);
        assert!(project.photos[0].tags.is_empty());
    }
}
