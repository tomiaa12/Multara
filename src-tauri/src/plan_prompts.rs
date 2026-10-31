//! 规划相关的提示词与文案，按 `app_locale` 的 locale 取值（未识别时用简体中文）。

/// 计划卡片上的批准 / 拒绝由界面提供，用户在输入框的回复会作为修改意见
pub fn reply_rules(locale: &str) -> &'static str {
    match locale {
        "en" => "[Plan output rules]\n\
- If something must be clarified first, only ask questions this turn and do not write a plan; write the plan after the user answers.\n\
- When you write a plan, do not append questions or multiple-choice prompts at the end, do not ask the user to reply to confirm, and do not explain how to exit plan mode. The UI provides an \"Approve & run\" button, and the user's replies are treated as revision feedback.",
        "zh-TW" => "【計畫輸出規則】\n\
- 有必須先釐清的問題時，本輪只提問、不出計畫，等使用者回答後再出計畫。\n\
- 出計畫時，結尾不要再附帶提問或選擇題，也不要請使用者回覆確認、不要說明如何退出計畫模式；介面會提供「核准並執行」按鈕，使用者的回覆會作為修改意見。",
        "ja" => "【計画の出力ルール】\n\
- 先に確認すべき点がある場合、このターンでは質問だけして計画は出さず、ユーザーの回答を待ってから計画を出してください。\n\
- 計画を出すときは、末尾に質問や選択肢を付けず、ユーザーに確認の返信を求めたり、計画モードの終了方法を説明したりしないでください。画面に「承認して実行」ボタンがあり、ユーザーの返信は修正意見として扱われます。",
        "ko" => "【계획 출력 규칙】\n\
- 먼저 확인해야 할 점이 있으면 이번 턴에는 질문만 하고 계획은 내지 마세요. 사용자가 답한 뒤 계획을 작성하세요.\n\
- 계획을 작성할 때는 끝에 질문이나 선택지를 덧붙이지 말고, 사용자에게 확인 답장을 요청하거나 계획 모드 종료 방법을 설명하지 마세요. 화면에 「승인 후 실행」 버튼이 있고, 사용자의 답장은 수정 의견으로 반영됩니다.",
        _ => "【计划输出规则】\n\
- 有必须先澄清的问题时，本轮只提问、不出计划，等用户回答后再出计划。\n\
- 出计划时，末尾不要再附带提问或选择题，也不要请用户回复确认、不要说明如何退出计划模式；界面会提供「批准并执行」按钮，用户的回复会作为修改意见。",
    }
}

/// Claude 自动模式只有走 EnterPlanMode / ExitPlanMode 才会被识别成计划；这两个工具是延迟加载的，要先 ToolSearch
pub fn auto_plan_rules(locale: &str) -> &'static str {
    match locale {
        "en" => "[When to plan first]\n\
- Plan before acting when the user asks for a plan or proposal first, or when the change spans multiple files or involves trade-offs between approaches. Do small, clear-cut changes directly.\n\
- Planning must go through plan mode: first run ToolSearch with `select:EnterPlanMode,ExitPlanMode` to load the tools, call EnterPlanMode, research read-only, then call ExitPlanMode to submit the full plan. Do not reply with a plan as plain text; the UI cannot show an approve button for it.",
        "zh-TW" => "【何時先規劃】\n\
- 使用者要求先出方案或計畫，或者改動跨多個檔案、有多種做法需要取捨時，先規劃再動手；簡單明確的小改動直接做。\n\
- 規劃必須走規劃模式：先用 ToolSearch 查詢 `select:EnterPlanMode,ExitPlanMode` 載入工具，呼叫 EnterPlanMode，唯讀調研後呼叫 ExitPlanMode 提交完整計畫。不要只用文字回覆計畫，那樣介面無法提供核准按鈕。",
        "ja" => "【先に計画を立てる場合】\n\
- ユーザーが先に方針や計画を求めたとき、または変更が複数ファイルにまたがる・複数のやり方から選ぶ必要があるときは、計画してから着手してください。小さく明確な変更はそのまま実施します。\n\
- 計画は必ず計画モードで行ってください。まず ToolSearch で `select:EnterPlanMode,ExitPlanMode` を検索してツールを読み込み、EnterPlanMode を呼び出し、読み取りのみで調査したあと ExitPlanMode で計画全体を提出します。計画をテキストだけで返さないでください。画面に承認ボタンを出せなくなります。",
        "ko" => "【먼저 계획할 때】\n\
- 사용자가 먼저 방안이나 계획을 요청하거나, 변경이 여러 파일에 걸치거나 여러 방법 중 선택이 필요하면 계획부터 세운 뒤 작업하세요. 작고 명확한 변경은 바로 하세요.\n\
- 계획은 반드시 계획 모드로 진행하세요. 먼저 ToolSearch로 `select:EnterPlanMode,ExitPlanMode`를 검색해 도구를 불러오고, EnterPlanMode를 호출한 뒤 읽기 전용으로 조사하고, ExitPlanMode로 전체 계획을 제출하세요. 계획을 텍스트로만 답하지 마세요. 그러면 화면에 승인 버튼을 표시할 수 없습니다.",
        _ => "【何时先规划】\n\
- 用户要求先出方案或计划，或者改动跨多个文件、有多种做法需要取舍时，先规划再动手；简单明确的小改动直接做。\n\
- 规划必须走规划模式：先用 ToolSearch 查询 `select:EnterPlanMode,ExitPlanMode` 加载工具，调用 EnterPlanMode，只读调研后调用 ExitPlanMode 提交完整计划。不要只用文字回复计划，那样界面无法给出批准按钮。",
    }
}

/// Claude 自动模式拦截 ExitPlanMode 后回给模型的拒绝说明
pub fn exit_plan_denied(locale: &str) -> &'static str {
    match locale {
        "en" => "The plan has been submitted for the user's approval. Stop and wait.",
        "zh-TW" => "計畫已提交給使用者審核，請停止並等待",
        "ja" => "計画をユーザーの承認待ちとして提出しました。ここで止めて待ってください。",
        "ko" => "계획을 사용자 승인 대기로 제출했습니다. 멈추고 기다리세요.",
        _ => "计划已提交给用户审批，请停止并等待",
    }
}

/// 计划没有标题时的兜底，与前端 `chat.plan.untitled` 对齐
pub fn untitled(locale: &str) -> &'static str {
    match locale {
        "en" => "Untitled plan",
        "zh-TW" => "未命名計畫",
        "ja" => "無題の計画",
        "ko" => "제목 없는 계획",
        _ => "未命名计划",
    }
}

/// 批准后写进会话的用户消息
pub fn approved_message(locale: &str, title: &str) -> String {
    match locale {
        "en" => format!("Approved plan: {title}"),
        "zh-TW" => format!("核准計畫：{title}"),
        "ja" => format!("計画を承認：{title}"),
        "ko" => format!("계획 승인: {title}"),
        _ => format!("批准计划：{title}"),
    }
}

/// 批准后发给执行轮的任务
pub fn approved_assignment(locale: &str, markdown: &str) -> String {
    match locale {
        "en" => format!("The user approved the plan below. Follow it exactly and report the results when done.\n\n{markdown}"),
        "zh-TW" => format!("使用者已核准以下計畫。請嚴格按計畫執行，並在完成後回報結果。\n\n{markdown}"),
        "ja" => format!("ユーザーが以下の計画を承認しました。計画どおりに実行し、完了したら結果を報告してください。\n\n{markdown}"),
        "ko" => format!("사용자가 아래 계획을 승인했습니다. 계획대로 정확히 실행하고 완료되면 결과를 보고하세요.\n\n{markdown}"),
        _ => format!("用户已批准以下计划。请严格按计划执行，并在完成后汇报结果。\n\n{markdown}"),
    }
}

/// 渠道（飞书等）计划卡片上的文案
pub struct PlanCardText {
    pub heading: &'static str,
    pub approve: &'static str,
    pub refine: &'static str,
    pub reject: &'static str,
    /// 点「继续完善」后提示用户怎么补充意见
    pub refine_hint: &'static str,
}

pub fn plan_card_text(locale: &str) -> PlanCardText {
    match locale {
        "en" => PlanCardText {
            heading: "Plan awaiting approval: ",
            approve: "Approve",
            refine: "Refine",
            reject: "Reject",
            refine_hint: "Reply with /plan followed by your feedback to keep refining the plan.",
        },
        "zh-TW" => PlanCardText {
            heading: "計畫待確認：",
            approve: "核准",
            refine: "繼續完善",
            reject: "拒絕",
            refine_hint: "請用 /plan 補充意見 回覆，以繼續完善計畫。",
        },
        "ja" => PlanCardText {
            heading: "承認待ちの計画：",
            approve: "承認",
            refine: "さらに詰める",
            reject: "却下",
            refine_hint: "計画を詰めるには「/plan 追加の意見」の形で返信してください。",
        },
        "ko" => PlanCardText {
            heading: "승인 대기 중인 계획: ",
            approve: "승인",
            refine: "계속 다듬기",
            reject: "거절",
            refine_hint: "계획을 계속 다듬으려면 「/plan 추가 의견」 형식으로 답장하세요.",
        },
        _ => PlanCardText {
            heading: "计划待确认：",
            approve: "批准",
            refine: "继续完善",
            reject: "拒绝",
            refine_hint: "请用 /plan 补充意见 回复，以继续完善计划。",
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_locale_has_its_own_text() {
        for locale in crate::app_locale::LOCALES {
            if *locale != "zh-CN" {
                assert_ne!(reply_rules(locale), reply_rules("zh-CN"), "{locale}");
                assert_ne!(auto_plan_rules(locale), auto_plan_rules("zh-CN"), "{locale}");
                assert_ne!(exit_plan_denied(locale), exit_plan_denied("zh-CN"), "{locale}");
                assert_ne!(plan_card_text(locale).heading, plan_card_text("zh-CN").heading, "{locale}");
            }
            assert!(auto_plan_rules(locale).contains("select:EnterPlanMode,ExitPlanMode"));
            assert!(approved_assignment(locale, "# P").ends_with("# P"));
        }
    }
}
