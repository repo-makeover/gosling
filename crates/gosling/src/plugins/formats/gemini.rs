use crate::plugins::{
    collect_skill_candidate, copy_dir_all, write_install_metadata, FormatNotSupported,
    ImportedSkill, PluginFormat, PluginInstall, PluginInstallOptions, SkillCandidate,
};
use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use fs_err as fs;
use serde::Deserialize;
use std::path::Path;

pub(in crate::plugins) const MANIFEST: &str = "gemini-extension.json";

#[derive(Debug, Deserialize)]
struct GeminiManifest {
    name: String,
    version: String,
}

pub(in crate::plugins) fn try_install_from_manifest_at_root(
    source: &str,
    checkout_dir: &Path,
    install_root: &Path,
    options: &PluginInstallOptions,
    last_update_check: Option<DateTime<Utc>>,
) -> Result<PluginInstall> {
    let manifest_path = checkout_dir.join(MANIFEST);
    if !manifest_path.is_file() {
        return Err(FormatNotSupported.into());
    }

    let manifest: GeminiManifest = serde_json::from_str(&fs::read_to_string(&manifest_path)?)
        .with_context(|| format!("Failed to parse {}", manifest_path.display()))?;

    validate_extension_name(&manifest.name)?;

    fs::create_dir_all(install_root)?;
    let destination = install_root.join(&manifest.name);
    if destination.exists() {
        bail!(
            "Plugin '{}' is already installed at {}",
            manifest.name,
            destination.display()
        );
    }

    let skills = find_skills(checkout_dir)?;
    if skills.is_empty() {
        bail!(
            "Plugin '{}' does not contain any Gemini skills",
            manifest.name
        );
    }

    copy_dir_all(checkout_dir, &destination)?;
    write_install_metadata(
        &destination,
        source,
        "gemini",
        options.auto_update,
        last_update_check,
    )?;

    Ok(PluginInstall {
        name: manifest.name,
        version: manifest.version,
        format: PluginFormat::Gemini,
        source: source.to_string(),
        directory: destination.clone(),
        skills: skills
            .into_iter()
            .map(|skill| ImportedSkill {
                name: skill.name,
                directory: destination.join(skill.relative_directory),
            })
            .collect(),
    })
}

fn validate_extension_name(name: &str) -> Result<()> {
    if name.is_empty() {
        bail!("Gemini extension name must not be empty");
    }

    if !name
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
    {
        bail!(
            "Invalid Gemini extension name '{}'. Names may only contain letters, numbers, and dashes",
            name
        );
    }

    Ok(())
}

fn find_skills(extension_dir: &Path) -> Result<Vec<SkillCandidate>> {
    let skills_dir = extension_dir.join("skills");
    if !skills_dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut skills = Vec::new();
    collect_skill_candidate(extension_dir, &skills_dir, &mut skills)?;

    for entry in fs::read_dir(&skills_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_skill_candidate(extension_dir, &path, &mut skills)?;
        }
    }

    skills.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(skills)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installs_gemini_extension_skills() {
        let install_root = tempfile::tempdir().unwrap();
        let repo = tempfile::tempdir().unwrap();
        fs::write(
            repo.path().join(MANIFEST),
            r#"{"name":"test-plugin","version":"1.0.0"}"#,
        )
        .unwrap();
        let skill_dir = repo.path().join("skills").join("audit");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: audit\ndescription: Audit code\n---\nDo an audit.",
        )
        .unwrap();

        let installed = try_install_from_manifest_at_root(
            "https://example.invalid/repo.git",
            repo.path(),
            install_root.path(),
            &PluginInstallOptions::default(),
            None,
        )
        .unwrap();

        assert_eq!(installed.name, "test-plugin");
        assert_eq!(installed.version, "1.0.0");
        assert_eq!(installed.skills.len(), 1);
        assert_eq!(installed.skills[0].name, "audit");
        assert!(installed.directory.join(MANIFEST).is_file());
        assert!(installed
            .directory
            .join(crate::plugins::INSTALL_METADATA)
            .is_file());
        assert_eq!(installed.directory, install_root.path().join("test-plugin"));
    }

    #[test]
    fn rejects_skills_without_required_frontmatter_before_installing() {
        let install_root = tempfile::tempdir().unwrap();
        let repo = tempfile::tempdir().unwrap();
        fs::write(
            repo.path().join(MANIFEST),
            r#"{"name":"bad-plugin","version":"1.0.0"}"#,
        )
        .unwrap();
        let skill_dir = repo.path().join("skills").join("broken");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(skill_dir.join("SKILL.md"), "Run a broken skill.").unwrap();

        let error = try_install_from_manifest_at_root(
            "https://example.invalid/bad-plugin.git",
            repo.path(),
            install_root.path(),
            &PluginInstallOptions::default(),
            None,
        )
        .unwrap_err();

        assert!(error
            .to_string()
            .contains("missing required YAML frontmatter"));
        assert!(!install_root.path().join("bad-plugin").exists());
    }
}
