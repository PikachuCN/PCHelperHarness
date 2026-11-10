use super::{OutputCallback, ToolOutput, ToolRequest, ToolResult};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs::{self, File};
use std::io::Read as _;
use std::path::{Path, PathBuf};
use tokio_util::sync::CancellationToken;

const MAX_VISITED_ENTRIES: usize = 20_000;
const MAX_FILE_CONTENT_BYTES: usize = 1024 * 1024;
const MAX_TOTAL_CONTENT_BYTES: usize = 50 * 1024 * 1024;
const MAX_WARNINGS: usize = 20;

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum SearchIn {
    Name,
    Content,
    Both,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
struct SearchOptions {
    path: String,
    query: String,
    search_in: Option<SearchIn>,
    extensions: Option<Vec<String>>,
    max_results: Option<usize>,
    max_depth: Option<usize>,
    include_hidden: Option<bool>,
}

struct ValidatedOptions {
    root: PathBuf,
    query: String,
    search_in: SearchIn,
    extensions: Vec<String>,
    max_results: usize,
    max_depth: usize,
    include_hidden: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SearchMatch {
    path: String,
    size: u64,
    modified_at: Option<String>,
    summary: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SearchReport {
    path: String,
    query: String,
    search_in: SearchIn,
    results: Vec<SearchMatch>,
    visited_entries: usize,
    content_bytes_read: u64,
    truncated: bool,
    warnings: Vec<String>,
}

impl Serialize for SearchIn {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(match self {
            Self::Name => "name",
            Self::Content => "content",
            Self::Both => "both",
        })
    }
}

#[derive(Debug)]
enum SearchError {
    Cancelled,
    Invalid(String),
}

pub async fn execute_search(
    request: &ToolRequest,
    cancel: CancellationToken,
    on_output: OutputCallback,
) -> ToolResult {
    let options = match SearchOptions::parse(&request.arguments) {
        Ok(options) => options,
        Err(error) => return ToolResult::error(request, error),
    };
    let search_cancel = cancel.clone();
    let mut task = tokio::task::spawn_blocking(move || search(options, search_cancel));
    let result = tokio::select! {
        result = &mut task => result,
        _ = cancel.cancelled() => {
            return cancelled_result(request, &on_output);
        }
    };

    match result {
        Ok(Ok(report)) => match serde_json::to_string_pretty(&report) {
            Ok(content) => {
                on_output(ToolOutput {
                    call_id: request.id.clone(),
                    stream: "stdout".to_string(),
                    text: content.clone(),
                });
                ToolResult {
                    call_id: request.id.clone(),
                    name: request.name.clone(),
                    content,
                    exit_code: Some(0),
                    duration_ms: 0,
                    timed_out: false,
                    cancelled: false,
                    truncated: report.truncated,
                    risk: request.risk.clone(),
                }
            }
            Err(error) => ToolResult::error(request, format!("格式化搜索结果失败：{error}")),
        },
        Ok(Err(SearchError::Cancelled)) => cancelled_result(request, &on_output),
        Ok(Err(SearchError::Invalid(error))) => {
            on_output(ToolOutput {
                call_id: request.id.clone(),
                stream: "stderr".to_string(),
                text: format!("错误：{error}\n"),
            });
            ToolResult::error(request, error)
        }
        Err(error) => ToolResult::error(request, format!("文件搜索任务失败：{error}")),
    }
}

fn cancelled_result(request: &ToolRequest, on_output: &OutputCallback) -> ToolResult {
    on_output(ToolOutput {
        call_id: request.id.clone(),
        stream: "stdout".to_string(),
        text: "文件搜索已取消\n".to_string(),
    });
    let mut result = ToolResult::error(request, "文件搜索已取消");
    result.cancelled = true;
    result.exit_code = None;
    result
}

impl SearchOptions {
    fn parse(arguments: &Value) -> Result<ValidatedOptions, String> {
        let options: SearchOptions = serde_json::from_value(arguments.clone())
            .map_err(|error| format!("search_files 参数无效：{error}"))?;
        let path = options.path.trim();
        if path.is_empty() || path.chars().count() > 32_768 {
            return Err("搜索路径不能为空且不能超过 32768 个字符".to_string());
        }
        let query = options.query.trim();
        if query.is_empty() || query.chars().count() > 1024 {
            return Err("搜索内容不能为空且不能超过 1024 个字符".to_string());
        }
        let max_results = options.max_results.unwrap_or(50);
        if !(1..=200).contains(&max_results) {
            return Err("max_results 应在 1 到 200 之间".to_string());
        }
        let max_depth = options.max_depth.unwrap_or(8);
        if !(1..=12).contains(&max_depth) {
            return Err("max_depth 应在 1 到 12 之间".to_string());
        }
        let extensions = options.extensions.unwrap_or_default();
        if extensions.len() > 32 {
            return Err("extensions 最多允许 32 项".to_string());
        }
        let extensions = extensions
            .into_iter()
            .map(|extension| {
                let extension = extension
                    .trim()
                    .trim_start_matches("*.")
                    .trim_start_matches('.')
                    .to_ascii_lowercase();
                if extension.is_empty()
                    || extension.len() > 32
                    || !extension
                        .chars()
                        .all(|character| character.is_ascii_alphanumeric())
                {
                    Err(format!("无效的文件扩展名：{extension}"))
                } else {
                    Ok(extension)
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ValidatedOptions {
            root: PathBuf::from(path),
            query: query.to_string(),
            search_in: options.search_in.unwrap_or(SearchIn::Both),
            extensions,
            max_results,
            max_depth,
            include_hidden: options.include_hidden.unwrap_or(false),
        })
    }
}

fn search(
    options: ValidatedOptions,
    cancel: CancellationToken,
) -> Result<SearchReport, SearchError> {
    let root_metadata = fs::symlink_metadata(&options.root).map_err(|error| {
        SearchError::Invalid(format!(
            "无法访问搜索路径“{}”：{error}",
            options.root.display()
        ))
    })?;
    if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
        return Err(SearchError::Invalid(format!(
            "搜索路径必须是非符号链接目录：{}",
            options.root.display()
        )));
    }

    let query = options.query.to_lowercase();
    let mut report = SearchReport {
        path: options.root.display().to_string(),
        query: options.query.clone(),
        search_in: options.search_in,
        results: Vec::new(),
        visited_entries: 0,
        content_bytes_read: 0,
        truncated: false,
        warnings: Vec::new(),
    };
    let mut skipped_warnings = 0usize;
    let mut directories = vec![(options.root.clone(), 0usize)];

    while let Some((directory, depth)) = directories.pop() {
        if cancel.is_cancelled() {
            return Err(SearchError::Cancelled);
        }
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) => {
                if directory == options.root {
                    return Err(SearchError::Invalid(format!(
                        "无法读取搜索目录“{}”：{error}",
                        directory.display()
                    )));
                }
                add_warning(
                    &mut report,
                    &mut skipped_warnings,
                    format!("无法读取目录“{}”：{error}", directory.display()),
                );
                continue;
            }
        };

        for entry in entries {
            if cancel.is_cancelled() {
                return Err(SearchError::Cancelled);
            }
            if report.visited_entries >= MAX_VISITED_ENTRIES {
                report.truncated = true;
                add_warning(
                    &mut report,
                    &mut skipped_warnings,
                    format!("已达到 {MAX_VISITED_ENTRIES} 项遍历上限，搜索提前停止"),
                );
                return Ok(report);
            }
            report.visited_entries += 1;
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    add_warning(
                        &mut report,
                        &mut skipped_warnings,
                        format!("读取目录项失败：{error}"),
                    );
                    continue;
                }
            };
            let path = entry.path();
            let file_type = match entry.file_type() {
                Ok(file_type) => file_type,
                Err(error) => {
                    add_warning(
                        &mut report,
                        &mut skipped_warnings,
                        format!("无法读取“{}”的类型：{error}", path.display()),
                    );
                    continue;
                }
            };
            if file_type.is_symlink() {
                continue;
            }
            let metadata = match entry.metadata() {
                Ok(metadata) => metadata,
                Err(error) => {
                    add_warning(
                        &mut report,
                        &mut skipped_warnings,
                        format!("无法读取“{}”的属性：{error}", path.display()),
                    );
                    continue;
                }
            };
            if !options.include_hidden && is_hidden(&path, &metadata) {
                continue;
            }
            if file_type.is_dir() {
                if depth + 1 < options.max_depth {
                    directories.push((path, depth + 1));
                }
                continue;
            }
            if !file_type.is_file() || !matches_extension(&path, &options.extensions) {
                continue;
            }

            let mut summaries = Vec::new();
            if matches!(options.search_in, SearchIn::Name | SearchIn::Both)
                && entry
                    .file_name()
                    .to_string_lossy()
                    .to_lowercase()
                    .contains(&query)
            {
                summaries.push("文件名匹配".to_string());
            }
            if matches!(options.search_in, SearchIn::Content | SearchIn::Both)
                && report.content_bytes_read < MAX_TOTAL_CONTENT_BYTES as u64
            {
                match search_content(&path, &query, report.content_bytes_read) {
                    Ok((summary, bytes_read)) => {
                        report.content_bytes_read += bytes_read;
                        if let Some(summary) = summary {
                            summaries.push(summary);
                        }
                    }
                    Err(error) => add_warning(
                        &mut report,
                        &mut skipped_warnings,
                        format!("无法读取文件“{}”：{error}", path.display()),
                    ),
                }
            }
            if summaries.is_empty() {
                continue;
            }
            report.results.push(SearchMatch {
                path: path.display().to_string(),
                size: metadata.len(),
                modified_at: metadata.modified().ok().map(|time| {
                    let time: DateTime<Utc> = time.into();
                    time.to_rfc3339()
                }),
                summary: summaries.join("；"),
            });
            if report.results.len() >= options.max_results {
                report.truncated = true;
                break;
            }
        }
        if report.results.len() >= options.max_results {
            break;
        }
    }
    if report.content_bytes_read >= MAX_TOTAL_CONTENT_BYTES as u64 {
        report.truncated = true;
        add_warning(
            &mut report,
            &mut skipped_warnings,
            "已达到 50 MB 内容读取上限，其余文件未读取内容".to_string(),
        );
    }
    if skipped_warnings > 0 {
        report
            .warnings
            .push(format!("另有 {skipped_warnings} 条读取警告未列出"));
    }
    Ok(report)
}

fn search_content(
    path: &Path,
    query: &str,
    total_bytes_read: u64,
) -> Result<(Option<String>, u64), std::io::Error> {
    let remaining = MAX_TOTAL_CONTENT_BYTES as u64 - total_bytes_read;
    let limit = remaining.min(MAX_FILE_CONTENT_BYTES as u64);
    let file = File::open(path)?;
    let mut bytes = Vec::with_capacity(limit as usize);
    file.take(limit).read_to_end(&mut bytes)?;
    let bytes_read = bytes.len() as u64;
    if bytes.contains(&0) {
        return Ok((None, bytes_read));
    }
    let Ok(content) = String::from_utf8(bytes) else {
        return Ok((None, bytes_read));
    };
    let mut lines = Vec::new();
    for (index, line) in content.lines().enumerate() {
        if line.to_lowercase().contains(query) {
            lines.push(format!("第 {} 行：{}", index + 1, truncate_line(line)));
            if lines.len() == 3 {
                break;
            }
        }
    }
    if lines.is_empty() {
        Ok((None, bytes_read))
    } else {
        Ok((Some(lines.join("；")), bytes_read))
    }
}

fn matches_extension(path: &Path, extensions: &[String]) -> bool {
    extensions.is_empty()
        || path.extension().is_some_and(|extension| {
            let extension = extension.to_string_lossy().to_ascii_lowercase();
            extensions.iter().any(|allowed| allowed == &extension)
        })
}

fn truncate_line(line: &str) -> String {
    const MAX_CHARS: usize = 240;
    let line = line.trim();
    if line.chars().count() <= MAX_CHARS {
        line.to_string()
    } else {
        let mut summary: String = line.chars().take(MAX_CHARS).collect();
        summary.push('…');
        summary
    }
}

fn add_warning(report: &mut SearchReport, skipped: &mut usize, warning: String) {
    if report.warnings.len() < MAX_WARNINGS {
        report.warnings.push(warning);
    } else {
        *skipped += 1;
    }
}

#[cfg(not(windows))]
fn is_hidden(path: &Path, _metadata: &fs::Metadata) -> bool {
    path.file_name()
        .is_some_and(|name| name.to_string_lossy().starts_with('.'))
}

#[cfg(windows)]
fn is_hidden(path: &Path, metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt as _;
    const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
    path.file_name()
        .is_some_and(|name| name.to_string_lossy().starts_with('.'))
        || metadata.file_attributes() & FILE_ATTRIBUTE_HIDDEN != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options(directory: &Path, query: &str) -> Value {
        serde_json::json!({
            "path": directory,
            "query": query,
            "search_in": "both",
            "max_results": 50,
            "max_depth": 4
        })
    }

    fn fixture() -> tempfile::TempDir {
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join("note.txt"), "hello\nneedle in text\n").unwrap();
        fs::write(directory.path().join("other.rs"), "fn needle() {}\n").unwrap();
        fs::write(directory.path().join("binary.txt"), b"needle\0binary").unwrap();
        fs::create_dir(directory.path().join("nested")).unwrap();
        fs::write(
            directory.path().join("nested").join("deep.txt"),
            "deep needle",
        )
        .unwrap();
        directory
    }

    #[test]
    fn searches_file_names_without_reading_content() {
        let directory = fixture();
        let mut arguments = options(directory.path(), "note");
        arguments["search_in"] = Value::String("name".to_string());
        let report = search(
            SearchOptions::parse(&arguments).unwrap(),
            CancellationToken::new(),
        )
        .unwrap();
        assert_eq!(report.results.len(), 1);
        assert!(report.results[0].path.ends_with("note.txt"));
        assert_eq!(report.results[0].summary, "文件名匹配");
        assert_eq!(report.content_bytes_read, 0);
        assert!(report.results[0].modified_at.is_some());
    }

    #[test]
    fn searches_utf8_content_and_skips_binary_files() {
        let directory = fixture();
        let mut arguments = options(directory.path(), "needle");
        arguments["search_in"] = Value::String("content".to_string());
        let report = search(
            SearchOptions::parse(&arguments).unwrap(),
            CancellationToken::new(),
        )
        .unwrap();
        assert_eq!(report.results.len(), 3);
        assert!(report
            .results
            .iter()
            .any(|result| result.path.ends_with("note.txt") && result.summary.contains("第 2 行")));
        assert!(report
            .results
            .iter()
            .all(|result| !result.path.ends_with("binary.txt")));
    }

    #[test]
    fn respects_depth_and_extension_filters() {
        let directory = fixture();
        let mut arguments = options(directory.path(), "needle");
        arguments["max_depth"] = Value::from(1);
        let report = search(
            SearchOptions::parse(&arguments).unwrap(),
            CancellationToken::new(),
        )
        .unwrap();
        assert!(report
            .results
            .iter()
            .all(|result| !result.path.ends_with("deep.txt")));

        arguments["max_depth"] = Value::from(4);
        arguments["extensions"] = serde_json::json!([".rs"]);
        let report = search(
            SearchOptions::parse(&arguments).unwrap(),
            CancellationToken::new(),
        )
        .unwrap();
        assert_eq!(report.results.len(), 1);
        assert!(report.results[0].path.ends_with("other.rs"));
    }

    #[test]
    fn validates_limits_and_missing_paths() {
        let directory = fixture();
        for (key, value) in [
            ("max_results", Value::from(0)),
            ("max_results", Value::from(201)),
            ("max_depth", Value::from(0)),
            ("max_depth", Value::from(13)),
        ] {
            let mut arguments = options(directory.path(), "needle");
            arguments[key] = value;
            assert!(SearchOptions::parse(&arguments).is_err());
        }
        let mut arguments = options(directory.path(), " ");
        arguments["query"] = Value::String(" ".to_string());
        assert!(SearchOptions::parse(&arguments).is_err());
        let arguments = options(&directory.path().join("missing"), "needle");
        let options = SearchOptions::parse(&arguments).unwrap();
        assert!(matches!(
            search(options, CancellationToken::new()),
            Err(SearchError::Invalid(_))
        ));
    }

    #[test]
    fn hidden_files_are_opt_in() {
        let directory = fixture();
        fs::write(directory.path().join(".hidden.txt"), "needle").unwrap();
        let arguments = options(directory.path(), "needle");
        let report = search(
            SearchOptions::parse(&arguments).unwrap(),
            CancellationToken::new(),
        )
        .unwrap();
        assert!(report
            .results
            .iter()
            .all(|result| !result.path.ends_with(".hidden.txt")));
        let mut arguments = options(directory.path(), "needle");
        arguments["include_hidden"] = Value::Bool(true);
        let report = search(
            SearchOptions::parse(&arguments).unwrap(),
            CancellationToken::new(),
        )
        .unwrap();
        assert!(report
            .results
            .iter()
            .any(|result| result.path.ends_with(".hidden.txt")));
    }
}
