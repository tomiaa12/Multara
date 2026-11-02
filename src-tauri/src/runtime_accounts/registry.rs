//! 每个内置运行时的登录存在哪。没登记的运行时只显示登录状态，不支持切换。
//!
//! 路径前缀：`~/` 是用户主目录；`$CODEX/`、`$KIMI/` 分别是 `CODEX_HOME`、`KIMI_CODE_HOME`
//! （没设时为 `~/.codex`、`~/.kimi-code`）。

use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    /// 能显示账号身份，切换写回 CLI 的登录
    A,
    /// 同 A，但凭据位置是按 CLI 约定推断的，没在每个版本上实测
    B,
    /// 「账号」是供应商配置：切换只换回账号相关的文件和字段
    C,
    /// 凭据位置未知，只给登录入口
    D,
}

pub enum KeyAccount {
    Fixed(&'static str),
    /// 当前系统用户名（Claude Code 用它当 account）
    CurrentUser,
}

pub enum Item {
    /// 整个文件
    File(&'static str),
    /// JSON 文件里的一个字段；只换这个字段，文件里其他设置保持不动
    JsonField {
        file: &'static str,
        pointer: &'static str,
    },
    /// 钥匙串里的一条（仅 macOS）
    Keychain {
        service: &'static str,
        account: KeyAccount,
    },
}

impl Item {
    /// 账号库里按它对应保存的值，改顺序不会错位。
    pub fn tag(&self) -> String {
        match self {
            Item::File(path) => format!("file:{path}"),
            Item::JsonField { file, pointer } => format!("json:{file}#{pointer}"),
            Item::Keychain { service, account } => match account {
                KeyAccount::Fixed(name) => format!("keychain:{service}/{name}"),
                KeyAccount::CurrentUser => format!("keychain:{service}/$USER"),
            },
        }
    }
}

pub struct Spec {
    pub runtime: &'static str,
    pub tier: Tier,
    pub items: &'static [Item],
    /// 所有凭据都在钥匙串里，非 macOS 不支持
    pub mac_only: bool,
    /// CLI 刷新 token 时用的 proper-lockfile 名（`$KIMI/oauth/<name>.lock`）
    pub kimi_lock: Option<&'static str>,
    /// 这些环境变量存在时，CLI 用它们而不是登录
    pub env_overrides: &'static [&'static str],
}

const SPECS: &[Spec] = &[
    Spec {
        runtime: "codex",
        tier: Tier::A,
        items: &[Item::File("$CODEX/auth.json")],
        mac_only: false,
        kimi_lock: None,
        env_overrides: &[],
    },
    Spec {
        runtime: "kimi",
        tier: Tier::A,
        items: &[Item::File("$KIMI/credentials/kimi-code.json")],
        mac_only: false,
        kimi_lock: Some("kimi-code"),
        env_overrides: &[],
    },
    Spec {
        runtime: "antigravity",
        tier: Tier::A,
        items: &[Item::Keychain {
            service: "gemini",
            account: KeyAccount::Fixed("antigravity"),
        }],
        mac_only: true,
        kimi_lock: None,
        env_overrides: &[],
    },
    Spec {
        runtime: "cursor",
        tier: Tier::A,
        items: &[
            Item::Keychain {
                service: "cursor-access-token",
                account: KeyAccount::Fixed("cursor-user"),
            },
            Item::Keychain {
                service: "cursor-refresh-token",
                account: KeyAccount::Fixed("cursor-user"),
            },
            Item::JsonField {
                file: "~/.cursor/cli-config.json",
                pointer: "/authInfo",
            },
        ],
        mac_only: true,
        kimi_lock: None,
        env_overrides: &["CURSOR_API_KEY"],
    },
    Spec {
        runtime: "grok",
        tier: Tier::A,
        items: &[Item::File("~/.grok/auth.json")],
        mac_only: false,
        kimi_lock: None,
        env_overrides: &[],
    },
    Spec {
        runtime: "claude",
        tier: Tier::B,
        items: &[
            Item::Keychain {
                service: "Claude Code-credentials",
                account: KeyAccount::CurrentUser,
            },
            Item::File("~/.claude/.credentials.json"),
            Item::JsonField {
                file: "~/.claude.json",
                pointer: "/oauthAccount",
            },
        ],
        mac_only: false,
        kimi_lock: None,
        env_overrides: &["ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN"],
    },
    Spec {
        runtime: "gemini",
        tier: Tier::B,
        items: &[
            Item::File("~/.gemini/oauth_creds.json"),
            Item::File("~/.gemini/google_accounts.json"),
            Item::File("~/.gemini/.env"),
            Item::JsonField {
                file: "~/.gemini/settings.json",
                pointer: "/security/auth",
            },
        ],
        mac_only: false,
        kimi_lock: None,
        env_overrides: &["GEMINI_API_KEY", "GOOGLE_API_KEY"],
    },
    Spec {
        runtime: "qwen",
        tier: Tier::B,
        items: &[
            Item::File("~/.qwen/oauth_creds.json"),
            Item::File("~/.qwen/.env"),
            Item::JsonField {
                file: "~/.qwen/settings.json",
                pointer: "/security/auth",
            },
        ],
        mac_only: false,
        kimi_lock: None,
        env_overrides: &[],
    },
    Spec {
        runtime: "opencode",
        tier: Tier::C,
        items: &[
            Item::File("~/.local/share/opencode/auth.json"),
            Item::JsonField {
                file: "~/.config/opencode/opencode.json",
                pointer: "/model",
            },
            Item::JsonField {
                file: "~/.config/opencode/opencode.json",
                pointer: "/provider",
            },
        ],
        mac_only: false,
        kimi_lock: None,
        env_overrides: &[],
    },
    Spec {
        runtime: "pi",
        tier: Tier::C,
        items: &[
            Item::File("~/.pi/agent/auth.json"),
            Item::JsonField {
                file: "~/.pi/agent/settings.json",
                pointer: "/defaultProvider",
            },
            Item::JsonField {
                file: "~/.pi/agent/settings.json",
                pointer: "/defaultModel",
            },
        ],
        mac_only: false,
        kimi_lock: None,
        env_overrides: &[],
    },
    Spec {
        runtime: "openclaw",
        tier: Tier::C,
        items: &[
            Item::JsonField {
                file: "~/.openclaw/openclaw.json",
                pointer: "/models/providers",
            },
            Item::JsonField {
                file: "~/.openclaw/openclaw.json",
                pointer: "/agents/defaults/model",
            },
        ],
        mac_only: false,
        kimi_lock: None,
        env_overrides: &[],
    },
    Spec {
        runtime: "hermes",
        tier: Tier::C,
        items: &[
            Item::File("~/.hermes/auth.json"),
            Item::File("~/.hermes/.anthropic_oauth.json"),
            Item::File("~/.hermes/.env"),
        ],
        mac_only: false,
        kimi_lock: None,
        env_overrides: &[],
    },
    Spec {
        runtime: "mcode",
        tier: Tier::C,
        items: &[Item::File("~/.minimax/config.yaml")],
        mac_only: false,
        kimi_lock: None,
        env_overrides: &[],
    },
    Spec {
        runtime: "deveco",
        tier: Tier::C,
        items: &[Item::File("~/.config/deveco/deveco.jsonc")],
        mac_only: false,
        kimi_lock: None,
        env_overrides: &[],
    },
];

pub fn spec(runtime: &str) -> Option<&'static Spec> {
    SPECS.iter().find(|spec| spec.runtime == runtime)
}

/// 「在终端登录」时跟在 CLI 后面的参数；没有登录子命令的 CLI 直接打开，在里面用 /login。
pub fn login_args(runtime: &str) -> &'static [&'static str] {
    match runtime {
        "codex" | "cursor" | "grok" | "kiro" => &["login"],
        _ => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_are_unique_within_each_spec() {
        for spec in SPECS {
            let mut tags: Vec<_> = spec.items.iter().map(Item::tag).collect();
            let count = tags.len();
            tags.sort();
            tags.dedup();
            assert_eq!(tags.len(), count, "{}", spec.runtime);
        }
    }
}
