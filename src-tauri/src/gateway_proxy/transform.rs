use serde_json::{json, Map, Value};

use super::ApiFormat;

#[derive(Default)]
struct CanonicalRequest {
    system: String,
    messages: Vec<Message>,
    tools: Vec<Tool>,
    max_tokens: Option<u64>,
    temperature: Option<f64>,
}

#[derive(Default)]
struct Message {
    role: String,
    parts: Vec<Part>,
}

enum Part {
    Text(String),
    ToolCall {
        id: String,
        name: String,
        arguments: Value,
    },
    ToolResult {
        id: String,
        output: Value,
    },
}

struct Tool {
    name: String,
    description: String,
    parameters: Value,
}

#[derive(Default)]
pub(super) struct CanonicalResponse {
    id: String,
    model: String,
    text: String,
    tool_calls: Vec<(String, String, Value)>,
    input_tokens: u64,
    output_tokens: u64,
}

pub(super) fn rewrite_request(
    body: Value,
    client: ApiFormat,
    upstream: ApiFormat,
    model: &str,
) -> Result<(Value, bool), String> {
    let wants_stream = body.get("stream").and_then(Value::as_bool).unwrap_or(false);
    let mut request = parse_request(body, client)?;
    let mut rendered = render_request(&mut request, upstream, model);
    if let Some(object) = rendered.as_object_mut() {
        object.insert("stream".into(), Value::Bool(false));
    }
    Ok((rendered, wants_stream))
}

pub(super) fn rewrite_response(
    body: Value,
    upstream: ApiFormat,
    client: ApiFormat,
    model: &str,
) -> Result<(Value, CanonicalResponse), String> {
    let mut response = parse_response(body, upstream)?;
    if response.model.is_empty() {
        response.model = model.to_string();
    }
    let rendered = render_response(&response, client);
    Ok((rendered, response))
}

pub(super) fn synthetic_sse(client: ApiFormat, response: &CanonicalResponse) -> String {
    match client {
        ApiFormat::Anthropic => anthropic_sse(response),
        ApiFormat::Responses => responses_sse(response),
        ApiFormat::Chat => chat_sse(response),
        ApiFormat::Gemini => gemini_sse(response),
    }
}

fn parse_request(body: Value, format: ApiFormat) -> Result<CanonicalRequest, String> {
    match format {
        ApiFormat::Anthropic => parse_anthropic_request(body),
        ApiFormat::Responses => parse_responses_request(body),
        ApiFormat::Chat => parse_chat_request(body),
        ApiFormat::Gemini => parse_gemini_request(body),
    }
}

fn parse_anthropic_request(body: Value) -> Result<CanonicalRequest, String> {
    let mut request = CanonicalRequest {
        system: text_value(body.get("system")),
        max_tokens: body.get("max_tokens").and_then(Value::as_u64),
        temperature: body.get("temperature").and_then(Value::as_f64),
        ..CanonicalRequest::default()
    };
    for message in body
        .get("messages")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        request.messages.push(Message {
            role: message
                .get("role")
                .and_then(Value::as_str)
                .unwrap_or("user")
                .to_string(),
            parts: parse_anthropic_parts(message.get("content")),
        });
    }
    request.tools = body
        .get("tools")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|tool| {
            Some(Tool {
                name: tool.get("name")?.as_str()?.to_string(),
                description: tool
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                parameters: tool
                    .get("input_schema")
                    .cloned()
                    .unwrap_or_else(empty_schema),
            })
        })
        .collect();
    Ok(request)
}

fn parse_responses_request(body: Value) -> Result<CanonicalRequest, String> {
    let mut request = CanonicalRequest {
        system: text_value(body.get("instructions")),
        max_tokens: body.get("max_output_tokens").and_then(Value::as_u64),
        temperature: body.get("temperature").and_then(Value::as_f64),
        ..CanonicalRequest::default()
    };
    let input = body.get("input").cloned().unwrap_or(Value::Null);
    if let Some(text) = input.as_str() {
        request.messages.push(Message {
            role: "user".into(),
            parts: vec![Part::Text(text.to_string())],
        });
    } else if let Some(items) = input.as_array() {
        for item in items {
            match item.get("type").and_then(Value::as_str) {
                Some("function_call") => request.messages.push(Message {
                    role: "assistant".into(),
                    parts: vec![Part::ToolCall {
                        id: item
                            .get("call_id")
                            .or_else(|| item.get("id"))
                            .and_then(Value::as_str)
                            .unwrap_or("call")
                            .to_string(),
                        name: item
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or("tool")
                            .to_string(),
                        arguments: parse_arguments(item.get("arguments")),
                    }],
                }),
                Some("function_call_output") => request.messages.push(Message {
                    role: "user".into(),
                    parts: vec![Part::ToolResult {
                        id: item
                            .get("call_id")
                            .and_then(Value::as_str)
                            .unwrap_or("call")
                            .to_string(),
                        output: item.get("output").cloned().unwrap_or(Value::Null),
                    }],
                }),
                _ => request.messages.push(Message {
                    role: item
                        .get("role")
                        .and_then(Value::as_str)
                        .unwrap_or("user")
                        .to_string(),
                    parts: parse_openai_parts(item.get("content")),
                }),
            }
        }
    }
    request.tools = parse_openai_tools(body.get("tools"));
    Ok(request)
}

fn parse_chat_request(body: Value) -> Result<CanonicalRequest, String> {
    let mut request = CanonicalRequest {
        max_tokens: body
            .get("max_completion_tokens")
            .or_else(|| body.get("max_tokens"))
            .and_then(Value::as_u64),
        temperature: body.get("temperature").and_then(Value::as_f64),
        ..CanonicalRequest::default()
    };
    for message in body
        .get("messages")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let role = message
            .get("role")
            .and_then(Value::as_str)
            .unwrap_or("user");
        if role == "system" || role == "developer" {
            request.system.push_str(&text_value(message.get("content")));
            continue;
        }
        let mut parts = parse_openai_parts(message.get("content"));
        if role == "tool" {
            parts.push(Part::ToolResult {
                id: message
                    .get("tool_call_id")
                    .and_then(Value::as_str)
                    .unwrap_or("call")
                    .to_string(),
                output: message.get("content").cloned().unwrap_or(Value::Null),
            });
        }
        for call in message
            .get("tool_calls")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let function = call.get("function").unwrap_or(call);
            parts.push(Part::ToolCall {
                id: call
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("call")
                    .to_string(),
                name: function
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("tool")
                    .to_string(),
                arguments: parse_arguments(function.get("arguments")),
            });
        }
        request.messages.push(Message {
            role: role.to_string(),
            parts,
        });
    }
    request.tools = parse_openai_tools(body.get("tools"));
    Ok(request)
}

fn parse_gemini_request(body: Value) -> Result<CanonicalRequest, String> {
    let generation = body.get("generationConfig").unwrap_or(&Value::Null);
    let mut request = CanonicalRequest {
        system: text_value(
            body.get("systemInstruction")
                .and_then(|value| value.get("parts")),
        ),
        max_tokens: generation.get("maxOutputTokens").and_then(Value::as_u64),
        temperature: generation.get("temperature").and_then(Value::as_f64),
        ..CanonicalRequest::default()
    };
    for content in body
        .get("contents")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let role = match content.get("role").and_then(Value::as_str) {
            Some("model") => "assistant",
            _ => "user",
        };
        let mut parts = Vec::new();
        for part in content
            .get("parts")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if let Some(text) = part.get("text").and_then(Value::as_str) {
                parts.push(Part::Text(text.to_string()));
            }
            if let Some(call) = part.get("functionCall") {
                parts.push(Part::ToolCall {
                    id: call
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or("call")
                        .to_string(),
                    name: call
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or("tool")
                        .to_string(),
                    arguments: call.get("args").cloned().unwrap_or_else(|| json!({})),
                });
            }
            if let Some(result) = part.get("functionResponse") {
                parts.push(Part::ToolResult {
                    id: result
                        .get("id")
                        .or_else(|| result.get("name"))
                        .and_then(Value::as_str)
                        .unwrap_or("call")
                        .to_string(),
                    output: result.get("response").cloned().unwrap_or(Value::Null),
                });
            }
        }
        request.messages.push(Message {
            role: role.into(),
            parts,
        });
    }
    request.tools = body
        .get("tools")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .flat_map(|tool| {
            tool.get("functionDeclarations")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
        })
        .filter_map(|tool| {
            Some(Tool {
                name: tool.get("name")?.as_str()?.to_string(),
                description: tool
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                parameters: tool.get("parameters").cloned().unwrap_or_else(empty_schema),
            })
        })
        .collect();
    Ok(request)
}

fn render_request(request: &mut CanonicalRequest, format: ApiFormat, model: &str) -> Value {
    match format {
        ApiFormat::Anthropic => render_anthropic_request(request, model),
        ApiFormat::Responses => render_responses_request(request, model),
        ApiFormat::Chat => render_chat_request(request, model),
        ApiFormat::Gemini => render_gemini_request(request),
    }
}

fn render_anthropic_request(request: &CanonicalRequest, model: &str) -> Value {
    let mut body = json!({
        "model": model,
        "max_tokens": request.max_tokens.unwrap_or(8192),
        "messages": request.messages.iter().map(|message| json!({
            "role": message.role,
            "content": anthropic_parts(&message.parts),
        })).collect::<Vec<_>>(),
    });
    insert_optional(&mut body, "system", nonempty(&request.system));
    insert_optional(
        &mut body,
        "temperature",
        request.temperature.map(Value::from),
    );
    if !request.tools.is_empty() {
        body["tools"] = Value::Array(
            request
                .tools
                .iter()
                .map(|tool| {
                    json!({
                        "name": tool.name,
                        "description": tool.description,
                        "input_schema": tool.parameters,
                    })
                })
                .collect(),
        );
    }
    body
}

fn render_responses_request(request: &CanonicalRequest, model: &str) -> Value {
    let mut input = Vec::new();
    for message in &request.messages {
        let mut content = Vec::new();
        for part in &message.parts {
            match part {
                Part::Text(text) => content.push(json!({
                    "type": if message.role == "assistant" { "output_text" } else { "input_text" },
                    "text": text,
                })),
                Part::ToolCall {
                    id,
                    name,
                    arguments,
                } => input.push(json!({
                    "type": "function_call",
                    "call_id": id,
                    "name": name,
                    "arguments": arguments.to_string(),
                })),
                Part::ToolResult { id, output } => input.push(json!({
                    "type": "function_call_output",
                    "call_id": id,
                    "output": output,
                })),
            }
        }
        if !content.is_empty() {
            input.push(json!({ "role": message.role, "content": content }));
        }
    }
    let mut body = json!({ "model": model, "input": input });
    insert_optional(&mut body, "instructions", nonempty(&request.system));
    insert_optional(
        &mut body,
        "max_output_tokens",
        request.max_tokens.map(Value::from),
    );
    insert_optional(
        &mut body,
        "temperature",
        request.temperature.map(Value::from),
    );
    if !request.tools.is_empty() {
        body["tools"] = Value::Array(
            request
                .tools
                .iter()
                .map(|tool| {
                    json!({
                        "type": "function",
                        "name": tool.name,
                        "description": tool.description,
                        "parameters": tool.parameters,
                    })
                })
                .collect(),
        );
    }
    body
}

fn render_chat_request(request: &CanonicalRequest, model: &str) -> Value {
    let mut messages = Vec::new();
    if !request.system.is_empty() {
        messages.push(json!({ "role": "system", "content": request.system }));
    }
    for message in &request.messages {
        let texts = message
            .parts
            .iter()
            .filter_map(|part| match part {
                Part::Text(text) => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");
        let calls = message
            .parts
            .iter()
            .filter_map(|part| match part {
                Part::ToolCall {
                    id,
                    name,
                    arguments,
                } => Some(json!({
                    "id": id,
                    "type": "function",
                    "function": { "name": name, "arguments": arguments.to_string() },
                })),
                _ => None,
            })
            .collect::<Vec<_>>();
        if !texts.is_empty() || !calls.is_empty() {
            let mut rendered = json!({ "role": message.role, "content": texts });
            if !calls.is_empty() {
                rendered["tool_calls"] = Value::Array(calls);
            }
            messages.push(rendered);
        }
        for part in &message.parts {
            if let Part::ToolResult { id, output } = part {
                messages.push(json!({
                    "role": "tool",
                    "tool_call_id": id,
                    "content": text_value(Some(output)),
                }));
            }
        }
    }
    let mut body = json!({ "model": model, "messages": messages });
    insert_optional(&mut body, "max_tokens", request.max_tokens.map(Value::from));
    insert_optional(
        &mut body,
        "temperature",
        request.temperature.map(Value::from),
    );
    if !request.tools.is_empty() {
        body["tools"] = Value::Array(
            request
                .tools
                .iter()
                .map(|tool| {
                    json!({
                        "type": "function",
                        "function": {
                            "name": tool.name,
                            "description": tool.description,
                            "parameters": tool.parameters,
                        },
                    })
                })
                .collect(),
        );
    }
    body
}

fn render_gemini_request(request: &CanonicalRequest) -> Value {
    let contents = request
        .messages
        .iter()
        .map(|message| {
            let parts = message
                .parts
                .iter()
                .map(|part| match part {
                    Part::Text(text) => json!({ "text": text }),
                    Part::ToolCall {
                        id,
                        name,
                        arguments,
                    } => json!({
                        "functionCall": { "id": id, "name": name, "args": arguments }
                    }),
                    Part::ToolResult { id, output } => json!({
                        "functionResponse": { "id": id, "name": id, "response": output }
                    }),
                })
                .collect::<Vec<_>>();
            json!({
                "role": if message.role == "assistant" { "model" } else { "user" },
                "parts": parts,
            })
        })
        .collect::<Vec<_>>();
    let mut body = json!({ "contents": contents });
    if !request.system.is_empty() {
        body["systemInstruction"] = json!({ "parts": [{ "text": request.system }] });
    }
    let mut generation = Map::new();
    if let Some(value) = request.max_tokens {
        generation.insert("maxOutputTokens".into(), Value::from(value));
    }
    if let Some(value) = request.temperature {
        generation.insert("temperature".into(), Value::from(value));
    }
    if !generation.is_empty() {
        body["generationConfig"] = Value::Object(generation);
    }
    if !request.tools.is_empty() {
        body["tools"] = json!([{
            "functionDeclarations": request.tools.iter().map(|tool| json!({
                "name": tool.name,
                "description": tool.description,
                "parameters": tool.parameters,
            })).collect::<Vec<_>>()
        }]);
    }
    body
}

fn parse_response(body: Value, format: ApiFormat) -> Result<CanonicalResponse, String> {
    let response = match format {
        ApiFormat::Anthropic => CanonicalResponse {
            id: string_at(&body, "id"),
            model: string_at(&body, "model"),
            text: blocks_text(body.get("content")),
            tool_calls: anthropic_tool_calls(body.get("content")),
            input_tokens: body
                .pointer("/usage/input_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            output_tokens: body
                .pointer("/usage/output_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(0),
        },
        ApiFormat::Responses => CanonicalResponse {
            id: string_at(&body, "id"),
            model: string_at(&body, "model"),
            text: responses_text(body.get("output")),
            tool_calls: responses_tool_calls(body.get("output")),
            input_tokens: body
                .pointer("/usage/input_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            output_tokens: body
                .pointer("/usage/output_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(0),
        },
        ApiFormat::Chat => {
            let message = body.pointer("/choices/0/message").unwrap_or(&Value::Null);
            CanonicalResponse {
                id: string_at(&body, "id"),
                model: string_at(&body, "model"),
                text: text_value(message.get("content")),
                tool_calls: chat_tool_calls(message.get("tool_calls")),
                input_tokens: body
                    .pointer("/usage/prompt_tokens")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
                output_tokens: body
                    .pointer("/usage/completion_tokens")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
            }
        }
        ApiFormat::Gemini => {
            let parts = body.pointer("/candidates/0/content/parts");
            CanonicalResponse {
                id: String::new(),
                model: string_at(&body, "modelVersion"),
                text: blocks_text(parts),
                tool_calls: gemini_tool_calls(parts),
                input_tokens: body
                    .pointer("/usageMetadata/promptTokenCount")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
                output_tokens: body
                    .pointer("/usageMetadata/candidatesTokenCount")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
            }
        }
    };
    Ok(response)
}

fn render_response(response: &CanonicalResponse, format: ApiFormat) -> Value {
    let id = if response.id.is_empty() {
        "multara-proxy"
    } else {
        &response.id
    };
    match format {
        ApiFormat::Anthropic => {
            let mut content = vec![json!({ "type": "text", "text": response.text })];
            content.extend(
                response
                    .tool_calls
                    .iter()
                    .map(|(call_id, name, arguments)| {
                        json!({
                            "type": "tool_use", "id": call_id, "name": name, "input": arguments
                        })
                    }),
            );
            json!({
                "id": id,
                "type": "message",
                "role": "assistant",
                "model": response.model,
                "content": content,
                "stop_reason": if response.tool_calls.is_empty() { "end_turn" } else { "tool_use" },
                "usage": { "input_tokens": response.input_tokens, "output_tokens": response.output_tokens }
            })
        }
        ApiFormat::Responses => {
            let mut output = vec![json!({
                "id": format!("{id}-message"),
                "type": "message",
                "status": "completed",
                "role": "assistant",
                "content": [{ "type": "output_text", "text": response.text, "annotations": [] }]
            })];
            output.extend(
                response
                    .tool_calls
                    .iter()
                    .map(|(call_id, name, arguments)| {
                        json!({
                            "type": "function_call",
                            "id": call_id,
                            "call_id": call_id,
                            "name": name,
                            "arguments": arguments.to_string(),
                            "status": "completed"
                        })
                    }),
            );
            json!({
                "id": id,
                "object": "response",
                "status": "completed",
                "model": response.model,
                "output": output,
                "usage": { "input_tokens": response.input_tokens, "output_tokens": response.output_tokens, "total_tokens": response.input_tokens + response.output_tokens }
            })
        }
        ApiFormat::Chat => {
            let calls = response
                .tool_calls
                .iter()
                .map(|(call_id, name, arguments)| {
                    json!({
                        "id": call_id,
                        "type": "function",
                        "function": { "name": name, "arguments": arguments.to_string() }
                    })
                })
                .collect::<Vec<_>>();
            json!({
                "id": id,
                "object": "chat.completion",
                "model": response.model,
                "choices": [{ "index": 0, "message": { "role": "assistant", "content": response.text, "tool_calls": calls }, "finish_reason": if calls.is_empty() { "stop" } else { "tool_calls" } }],
                "usage": { "prompt_tokens": response.input_tokens, "completion_tokens": response.output_tokens, "total_tokens": response.input_tokens + response.output_tokens }
            })
        }
        ApiFormat::Gemini => {
            let mut parts = vec![json!({ "text": response.text })];
            parts.extend(
                response
                    .tool_calls
                    .iter()
                    .map(|(call_id, name, arguments)| {
                        json!({
                            "functionCall": { "id": call_id, "name": name, "args": arguments }
                        })
                    }),
            );
            json!({
                "candidates": [{ "content": { "role": "model", "parts": parts }, "finishReason": "STOP" }],
                "usageMetadata": { "promptTokenCount": response.input_tokens, "candidatesTokenCount": response.output_tokens, "totalTokenCount": response.input_tokens + response.output_tokens },
                "modelVersion": response.model
            })
        }
    }
}

fn anthropic_sse(response: &CanonicalResponse) -> String {
    let message = render_response(response, ApiFormat::Anthropic);
    let start = json!({
        "type": "message_start",
        "message": {
            "id": message["id"], "type": "message", "role": "assistant",
            "model": message["model"], "content": [],
            "stop_reason": Value::Null,
            "usage": { "input_tokens": response.input_tokens, "output_tokens": 0 }
        }
    });
    let mut events = vec![
        sse("message_start", &start),
        sse(
            "content_block_start",
            &json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}),
        ),
        sse(
            "content_block_delta",
            &json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":response.text}}),
        ),
        sse(
            "content_block_stop",
            &json!({"type":"content_block_stop","index":0}),
        ),
    ];
    for (index, (id, name, arguments)) in response.tool_calls.iter().enumerate() {
        let block = index + 1;
        events.push(sse("content_block_start", &json!({"type":"content_block_start","index":block,"content_block":{"type":"tool_use","id":id,"name":name,"input":{}}})));
        events.push(sse("content_block_delta", &json!({"type":"content_block_delta","index":block,"delta":{"type":"input_json_delta","partial_json":arguments.to_string()}})));
        events.push(sse(
            "content_block_stop",
            &json!({"type":"content_block_stop","index":block}),
        ));
    }
    events.push(sse("message_delta", &json!({"type":"message_delta","delta":{"stop_reason":if response.tool_calls.is_empty(){"end_turn"}else{"tool_use"}},"usage":{"output_tokens":response.output_tokens}})));
    events.push(sse("message_stop", &json!({"type":"message_stop"})));
    events.concat()
}

fn responses_sse(response: &CanonicalResponse) -> String {
    let rendered = render_response(response, ApiFormat::Responses);
    let message = json!({
        "id": format!("{}-message", rendered["id"].as_str().unwrap_or("multara-proxy")),
        "type": "message",
        "status": "in_progress",
        "role": "assistant",
        "content": []
    });
    let part = json!({ "type": "output_text", "text": "", "annotations": [] });
    let mut events = vec![
        sse(
            "response.created",
            &json!({"type":"response.created","response":{"id":rendered["id"],"object":"response","status":"in_progress","model":rendered["model"],"output":[]}}),
        ),
        sse(
            "response.output_item.added",
            &json!({"type":"response.output_item.added","output_index":0,"item":message}),
        ),
        sse(
            "response.content_part.added",
            &json!({"type":"response.content_part.added","output_index":0,"item_id":format!("{}-message", rendered["id"].as_str().unwrap_or("multara-proxy")),"content_index":0,"part":part}),
        ),
        sse(
            "response.output_text.delta",
            &json!({"type":"response.output_text.delta","output_index":0,"content_index":0,"delta":response.text}),
        ),
        sse(
            "response.output_text.done",
            &json!({"type":"response.output_text.done","output_index":0,"content_index":0,"text":response.text}),
        ),
        sse(
            "response.content_part.done",
            &json!({"type":"response.content_part.done","output_index":0,"content_index":0,"part":{"type":"output_text","text":response.text,"annotations":[]}}),
        ),
        sse(
            "response.output_item.done",
            &json!({"type":"response.output_item.done","output_index":0,"item":rendered["output"][0]}),
        ),
    ];
    for (index, item) in rendered["output"]
        .as_array()
        .into_iter()
        .flatten()
        .skip(1)
        .enumerate()
    {
        let output_index = index + 1;
        events.push(sse("response.output_item.added", &json!({"type":"response.output_item.added","output_index":output_index,"item":{"id":item["id"],"type":"function_call","call_id":item["call_id"],"name":item["name"],"arguments":"","status":"in_progress"}})));
        events.push(sse("response.function_call_arguments.delta", &json!({"type":"response.function_call_arguments.delta","output_index":output_index,"item_id":item["id"],"delta":item["arguments"]})));
        events.push(sse("response.function_call_arguments.done", &json!({"type":"response.function_call_arguments.done","output_index":output_index,"item_id":item["id"],"arguments":item["arguments"]})));
        events.push(sse(
            "response.output_item.done",
            &json!({"type":"response.output_item.done","output_index":output_index,"item":item}),
        ));
    }
    events.push(sse(
        "response.completed",
        &json!({"type":"response.completed","response":rendered}),
    ));
    events.push("data: [DONE]\n\n".to_string());
    events.concat()
}

fn chat_sse(response: &CanonicalResponse) -> String {
    let id = if response.id.is_empty() {
        "multara-proxy"
    } else {
        &response.id
    };
    let calls = response
        .tool_calls
        .iter()
        .map(|(call_id, name, arguments)| {
            json!({
                "index": 0,
                "id": call_id,
                "type": "function",
                "function": { "name": name, "arguments": arguments.to_string() }
            })
        })
        .collect::<Vec<_>>();
    let chunk = json!({
        "id": id, "object": "chat.completion.chunk", "model": response.model,
        "choices": [{"index":0,"delta":{"role":"assistant","content":response.text,"tool_calls":calls},"finish_reason":Value::Null}]
    });
    let stop = json!({
        "id": id, "object": "chat.completion.chunk", "model": response.model,
        "choices": [{"index":0,"delta":{},"finish_reason":if calls.is_empty(){"stop"}else{"tool_calls"}}]
    });
    format!("data: {chunk}\n\ndata: {stop}\n\ndata: [DONE]\n\n")
}

fn gemini_sse(response: &CanonicalResponse) -> String {
    let rendered = render_response(response, ApiFormat::Gemini);
    format!("data: {rendered}\n\n")
}

fn sse(event: &str, value: &Value) -> String {
    format!("event: {event}\ndata: {value}\n\n")
}

fn parse_anthropic_parts(content: Option<&Value>) -> Vec<Part> {
    if let Some(text) = content.and_then(Value::as_str) {
        return vec![Part::Text(text.to_string())];
    }
    content
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|block| match block.get("type").and_then(Value::as_str) {
            Some("text") => block
                .get("text")
                .and_then(Value::as_str)
                .map(|text| Part::Text(text.to_string())),
            Some("tool_use") => Some(Part::ToolCall {
                id: string_at(block, "id"),
                name: string_at(block, "name"),
                arguments: block.get("input").cloned().unwrap_or_else(|| json!({})),
            }),
            Some("tool_result") => Some(Part::ToolResult {
                id: string_at(block, "tool_use_id"),
                output: block.get("content").cloned().unwrap_or(Value::Null),
            }),
            _ => None,
        })
        .collect()
}

fn parse_openai_parts(content: Option<&Value>) -> Vec<Part> {
    if let Some(text) = content.and_then(Value::as_str) {
        return vec![Part::Text(text.to_string())];
    }
    content
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|block| {
            block
                .get("text")
                .and_then(Value::as_str)
                .map(|text| Part::Text(text.to_string()))
        })
        .collect()
}

fn parse_openai_tools(value: Option<&Value>) -> Vec<Tool> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let function = entry.get("function").unwrap_or(entry);
            Some(Tool {
                name: function.get("name")?.as_str()?.to_string(),
                description: function
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                parameters: function
                    .get("parameters")
                    .cloned()
                    .unwrap_or_else(empty_schema),
            })
        })
        .collect()
}

fn anthropic_parts(parts: &[Part]) -> Vec<Value> {
    parts
        .iter()
        .map(|part| match part {
            Part::Text(text) => json!({ "type": "text", "text": text }),
            Part::ToolCall {
                id,
                name,
                arguments,
            } => json!({ "type": "tool_use", "id": id, "name": name, "input": arguments }),
            Part::ToolResult { id, output } => {
                json!({ "type": "tool_result", "tool_use_id": id, "content": output })
            }
        })
        .collect()
}

fn anthropic_tool_calls(content: Option<&Value>) -> Vec<(String, String, Value)> {
    content
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|block| {
            (block.get("type").and_then(Value::as_str) == Some("tool_use")).then(|| {
                (
                    string_at(block, "id"),
                    string_at(block, "name"),
                    block.get("input").cloned().unwrap_or_else(|| json!({})),
                )
            })
        })
        .collect()
}

fn responses_tool_calls(output: Option<&Value>) -> Vec<(String, String, Value)> {
    output
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| {
            (item.get("type").and_then(Value::as_str) == Some("function_call")).then(|| {
                (
                    item.get("call_id")
                        .or_else(|| item.get("id"))
                        .and_then(Value::as_str)
                        .unwrap_or("call")
                        .to_string(),
                    string_at(item, "name"),
                    parse_arguments(item.get("arguments")),
                )
            })
        })
        .collect()
}

fn chat_tool_calls(calls: Option<&Value>) -> Vec<(String, String, Value)> {
    calls
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|call| {
            let function = call.get("function").unwrap_or(call);
            (
                string_at(call, "id"),
                string_at(function, "name"),
                parse_arguments(function.get("arguments")),
            )
        })
        .collect()
}

fn gemini_tool_calls(parts: Option<&Value>) -> Vec<(String, String, Value)> {
    parts
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|part| {
            let call = part.get("functionCall")?;
            Some((
                call.get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("call")
                    .to_string(),
                string_at(call, "name"),
                call.get("args").cloned().unwrap_or_else(|| json!({})),
            ))
        })
        .collect()
}

fn responses_text(output: Option<&Value>) -> String {
    output
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| {
            (item.get("type").and_then(Value::as_str) == Some("message"))
                .then(|| blocks_text(item.get("content")))
        })
        .collect::<Vec<_>>()
        .join("")
}

fn blocks_text(value: Option<&Value>) -> String {
    if let Some(text) = value.and_then(Value::as_str) {
        return text.to_string();
    }
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|block| {
            block
                .get("text")
                .and_then(Value::as_str)
                .or_else(|| block.get("output_text").and_then(Value::as_str))
        })
        .collect::<Vec<_>>()
        .join("")
}

fn text_value(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(_)) => blocks_text(value),
        Some(Value::Object(object)) => {
            text_value(object.get("parts").or_else(|| object.get("content")))
        }
        Some(value) if !value.is_null() => value.to_string(),
        _ => String::new(),
    }
}

fn parse_arguments(value: Option<&Value>) -> Value {
    match value {
        Some(Value::String(text)) => {
            serde_json::from_str(text).unwrap_or_else(|_| json!({ "value": text }))
        }
        Some(value) => value.clone(),
        None => json!({}),
    }
}

fn empty_schema() -> Value {
    json!({ "type": "object", "properties": {} })
}

fn nonempty(value: &str) -> Option<Value> {
    (!value.is_empty()).then(|| Value::String(value.to_string()))
}

fn insert_optional(body: &mut Value, key: &str, value: Option<Value>) {
    if let (Some(object), Some(value)) = (body.as_object_mut(), value) {
        object.insert(key.to_string(), value);
    }
}

fn string_at(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_anthropic_tools_to_responses() {
        let body = json!({
            "model": "claude-sonnet-5",
            "max_tokens": 100,
            "messages": [{"role":"user","content":[{"type":"text","text":"read"},{"type":"tool_result","tool_use_id":"call-1","content":"ok"}]}],
            "tools": [{"name":"read_file","description":"Read","input_schema":{"type":"object"}}],
            "stream": true
        });
        let (mapped, stream) = rewrite_request(
            body,
            ApiFormat::Anthropic,
            ApiFormat::Responses,
            "gpt-6.1-sol",
        )
        .unwrap();
        assert!(stream);
        assert_eq!(mapped["model"], "gpt-6.1-sol");
        assert_eq!(mapped["stream"], false);
        assert_eq!(mapped["tools"][0]["name"], "read_file");
    }

    #[test]
    fn converts_responses_output_to_anthropic_message() {
        let body = json!({
            "id":"resp-1",
            "model":"gpt-6.1-sol",
            "output":[{"type":"message","content":[{"type":"output_text","text":"ok"}]}],
            "usage":{"input_tokens":4,"output_tokens":1}
        });
        let (mapped, _) = rewrite_response(
            body,
            ApiFormat::Responses,
            ApiFormat::Anthropic,
            "gpt-6.1-sol",
        )
        .unwrap();
        assert_eq!(mapped["content"][0]["text"], "ok");
        assert_eq!(mapped["usage"]["input_tokens"], 4);
    }
}
