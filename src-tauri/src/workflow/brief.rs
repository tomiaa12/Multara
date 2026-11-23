//! 智能体节点收到的任务说明。引擎拼好上下文，不经过用户模板。

use serde_json::Value;

use super::model::{NodeDef, WorkspaceAccess};

pub struct BriefInput<'a> {
    pub workflow_name: &'a str,
    pub node: &'a NodeDef,
    pub iteration: i64,
    pub attempt: i64,
    pub task: &'a str,
    pub run_inputs: &'a Value,
    pub steps: &'a [StepText],
    pub allowed_next: &'a [(&'a str, &'a str)],
    pub route: &'a str,
    pub loop_note: Option<String>,
    pub allow_tasks: bool,
    pub workspace_lines: &'a [String],
    /// 做完后停住，等人确认才进入下一步。
    pub human_confirm: bool,
}

pub struct StepText {
    pub title: String,
    pub content: String,
}

pub fn agent_brief(input: &BriefInput<'_>) -> String {
    let title = if input.node.title.trim().is_empty() { input.node.id.as_str() } else { input.node.title.as_str() };
    let mut out = String::new();
    out.push_str(&tr!(
        "[工作流节点] {workflow} › {node}（第 {iteration} 轮，第 {attempt} 次尝试）",
        workflow = input.workflow_name,
        node = title,
        iteration = input.iteration + 1,
        attempt = input.attempt
    ));
    out.push_str("\n\n## ");
    out.push_str(&tr!("任务"));
    out.push('\n');
    out.push_str(input.task.trim());
    out.push('\n');

    if let Some(note) = &input.loop_note {
        out.push_str("\n## ");
        out.push_str(&tr!("打回"));
        out.push('\n');
        out.push_str(note);
        out.push('\n');
    }

    if let Some(map) = input.run_inputs.as_object() {
        let lines: Vec<String> = map
            .iter()
            .filter(|(_, value)| !value.is_null())
            .map(|(key, value)| format!("- {key}：{}", value_text(value)))
            .collect();
        if !lines.is_empty() {
            out.push_str("\n## ");
            out.push_str(&tr!("这次运行的输入"));
            out.push('\n');
            out.push_str(&lines.join("\n"));
            out.push('\n');
        }
    }

    if !input.steps.is_empty() {
        out.push_str("\n## ");
        out.push_str(&tr!("上一步的结果"));
        out.push('\n');
        for step in input.steps {
            out.push_str(&format!("### {}\n{}\n", step.title, step.content.trim()));
        }
    }

    if !input.workspace_lines.is_empty() {
        out.push_str("\n## ");
        out.push_str(&tr!("工作目录"));
        out.push('\n');
        for line in input.workspace_lines {
            out.push_str(line);
            out.push('\n');
        }
    }

    if input.allowed_next.len() > 1 && input.route == "all" {
        out.push_str("\n## ");
        out.push_str(&tr!("同时交给"));
        out.push('\n');
        out.push_str(&tr!("结果会同时交给下面每一个下一步，不用选择。"));
        out.push('\n');
        for (id, label) in input.allowed_next {
            out.push_str(&format!("- {id}：{label}\n"));
        }
    }
    if input.allowed_next.len() > 1 && input.route == "auto" && !input.human_confirm {
        out.push_str("\n## ");
        out.push_str(&tr!("完成后选择下一步"));
        out.push('\n');
        out.push_str(&tr!("只能选下面已有的一个节点，用命令提交。选错或不提交，这一步失败。"));
        out.push('\n');
        for (id, label) in input.allowed_next {
            out.push_str(&format!("- {id}：{label}\n"));
            out.push_str(&format!("  `multara node next --id {id} --reason \"…\"`\n"));
        }
    }

    out.push_str("\n## ");
    out.push_str(&tr!("规则"));
    out.push('\n');
    if !input.allow_tasks {
        out.push_str(&format!("- {}\n", tr!("不要 @ 其他智能体，不要创建任务。")));
    }
    if input.node.workspaces.iter().any(|usage| usage.access == WorkspaceAccess::Write) {
        out.push_str(&format!("- {}\n", tr!("不要 git commit / push，提交由工作流负责。")));
    }
    out.push_str(&format!("- {}\n", tr!("做不到时：`multara node fail --reason \"…\"`")));
    out.push_str(&format!("- {}\n", tr!("最后一条正文要留下回答本身，它会交给下一步。不要只用一句收尾把回答盖掉。")));
    if input.human_confirm {
        out.push_str(&format!("- {}\n", tr!("这一步需要人工确认。做完后停住，不要自己进入下一步。")));
    }
    out
}

fn value_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

pub fn workspace_line(slot: &str, access: WorkspaceAccess, path: &str, branch: &str) -> String {
    let access = if access == WorkspaceAccess::Write { tr!("可写") } else { tr!("只读") };
    if branch.is_empty() {
        format!("- {slot}（{access}）：{path}")
    } else {
        format!("- {slot}（{access}）：{path}，{} {branch}", tr!("分支"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::model::NodeDef;
    use serde_json::json;

    #[test]
    fn brief_includes_task_previous_and_choice() {
        let node: NodeDef = serde_json::from_value(json!({
            "id": "review", "kind": "agent", "title": "测试",
            "workspaces": [{ "slot": "app", "access": "write" }]
        }))
        .unwrap();
        let brief = agent_brief(&BriefInput {
            workflow_name: "代码审查",
            node: &node,
            iteration: 1,
            attempt: 0,
            task: "检查实现",
            run_inputs: &json!({ "topic": "升级风险" }),
            steps: &[StepText { title: "开发".into(), content: "已改完".into() }],
            allowed_next: &[("end", "结束"), ("dev", "开发")],
            route: "auto",
            loop_note: Some("第 2 轮。失败原因：测试没过".into()),
            allow_tasks: false,
            workspace_lines: &["- app（可写）：/tmp/app".into()],
            human_confirm: true,
        });
        assert!(brief.contains("不要自己进入下一步"));
        assert!(!brief.contains("multara node next"));
        assert!(brief.contains("检查实现"));
        assert!(brief.contains("升级风险"));
        assert!(brief.contains("已改完"));
        assert!(brief.contains("第 2 轮"));
        assert!(brief.contains("不要创建任务"));
    }
}
