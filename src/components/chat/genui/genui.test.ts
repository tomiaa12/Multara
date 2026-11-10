import { beforeEach, describe, expect, it } from "vitest";
import { genUiBlockId, parseSpecs, scanFences, splitGenUi } from "./block";
import { niceTicks, pieSlices, barRects, valueRange } from "./chart-geometry";
import {
  initialFormValue,
  missingRequired,
  validateCard,
  validateChart,
  validateForm,
  validateStats,
  validateTable,
} from "./schema";
import { clearGenUiStateForTest, readGenUiState, writeGenUiState } from "./state";
import { genUiEntry, genUiLabel, genUiToText } from "./registry";
import { translate } from "../../../i18n";
import { extractQuestions } from "../question-block";
import { formatGenUiAction, genUiSendingIds, genUiSubmissions, sendGenUiAction } from "../ui-outbound";
import type { ChatMessage } from "../../../domain/types";

const fence = (json: string, lang = "multara-ui") => `\`\`\`${lang}\n${json}\n\`\`\``;

describe("splitGenUi", () => {
  it("leaves plain markdown as one segment", () => {
    expect(splitGenUi("hello\n\n```ts\nconst a = 1;\n```")).toEqual({
      segments: [{ kind: "md", text: "hello\n\n```ts\nconst a = 1;\n```" }],
      streaming: false,
    });
  });

  it("splits a block in the middle of text and keeps order", () => {
    const text = `before\n${fence('{"type":"stats","props":{"items":[]}}')}\nafter`;
    const { segments, streaming } = splitGenUi(text);
    expect(streaming).toBe(false);
    expect(segments.map((segment) => segment.kind)).toEqual(["md", "ui", "md"]);
    const ui = segments[1];
    expect(ui.kind === "ui" && ui.specs?.[0].type).toBe("stats");
    expect(segments.map((segment) => (segment.kind === "md" ? segment.text : segment.source)).join("\n")).toBe(text);
  });

  it("numbers multiple blocks", () => {
    const text = `${fence('{"type":"card","props":{"body":"a"}}')}\n${fence('{"type":"card","props":{"body":"b"}}')}`;
    const indexes = splitGenUi(text).segments.flatMap((segment) => (segment.kind === "ui" ? [segment.index] : []));
    expect(indexes).toEqual([0, 1]);
  });

  it("reports an unclosed block as pending (streaming)", () => {
    const { segments, streaming } = splitGenUi('intro\n```multara-ui\n{"type":"ta');
    expect(streaming).toBe(true);
    expect(segments.map((segment) => segment.kind)).toEqual(["md", "pending"]);
  });

  it("ignores blocks nested inside another fence", () => {
    const text = `\`\`\`\`markdown\n${fence('{"type":"card"}')}\n\`\`\`\``;
    expect(splitGenUi(text).segments).toEqual([{ kind: "md", text }]);
  });

  it("supports tilde fences", () => {
    const { segments } = splitGenUi('~~~multara-ui\n{"type":"card","props":{"body":"x"}}\n~~~');
    expect(segments[0].kind).toBe("ui");
  });

  it("marks bad JSON as unparsed", () => {
    const segment = splitGenUi(fence('{"type":')).segments[0];
    expect(segment.kind === "ui" && segment.specs).toBeNull();
  });
});

describe("parseSpecs", () => {
  it("accepts blocks arrays and defaults version", () => {
    const { specs } = parseSpecs('{"blocks":[{"type":"card","props":{}},{"type":"stats","version":2}]}');
    expect(specs?.map((spec) => [spec.type, spec.version])).toEqual([["card", 1], ["stats", 2]]);
  });

  it("rejects specs without type", () => {
    expect(parseSpecs('{"props":{}}').specs).toBeNull();
  });

  it("builds stable block ids", () => {
    expect(genUiBlockId("m1", 2)).toBe("m1-b2");
    expect(genUiBlockId("m1", 2, 1)).toBe("m1-b2.1");
  });
});

describe("schemas", () => {
  it("validates cards and caps actions", () => {
    const result = validateCard({
      fields: [{ label: "a", value: 1 }],
      actions: [1, 2, 3, 4, 5].map((index) => ({ id: `a${index}`, label: `A${index}` })),
    });
    expect(result.ok && result.props.actions).toHaveLength(4);
    expect(validateCard({}).ok).toBe(false);
  });

  it("validates stats", () => {
    const result = validateStats({ items: [{ label: "x", value: 3, delta: "1.5" }, { label: "" }] });
    expect(result.ok && result.props.items).toEqual([{ label: "x", value: "3", delta: 1.5, unit: "" }]);
    expect(validateStats({ items: [] }).ok).toBe(false);
  });

  it("truncates tables past the row limit and infers columns", () => {
    const rows = Array.from({ length: 250 }, (_, index) => ({ id: index, nested: { a: 1 } }));
    const result = validateTable({ rows });
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    expect(result.props.columns.map((column) => column.key)).toEqual(["id", "nested"]);
    expect(result.props.rows).toHaveLength(200);
    expect(result.props.rows[0].nested).toBe('{"a":1}');
    expect(result.truncated).toEqual({ shown: 200, total: 250 });
  });

  it("validates charts and keeps one series for pie", () => {
    const result = validateChart({
      chartType: "pie",
      xKey: "k",
      series: ["a", "b"],
      data: [{ k: "x", a: "2", b: 3 }],
    });
    expect(result.ok && result.props.series).toEqual([{ key: "a", label: "a" }]);
    expect(result.ok && result.props.data[0].a).toBe(2);
    expect(validateChart({ chartType: "radar", xKey: "k", series: ["a"], data: [{}] }).ok).toBe(false);
  });

  it("validates forms, drops duplicate names and downgrades empty selects", () => {
    const result = validateForm({
      fields: [
        { name: "a", kind: "select" },
        { name: "a", kind: "text" },
        { name: "b", kind: "weird", required: true },
      ],
    });
    expect(result.ok && result.props.fields.map((field) => [field.name, field.kind])).toEqual([["a", "text"], ["b", "text"]]);
  });

  it("computes initial values and required fields", () => {
    const result = validateForm({
      fields: [
        { name: "s", kind: "slider", min: 5 },
        { name: "m", kind: "multiselect", options: ["x", "y"], default: ["y", "z"] },
        { name: "t", kind: "text", required: true },
        { name: "w", kind: "switch", default: true },
      ],
    });
    if (!result.ok) throw new Error("form should validate");
    const values = Object.fromEntries(result.props.fields.map((field) => [field.name, initialFormValue(field)]));
    expect(values).toEqual({ s: 5, m: ["y"], t: "", w: true });
    expect(missingRequired(result.props.fields, values)).toBe("t");
    expect(missingRequired(result.props.fields, { ...values, t: "ok" })).toBeNull();
  });

  it("registers every first-batch type", () => {
    for (const type of ["card", "stats", "table", "chart", "form"]) {
      expect(genUiEntry(type)?.component).toBeTruthy();
    }
    expect(genUiEntry("question")?.component).toBeNull();
    expect(genUiEntry("map")).toBeUndefined();
  });

  it("labels each type for people", () => {
    for (const type of ["card", "stats", "table", "chart", "form", "question"]) {
      expect(translate(`genui.type.${type}`, "zh-CN")).not.toBe(`genui.type.${type}`);
      expect(genUiLabel(type)).not.toBe(type);
    }
    expect(translate("genui.type.form", "zh-CN")).toBe("表单");
    expect(genUiLabel("map")).toBe("map");
  });
});

describe("chart geometry", () => {
  it("makes nice ticks that include zero", () => {
    expect(niceTicks(0, 1800)).toEqual([0, 500, 1000, 1500, 2000]);
    expect(niceTicks(-3, 7)[0]).toBeLessThanOrEqual(-3);
  });

  it("stacks bar ranges and rects", () => {
    const props = {
      chartType: "bar" as const,
      xKey: "x",
      series: [{ key: "a", label: "a" }, { key: "b", label: "b" }],
      data: [{ x: "1", a: 2, b: 3 }],
      unit: "",
      stacked: true,
    };
    expect(valueRange(props)).toEqual({ min: 0, max: 5 });
    const rects = barRects(props, niceTicks(0, 5));
    expect(rects).toHaveLength(2);
    expect(rects[1].y).toBeLessThan(rects[0].y);
  });

  it("skips non-positive pie slices", () => {
    const slices = pieSlices({
      chartType: "pie",
      xKey: "x",
      series: [{ key: "v", label: "v" }],
      data: [{ x: "a", v: 3 }, { x: "b", v: 0 }, { x: "c", v: 1 }],
      unit: "",
      stacked: false,
    });
    expect(slices.map((slice) => [slice.label, slice.share])).toEqual([["a", 0.75], ["c", 0.25]]);
  });
});

describe("state cache", () => {
  beforeEach(() => clearGenUiStateForTest());

  it("keeps per-block fields", () => {
    writeGenUiState("m/0/table", "sort", { key: "a", dir: "asc" });
    expect(readGenUiState("m/0/table", "sort")).toEqual({ key: "a", dir: "asc" });
    expect(readGenUiState("m/1/table", "sort")).toBeUndefined();
  });
});

describe("question alias", () => {
  it("treats a multara-ui question block like multara-questions", () => {
    const json = JSON.stringify({ type: "question", props: { questions: [{ question: "Which?", options: ["A", "B"] }] } });
    const result = extractQuestions(`Pick one.\n\n${fence(json)}`, { genui: true });
    expect(result.body).toBe("Pick one.");
    expect(result.questions.map((question) => question.question)).toEqual(["Which?"]);
  });

  it("leaves multara-ui question blocks as code when the switch is off", () => {
    const json = JSON.stringify({ type: "question", props: { questions: [{ question: "Which?" }] } });
    const text = `Pick one.\n\n${fence(json)}`;
    expect(extractQuestions(text)).toEqual({ body: text, questions: [], streaming: false });
    expect(extractQuestions(text, { genui: false }).questions).toEqual([]);
  });

  it("does not turn other ui blocks into questions", () => {
    const text = `Data:\n${fence('{"type":"table","props":{"rows":[{"a":1}]}}')}`;
    expect(extractQuestions(text, { genui: true })).toEqual({ body: text, questions: [], streaming: false });
  });

  it("keeps legacy multara-questions working", () => {
    const result = extractQuestions(`Hi\n${fence('{"questions":[{"question":"Q?"}]}', "multara-questions")}`);
    expect(result.questions).toHaveLength(1);
    expect(result.body).toBe("Hi");
  });
});

describe("scanFences", () => {
  it("round-trips the original text", () => {
    const text = "a\n```multara-questions\n{}\n```\nb\n```ts\nx\n```\n```multara-questions\n{";
    const segments = scanFences(text, "multara-questions");
    expect(segments.map((segment) => segment.kind)).toEqual(["text", "block", "text", "open"]);
    expect(segments.map((segment) => (segment.kind === "text" ? segment.text : segment.source)).join("\n")).toBe(text);
  });
});

describe("copy content", () => {
  it("keeps the raw multara-ui fence in the body used for copy", () => {
    // AssistantMessage 的 bodyText（复制、hidesAssignment、isInternalReply 都用它）来自 extractQuestions().body
    const block = fence('{"type":"table","props":{"rows":[{"a":1}]}}');
    const text = `Result:\n\n${block}\n\nDone.`;
    expect(extractQuestions(text, { genui: true }).body).toBe(text);
  });
});

describe("toText", () => {
  it("turns validated blocks into plain text", () => {
    expect(genUiToText("table", { columns: ["a", "b"], rows: [{ a: 1, b: "x y" }] })).toBe("a | b\n1 | x y");
    expect(genUiToText("stats", { items: [{ label: "Orders", value: 12, delta: 3 }] })).toBe("Orders: 12 (+3)");
    expect(genUiToText("chart", {
      chartType: "line", xKey: "d", series: ["v"], data: [{ d: "mon", v: 1 }, { d: "tue", v: 4 }],
    })).toBe("v: n=2, min=1, max=4, tue=4");
    expect(genUiToText("card", { fields: [{ label: "A", value: "1" }], actions: [{ id: "ok", label: "OK" }] })).toBe("A: 1\n[OK]");
    expect(genUiToText("form", { fields: [{ name: "r", label: "Range", required: true, kind: "select", options: ["7d"] }] })).toBe("Range* (7d)");
    expect(genUiToText("map", {})).toBeNull();
  });
});

describe("ui-outbound", () => {
  it("sends once: a queued submission marks its block as sending", () => {
    const sent: unknown[] = [];
    const queue: { source?: { sourceType: string; meta: Record<string, unknown> } }[] = [];
    const send = (content: string, sessionId: string, attachments: [], options: { source: { sourceType: "genui"; meta: Record<string, unknown> } }) => {
      sent.push(content);
      queue.push({ source: options.source });
    };
    const action = { sessionId: "s", messageId: "m1", agentId: "a1", blockId: "m1-b0", type: "form", action: "submit", values: {} };
    // AssistantMessage 只在块不在发送队列、也没提交过时才调用 send
    const click = () => {
      if (genUiSendingIds(queue, "m1").has(action.blockId)) return;
      sendGenUiAction(send, action);
    };
    click();
    click();
    click();
    expect(sent).toHaveLength(1);
    expect(queue[0].source?.meta).toMatchObject({ blockId: "m1-b0", messageId: "m1", agentId: "a1" });
    expect([...genUiSendingIds(queue, "m2")]).toEqual([]);
    // 发送失败出队后可以再点
    queue.length = 0;
    click();
    expect(sent).toHaveLength(2);
  });

  it("falls back to the type label when the block has no title", () => {
    const head = formatGenUiAction({ blockId: "m1-b0", type: "form", action: "submit" }).split("\n")[0];
    expect(head).toContain(`· ${genUiLabel("form")}]`);
  });

  it("formats actions for people and models", () => {
    const text = formatGenUiAction({ blockId: "m1-b0", type: "form", title: "Filters", action: "submit", values: { a: 1 } });
    const [head, body] = text.split("\n");
    expect(head).toMatch(/· Filters\] submit$/);
    expect(JSON.parse(body)).toEqual({ blockId: "m1-b0", values: { a: 1 } });
  });

  it("derives submissions from later genui user messages", () => {
    const message = (partial: Partial<ChatMessage>) => ({ role: "user", content: "", ...partial }) as ChatMessage;
    const submissions = genUiSubmissions([
      message({ sourceType: "genui", sourceMeta: { messageId: "m1", blockId: "m1-b0", action: "submit", values: { a: 1 } } }),
      message({ sourceType: "genui", sourceMeta: { messageId: "m2", blockId: "m2-b0", action: "x" } }),
      message({ sourceType: "feishu", sourceMeta: { messageId: "m1", blockId: "m1-b1" } }),
    ], "m1");
    expect([...submissions.entries()]).toEqual([["m1-b0", { action: "submit", values: { a: 1 } }]]);
  });
});
