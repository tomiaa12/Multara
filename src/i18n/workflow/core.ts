import type { Locale } from "../types";
import type { LocaleParts } from "./index";

/** [key, zh-CN, zh-TW, en, ja, ko] */
type Row = [string, string, string, string, string, string];

const rows: Row[] = [
  ["workflow.desktopOnly", "请在桌面应用中使用工作流", "請在桌面應用程式中使用工作流程", "Please use workflows in the desktop app", "ワークフローはデスクトップアプリで使用してください", "워크플로는 데스크톱 앱에서 사용하세요"],

  // ---- 画布 / 节点 ----
  ["workflow.canvas.assignments", "{count} 个赋值", "{count} 個賦值", "{count} assignments", "{count} 件の代入", "할당 {count}개"],
  ["workflow.canvas.gateApprove", "通过", "通過", "Approve", "承認", "승인"],
  ["workflow.canvas.gateChanges", "打回修改", "退回修改", "Request changes", "差し戻し", "수정 요청"],
  ["workflow.canvas.gateReject", "拒绝", "拒絕", "Reject", "却下", "거부"],
  ["workflow.canvas.iterAttempt", "第 {iteration} 轮 · 第 {attempt} 次", "第 {iteration} 輪 · 第 {attempt} 次", "Round {iteration} · attempt {attempt}", "{iteration} 周目 · {attempt} 回目", "{iteration}라운드 · {attempt}번째 시도"],
  ["workflow.canvas.maxRounds", "≤{count} 轮", "≤{count} 輪", "≤{count} rounds", "最大 {count} 回", "최대 {count}회"],
  ["workflow.canvas.noAgent", "未选择智能体", "未選擇智慧體", "No agent selected", "エージェント未選択", "에이전트 미선택"],
  ["workflow.canvas.noCommand", "未填写命令", "未填寫指令", "No command", "コマンド未入力", "명령 없음"],
  ["workflow.canvas.noSlot", "未选槽位", "未選槽位", "No slot", "スロット未選択", "슬롯 없음"],
  ["workflow.canvas.noWorkflow", "未选择工作流", "未選擇工作流程", "No workflow selected", "ワークフロー未選択", "워크플로 미선택"],
  ["workflow.canvas.planMode", "计划模式", "計畫模式", "Plan", "プラン", "플랜"],

  // ---- 编辑器 ----
  ["workflow.editor.autoLayout", "自动布局", "自動佈局", "Auto layout", "自動レイアウト", "자동 정렬"],
  ["workflow.editor.back", "返回工作流列表", "返回工作流程列表", "Back to workflows", "ワークフロー一覧へ", "워크플로 목록으로"],
  ["workflow.editor.backEdgeTitle", "创建循环（回边）", "建立循環（回邊）", "Create loop (back edge)", "ループ（バックエッジ）を作成", "루프(백 엣지) 만들기"],
  ["workflow.editor.backEdgeHint", "目标节点是上游节点，这条线会让流程回到那里重新执行。需要设置最多循环的轮数，超出后触发 exhausted 端口。", "目標節點是上游節點，這條線會讓流程回到那裡重新執行。需設定最多循環輪數，超出後觸發 exhausted 連接埠。", "The target is an upstream node, so this edge sends the flow back to run again. Set the maximum number of rounds; after that the exhausted port fires.", "接続先は上流ノードのため、この線でフローがそこへ戻って再実行されます。最大周回数を設定してください。超過すると exhausted ポートが発火します。", "대상이 상위 노드이므로 이 선은 흐름을 되돌려 다시 실행합니다. 최대 반복 횟수를 설정하세요. 초과하면 exhausted 포트가 실행됩니다."],
  ["workflow.editor.maxIterations", "最多循环轮数（1~20）", "最多循環輪數（1~20）", "Max rounds (1-20)", "最大周回数（1〜20）", "최대 반복 횟수 (1~20)"],
  ["workflow.editor.conflict", "草稿已在别处被修改，当前改动未保存", "草稿已在其他地方被修改，目前的變更尚未儲存", "The draft was changed elsewhere; your edits are not saved", "下書きが別の場所で変更されました。現在の編集は保存されていません", "초안이 다른 곳에서 변경되어 현재 수정 내용이 저장되지 않았습니다"],
  ["workflow.editor.copy", "复制", "複製", "Copy", "コピー", "복사"],
  ["workflow.editor.dirty", "有未保存的改动…", "有未儲存的變更…", "Unsaved changes…", "未保存の変更あり…", "저장되지 않은 변경 사항…"],
  ["workflow.editor.draftChanges", "草稿 · 有未发布改动（已发布 v{version}）", "草稿 · 有未發布變更（已發布 v{version}）", "Draft · unpublished changes (published v{version})", "下書き · 未公開の変更あり（公開済み v{version}）", "초안 · 게시되지 않은 변경 (게시됨 v{version})"],
  ["workflow.editor.draftUnpublished", "草稿 · 未发布", "草稿 · 未發布", "Draft · not published", "下書き · 未公開", "초안 · 미게시"],
  ["workflow.editor.errorsBadge", "{count} 个错误，发布前需要解决", "{count} 個錯誤，發布前需要解決", "{count} errors to fix before publishing", "公開前に解決が必要なエラーが {count} 件あります", "게시 전에 해결해야 할 오류 {count}개"],
  ["workflow.editor.export", "导出为文件", "匯出為檔案", "Export to file", "ファイルにエクスポート", "파일로 내보내기"],
  ["workflow.editor.import", "从文件导入为新工作流", "從檔案匯入為新工作流程", "Import file as new workflow", "ファイルから新規ワークフローとしてインポート", "파일에서 새 워크플로로 가져오기"],
  ["workflow.editor.multiTitle", "已选中 {count} 项", "已選取 {count} 項", "{count} items selected", "{count} 件を選択中", "{count}개 선택됨"],
  ["workflow.editor.name", "工作流名称", "工作流程名稱", "Workflow name", "ワークフロー名", "워크플로 이름"],
  ["workflow.editor.notFound", "找不到这个工作流", "找不到這個工作流程", "Workflow not found", "ワークフローが見つかりません", "워크플로를 찾을 수 없습니다"],
  ["workflow.editor.publish", "发布", "發布", "Publish", "公開", "게시"],
  ["workflow.editor.publishTitle", "发布新版本", "發布新版本", "Publish a new version", "新しいバージョンを公開", "새 버전 게시"],
  ["workflow.editor.publishBlocked", "还有 {count} 个错误，发布会被拒绝", "還有 {count} 個錯誤，發布會被拒絕", "{count} errors remain; publishing will be rejected", "エラーが {count} 件残っているため公開は拒否されます", "오류가 {count}개 남아 있어 게시가 거부됩니다"],
  ["workflow.editor.publishNote", "版本说明", "版本說明", "Release note", "リリースノート", "릴리스 노트"],
  ["workflow.editor.publishNotePlaceholder", "这一版改了什么", "這一版改了什麼", "What changed in this version", "このバージョンの変更点", "이 버전에서 바뀐 내용"],
  ["workflow.editor.published", "已发布 v{version}", "已發布 v{version}", "Published v{version}", "公開済み v{version}", "게시됨 v{version}"],
  ["workflow.editor.redo", "重做 (⇧⌘Z)", "重做 (⇧⌘Z)", "Redo (⇧⌘Z)", "やり直す (⇧⌘Z)", "다시 실행 (⇧⌘Z)"],
  ["workflow.editor.undo", "撤销 (⌘Z)", "復原 (⌘Z)", "Undo (⌘Z)", "元に戻す (⌘Z)", "실행 취소 (⌘Z)"],
  ["workflow.editor.reload", "重新载入", "重新載入", "Reload", "再読み込み", "다시 불러오기"],
  ["workflow.editor.run", "运行", "執行", "Run", "実行", "실행"],
  ["workflow.editor.runNeedsPublish", "发布后才能运行已发布版本", "發布後才能執行已發布版本", "Publish first to run the published version", "公開後に公開版を実行できます", "게시한 뒤에 게시 버전을 실행할 수 있습니다"],
  ["workflow.editor.runs", "运行记录", "執行記錄", "Runs", "実行履歴", "실행 기록"],
  ["workflow.editor.saveFailed", "保存失败：{error}", "儲存失敗：{error}", "Save failed: {error}", "保存に失敗しました: {error}", "저장 실패: {error}"],
  ["workflow.editor.saveTemplate", "保存为节点模板", "儲存為節點範本", "Save as node template", "ノードテンプレートとして保存", "노드 템플릿으로 저장"],
  ["workflow.editor.saving", "保存中…", "儲存中…", "Saving…", "保存中…", "저장 중…"],
  ["workflow.editor.shortcuts", "⌘C 复制 · ⌘V 粘贴 · Delete 删除 · ⌘Z 撤销 · 拖动或按住 Shift 框选", "⌘C 複製 · ⌘V 貼上 · Delete 刪除 · ⌘Z 復原 · 拖曳或按住 Shift 框選", "⌘C copy · ⌘V paste · Delete remove · ⌘Z undo · hold Shift to box-select", "⌘C コピー · ⌘V 貼り付け · Delete 削除 · ⌘Z 元に戻す · Shift で範囲選択", "⌘C 복사 · ⌘V 붙여넣기 · Delete 삭제 · ⌘Z 실행 취소 · Shift로 영역 선택"],
  ["workflow.editor.templateDesc", "说明（可选）", "說明（選填）", "Description (optional)", "説明（任意）", "설명 (선택)"],
  ["workflow.editor.templateName", "模板名称", "範本名稱", "Template name", "テンプレート名", "템플릿 이름"],
  ["workflow.editor.testRun", "试运行", "試執行", "Test run", "テスト実行", "테스트 실행"],
  ["workflow.editor.testRunHint", "用当前草稿跑一次，不需要先发布", "以目前草稿執行一次，不需先發布", "Run the current draft without publishing", "公開せず現在の下書きで実行します", "게시하지 않고 현재 초안으로 실행합니다"],
  ["workflow.editor.validate", "校验", "驗證", "Validate", "検証", "검증"],

  // ---- 列表 ----
  ["workflow.list.title", "工作流", "工作流程", "Workflows", "ワークフロー", "워크플로"],
  ["workflow.list.subtitle", "把智能体、脚本、人工审批编排成可重复运行的流程", "將智慧體、腳本、人工審批編排成可重複執行的流程", "Orchestrate agents, scripts and human approvals into repeatable flows", "エージェント・スクリプト・人の承認を繰り返し実行できるフローにまとめます", "에이전트, 스크립트, 사람의 승인을 반복 실행 가능한 흐름으로 구성합니다"],
  ["workflow.list.create", "新建工作流", "新增工作流程", "New workflow", "ワークフローを作成", "새 워크플로"],
  ["workflow.list.import", "导入", "匯入", "Import", "インポート", "가져오기"],
  ["workflow.list.blank", "空白工作流", "空白工作流程", "Blank workflow", "空のワークフロー", "빈 워크플로"],
  ["workflow.list.name", "名称", "名稱", "Name", "名前", "이름"],
  ["workflow.list.namePlaceholder", "例如：需求到上线", "例如：需求到上線", "e.g. Requirement to release", "例: 要件からリリースまで", "예: 요구사항에서 릴리스까지"],
  ["workflow.list.description", "说明（可选）", "說明（選填）", "Description (optional)", "説明（任意）", "설명 (선택)"],
  ["workflow.list.template", "从模板创建", "從範本建立", "Start from template", "テンプレートから作成", "템플릿에서 시작"],
  ["workflow.list.empty", "还没有工作流", "尚無工作流程", "No workflows yet", "ワークフローがありません", "워크플로가 없습니다"],
  ["workflow.list.emptyHint", "新建一个空白工作流，或从内置模板开始。", "新增空白工作流程，或從內建範本開始。", "Create a blank workflow or start from a built-in template.", "空のワークフローを作成するか、組み込みテンプレートから始めましょう。", "빈 워크플로를 만들거나 기본 제공 템플릿으로 시작하세요."],
  ["workflow.list.unpublished", "未发布", "未發布", "Unpublished", "未公開", "미게시"],
  ["workflow.list.hasChanges", "有未发布改动", "有未發布變更", "unpublished changes", "未公開の変更あり", "게시되지 않은 변경 있음"],
  ["workflow.list.nodes", "{count} 个节点", "{count} 個節點", "{count} nodes", "{count} ノード", "노드 {count}개"],
  ["workflow.list.lastRun", "最近运行：{time}", "最近執行：{time}", "Last run: {time}", "最終実行: {time}", "최근 실행: {time}"],
  ["workflow.list.enable", "启用工作流", "啟用工作流程", "Enable workflow", "ワークフローを有効化", "워크플로 활성화"],
  ["workflow.list.enableNeedsPublish", "发布后才能启用", "發布後才能啟用", "Publish before enabling", "公開後に有効化できます", "게시 후 활성화할 수 있습니다"],
  ["workflow.list.runs", "运行记录", "執行記錄", "Runs", "実行履歴", "실행 기록"],
  ["workflow.list.duplicate", "创建副本", "建立副本", "Duplicate", "複製", "복제"],
  ["workflow.list.deleteConfirm", "确定删除工作流「{name}」？运行记录也会一并删除。", "確定刪除工作流程「{name}」？執行記錄也會一併刪除。", "Delete workflow “{name}”? Its run history will be deleted too.", "ワークフロー「{name}」を削除しますか？実行履歴も削除されます。", "워크플로 “{name}”을(를) 삭제할까요? 실행 기록도 함께 삭제됩니다."],

  // ---- 节点库 ----
  ["workflow.palette.search", "搜索节点", "搜尋節點", "Search nodes", "ノードを検索", "노드 검색"],
  ["workflow.palette.base", "基础节点", "基礎節點", "Basic nodes", "基本ノード", "기본 노드"],
  ["workflow.palette.builtin", "内置模板", "內建範本", "Built-in templates", "組み込みテンプレート", "기본 제공 템플릿"],
  ["workflow.palette.mine", "我的模板", "我的範本", "My templates", "マイテンプレート", "내 템플릿"],
  ["workflow.palette.mineEmpty", "选中节点后点「保存为节点模板」", "選取節點後點「儲存為節點範本」", "Select nodes and click “Save as node template”", "ノードを選択して「ノードテンプレートとして保存」", "노드를 선택하고 “노드 템플릿으로 저장”을 누르세요"],
  ["workflow.palette.agents", "智能体与小队", "智慧體與小隊", "Agents & squads", "エージェントとスクワッド", "에이전트와 스쿼드"],
  ["workflow.palette.dragHint", "拖到画布，或双击添加", "拖曳到畫布，或雙擊新增", "Drag onto the canvas or double-click to add", "キャンバスへドラッグ、またはダブルクリックで追加", "캔버스로 끌어놓거나 더블클릭하여 추가"],
  ["workflow.palette.deleteTemplate", "删除模板「{name}」？", "刪除範本「{name}」？", "Delete template “{name}”?", "テンプレート「{name}」を削除しますか？", "템플릿 “{name}”을(를) 삭제할까요?"],

  // ---- 问题面板 ----
  ["workflow.problems.title", "问题", "問題", "Problems", "問題", "문제"],
  ["workflow.problems.none", "没有发现问题", "沒有發現問題", "No problems found", "問題は見つかりませんでした", "문제가 없습니다"],
];

const order: Locale[] = ["zh-CN", "zh-TW", "en", "ja", "ko"];
const parts: LocaleParts = { "zh-CN": {}, "zh-TW": {}, en: {}, ja: {}, ko: {} };
for (const [key, ...values] of rows) {
  order.forEach((locale, index) => {
    parts[locale][key] = values[index];
  });
}

export default parts;
