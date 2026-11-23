// @vitest-environment node
import { describe, expect, it } from "vitest";
import type { WorkflowDef } from "./workflow";
import { renameNodeInput, renameNodeOutput, replacePath } from "./workflow-rename";

describe("replacePath", () => {
  it("renames a template variable and leaves longer names alone", () => {
    const source = "{{ inputs.question }} 和 {{ inputs.questionExtra }} 以及 {{ inputs.question.title }}";
    expect(replacePath(source, "inputs.", "question", "用户问题")).toBe(
      "{{ inputs.用户问题 }} 和 {{ inputs.questionExtra }} 以及 {{ inputs.用户问题.title }}",
    );
  });

  it("renames bracket access", () => {
    expect(replacePath(`{{ inputs["question"] }}`, "inputs.", "question", "q2")).toBe(`{{ inputs["q2"] }}`);
  });
});

describe("rename ports", () => {
  function def(): WorkflowDef {
    return {
      schemaVersion: 1,
      inputs: [{ name: "question", type: "text", required: true }],
      workspaces: [{ key: "app", label: "代码", project: "{{ run.inputs.question }}", isolation: "direct" }],
      nodes: [
        { id: "start", kind: "trigger", title: "开始", config: { sources: ["manual"] }, position: { x: 0, y: 0 } },
        {
          id: "ask", kind: "agent", title: "回答",
          config: { prompt: "问题：{{ inputs.question }}\n上游：{{ nodes.start.outputs.question }}" },
          position: { x: 0, y: 0 },
        },
      ],
      edges: [{ id: "e1", source: "start", target: "ask" }],
      settings: { ownerAgentId: "", maxParallelNodes: 4, concurrency: "allow" },
    };
  }

  it("keeps the edge and updates both the node input and the workflow input", () => {
    const workflow = def();
    renameNodeOutput(workflow, "start", "question", "用户问题");
    renameNodeInput(workflow, "ask", "question", "用户问题");
    expect(workflow.edges[0]).toMatchObject({ source: "start", target: "ask" });
    expect(workflow.nodes[1].config).toMatchObject({
      prompt: "问题：{{ inputs.用户问题 }}\n上游：{{ nodes.start.outputs.用户问题 }}",
    });
    expect(workflow.workspaces[0].project).toBe("{{ run.inputs.用户问题 }}");
  });
});
