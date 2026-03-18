//! Precise code modification engine using unified diffs.
//!
//! Instead of agents rewriting entire files, they produce minimal diffs —
//! saving tokens and preventing accidental deletions. This module provides
//! generation, validation, formatting, parsing, and application of unified
//! diffs across single files and multi-file plans.

use serde::{Deserialize, Serialize};
use std::fmt;

// ─────────────────────────── Data types ──────────────────────────────────────

/// A single line in a diff.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiffLine {
    /// Unchanged context line.
    Context(String),
    /// Line added in the new version.
    Added(String),
    /// Line removed from the old version.
    Removed(String),
}

impl fmt::Display for DiffLine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Context(l) => write!(f, " {l}"),
            Self::Added(l) => write!(f, "+{l}"),
            Self::Removed(l) => write!(f, "-{l}"),
        }
    }
}

/// A single change hunk within a diff.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffHunk {
    /// Line number in the original file where the hunk starts (1-based).
    pub old_start: usize,
    /// Number of lines from the original file in this hunk.
    pub old_count: usize,
    /// Line number in the new file where the hunk starts (1-based).
    pub new_start: usize,
    /// Number of lines in the new file in this hunk.
    pub new_count: usize,
    /// The diff lines (prefixed with ' ', '+', '-').
    pub lines: Vec<DiffLine>,
}

/// The type of file operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiffOperation {
    /// Modify an existing file.
    Modify,
    /// Create a new file.
    Create,
    /// Delete a file.
    Delete,
    /// Rename a file.
    Rename {
        /// Original path before rename.
        from: String,
    },
}

/// A diff for a single file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileDiff {
    /// Path to the file being modified.
    pub path: String,
    /// The operation being performed.
    pub operation: DiffOperation,
    /// Individual change hunks.
    pub hunks: Vec<DiffHunk>,
    /// Lines added total.
    pub additions: usize,
    /// Lines removed total.
    pub deletions: usize,
}

/// An ordered plan of diffs across multiple files.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffPlan {
    /// Human-readable description of the overall change.
    pub description: String,
    /// Ordered list of file diffs (order matters for dependencies).
    pub diffs: Vec<FileDiff>,
    /// Total lines added across all files.
    pub total_additions: usize,
    /// Total lines removed across all files.
    pub total_deletions: usize,
    /// Estimated token count for this plan.
    pub estimated_tokens: usize,
}

/// Result of applying a diff.
#[derive(Debug, Clone)]
pub struct ApplyResult {
    /// Path of the file the diff was applied to.
    pub path: String,
    /// Whether the application succeeded.
    pub success: bool,
    /// The resulting file content on success.
    pub new_content: Option<String>,
    /// Error description on failure.
    pub error: Option<String>,
}

/// Validation result for a diff.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    /// Whether the diff is valid and can be applied cleanly.
    pub valid: bool,
    /// Individual issues found during validation.
    pub issues: Vec<ValidationIssue>,
}

/// A single issue found during diff validation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationIssue {
    /// Index of the hunk that triggered this issue.
    pub hunk_index: usize,
    /// Human-readable description of the issue.
    pub message: String,
    /// Severity of the issue.
    pub severity: IssueSeverity,
}

/// Severity level for a validation issue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum IssueSeverity {
    /// Non-blocking; diff can still be applied.
    Warning,
    /// Blocking; diff cannot be applied cleanly.
    Error,
}

// ─────────────────────────── Engine ──────────────────────────────────────────

/// Configuration for the diff engine.
#[derive(Debug, Clone)]
pub struct DiffConfig {
    /// Number of context lines around each hunk (default: 3).
    pub context_lines: usize,
    /// Whether to minimize diffs by merging adjacent hunks.
    pub minimize: bool,
}

impl Default for DiffConfig {
    fn default() -> Self {
        Self {
            context_lines: 3,
            minimize: true,
        }
    }
}

/// The diff engine for generating and applying code changes.
///
/// Implements line-based diff generation using a longest-common-subsequence
/// algorithm, unified-format serialization/parsing, validation, and multi-file
/// diff plans.
pub struct DiffEngine {
    config: DiffConfig,
}

impl Default for DiffEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl DiffEngine {
    /// Create a new `DiffEngine` with default configuration.
    pub fn new() -> Self {
        Self {
            config: DiffConfig::default(),
        }
    }

    /// Create a new `DiffEngine` with the given configuration.
    pub fn with_config(config: DiffConfig) -> Self {
        Self { config }
    }

    // ── Diff generation ──────────────────────────────────────────────────

    /// Generate a unified diff between `old` and `new` content for a file at `path`.
    ///
    /// Returns a [`FileDiff`] containing hunks that describe the minimal set of
    /// changes between the two versions.
    pub fn generate_diff(&self, path: &str, old: &str, new: &str) -> FileDiff {
        let old_lines = split_lines(old);
        let new_lines = split_lines(new);

        let ops = compute_edit_ops(&old_lines, &new_lines);
        let hunks = self.ops_to_hunks(&old_lines, &new_lines, &ops);

        let mut additions = 0;
        let mut deletions = 0;
        for h in &hunks {
            for l in &h.lines {
                match l {
                    DiffLine::Added(_) => additions += 1,
                    DiffLine::Removed(_) => deletions += 1,
                    DiffLine::Context(_) => {}
                }
            }
        }

        let operation = if old.is_empty() && !new.is_empty() {
            DiffOperation::Create
        } else if !old.is_empty() && new.is_empty() {
            DiffOperation::Delete
        } else {
            DiffOperation::Modify
        };

        FileDiff {
            path: path.to_string(),
            operation,
            hunks,
            additions,
            deletions,
        }
    }

    /// Generate the smallest possible diff between `old` and `new`.
    ///
    /// Uses zero context lines and merges all adjacent hunks to produce the
    /// most compact representation.
    pub fn minimize_diff(&self, path: &str, old: &str, new: &str) -> FileDiff {
        let minimal = Self::with_config(DiffConfig {
            context_lines: 0,
            minimize: true,
        });
        minimal.generate_diff(path, old, new)
    }

    // ── Apply ────────────────────────────────────────────────────────────

    /// Apply a [`FileDiff`] to file `content`, returning the resulting content.
    ///
    /// Each hunk is applied in order, adjusting line offsets as earlier hunks
    /// shift content. Context lines are verified before application; a mismatch
    /// causes the operation to fail.
    pub fn apply_diff(&self, content: &str, diff: &FileDiff) -> ApplyResult {
        // Handle Create from empty.
        if diff.operation == DiffOperation::Create && content.is_empty() {
            let mut result = String::new();
            for hunk in &diff.hunks {
                for line in &hunk.lines {
                    if let DiffLine::Added(l) = line {
                        result.push_str(l);
                        result.push('\n');
                    }
                }
            }
            return ApplyResult {
                path: diff.path.clone(),
                success: true,
                new_content: Some(result),
                error: None,
            };
        }

        // Handle Delete.
        if diff.operation == DiffOperation::Delete {
            return ApplyResult {
                path: diff.path.clone(),
                success: true,
                new_content: Some(String::new()),
                error: None,
            };
        }

        let mut lines: Vec<String> = split_lines(content).into_iter().map(String::from).collect();
        let mut offset: isize = 0;

        for (hunk_idx, hunk) in diff.hunks.iter().enumerate() {
            let start = (hunk.old_start as isize - 1 + offset) as usize;

            // Verify context lines match.
            let mut pos = start;
            for line in &hunk.lines {
                match line {
                    DiffLine::Context(ctx) => {
                        if pos >= lines.len() || lines[pos] != *ctx {
                            return ApplyResult {
                                path: diff.path.clone(),
                                success: false,
                                new_content: None,
                                error: Some(format!(
                                    "Context mismatch at hunk {hunk_idx}, line {pos}"
                                )),
                            };
                        }
                        pos += 1;
                    }
                    DiffLine::Removed(rem) => {
                        if pos >= lines.len() || lines[pos] != *rem {
                            return ApplyResult {
                                path: diff.path.clone(),
                                success: false,
                                new_content: None,
                                error: Some(format!(
                                    "Remove mismatch at hunk {hunk_idx}, line {pos}"
                                )),
                            };
                        }
                        pos += 1;
                    }
                    DiffLine::Added(_) => {}
                }
            }

            // Apply the hunk: build replacement slice.
            let mut new_lines: Vec<String> = Vec::new();
            for line in &hunk.lines {
                match line {
                    DiffLine::Context(ctx) => new_lines.push(ctx.clone()),
                    DiffLine::Added(add) => new_lines.push(add.clone()),
                    DiffLine::Removed(_) => {}
                }
            }

            let old_len = hunk.old_count;
            let new_len = new_lines.len();

            // Replace old_len lines starting at `start` with new_lines.
            let end = start + old_len;
            let end = end.min(lines.len());
            lines.splice(start..end, new_lines);

            offset += new_len as isize - old_len as isize;
        }

        let mut result = lines.join("\n");
        // Preserve trailing newline if the original had one.
        if !result.is_empty() && !result.ends_with('\n') {
            result.push('\n');
        }

        ApplyResult {
            path: diff.path.clone(),
            success: true,
            new_content: Some(result),
            error: None,
        }
    }

    // ── Validation ───────────────────────────────────────────────────────

    /// Validate that a diff can be applied cleanly to the given `content`.
    ///
    /// Checks that context and removed lines match the actual file content,
    /// that hunk ranges are within bounds, and flags any issues found.
    pub fn validate_diff(&self, content: &str, diff: &FileDiff) -> ValidationResult {
        let lines: Vec<&str> = split_lines(content);
        let mut issues = Vec::new();
        let mut offset: isize = 0;

        for (hunk_idx, hunk) in diff.hunks.iter().enumerate() {
            let start = hunk.old_start as isize - 1 + offset;
            if start < 0 {
                issues.push(ValidationIssue {
                    hunk_index: hunk_idx,
                    message: format!(
                        "Hunk old_start {} with offset {offset} yields negative position",
                        hunk.old_start
                    ),
                    severity: IssueSeverity::Error,
                });
                continue;
            }
            let start = start as usize;

            if start + hunk.old_count > lines.len() {
                issues.push(ValidationIssue {
                    hunk_index: hunk_idx,
                    message: format!(
                        "Hunk extends beyond file end (start={start}, old_count={}, file_lines={})",
                        hunk.old_count,
                        lines.len()
                    ),
                    severity: IssueSeverity::Error,
                });
                continue;
            }

            let mut pos = start;
            for line in &hunk.lines {
                match line {
                    DiffLine::Context(ctx) => {
                        if pos >= lines.len() || lines[pos] != ctx.as_str() {
                            let actual = if pos < lines.len() {
                                lines[pos].to_string()
                            } else {
                                "<EOF>".to_string()
                            };
                            issues.push(ValidationIssue {
                                hunk_index: hunk_idx,
                                message: format!(
                                    "Context mismatch at line {pos}: expected {ctx:?}, got {actual:?}"
                                ),
                                severity: IssueSeverity::Error,
                            });
                        }
                        pos += 1;
                    }
                    DiffLine::Removed(rem) => {
                        if pos >= lines.len() || lines[pos] != rem.as_str() {
                            let actual = if pos < lines.len() {
                                lines[pos].to_string()
                            } else {
                                "<EOF>".to_string()
                            };
                            issues.push(ValidationIssue {
                                hunk_index: hunk_idx,
                                message: format!(
                                    "Remove mismatch at line {pos}: expected {rem:?}, got {actual:?}"
                                ),
                                severity: IssueSeverity::Error,
                            });
                        }
                        pos += 1;
                    }
                    DiffLine::Added(_) => {}
                }
            }

            // Update offset for next hunk.
            let added = hunk.lines.iter().filter(|l| matches!(l, DiffLine::Added(_))).count();
            let removed = hunk
                .lines
                .iter()
                .filter(|l| matches!(l, DiffLine::Removed(_)))
                .count();
            offset += added as isize - removed as isize;
        }

        let valid = !issues.iter().any(|i| i.severity == IssueSeverity::Error);
        ValidationResult { valid, issues }
    }

    // ── Plan ─────────────────────────────────────────────────────────────

    /// Create a [`DiffPlan`] from a list of `(path, old_content, new_content)` tuples.
    ///
    /// Each tuple generates one [`FileDiff`], and the plan aggregates statistics
    /// across all files.
    pub fn create_plan(&self, description: &str, changes: &[(&str, &str, &str)]) -> DiffPlan {
        let mut diffs = Vec::with_capacity(changes.len());
        let mut total_add = 0;
        let mut total_del = 0;

        for &(path, old, new) in changes {
            let d = self.generate_diff(path, old, new);
            total_add += d.additions;
            total_del += d.deletions;
            diffs.push(d);
        }

        let estimated_tokens: usize = diffs.iter().map(|d| self.estimate_tokens(d)).sum();

        DiffPlan {
            description: description.to_string(),
            diffs,
            total_additions: total_add,
            total_deletions: total_del,
            estimated_tokens,
        }
    }

    /// Apply an entire [`DiffPlan`], returning results for each file.
    ///
    /// `contents` is a slice of `(path, current_content)` pairs. Each diff in
    /// the plan is matched to its file by path.
    pub fn apply_plan(&self, contents: &[(&str, &str)], plan: &DiffPlan) -> Vec<ApplyResult> {
        let mut results = Vec::with_capacity(plan.diffs.len());
        // Build a mutable map so that later diffs see the results of earlier ones.
        let mut content_map: std::collections::HashMap<String, String> = contents
            .iter()
            .map(|(p, c)| (p.to_string(), c.to_string()))
            .collect();

        for diff in &plan.diffs {
            let current = content_map.get(&diff.path).map(|s| s.as_str()).unwrap_or("");
            let result = self.apply_diff(current, diff);
            if result.success {
                if let Some(ref new_content) = result.new_content {
                    content_map.insert(diff.path.clone(), new_content.clone());
                }
            }
            results.push(result);
        }

        results
    }

    // ── Unified format ───────────────────────────────────────────────────

    /// Format a [`FileDiff`] as unified diff text (the standard `diff -u` format).
    pub fn format_unified(&self, diff: &FileDiff) -> String {
        let mut out = String::new();

        let a_path = match &diff.operation {
            DiffOperation::Rename { from } => from.clone(),
            _ => diff.path.clone(),
        };
        let b_path = &diff.path;

        out.push_str(&format!("--- a/{a_path}\n"));
        out.push_str(&format!("+++ b/{b_path}\n"));

        for hunk in &diff.hunks {
            out.push_str(&format!(
                "@@ -{},{} +{},{} @@\n",
                hunk.old_start, hunk.old_count, hunk.new_start, hunk.new_count
            ));
            for line in &hunk.lines {
                out.push_str(&format!("{line}\n"));
            }
        }

        out
    }

    /// Parse unified diff text back into a [`FileDiff`].
    ///
    /// Expects the standard `--- a/` / `+++ b/` header followed by `@@ ... @@`
    /// hunks with ` `, `+`, and `-` prefixed lines.
    pub fn parse_unified(&self, text: &str) -> Result<FileDiff, String> {
        let text_lines: Vec<&str> = text.lines().collect();
        if text_lines.len() < 2 {
            return Err("Diff text too short: expected at least --- and +++ lines".to_string());
        }

        // Parse header.
        let a_line = text_lines
            .first()
            .ok_or_else(|| "Missing --- line".to_string())?;
        let b_line = text_lines
            .get(1)
            .ok_or_else(|| "Missing +++ line".to_string())?;

        if !a_line.starts_with("--- ") {
            return Err(format!("Expected '--- ' prefix, got: {a_line}"));
        }
        if !b_line.starts_with("+++ ") {
            return Err(format!("Expected '+++ ' prefix, got: {b_line}"));
        }

        let a_path = a_line
            .strip_prefix("--- a/")
            .unwrap_or(a_line.strip_prefix("--- ").unwrap_or(a_line));
        let b_path = b_line
            .strip_prefix("+++ b/")
            .unwrap_or(b_line.strip_prefix("+++ ").unwrap_or(b_line));

        let path = b_path.to_string();
        let operation = if a_path == "/dev/null" {
            DiffOperation::Create
        } else if b_path == "/dev/null" {
            DiffOperation::Delete
        } else if a_path != b_path {
            DiffOperation::Rename {
                from: a_path.to_string(),
            }
        } else {
            DiffOperation::Modify
        };

        // Parse hunks.
        let mut hunks = Vec::new();
        let mut additions = 0;
        let mut deletions = 0;
        let mut i = 2;

        while i < text_lines.len() {
            let line = text_lines[i];
            if line.starts_with("@@ ") {
                // Parse hunk header: @@ -old_start,old_count +new_start,new_count @@
                let header = parse_hunk_header(line)?;
                i += 1;

                let mut hunk_lines = Vec::new();
                while i < text_lines.len() && !text_lines[i].starts_with("@@ ") {
                    let dl = text_lines[i];
                    if let Some(ctx) = dl.strip_prefix(' ') {
                        hunk_lines.push(DiffLine::Context(ctx.to_string()));
                    } else if let Some(add) = dl.strip_prefix('+') {
                        hunk_lines.push(DiffLine::Added(add.to_string()));
                        additions += 1;
                    } else if let Some(rem) = dl.strip_prefix('-') {
                        hunk_lines.push(DiffLine::Removed(rem.to_string()));
                        deletions += 1;
                    } else if dl.is_empty() {
                        // Treat empty lines as context with empty content.
                        hunk_lines.push(DiffLine::Context(String::new()));
                    } else {
                        return Err(format!(
                            "Unexpected line prefix at line {i}: {dl:?}"
                        ));
                    }
                    i += 1;
                }

                hunks.push(DiffHunk {
                    old_start: header.0,
                    old_count: header.1,
                    new_start: header.2,
                    new_count: header.3,
                    lines: hunk_lines,
                });
            } else {
                i += 1;
            }
        }

        Ok(FileDiff {
            path,
            operation,
            hunks,
            additions,
            deletions,
        })
    }

    // ── Token estimation ─────────────────────────────────────────────────

    /// Estimate token count for a diff (for LLM context budgeting).
    ///
    /// Uses a rough heuristic of 1 token per 4 characters.
    pub fn estimate_tokens(&self, diff: &FileDiff) -> usize {
        let mut chars = 0;
        // Count header.
        chars += diff.path.len() * 2 + 20; // --- a/path + +++ b/path

        for hunk in &diff.hunks {
            chars += 30; // @@ header line
            for line in &hunk.lines {
                chars += match line {
                    DiffLine::Context(l) | DiffLine::Added(l) | DiffLine::Removed(l) => {
                        l.len() + 2 // prefix char + newline
                    }
                };
            }
        }

        // Roughly 1 token per 4 characters.
        (chars + 3) / 4
    }

    // ── Private helpers ──────────────────────────────────────────────────

    /// Convert edit operations into hunks with context lines.
    fn ops_to_hunks<'a>(
        &self,
        old_lines: &[&'a str],
        new_lines: &[&'a str],
        ops: &[EditOp],
    ) -> Vec<DiffHunk> {
        if ops.is_empty() {
            return Vec::new();
        }

        // Find ranges of non-Equal ops and expand with context.
        let mut change_ranges: Vec<(usize, usize)> = Vec::new();
        let mut i = 0;
        while i < ops.len() {
            if ops[i] != EditOp::Equal {
                let start = i;
                while i < ops.len() && ops[i] != EditOp::Equal {
                    i += 1;
                }
                change_ranges.push((start, i));
            } else {
                i += 1;
            }
        }

        if change_ranges.is_empty() {
            return Vec::new();
        }

        // Expand each change range with context, merge overlapping.
        let ctx = self.config.context_lines;
        let mut merged: Vec<(usize, usize)> = Vec::new();

        for &(cs, ce) in &change_ranges {
            let start = cs.saturating_sub(ctx);
            let end = (ce + ctx).min(ops.len());

            if let Some(last) = merged.last_mut() {
                if start <= last.1 {
                    last.1 = end;
                } else {
                    merged.push((start, end));
                }
            } else {
                merged.push((start, end));
            }
        }

        // Build hunks from merged ranges.
        let mut hunks = Vec::new();
        for &(range_start, range_end) in &merged {
            let mut lines = Vec::new();
            let mut old_pos = 0usize;
            let mut new_pos = 0usize;

            // Walk ops up to range_start to find old_pos / new_pos.
            for op in ops.iter().take(range_start) {
                match op {
                    EditOp::Equal => {
                        old_pos += 1;
                        new_pos += 1;
                    }
                    EditOp::Delete => {
                        old_pos += 1;
                    }
                    EditOp::Insert => {
                        new_pos += 1;
                    }
                }
            }

            let hunk_old_start = old_pos + 1; // 1-based
            let hunk_new_start = new_pos + 1;
            let mut hunk_old_count = 0;
            let mut hunk_new_count = 0;

            for op in ops.iter().take(range_end).skip(range_start) {
                match op {
                    EditOp::Equal => {
                        let text = old_lines.get(old_pos).copied().unwrap_or("");
                        lines.push(DiffLine::Context(text.to_string()));
                        old_pos += 1;
                        new_pos += 1;
                        hunk_old_count += 1;
                        hunk_new_count += 1;
                    }
                    EditOp::Delete => {
                        let text = old_lines.get(old_pos).copied().unwrap_or("");
                        lines.push(DiffLine::Removed(text.to_string()));
                        old_pos += 1;
                        hunk_old_count += 1;
                    }
                    EditOp::Insert => {
                        let text = new_lines.get(new_pos).copied().unwrap_or("");
                        lines.push(DiffLine::Added(text.to_string()));
                        new_pos += 1;
                        hunk_new_count += 1;
                    }
                }
            }

            hunks.push(DiffHunk {
                old_start: hunk_old_start,
                old_count: hunk_old_count,
                new_start: hunk_new_start,
                new_count: hunk_new_count,
                lines,
            });
        }

        hunks
    }
}

// ─────────────────────────── Internal helpers ────────────────────────────────

/// Edit operation produced by the LCS diff algorithm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EditOp {
    Equal,
    Insert,
    Delete,
}

/// Split text into lines, handling the trailing newline correctly.
///
/// A trailing newline does NOT produce an extra empty element, matching the
/// convention that `"foo\nbar\n"` is two lines, not three.
fn split_lines(text: &str) -> Vec<&str> {
    if text.is_empty() {
        return Vec::new();
    }
    let mut lines: Vec<&str> = text.split('\n').collect();
    // Remove trailing empty string from a final newline.
    if lines.last() == Some(&"") {
        lines.pop();
    }
    lines
}

/// Compute the edit operations (Equal / Insert / Delete) between two line
/// sequences using a standard LCS dynamic-programming algorithm.
fn compute_edit_ops<'a>(old: &[&'a str], new: &[&'a str]) -> Vec<EditOp> {
    let m = old.len();
    let n = new.len();

    // Build LCS table.
    let mut table = vec![vec![0u32; n + 1]; m + 1];
    for i in 1..=m {
        for j in 1..=n {
            if old[i - 1] == new[j - 1] {
                table[i][j] = table[i - 1][j - 1] + 1;
            } else {
                table[i][j] = table[i - 1][j].max(table[i][j - 1]);
            }
        }
    }

    // Backtrack to produce edit ops.
    let mut ops = Vec::new();
    let mut i = m;
    let mut j = n;

    while i > 0 || j > 0 {
        if i > 0 && j > 0 && old[i - 1] == new[j - 1] {
            ops.push(EditOp::Equal);
            i -= 1;
            j -= 1;
        } else if j > 0 && (i == 0 || table[i][j - 1] >= table[i - 1][j]) {
            ops.push(EditOp::Insert);
            j -= 1;
        } else {
            ops.push(EditOp::Delete);
            i -= 1;
        }
    }

    ops.reverse();
    ops
}

/// Parse a `@@ -old_start,old_count +new_start,new_count @@` header line.
fn parse_hunk_header(line: &str) -> Result<(usize, usize, usize, usize), String> {
    // Strip leading "@@ " and trailing " @@" (and any section heading after).
    let inner = line
        .strip_prefix("@@ ")
        .and_then(|s| {
            if let Some(pos) = s.find(" @@") {
                Some(&s[..pos])
            } else {
                None
            }
        })
        .ok_or_else(|| format!("Invalid hunk header: {line}"))?;

    let parts: Vec<&str> = inner.split_whitespace().collect();
    if parts.len() != 2 {
        return Err(format!("Expected 2 parts in hunk header, got {}: {line}", parts.len()));
    }

    let old_part = parts[0]
        .strip_prefix('-')
        .ok_or_else(|| format!("Missing '-' in old range: {line}"))?;
    let new_part = parts[1]
        .strip_prefix('+')
        .ok_or_else(|| format!("Missing '+' in new range: {line}"))?;

    let (old_start, old_count) = parse_range(old_part, line)?;
    let (new_start, new_count) = parse_range(new_part, line)?;

    Ok((old_start, old_count, new_start, new_count))
}

/// Parse a range like `10,5` or `10` (count defaults to 1 if omitted).
fn parse_range(s: &str, context: &str) -> Result<(usize, usize), String> {
    if let Some((start_s, count_s)) = s.split_once(',') {
        let start: usize = start_s
            .parse()
            .map_err(|e| format!("Bad start in {context}: {e}"))?;
        let count: usize = count_s
            .parse()
            .map_err(|e| format!("Bad count in {context}: {e}"))?;
        Ok((start, count))
    } else {
        let start: usize = s
            .parse()
            .map_err(|e| format!("Bad range in {context}: {e}"))?;
        Ok((start, 1))
    }
}

// ─────────────────────────── Tests ───────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn engine() -> DiffEngine {
        DiffEngine::new()
    }

    // ── 1. No changes ────────────────────────────────────────────────────

    #[test]
    fn test_generate_diff_no_changes() {
        let e = engine();
        let content = "fn main() {\n    println!(\"hello\");\n}\n";
        let diff = e.generate_diff("src/main.rs", content, content);
        assert!(diff.hunks.is_empty());
        assert_eq!(diff.additions, 0);
        assert_eq!(diff.deletions, 0);
    }

    // ── 2. Single line change ────────────────────────────────────────────

    #[test]
    fn test_generate_diff_single_line_change() {
        let e = engine();
        let old = "fn main() {\n    println!(\"hello\");\n}\n";
        let new = "fn main() {\n    println!(\"hello world\");\n}\n";
        let diff = e.generate_diff("src/main.rs", old, new);

        assert_eq!(diff.additions, 1);
        assert_eq!(diff.deletions, 1);
        assert_eq!(diff.hunks.len(), 1);
    }

    // ── 3. Add lines ────────────────────────────────────────────────────

    #[test]
    fn test_generate_diff_add_lines() {
        let e = engine();
        let old = "fn main() {\n    println!(\"hello\");\n}\n";
        let new = "fn main() {\n    println!(\"hello\");\n    eprintln!(\"debug\");\n}\n";
        let diff = e.generate_diff("src/main.rs", old, new);

        assert_eq!(diff.additions, 1);
        assert_eq!(diff.deletions, 0);
    }

    // ── 4. Remove lines ─────────────────────────────────────────────────

    #[test]
    fn test_generate_diff_remove_lines() {
        let e = engine();
        let old = "line1\nline2\nline3\nline4\n";
        let new = "line1\nline4\n";
        let diff = e.generate_diff("file.txt", old, new);

        assert_eq!(diff.deletions, 2);
        assert_eq!(diff.additions, 0);
    }

    // ── 5. Mixed changes ────────────────────────────────────────────────

    #[test]
    fn test_generate_diff_mixed_changes() {
        let e = engine();
        let old = "fn main() {\n    println!(\"hello\");\n}\n";
        let new = "fn main() {\n    println!(\"hello world\");\n    eprintln!(\"debug\");\n}\n";
        let diff = e.generate_diff("src/main.rs", old, new);

        assert!(diff.additions >= 1);
        assert!(diff.deletions >= 1 || diff.additions >= 2);
        assert!(!diff.hunks.is_empty());
    }

    // ── 6. Context lines ────────────────────────────────────────────────

    #[test]
    fn test_generate_diff_context_lines() {
        let e = engine();
        let old = "a\nb\nc\nd\ne\nf\ng\nh\n";
        let new = "a\nb\nc\nX\ne\nf\ng\nh\n";
        let diff = e.generate_diff("file.txt", old, new);

        assert_eq!(diff.hunks.len(), 1);
        // Context lines should be present around the change.
        let ctx_count = diff.hunks[0]
            .lines
            .iter()
            .filter(|l| matches!(l, DiffLine::Context(_)))
            .count();
        assert!(ctx_count > 0, "Expected context lines around the change");
    }

    // ── 7. Apply single change ──────────────────────────────────────────

    #[test]
    fn test_apply_diff_single_change() {
        let e = engine();
        let old = "fn main() {\n    println!(\"hello\");\n}\n";
        let new = "fn main() {\n    println!(\"hello world\");\n}\n";

        let diff = e.generate_diff("src/main.rs", old, new);
        let result = e.apply_diff(old, &diff);

        assert!(result.success, "Apply failed: {:?}", result.error);
        assert_eq!(result.new_content.unwrap(), new);
    }

    // ── 8. Apply multiple hunks ─────────────────────────────────────────

    #[test]
    fn test_apply_diff_multiple_hunks() {
        let e = DiffEngine::with_config(DiffConfig {
            context_lines: 1,
            minimize: true,
        });
        // Create content with changes far apart so they produce separate hunks.
        let mut old_lines = Vec::new();
        for i in 0..20 {
            old_lines.push(format!("line {i}"));
        }
        let old = old_lines.join("\n") + "\n";

        let mut new_lines = old_lines.clone();
        new_lines[2] = "CHANGED line 2".to_string();
        new_lines[17] = "CHANGED line 17".to_string();
        let new = new_lines.join("\n") + "\n";

        let diff = e.generate_diff("file.txt", &old, &new);
        assert!(
            diff.hunks.len() >= 2,
            "Expected at least 2 hunks, got {}",
            diff.hunks.len()
        );

        let result = e.apply_diff(&old, &diff);
        assert!(result.success, "Apply failed: {:?}", result.error);
        assert_eq!(result.new_content.unwrap(), new);
    }

    // ── 9. Apply add to empty ───────────────────────────────────────────

    #[test]
    fn test_apply_diff_add_to_empty() {
        let e = engine();
        let old = "";
        let new = "fn main() {\n    println!(\"hello\");\n}\n";

        let diff = e.generate_diff("new_file.rs", old, new);
        assert_eq!(diff.operation, DiffOperation::Create);

        let result = e.apply_diff(old, &diff);
        assert!(result.success);
        assert_eq!(result.new_content.unwrap(), new);
    }

    // ── 10. Apply delete all ────────────────────────────────────────────

    #[test]
    fn test_apply_diff_delete_all() {
        let e = engine();
        let old = "fn main() {\n    println!(\"hello\");\n}\n";
        let new = "";

        let diff = e.generate_diff("old_file.rs", old, new);
        assert_eq!(diff.operation, DiffOperation::Delete);

        let result = e.apply_diff(old, &diff);
        assert!(result.success);
        assert_eq!(result.new_content.unwrap(), "");
    }

    // ── 11. Validate clean diff ─────────────────────────────────────────

    #[test]
    fn test_validate_diff_clean() {
        let e = engine();
        let old = "fn main() {\n    println!(\"hello\");\n}\n";
        let new = "fn main() {\n    println!(\"hello world\");\n}\n";

        let diff = e.generate_diff("src/main.rs", old, new);
        let validation = e.validate_diff(old, &diff);

        assert!(validation.valid);
        assert!(validation.issues.is_empty());
    }

    // ── 12. Validate context mismatch ───────────────────────────────────

    #[test]
    fn test_validate_diff_context_mismatch() {
        let e = engine();
        let old = "fn main() {\n    println!(\"hello\");\n}\n";
        let new = "fn main() {\n    println!(\"hello world\");\n}\n";

        let diff = e.generate_diff("src/main.rs", old, new);

        // Validate against different content.
        let wrong = "fn other() {\n    eprintln!(\"nope\");\n}\n";
        let validation = e.validate_diff(wrong, &diff);

        assert!(!validation.valid);
        assert!(!validation.issues.is_empty());
    }

    // ── 13. Format unified ──────────────────────────────────────────────

    #[test]
    fn test_format_unified() {
        let e = engine();
        let old = "fn main() {\n    println!(\"hello\");\n}\n";
        let new = "fn main() {\n    println!(\"hello world\");\n}\n";

        let diff = e.generate_diff("src/main.rs", old, new);
        let formatted = e.format_unified(&diff);

        assert!(formatted.contains("--- a/src/main.rs"));
        assert!(formatted.contains("+++ b/src/main.rs"));
        assert!(formatted.contains("@@ "));
        assert!(formatted.contains("-    println!(\"hello\");"));
        assert!(formatted.contains("+    println!(\"hello world\");"));
    }

    // ── 14. Parse unified ───────────────────────────────────────────────

    #[test]
    fn test_parse_unified() {
        let text = "\
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,3 +1,3 @@
 fn main() {
-    println!(\"hello\");
+    println!(\"hello world\");
 }
";
        let e = engine();
        let diff = e.parse_unified(text).unwrap();

        assert_eq!(diff.path, "src/main.rs");
        assert_eq!(diff.operation, DiffOperation::Modify);
        assert_eq!(diff.hunks.len(), 1);
        assert_eq!(diff.additions, 1);
        assert_eq!(diff.deletions, 1);
    }

    // ── 15. Roundtrip format→parse ──────────────────────────────────────

    #[test]
    fn test_roundtrip_format_parse() {
        let e = engine();
        let old = "line1\nline2\nline3\nline4\nline5\n";
        let new = "line1\nline2\nmodified\nline4\nline5\nextra\n";

        let diff = e.generate_diff("file.txt", old, new);
        let formatted = e.format_unified(&diff);
        let parsed = e.parse_unified(&formatted).unwrap();

        assert_eq!(parsed.path, diff.path);
        assert_eq!(parsed.additions, diff.additions);
        assert_eq!(parsed.deletions, diff.deletions);
        assert_eq!(parsed.hunks.len(), diff.hunks.len());

        // The parsed diff should also apply correctly.
        let result = e.apply_diff(old, &parsed);
        assert!(result.success, "Roundtrip apply failed: {:?}", result.error);
        assert_eq!(result.new_content.unwrap(), new);
    }

    // ── 16. Minimize diff ───────────────────────────────────────────────

    #[test]
    fn test_minimize_diff() {
        let e = engine();
        let old = "a\nb\nc\nd\ne\nf\ng\nh\n";
        let new = "a\nb\nc\nX\ne\nf\ng\nh\n";

        let normal = e.generate_diff("file.txt", old, new);
        let minimal = e.minimize_diff("file.txt", old, new);

        // The minimized diff should have fewer or equal context lines.
        let normal_ctx: usize = normal
            .hunks
            .iter()
            .flat_map(|h| h.lines.iter())
            .filter(|l| matches!(l, DiffLine::Context(_)))
            .count();
        let minimal_ctx: usize = minimal
            .hunks
            .iter()
            .flat_map(|h| h.lines.iter())
            .filter(|l| matches!(l, DiffLine::Context(_)))
            .count();

        assert!(minimal_ctx <= normal_ctx);
        // Both should still have the same actual changes.
        assert_eq!(normal.additions, minimal.additions);
        assert_eq!(normal.deletions, minimal.deletions);
    }

    // ── 17. Create plan with multiple files ─────────────────────────────

    #[test]
    fn test_create_plan_multiple_files() {
        let e = engine();
        let changes: Vec<(&str, &str, &str)> = vec![
            ("src/lib.rs", "pub mod a;\n", "pub mod a;\npub mod b;\n"),
            (
                "src/b.rs",
                "",
                "pub fn hello() {\n    println!(\"hello\");\n}\n",
            ),
        ];

        let plan = e.create_plan("Add module b", &changes);

        assert_eq!(plan.description, "Add module b");
        assert_eq!(plan.diffs.len(), 2);
        assert!(plan.total_additions >= 2);
        assert!(plan.estimated_tokens > 0);
    }

    // ── 18. Apply plan ──────────────────────────────────────────────────

    #[test]
    fn test_apply_plan() {
        let e = engine();
        let old_lib = "pub mod a;\n";
        let new_lib = "pub mod a;\npub mod b;\n";
        let old_b = "";
        let new_b = "pub fn hello() {\n    println!(\"hello\");\n}\n";

        let changes: Vec<(&str, &str, &str)> = vec![
            ("src/lib.rs", old_lib, new_lib),
            ("src/b.rs", old_b, new_b),
        ];
        let plan = e.create_plan("Add module b", &changes);

        let contents = vec![("src/lib.rs", old_lib), ("src/b.rs", old_b)];
        let results = e.apply_plan(&contents, &plan);

        assert_eq!(results.len(), 2);
        for r in &results {
            assert!(r.success, "Failed for {}: {:?}", r.path, r.error);
        }
        assert_eq!(results[0].new_content.as_deref().unwrap(), new_lib);
        assert_eq!(results[1].new_content.as_deref().unwrap(), new_b);
    }

    // ── 19. Estimate tokens ─────────────────────────────────────────────

    #[test]
    fn test_estimate_tokens() {
        let e = engine();
        let old = "fn main() {\n    println!(\"hello\");\n}\n";
        let new = "fn main() {\n    println!(\"hello world\");\n    eprintln!(\"debug\");\n}\n";

        let diff = e.generate_diff("src/main.rs", old, new);
        let tokens = e.estimate_tokens(&diff);

        assert!(tokens > 0, "Token estimate should be positive");
        // Sanity: a small diff should not be thousands of tokens.
        assert!(tokens < 500, "Token estimate too high for small diff: {tokens}");
    }

    // ── 20. Create operation ────────────────────────────────────────────

    #[test]
    fn test_diff_operation_create() {
        let e = engine();
        let diff = e.generate_diff(
            "new.rs",
            "",
            "fn new() {}\n",
        );
        assert_eq!(diff.operation, DiffOperation::Create);
        assert_eq!(diff.additions, 1);
        assert_eq!(diff.deletions, 0);
    }

    // ── 21. Delete operation ────────────────────────────────────────────

    #[test]
    fn test_diff_operation_delete() {
        let e = engine();
        let diff = e.generate_diff("old.rs", "fn old() {}\n", "");
        assert_eq!(diff.operation, DiffOperation::Delete);
        assert_eq!(diff.deletions, 1);
        assert_eq!(diff.additions, 0);
    }

    // ── 22. DiffLine equality ───────────────────────────────────────────

    #[test]
    fn test_diff_line_equality() {
        let ctx1 = DiffLine::Context("hello".to_string());
        let ctx2 = DiffLine::Context("hello".to_string());
        let add = DiffLine::Added("hello".to_string());
        let rem = DiffLine::Removed("hello".to_string());

        assert_eq!(ctx1, ctx2);
        assert_ne!(ctx1, add);
        assert_ne!(add, rem);
        assert_ne!(ctx1, rem);

        // Same variant, different content.
        let ctx3 = DiffLine::Context("world".to_string());
        assert_ne!(ctx1, ctx3);
    }
}
