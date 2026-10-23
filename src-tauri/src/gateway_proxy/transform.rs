use serde_json::{json, Map, Value};

use super::{
    protocol::{clean_schema, is_reasoning_model, RESPONSES_MIN_OUTPUT_TOKENS},
    ApiFormat,
};

#[derive(Default)]
struct CanonicalRequest {
    system: String,
    messages: Vec<Message>,
    tools: Vec<Tool>,
    max_tokens: Option<u64>,
    temperature: Option<f64>,
    top_p: Option<f64>,
    stop: Option<Value>,
    tool_choice: Option<Value>,
    parallel_tool_calls: Option<bool>,
    reasoning_effort: Option<String>,
}

#[derive(Default)]
struct Message {
    role: String,
    parts: Vec<Part>,
}

enum Part {
    Text(String),
    Image(String),
    Document {
        url: String,
        filename: String,
    },
    Reasoning {
        text: String,
        encrypted: Option<String>,
    },
    ToolCall {
        id: String,
        name: String,
        arguments: Value,
        signature: Option<String>,
    },
    ToolResult {
        id: String,
        name: Option<String>,
        output: Value,
        is_error: bool,
    },
}

struct Tool {
    name: String,
    description: String,
    parameters: Value,
    kind: String,
    raw: Value,
}

#[derive(Default)]
pub(super) struct CanonicalResponse {
    id: String,
    model: String,
    text: String,
    tool_calls: Vec<(String, String, Value)>,
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
    cache_write_tokens: u64,
    stop_reason: String,
    refusal: String,
    reasoning_text: String,
    reasoning_encrypted: Option<String>,
    annotations: Vec<Value>,
}

pub(super) fn rewrite_request(
    body: Value,
    client: ApiFormat,
    upstream: ApiFormat,
    model: &str,
) -> Result<(Value, bool), String> {
    let wants_stream = body.get("stream").and_then(Value::as_bool).unwrap_or(false);
    if client == upstream {
        let mut body = body;
        if let Some(object) = body.as_object_mut() {
            object.insert("model".into(), Value::String(model.to_string()));
            object.insert("stream".into(), Value::Bool(false));
        }
        return Ok((body, wants_stream));
    }
    let body = if client == ApiFormat::Responses {
        flatten_responses_namespaces(body)
    } else {
        body
    };
    let mut request = parse_request(body, client)?;
    attach_tool_names(&mut request);
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
    let mut response = parse_response(body.clone(), upstream)?;
    if response.model.is_empty() {
        response.model = model.to_string();
    }
    let rendered = if upstream == client {
        body
    } else {
        render_response(&response, client)
    };
    Ok((rendered, response))
}

#[allow(dead_code)]
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
        top_p: body.get("top_p").and_then(Value::as_f64),
        stop: body.get("stop_sequences").cloned(),
        tool_choice: body.get("tool_choice").cloned(),
        parallel_tool_calls: body
            .pointer("/tool_choice/disable_parallel_tool_use")
            .and_then(Value::as_bool)
            .map(|disabled| !disabled),
        reasoning_effort: anthropic_reasoning_effort(&body).map(str::to_string),
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
                kind: tool
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or("function")
                    .to_string(),
                raw: tool.clone(),
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
        top_p: body.get("top_p").and_then(Value::as_f64),
        tool_choice: body.get("tool_choice").cloned(),
        parallel_tool_calls: body.get("parallel_tool_calls").and_then(Value::as_bool),
        reasoning_effort: body
            .pointer("/reasoning/effort")
            .and_then(Value::as_str)
            .map(str::to_string),
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
                        signature: item
                            .get("encrypted_content")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                    }],
                }),
                Some("function_call_output") => request.messages.push(Message {
                    role: "user".into(),
                    parts: vec![responses_tool_result(item)],
                }),
                Some("input_image") | Some("input_file") | Some("reasoning") => {
                    request.messages.push(Message {
                        role: "user".into(),
                        parts: parse_openai_parts(Some(&Value::Array(vec![item.clone()]))),
                    })
                }
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
        top_p: body.get("top_p").and_then(Value::as_f64),
        stop: body.get("stop").cloned(),
        tool_choice: body.get("tool_choice").cloned(),
        parallel_tool_calls: body.get("parallel_tool_calls").and_then(Value::as_bool),
        reasoning_effort: body
            .get("reasoning_effort")
            .and_then(Value::as_str)
            .map(str::to_string),
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
            if !request.system.is_empty() {
                request.system.push_str("\n\n");
            }
            request.system.push_str(&text_value(message.get("content")));
            continue;
        }
        let mut parts = if role == "tool" {
            Vec::new()
        } else {
            parse_openai_parts(message.get("content"))
        };
        if role == "tool" {
            parts.push(Part::ToolResult {
                id: message
                    .get("tool_call_id")
                    .and_then(Value::as_str)
                    .unwrap_or("call")
                    .to_string(),
                name: None,
                output: message.get("content").cloned().unwrap_or(Value::Null),
                is_error: false,
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
                signature: call
                    .get("thoughtSignature")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            });
        }
        request.messages.push(Message {
            role: if role == "tool" { "user" } else { role }.to_string(),
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
        top_p: generation.get("topP").and_then(Value::as_f64),
        stop: generation.get("stopSequences").cloned(),
        tool_choice: gemini_tool_choice(body.get("toolConfig")),
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
            if let Some(data) = part.get("inlineData").or_else(|| part.get("inline_data")) {
                let mime = data
                    .get("mimeType")
                    .or_else(|| data.get("mime_type"))
                    .and_then(Value::as_str)
                    .unwrap_or("application/octet-stream");
                let url = format!(
                    "data:{mime};base64,{}",
                    data.get("data").and_then(Value::as_str).unwrap_or_default()
                );
                if mime.starts_with("image/") {
                    parts.push(Part::Image(url));
                } else {
                    parts.push(Part::Document {
                        url,
                        filename: "document".into(),
                    });
                }
            }
            if let Some(file) = part.get("fileData").or_else(|| part.get("file_data")) {
                let url = file
                    .get("fileUri")
                    .or_else(|| file.get("file_uri"))
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                if !url.is_empty() {
                    parts.push(Part::Document {
                        url,
                        filename: "document".into(),
                    });
                }
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
                    signature: call
                        .get("thoughtSignature")
                        .and_then(Value::as_str)
                        .map(str::to_string),
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
                    name: result
                        .get("name")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    output: result.get("response").cloned().unwrap_or(Value::Null),
                    is_error: false,
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
                kind: "function".into(),
                raw: tool.clone(),
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
    insert_optional(&mut body, "top_p", request.top_p.map(Value::from));
    insert_optional(&mut body, "stop_sequences", request.stop.clone());
    insert_optional(
        &mut body,
        "tool_choice",
        request.tool_choice.as_ref().map(tool_choice_to_anthropic),
    );
    if let Some(effort) = request.reasoning_effort.as_deref() {
        body["output_config"] = json!({ "effort": effort });
        body["thinking"] = json!({ "type": "adaptive" });
    }
    if !request.tools.is_empty() {
        body["tools"] = Value::Array(request.tools.iter().map(render_anthropic_tool).collect());
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
                Part::Image(image_url) if message.role != "assistant" => content.push(json!({
                    "type": "input_image",
                    "image_url": image_url,
                })),
                Part::Image(_) => {}
                Part::Document { url, filename } if message.role != "assistant" => {
                    content.push(responses_file_part(url, filename))
                }
                Part::Document { .. } => {}
                Part::Reasoning { text, encrypted } => {
                    flush_responses_content(&mut input, &message.role, &mut content);
                    input.push(json!({
                        "type": "reasoning",
                        "summary": if text.is_empty() { Vec::<Value>::new() } else { vec![json!({"type":"summary_text","text":text})] },
                        "encrypted_content": encrypted,
                    }));
                }
                Part::ToolCall {
                    id,
                    name,
                    arguments,
                    ..
                } => {
                    flush_responses_content(&mut input, &message.role, &mut content);
                    input.push(json!({
                        "type": "function_call",
                        "call_id": id,
                        "name": name,
                        "arguments": arguments.to_string(),
                    }));
                }
                Part::ToolResult {
                    id,
                    output,
                    is_error,
                    ..
                } => {
                    flush_responses_content(&mut input, &message.role, &mut content);
                    input.push(json!({
                        "type": "function_call_output",
                        "call_id": id,
                        "output": responses_tool_output(output, *is_error),
                    }));
                }
            }
        }
        flush_responses_content(&mut input, &message.role, &mut content);
    }
    let mut body = json!({ "model": model, "input": input });
    insert_optional(&mut body, "instructions", nonempty(&request.system));
    insert_optional(
        &mut body,
        "max_output_tokens",
        request
            .max_tokens
            .map(|value| Value::from(value.max(RESPONSES_MIN_OUTPUT_TOKENS))),
    );
    insert_optional(
        &mut body,
        "temperature",
        request.temperature.map(Value::from),
    );
    insert_optional(&mut body, "top_p", request.top_p.map(Value::from));
    insert_optional(
        &mut body,
        "tool_choice",
        request.tool_choice.as_ref().map(tool_choice_to_responses),
    );
    insert_optional(
        &mut body,
        "parallel_tool_calls",
        request.parallel_tool_calls.map(Value::Bool),
    );
    if let Some(effort) = request.reasoning_effort.as_deref() {
        body["reasoning"] = json!({ "effort": effort });
    }
    if !request.tools.is_empty() {
        body["tools"] = Value::Array(request.tools.iter().map(render_responses_tool).collect());
    }
    body
}

fn flush_responses_content(input: &mut Vec<Value>, role: &str, content: &mut Vec<Value>) {
    if !content.is_empty() {
        input.push(json!({ "role": role, "content": std::mem::take(content) }));
    }
}

fn chat_content_parts(parts: &[Part]) -> Vec<Value> {
    parts
        .iter()
        .filter_map(|part| match part {
            Part::Text(text) => Some(json!({ "type": "text", "text": text })),
            Part::Image(url) => Some(json!({
                "type": "image_url",
                "image_url": { "url": url }
            })),
            Part::Document { url, filename } => Some(chat_file_part(url, filename)),
            _ => None,
        })
        .collect()
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
        let images = message
            .parts
            .iter()
            .filter_map(|part| match part {
                Part::Image(image_url) => Some(json!({
                    "type": "image_url",
                    "image_url": { "url": image_url }
                })),
                _ => None,
            })
            .collect::<Vec<_>>();
        let documents = message
            .parts
            .iter()
            .filter_map(|part| match part {
                Part::Document { url, filename } => Some(chat_file_part(url, filename)),
                _ => None,
            })
            .collect::<Vec<_>>();
        let reasoning = message
            .parts
            .iter()
            .filter_map(|part| match part {
                Part::Reasoning { text, .. } if !text.is_empty() => Some(text.as_str()),
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
                    ..
                } => Some(json!({
                    "id": id,
                    "type": "function",
                    "function": { "name": name, "arguments": arguments.to_string() },
                })),
                _ => None,
            })
            .collect::<Vec<_>>();
        if !texts.is_empty() || !images.is_empty() || !documents.is_empty() || !calls.is_empty() {
            let content = if images.is_empty() && documents.is_empty() {
                Value::String(texts)
            } else {
                Value::Array(chat_content_parts(&message.parts))
            };
            let mut rendered = json!({ "role": message.role, "content": content });
            if !calls.is_empty() {
                rendered["tool_calls"] = Value::Array(calls);
            }
            if !reasoning.is_empty() {
                rendered["reasoning_content"] = Value::String(reasoning);
            }
            messages.push(rendered);
        }
        for part in &message.parts {
            if let Part::ToolResult { id, output, .. } = part {
                messages.push(json!({
                    "role": "tool",
                    "tool_call_id": id,
                    "content": text_value(Some(output)),
                }));
            }
        }
    }
    let mut body = json!({ "model": model, "messages": messages });
    insert_optional(
        &mut body,
        if is_reasoning_model(model) {
            "max_completion_tokens"
        } else {
            "max_tokens"
        },
        request.max_tokens.map(Value::from),
    );
    insert_optional(
        &mut body,
        "temperature",
        request.temperature.map(Value::from),
    );
    insert_optional(&mut body, "top_p", request.top_p.map(Value::from));
    insert_optional(&mut body, "stop", request.stop.clone());
    insert_optional(
        &mut body,
        "tool_choice",
        request.tool_choice.as_ref().map(tool_choice_to_chat),
    );
    insert_optional(
        &mut body,
        "parallel_tool_calls",
        request.parallel_tool_calls.map(Value::Bool),
    );
    insert_optional(
        &mut body,
        "reasoning_effort",
        request
            .reasoning_effort
            .as_ref()
            .map(|value| Value::String(value.clone())),
    );
    if !request.tools.is_empty() {
        body["tools"] = Value::Array(
            request
                .tools
                .iter()
                .filter(|tool| tool.kind == "function")
                .map(|tool| {
                    json!({
                        "type": "function",
                        "function": {
                            "name": tool.name,
                            "description": tool.description,
                            "parameters": clean_schema(tool.parameters.clone()),
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
                    Part::Image(image_url) => gemini_image_part(image_url),
                    Part::Document { url, .. } => gemini_document_part(url),
                    Part::Reasoning { text, .. } => json!({ "text": text, "thought": true }),
                    Part::ToolCall {
                        id,
                        name,
                        arguments,
                        signature,
                    } => {
                        let mut call = json!({ "id": id, "name": name, "args": arguments });
                        if let Some(signature) = signature {
                            call["thoughtSignature"] = Value::String(signature.clone());
                        }
                        json!({ "functionCall": call })
                    }
                    Part::ToolResult {
                        id, name, output, ..
                    } => json!({
                        "functionResponse": {
                            "id": id,
                            "name": name.as_deref().unwrap_or(id),
                            "response": normalize_gemini_tool_response(output)
                        }
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
    if let Some(value) = request.top_p {
        generation.insert("topP".into(), Value::from(value));
    }
    if let Some(value) = request.stop.clone() {
        generation.insert("stopSequences".into(), value);
    }
    if !generation.is_empty() {
        body["generationConfig"] = Value::Object(generation);
    }
    if !request.tools.is_empty() {
        body["tools"] = json!([{
            "functionDeclarations": request.tools.iter().filter(|tool| tool.kind == "function").map(|tool| json!({
                // Gemini only accepts function declarations.
                "name": tool.name,
                "description": tool.description,
                "parameters": clean_schema(tool.parameters.clone()),
            })).collect::<Vec<_>>()
        }]);
    }
    if let Some(choice) = request.tool_choice.as_ref().and_then(tool_choice_to_gemini) {
        body["toolConfig"] = choice;
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
            input_tokens: responses_fresh_input(&body),
            output_tokens: body
                .pointer("/usage/output_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            cache_read_tokens: body
                .pointer("/usage/cache_read_input_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            cache_write_tokens: body
                .pointer("/usage/cache_creation_input_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            stop_reason: string_at(&body, "stop_reason"),
            refusal: anthropic_refusal(&body),
            reasoning_text: anthropic_reasoning_text(body.get("content")),
            reasoning_encrypted: anthropic_reasoning_signature(body.get("content")),
            annotations: Vec::new(),
        },
        ApiFormat::Responses => CanonicalResponse {
            id: string_at(&body, "id"),
            model: string_at(&body, "model"),
            text: responses_text_with_citations(body.get("output")),
            tool_calls: responses_tool_calls(body.get("output")),
            input_tokens: body
                .pointer("/usage/input_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            output_tokens: body
                .pointer("/usage/output_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            cache_read_tokens: body
                .pointer("/usage/input_tokens_details/cached_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            cache_write_tokens: body
                .pointer("/usage/input_tokens_details/cache_write_tokens")
                .or_else(|| body.pointer("/usage/cache_creation_input_tokens"))
                .and_then(Value::as_u64)
                .unwrap_or(0),
            stop_reason: responses_stop_reason(&body),
            refusal: responses_refusal(body.get("output")),
            reasoning_text: responses_reasoning_text(body.get("output")),
            reasoning_encrypted: responses_reasoning_encrypted(body.get("output")),
            annotations: responses_annotations(body.get("output")),
        },
        ApiFormat::Chat => {
            let message = body.pointer("/choices/0/message").unwrap_or(&Value::Null);
            CanonicalResponse {
                id: string_at(&body, "id"),
                model: string_at(&body, "model"),
                text: text_value(message.get("content")),
                tool_calls: chat_tool_calls(message.get("tool_calls")),
                input_tokens: chat_fresh_input(&body),
                output_tokens: body
                    .pointer("/usage/completion_tokens")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
                cache_read_tokens: body
                    .pointer("/usage/prompt_tokens_details/cached_tokens")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
                cache_write_tokens: body
                    .pointer("/usage/prompt_tokens_details/cache_write_tokens")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
                stop_reason: message
                    .get("finish_reason")
                    .or_else(|| body.pointer("/choices/0/finish_reason"))
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                refusal: message
                    .get("refusal")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                reasoning_text: message
                    .get("reasoning_content")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                reasoning_encrypted: None,
                annotations: message
                    .get("annotations")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default(),
            }
        }
        ApiFormat::Gemini => {
            let parts = body.pointer("/candidates/0/content/parts");
            CanonicalResponse {
                id: String::new(),
                model: string_at(&body, "modelVersion"),
                text: blocks_text(parts),
                tool_calls: gemini_tool_calls(parts),
                input_tokens: gemini_fresh_input(&body),
                output_tokens: body
                    .pointer("/usageMetadata/candidatesTokenCount")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
                cache_read_tokens: body
                    .pointer("/usageMetadata/cachedContentTokenCount")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
                cache_write_tokens: 0,
                stop_reason: body
                    .pointer("/candidates/0/finishReason")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                refusal: body
                    .pointer("/promptFeedback/blockReason")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                reasoning_text: gemini_reasoning_text(parts),
                reasoning_encrypted: None,
                annotations: Vec::new(),
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
            let mut content = Vec::new();
            if !response.reasoning_text.is_empty() {
                let mut reasoning =
                    json!({ "type": "thinking", "thinking": response.reasoning_text });
                if let Some(signature) = &response.reasoning_encrypted {
                    reasoning["signature"] = Value::String(signature.clone());
                }
                content.push(reasoning);
            }
            let visible_text = if response.text.is_empty() {
                &response.refusal
            } else {
                &response.text
            };
            if !visible_text.is_empty() {
                content.push(json!({ "type": "text", "text": visible_text }));
            }
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
                "stop_reason": anthropic_stop_reason(response),
                "usage": {
                    "input_tokens": response.input_tokens,
                    "output_tokens": response.output_tokens,
                    "cache_read_input_tokens": response.cache_read_tokens,
                    "cache_creation_input_tokens": response.cache_write_tokens
                }
            })
        }
        ApiFormat::Responses => {
            let mut output = Vec::new();
            if !response.reasoning_text.is_empty() || response.reasoning_encrypted.is_some() {
                output.push(json!({
                    "id": format!("{id}-reasoning"),
                    "type": "reasoning",
                    "summary": if response.reasoning_text.is_empty() { Vec::<Value>::new() } else { vec![json!({"type":"summary_text","text":response.reasoning_text})] },
                    "encrypted_content": response.reasoning_encrypted,
                }));
            }
            let content_part = if response.refusal.is_empty() {
                json!({ "type": "output_text", "text": response.text, "annotations": response.annotations })
            } else {
                json!({ "type": "refusal", "refusal": response.refusal })
            };
            output.push(json!({
                "id": format!("{id}-message"),
                "type": "message",
                "status": "completed",
                "role": "assistant",
                "content": [content_part]
            }));
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
            let mut rendered = json!({
                "id": id,
                "object": "response",
                "status": responses_status(response),
                "model": response.model,
                "output": output,
                "usage": {
                    "input_tokens": response.input_tokens + response.cache_read_tokens + response.cache_write_tokens,
                    "output_tokens": response.output_tokens,
                    "total_tokens": response.input_tokens + response.cache_read_tokens + response.cache_write_tokens + response.output_tokens,
                    "input_tokens_details": {
                        "cached_tokens": response.cache_read_tokens,
                        "cache_write_tokens": response.cache_write_tokens
                    }
                }
            });
            if responses_status(response) == "incomplete" {
                rendered["incomplete_details"] = json!({
                    "reason": if response.refusal.is_empty() {
                        "max_output_tokens"
                    } else {
                        "content_filter"
                    }
                });
            }
            rendered
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
                "choices": [{ "index": 0, "message": {
                    "role": "assistant",
                    "content": response.text,
                    "refusal": if response.refusal.is_empty() { Value::Null } else { Value::String(response.refusal.clone()) },
                    "reasoning_content": if response.reasoning_text.is_empty() { Value::Null } else { Value::String(response.reasoning_text.clone()) },
                    "annotations": response.annotations,
                    "tool_calls": calls
                }, "finish_reason": chat_finish_reason(response) }],
                "usage": {
                    "prompt_tokens": response.input_tokens + response.cache_read_tokens + response.cache_write_tokens,
                    "completion_tokens": response.output_tokens,
                    "total_tokens": response.input_tokens + response.cache_read_tokens + response.cache_write_tokens + response.output_tokens,
                    "prompt_tokens_details": {
                        "cached_tokens": response.cache_read_tokens,
                        "cache_write_tokens": response.cache_write_tokens
                    }
                }
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
                "candidates": [{ "content": { "role": "model", "parts": parts }, "finishReason": gemini_finish_reason(response) }],
                "usageMetadata": {
                    "promptTokenCount": response.input_tokens + response.cache_read_tokens,
                    "candidatesTokenCount": response.output_tokens,
                    "cachedContentTokenCount": response.cache_read_tokens,
                    "totalTokenCount": response.input_tokens + response.cache_read_tokens + response.output_tokens
                },
                "modelVersion": response.model
            })
        }
    }
}

#[allow(dead_code)]
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

#[allow(dead_code)]
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

#[allow(dead_code)]
fn chat_sse(response: &CanonicalResponse) -> String {
    let id = if response.id.is_empty() {
        "multara-proxy"
    } else {
        &response.id
    };
    let calls = response
        .tool_calls
        .iter()
        .enumerate()
        .map(|(index, (call_id, name, arguments))| {
            json!({
                "index": index,
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

#[allow(dead_code)]
fn gemini_sse(response: &CanonicalResponse) -> String {
    let rendered = render_response(response, ApiFormat::Gemini);
    format!("data: {rendered}\n\n")
}

#[allow(dead_code)]
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
            Some("image") => block
                .get("source")
                .and_then(image_url_from_anthropic_source)
                .map(Part::Image),
            Some("document") => block
                .get("source")
                .and_then(image_url_from_anthropic_source)
                .map(|url| Part::Document {
                    url,
                    filename: block
                        .get("title")
                        .or_else(|| block.get("filename"))
                        .and_then(Value::as_str)
                        .unwrap_or("document.pdf")
                        .to_string(),
                }),
            Some("thinking") | Some("redacted_thinking") => Some(Part::Reasoning {
                text: block
                    .get("thinking")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                encrypted: block
                    .get("signature")
                    .or_else(|| block.get("data"))
                    .and_then(Value::as_str)
                    .map(str::to_string),
            }),
            Some("tool_use") => Some(Part::ToolCall {
                id: string_at(block, "id"),
                name: string_at(block, "name"),
                arguments: block.get("input").cloned().unwrap_or_else(|| json!({})),
                signature: None,
            }),
            Some("tool_result") => Some(Part::ToolResult {
                id: string_at(block, "tool_use_id"),
                name: None,
                output: block.get("content").cloned().unwrap_or(Value::Null),
                is_error: block
                    .get("is_error")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
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
        .filter_map(|block| match block.get("type").and_then(Value::as_str) {
            Some("text") | Some("input_text") | Some("output_text") => block
                .get("text")
                .and_then(Value::as_str)
                .map(|text| Part::Text(text.to_string())),
            Some("image") => block
                .get("source")
                .and_then(image_url_from_anthropic_source)
                .or_else(|| image_url_from_openai_block(block))
                .map(Part::Image),
            Some("input_image") | Some("image_url") => {
                image_url_from_openai_block(block).map(Part::Image)
            }
            Some("input_file") | Some("file") => openai_file_url(block).map(|url| Part::Document {
                url,
                filename: block
                    .get("filename")
                    .or_else(|| block.pointer("/file/filename"))
                    .and_then(Value::as_str)
                    .unwrap_or("document")
                    .to_string(),
            }),
            Some("reasoning") => Some(Part::Reasoning {
                text: block
                    .get("summary")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(|part| part.get("text").and_then(Value::as_str))
                    .collect::<Vec<_>>()
                    .join("\n"),
                encrypted: block
                    .get("encrypted_content")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            }),
            _ => None,
        })
        .collect()
}

fn parse_openai_tools(value: Option<&Value>) -> Vec<Tool> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let kind = entry
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("function");
            if kind.starts_with("web_search") {
                return Some(Tool {
                    name: "web_search".into(),
                    description: String::new(),
                    parameters: empty_schema(),
                    kind: kind.to_string(),
                    raw: entry.clone(),
                });
            }
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
                kind: kind.to_string(),
                raw: entry.clone(),
            })
        })
        .collect()
}

fn anthropic_parts(parts: &[Part]) -> Vec<Value> {
    parts
        .iter()
        .map(|part| match part {
            Part::Text(text) => json!({ "type": "text", "text": text }),
            Part::Image(image_url) => json!({
                "type": "image",
                "source": anthropic_image_source(image_url),
            }),
            Part::Document { url, filename } => json!({
                "type": "document",
                "title": filename,
                "source": anthropic_document_source(url),
            }),
            Part::Reasoning { text, encrypted } => {
                if let Some(signature) = encrypted {
                    json!({ "type": "thinking", "thinking": text, "signature": signature })
                } else {
                    json!({ "type": "thinking", "thinking": text })
                }
            }
            Part::ToolCall {
                id,
                name,
                arguments,
                ..
            } => json!({ "type": "tool_use", "id": id, "name": name, "input": arguments }),
            Part::ToolResult {
                id,
                output,
                is_error,
                ..
            } => {
                let mut block =
                    json!({ "type": "tool_result", "tool_use_id": id, "content": output });
                if *is_error {
                    block["is_error"] = Value::Bool(true);
                }
                block
            }
        })
        .collect()
}

fn image_url_from_anthropic_source(source: &Value) -> Option<String> {
    match source.get("type").and_then(Value::as_str) {
        Some("base64") => Some(format!(
            "data:{};base64,{}",
            source.get("media_type")?.as_str()?,
            source.get("data")?.as_str()?
        )),
        Some("url") => source
            .get("url")
            .and_then(Value::as_str)
            .map(str::to_string),
        _ => None,
    }
}

fn image_url_from_openai_block(block: &Value) -> Option<String> {
    block
        .get("image_url")
        .and_then(Value::as_str)
        .or_else(|| {
            block
                .get("image_url")
                .and_then(|image| image.get("url"))
                .and_then(Value::as_str)
        })
        .or_else(|| block.get("url").and_then(Value::as_str))
        .map(str::to_string)
}

fn anthropic_image_source(image_url: &str) -> Value {
    if let Some(data) = parse_base64_data_url(image_url) {
        return json!({
            "type": "base64",
            "media_type": data.0,
            "data": data.1,
        });
    }
    json!({ "type": "url", "url": image_url })
}

fn gemini_image_part(image_url: &str) -> Value {
    if let Some(data) = parse_base64_data_url(image_url) {
        return json!({ "inlineData": { "mimeType": data.0, "data": data.1 } });
    }
    json!({ "fileData": { "fileUri": image_url } })
}

fn parse_base64_data_url(value: &str) -> Option<(&str, &str)> {
    value.strip_prefix("data:")?.split_once(";base64,")
}

fn attach_tool_names(request: &mut CanonicalRequest) {
    let mut names = std::collections::HashMap::new();
    for message in &request.messages {
        for part in &message.parts {
            if let Part::ToolCall { id, name, .. } = part {
                names.insert(id.clone(), name.clone());
            }
        }
    }
    for message in &mut request.messages {
        for part in &mut message.parts {
            if let Part::ToolResult { id, name, .. } = part {
                if name.is_none() {
                    *name = names.get(id).cloned();
                }
            }
        }
    }
}

fn flatten_responses_namespaces(mut body: Value) -> Value {
    let Some(tools) = body.get("tools").and_then(Value::as_array) else {
        return body;
    };
    let mut flattened = Vec::new();
    for tool in tools {
        if tool.get("type").and_then(Value::as_str) != Some("namespace") {
            flattened.push(tool.clone());
            continue;
        }
        let namespace = tool.get("name").and_then(Value::as_str).unwrap_or_default();
        for child in tool
            .get("tools")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let mut child = child.clone();
            if let Some(name) = child.get("name").and_then(Value::as_str) {
                child["name"] = Value::String(format!("{namespace}__{name}"));
            }
            flattened.push(child);
        }
    }
    body["tools"] = Value::Array(flattened);
    if let Some(input) = body.get_mut("input").and_then(Value::as_array_mut) {
        for item in input {
            if item.get("type").and_then(Value::as_str) != Some("function_call") {
                continue;
            }
            let namespace = item
                .get("namespace")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if let Some(name) = item.get("name").and_then(Value::as_str) {
                if !namespace.is_empty() {
                    item["name"] = Value::String(format!("{namespace}__{name}"));
                }
            }
            item.as_object_mut()
                .map(|object| object.remove("namespace"));
        }
    }
    body
}

fn responses_tool_output(output: &Value, is_error: bool) -> Value {
    let mut parts = Vec::new();
    if is_error {
        parts.push(json!({ "type": "input_text", "text": "[multara:tool-result-error]" }));
    }
    match output {
        Value::String(text) if !is_error => return Value::String(text.clone()),
        Value::String(text) => parts.push(json!({ "type": "input_text", "text": text })),
        Value::Array(items) => {
            for item in items {
                match item.get("type").and_then(Value::as_str) {
                    Some("text" | "input_text" | "output_text") => {
                        if let Some(text) = item.get("text").and_then(Value::as_str) {
                            parts.push(json!({ "type": "input_text", "text": text }));
                        }
                    }
                    Some("image" | "input_image" | "image_url") => {
                        let url = item
                            .get("source")
                            .and_then(image_url_from_anthropic_source)
                            .or_else(|| image_url_from_openai_block(item));
                        if let Some(url) = url {
                            parts.push(json!({ "type": "input_image", "image_url": url }));
                        }
                    }
                    Some("document" | "input_file" | "file") => {
                        let url = item
                            .get("source")
                            .and_then(image_url_from_anthropic_source)
                            .or_else(|| openai_file_url(item));
                        if let Some(url) = url {
                            parts.push(responses_file_part(
                                &url,
                                item.get("title")
                                    .or_else(|| item.get("filename"))
                                    .and_then(Value::as_str)
                                    .unwrap_or("document"),
                            ));
                        }
                    }
                    _ => parts.push(json!({
                        "type": "input_text",
                        "text": item.to_string(),
                    })),
                }
            }
        }
        Value::Null => {}
        value => parts.push(json!({ "type": "input_text", "text": value.to_string() })),
    }
    Value::Array(parts)
}

fn responses_tool_result(item: &Value) -> Part {
    let id = item
        .get("call_id")
        .and_then(Value::as_str)
        .unwrap_or("call")
        .to_string();
    let mut output = item.get("output").cloned().unwrap_or(Value::Null);
    let mut is_error = false;
    if let Some(parts) = output.as_array_mut() {
        parts.retain(|part| {
            let marker = part.get("type").and_then(Value::as_str) == Some("input_text")
                && part.get("text").and_then(Value::as_str) == Some("[multara:tool-result-error]");
            is_error |= marker;
            !marker
        });
    }
    Part::ToolResult {
        id,
        name: None,
        output,
        is_error,
    }
}

fn normalize_gemini_tool_response(output: &Value) -> Value {
    match output {
        Value::Object(_) => output.clone(),
        Value::String(text) => json!({ "content": text }),
        value => json!({ "content": value }),
    }
}

fn openai_file_url(block: &Value) -> Option<String> {
    block
        .get("file_url")
        .or_else(|| block.get("file_data"))
        .or_else(|| block.pointer("/file/file_data"))
        .or_else(|| block.pointer("/file/file_id"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn responses_file_part(url: &str, filename: &str) -> Value {
    if url.starts_with("data:") {
        json!({ "type": "input_file", "file_data": url, "filename": filename })
    } else {
        json!({ "type": "input_file", "file_url": url, "filename": filename })
    }
}

fn chat_file_part(url: &str, filename: &str) -> Value {
    if url.starts_with("data:") {
        json!({ "type": "file", "file": { "file_data": url, "filename": filename } })
    } else {
        json!({ "type": "file", "file": { "file_id": url, "filename": filename } })
    }
}

fn anthropic_document_source(url: &str) -> Value {
    if let Some((media_type, data)) = parse_base64_data_url(url) {
        return json!({ "type": "base64", "media_type": media_type, "data": data });
    }
    json!({ "type": "url", "url": url })
}

fn gemini_document_part(url: &str) -> Value {
    if let Some((mime_type, data)) = parse_base64_data_url(url) {
        return json!({ "inlineData": { "mimeType": mime_type, "data": data } });
    }
    json!({ "fileData": { "fileUri": url } })
}

fn anthropic_reasoning_effort(body: &Value) -> Option<&str> {
    body.pointer("/output_config/effort")
        .and_then(Value::as_str)
        .or_else(
            || match body.pointer("/thinking/type").and_then(Value::as_str) {
                Some("adaptive") => Some("high"),
                Some("enabled") => match body
                    .pointer("/thinking/budget_tokens")
                    .and_then(Value::as_u64)
                {
                    Some(value) if value < 4_000 => Some("low"),
                    Some(value) if value < 16_000 => Some("medium"),
                    _ => Some("high"),
                },
                _ => None,
            },
        )
}

fn tool_choice_to_chat(value: &Value) -> Value {
    match value {
        Value::String(choice) if choice == "any" => json!("required"),
        Value::Object(choice) => match choice.get("type").and_then(Value::as_str) {
            Some("any") => json!("required"),
            Some("auto") => json!("auto"),
            Some("none") => json!("none"),
            Some("tool") => json!({
                "type": "function",
                "function": { "name": choice.get("name").and_then(Value::as_str).unwrap_or_default() }
            }),
            _ => value.clone(),
        },
        _ => value.clone(),
    }
}

fn tool_choice_to_responses(value: &Value) -> Value {
    match value {
        Value::String(choice) if choice == "any" => json!("required"),
        Value::Object(choice) => match choice.get("type").and_then(Value::as_str) {
            Some("any") => json!("required"),
            Some("tool") => json!({
                "type": "function",
                "name": choice.get("name").and_then(Value::as_str).unwrap_or_default()
            }),
            _ => value.clone(),
        },
        _ => value.clone(),
    }
}

fn tool_choice_to_anthropic(value: &Value) -> Value {
    match value {
        Value::String(choice) => match choice.as_str() {
            "required" | "any" => json!({ "type": "any" }),
            "none" => json!({ "type": "none" }),
            _ => json!({ "type": "auto" }),
        },
        Value::Object(choice) if choice.get("type").and_then(Value::as_str) == Some("function") => {
            json!({
                "type": "tool",
                "name": choice
                    .get("name")
                    .or_else(|| choice.get("function").and_then(|value| value.get("name")))
                    .and_then(Value::as_str)
                    .unwrap_or_default()
            })
        }
        _ => value.clone(),
    }
}

fn tool_choice_to_gemini(value: &Value) -> Option<Value> {
    let mode = match value {
        Value::String(choice) => match choice.as_str() {
            "none" => "NONE",
            "required" | "any" => "ANY",
            _ => "AUTO",
        },
        Value::Object(choice) => match choice.get("type").and_then(Value::as_str) {
            Some("none") => "NONE",
            Some("any" | "tool" | "function") => "ANY",
            _ => "AUTO",
        },
        _ => return None,
    };
    Some(json!({ "functionCallingConfig": { "mode": mode } }))
}

fn gemini_tool_choice(value: Option<&Value>) -> Option<Value> {
    let config = value?;
    let mode = config
        .pointer("/functionCallingConfig/mode")
        .or_else(|| config.pointer("/function_calling_config/mode"))
        .and_then(Value::as_str)?;
    Some(match mode {
        "NONE" => json!("none"),
        "ANY" => json!("required"),
        _ => json!("auto"),
    })
}

fn render_anthropic_tool(tool: &Tool) -> Value {
    if tool.kind.starts_with("web_search") {
        let mut value = tool.raw.clone();
        value["name"] = Value::String(tool.name.clone());
        return value;
    }
    json!({
        "name": tool.name,
        "description": tool.description,
        "input_schema": tool.parameters,
    })
}

fn render_responses_tool(tool: &Tool) -> Value {
    if tool.kind.starts_with("web_search") {
        let mut value = json!({ "type": "web_search" });
        if let Some(limit) = tool.raw.get("max_uses") {
            value["max_uses"] = limit.clone();
        }
        return value;
    }
    json!({
        "type": "function",
        "name": tool.name,
        "description": tool.description,
        "parameters": clean_schema(tool.parameters.clone()),
        "strict": false,
    })
}

fn anthropic_reasoning_text(content: Option<&Value>) -> String {
    content
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|block| {
            matches!(
                block.get("type").and_then(Value::as_str),
                Some("thinking" | "redacted_thinking")
            )
        })
        .filter_map(|block| block.get("thinking").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("")
}

fn anthropic_reasoning_signature(content: Option<&Value>) -> Option<String> {
    content
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find_map(|block| {
            block
                .get("signature")
                .or_else(|| block.get("data"))
                .and_then(Value::as_str)
                .map(str::to_string)
        })
}

fn anthropic_refusal(body: &Value) -> String {
    if body.get("stop_reason").and_then(Value::as_str) == Some("refusal") {
        blocks_text(body.get("content"))
    } else {
        String::new()
    }
}

fn responses_reasoning_items(output: Option<&Value>) -> impl Iterator<Item = &Value> {
    output
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|item| item.get("type").and_then(Value::as_str) == Some("reasoning"))
}

fn responses_reasoning_text(output: Option<&Value>) -> String {
    responses_reasoning_items(output)
        .flat_map(|item| {
            item.get("summary")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
        })
        .filter_map(|part| part.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("")
}

fn responses_reasoning_encrypted(output: Option<&Value>) -> Option<String> {
    responses_reasoning_items(output).find_map(|item| {
        item.get("encrypted_content")
            .and_then(Value::as_str)
            .map(str::to_string)
    })
}

fn responses_refusal(output: Option<&Value>) -> String {
    output
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| item.get("content").and_then(Value::as_array))
        .flatten()
        .filter_map(|part| part.get("refusal").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("")
}

fn responses_annotations(output: Option<&Value>) -> Vec<Value> {
    output
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| item.get("content").and_then(Value::as_array))
        .flatten()
        .filter_map(|part| part.get("annotations").and_then(Value::as_array))
        .flatten()
        .cloned()
        .collect()
}

fn responses_stop_reason(body: &Value) -> String {
    body.pointer("/incomplete_details/reason")
        .and_then(Value::as_str)
        .or_else(|| body.get("status").and_then(Value::as_str))
        .unwrap_or_default()
        .to_string()
}

fn gemini_reasoning_text(parts: Option<&Value>) -> String {
    parts
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|part| part.get("thought").and_then(Value::as_bool) == Some(true))
        .filter_map(|part| part.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("")
}

fn responses_fresh_input(body: &Value) -> u64 {
    body.pointer("/usage/input_tokens")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        .saturating_sub(
            body.pointer("/usage/input_tokens_details/cached_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(0),
        )
        .saturating_sub(
            body.pointer("/usage/input_tokens_details/cache_write_tokens")
                .or_else(|| body.pointer("/usage/cache_creation_input_tokens"))
                .and_then(Value::as_u64)
                .unwrap_or(0),
        )
}

fn chat_fresh_input(body: &Value) -> u64 {
    body.pointer("/usage/prompt_tokens")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        .saturating_sub(
            body.pointer("/usage/prompt_tokens_details/cached_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(0),
        )
        .saturating_sub(
            body.pointer("/usage/prompt_tokens_details/cache_write_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(0),
        )
}

fn gemini_fresh_input(body: &Value) -> u64 {
    body.pointer("/usageMetadata/promptTokenCount")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        .saturating_sub(
            body.pointer("/usageMetadata/cachedContentTokenCount")
                .and_then(Value::as_u64)
                .unwrap_or(0),
        )
}

fn anthropic_stop_reason(response: &CanonicalResponse) -> &'static str {
    if !response.refusal.is_empty()
        || matches!(
            response.stop_reason.as_str(),
            "content_filter" | "SAFETY" | "BLOCKLIST" | "PROHIBITED_CONTENT"
        )
    {
        "refusal"
    } else if matches!(
        response.stop_reason.as_str(),
        "length" | "max_output_tokens" | "MAX_TOKENS"
    ) {
        "max_tokens"
    } else if !response.tool_calls.is_empty()
        || matches!(
            response.stop_reason.as_str(),
            "tool_calls" | "function_call"
        )
    {
        "tool_use"
    } else {
        "end_turn"
    }
}

fn responses_status(response: &CanonicalResponse) -> &'static str {
    if !response.refusal.is_empty()
        || matches!(
            response.stop_reason.as_str(),
            "length" | "max_tokens" | "max_output_tokens" | "content_filter"
        )
    {
        "incomplete"
    } else {
        "completed"
    }
}

fn chat_finish_reason(response: &CanonicalResponse) -> &'static str {
    if !response.refusal.is_empty() {
        "content_filter"
    } else if matches!(
        response.stop_reason.as_str(),
        "max_tokens" | "max_output_tokens" | "MAX_TOKENS"
    ) {
        "length"
    } else if !response.tool_calls.is_empty() {
        "tool_calls"
    } else {
        "stop"
    }
}

fn gemini_finish_reason(response: &CanonicalResponse) -> &'static str {
    if !response.refusal.is_empty() {
        "SAFETY"
    } else if matches!(
        response.stop_reason.as_str(),
        "length" | "max_tokens" | "max_output_tokens"
    ) {
        "MAX_TOKENS"
    } else {
        "STOP"
    }
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

fn responses_text_with_citations(output: Option<&Value>) -> String {
    let text = responses_text(output);
    let citations = output
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|item| item.get("type").and_then(Value::as_str) == Some("web_search_call"))
        .flat_map(|item| {
            item.pointer("/action/sources")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
        })
        .filter_map(|source| {
            let url = source.get("url").and_then(Value::as_str)?;
            let title = source.get("title").and_then(Value::as_str).unwrap_or(url);
            Some(format!("[{title}]({url})"))
        })
        .collect::<Vec<_>>();
    if citations.is_empty() {
        text
    } else if text.is_empty() {
        format!("Sources: {}", citations.join(", "))
    } else {
        format!("{text}\n\nSources: {}", citations.join(", "))
    }
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
    fn converts_anthropic_image_to_responses_input_image() {
        let body = json!({
            "model": "claude-sonnet-5",
            "messages": [{
                "role": "user",
                "content": [
                    {"type":"text","text":"这是什么？"},
                    {
                        "type":"image",
                        "source":{"type":"base64","media_type":"image/png","data":"aW1hZ2U="}
                    }
                ]
            }]
        });
        let (mapped, _) = rewrite_request(
            body,
            ApiFormat::Anthropic,
            ApiFormat::Responses,
            "gpt-6.1-sol",
        )
        .unwrap();

        assert_eq!(mapped["input"][0]["content"][0]["type"], "input_text");
        assert_eq!(mapped["input"][0]["content"][1]["type"], "input_image");
        assert_eq!(
            mapped["input"][0]["content"][1]["image_url"],
            "data:image/png;base64,aW1hZ2U="
        );
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

    #[test]
    fn converts_chat_tool_message_to_anthropic_user_tool_result() {
        let body = json!({
            "messages": [
                {"role":"assistant","content":"","tool_calls":[{"id":"call-1","type":"function","function":{"name":"read","arguments":"{}"}}]},
                {"role":"tool","tool_call_id":"call-1","content":"done"}
            ]
        });
        let (mapped, _) =
            rewrite_request(body, ApiFormat::Chat, ApiFormat::Anthropic, "claude").unwrap();
        assert_eq!(mapped["messages"][1]["role"], "user");
        assert_eq!(mapped["messages"][1]["content"][0]["type"], "tool_result");
        assert_eq!(mapped["messages"][1]["content"][0]["content"], "done");
        assert_eq!(mapped["messages"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn preserves_document_and_tool_media_for_responses() {
        let body = json!({
            "messages": [{"role":"user","content":[
                {"type":"document","title":"manual.pdf","source":{"type":"base64","media_type":"application/pdf","data":"JVBERg=="}},
                {"type":"tool_result","tool_use_id":"call-1","is_error":true,"content":[
                    {"type":"image","source":{"type":"base64","media_type":"image/png","data":"aW1n"}}
                ]}
            ]}]
        });
        let (mapped, _) =
            rewrite_request(body, ApiFormat::Anthropic, ApiFormat::Responses, "gpt-5").unwrap();
        assert_eq!(mapped["input"][0]["content"][0]["type"], "input_file");
        assert_eq!(mapped["input"][1]["type"], "function_call_output");
        assert!(mapped["input"][1]["output"]
            .as_array()
            .unwrap()
            .iter()
            .any(|part| part["type"] == "input_image"));
    }

    #[test]
    fn maps_request_controls_and_clamps_responses_token_floor() {
        let body = json!({
            "max_tokens": 1,
            "top_p": 0.8,
            "tool_choice": {"type":"any"},
            "thinking": {"type":"enabled","budget_tokens":8000},
            "messages": [{"role":"user","content":"hi"}]
        });
        let (mapped, _) =
            rewrite_request(body, ApiFormat::Anthropic, ApiFormat::Responses, "gpt-5").unwrap();
        assert_eq!(mapped["max_output_tokens"], RESPONSES_MIN_OUTPUT_TOKENS);
        assert_eq!(mapped["top_p"], 0.8);
        assert_eq!(mapped["tool_choice"], "required");
        assert_eq!(mapped["reasoning"]["effort"], "medium");
    }

    #[test]
    fn parses_gemini_inline_data_and_preserves_cache_usage() {
        let request = json!({
            "contents": [{"role":"user","parts":[{"inlineData":{"mimeType":"image/png","data":"aW1n"}}]}]
        });
        let (mapped, _) =
            rewrite_request(request, ApiFormat::Gemini, ApiFormat::Anthropic, "claude").unwrap();
        assert_eq!(mapped["messages"][0]["content"][0]["type"], "image");

        let response = json!({
            "modelVersion":"gemini",
            "candidates":[{"content":{"parts":[{"text":"ok"}]},"finishReason":"STOP"}],
            "usageMetadata":{"promptTokenCount":10,"cachedContentTokenCount":6,"candidatesTokenCount":2}
        });
        let (mapped, _) =
            rewrite_response(response, ApiFormat::Gemini, ApiFormat::Anthropic, "gemini").unwrap();
        assert_eq!(mapped["usage"]["input_tokens"], 4);
        assert_eq!(mapped["usage"]["cache_read_input_tokens"], 6);
    }

    #[test]
    fn flattens_responses_namespace_tools() {
        let body = json!({
            "input": "hi",
            "tools": [{
                "type":"namespace",
                "name":"mcp",
                "tools":[{"type":"function","name":"read","parameters":{"type":"object"}}]
            }]
        });
        let (mapped, _) =
            rewrite_request(body, ApiFormat::Responses, ApiFormat::Chat, "model").unwrap();
        assert_eq!(mapped["tools"][0]["function"]["name"], "mcp__read");
    }
}
