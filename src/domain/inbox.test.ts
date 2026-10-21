// @vitest-environment node
import { describe, expect, it } from "vitest";
import { firstUnreadMessageId, inboxPreview, shouldNotifyInbox, unreadBadgeLabel, unreadTotal, type InboxNoticeInput } from "./inbox";

function notice(partial: Partial<InboxNoticeInput> = {}): InboxNoticeInput {
  return {
    status: "completed",
    previousStatus: "running",
    sessionId: "session-1",
    projectId: "project-1",
    taskId: null,
    currentPath: "/inbox",
    ...partial,
  };
}

describe("shouldNotifyInbox", () => {
  it("notifies a workbench chat when the user is not on that requirement", () => {
    expect(shouldNotifyInbox(notice())).toBe(true);
    expect(shouldNotifyInbox(notice({ currentPath: "/projects" }))).toBe(true);
    expect(shouldNotifyInbox(notice({ currentPath: "/projects/other" }))).toBe(true);
  });

  it("stays quiet while the user is already in that workbench or task", () => {
    expect(shouldNotifyInbox(notice({ currentPath: "/projects/project-1" }))).toBe(false);
    expect(shouldNotifyInbox(notice({
      taskId: "task-1",
      currentPath: "/tasks/task-1",
    }))).toBe(false);
  });

  it("notifies a task chat from anywhere except its own detail page", () => {
    expect(shouldNotifyInbox(notice({
      taskId: "task-1",
      currentPath: "/projects/project-1",
    }))).toBe(true);
  });

  it("ignores runs that are still going, automation sessions, and chats with no requirement", () => {
    expect(shouldNotifyInbox(notice({ previousStatus: "queued" }))).toBe(false);
    expect(shouldNotifyInbox(notice({ status: "running" }))).toBe(false);
    expect(shouldNotifyInbox(notice({ sessionId: "automation-run:1" }))).toBe(false);
    expect(shouldNotifyInbox(notice({ projectId: null, taskId: null }))).toBe(false);
  });

  it("notifies when the agent needs input or permission", () => {
    expect(shouldNotifyInbox(notice({ status: "needs_input" }))).toBe(true);
    expect(shouldNotifyInbox(notice({ status: "needs_permission" }))).toBe(true);
  });
});

describe("firstUnreadMessageId", () => {
  it("treats a conversation without a cursor as unread from the first message", () => {
    expect(firstUnreadMessageId(["m1", "m2"], null)).toBe("m1");
    expect(firstUnreadMessageId([], null)).toBeNull();
  });

  it("starts at the message after the read cursor", () => {
    expect(firstUnreadMessageId(["m1", "m2", "m3"], "m1")).toBe("m2");
    expect(firstUnreadMessageId(["m1", "m2"], "m2")).toBeNull();
    expect(firstUnreadMessageId(["m1", "m2"], "missing")).toBe("m1");
  });
});

describe("unread badges", () => {
  it("sums message counts and caps the label", () => {
    expect(unreadTotal([{ unreadCount: 1 }, { unreadCount: 20 }])).toBe(21);
    expect(unreadBadgeLabel(10)).toBe("10");
    expect(unreadBadgeLabel(100)).toBe("99+");
  });
});

describe("inboxPreview", () => {
  it("uses the latest assistant text and truncates it", () => {
    expect(inboxPreview(["旧的", "  新的回复  "])).toBe("新的回复");
    expect(inboxPreview([`${"甲".repeat(90)}`])).toBe(`${"甲".repeat(80)}…`);
    expect(inboxPreview(["", "  "], "")).toBe("对话已结束");
    expect(inboxPreview(["**1.2.1 已提交并开始发布。**"])).toBe("1.2.1 已提交并开始发布。");
  });
});