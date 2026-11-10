use std::{fs, io::Write, path::Path};

#[derive(Clone, Copy)]
pub struct BuiltinSkill {
    pub name: &'static str,
    /// 简体中文正文（默认）
    pub zh_cn: &'static str,
    pub en: &'static str,
    pub zh_tw: &'static str,
    pub ja: &'static str,
    pub ko: &'static str,
}

pub const ALL: &[BuiltinSkill] = &[
    BuiltinSkill {
        name: "multara-onboarding",
        zh_cn: include_str!("../skills/multara-onboarding/SKILL.md"),
        en: include_str!("../skills/multara-onboarding/SKILL.en.md"),
        zh_tw: include_str!("../skills/multara-onboarding/SKILL.zh-TW.md"),
        ja: include_str!("../skills/multara-onboarding/SKILL.ja.md"),
        ko: include_str!("../skills/multara-onboarding/SKILL.ko.md"),
    },
    BuiltinSkill {
        name: "multara-working-on-tasks",
        zh_cn: include_str!("../skills/multara-working-on-tasks/SKILL.md"),
        en: include_str!("../skills/multara-working-on-tasks/SKILL.en.md"),
        zh_tw: include_str!("../skills/multara-working-on-tasks/SKILL.zh-TW.md"),
        ja: include_str!("../skills/multara-working-on-tasks/SKILL.ja.md"),
        ko: include_str!("../skills/multara-working-on-tasks/SKILL.ko.md"),
    },
    BuiltinSkill {
        name: "multara-creating-agents",
        zh_cn: include_str!("../skills/multara-creating-agents/SKILL.md"),
        en: include_str!("../skills/multara-creating-agents/SKILL.en.md"),
        zh_tw: include_str!("../skills/multara-creating-agents/SKILL.zh-TW.md"),
        ja: include_str!("../skills/multara-creating-agents/SKILL.ja.md"),
        ko: include_str!("../skills/multara-creating-agents/SKILL.ko.md"),
    },
    BuiltinSkill {
        name: "multara-projects-and-workspaces",
        zh_cn: include_str!("../skills/multara-projects-and-workspaces/SKILL.md"),
        en: include_str!("../skills/multara-projects-and-workspaces/SKILL.en.md"),
        zh_tw: include_str!("../skills/multara-projects-and-workspaces/SKILL.zh-TW.md"),
        ja: include_str!("../skills/multara-projects-and-workspaces/SKILL.ja.md"),
        ko: include_str!("../skills/multara-projects-and-workspaces/SKILL.ko.md"),
    },
    BuiltinSkill {
        name: "multara-squads",
        zh_cn: include_str!("../skills/multara-squads/SKILL.md"),
        en: include_str!("../skills/multara-squads/SKILL.en.md"),
        zh_tw: include_str!("../skills/multara-squads/SKILL.zh-TW.md"),
        ja: include_str!("../skills/multara-squads/SKILL.ja.md"),
        ko: include_str!("../skills/multara-squads/SKILL.ko.md"),
    },
    BuiltinSkill {
        name: "multara-automations",
        zh_cn: include_str!("../skills/multara-automations/SKILL.md"),
        en: include_str!("../skills/multara-automations/SKILL.en.md"),
        zh_tw: include_str!("../skills/multara-automations/SKILL.zh-TW.md"),
        ja: include_str!("../skills/multara-automations/SKILL.ja.md"),
        ko: include_str!("../skills/multara-automations/SKILL.ko.md"),
    },
    BuiltinSkill {
        name: "multara-skill-importing",
        zh_cn: include_str!("../skills/multara-skill-importing/SKILL.md"),
        en: include_str!("../skills/multara-skill-importing/SKILL.en.md"),
        zh_tw: include_str!("../skills/multara-skill-importing/SKILL.zh-TW.md"),
        ja: include_str!("../skills/multara-skill-importing/SKILL.ja.md"),
        ko: include_str!("../skills/multara-skill-importing/SKILL.ko.md"),
    },
    BuiltinSkill {
        name: GENUI_SKILL,
        zh_cn: include_str!("../skills/multara-ui/SKILL.md"),
        en: include_str!("../skills/multara-ui/SKILL.en.md"),
        zh_tw: include_str!("../skills/multara-ui/SKILL.zh-TW.md"),
        ja: include_str!("../skills/multara-ui/SKILL.ja.md"),
        ko: include_str!("../skills/multara-ui/SKILL.ko.md"),
    },
];

/// 生成式界面（实验功能）的 skill：只有设置里打开时才列进简报。
pub const GENUI_SKILL: &str = "multara-ui";

/// skill 目录里的附属文件（`SKILL.md` 按需引用）。目前只有 multara-ui 的 Canvas 说明。
struct ExtraFile {
    skill: &'static str,
    file: &'static str,
    zh_cn: &'static str,
    en: &'static str,
    zh_tw: &'static str,
    ja: &'static str,
    ko: &'static str,
}

const EXTRA_FILES: &[ExtraFile] = &[ExtraFile {
    skill: GENUI_SKILL,
    file: "canvas.md",
    zh_cn: include_str!("../skills/multara-ui/canvas.md"),
    en: include_str!("../skills/multara-ui/canvas.en.md"),
    zh_tw: include_str!("../skills/multara-ui/canvas.zh-TW.md"),
    ja: include_str!("../skills/multara-ui/canvas.ja.md"),
    ko: include_str!("../skills/multara-ui/canvas.ko.md"),
}];

impl ExtraFile {
    fn markdown_for(&self, locale: &str) -> &'static str {
        match locale {
            "en" => self.en,
            "zh-TW" => self.zh_tw,
            "ja" => self.ja,
            "ko" => self.ko,
            _ => self.zh_cn,
        }
    }
}

impl BuiltinSkill {
    pub fn markdown_for(&self, locale: &str) -> &'static str {
        match locale {
            "en" => self.en,
            "zh-TW" => self.zh_tw,
            "ja" => self.ja,
            "ko" => self.ko,
            _ => self.zh_cn,
        }
    }

    pub fn description_for(&self, locale: &str) -> String {
        frontmatter_description(self.markdown_for(locale)).unwrap_or_else(|| self.name.to_string())
    }
}

/// 当前开关下对智能体可见的内置 skill。正文仍全部落盘（`materialize`），只是不列出来。
pub fn enabled(genui_enabled: bool) -> impl Iterator<Item = &'static BuiltinSkill> {
    ALL.iter()
        .filter(move |skill| genui_enabled || skill.name != GENUI_SKILL)
}

pub fn names(genui_enabled: bool) -> Vec<&'static str> {
    enabled(genui_enabled).map(|skill| skill.name).collect()
}

/// 每次启动运行都会调；内容没变就不写。写入走同目录临时文件 + rename，
/// 并发运行或 CLI 正在读时不会看到写了一半的文件。
pub fn materialize(root: &Path, locale: &str) -> Result<(), String> {
    let files = ALL
        .iter()
        .map(|skill| (skill.name, "SKILL.md", skill.markdown_for(locale)))
        .chain(
            EXTRA_FILES
                .iter()
                .map(|extra| (extra.skill, extra.file, extra.markdown_for(locale))),
        );
    for (name, file_name, markdown) in files {
        let dir = root.join(name);
        let path = dir.join(file_name);
        if fs::read(&path).is_ok_and(|current| current == markdown.as_bytes()) {
            continue;
        }
        fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
        let mut file = tempfile::NamedTempFile::new_in(&dir).map_err(|error| error.to_string())?;
        file.write_all(markdown.as_bytes())
            .map_err(|error| error.to_string())?;
        file.persist(&path)
            .map_err(|error| error.error.to_string())?;
    }
    Ok(())
}

fn frontmatter_description(markdown: &str) -> Option<String> {
    let rest = markdown.strip_prefix("---")?;
    let end = rest.find("\n---")?;
    for line in rest[..end].lines() {
        let line = line.trim();
        let Some(value) = line.strip_prefix("description:") else {
            continue;
        };
        return Some(value.trim().trim_matches('"').to_string());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn materialize_rewrites_only_changed_files() {
        let root = tempfile::tempdir().unwrap();
        materialize(root.path(), "en").unwrap();
        let path = root.path().join(ALL[0].name).join("SKILL.md");
        assert_eq!(fs::read_to_string(&path).unwrap(), ALL[0].en);
        let modified = fs::metadata(&path).unwrap().modified().unwrap();
        materialize(root.path(), "en").unwrap();
        assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), modified);
        materialize(root.path(), "zh-CN").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), ALL[0].zh_cn);
        let leftovers = fs::read_dir(path.parent().unwrap()).unwrap().count();
        assert_eq!(leftovers, 1);
    }

    #[test]
    fn embeds_every_product_skill() {
        let names = names(true);
        assert!(names.contains(&"multara-onboarding"));
        assert!(names.contains(&GENUI_SKILL));
        assert!(!super::names(false).contains(&GENUI_SKILL));
        assert!(names.contains(&"multara-working-on-tasks"));
        assert!(names.contains(&"multara-creating-agents"));
        for skill in ALL {
            for locale in ["zh-CN", "en", "zh-TW", "ja", "ko"] {
                let markdown = skill.markdown_for(locale);
                assert!(
                    markdown.contains(&format!("name: {}", skill.name)),
                    "{} missing name in {locale}",
                    skill.name
                );
                assert!(
                    frontmatter_description(markdown).is_some(),
                    "{} missing description in {locale}",
                    skill.name
                );
            }
        }
    }
}
