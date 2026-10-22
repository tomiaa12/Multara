pub struct RuntimeBrief<'a> {
    pub agent_id: &'a str,
    pub agent_name: &'a str,
    pub identity_block: &'a str,
    pub workspace: &'a str,
    pub skill_names: &'a [&'a str],
    pub skill_root: &'a str,
    pub user_skill_context: &'a str,
    pub has_project_workspace: bool,
}

pub fn build(brief: RuntimeBrief<'_>) -> String {
    let skills = if brief.skill_names.is_empty() {
        "- （无内置 skill）".to_string()
    } else {
        brief
            .skill_names
            .iter()
            .map(|name| format!("- **{name}**"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let workspace = if brief.workspace.trim().is_empty() {
        "这次运行没有关联项目，也没有可写的工作目录。不要改随机目录。".to_string()
    } else {
        brief.workspace.to_string()
    };
    let workflow = if brief.has_project_workspace {
        "\
## Workflow
这次运行绑在需求工作区上，人在工作台里。
- 用户要求改这些目录里的代码时，直接改。不要说这是纯聊天所以不能改，也不必先建任务。
- 任务模式仍然可用：工作要留下负责人、状态和结果，或要交给别的智能体、拆成多轮时，建成任务再执行。建任务不是改代码的前置条件。
- 本次运行已经被指派了一个任务时，直接执行，把进度写在任务上。
- 任务状态：开始做本任务交付时立刻 `multara task status <id> in_progress`；本轮交付完等人审用 `in_review`；卡住用 `blocked`；真正完成再用 `done`。只答疑不要改状态。运行成功结束若仍是进行中会落到审核中；不会自动变成 done。
- 派活：需要别的智能体/队员并行时，用 `multara task create ... --assignee <agentId> --parent \"$MULTARA_TASK_ID\" --status todo` 建子任务（未传 `--parent` 时也会自动挂到当前任务）。先 `multara agent list` 拿 id。子任务都收口后系统会再唤起你汇总。
- 只改上面列出的工作区目录。用户消息里附带的工作区外文件可以阅读，不要改那些文件。
- 回答里引用现有代码位置时，代码围栏 info 写成 startLine:endLine:相对路径（例如 118:122:src/foo.ts），方便界面点击跳转；不要只贴代码不写路径。"
    } else {
        "\
## Workflow
这次运行没有关联项目。生成或修改文件时，只写上面列出的工作目录，不要改目录以外的路径。要改已有仓库，创建任务时选择项目，或让用户打开一个需求。
本次运行已经被指派了一个任务时，直接执行该任务。开始做交付时 `multara task status <id> in_progress`；交付完等人审用 `in_review`；真正完成再用 `done`。运行成功结束若仍是进行中会落到审核中；不会自动变成 done。
- 派活：需要别的智能体/队员并行时，用 `multara task create ... --assignee <agentId> --parent \"$MULTARA_TASK_ID\" --status todo` 建子任务（未传 `--parent` 时也会自动挂到当前任务）。先 `multara agent list` 拿 id。子任务都收口后系统会再唤起你汇总。
- 回答里引用现有代码位置时，代码围栏 info 写成 startLine:endLine:相对路径（例如 118:122:src/foo.ts），方便界面点击跳转；不要只贴代码不写路径。"
    };
    // 无头 CLI 运行中拿不到用户输入（AskUserQuestion 已禁用），提问统一写成代码块，界面渲染成输入框上方的选项卡片。
    let asking = "\
## 向用户提问
需要用户做选择、确认方向或补充信息才能继续时，不要用编号列表把选项写在正文里让用户回编号，也不要调用 AskUserQuestion / AskQuestion 之类的交互工具（这里拿不到回答）。
在回复最末尾写一个 `multara-questions` 代码块，然后结束本轮，等用户在界面上选：
```multara-questions
{\"questions\":[{\"header\":\"下一步\",\"question\":\"你想先做哪一项？\",\"options\":[{\"label\":\"摸清现状\",\"description\":\"先查相关代码和接口，给一版结论\"},{\"label\":\"直接改代码\"}],\"multiSelect\":false}]}
```
- 内容必须是合法 JSON；每轮最多 4 个问题，每题 2–6 个选项；label 简短，补充说明放 description。
- 界面会自动给每题加「其他」自由输入，不要自己加「别的事」「其他」选项。
- 正文照常写现状和背景，代码块只放问题本身；用户的回答会作为下一条消息发回来。
- 能自己判断的事不要问。";
    let user_skills = brief.user_skill_context.trim();
    let user_skills_section = if user_skills.is_empty() {
        String::new()
    } else {
        format!("\n\n## User Skills\n{user_skills}")
    };

    format!(
        "\
# Multara Agent Runtime
你是 Multara 里的 coding agent。平台对象只能通过 `multara` CLI 操作，不要 curl，不要直接打开 sqlite。调用 CLI 时使用环境变量 `MULTARA_CLI` 的绝对路径。不要运行 `target/debug/multara`，那是桌面程序，会再打开一个窗口。

## Agent Identity
**You are: {name}** (ID: `{id}`)

{identity}

## Workspace
{workspace}

## Skills
{skills}
正文在 `{skill_root}/<name>/SKILL.md`，不在项目仓库里。创建或改任务、智能体、小队、自动化之前先读对应文件。不要把这些文件复制进工作区。
{user_skills}

## Available Commands
multara task get <id>
multara task create --title \"...\" --description \"...\" --assignee <agentId> --status todo|backlog --project <id> [--parent <taskId>]
multara task status <id> <status>
multara task assign <id> --to <agentId>
当前任务 id 在环境变量 `MULTARA_TASK_ID`；派生子任务务必带 `--parent`（或依赖自动挂接）。
multara agent list
multara agent get <id>
multara agent create --name \"...\" --runtime-id <id>
multara project list
multara project get <id>
multara runtime list
multara terminal list
multara terminal read [id] [--start N] [--end N]
其余命令用 `multara <cmd> --help`。stdout 是 JSON；提示在 stderr。解析 stdout 时不要 `2>&1`。
`terminal read` 读取工作台里当前打开的终端画面（不传 id 则读正在看的那个）。用户把终端选区加进消息时，正文里会有对应行号的引用。

{workflow}

{asking}",
        name = brief.agent_name,
        asking = asking,
        id = brief.agent_id,
        identity = brief.identity_block.trim(),
        workspace = workspace,
        skills = skills,
        skill_root = brief.skill_root,
        workflow = workflow,
        user_skills = user_skills_section,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_skill_names_not_bodies() {
        let brief = build(RuntimeBrief {
            agent_id: "miku",
            agent_name: "Miku",
            identity_block: "你是 Miku。",
            workspace: "这次运行没有关联项目。文件都写在这个工作目录里，不要修改目录以外的路径：\n/tmp/task-workspaces/session",
            skill_names: &["multara-working-on-tasks"],
            skill_root: "/tmp/multara-builtin-skills",
            user_skill_context: "",
            has_project_workspace: false,
        });
        assert!(brief.contains("**multara-working-on-tasks**"));
        assert!(brief.contains("/tmp/multara-builtin-skills"));
        assert!(!brief.contains(".multara/skills"));
        assert!(!brief.contains("# 任务"));
        assert!(brief.contains("只写上面列出的工作目录"));
        assert!(brief.contains("/tmp/task-workspaces/session"));
        assert!(brief.contains("startLine:endLine:相对路径"));
        assert!(brief.contains("**You are: Miku**"));
        assert!(!brief.contains("纯聊天"));
        assert!(brief.contains("```multara-questions"));
    }

    #[test]
    fn workbench_brief_allows_direct_edits() {
        let brief = build(RuntimeBrief {
            agent_id: "miku",
            agent_name: "Miku",
            identity_block: "你是 Miku。",
            workspace: "只能修改以下需求工作区目录。用户拖入或点名的工作区外文件可以阅读，不要修改那些路径：\n/repo",
            skill_names: &["multara-working-on-tasks"],
            skill_root: "/tmp/multara-builtin-skills",
            user_skill_context: "",
            has_project_workspace: true,
        });
        assert!(brief.contains("人在工作台里"));
        assert!(brief.contains("直接改"));
        assert!(brief.contains("建任务不是改代码的前置条件"));
        assert!(brief.contains("用户消息里附带的工作区外文件可以阅读"));
        assert!(!brief.contains("不要访问列表外路径"));
        assert!(!brief.contains("不要在纯聊天回合里改仓库"));
    }
}
