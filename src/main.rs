mod prompt_registry;
mod resource_registry;
mod tool_registry;

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use chrono::Datelike;
use context_server::{
    ComputedPrompt, ContextServer, ContextServerRpcRequest, ContextServerRpcResponse, Prompt,
    PromptContent, PromptExecutor, PromptMessage, PromptRole, Tool, ToolContent, ToolExecutor,
};
use indoc::formatdoc;
use serde_json::{json, Value};
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::{
    prompt_registry::PromptRegistry, resource_registry::ResourceRegistry,
    tool_registry::ToolRegistry,
};

struct ContextServerState {
    rpc: ContextServer,
}

impl ContextServerState {
    fn new() -> Result<Self> {
        let resource_registry = Arc::new(ResourceRegistry::default());

        let tool_registry = Arc::new(ToolRegistry::default());
        tool_registry.register(Arc::new(NowTool));

        let prompt_registry = Arc::new(PromptRegistry::default());
        prompt_registry.register(Arc::new(NowPrompt));

        Ok(Self {
            rpc: ContextServer::builder()
                .with_server_info((env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION")))
                .with_resources(resource_registry)
                .with_tools(tool_registry)
                .with_prompts(prompt_registry)
                .build()?,
        })
    }

    async fn process_request(
        &self,
        request: ContextServerRpcRequest,
    ) -> Result<Option<ContextServerRpcResponse>> {
        self.rpc.handle_incoming_message(request).await
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let state = ContextServerState::new()?;
    let mut stdin = BufReader::new(io::stdin()).lines();
    let mut stdout = io::stdout();

    while let Some(line) = stdin.next_line().await? {
        let request: ContextServerRpcRequest = match serde_json::from_str(&line) {
            Ok(req) => req,
            Err(e) => {
                eprintln!("Error parsing request: {}", e);
                continue;
            }
        };

        if let Some(response) = state.process_request(request).await? {
            let response_json = serde_json::to_string(&response)?;
            stdout.write_all(response_json.as_bytes()).await?;
            stdout.write_all(b"\n").await?;
            stdout.flush().await?;
        }
    }

    Ok(())
}

fn get_current_time_info() -> String {
    let local_now = chrono::Local::now();
    let week = local_now.iso_week().week();
    let day = local_now.format("%A").to_string();

    formatdoc! {"
        Current local time: {}
        Week of the year: {}
        Day of the week: {}
    ", local_now, week, day}
}

struct NowTool;

#[async_trait]
impl ToolExecutor for NowTool {
    async fn execute(&self, _arguments: Option<Value>) -> Result<Vec<ToolContent>> {
        let result = get_current_time_info();
        Ok(vec![ToolContent::Text { text: result }])
    }

    fn to_tool(&self) -> Tool {
        Tool {
            name: "now".into(),
            description: Some(
                "Retrieve the current local time, week of the year, and day of the week.".into(),
            ),
            input_schema: json!({
                "type": "object",
                "properties": {},
            }),
        }
    }
}

struct NowPrompt;

#[async_trait]
impl PromptExecutor for NowPrompt {
    fn name(&self) -> &str {
        "Now"
    }

    async fn compute(&self, _arguments: Option<Value>) -> Result<ComputedPrompt> {
        let content = get_current_time_info();

        Ok(ComputedPrompt {
            description: "Current time information".into(),
            messages: vec![PromptMessage {
                role: PromptRole::User,
                content: PromptContent::Text {
                    text: content.into(),
                },
            }],
        })
    }

    fn to_prompt(&self) -> Prompt {
        Prompt {
            name: self.name().to_string(),
            arguments: vec![],
        }
    }
}
