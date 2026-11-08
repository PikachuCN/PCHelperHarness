use crate::config;
use include_dir::{include_dir, Dir};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Component, Path, PathBuf};
use thiserror::Error;

static EMBEDDED_SKILLS: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/skills");

#[derive(Debug, Error)]
pub enum SkillError {
    #[error("无法访问 Skills 目录：{0}")]
    Io(#[from] std::io::Error),
    #[error("Skill 格式不正确：{0}")]
    Format(String),
    #[error("找不到 Skill：{0}")]
    NotFound(String),
    #[error("文件路径超出 Skill 目录")]
    InvalidPath,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillMeta {
    pub name: String,
    pub description: String,
    pub source: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadedSkill {
    pub name: String,
    pub description: String,
    pub content: String,
    pub files: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct Frontmatter {
    name: String,
    description: String,
}

pub fn ensure_installed() -> Result<PathBuf, SkillError> {
    let root = config::skills_dir();
    if !root.exists() {
        fs::create_dir_all(&root)?;
        extract_dir(&EMBEDDED_SKILLS, &root)?;
    }
    Ok(root)
}

fn extract_dir(dir: &Dir<'_>, target: &Path) -> Result<(), SkillError> {
    fs::create_dir_all(target)?;
    for file in dir.files() {
        let relative = file
            .path()
            .strip_prefix(dir.path())
            .map_err(|error| SkillError::Format(format!("嵌入资源路径无效：{error}")))?;
        let path = target.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        if !path.exists() {
            fs::write(path, file.contents())?;
        }
    }
    for child in dir.dirs() {
        let name = child
            .path()
            .file_name()
            .ok_or_else(|| SkillError::Format("嵌入 Skill 缺少目录名".to_string()))?;
        extract_dir(child, &target.join(name))?;
    }
    Ok(())
}

fn parse_skill(path: &Path, builtin_names: &[String]) -> Result<SkillMeta, SkillError> {
    let text = fs::read_to_string(path)?;
    let (frontmatter, _) = parse_frontmatter(&text)?;
    let source = if builtin_names.iter().any(|name| name == &frontmatter.name) {
        "内置"
    } else {
        "用户"
    };
    Ok(SkillMeta {
        name: frontmatter.name,
        description: frontmatter.description,
        source: source.to_string(),
        path: path.parent().unwrap_or(path).display().to_string(),
    })
}

fn parse_frontmatter(text: &str) -> Result<(Frontmatter, String), SkillError> {
    let normalized = text.replace("\r\n", "\n");
    let rest = normalized
        .strip_prefix("---\n")
        .ok_or_else(|| SkillError::Format("SKILL.md 必须以 YAML frontmatter 开始".to_string()))?;
    let end = rest
        .find("\n---\n")
        .ok_or_else(|| SkillError::Format("SKILL.md 缺少 frontmatter 结束标记".to_string()))?;
    let yaml = &rest[..end];
    let body = &rest[end + 5..];
    let frontmatter: Frontmatter =
        serde_yaml_ng::from_str(yaml).map_err(|error| SkillError::Format(error.to_string()))?;
    if frontmatter.name.trim().is_empty() || frontmatter.description.trim().is_empty() {
        return Err(SkillError::Format(
            "name 和 description 不能为空".to_string(),
        ));
    }
    Ok((frontmatter, body.to_string()))
}

pub fn list() -> Result<Vec<SkillMeta>, SkillError> {
    let root = ensure_installed()?;
    let builtin_names = builtin_names();
    let mut skills = Vec::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let path = entry.path().join("SKILL.md");
        if path.is_file() {
            match parse_skill(&path, &builtin_names) {
                Ok(skill) => skills.push(skill),
                Err(error) => eprintln!("跳过 {}：{error}", path.display()),
            }
        }
    }
    skills.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(skills)
}

pub fn prompt_catalog() -> String {
    match list() {
        Ok(skills) if skills.is_empty() => "（暂无可用 Skills）".to_string(),
        Ok(skills) => skills
            .into_iter()
            .map(|skill| format!("- {}：{}", skill.name, skill.description))
            .collect::<Vec<_>>()
            .join("\n"),
        Err(error) => format!("（Skills 加载失败：{error}）"),
    }
}

fn validate_name(name: &str) -> Result<(), SkillError> {
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(SkillError::InvalidPath);
    }
    Ok(())
}

pub fn load(name: &str) -> Result<LoadedSkill, SkillError> {
    validate_name(name)?;
    let root = ensure_installed()?;
    let directory = root.join(name);
    let skill_path = directory.join("SKILL.md");
    if !skill_path.is_file() {
        return Err(SkillError::NotFound(name.to_string()));
    }
    let text = fs::read_to_string(&skill_path)?;
    let (frontmatter, body) = parse_frontmatter(&text)?;
    let mut files = Vec::new();
    collect_files(&directory, &directory, &mut files)?;
    files.sort();
    Ok(LoadedSkill {
        name: frontmatter.name,
        description: frontmatter.description,
        content: body,
        files,
    })
}

fn collect_files(root: &Path, dir: &Path, output: &mut Vec<String>) -> Result<(), SkillError> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            collect_files(root, &path, output)?;
        } else if let Ok(relative) = path.strip_prefix(root) {
            output.push(relative.to_string_lossy().replace('\\', "/"));
        }
    }
    Ok(())
}

pub fn read_file(name: &str, relative: &str, max_bytes: usize) -> Result<String, SkillError> {
    validate_name(name)?;
    let relative_path = Path::new(relative);
    if relative_path.is_absolute()
        || relative_path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(SkillError::InvalidPath);
    }
    let root = ensure_installed()?.join(name);
    let path = root.join(relative_path);
    if !path.is_file() {
        return Err(SkillError::NotFound(format!("{name}/{relative}")));
    }
    let bytes = fs::read(path)?;
    let text = String::from_utf8_lossy(&bytes[..bytes.len().min(max_bytes)]).to_string();
    Ok(text)
}

fn builtin_names() -> Vec<String> {
    EMBEDDED_SKILLS
        .dirs()
        .filter_map(|dir| dir.path().file_name())
        .map(|name| name.to_string_lossy().to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_anthropic_frontmatter() {
        let text = "---\nname: network-troubleshoot\ndescription: 排查网络连接问题\n---\n\n# 网络排查\n先诊断。";
        let (meta, body) = parse_frontmatter(text).unwrap();
        assert_eq!(meta.name, "network-troubleshoot");
        assert_eq!(meta.description, "排查网络连接问题");
        assert!(body.contains("先诊断"));
    }

    #[test]
    fn rejects_invalid_frontmatter() {
        assert!(parse_frontmatter("# 没有头部").is_err());
        assert!(parse_frontmatter("---\nname: x\n---\n正文").is_err());
    }

    #[test]
    fn rejects_unsafe_skill_names() {
        assert!(validate_name("../system32").is_err());
        assert!(validate_name("network-troubleshoot").is_ok());
    }

    #[test]
    fn rejects_parent_paths() {
        let result = std::panic::catch_unwind(|| {
            let path = Path::new("../secret.txt");
            path.components()
                .any(|part| !matches!(part, Component::Normal(_)))
        });
        assert_eq!(result.unwrap(), true);
    }

    #[test]
    fn extracts_skills_without_duplicate_directories_or_overwrites() {
        let directory = tempfile::tempdir().unwrap();
        extract_dir(&EMBEDDED_SKILLS, directory.path()).unwrap();
        for skill in EMBEDDED_SKILLS.dirs() {
            let name = skill.path().file_name().unwrap();
            let expected = directory.path().join(name).join("SKILL.md");
            assert!(expected.is_file(), "{}", expected.display());
            let duplicated = directory.path().join(name).join(name).join("SKILL.md");
            assert!(!duplicated.exists(), "{}", duplicated.display());
        }
        let existing = directory
            .path()
            .join("network-troubleshoot")
            .join("SKILL.md");
        fs::write(&existing, "用户修改").unwrap();
        extract_dir(&EMBEDDED_SKILLS, directory.path()).unwrap();
        assert_eq!(fs::read_to_string(existing).unwrap(), "用户修改");
    }
}
