//! IM 渠道接入（飞书 / Lark 优先）。
//!
//! 渠道只做两件事：把平台消息变成一次普通的会话输入，以及把运行结果发回平台。
//! 执行路径与本地聊天完全一致，因此 Multara 界面里看到的始终是同一份数据。

use serde::{Deserialize, Serialize};

pub mod commands;
pub mod feishu;
pub mod outbound;
mod pairing;
pub mod router;
pub mod secret;
pub mod store;
pub mod supervisor;

pub const TYPE_FEISHU: &str = "feishu";
pub const TYPE_LARK: &str = "lark";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChatType {
    P2p,
    Group,
}

impl ChatType {
    pub fn as_str(self) -> &'static str {
        match self {
            ChatType::P2p => "p2p",
            ChatType::Group => "group",
        }
    }

    pub fn parse(value: &str) -> ChatType {
        if value == "p2p" {
            ChatType::P2p
        } else {
            ChatType::Group
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ChannelConfig {
    pub app_id: String,
    /// `feishu`（默认）或 `lark`。
    pub region: String,
    pub bot_open_id: Option<String>,
    pub bot_name: Option<String>,
    pub group_enabled: bool,
    /// 新用户私聊机器人时发送这串数字完成绑定。
    pub pairing_code: String,
    /// 覆盖 API 域名，仅用于代理 / 测试部署。
    pub api_base: Option<String>,
}

impl ChannelConfig {
    pub fn region(&self) -> &str {
        if self.region == TYPE_LARK {
            TYPE_LARK
        } else {
            TYPE_FEISHU
        }
    }

    pub fn api_base(&self) -> String {
        if let Some(base) = self
            .api_base
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
        {
            return base.trim_end_matches('/').to_string();
        }
        if self.region() == TYPE_LARK {
            "https://open.larksuite.com".to_string()
        } else {
            "https://open.feishu.cn".to_string()
        }
    }
}

#[derive(Clone, Debug)]
pub struct Installation {
    pub id: String,
    pub channel_type: String,
    pub name: String,
    pub target_type: String,
    pub target_id: String,
    pub project_id: Option<String>,
    pub config: ChannelConfig,
    pub enabled: bool,
    pub status: String,
    pub last_error: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaRef {
    pub message_id: String,
    pub file_key: String,
    pub kind: String,
    pub name: String,
}

#[derive(Clone, Debug)]
pub struct InboundMessage {
    pub installation_id: String,
    pub event_id: String,
    pub message_id: String,
    pub chat_id: String,
    pub chat_type: ChatType,
    pub thread_id: Option<String>,
    pub root_id: Option<String>,
    pub parent_id: Option<String>,
    pub sender_id: String,
    pub sender_name: Option<String>,
    pub text: String,
    pub addressed_to_bot: bool,
    pub media: Vec<MediaRef>,
    pub created_at: String,
}

#[derive(Clone, Debug)]
pub struct CardAction {
    pub installation_id: String,
    pub event_id: String,
    pub chat_id: String,
    pub message_id: String,
    pub operator_id: String,
    pub value: serde_json::Value,
}

pub enum InboundEvent {
    Message(InboundMessage),
    CardAction(CardAction),
}

#[derive(Clone, Debug, Default)]
pub struct OutboundMessage {
    pub chat_id: String,
    pub chat_type: ChatType,
    pub thread_id: Option<String>,
    pub reply_to_message_id: Option<String>,
    pub sender_id: Option<String>,
    pub text: String,
    pub run_id: Option<String>,
}

impl Default for ChatType {
    fn default() -> Self {
        ChatType::P2p
    }
}
