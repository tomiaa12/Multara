/** 从示例 JSON 推断 JSON Schema（draft-07 子集），用于端口 schema 编辑的「从示例生成」。 */
export type Schema = Record<string, unknown>;

export function inferSchema(sample: unknown, depth = 0): Schema {
  if (sample === null || sample === undefined) return {};
  if (Array.isArray(sample)) {
    if (!sample.length || depth > 6) return { type: "array" };
    return { type: "array", items: inferSchema(sample[0], depth + 1) };
  }
  switch (typeof sample) {
    case "string": return { type: "string" };
    case "number": return { type: Number.isInteger(sample) ? "integer" : "number" };
    case "boolean": return { type: "boolean" };
    case "object": {
      if (depth > 6) return { type: "object" };
      const entries = Object.entries(sample as Record<string, unknown>);
      const schema: Schema = {
        type: "object",
        properties: Object.fromEntries(entries.map(([key, value]) => [key, inferSchema(value, depth + 1)])),
      };
      if (entries.length) schema.required = entries.filter(([, value]) => value !== null && value !== undefined).map(([key]) => key);
      return schema;
    }
    default: return {};
  }
}

/** 解析可能为空的 JSON 文本：空串返回 undefined，非法返回 Error */
export function parseJsonText(text: string): unknown | undefined | Error {
  if (!text.trim()) return undefined;
  try {
    return JSON.parse(text);
  } catch (cause) {
    return cause instanceof Error ? cause : new Error(String(cause));
  }
}
