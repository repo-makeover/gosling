use crate::config::GoslingMode;
use crate::conversation::message::{Message, ToolRequest};
use crate::session::SessionManager;
use crate::tool_inspection::{InspectionAction, InspectionResult, ToolInspector};
use crate::workspace::{WorkspaceFolderAccess, WorkspaceFolderPolicy};
use anyhow::Result;
use async_trait::async_trait;
use rmcp::model::CallToolRequestParams;
use std::ffi::OsString;
use std::io::ErrorKind;
use std::path::Component;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Enforces the session's pinned filesystem boundary. Ordinary sessions opt
/// in through `Session::restrict_tools_to_working_dirs`; workspace sessions
/// always enforce their saved folder policy. With the restriction on, every
/// out-of-scope path requires approval. A workspace session with it off still
/// requires approval for out-of-scope mutations except temporary scratch
/// paths, while out-of-scope reads pass. Mutations under read-only workspace
/// roots are denied outright.
pub struct WorkingDirScopeInspector {
    session_manager: Arc<SessionManager>,
}

impl WorkingDirScopeInspector {
    pub fn new(session_manager: Arc<SessionManager>) -> Self {
        Self { session_manager }
    }
}

#[async_trait]
impl ToolInspector for WorkingDirScopeInspector {
    fn name(&self) -> &'static str {
        "working_dir_scope"
    }

    async fn inspect(
        &self,
        session_id: &str,
        tool_requests: &[ToolRequest],
        _messages: &[Message],
        _gosling_mode: GoslingMode,
    ) -> Result<Vec<InspectionResult>> {
        let session = self.session_manager.get_session(session_id, false).await?;
        if !session.restrict_tools_to_working_dirs && session.workspace_context.is_none() {
            return Ok(Vec::new());
        }

        let mut allowed_dirs = Vec::with_capacity(1 + session.additional_working_dirs.len());
        allowed_dirs.push(session.working_dir.clone());
        allowed_dirs.extend(session.additional_working_dirs.iter().cloned());
        let scratch_dirs = if session.restrict_tools_to_working_dirs {
            Vec::new()
        } else {
            temporary_scratch_dirs()
        };

        let mut results = Vec::new();
        for request in tool_requests {
            let Ok(tool_call) = &request.tool_call else {
                continue;
            };
            if let Some(context) = &session.workspace_context {
                let policy = context.effective_folder_policy();
                if is_mutating_tool_call(tool_call) {
                    if is_shell_tool(tool_call)
                        && policy
                            .roots
                            .iter()
                            .any(|root| root.access == WorkspaceFolderAccess::Read)
                    {
                        results.push(InspectionResult {
                            tool_request_id: request.id.clone(),
                            action: InspectionAction::Deny,
                            reason: "mutating shell commands cannot be safely scoped while the workspace has read-only roots; use a structured file tool or a workspace without read-only folders".to_string(),
                            confidence: 1.0,
                            inspector_name: self.name().to_string(),
                            finding_id: Some("AUD-GOS-003".to_string()),
                            metadata: None,
                        });
                        continue;
                    }
                    if let Some(path) =
                        first_read_only_path(tool_call, &session.working_dir, &policy)?
                    {
                        results.push(InspectionResult {
                            tool_request_id: request.id.clone(),
                            action: InspectionAction::Deny,
                            reason: format!(
                                "workspace folder policy forbids mutation under {}",
                                path.display()
                            ),
                            confidence: 1.0,
                            inspector_name: self.name().to_string(),
                            finding_id: Some("AUD-GOS-003".to_string()),
                            metadata: None,
                        });
                        continue;
                    }
                }
            }
            if is_shell_tool(tool_call)
                && tool_call
                    .arguments
                    .as_ref()
                    .and_then(|args| args.get("command"))
                    .and_then(|value| value.as_str())
                    .is_some_and(|command| !analyze_shell(command).complete)
            {
                let reason = "Shell syntax could not be fully inspected against this session's folders. Split the command into simpler calls or approve this call once.".to_string();
                results.push(InspectionResult {
                    tool_request_id: request.id.clone(),
                    action: InspectionAction::RequireApproval(Some(reason.clone())),
                    reason,
                    confidence: 1.0,
                    inspector_name: self.name().to_string(),
                    finding_id: Some("SEC-GSL-901".to_string()),
                    metadata: None,
                });
                continue;
            }
            let candidate_paths = if session.restrict_tools_to_working_dirs {
                referenced_paths(tool_call, &session.working_dir)
            } else {
                mutation_paths(tool_call, &session.working_dir)
            };
            let Some(path) = out_of_scope_path(&candidate_paths, &allowed_dirs, &scratch_dirs)?
            else {
                continue;
            };

            let dirs_list = allowed_dirs
                .iter()
                .map(|d| d.display().to_string())
                .collect::<Vec<_>>()
                .join(", ");
            let message = if session.restrict_tools_to_working_dirs {
                format!(
                    "\"{}\" touches {}, which is outside your working directories ({}). \
                     This session has \"restrict tools to working directories\" turned on.",
                    tool_call.name,
                    path.display(),
                    dirs_list
                )
            } else {
                format!(
                    "\"{}\" would modify {}, which is outside this workspace session's \
                     folders ({}). Add that folder to the session to allow it without approval.",
                    tool_call.name,
                    path.display(),
                    dirs_list
                )
            };
            results.push(InspectionResult {
                tool_request_id: request.id.clone(),
                action: InspectionAction::RequireApproval(Some(message)),
                reason: "path outside configured working directories".to_string(),
                confidence: 1.0,
                inspector_name: self.name().to_string(),
                finding_id: None,
                metadata: None,
            });
        }
        Ok(results)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn auto_downgrades_require_approval(&self) -> bool {
        false
    }
}

fn normalize_resolved_path(path: PathBuf) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Normal(segment) => normalized.push(segment),
        }
    }
    normalized
}

fn canonicalize_potential_path(path: &Path) -> Result<PathBuf> {
    let mut existing_ancestor = path.to_path_buf();
    let mut missing_segments: Vec<OsString> = Vec::new();

    loop {
        match std::fs::canonicalize(&existing_ancestor) {
            Ok(canonical_ancestor) => {
                missing_segments.reverse();
                let resolved = missing_segments
                    .into_iter()
                    .fold(canonical_ancestor, |path, segment| path.join(segment));
                return Ok(normalize_resolved_path(resolved));
            }
            Err(error) if error.kind() == ErrorKind::NotFound => {
                match std::fs::symlink_metadata(&existing_ancestor) {
                    Ok(metadata) if metadata.file_type().is_symlink() => {
                        anyhow::bail!(
                            "cannot authorize path through dangling symbolic link: {}",
                            path.display()
                        );
                    }
                    Ok(_) => return Err(error.into()),
                    Err(metadata_error) if metadata_error.kind() == ErrorKind::NotFound => {}
                    Err(metadata_error) => return Err(metadata_error.into()),
                }

                let Some(name) = existing_ancestor.file_name().map(OsString::from) else {
                    return Err(error.into());
                };
                let Some(parent) = existing_ancestor.parent() else {
                    return Err(error.into());
                };
                missing_segments.push(name);
                existing_ancestor = parent.to_path_buf();
            }
            Err(error) => return Err(error.into()),
        }
    }
}

fn canonical_allowed_dirs(dirs: &[PathBuf]) -> Vec<PathBuf> {
    dirs.iter()
        .filter_map(|dir| canonicalize_potential_path(dir).ok())
        .collect()
}

fn temporary_scratch_dirs() -> Vec<PathBuf> {
    let dirs = vec![std::env::temp_dir()];
    // macOS tools also use /tmp even when TMPDIR points to a per-user directory.
    #[cfg(unix)]
    let dirs = dirs
        .into_iter()
        .chain(["/tmp", "/var/tmp"].map(PathBuf::from))
        .collect::<Vec<_>>();
    canonical_allowed_dirs(&dirs)
        .into_iter()
        .filter(|dir| dir.parent().is_some())
        .collect()
}

fn is_within_any(path: &Path, dirs: &[PathBuf]) -> Result<bool> {
    let canonical_path = canonicalize_potential_path(path)?;
    let canonical_dirs = canonical_allowed_dirs(dirs);
    if canonical_dirs.is_empty() {
        anyhow::bail!("no working directory could be canonicalized");
    }
    Ok(canonical_dirs
        .iter()
        .any(|dir| canonical_path.starts_with(dir)))
}

fn resolve(value: &str, working_dir: &Path) -> PathBuf {
    if let Ok(url) = url::Url::parse(value) {
        if url.scheme() == "file" {
            if let Ok(path) = url.to_file_path() {
                return path;
            }
        }
    }
    for prefix in ["~/", "$HOME/", "${HOME}/"] {
        if let Some(relative) = value.strip_prefix(prefix) {
            if let Some(home) = dirs::home_dir() {
                return home.join(relative);
            }
        }
    }
    let path = Path::new(value);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        working_dir.join(path)
    }
}

fn argument_key_tokens(key: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut token = String::new();
    let mut previous_was_lowercase = false;
    for character in key.chars() {
        if !character.is_ascii_alphanumeric() {
            if !token.is_empty() {
                tokens.push(std::mem::take(&mut token));
            }
            previous_was_lowercase = false;
            continue;
        }
        if character.is_ascii_uppercase() && previous_was_lowercase && !token.is_empty() {
            tokens.push(std::mem::take(&mut token));
        }
        token.push(character.to_ascii_lowercase());
        previous_was_lowercase = character.is_ascii_lowercase();
    }
    if !token.is_empty() {
        tokens.push(token);
    }
    tokens
}

fn argument_key_has_path_semantics(key: &str) -> bool {
    argument_key_tokens(key).iter().any(|token| {
        matches!(
            token.as_str(),
            "path"
                | "paths"
                | "file"
                | "files"
                | "filename"
                | "filenames"
                | "directory"
                | "directories"
                | "dir"
                | "dirs"
                | "folder"
                | "folders"
                | "root"
                | "roots"
                | "cwd"
                | "uri"
                | "uris"
        )
    })
}

fn argument_key_is_text_payload(key: &str) -> bool {
    argument_key_tokens(key).iter().any(|token| {
        matches!(
            token.as_str(),
            "body" | "content" | "prompt" | "query" | "replacement" | "template" | "text"
        )
    })
}

fn looks_like_explicit_path(value: &str) -> bool {
    let bytes = value.as_bytes();
    value.starts_with('/')
        || value.starts_with("./")
        || value.starts_with("../")
        || value.starts_with("~/")
        || value.starts_with("$HOME/")
        || value.starts_with("${HOME}/")
        || value.starts_with("file://")
        || value.starts_with('\\')
        || value.starts_with(".\\")
        || value.starts_with("..\\")
        || (bytes.len() >= 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && matches!(bytes[2], b'/' | b'\\'))
}

fn path_from_shell_token(token: &str) -> Option<&str> {
    let candidate = if token.starts_with('-') {
        token.split_once('=')?.1
    } else if let Some((_, value)) = token.split_once('=') {
        if looks_like_explicit_path(value) {
            value
        } else {
            token
        }
    } else {
        token
    };
    let candidate = candidate.trim_start_matches(|character: char| {
        character.is_ascii_digit() || matches!(character, '<' | '>' | '&')
    });
    (looks_like_explicit_path(candidate) && !is_device_stream(Path::new(candidate)))
        .then_some(candidate)
}

/// Kernel pseudo-files that stand in for the process's own streams or a
/// discard sink. Redirecting to them changes nothing on disk, so they never
/// count as touching a path; raw disks and other device nodes still do.
fn is_device_stream(path: &Path) -> bool {
    let mut components = path.components();
    if components.next() != Some(Component::RootDir)
        || components.next() != Some(Component::Normal("dev".as_ref()))
    {
        return false;
    }
    match components.next() {
        Some(Component::Normal(name)) => {
            let rest = components.next();
            match name.to_str() {
                Some(
                    "null" | "zero" | "random" | "urandom" | "stdin" | "stdout" | "stderr" | "tty",
                ) => rest.is_none(),
                Some("fd") => matches!(rest, Some(Component::Normal(descriptor))
                    if descriptor.to_str().is_some_and(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit()))
                    && components.next().is_none()),
                _ => false,
            }
        }
        _ => false,
    }
}

fn collect_referenced_paths(
    value: &serde_json::Value,
    key: &str,
    inherited_path_semantics: bool,
    working_dir: &Path,
    paths: &mut Vec<PathBuf>,
) {
    let path_semantics = inherited_path_semantics || argument_key_has_path_semantics(key);
    match value {
        serde_json::Value::String(value) => {
            if path_semantics
                || (!argument_key_is_text_payload(key) && looks_like_explicit_path(value))
            {
                let path = resolve(value, working_dir);
                if !is_device_stream(&path) {
                    paths.push(path);
                }
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                collect_referenced_paths(value, key, path_semantics, working_dir, paths);
            }
        }
        serde_json::Value::Object(values) => {
            for (nested_key, value) in values {
                collect_referenced_paths(value, nested_key, false, working_dir, paths);
            }
        }
        _ => {}
    }
}

// Nested `sh -c` calls and shell-received heredocs share this analysis limit.
const MAX_SHELL_ANALYSIS_DEPTH: usize = 16;

struct ShellSegment {
    words: Vec<String>,
    read_only: bool,
}

struct ShellAnalysis {
    segments: Vec<ShellSegment>,
    complete: bool,
}

fn analyze_shell(command: &str) -> ShellAnalysis {
    analyze_shell_at_depth(command, 0)
}

fn analyze_shell_at_depth(command: &str, depth: usize) -> ShellAnalysis {
    let mut analysis = ShellAnalysis {
        segments: Vec::new(),
        complete: false,
    };
    if depth >= MAX_SHELL_ANALYSIS_DEPTH {
        return analysis;
    }
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_bash::LANGUAGE.into())
        .expect("Bash grammar must match the parser");
    let Some(tree) = parser.parse(command, None) else {
        return analysis;
    };
    let spliced = splice_shell_continuations(command, tree.root_node());
    let command = spliced.as_deref().unwrap_or(command);
    let tree = if spliced.is_some() {
        let Some(tree) = parser.parse(command, None) else {
            return analysis;
        };
        tree
    } else {
        tree
    };
    analysis.complete = !tree.root_node().has_error();
    let mut pending_nodes = vec![tree.root_node()];
    while let Some(node) = pending_nodes.pop() {
        match node.kind() {
            "command" => {
                let words = parsed_command_words(node, command);
                let executable_words = unwrap_command_words(&words);
                if let Some(executable) = executable_words.first() {
                    let executable = Path::new(executable)
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or_default();
                    if matches!(executable, "sh" | "bash" | "zsh" | "dash" | "ksh") {
                        if let Some(command_flag_index) = executable_words
                            .iter()
                            .position(|word| word.starts_with('-') && word.contains('c'))
                        {
                            if let Some(script) = executable_words.get(command_flag_index + 1) {
                                let nested = analyze_shell_at_depth(script, depth + 1);
                                analysis.complete &= nested.complete;
                                analysis.segments.extend(nested.segments);
                            } else {
                                analysis.complete = false;
                            }
                        }
                    }
                    // env -S has its own expansion grammar; never silently treat it
                    // as a read-only environment listing.
                    if executable == "env"
                        && executable_words
                            .iter()
                            .any(|word| word == "-S" || word.starts_with("--split-string"))
                    {
                        analysis.complete = false;
                    }
                }
                let read_only = shell_segment_is_read_only(&words);
                analysis.segments.push(ShellSegment { words, read_only });
            }
            "file_redirect" => {
                let raw = node.utf8_text(command.as_bytes()).unwrap_or_default();
                match shell_words::split(raw) {
                    Ok(mut words) => {
                        let read_only = !(0..words.len())
                            .any(|index| redirects_output_to_file(&words, index))
                            && !raw.contains("<>");
                        // Path collection skips the executable slot; redirects have none.
                        words.insert(0, String::new());
                        analysis.segments.push(ShellSegment { words, read_only });
                    }
                    Err(_) => analysis.complete = false,
                }
            }
            "heredoc_redirect" => {
                // Quoted bodies are data unless the receiving command is a shell.
                // Unquoted command substitutions are independently visited below.
                let receiver = node
                    .parent()
                    .and_then(|parent| parent.child_by_field_name("body"));
                if receiver.is_some_and(|body| is_shell_receiver(body, command)) {
                    let mut cursor = node.walk();
                    let body = node
                        .named_children(&mut cursor)
                        .find(|child| child.kind() == "heredoc_body");
                    if let Some(body) = body {
                        let nested = analyze_shell_at_depth(
                            body.utf8_text(command.as_bytes()).unwrap_or_default(),
                            depth + 1,
                        );
                        analysis.complete &= nested.complete;
                        analysis.segments.extend(nested.segments);
                    }
                }
            }
            _ => {}
        }
        let mut cursor = node.walk();
        let children: Vec<_> = node.named_children(&mut cursor).collect();
        // The stack is LIFO; reverse children to preserve source order.
        pending_nodes.extend(children.into_iter().rev());
    }
    analysis
}

fn splice_shell_continuations(command: &str, root: tree_sitter::Node<'_>) -> Option<String> {
    if !command.contains("\\\n") {
        return None;
    }
    // Bash removes continuations before identifying word/comment boundaries.
    // The grammar treats a hash after a continuation as a fresh comment, so
    // splice executable source while retaining literal strings and heredoc data.
    let mut literal_ranges = Vec::new();
    let mut pending_nodes = vec![root];
    while let Some(node) = pending_nodes.pop() {
        if matches!(node.kind(), "raw_string" | "heredoc_body" | "comment") {
            literal_ranges.push(node.byte_range());
        } else {
            let mut cursor = node.walk();
            pending_nodes.extend(node.named_children(&mut cursor));
        }
    }
    literal_ranges.sort_by_key(|range| range.start);
    let source_bytes = command.as_bytes();
    let mut spliced_bytes = Vec::with_capacity(source_bytes.len());
    let mut byte_index = 0;
    let mut remaining_literal_ranges = literal_ranges.iter().peekable();
    while byte_index < source_bytes.len() {
        while remaining_literal_ranges
            .peek()
            .is_some_and(|range| range.end <= byte_index)
        {
            remaining_literal_ranges.next();
        }
        if remaining_literal_ranges
            .peek()
            .is_some_and(|range| range.contains(&byte_index))
        {
            spliced_bytes.push(source_bytes[byte_index]);
            byte_index += 1;
        } else if source_bytes[byte_index] == b'\\' && byte_index + 1 < source_bytes.len() {
            if source_bytes[byte_index + 1] != b'\n' {
                spliced_bytes.extend_from_slice(&source_bytes[byte_index..byte_index + 2]);
            }
            byte_index += 2;
        } else {
            spliced_bytes.push(source_bytes[byte_index]);
            byte_index += 1;
        }
    }
    (spliced_bytes != source_bytes)
        .then(|| String::from_utf8(spliced_bytes).expect("Removing ASCII preserves UTF-8"))
}

fn parsed_command_words(node: tree_sitter::Node<'_>, command: &str) -> Vec<String> {
    let mut words = Vec::new();
    if let Some(name) = node.child_by_field_name("name") {
        words.push(shell_word(name, command));
    }
    let mut cursor = node.walk();
    for argument in node.children_by_field_name("argument", &mut cursor) {
        words.push(shell_word(argument, command));
    }
    words
}

fn is_shell_receiver(mut node: tree_sitter::Node<'_>, command: &str) -> bool {
    while node.kind() == "pipeline" {
        let Some(last) = node.named_child(node.named_child_count().saturating_sub(1) as u32) else {
            return false;
        };
        node = last;
    }
    let words = parsed_command_words(node, command);
    unwrap_command_words(&words).first().is_some_and(|name| {
        matches!(
            Path::new(name).file_name().and_then(|name| name.to_str()),
            Some("sh" | "bash" | "zsh" | "dash" | "ksh")
        )
    })
}

fn shell_word(node: tree_sitter::Node<'_>, command: &str) -> String {
    let raw = node.utf8_text(command.as_bytes()).unwrap_or_default();
    match shell_words::split(raw) {
        Ok(words) if words.len() == 1 => words.into_iter().next().unwrap(),
        _ => raw.to_string(),
    }
}

/// Unwrap commands that forward their arguments to another executable.
/// Unsupported `env` options preserve the original words for conservative classification.
fn unwrap_command_words(segment: &[String]) -> &[String] {
    let mut words = segment;
    while let Some((first, rest)) = words.split_first() {
        match Path::new(first)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
        {
            "env" => {
                words = rest;
                while let Some((first, rest)) = words.split_first() {
                    match first.as_str() {
                        "-i" | "--ignore-environment" | "--" => words = rest,
                        "-u" | "--unset" | "-C" | "--chdir" => {
                            words = rest.get(1..).unwrap_or_default()
                        }
                        word if word.starts_with("--unset=")
                            || word.starts_with("--chdir=")
                            || (!word.starts_with('-') && word.contains('=')) =>
                        {
                            words = rest
                        }
                        word if !word.starts_with('-') => break,
                        _ => return segment,
                    }
                }
            }
            "command" | "builtin" | "exec" => {
                words = rest;
                if words
                    .first()
                    .is_some_and(|word| word == "--" || word == "-p")
                {
                    words = &words[1..];
                }
            }
            _ => return words,
        }
    }
    words
}

fn collect_shell_segment_paths(segment: &[String], working_dir: &Path, paths: &mut Vec<PathBuf>) {
    for token in segment.iter().skip(1) {
        if let Some(path) = path_from_shell_token(token) {
            paths.push(resolve(path, working_dir));
        } else if let Some(path) = embedded_redirect_target(token) {
            paths.push(resolve(path, working_dir));
        }
    }
}

/// A redirection target quoted inside a program argument, such as
/// `awk '{print > "/tmp/out"}'`, which word splitting hides in one token.
fn embedded_redirect_target(token: &str) -> Option<&str> {
    let (_, target) = token.rsplit_once('>')?;
    let target = target
        .trim()
        .trim_matches(|character: char| matches!(character, '"' | '\'' | '}' | ')' | ';'));
    (looks_like_explicit_path(target) && !is_device_stream(Path::new(target))).then_some(target)
}

fn referenced_paths(tool_call: &CallToolRequestParams, working_dir: &Path) -> Vec<PathBuf> {
    let Some(args) = tool_call.arguments.as_ref() else {
        return Vec::new();
    };
    let mut paths = Vec::new();
    for (key, value) in args {
        if key != "command" {
            collect_referenced_paths(value, key, false, working_dir, &mut paths);
        }
    }
    if let Some(command) = args.get("command").and_then(|value| value.as_str()) {
        for segment in analyze_shell(command).segments {
            collect_shell_segment_paths(&segment.words, working_dir, &mut paths);
        }
    }
    paths
}

/// Paths a call may change. Shell pipelines are judged per segment so a
/// read-only `cat` of reference material does not count against a later
/// segment that only writes inside the session's folders.
fn mutation_paths(tool_call: &CallToolRequestParams, working_dir: &Path) -> Vec<PathBuf> {
    if !is_shell_tool(tool_call) {
        return if is_mutating_tool_call(tool_call) {
            referenced_paths(tool_call, working_dir)
        } else {
            Vec::new()
        };
    }
    let Some(command) = tool_call
        .arguments
        .as_ref()
        .and_then(|args| args.get("command"))
        .and_then(|value| value.as_str())
    else {
        return Vec::new();
    };
    let mut paths = Vec::new();
    for segment in analyze_shell(command).segments {
        if !segment.read_only {
            collect_shell_segment_paths(&segment.words, working_dir, &mut paths);
        }
    }
    paths
}

fn first_read_only_path(
    tool_call: &CallToolRequestParams,
    working_dir: &Path,
    policy: &WorkspaceFolderPolicy,
) -> Result<Option<PathBuf>> {
    let canonical_roots = policy
        .roots
        .iter()
        .filter_map(|root| {
            canonicalize_potential_path(Path::new(&root.path))
                .ok()
                .map(|path| (path, root.access))
        })
        .collect::<Vec<_>>();
    for path in referenced_paths(tool_call, working_dir) {
        let canonical_path = canonicalize_potential_path(&path)?;
        let access = canonical_roots
            .iter()
            .filter(|(root, _)| canonical_path.starts_with(root))
            .max_by_key(|(root, _)| root.components().count())
            .map(|(_, access)| *access);
        if access == Some(WorkspaceFolderAccess::Read) {
            return Ok(Some(canonical_path));
        }
    }
    Ok(None)
}

fn is_mutating_tool_call(tool_call: &CallToolRequestParams) -> bool {
    let name = tool_call.name.to_ascii_lowercase();
    let operation = name.rsplit("__").next().unwrap_or(&name);
    let mutation_markers = [
        "write", "edit", "delete", "remove", "create", "mkdir", "move", "copy", "rename", "patch",
        "replace", "append", "truncate", "chmod", "chown", "upload", "save",
    ];
    if mutation_markers
        .iter()
        .any(|marker| operation.contains(marker))
    {
        return true;
    }

    if is_shell_tool(tool_call) {
        let command = tool_call
            .arguments
            .as_ref()
            .and_then(|args| args.get("command"))
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        return !is_confidently_read_only_shell(command);
    }

    let read_markers = [
        "read", "list", "search", "find", "grep", "query", "get", "inspect", "view", "stat",
        "preview", "fetch",
    ];
    !read_markers.iter().any(|marker| operation.contains(marker))
}

fn is_shell_tool(tool_call: &CallToolRequestParams) -> bool {
    crate::permission::tool_class::is_code_execution_tool(&tool_call.name)
}

fn is_confidently_read_only_shell(command: &str) -> bool {
    let analysis = analyze_shell(command);
    analysis.complete && analysis.segments.iter().all(|segment| segment.read_only)
}

/// Whether `token` opens an output redirection (`>`, `>>`, `2>`, `&>`, and
/// their glued forms) that lands somewhere other than a device stream.
fn redirects_output_to_file(segment: &[String], index: usize) -> bool {
    let token = segment[index].as_str();
    let operator_start = token
        .find(|character: char| !character.is_ascii_digit() && character != '&')
        .unwrap_or(token.len());
    let (descriptor, rest) = token.split_at(operator_start);
    if descriptor.contains('&') && !descriptor.starts_with('&') {
        return false;
    }
    let Some(target) = rest
        .strip_prefix(">>")
        .or_else(|| rest.strip_prefix(">|"))
        .or_else(|| rest.strip_prefix('>'))
    else {
        return false;
    };
    let target = if target.is_empty() {
        segment
            .get(index + 1)
            .map(String::as_str)
            .unwrap_or_default()
    } else {
        target
    };
    if let Some(duplicated) = target.strip_prefix('&') {
        return !duplicated.bytes().all(|byte| byte.is_ascii_digit());
    }
    !is_device_stream(Path::new(target))
}

fn shell_segment_is_read_only(segment: &[String]) -> bool {
    let words = unwrap_command_words(segment);
    let Some(executable) = words.first() else {
        return true;
    };
    let executable = Path::new(executable)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    match executable {
        "cat" | "ls" | "pwd" | "rg" | "grep" | "head" | "tail" | "wc" | "stat" | "file"
        | "echo" | "printf" | "test" | "[" | "true" | "false" | "ps" | "lsof" | "which"
        | "type" | "printenv" | "date" | "diff" | "du" | "df" | "basename" | "dirname"
        | "realpath" | "readlink" | "jq" | "md5" | "md5sum" | "shasum" | "sha256sum" | "cut"
        | "tr" | "whoami" | "uname" | "hostname" | "id" => true,
        "sort" => !words
            .iter()
            .any(|token| token == "-o" || token.starts_with("-o") || token.starts_with("--output")),
        "awk" | "gawk" | "mawk" | "nawk" => !words
            .iter()
            .any(|token| token.contains('>') || token == "-i" || token.starts_with("--in-place")),
        "find" => !words.iter().any(|token| {
            matches!(
                token.as_str(),
                "-delete" | "-exec" | "-execdir" | "-ok" | "-okdir"
            )
        }),
        "sed" => !words
            .iter()
            .any(|token| token == "-i" || token.starts_with("-i")),
        "git" => words.get(1).is_some_and(|subcommand| {
            matches!(
                subcommand.as_str(),
                "diff" | "grep" | "log" | "show" | "status"
            )
        }),
        _ => false,
    }
}

/// Returns the first out-of-scope path among the given resolved paths, if
/// any. Callers only pass explicit `path` arguments and explicit absolute or
/// relative shell paths; ambiguous path-free calls are left alone rather than
/// guessed at.
fn out_of_scope_path(
    paths: &[PathBuf],
    allowed_dirs: &[PathBuf],
    scratch_dirs: &[PathBuf],
) -> Result<Option<PathBuf>> {
    for resolved in paths {
        let canonical_path = canonicalize_potential_path(resolved)?;
        // Compare resolved targets so a temp symlink cannot exempt an outside write.
        // The temp directories themselves are not scratch entries.
        if !scratch_dirs.contains(&canonical_path)
            && scratch_dirs
                .iter()
                .any(|dir| canonical_path.starts_with(dir))
        {
            continue;
        }
        if !is_within_any(resolved, allowed_dirs)? {
            return Ok(Some(canonical_path));
        }
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::model::JsonObject;

    fn out_of_scope_path(
        tool_call: &CallToolRequestParams,
        working_dir: &Path,
        allowed_dirs: &[PathBuf],
    ) -> Result<Option<PathBuf>> {
        super::out_of_scope_path(&referenced_paths(tool_call, working_dir), allowed_dirs, &[])
    }

    // These paths are only inspected, never created; temp storage is a separate allowance.
    fn non_temporary_target(name: &str) -> PathBuf {
        let cwd = std::env::current_dir().unwrap();
        cwd.ancestors()
            .last()
            .unwrap()
            .join(format!("gosling-scope-regression-{}", uuid::Uuid::now_v7()))
            .join(name)
    }

    fn tool_call(name: &str, args: JsonObject) -> CallToolRequestParams {
        CallToolRequestParams::new(name.to_string()).with_arguments(args)
    }

    fn json_args(pairs: &[(&str, &str)]) -> JsonObject {
        pairs
            .iter()
            .map(|(k, v)| {
                (
                    (*k).to_string(),
                    serde_json::Value::String((*v).to_string()),
                )
            })
            .collect()
    }

    #[test]
    fn flags_path_outside_working_dirs() {
        let working_dir = PathBuf::from("/home/user/project");
        let allowed = vec![working_dir.clone()];
        let call = tool_call(
            "developer__text_editor__write",
            json_args(&[("path", "/etc/passwd")]),
        );

        let result = out_of_scope_path(&call, &working_dir, &allowed).unwrap();
        assert_eq!(
            result,
            Some(canonicalize_potential_path(Path::new("/etc/passwd")).unwrap())
        );
    }

    #[test]
    fn allows_path_inside_working_dir() {
        let working_dir = PathBuf::from("/home/user/project");
        let allowed = vec![working_dir.clone()];
        let call = tool_call(
            "developer__text_editor__write",
            json_args(&[("path", "/home/user/project/src/main.rs")]),
        );

        assert_eq!(
            out_of_scope_path(&call, &working_dir, &allowed).unwrap(),
            None
        );
    }

    #[test]
    fn allows_path_inside_additional_working_dir() {
        let working_dir = PathBuf::from("/home/user/project");
        let allowed = vec![working_dir.clone(), PathBuf::from("/home/user/other")];
        let call = tool_call(
            "developer__text_editor__write",
            json_args(&[("path", "/home/user/other/file.txt")]),
        );

        assert_eq!(
            out_of_scope_path(&call, &working_dir, &allowed).unwrap(),
            None
        );
    }

    #[test]
    fn allows_relative_path() {
        let working_dir = PathBuf::from("/home/user/project");
        let allowed = vec![working_dir.clone()];
        let call = tool_call(
            "developer__text_editor__write",
            json_args(&[("path", "src/main.rs")]),
        );

        assert_eq!(
            out_of_scope_path(&call, &working_dir, &allowed).unwrap(),
            None
        );
    }

    #[test]
    fn flags_relative_parent_traversal() {
        let root = tempfile::tempdir().unwrap();
        let working_dir = root.path().join("project");
        std::fs::create_dir(&working_dir).unwrap();
        let allowed = vec![working_dir.clone()];
        let call = tool_call(
            "developer__text_editor__write",
            json_args(&[("path", "../outside.txt")]),
        );

        assert_eq!(
            out_of_scope_path(&call, &working_dir, &allowed).unwrap(),
            Some(
                std::fs::canonicalize(root.path())
                    .unwrap()
                    .join("outside.txt")
            )
        );
    }

    #[test]
    fn flags_nested_path_aliases_and_explicit_paths_under_unknown_keys() {
        let root = tempfile::tempdir().unwrap();
        let working_dir = root.path().join("project");
        std::fs::create_dir(&working_dir).unwrap();
        let outside = root.path().join("outside.txt");
        let call = tool_call(
            "third_party__export",
            serde_json::from_value(serde_json::json!({
                "options": {
                    "outputFile": outside,
                    "secondaryTarget": "../also-outside.txt"
                }
            }))
            .unwrap(),
        );

        assert!(
            out_of_scope_path(&call, &working_dir, std::slice::from_ref(&working_dir))
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn checks_arrays_under_path_semantic_keys() {
        let root = tempfile::tempdir().unwrap();
        let working_dir = root.path().join("project");
        std::fs::create_dir(&working_dir).unwrap();
        let call = tool_call(
            "third_party__batch",
            serde_json::from_value(serde_json::json!({
                "input_files": ["inside.txt", "../outside.txt"]
            }))
            .unwrap(),
        );

        assert!(
            out_of_scope_path(&call, &working_dir, std::slice::from_ref(&working_dir))
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn does_not_treat_text_payloads_as_paths() {
        let root = tempfile::tempdir().unwrap();
        let working_dir = root.path().join("project");
        std::fs::create_dir(&working_dir).unwrap();
        let call = tool_call(
            "developer__text_editor__write",
            serde_json::from_value(serde_json::json!({
                "path": "inside.txt",
                "content": "/outside-looking prose"
            }))
            .unwrap(),
        );

        assert_eq!(
            out_of_scope_path(&call, &working_dir, std::slice::from_ref(&working_dir)).unwrap(),
            None
        );
    }

    #[cfg(unix)]
    #[test]
    fn flags_existing_and_missing_paths_through_symlink_escape() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().unwrap();
        let working_dir = root.path().join("project");
        let outside = root.path().join("outside");
        std::fs::create_dir(&working_dir).unwrap();
        std::fs::create_dir(&outside).unwrap();
        std::fs::write(outside.join("secret.txt"), "secret").unwrap();
        symlink(&outside, working_dir.join("redirect")).unwrap();
        let allowed = vec![working_dir.clone()];

        for path in ["redirect/secret.txt", "redirect/new.txt"] {
            let call = tool_call(
                "developer__text_editor__write",
                json_args(&[("path", path)]),
            );
            assert!(out_of_scope_path(&call, &working_dir, &allowed)
                .unwrap()
                .is_some());
        }
    }

    #[cfg(unix)]
    #[test]
    fn dangling_symlink_fails_closed() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().unwrap();
        let working_dir = root.path().join("project");
        std::fs::create_dir(&working_dir).unwrap();
        symlink(
            working_dir.join("missing-target"),
            working_dir.join("dangling"),
        )
        .unwrap();
        let call = tool_call(
            "developer__text_editor__write",
            json_args(&[("path", "dangling/new.txt")]),
        );

        assert!(
            out_of_scope_path(&call, &working_dir, std::slice::from_ref(&working_dir)).is_err()
        );
    }

    #[test]
    fn flags_shell_command_with_absolute_path_outside_scope() {
        let working_dir = PathBuf::from("/home/user/project");
        let allowed = vec![working_dir.clone()];
        let call = tool_call(
            "developer__shell",
            json_args(&[("command", "cat /etc/passwd")]),
        );

        let result = out_of_scope_path(&call, &working_dir, &allowed).unwrap();
        assert_eq!(
            result,
            Some(canonicalize_potential_path(Path::new("/etc/passwd")).unwrap())
        );
    }

    #[test]
    fn does_not_guess_at_relative_shell_command() {
        let working_dir = PathBuf::from("/home/user/project");
        let allowed = vec![working_dir.clone()];
        let call = tool_call("developer__shell", json_args(&[("command", "ls -la src")]));

        assert_eq!(
            out_of_scope_path(&call, &working_dir, &allowed).unwrap(),
            None
        );
    }

    #[test]
    fn flags_shell_command_with_explicit_relative_parent_path() {
        let root = tempfile::tempdir().unwrap();
        let working_dir = root.path().join("project");
        std::fs::create_dir(&working_dir).unwrap();
        let call = tool_call(
            "developer__shell",
            json_args(&[("command", "rm ../valuable.txt")]),
        );

        assert!(
            out_of_scope_path(&call, &working_dir, std::slice::from_ref(&working_dir))
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn flags_shell_home_expansion_outside_scope() {
        let working_dir = tempfile::tempdir().unwrap();
        let call = tool_call(
            "developer__shell",
            json_args(&[("command", "cat ~/.ssh/id_rsa")]),
        );

        assert!(out_of_scope_path(
            &call,
            working_dir.path(),
            std::slice::from_ref(&working_dir.path().to_path_buf())
        )
        .unwrap()
        .is_some());
    }

    #[test]
    fn flags_shell_option_and_redirection_paths_outside_scope() {
        let working_dir = tempfile::tempdir().unwrap();
        let allowed = vec![working_dir.path().to_path_buf()];
        for command in [
            "tool --output=/etc/gosling-output",
            "echo data >/etc/gosling-output",
        ] {
            let call = tool_call("developer__shell", json_args(&[("command", command)]));
            assert!(out_of_scope_path(&call, working_dir.path(), &allowed)
                .unwrap()
                .is_some());
        }
    }

    #[test]
    fn flags_file_uri_outside_scope() {
        let working_dir = tempfile::tempdir().unwrap();
        let call = tool_call(
            "third_party__resource",
            json_args(&[("resourceUri", "file:///etc/passwd")]),
        );

        assert!(out_of_scope_path(
            &call,
            working_dir.path(),
            std::slice::from_ref(&working_dir.path().to_path_buf())
        )
        .unwrap()
        .is_some());
    }

    #[test]
    fn no_arguments_never_flagged() {
        let working_dir = PathBuf::from("/home/user/project");
        let allowed = vec![working_dir.clone()];
        let call = CallToolRequestParams::new("developer__todo__read".to_string());

        assert_eq!(
            out_of_scope_path(&call, &working_dir, &allowed).unwrap(),
            None
        );
    }

    fn write_request(id: &str, path: &str) -> ToolRequest {
        ToolRequest {
            id: id.to_string(),
            tool_call: Ok(tool_call(
                "developer__text_editor__write",
                json_args(&[("path", path)]),
            )),
            metadata: None,
            tool_meta: None,
        }
    }

    fn read_request(id: &str, path: &str) -> ToolRequest {
        ToolRequest {
            id: id.to_string(),
            tool_call: Ok(tool_call(
                "developer__text_editor__read",
                json_args(&[("path", path)]),
            )),
            metadata: None,
            tool_meta: None,
        }
    }

    fn shell_request(id: &str, command: &str) -> ToolRequest {
        ToolRequest {
            id: id.to_string(),
            tool_call: Ok(tool_call(
                "developer__shell",
                json_args(&[("command", command)]),
            )),
            metadata: None,
            tool_meta: None,
        }
    }

    #[tokio::test]
    async fn workspace_policy_denies_read_only_mutation_and_allows_reads_and_outputs() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("project");
        let reference = root.path().join("reference");
        let output = root.path().join("output");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::create_dir_all(&reference).unwrap();
        std::fs::create_dir_all(&output).unwrap();

        let session_manager = Arc::new(SessionManager::new(root.path().to_path_buf()));
        let session = session_manager
            .create_session(
                project.clone(),
                "workspace".into(),
                crate::session::SessionType::User,
                GoslingMode::default(),
            )
            .await
            .unwrap();
        let context = crate::workspace::WorkspaceSessionContext {
            workspace_id: "workspace".into(),
            workspace_name: "Workspace".into(),
            primary_working_folder: project.to_string_lossy().to_string(),
            folders: Vec::new(),
            product_output_folders: Vec::new(),
            folder_policy: WorkspaceFolderPolicy {
                roots: vec![
                    crate::workspace::WorkspaceFolderPolicyRoot {
                        path: project.to_string_lossy().to_string(),
                        access: WorkspaceFolderAccess::ReadWrite,
                    },
                    crate::workspace::WorkspaceFolderPolicyRoot {
                        path: reference.to_string_lossy().to_string(),
                        access: WorkspaceFolderAccess::Read,
                    },
                    crate::workspace::WorkspaceFolderPolicyRoot {
                        path: output.to_string_lossy().to_string(),
                        access: WorkspaceFolderAccess::ReadWrite,
                    },
                ],
            },
        };
        session_manager
            .update(&session.id)
            .workspace_snapshot(
                "workspace".into(),
                "Workspace".into(),
                None,
                None,
                None,
                context,
            )
            .apply()
            .await
            .unwrap();

        let inspector = WorkingDirScopeInspector::new(session_manager.clone());
        let results = inspector
            .inspect(
                &session.id,
                &[
                    write_request(
                        "write-reference",
                        reference.join("valuable.txt").to_str().unwrap(),
                    ),
                    read_request(
                        "read-reference",
                        reference.join("valuable.txt").to_str().unwrap(),
                    ),
                    write_request("write-output", output.join("report.md").to_str().unwrap()),
                    shell_request(
                        "shell-write-output",
                        &format!("touch {}", output.join("shell-report.md").display()),
                    ),
                ],
                &[],
                GoslingMode::Auto,
            )
            .await
            .unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].tool_request_id, "write-reference");
        assert_eq!(results[0].action, InspectionAction::Deny);
        assert_eq!(results[1].tool_request_id, "shell-write-output");
        assert_eq!(results[1].action, InspectionAction::Deny);
        assert!(results[1].reason.contains("cannot be safely scoped"));
        let reloaded = session_manager
            .get_session(&session.id, false)
            .await
            .unwrap();
        // Workspace folder policy is enforced above via workspace_context, not the
        // opt-in restriction flag, which now defaults off for workspace sessions.
        assert!(!reloaded.restrict_tools_to_working_dirs);
        assert!(reloaded.additional_working_dirs.contains(&reference));
        assert!(reloaded.additional_working_dirs.contains(&output));
    }

    #[tokio::test]
    async fn workspace_session_root_is_not_granted_to_a_sibling_session() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("project");
        let private = non_temporary_target("private");
        std::fs::create_dir_all(&project).unwrap();

        let session_manager = Arc::new(SessionManager::new(root.path().to_path_buf()));
        let selected = session_manager
            .create_session(
                project.clone(),
                "selected".into(),
                crate::session::SessionType::User,
                GoslingMode::default(),
            )
            .await
            .unwrap();
        let sibling = session_manager
            .create_session(
                project.clone(),
                "sibling".into(),
                crate::session::SessionType::User,
                GoslingMode::default(),
            )
            .await
            .unwrap();
        let base_context = crate::workspace::WorkspaceSessionContext {
            workspace_id: "workspace".into(),
            workspace_name: "Workspace".into(),
            primary_working_folder: project.to_string_lossy().to_string(),
            folders: Vec::new(),
            product_output_folders: Vec::new(),
            folder_policy: WorkspaceFolderPolicy {
                roots: vec![crate::workspace::WorkspaceFolderPolicyRoot {
                    path: project.to_string_lossy().to_string(),
                    access: WorkspaceFolderAccess::ReadWrite,
                }],
            },
        };
        session_manager
            .update(&selected.id)
            .workspace_snapshot(
                "workspace".into(),
                "Workspace".into(),
                None,
                None,
                None,
                crate::workspace::WorkspaceSessionContext {
                    folder_policy: WorkspaceFolderPolicy {
                        roots: vec![
                            crate::workspace::WorkspaceFolderPolicyRoot {
                                path: project.to_string_lossy().to_string(),
                                access: WorkspaceFolderAccess::ReadWrite,
                            },
                            crate::workspace::WorkspaceFolderPolicyRoot {
                                path: private.to_string_lossy().to_string(),
                                access: WorkspaceFolderAccess::ReadWrite,
                            },
                        ],
                    },
                    ..base_context.clone()
                },
            )
            .apply()
            .await
            .unwrap();
        session_manager
            .update(&sibling.id)
            .workspace_snapshot(
                "workspace".into(),
                "Workspace".into(),
                None,
                None,
                None,
                base_context,
            )
            .apply()
            .await
            .unwrap();

        let inspector = WorkingDirScopeInspector::new(session_manager);
        let target = private.join("notes.md");
        let selected_results = inspector
            .inspect(
                &selected.id,
                &[write_request("selected-write", target.to_str().unwrap())],
                &[],
                GoslingMode::Auto,
            )
            .await
            .unwrap();
        let sibling_results = inspector
            .inspect(
                &sibling.id,
                &[write_request("sibling-write", target.to_str().unwrap())],
                &[],
                GoslingMode::Auto,
            )
            .await
            .unwrap();

        assert!(selected_results.is_empty());
        assert_eq!(sibling_results.len(), 1);
        assert!(matches!(
            sibling_results[0].action,
            InspectionAction::RequireApproval(_)
        ));
    }

    #[test]
    fn nested_read_only_root_overrides_writable_parent_and_shell_is_fail_closed() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("project");
        let reference = project.join("reference");
        std::fs::create_dir_all(&reference).unwrap();
        let policy = WorkspaceFolderPolicy {
            roots: vec![
                crate::workspace::WorkspaceFolderPolicyRoot {
                    path: project.to_string_lossy().to_string(),
                    access: WorkspaceFolderAccess::ReadWrite,
                },
                crate::workspace::WorkspaceFolderPolicyRoot {
                    path: reference.to_string_lossy().to_string(),
                    access: WorkspaceFolderAccess::Read,
                },
            ],
        };
        let target = reference.join("valuable.txt");
        let shell_write = tool_call(
            "developer__shell",
            json_args(&[("command", &format!("rm {}", target.display()))]),
        );
        let shell_read = tool_call(
            "developer__shell",
            json_args(&[("command", &format!("cat {}", target.display()))]),
        );

        assert!(is_mutating_tool_call(&shell_write));
        assert_eq!(
            first_read_only_path(&shell_write, &project, &policy).unwrap(),
            Some(canonicalize_potential_path(&target).unwrap())
        );
        assert!(!is_mutating_tool_call(&shell_read));
    }

    #[tokio::test]
    async fn off_by_default_never_flags_anything() {
        let dir = tempfile::tempdir().unwrap();
        let session_manager = Arc::new(SessionManager::new(dir.path().to_path_buf()));
        let session = session_manager
            .create_session(
                dir.path().to_path_buf(),
                "test".into(),
                crate::session::SessionType::User,
                GoslingMode::default(),
            )
            .await
            .unwrap();

        let inspector = WorkingDirScopeInspector::new(session_manager);
        let results = inspector
            .inspect(
                &session.id,
                &[write_request("req-1", "/etc/passwd")],
                &[],
                GoslingMode::Auto,
            )
            .await
            .unwrap();

        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn flags_out_of_scope_write_when_restriction_enabled() {
        let dir = tempfile::tempdir().unwrap();
        let session_manager = Arc::new(SessionManager::new(dir.path().to_path_buf()));
        let session = session_manager
            .create_session(
                dir.path().to_path_buf(),
                "test".into(),
                crate::session::SessionType::User,
                GoslingMode::default(),
            )
            .await
            .unwrap();
        session_manager
            .update(&session.id)
            .restrict_tools_to_working_dirs(true)
            .apply()
            .await
            .unwrap();

        let inspector = WorkingDirScopeInspector::new(session_manager);
        let results = inspector
            .inspect(
                &session.id,
                &[write_request("req-1", "/etc/passwd")],
                &[],
                GoslingMode::Auto,
            )
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].tool_request_id, "req-1");
        match &results[0].action {
            InspectionAction::RequireApproval(Some(message)) => {
                assert!(message.contains("/etc/passwd"));
                assert!(message.contains(&dir.path().display().to_string()));
            }
            other => panic!("expected RequireApproval with a message, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn allows_in_scope_write_when_restriction_enabled() {
        let dir = tempfile::tempdir().unwrap();
        let session_manager = Arc::new(SessionManager::new(dir.path().to_path_buf()));
        let session = session_manager
            .create_session(
                dir.path().to_path_buf(),
                "test".into(),
                crate::session::SessionType::User,
                GoslingMode::default(),
            )
            .await
            .unwrap();
        session_manager
            .update(&session.id)
            .restrict_tools_to_working_dirs(true)
            .apply()
            .await
            .unwrap();

        let in_scope_path = dir.path().join("file.txt");
        let inspector = WorkingDirScopeInspector::new(session_manager);
        let results = inspector
            .inspect(
                &session.id,
                &[write_request("req-1", in_scope_path.to_str().unwrap())],
                &[],
                GoslingMode::Auto,
            )
            .await
            .unwrap();

        assert!(results.is_empty());
    }

    async fn workspace_session(
        session_manager: &Arc<SessionManager>,
        project: &Path,
    ) -> crate::session::Session {
        let session = session_manager
            .create_session(
                project.to_path_buf(),
                "workspace".into(),
                crate::session::SessionType::User,
                GoslingMode::default(),
            )
            .await
            .unwrap();
        let context = crate::workspace::WorkspaceSessionContext {
            workspace_id: "workspace".into(),
            workspace_name: "Workspace".into(),
            primary_working_folder: project.to_string_lossy().to_string(),
            folders: Vec::new(),
            product_output_folders: Vec::new(),
            folder_policy: WorkspaceFolderPolicy {
                roots: vec![crate::workspace::WorkspaceFolderPolicyRoot {
                    path: project.to_string_lossy().to_string(),
                    access: WorkspaceFolderAccess::ReadWrite,
                }],
            },
        };
        session_manager
            .update(&session.id)
            .workspace_snapshot(
                "workspace".into(),
                "Workspace".into(),
                None,
                None,
                None,
                context,
            )
            .apply()
            .await
            .unwrap();
        session_manager
            .get_session(&session.id, false)
            .await
            .unwrap()
    }

    /// A workspace session that has not turned on "restrict tools to working
    /// directories" may read reference material outside its folders without a
    /// prompt; only out-of-scope mutations still require approval.
    #[tokio::test]
    async fn workspace_session_without_restriction_prompts_only_for_out_of_scope_mutations() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("project");
        let reference = root.path().join("reference");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::create_dir_all(&reference).unwrap();
        let notes = reference.join("notes.md");
        std::fs::write(&notes, "reference").unwrap();
        let outside = non_temporary_target("outside");

        let session_manager = Arc::new(SessionManager::new(root.path().to_path_buf()));
        let session = workspace_session(&session_manager, &project).await;
        assert!(!session.restrict_tools_to_working_dirs);

        let inspector = WorkingDirScopeInspector::new(session_manager);
        let results = inspector
            .inspect(
                &session.id,
                &[
                    read_request("read-outside", notes.to_str().unwrap()),
                    shell_request(
                        "cat-then-script",
                        &format!("cat {}; python3 - <<'PY'\nprint(1)\nPY", notes.display()),
                    ),
                    write_request("write-outside", outside.join("draft.md").to_str().unwrap()),
                    shell_request(
                        "shell-write-outside",
                        &format!("touch {}", outside.join("touched.md").display()),
                    ),
                ],
                &[],
                GoslingMode::Auto,
            )
            .await
            .unwrap();

        let flagged: Vec<&str> = results
            .iter()
            .map(|result| result.tool_request_id.as_str())
            .collect();
        assert_eq!(flagged, vec!["write-outside", "shell-write-outside"]);
        for result in &results {
            match &result.action {
                InspectionAction::RequireApproval(Some(message)) => {
                    assert!(message.contains("workspace session"), "{message}");
                    assert!(!message.contains("turned on"), "{message}");
                }
                other => panic!("expected RequireApproval with a message, got {other:?}"),
            }
        }
    }

    #[tokio::test]
    async fn workspace_session_with_restriction_prompts_for_out_of_scope_reads() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("project");
        let reference = root.path().join("reference");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::create_dir_all(&reference).unwrap();
        let notes = reference.join("notes.md");
        std::fs::write(&notes, "reference").unwrap();

        let session_manager = Arc::new(SessionManager::new(root.path().to_path_buf()));
        let session = workspace_session(&session_manager, &project).await;
        session_manager
            .update(&session.id)
            .restrict_tools_to_working_dirs(true)
            .apply()
            .await
            .unwrap();

        let inspector = WorkingDirScopeInspector::new(session_manager);
        let results = inspector
            .inspect(
                &session.id,
                &[read_request("read-outside", notes.to_str().unwrap())],
                &[],
                GoslingMode::Auto,
            )
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        match &results[0].action {
            InspectionAction::RequireApproval(Some(message)) => {
                assert!(message.contains("turned on"), "{message}");
            }
            other => panic!("expected RequireApproval with a message, got {other:?}"),
        }
    }

    fn shell_mutation_out_of_scope(command: &str, working_dir: &Path) -> Option<PathBuf> {
        let call = tool_call("developer__shell", json_args(&[("command", command)]));
        let allowed = vec![working_dir.to_path_buf()];
        super::out_of_scope_path(&mutation_paths(&call, working_dir), &allowed, &[]).unwrap()
    }

    #[test]
    fn device_streams_are_recognized_narrowly() {
        for path in [
            "/dev/null",
            "/dev/stdout",
            "/dev/stderr",
            "/dev/tty",
            "/dev/fd/3",
            "/dev/urandom",
        ] {
            assert!(is_device_stream(Path::new(path)), "{path}");
        }
        for path in [
            "/dev/disk2",
            "/dev/fd/abc",
            "/dev/fd",
            "/dev/null/child",
            "/devices/null",
            "dev/null",
        ] {
            assert!(!is_device_stream(Path::new(path)), "{path}");
        }
    }

    #[test]
    fn redirections_to_device_streams_do_not_count_as_writes() {
        let working_dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let outside_file = outside.path().join("input.txt");
        std::fs::write(&outside_file, "data").unwrap();
        for command in [
            format!(
                "sample 123 3 -file {} >/dev/null 2>&1",
                working_dir.path().join("out.txt").display()
            ),
            format!(
                "find {} -name '*.md' 2>/dev/null | head -5",
                outside.path().display()
            ),
            format!("grep -n pattern {} 2> /dev/null", outside_file.display()),
            format!("cat {} > /dev/stdout", outside_file.display()),
            "printf 'x' &>/dev/null".to_string(),
        ] {
            assert_eq!(
                shell_mutation_out_of_scope(&command, working_dir.path()),
                None,
                "{command}"
            );
        }

        let raw_disk = shell_mutation_out_of_scope("echo x > /dev/disk2", working_dir.path());
        assert_eq!(raw_disk.as_deref(), Some(Path::new("/dev/disk2")));
        let stderr_to_file = shell_mutation_out_of_scope(
            &format!("ls 2>{}", outside.path().join("err.log").display()),
            working_dir.path(),
        );
        assert_eq!(
            stderr_to_file,
            Some(canonicalize_potential_path(&outside.path().join("err.log")).unwrap())
        );
    }

    fn shell_segments(command: &str) -> Vec<Vec<String>> {
        let analysis = analyze_shell(command);
        assert!(analysis.complete, "{command}");
        analysis
            .segments
            .into_iter()
            .map(|segment| segment.words)
            .collect()
    }

    #[test]
    fn segments_split_at_glued_separators_and_newlines() {
        assert_eq!(
            shell_segments("head -3 a;printf 'x;y' | wc -l\ncd b && rm c || true"),
            vec![
                vec!["head", "-3", "a"],
                vec!["printf", "x;y"],
                vec!["wc", "-l"],
                vec!["cd", "b"],
                vec!["rm", "c"],
                vec!["true"]
            ]
        );
        let working_dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let outside_file = outside.path().join("notes.md");
        std::fs::write(&outside_file, "notes").unwrap();
        for command in [
            format!(
                "head -55 {}; printf '\\n---next---\\n'",
                outside_file.display()
            ),
            format!("tail -65 '{}'; ps -p 1 -o pid", outside_file.display()),
            format!("cat {}\nls -la", outside_file.display()),
        ] {
            assert_eq!(
                shell_mutation_out_of_scope(&command, working_dir.path()),
                None,
                "{command}"
            );
        }
        let written = outside.path().join("draft.md");
        let expected = canonicalize_potential_path(&written).unwrap();
        for command in [
            format!(
                "cat {}; echo hi > {}",
                outside_file.display(),
                written.display()
            ),
            format!(
                "cd {}\nrm -rf {}",
                working_dir.path().display(),
                written.display()
            ),
        ] {
            assert_eq!(
                shell_mutation_out_of_scope(&command, working_dir.path()).as_deref(),
                Some(expected.as_path()),
                "{command}"
            );
        }
    }

    #[test]
    fn shell_comments_do_not_change_command_boundaries() {
        for comment in [
            "# launchd's activation API",
            "# unmatched \"quote; | && > /outside/comment.txt",
            "# escaped newline stays in the comment \\",
        ] {
            assert_eq!(
                shell_segments(&format!("{comment}\nprintf ok >/dev/null\nls")),
                vec![vec!["printf", "ok"], vec!["", ">/dev/null"], vec!["ls"]],
                "{comment}"
            );
            assert_eq!(
                shell_segments(&format!("printf ok {comment}\nls")),
                vec![vec!["printf", "ok"], vec!["ls"]],
                "{comment}"
            );
        }
        assert_eq!(
            shell_segments("printf ok # trailing unmatched '\""),
            vec![vec!["printf", "ok"]]
        );
        assert_eq!(
            shell_segments("true;# unmatched '\nls &&# unmatched \"\npwd"),
            vec![vec!["true"], vec!["ls"], vec!["pwd"]]
        );
    }

    #[test]
    fn literal_hashes_and_line_continuations_remain_arguments() {
        for argument in [
            "path#fragment",
            "'literal # hash'",
            "\"literal # hash\"",
            "'two\n# literal lines'",
            "\\#escaped",
            "word\\ #suffix",
            "word\\\n#suffix",
            "''#suffix",
            "word\u{00a0}#suffix",
        ] {
            let command = format!("printf %s {argument}");
            let parsed = shell_segments(&command);
            assert_eq!(parsed.len(), 1, "{command}");
            assert_eq!(
                parsed[0],
                shell_words::split(&command).unwrap(),
                "{command}"
            );
        }
        assert_eq!(
            shell_segments("printf ok \\\n# unmatched '\nls"),
            vec![vec!["printf", "ok"], vec!["ls"]]
        );
    }

    #[tokio::test]
    async fn workspace_session_does_not_prompt_after_heredoc_comment() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("project");
        std::fs::create_dir(&project).unwrap();
        let session_manager = Arc::new(SessionManager::new(root.path().to_path_buf()));
        let session = workspace_session(&session_manager, &project).await;
        let inspector = WorkingDirScopeInspector::new(session_manager);
        let command = "set -eu\npython3 - <<'PY'\n\
            # Dedicated test uses launchd's real activation API.\n\
            print('ready')\nPY\n\
            curl --fail --max-time 10 -sS http://127.0.0.1:18998/ >/dev/null\n\
            launchctl print gui/$(id -u)/local.mac-demand.test | grep -E 'state =|pid =|runs ='";
        let outside = non_temporary_target("outside.txt");
        let results = inspector
            .inspect(
                &session.id,
                &[
                    shell_request("diagnostic", command),
                    shell_request(
                        "real-write",
                        &format!("{command}\nprintf x > '{}'", outside.display()),
                    ),
                    shell_request(
                        "write-after-comment",
                        &format!(
                            "# don't skip the next command\nprintf x > '{}'",
                            outside.display()
                        ),
                    ),
                ],
                &[],
                GoslingMode::Auto,
            )
            .await
            .unwrap();
        let flagged: Vec<&str> = results
            .iter()
            .map(|result| result.tool_request_id.as_str())
            .collect();
        assert_eq!(flagged, vec!["real-write", "write-after-comment"]);
        for result in results {
            match result.action {
                InspectionAction::RequireApproval(Some(message)) => {
                    assert!(message.contains(outside.to_str().unwrap()), "{message}");
                }
                other => panic!("expected approval for the actual write, got {other:?}"),
            }
        }
    }

    #[tokio::test]
    async fn workspace_session_does_not_prompt_for_diagnostic_pipelines() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("project");
        let library = root.path().join("Library");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::create_dir_all(library.join("logs")).unwrap();
        let log = library.join("logs").join("service.log");
        std::fs::write(&log, "log").unwrap();
        let outside = non_temporary_target("outside");

        let session_manager = Arc::new(SessionManager::new(root.path().to_path_buf()));
        let session = workspace_session(&session_manager, &project).await;
        let inspector = WorkingDirScopeInspector::new(session_manager);
        let results = inspector
            .inspect(
                &session.id,
                &[
                    shell_request(
                        "profile",
                        &format!(
                            "printf '\\n---profile---\\n'; sample 123 3 10 -file {} >/dev/null 2>&1; ls -t '{}' | head -4",
                            project.join("sample.txt").display(),
                            library.join("logs").display()
                        ),
                    ),
                    shell_request(
                        "logs",
                        &format!(
                            "printf '\\n---logs---\\n'; tail -65 '{}'; find '{}' -iname '*.log'; lsof -a -p 1 -d cwd -Fn 2>/dev/null; ps -p 1 -o pid,command",
                            log.display(),
                            library.display()
                        ),
                    ),
                    shell_request(
                        "config",
                        &format!(
                            "cd {} && printf '\\n---config---\\n'; find {} -maxdepth 3 -type f -iname '*config*' 2>/dev/null | head -45; grep -nE 'remote|provider' src/config.py 2>/dev/null | head -65",
                            project.display(),
                            library.display()
                        ),
                    ),
                    shell_request(
                        "store",
                        &format!(
                            "cd {} && printf '\\n---store---\\n' && rg -n 'retention' src/*.py | head -130 && lsof -n -c python3 2>/dev/null | awk '/index.sqlite$/ {{print $NF}}' | sort -u && sed -n '1,260p' src/retention.py",
                            project.display()
                        ),
                    ),
                    shell_request(
                        "ports",
                        "for port in 11434 11435; do printf '\\nPORT %s\\n' \"$port\"; curl -fsS --max-time 3 \"http://127.0.0.1:$port/api/ps\"; done; top -l 1 -n 5 -o cpu",
                    ),
                    shell_request(
                        "write-outside",
                        &format!("printf 'x' >> {}", outside.join("draft.md").display()),
                    ),
                    shell_request(
                        "awk-write-outside",
                        &format!(
                            "ps -o pid | awk '{{print > \"{}\"}}'",
                            outside.join("pids.txt").display()
                        ),
                    ),
                ],
                &[],
                GoslingMode::Auto,
            )
            .await
            .unwrap();

        let flagged: Vec<&str> = results
            .iter()
            .map(|result| result.tool_request_id.as_str())
            .collect();
        assert_eq!(flagged, vec!["write-outside", "awk-write-outside"]);
    }
}
