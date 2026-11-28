use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

fn on() -> bool {
    true
}

pub fn default_on_flag() -> bool {
    true
}

/// 集成用户能用的 Multara 能力、需求范围，以及渠道自己的能力。缺省字段视为允许。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ChannelGrant {
    #[serde(default = "on")]
    pub create_task: bool,
    #[serde(default = "on")]
    pub add_agent: bool,
    #[serde(default = "on")]
    pub add_squad: bool,
    #[serde(default = "on")]
    pub create_automation: bool,
    #[serde(default = "on")]
    pub use_workflow: bool,
    #[serde(default = "on")]
    pub read_files: bool,
    #[serde(default = "on")]
    pub write_files: bool,
    #[serde(default = "on")]
    pub projects_unrestricted: bool,
    #[serde(default)]
    pub project_ids: Vec<String>,
    /// 显式 false 才禁止；缺 key 视为允许。
    #[serde(default)]
    pub channel: BTreeMap<String, bool>,
}

impl Default for ChannelGrant {
    fn default() -> Self {
        Self {
            create_task: true,
            add_agent: true,
            add_squad: true,
            create_automation: true,
            use_workflow: true,
            read_files: true,
            write_files: true,
            projects_unrestricted: true,
            project_ids: Vec::new(),
            channel: BTreeMap::new(),
        }
    }
}

impl ChannelGrant {
    pub fn channel_allowed(&self, key: &str) -> bool {
        self.channel.get(key).copied().unwrap_or(true)
    }

    pub fn allows_project(&self, project_id: &str) -> bool {
        self.projects_unrestricted || self.project_ids.iter().any(|id| id == project_id)
    }

    pub fn is_open(&self) -> bool {
        self.create_task
            && self.add_agent
            && self.add_squad
            && self.create_automation
            && self.use_workflow
            && self.read_files
            && self.write_files
            && self.projects_unrestricted
            && self.channel.values().all(|allowed| *allowed)
    }
}

pub fn runtime_note(grant: &ChannelGrant) -> String {
    if grant.is_open() {
        return String::new();
    }
    let mut denied = Vec::new();
    if !grant.create_task {
        denied.push("创建任务".to_string());
    }
    if !grant.add_agent {
        denied.push("添加智能体".to_string());
    }
    if !grant.add_squad {
        denied.push("添加小队".to_string());
    }
    if !grant.create_automation {
        denied.push("创建自动化".to_string());
    }
    if !grant.use_workflow {
        denied.push("使用工作流".to_string());
    }
    if !grant.read_files {
        denied.push("读取工作区文件".to_string());
    }
    if !grant.write_files {
        denied.push("修改工作区文件".to_string());
    }
    if !grant.projects_unrestricted {
        if grant.project_ids.is_empty() {
            denied.push("访问任何需求".to_string());
        } else {
            denied.push(format!(
                "访问列出的需求以外的需求（仅 {}）",
                grant.project_ids.join("、")
            ));
        }
    }
    for key in ["comment", "readDocs", "editDocs", "readBase", "history", "react", "upload"] {
        if !grant.channel_allowed(key) {
            denied.push(key.to_string());
        }
    }
    crate::tr!(
        "这个集成用户不允许使用：{list}。不要调用这些能力。",
        list = denied.join("、")
    )
}

#[cfg(test)]
mod tests {
    use crate::channel::ChannelConfig;

    #[test]
    fn old_config_still_requires_pairing_and_allows_everything() {
        let config: ChannelConfig = serde_json::from_str(r#"{"appId":"cli_x","pairingCode":"123456"}"#).unwrap();
        assert!(config.pairing_required);
        assert!(config.default_grant.create_task);
        assert!(config.default_grant.projects_unrestricted);
        assert!(config.default_grant.is_open());
    }
}
