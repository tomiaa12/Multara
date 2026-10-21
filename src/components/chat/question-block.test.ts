import { describe, expect, test } from "vitest";
import { extractQuestions, formatAnswers, parseQuestions } from "./question-block";

const reply = [
  "当前工作区绑的是需求：万元入金开户驳回场景。",
  "",
  "```multara-questions",
  JSON.stringify({
    questions: [
      {
        question: "你现在想先做哪一项？",
        header: "下一步",
        options: [
          { label: "摸清现状", description: "先查代码与接口给结论" },
          "直接改代码",
        ],
      },
    ],
  }),
  "```",
].join("\n");

describe("extractQuestions", () => {
  test("strips the block from the body and parses options", () => {
    const result = extractQuestions(reply);
    expect(result.body).toBe("当前工作区绑的是需求：万元入金开户驳回场景。");
    expect(result.streaming).toBe(false);
    expect(result.questions).toEqual([
      {
        id: "q1",
        question: "你现在想先做哪一项？",
        header: "下一步",
        options: [
          { label: "摸清现状", description: "先查代码与接口给结论" },
          { label: "直接改代码" },
        ],
        multiSelect: false,
        allowCustom: true,
      },
    ]);
  });

  test("hides a half-streamed block", () => {
    const result = extractQuestions("先说明一下。\n\n```multara-questions\n{\"questions\":[{\"quest");
    expect(result.body).toBe("先说明一下。");
    expect(result.streaming).toBe(true);
    expect(result.questions).toEqual([]);
  });

  test("leaves ordinary text and code fences alone", () => {
    const text = "看这里\n```ts\nconst a = 1;\n```";
    expect(extractQuestions(text)).toEqual({ body: text, questions: [], streaming: false });
  });

  test("drops a malformed block without leaking the JSON", () => {
    const result = extractQuestions("正文\n```multara-questions\n{oops}\n```");
    expect(result.body).toBe("正文");
    expect(result.questions).toEqual([]);
  });
});

describe("parseQuestions", () => {
  test("accepts a bare array and a single object", () => {
    expect(parseQuestions('[{"question":"A?","options":["x"]}]')).toHaveLength(1);
    expect(parseQuestions('{"question":"B?","multiSelect":true}')[0]).toMatchObject({
      question: "B?",
      multiSelect: true,
      allowCustom: true,
    });
  });

  test("skips entries without question text", () => {
    expect(parseQuestions('{"questions":[{"options":["x"]},{"question":"ok"}]}')).toHaveLength(1);
  });
});

describe("formatAnswers", () => {
  const questions = parseQuestions(JSON.stringify([
    { id: "scope", header: "范围", question: "改哪里？", options: ["client", "acct"], multiSelect: true },
    { id: "when", question: "什么时候？", options: ["现在"] },
  ]));

  test("labels each answer when there are several questions", () => {
    expect(formatAnswers(questions, {
      scope: { selected: ["client", "acct"] },
      when: { selected: [], custom: "明天" },
    })).toBe("范围：client、acct\n什么时候？：明天");
  });

  test("sends only the value for a single question", () => {
    expect(formatAnswers(questions.slice(1), { when: { selected: ["现在"] } })).toBe("现在");
  });

  test("marks unanswered questions as skipped", () => {
    expect(formatAnswers(questions, {})).toBe("范围：（跳过）\n什么时候？：（跳过）");
  });
});
