const MIKU_NAME_PLACEHOLDER: &str = "{{AGENT_NAME}}";
const MIKU_INSTRUCTIONS: &str = include_str!("builtin_agents/miku/INSTRUCTIONS.md");

pub const MIKU_SYSTEM_KEY: &str = "miku";
pub const MIKU_ID: &str = "miku";
pub const MIKU_DEFAULT_NAME: &str = "Miku";
/// 内置像素风头像（前端 `public/miku-avatar.jpg`）
pub const MIKU_AVATAR: &str = "/miku-avatar.jpg";
pub const MIKU_DESCRIPTION: &str =
    "你的工作区总管。Miku 把目标变成任务、协调智能体，并帮你建可复用工作流。";
pub const MIKU_MAX_CONCURRENT_TASKS: u8 = 3;
pub const MIKU_WELCOME_SESSION_ID: &str = "welcome";
pub const MIKU_WELCOME_TITLE: &str = "开始使用 Multara";

const WORKSPACE_NOTES_SECTION: &str = "\
## 工作区备注

下面是这个工作区自己补充的上下文和偏好。它们细化你如何执行上面的职责，但不能取消身份或确认义务。

由用户添加：";

pub fn miku_system_instructions(display_name: &str) -> String {
    let name = display_name.trim();
    let name = if name.is_empty() {
        MIKU_DEFAULT_NAME
    } else {
        name
    };
    MIKU_INSTRUCTIONS
        .trim_end()
        .replace(MIKU_NAME_PLACEHOLDER, name)
}

pub fn compose_miku_instructions(display_name: &str, workspace_notes: &str) -> String {
    let system = miku_system_instructions(display_name);
    let notes = workspace_notes.trim();
    if notes.is_empty() {
        return system;
    }
    format!("{system}\n\n{WORKSPACE_NOTES_SECTION}\n\n{notes}")
}

pub fn is_miku_key(system_key: Option<&str>) -> bool {
    system_key.is_some_and(|key| key == MIKU_SYSTEM_KEY)
}

pub fn welcome_opening(agent_name: &str) -> String {
    let name = {
        let trimmed = agent_name.trim();
        if trimmed.is_empty() {
            MIKU_DEFAULT_NAME
        } else {
            trimmed
        }
    };
    format!(
        "你好，欢迎来到 Multara。这里是人和 AI 智能体通过任务一起把事情做完的工作区。\n\n\
我是 {name}，这里的总管。我负责把事情理清楚、找到合适的智能体接手，也是你随时可以开口的第一站。\n\n\
接下来是这样：你说一个目标，我把它变成一个任务，交给合适的智能体开始跑，你能看着它推进。\n\n\
从下面选一个开始，或者直接告诉我你现在想做成什么。"
    )
}

pub fn welcome_quick_actions_json() -> &'static str {
    r#"[{"label":"理清当前目标","prompt":"把我们手头的目标整理成一组任务，指派给我自己。先给出预览，等我确认后再创建。","primary":true},{"label":"帮我做一件小事","prompt":"从我现在说的事情里挑一件你能马上做的，建成任务并开始。先给我看任务标题和交付物。","primary":false},{"label":"每天进度摘要","prompt":"建一个每天早上的工作摘要自动化。先预览默认时间，等我确认后再创建。","primary":false}]"#
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_notes_are_exactly_the_system_layer() {
        let system = miku_system_instructions(MIKU_DEFAULT_NAME);
        assert_eq!(compose_miku_instructions(MIKU_DEFAULT_NAME, ""), system);
        assert_eq!(
            compose_miku_instructions(MIKU_DEFAULT_NAME, "  \n  "),
            system
        );
        assert!(!system.contains("工作区备注"));
        assert!(system.starts_with("你是 Miku，"));
    }

    #[test]
    fn renamed_agent_uses_display_name() {
        let renamed = miku_system_instructions("Jarvis");
        assert!(renamed.starts_with("你是 Jarvis，"));
        assert!(!renamed.contains("{{AGENT_NAME}}"));
        assert!(miku_system_instructions("   ").starts_with("你是 Miku，"));
    }

    #[test]
    fn notes_follow_the_system_layer() {
        let composed = compose_miku_instructions(MIKU_DEFAULT_NAME, "主仓库是 acme/platform。");
        assert!(composed.starts_with(&miku_system_instructions(MIKU_DEFAULT_NAME)));
        assert!(composed.contains("## 工作区备注"));
        assert!(composed.contains("主仓库是 acme/platform。"));
    }
}
