//! Bounded file observation for output history. Inventory listing never calls this module.

use super::Session;
use anyhow::{ensure, Result};
use gosling_sdk_types::custom_requests::{OutputAttributionKind, OutputRevisionDto};
use gosling_sdk_types::workspace::WorkspaceFolderAccess;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub const MAX_OUTPUT_REVISION_BYTES: usize = 8 * 1024 * 1024;
pub(crate) const MAX_CAPTURE_BYTES: usize = 32 * 1024 * 1024;
const HISTORY_START: &str = "\n\n<!-- gosling:output-history:start -->\n";
const HISTORY_END: &str = "<!-- gosling:output-history:end -->\n";

#[derive(Debug, thiserror::Error)]
pub enum OutputRevisionError {
    #[error("{0}")]
    Validation(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Conflict(String),
    #[error("{0}")]
    Limit(String),
}

#[derive(Clone)]
pub(crate) struct OutputSnapshot {
    pub bytes: Vec<u8>,
    pub body: Vec<u8>,
    pub hash: String,
}

pub(crate) fn digest(bytes: &[u8]) -> String {
    crate::utils::bytes_to_hex(Sha256::digest(bytes))
}

pub(crate) fn is_output_document(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|value| value.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some(
            "md" | "markdown"
                | "txt"
                | "csv"
                | "tsv"
                | "pdf"
                | "doc"
                | "docx"
                | "rtf"
                | "odt"
                | "xlsx"
                | "pptx"
                | "html"
                | "htm"
                | "png"
                | "jpg"
                | "jpeg"
                | "svg"
                | "webp"
        )
    )
}

pub(crate) fn output_roots(session: &Session) -> Vec<PathBuf> {
    output_roots_with_warnings(session).0
}

pub(crate) fn output_roots_with_warnings(session: &Session) -> (Vec<PathBuf>, Vec<String>) {
    let mut roots = vec![
        session.working_dir.join("Outputs"),
        session.working_dir.join("outputs"),
    ];
    if let Some(context) = &session.workspace_context {
        roots.extend(
            context
                .product_output_folders
                .iter()
                .map(|folder| PathBuf::from(&folder.path)),
        );
    }
    let mut warnings = Vec::new();
    let roots = roots
        .into_iter()
        .filter_map(|root| match root.canonicalize() {
            Ok(root) => Some(root),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => {
                warnings.push(format!("{}: {error}", root.display()));
                None
            }
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    (roots, warnings)
}

pub(crate) fn canonical_output_path(
    session: &Session,
    path: &Path,
    write: bool,
) -> Result<PathBuf> {
    ensure!(path.is_absolute(), "Output path must be absolute");
    ensure!(
        is_output_document(path),
        "This file type does not support output history"
    );
    if let Ok(metadata) = fs::symlink_metadata(path) {
        ensure!(
            !metadata.file_type().is_symlink(),
            "Output history does not follow symbolic links"
        );
    }
    let resolved = match path.canonicalize() {
        Ok(path) => path,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let parent = path
                .parent()
                .ok_or_else(|| anyhow::anyhow!("Output has no parent"))?;
            parent.canonicalize()?.join(
                path.file_name()
                    .ok_or_else(|| anyhow::anyhow!("Output has no filename"))?,
            )
        }
        Err(error) => return Err(error.into()),
    };
    let roots = if let Some(context) = &session.workspace_context {
        let policy = context.effective_folder_policy();
        if write {
            for root in &policy.roots {
                if root.access == WorkspaceFolderAccess::Read {
                    if let Ok(root) = Path::new(&root.path).canonicalize() {
                        ensure!(!resolved.starts_with(root), "Output folder is read-only");
                    }
                }
            }
        }
        policy
            .roots
            .into_iter()
            .map(|root| PathBuf::from(root.path))
            .collect::<Vec<_>>()
    } else {
        std::iter::once(session.working_dir.clone())
            .chain(session.additional_working_dirs.clone())
            .collect()
    };
    ensure!(
        roots
            .iter()
            .filter_map(|root| root.canonicalize().ok())
            .any(|root| resolved.starts_with(root)),
        "Output path is outside this session's folders"
    );
    Ok(resolved)
}

pub(crate) fn read_snapshot(path: &Path) -> Result<Option<OutputSnapshot>> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink(),
        "Output must be a regular file"
    );
    ensure!(
        metadata.len() <= MAX_OUTPUT_REVISION_BYTES as u64,
        OutputRevisionError::Limit("Output exceeds the 8 MiB revision limit".into())
    );
    ensure!(
        path.canonicalize()? == path,
        "Output path changed while reading history"
    );
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options.open(path)?;
    ensure!(file.metadata()?.is_file(), "Output must be a regular file");
    let mut bytes = Vec::new();
    file.take(MAX_OUTPUT_REVISION_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        path.canonicalize()? == path,
        "Output path changed while reading history"
    );
    ensure!(
        bytes.len() <= MAX_OUTPUT_REVISION_BYTES,
        OutputRevisionError::Limit("Output exceeds the 8 MiB revision limit".into())
    );
    let body = markdown_body(path, &bytes);
    Ok(Some(OutputSnapshot {
        hash: digest(&bytes),
        bytes,
        body,
    }))
}

pub(crate) fn markdown_body(path: &Path, bytes: &[u8]) -> Vec<u8> {
    static FOOTER_START: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"\r?\n[ \t]*\r?\n[ \t]*<!-- gosling:output-history:start -->[ \t]*\r?\n")
            .unwrap()
    });
    if is_markdown(path) {
        if let Ok(text) = std::str::from_utf8(bytes) {
            if let Some(footer) = FOOTER_START.find_iter(text).last() {
                if text
                    .split_at(footer.end())
                    .1
                    .trim_end()
                    .strip_suffix(HISTORY_END.trim_end())
                    .is_some_and(|history| history.trim_end_matches([' ', '\t']).ends_with('\n'))
                {
                    // Match editor-formatted delimiters without normalizing the document body.
                    return bytes[..footer.start()].to_vec();
                }
            }
        }
    }
    bytes.to_vec()
}

fn is_markdown(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("md") || ext.eq_ignore_ascii_case("markdown"))
}

fn table_cell(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('|', "&#124;")
        .replace(['\n', '\r'], " ")
}

pub(crate) fn annotated_snapshot(
    path: &Path,
    body: &[u8],
    revisions: &[OutputRevisionDto],
) -> Vec<u8> {
    if !is_markdown(path) || std::str::from_utf8(body).is_err() {
        return body.to_vec();
    }
    let mut result = body.to_vec();
    let mut footer = String::from(HISTORY_START);
    footer.push_str("## Output contribution history\n\nRecorded by gosling. Observed changes identify the running agent; they are not proof of exclusive authorship.\n\n| Revision | Recorded (UTC) | Agent | Provider / selected model | Resolved model | Action | Attribution | Chat |\n| --- | --- | --- | --- | --- | --- | --- | --- |\n");
    for revision in revisions {
        let contributor = &revision.contributor;
        let attribution = match revision.attribution {
            OutputAttributionKind::Tool => "Tool write",
            OutputAttributionKind::Observed => "Observed during tool",
            OutputAttributionKind::Unknown => "Unknown",
            OutputAttributionKind::User => "User restore",
        };
        footer.push_str(&format!(
            "| v{} | {} | {} | {} / {} | {} | {:?} | {} | {} ({}) |\n",
            revision.version,
            table_cell(&revision.recorded_at),
            table_cell(&contributor.agent),
            table_cell(contributor.provider.as_deref().unwrap_or("unknown")),
            table_cell(contributor.selected_model.as_deref().unwrap_or("unknown")),
            table_cell(contributor.resolved_model.as_deref().unwrap_or("unknown")),
            revision.action,
            attribution,
            table_cell(&contributor.session_name),
            table_cell(&contributor.session_id)
        ));
    }
    footer.push_str(HISTORY_END);
    result.extend_from_slice(footer.as_bytes());
    result
}

pub(crate) struct PreparedOutputReplacement {
    temporary: tempfile::NamedTempFile,
    session: Session,
    path: PathBuf,
    expected_hash: String,
}

impl PreparedOutputReplacement {
    pub(crate) fn persist(self) -> Result<()> {
        check_replacement_target(&self.session, &self.path, &self.expected_hash)?;
        self.temporary.persist(&self.path)?;
        Ok(())
    }
}

fn check_replacement_target(session: &Session, path: &Path, expected_hash: &str) -> Result<()> {
    ensure!(
        canonical_output_path(session, path, true)? == path,
        OutputRevisionError::Conflict("Output path changed while recording history".into())
    );
    let current = read_snapshot(path)?
        .ok_or_else(|| OutputRevisionError::NotFound("Output no longer exists".into()))?;
    ensure!(
        current.hash == expected_hash,
        OutputRevisionError::Conflict(
            "Output changed; refresh its history before restoring".into()
        )
    );
    Ok(())
}

/// Stage and sync bytes without touching the live file, so SQLite can commit first.
pub(crate) fn prepare_replacement(
    session: &Session,
    path: &Path,
    expected_hash: &str,
    bytes: &[u8],
) -> Result<PreparedOutputReplacement> {
    check_replacement_target(session, path, expected_hash)?;
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Output has no parent"))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary
        .as_file()
        .set_permissions(fs::metadata(path)?.permissions())?;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    check_replacement_target(session, path, expected_hash)?;
    Ok(PreparedOutputReplacement {
        temporary,
        session: session.clone(),
        path: path.to_owned(),
        expected_hash: expected_hash.into(),
    })
}

pub(crate) fn replace_if_unchanged(
    session: &Session,
    path: &Path,
    expected_hash: &str,
    bytes: &[u8],
) -> Result<()> {
    prepare_replacement(session, path, expected_hash, bytes)?.persist()
}

pub(crate) fn scan_output_roots(roots: &[PathBuf]) -> (BTreeSet<PathBuf>, Vec<String>) {
    let mut warnings = Vec::new();
    let mut files = BTreeSet::new();
    let mut pending: Vec<_> = roots.iter().cloned().map(|root| (root, 0)).collect();
    let mut visited = 0;
    while let Some((directory, depth)) = pending.pop() {
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) => {
                warnings.push(format!("{}: {error}", directory.display()));
                continue;
            }
        };
        for entry in entries {
            visited += 1;
            if visited > 2000 {
                warnings.push("Output observation stopped at 2000 directory entries".into());
                return (files, warnings);
            }
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    warnings.push(error.to_string());
                    continue;
                }
            };
            let kind = match entry.file_type() {
                Ok(kind) => kind,
                Err(error) => {
                    warnings.push(error.to_string());
                    continue;
                }
            };
            if kind.is_symlink() {
                continue;
            }
            if kind.is_dir() && depth < 4 && !entry.file_name().to_string_lossy().starts_with('.') {
                pending.push((entry.path(), depth + 1));
            } else if kind.is_dir()
                && depth >= 4
                && !entry.file_name().to_string_lossy().starts_with('.')
            {
                warnings.push(format!(
                    "{}: Output observation stopped at four directory levels",
                    entry.path().display()
                ));
            } else if kind.is_file() && is_output_document(&entry.path()) {
                if files.len() == 200 {
                    warnings.push("Output observation stopped at 200 files".into());
                    return (files, warnings);
                }
                files.insert(entry.path());
            }
        }
    }
    (files, warnings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_footer_parsing_preserves_body_bytes_and_earlier_markers() {
        let body = "# Résumé\r\n\r\n<!-- gosling:output-history:start -->\r\nExample, not the final footer.\r\n";
        for newline in ["\n", "\r\n"] {
            for trailing in ["", "  ", "\n\n", " \r\n\t"] {
                let footer = format!("{newline} \t{newline}<!-- gosling:output-history:start -->  {newline}Table{newline}  <!-- gosling:output-history:end -->{trailing}");
                let text = format!("{body}{footer}");
                assert_eq!(
                    markdown_body(Path::new("report.md"), text.as_bytes()),
                    body.as_bytes()
                );
                assert_eq!(
                    markdown_body(Path::new("report.txt"), text.as_bytes()),
                    text.as_bytes()
                );
            }
        }
    }

    #[test]
    fn incomplete_or_nonterminal_history_is_document_content() {
        for text in [
            "Body\n\n<!-- gosling:output-history:start -->\nUnfinished",
            "Body\n\n<!-- gosling:output-history:start -->\nTable\n<!-- gosling:output-history:end -->\nUser appendix",
            "Body\n\n<!-- gosling:output-history:start -->\nTable <!-- gosling:output-history:end -->\n",
        ] {
            assert_eq!(markdown_body(Path::new("report.md"), text.as_bytes()), text.as_bytes());
        }
    }
}
