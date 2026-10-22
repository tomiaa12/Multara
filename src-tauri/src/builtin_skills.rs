use std::{fs, path::Path};

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
];

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
        frontmatter_description(self.markdown_for(locale))
            .unwrap_or_else(|| self.name.to_string())
    }
}

pub fn names() -> Vec<&'static str> {
    ALL.iter().map(|skill| skill.name).collect()
}

pub fn materialize(root: &Path, locale: &str) -> Result<(), String> {
    for skill in ALL {
        let dir = root.join(skill.name);
        fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
        fs::write(dir.join("SKILL.md"), skill.markdown_for(locale))
            .map_err(|error| error.to_string())?;
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
    fn embeds_every_product_skill() {
        let names = names();
        assert!(names.contains(&"multara-onboarding"));
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
