use std::{
    collections::VecDeque,
    io::{BufRead, BufReader, Read},
};

use serde_json::{json, Value};

use super::ApiFormat;

#[derive(Default)]
struct Delta {
    text: String,
    reasoning: String,
    tool_id: String,
    tool_name: String,
    tool_arguments: String,
    tool_start: bool,
    /// Chat 上游可能在每个参数碎片里重复带 `id`；只有它需要按 id/index 判断是否续写同一个调用。
    tool_continuation_check: bool,
    tool_upstream_index: Option<u64>,
    done: bool,
    input_tokens: u64,
    output_tokens: u64,
}

pub struct BridgeReader<R: Read> {
    source: BufReader<R>,
    upstream: ApiFormat,
    client: ApiFormat,
    pending: VecDeque<u8>,
    started: bool,
    text_open: bool,
    reasoning_open: bool,
    tool_open: bool,
    tool_index: usize,
    next_block_index: usize,
    text_block_index: usize,
    reasoning_block_index: usize,
    tool_block_index: usize,
    finished: bool,
    response_id: String,
    model: String,
    responses_text_started: bool,
    text_output_index: usize,
    text_accumulator: String,
    tool_id: String,
    tool_name: String,
    tool_arguments: String,
    tool_upstream_index: Option<u64>,
    completed_outputs: Vec<Value>,
}

impl<R: Read> BridgeReader<R> {
    pub fn new(source: R, upstream: ApiFormat, client: ApiFormat, model: String) -> Self {
        Self {
            source: BufReader::new(source),
            upstream,
            client,
            pending: VecDeque::new(),
            started: false,
            text_open: false,
            reasoning_open: false,
            tool_open: false,
            tool_index: 0,
            next_block_index: 0,
            text_block_index: 0,
            reasoning_block_index: 0,
            tool_block_index: 0,
            finished: false,
            response_id: format!("multara-{}", uuid::Uuid::new_v4()),
            model,
            responses_text_started: false,
            text_output_index: 0,
            text_accumulator: String::new(),
            tool_id: String::new(),
            tool_name: String::new(),
            tool_arguments: String::new(),
            tool_upstream_index: None,
            completed_outputs: Vec::new(),
        }
    }

    fn fill(&mut self) -> std::io::Result<()> {
        if self.finished {
            return Ok(());
        }
        let Some((event, data)) = read_event(&mut self.source)? else {
            self.finish(0, 0);
            return Ok(());
        };
        if data == "[DONE]" {
            self.finish(0, 0);
            return Ok(());
        }
        let Ok(value) = serde_json::from_str::<Value>(&data) else {
            return Ok(());
        };
        let delta = extract_delta(self.upstream, &event, &value);
        if !self.started {
            self.start();
        }
        self.emit(delta);
        Ok(())
    }

    fn push(&mut self, text: impl AsRef<str>) {
        self.pending.extend(text.as_ref().as_bytes());
    }

    fn start(&mut self) {
        self.started = true;
        match self.client {
            ApiFormat::Anthropic => self.push(format!(
                "event: message_start\ndata: {}\n\n",
                json!({
                    "type":"message_start",
                    "message":{"id":self.response_id,"type":"message","role":"assistant","model":self.model,"content":[],"stop_reason":Value::Null,"usage":{"input_tokens":0,"output_tokens":0}}
                })
            )),
            ApiFormat::Responses => self.push(format!(
                "event: response.created\ndata: {}\n\n",
                json!({"type":"response.created","response":{"id":self.response_id,"object":"response","status":"in_progress","model":self.model,"output":[]}})
            )),
            _ => {}
        }
    }

    fn emit(&mut self, delta: Delta) {
        if !delta.reasoning.is_empty() {
            self.emit_reasoning(&delta.reasoning);
        }
        if delta.tool_start && !self.continues_open_tool(&delta) {
            self.tool_upstream_index = delta.tool_upstream_index;
            self.emit_tool_start(&delta.tool_id, &delta.tool_name);
        }
        if !delta.tool_arguments.is_empty() {
            self.emit_tool_arguments(&delta.tool_arguments);
        }
        if !delta.text.is_empty() {
            self.emit_text(&delta.text);
        }
        if delta.done {
            self.finish(delta.input_tokens, delta.output_tokens);
        }
    }

    fn emit_text(&mut self, text: &str) {
        self.close_tool();
        match self.client {
            ApiFormat::Anthropic => {
                self.close_reasoning();
                if !self.text_open {
                    self.text_open = true;
                    self.text_block_index = self.next_block_index;
                    self.next_block_index += 1;
                    self.push(format!(
                        "event: content_block_start\ndata: {}\n\n",
                        json!({"type":"content_block_start","index":self.text_block_index,"content_block":{"type":"text","text":""}})
                    ));
                }
                self.push(format!(
                    "event: content_block_delta\ndata: {}\n\n",
                    json!({"type":"content_block_delta","index":self.text_block_index,"delta":{"type":"text_delta","text":text}})
                ));
            }
            ApiFormat::Responses => {
                if !self.responses_text_started {
                    self.responses_text_started = true;
                    self.text_output_index = self.completed_outputs.len();
                    self.push(format!(
                        "event: response.output_item.added\ndata: {}\n\nevent: response.content_part.added\ndata: {}\n\n",
                        json!({"type":"response.output_item.added","output_index":self.text_output_index,"item":{"id":format!("{}-message",self.response_id),"type":"message","status":"in_progress","role":"assistant","content":[]}}),
                        json!({"type":"response.content_part.added","output_index":self.text_output_index,"item_id":format!("{}-message",self.response_id),"content_index":0,"part":{"type":"output_text","text":"","annotations":[]}})
                    ));
                }
                self.text_accumulator.push_str(text);
                self.push(format!(
                    "event: response.output_text.delta\ndata: {}\n\n",
                    json!({"type":"response.output_text.delta","output_index":self.text_output_index,"content_index":0,"delta":text})
                ));
            }
            ApiFormat::Chat => self.push(format!(
                "data: {}\n\n",
                json!({"id":self.response_id,"object":"chat.completion.chunk","model":self.model,"choices":[{"index":0,"delta":{"content":text},"finish_reason":Value::Null}]})
            )),
            ApiFormat::Gemini => self.push(format!(
                "data: {}\n\n",
                json!({"candidates":[{"content":{"role":"model","parts":[{"text":text}]}}],"modelVersion":self.model})
            )),
        }
    }

    fn emit_reasoning(&mut self, text: &str) {
        match self.client {
            ApiFormat::Anthropic => {
                if !self.reasoning_open {
                    self.reasoning_open = true;
                    self.reasoning_block_index = self.next_block_index;
                    self.next_block_index += 1;
                    self.push(format!(
                        "event: content_block_start\ndata: {}\n\n",
                        json!({"type":"content_block_start","index":self.reasoning_block_index,"content_block":{"type":"thinking","thinking":""}})
                    ));
                }
                self.push(format!(
                    "event: content_block_delta\ndata: {}\n\n",
                    json!({"type":"content_block_delta","index":self.reasoning_block_index,"delta":{"type":"thinking_delta","thinking":text}})
                ));
            }
            ApiFormat::Responses => self.push(format!(
                "event: response.reasoning_summary_text.delta\ndata: {}\n\n",
                json!({"type":"response.reasoning_summary_text.delta","output_index":0,"summary_index":0,"delta":text})
            )),
            ApiFormat::Chat => self.push(format!(
                "data: {}\n\n",
                json!({"id":self.response_id,"object":"chat.completion.chunk","model":self.model,"choices":[{"index":0,"delta":{"reasoning_content":text},"finish_reason":Value::Null}]})
            )),
            ApiFormat::Gemini => self.push(format!(
                "data: {}\n\n",
                json!({"candidates":[{"content":{"role":"model","parts":[{"text":text,"thought":true}]}}],"modelVersion":self.model})
            )),
        }
    }

    /// 同一个调用的后续参数碎片（id 相同或为空、index 未变）只追加参数，不再开新 tool_use。
    fn continues_open_tool(&self, delta: &Delta) -> bool {
        if !delta.tool_continuation_check || !self.tool_open {
            return false;
        }
        let same_id = delta.tool_id.is_empty() || delta.tool_id == self.tool_id;
        let same_index = delta.tool_upstream_index == self.tool_upstream_index;
        same_id && same_index
    }

    fn emit_tool_start(&mut self, id: &str, name: &str) {
        self.close_reasoning();
        self.close_text();
        if self.tool_open {
            if self.client == ApiFormat::Anthropic {
                self.push(format!(
                    "event: content_block_stop\ndata: {}\n\n",
                    json!({"type":"content_block_stop","index":self.tool_block_index})
                ));
            }
            self.tool_index += 1;
        }
        self.tool_open = true;
        self.tool_id = id.to_string();
        self.tool_name = name.to_string();
        self.tool_arguments.clear();
        self.tool_block_index = self.next_block_index;
        self.next_block_index += 1;
        match self.client {
            ApiFormat::Anthropic => self.push(format!(
                "event: content_block_start\ndata: {}\n\n",
                json!({"type":"content_block_start","index":self.tool_block_index,"content_block":{"type":"tool_use","id":id,"name":name,"input":{}}})
            )),
            ApiFormat::Responses => self.push(format!(
                "event: response.output_item.added\ndata: {}\n\n",
                json!({"type":"response.output_item.added","output_index":self.tool_index + usize::from(self.responses_text_started),"item":{"id":id,"type":"function_call","call_id":id,"name":name,"arguments":"","status":"in_progress"}})
            )),
            ApiFormat::Chat => self.push(format!(
                "data: {}\n\n",
                json!({"id":self.response_id,"object":"chat.completion.chunk","model":self.model,"choices":[{"index":0,"delta":{"tool_calls":[{"index":self.tool_index,"id":id,"type":"function","function":{"name":name,"arguments":""}}]},"finish_reason":Value::Null}]})
            )),
            ApiFormat::Gemini => {}
        }
    }

    fn emit_tool_arguments(&mut self, arguments: &str) {
        self.tool_arguments.push_str(arguments);
        match self.client {
            ApiFormat::Anthropic => self.push(format!(
                "event: content_block_delta\ndata: {}\n\n",
                json!({"type":"content_block_delta","index":self.tool_block_index,"delta":{"type":"input_json_delta","partial_json":arguments}})
            )),
            ApiFormat::Responses => self.push(format!(
                "event: response.function_call_arguments.delta\ndata: {}\n\n",
                json!({"type":"response.function_call_arguments.delta","output_index":self.tool_index + usize::from(self.responses_text_started),"delta":arguments})
            )),
            ApiFormat::Chat => self.push(format!(
                "data: {}\n\n",
                json!({"id":self.response_id,"object":"chat.completion.chunk","model":self.model,"choices":[{"index":0,"delta":{"tool_calls":[{"index":self.tool_index,"function":{"arguments":arguments}}]},"finish_reason":Value::Null}]})
            )),
            ApiFormat::Gemini => {}
        }
    }

    fn close_text(&mut self) {
        if self.client == ApiFormat::Anthropic && self.text_open {
            self.push(format!(
                "event: content_block_stop\ndata: {}\n\n",
                json!({"type":"content_block_stop","index":self.text_block_index})
            ));
            self.text_open = false;
        }
    }

    fn close_reasoning(&mut self) {
        if self.client == ApiFormat::Anthropic && self.reasoning_open {
            self.push(format!(
                "event: content_block_stop\ndata: {}\n\n",
                json!({"type":"content_block_stop","index":self.reasoning_block_index})
            ));
            self.reasoning_open = false;
        }
    }

    fn close_tool(&mut self) {
        if self.client == ApiFormat::Anthropic && self.tool_open {
            self.push(format!(
                "event: content_block_stop\ndata: {}\n\n",
                json!({"type":"content_block_stop","index":self.tool_block_index})
            ));
        }
        if self.tool_open {
            if self.client == ApiFormat::Responses {
                let item = json!({
                    "id":self.tool_id,
                    "type":"function_call",
                    "call_id":self.tool_id,
                    "name":self.tool_name,
                    "arguments":self.tool_arguments,
                    "status":"completed"
                });
                self.push(format!(
                    "event: response.function_call_arguments.done\ndata: {}\n\nevent: response.output_item.done\ndata: {}\n\n",
                    json!({"type":"response.function_call_arguments.done","output_index":self.tool_index + usize::from(self.responses_text_started),"item_id":self.tool_id,"arguments":self.tool_arguments}),
                    json!({"type":"response.output_item.done","output_index":self.tool_index + usize::from(self.responses_text_started),"item":item})
                ));
                self.completed_outputs.push(item);
            }
            self.tool_open = false;
            self.tool_index += 1;
        }
    }

    fn finish(&mut self, input_tokens: u64, output_tokens: u64) {
        if self.finished {
            return;
        }
        if !self.started {
            self.start();
        }
        self.close_reasoning();
        self.close_text();
        let ended_with_tool = self.tool_open;
        self.close_tool();
        if self.client == ApiFormat::Responses && self.responses_text_started {
            let message = json!({
                "id":format!("{}-message",self.response_id),
                "type":"message",
                "status":"completed",
                "role":"assistant",
                "content":[{"type":"output_text","text":self.text_accumulator,"annotations":[]}]
            });
            self.push(format!(
                "event: response.output_text.done\ndata: {}\n\nevent: response.content_part.done\ndata: {}\n\nevent: response.output_item.done\ndata: {}\n\n",
                json!({"type":"response.output_text.done","output_index":self.text_output_index,"content_index":0,"text":self.text_accumulator}),
                json!({"type":"response.content_part.done","output_index":self.text_output_index,"content_index":0,"part":{"type":"output_text","text":self.text_accumulator,"annotations":[]}}),
                json!({"type":"response.output_item.done","output_index":self.text_output_index,"item":message})
            ));
            self.completed_outputs
                .insert(self.text_output_index, message);
        }
        match self.client {
            ApiFormat::Anthropic => self.push(format!(
                "event: message_delta\ndata: {}\n\nevent: message_stop\ndata: {{\"type\":\"message_stop\"}}\n\n",
                json!({"type":"message_delta","delta":{"stop_reason":if ended_with_tool{"tool_use"}else{"end_turn"}},"usage":{"output_tokens":output_tokens}})
            )),
            ApiFormat::Responses => self.push(format!(
                "event: response.completed\ndata: {}\n\ndata: [DONE]\n\n",
                json!({"type":"response.completed","response":{"id":self.response_id,"object":"response","status":"completed","model":self.model,"output":self.completed_outputs,"usage":{"input_tokens":input_tokens,"output_tokens":output_tokens,"total_tokens":input_tokens + output_tokens}}})
            )),
            ApiFormat::Chat => self.push(format!(
                "data: {}\n\ndata: [DONE]\n\n",
                json!({"id":self.response_id,"object":"chat.completion.chunk","model":self.model,"choices":[{"index":0,"delta":{},"finish_reason":if ended_with_tool{"tool_calls"}else{"stop"}}],"usage":{"prompt_tokens":input_tokens,"completion_tokens":output_tokens,"total_tokens":input_tokens + output_tokens}})
            )),
            ApiFormat::Gemini => {}
        }
        self.finished = true;
    }
}

impl<R: Read> Read for BridgeReader<R> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        while self.pending.is_empty() && !self.finished {
            self.fill()?;
        }
        let count = buffer.len().min(self.pending.len());
        for target in &mut buffer[..count] {
            *target = self.pending.pop_front().unwrap_or_default();
        }
        Ok(count)
    }
}

/// 单个事件（含多行 data 拼接）的上限，上游不换行地一直写也不会把内存吃光。
const MAX_EVENT_BYTES: usize = 16 * 1024 * 1024;

fn read_event<R: BufRead>(reader: &mut R) -> std::io::Result<Option<(String, String)>> {
    let mut event = String::new();
    let mut data = String::new();
    loop {
        let mut line = String::new();
        let read = Read::take(&mut *reader, (MAX_EVENT_BYTES + 1) as u64).read_line(&mut line)?;
        if line.len() + data.len() > MAX_EVENT_BYTES {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                tr!("上游事件过大"),
            ));
        }
        if read == 0 {
            return if data.is_empty() {
                Ok(None)
            } else {
                Ok(Some((event, data)))
            };
        }
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            if !data.is_empty() {
                return Ok(Some((event, data)));
            }
            continue;
        }
        if let Some(value) = line.strip_prefix("event:") {
            event = value.trim().to_string();
        } else if let Some(value) = line.strip_prefix("data:") {
            if !data.is_empty() {
                data.push('\n');
            }
            data.push_str(value.trim());
        } else if line.starts_with('{') {
            data.push_str(line);
            return Ok(Some((event, data)));
        }
    }
}

fn extract_delta(format: ApiFormat, event: &str, value: &Value) -> Delta {
    match format {
        ApiFormat::Anthropic => Delta {
            text: value
                .pointer("/delta/text")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            reasoning: value
                .pointer("/delta/thinking")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            tool_id: value
                .pointer("/content_block/id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            tool_name: value
                .pointer("/content_block/name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            tool_arguments: value
                .pointer("/delta/partial_json")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            tool_start: event == "content_block_start"
                && value.pointer("/content_block/type").and_then(Value::as_str) == Some("tool_use"),
            tool_continuation_check: false,
            tool_upstream_index: None,
            done: event == "message_stop",
            input_tokens: value
                .pointer("/message/usage/input_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            output_tokens: value
                .pointer("/usage/output_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(0),
        },
        ApiFormat::Responses => Delta {
            text: value
                .get("delta")
                .and_then(Value::as_str)
                .filter(|_| event == "response.output_text.delta")
                .unwrap_or_default()
                .to_string(),
            reasoning: value
                .get("delta")
                .and_then(Value::as_str)
                .filter(|_| event.contains("reasoning") || event.contains("summary_text"))
                .unwrap_or_default()
                .to_string(),
            tool_id: value
                .pointer("/item/call_id")
                .or_else(|| value.pointer("/item/id"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            tool_name: value
                .pointer("/item/name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            tool_arguments: value
                .get("delta")
                .and_then(Value::as_str)
                .filter(|_| event == "response.function_call_arguments.delta")
                .unwrap_or_default()
                .to_string(),
            tool_start: event == "response.output_item.added"
                && value.pointer("/item/type").and_then(Value::as_str) == Some("function_call"),
            tool_continuation_check: false,
            tool_upstream_index: None,
            done: event == "response.completed" || event == "response.failed",
            input_tokens: value
                .pointer("/response/usage/input_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            output_tokens: value
                .pointer("/response/usage/output_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(0),
        },
        ApiFormat::Chat => {
            let tool = value
                .pointer("/choices/0/delta/tool_calls/0")
                .unwrap_or(&Value::Null);
            Delta {
                text: value
                    .pointer("/choices/0/delta/content")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                reasoning: value
                    .pointer("/choices/0/delta/reasoning_content")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                tool_id: tool
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                tool_name: tool
                    .pointer("/function/name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                tool_arguments: tool
                    .pointer("/function/arguments")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                tool_start: tool
                    .get("id")
                    .is_some_and(|id| id.as_str().is_some_and(|id| !id.is_empty())),
                tool_continuation_check: true,
                tool_upstream_index: tool.get("index").and_then(Value::as_u64),
                done: value
                    .pointer("/choices/0/finish_reason")
                    .is_some_and(|value| !value.is_null()),
                input_tokens: value
                    .pointer("/usage/prompt_tokens")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
                output_tokens: value
                    .pointer("/usage/completion_tokens")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
            }
        }
        ApiFormat::Gemini => {
            let part = value
                .pointer("/candidates/0/content/parts/0")
                .unwrap_or(&Value::Null);
            let call = part.get("functionCall").unwrap_or(&Value::Null);
            Delta {
                text: part
                    .get("text")
                    .and_then(Value::as_str)
                    .filter(|_| part.get("thought").and_then(Value::as_bool) != Some(true))
                    .unwrap_or_default()
                    .to_string(),
                reasoning: part
                    .get("text")
                    .and_then(Value::as_str)
                    .filter(|_| part.get("thought").and_then(Value::as_bool) == Some(true))
                    .unwrap_or_default()
                    .to_string(),
                tool_id: call
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("call")
                    .to_string(),
                tool_name: call
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                tool_arguments: call.get("args").map(Value::to_string).unwrap_or_default(),
                tool_start: call.is_object(),
                tool_continuation_check: false,
                tool_upstream_index: None,
                done: value
                    .pointer("/candidates/0/finishReason")
                    .and_then(Value::as_str)
                    .is_some(),
                input_tokens: value
                    .pointer("/usageMetadata/promptTokenCount")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
                output_tokens: value
                    .pointer("/usageMetadata/candidatesTokenCount")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Read};

    use super::*;

    #[test]
    fn converts_chat_text_stream_to_anthropic_events() {
        let source = concat!(
            "data: {\"choices\":[{\"delta\":{\"content\":\"ok\"},\"finish_reason\":null}]}\n\n",
            "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":2,\"completion_tokens\":1}}\n\n",
            "data: [DONE]\n\n"
        );
        let mut bridge = BridgeReader::new(
            Cursor::new(source),
            ApiFormat::Chat,
            ApiFormat::Anthropic,
            "model".into(),
        );
        let mut output = String::new();
        bridge.read_to_string(&mut output).unwrap();
        assert!(output.contains("text_delta"));
        assert!(output.contains("\"ok\""));
        assert!(output.contains("event: message_stop"));
    }

    #[test]
    fn repeated_chat_tool_id_continues_one_call() {
        let source = concat!(
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"c1\",\"function\":{\"name\":\"Bash\",\"arguments\":\"{\\\"com\"}}]},\"finish_reason\":null}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"c1\",\"function\":{\"name\":\"\",\"arguments\":\"mand\\\":\\\"pwd\\\"}\"}}]},\"finish_reason\":null}]}\n\n",
            "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\n"
        );
        let mut bridge = BridgeReader::new(
            Cursor::new(source),
            ApiFormat::Chat,
            ApiFormat::Anthropic,
            "model".into(),
        );
        let mut output = String::new();
        bridge.read_to_string(&mut output).unwrap();
        assert_eq!(output.matches("event: content_block_start").count(), 1);
        assert_eq!(output.matches("input_json_delta").count(), 2);
    }

    #[test]
    fn keeps_parallel_chat_tool_indexes() {
        let source = concat!(
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"c1\",\"function\":{\"name\":\"one\",\"arguments\":\"{}\"}}]},\"finish_reason\":null}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":1,\"id\":\"c2\",\"function\":{\"name\":\"two\",\"arguments\":\"{}\"}}]},\"finish_reason\":null}]}\n\n",
            "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\n"
        );
        let mut bridge = BridgeReader::new(
            Cursor::new(source),
            ApiFormat::Chat,
            ApiFormat::Chat,
            "model".into(),
        );
        let mut output = String::new();
        bridge.read_to_string(&mut output).unwrap();
        assert!(output.contains("\"index\":0"));
        assert!(output.contains("\"index\":1"));
    }
}
