use std::collections::HashSet;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::cli_catalog;
use crate::resources;

/// 技能目录名上限。对齐 VS Code 的 skill name 截断。
const SKILL_NAME_CHARS: usize = 64;
/// 描述上限。对齐 VS Code，超长描述不整段进提示词。
const SKILL_DESCRIPTION_CHARS: usize = 1024;
/// 名称和描述清单的字符预算。超出后只留名称。
const SKILL_LISTING_CHAR_BUDGET: usize = 15_000;
/// 被预算截掉的技能，名称列表自己的预算。
const SKILL_TRUNCATED_NAMES_CHAR_BUDGET: usize = 5_000;
/// 只读 frontmatter 用，避免为了描述把整份 SKILL.md 读进内存。
const SKILL_FRONTMATTER_READ_BYTES: u64 = 64 * 1024;

/// VS Code 默认的项目技能目录，排在各 CLI 约定之前。同名时先出现的留下。
const VSCODE_SKILL_DIRS: &[&str] = &[".agents/skills", ".github/skills", ".claude/skills"];

pub struct ProjectSkill {
    pub name: String,
    pub description: String,
    pub path: PathBuf,
}

/// 每个关联的 git 项目根目录收一层技能：`<根>/<约定>/<名称>/SKILL.md`。
/// 不往下递归，也不把嵌套仓库当成根。同名只保留先扫到的那个。
pub fn discover(workspace_roots: &[String]) -> Vec<ProjectSkill> {
    let conventions = skill_conventions();
    let mut seen_roots = HashSet::new();
    let mut seen_names = HashSet::new();
    let mut skills = Vec::new();
    for root in workspace_roots {
        let root = root.trim();
        if root.is_empty() {
            continue;
        }
        let Ok(root) = fs::canonicalize(root) else {
            continue;
        };
        if !root.is_dir() || !seen_roots.insert(root.clone()) {
            continue;
        }
        for relative in &conventions {
            collect_dir(&root.join(relative), &mut seen_names, &mut skills);
        }
    }
    skills
}

pub fn prompt_section(skills: &[ProjectSkill], locale: &str) -> String {
    prompt_section_limited(
        skills,
        locale,
        SKILL_LISTING_CHAR_BUDGET,
        SKILL_TRUNCATED_NAMES_CHAR_BUDGET,
    )
}

fn prompt_section_limited(
    skills: &[ProjectSkill],
    locale: &str,
    listing_budget: usize,
    names_budget: usize,
) -> String {
    if skills.is_empty() {
        return String::new();
    }
    let mut entries = Vec::new();
    let mut used = 0_usize;
    let mut truncated_at = skills.len();
    for (index, skill) in skills.iter().enumerate() {
        let entry = format!(
            "- **{}** — {}\n  `{}`",
            skill.name,
            skill.description,
            skill.path.display()
        );
        let addition = entry.chars().count() + 1;
        if used + addition > listing_budget {
            truncated_at = index;
            break;
        }
        used += addition;
        entries.push(entry);
    }
    let mut body = format!(
        "## Project Skills\n{}\n\n{}",
        intro(locale),
        entries.join("\n")
    );
    if truncated_at < skills.len() {
        body.push_str("\n\n");
        body.push_str(&truncated_names(
            locale,
            &skills[truncated_at..],
            names_budget,
        ));
    }
    body
}

fn skill_conventions() -> Vec<String> {
    let mut dirs: Vec<String> = VSCODE_SKILL_DIRS
        .iter()
        .map(|dir| (*dir).to_string())
        .collect();
    for owner in cli_catalog::skill_owners() {
        let Some(home) = cli_catalog::entry(owner).and_then(|item| item.skills_home.clone()) else {
            continue;
        };
        if !dirs.iter().any(|dir| dir == &home) {
            dirs.push(home);
        }
    }
    dirs
}

fn collect_dir(dir: &Path, seen_names: &mut HashSet<String>, skills: &mut Vec<ProjectSkill>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut children: Vec<_> = entries.flatten().collect();
    children.sort_by_key(|entry| entry.file_name());
    for entry in children {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let skill_file = path.join("SKILL.md");
        if !skill_file.is_file() {
            continue;
        }
        let name = clip_chars(&entry.file_name().to_string_lossy(), SKILL_NAME_CHARS);
        if name.is_empty() || !seen_names.insert(name.clone()) {
            continue;
        }
        let Some(description) = read_description(&skill_file) else {
            continue;
        };
        skills.push(ProjectSkill {
            name,
            description,
            path: skill_file,
        });
    }
}

fn read_description(path: &Path) -> Option<String> {
    let file = File::open(path).ok()?;
    let mut limited = file.take(SKILL_FRONTMATTER_READ_BYTES);
    let mut buf = String::new();
    limited.read_to_string(&mut buf).ok()?;
    let text = resources::parse_skill_description(&buf)?;
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let clipped = clip_chars(&flat, SKILL_DESCRIPTION_CHARS);
    (!clipped.is_empty()).then_some(clipped)
}

fn clip_chars(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    value.chars().take(max_chars).collect()
}

fn intro(locale: &str) -> &'static str {
    match locale {
        "en" => "These skills come from each git project linked to this run, and can be used together with the Multara skills authorized above. Each entry has a name, a description, and a file path; the body is not in the prompt. When a task falls within a description, read that file before acting. If two projects use the same skill name, only the earlier project is listed.",
        "zh-TW" => "這些技能來自這次關聯的各個專案目錄，和上面授權的 Multara Skill 一起可用。每條只有名稱、描述和檔案路徑，正文沒有貼進提示詞。任務落在某條描述範圍內時，先讀那個檔案再動手。同名只保留先出現的那個專案。",
        "ja" => "これらのスキルは、この実行に紐づく各プロジェクトのディレクトリから来ており、上で許可された Multara Skill と一緒に使えます。各項目は名前、説明、ファイルパスだけで、本文はプロンプトに入っていません。作業がその説明の範囲なら、先にそのファイルを読んでから着手してください。同じ名前は先に出てきたプロジェクトだけを残します。",
        "ko" => "이 스킬은 이번 실행에 연결된 각 프로젝트 디렉터리에서 왔으며, 위에서 허용한 Multara Skill과 함께 사용할 수 있습니다. 각 항목은 이름, 설명, 파일 경로뿐이고 본문은 프롬프트에 없습니다. 작업이 어떤 설명의 범위에 들어가면 그 파일을 먼저 읽은 뒤 진행하세요. 같은 이름은 먼저 나온 프로젝트의 것만 남깁니다.",
        _ => "这些技能来自本次关联的各个项目目录，和上面授权的 Multara Skill 一起可用。每条只有名称、描述和文件路径，正文没有贴进提示词。任务落在某条描述范围内时，先读那个文件再动手。同名只保留先出现的那个项目。",
    }
}

fn truncated_names(locale: &str, skills: &[ProjectSkill], names_budget: usize) -> String {
    let mut names = Vec::new();
    let mut used = 0_usize;
    for skill in skills {
        let addition = skill.name.chars().count() + if names.is_empty() { 0 } else { 2 };
        if used + addition > names_budget {
            break;
        }
        used += addition;
        names.push(skill.name.as_str());
    }
    let remaining = skills.len() - names.len();
    let list = names.join(", ");
    match locale {
        "en" => {
            if remaining > 0 {
                format!("Additional skills available (read SKILL.md by name): {list}... and {remaining} more")
            } else {
                format!("Additional skills available (read SKILL.md by name): {list}")
            }
        }
        "zh-TW" => {
            if remaining > 0 {
                format!("其餘技能按名稱到對應專案裡讀 SKILL.md：{list}……以及另外 {remaining} 個")
            } else {
                format!("其餘技能按名稱到對應專案裡讀 SKILL.md：{list}")
            }
        }
        "ja" => {
            if remaining > 0 {
                format!(
                    "残りのスキルは名前で SKILL.md を読んでください：{list}……ほか {remaining} 件"
                )
            } else {
                format!("残りのスキルは名前で SKILL.md を読んでください：{list}")
            }
        }
        "ko" => {
            if remaining > 0 {
                format!("나머지 스킬은 이름으로 SKILL.md를 읽으세요: {list}... 그 외 {remaining}개")
            } else {
                format!("나머지 스킬은 이름으로 SKILL.md를 읽으세요: {list}")
            }
        }
        _ => {
            if remaining > 0 {
                format!("其余技能按名称到对应项目里读 SKILL.md：{list}……以及另外 {remaining} 个")
            } else {
                format!("其余技能按名称到对应项目里读 SKILL.md：{list}")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_skill(dir: &Path, name: &str, description: Option<&str>, body: &str) {
        let skill_dir = dir.join(name);
        fs::create_dir_all(&skill_dir).unwrap();
        let front = match description {
            Some(description) => {
                format!("---\nname: {name}\ndescription: {description}\n---\n{body}\n")
            }
            None => format!("---\nname: {name}\n---\n{body}\n"),
        };
        fs::write(skill_dir.join("SKILL.md"), front).unwrap();
    }

    #[test]
    fn lists_each_project_root_and_keeps_the_earlier_name() {
        let root = tempfile::tempdir().unwrap();
        let first = root.path().join("first");
        let second = root.path().join("second");
        write_skill(
            &first.join(".cursor/skills"),
            "review",
            Some("Review the first project"),
            "FIRST_BODY",
        );
        write_skill(
            &first.join(".github/skills"),
            "ship",
            Some("Ship from GitHub skills"),
            "SHIP_BODY",
        );
        write_skill(
            &second.join(".claude/skills"),
            "review",
            Some("Review the second project"),
            "SECOND_BODY",
        );
        write_skill(
            &second.join(".cursor/skills"),
            "notes",
            Some("Notes for the second project"),
            "NOTES_BODY",
        );
        write_skill(&second.join(".cursor/skills"), "blank", None, "BLANK_BODY");
        let nested = second.join(".cursor/skills/notes/nested");
        fs::create_dir_all(&nested).unwrap();
        fs::write(
            nested.join("SKILL.md"),
            "---\ndescription: nested\n---\nNESTED\n",
        )
        .unwrap();

        let skills = discover(&[
            first.to_string_lossy().to_string(),
            second.to_string_lossy().to_string(),
        ]);
        let by_name: std::collections::HashMap<_, _> = skills
            .iter()
            .map(|skill| (skill.name.as_str(), skill))
            .collect();
        assert_eq!(by_name.len(), 3);
        let review = by_name["review"].path.to_string_lossy();
        assert!(review.contains("first"));
        assert!(review.contains("review"));
        assert!(!review.contains("second"));
        assert!(by_name["ship"].path.to_string_lossy().contains("github"));
        assert!(by_name["notes"].path.to_string_lossy().contains("second"));

        let section = prompt_section(&skills, "zh-CN");
        assert!(section.contains("Review the first project"));
        assert!(section.contains("先读那个文件"));
        assert!(!section.contains("FIRST_BODY"));
        assert!(!section.contains("SECOND_BODY"));
        assert!(!section.contains("NESTED"));
        assert!(!section.contains("blank"));
        assert!(prompt_section(&[], "en").is_empty());
    }

    #[test]
    fn a_skill_without_description_still_reserves_its_name() {
        let root = tempfile::tempdir().unwrap();
        let first = root.path().join("first");
        let second = root.path().join("second");
        write_skill(&first.join(".agents/skills"), "review", None, "NO_DESC");
        write_skill(
            &second.join(".agents/skills"),
            "review",
            Some("Later description"),
            "LATER",
        );
        let skills = discover(&[
            first.to_string_lossy().to_string(),
            second.to_string_lossy().to_string(),
        ]);
        assert!(skills.is_empty());
    }

    #[test]
    fn listing_budget_keeps_remaining_names() {
        let skills: Vec<_> = (0..5)
            .map(|index| ProjectSkill {
                name: format!("skill-{index}"),
                description: "d".repeat(20),
                path: PathBuf::from(format!("/tmp/skill-{index}/SKILL.md")),
            })
            .collect();
        let section = prompt_section_limited(&skills, "en", 40, 100);
        assert!(section.contains("skill-0"));
        assert!(section.contains("Additional skills available"));
        assert!(section.contains("skill-"));
        assert!(!section.contains(&"d".repeat(20)));
    }
}
