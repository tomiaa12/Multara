use std::{fs, path::Path};

#[derive(Clone, Copy)]
pub struct BuiltinSkill {
    pub name: &'static str,
    pub description: &'static str,
    pub markdown: &'static str,
}

pub const ALL: &[BuiltinSkill] = &[
    BuiltinSkill {
        name: "multara-onboarding",
        description: "欢迎会话的引导。产品已发出开场白时使用，直到这次对话把用户交出去。",
        markdown: include_str!("../skills/multara-onboarding/SKILL.md"),
    },
    BuiltinSkill {
        name: "multara-working-on-tasks",
        description:
            "创建、更新任务，并列出任务与运行。用户提到任务、Issue、把工作交给智能体时使用。",
        markdown: include_str!("../skills/multara-working-on-tasks/SKILL.md"),
    },
    BuiltinSkill {
        name: "multara-creating-agents",
        description:
            "创建、更新智能体。用户提到 specialist、新建智能体、改指令或运行时配置时使用。",
        markdown: include_str!("../skills/multara-creating-agents/SKILL.md"),
    },
    BuiltinSkill {
        name: "multara-projects-and-workspaces",
        description:
            "创建、更新需求（项目），绑定工作区。多个任务共享一个结果、或需要改代码时使用。",
        markdown: include_str!("../skills/multara-projects-and-workspaces/SKILL.md"),
    },
    BuiltinSkill {
        name: "multara-squads",
        description: "创建、更新小队，把工作通过队长下达给一组智能体。",
        markdown: include_str!("../skills/multara-squads/SKILL.md"),
    },
    BuiltinSkill {
        name: "multara-automations",
        description:
            "创建定时、间隔或事件触发的自动化。应当按日程或外部事件启动、而不是等人来问时使用。",
        markdown: include_str!("../skills/multara-automations/SKILL.md"),
    },
    BuiltinSkill {
        name: "multara-skill-importing",
        description: "列出工作区 skill 与内置 skill。第一期不支持 URL 导入。",
        markdown: include_str!("../skills/multara-skill-importing/SKILL.md"),
    },
];

pub fn names() -> Vec<&'static str> {
    ALL.iter().map(|skill| skill.name).collect()
}

pub fn materialize(root: &Path) -> Result<(), String> {
    for skill in ALL {
        let dir = root.join(skill.name);
        fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
        fs::write(dir.join("SKILL.md"), skill.markdown).map_err(|error| error.to_string())?;
    }
    Ok(())
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
            assert!(skill.markdown.contains(&format!("name: {}", skill.name)));
        }
    }
}
