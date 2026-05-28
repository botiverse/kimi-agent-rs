mod tool_test_utils;

use std::collections::{HashSet, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use tempfile::TempDir;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use kimi_agent::config::ModelCapability;
use kimi_agent::llm::LLM;
use kimi_agent::soul::agent::{Agent, Runtime};
use kimi_agent::soul::context::Context;
use kimi_agent::soul::kimisoul::KimiSoul;
use kimi_agent::soul::run_soul;
use kimi_agent::soul::toolset::KimiToolset;
use kimi_agent::utils::QueueShutDown;
use kimi_agent::wire::{UserInput, Wire};
use kosong::chat_provider::{
    ChatProvider, ChatProviderError, ChatProviderErrorKind, StreamedMessage, ThinkingEffort,
    TokenUsage,
};
use kosong::message::{ContentPart, Message, StreamedMessagePart, TextPart};
use kosong::tooling::Tool;

use tool_test_utils::RuntimeFixture;

#[derive(Clone)]
enum Attempt {
    RetryableError(RetryErrorKind),
    Success(Vec<StreamedMessagePart>),
}

#[derive(Clone)]
enum RetryErrorKind {
    Timeout,
    Connection,
    EmptyResponse,
}

struct SequenceStreamedMessage {
    parts: VecDeque<StreamedMessagePart>,
}

impl SequenceStreamedMessage {
    fn new(parts: Vec<StreamedMessagePart>) -> Self {
        Self { parts: parts.into() }
    }
}

#[async_trait]
impl StreamedMessage for SequenceStreamedMessage {
    async fn next_part(&mut self) -> Result<Option<StreamedMessagePart>, ChatProviderError> {
        Ok(self.parts.pop_front())
    }

    fn id(&self) -> Option<String> {
        Some("step-retry-test".to_string())
    }

    fn usage(&self) -> Option<TokenUsage> {
        None
    }
}

struct SequenceChatProvider {
    attempts: Vec<Attempt>,
    index: AtomicUsize,
}

impl SequenceChatProvider {
    fn new(attempts: Vec<Attempt>) -> Self {
        Self {
            attempts,
            index: AtomicUsize::new(0),
        }
    }
}

#[async_trait]
impl ChatProvider for SequenceChatProvider {
    fn name(&self) -> &str {
        "sequence"
    }

    fn model_name(&self) -> &str {
        "sequence"
    }

    fn thinking_effort(&self) -> Option<ThinkingEffort> {
        None
    }

    async fn generate(
        &self,
        _system_prompt: &str,
        _tools: &[Tool],
        _history: &[Message],
    ) -> Result<Box<dyn StreamedMessage>, ChatProviderError> {
        let idx = self.index.fetch_add(1, Ordering::SeqCst);
        let attempt = self.attempts.get(idx).cloned().unwrap_or_else(|| {
            Attempt::RetryableError(RetryErrorKind::EmptyResponse)
        });

        match attempt {
            Attempt::RetryableError(kind) => {
                let err_kind = match kind {
                    RetryErrorKind::Timeout => ChatProviderErrorKind::Timeout,
                    RetryErrorKind::Connection => ChatProviderErrorKind::Connection,
                    RetryErrorKind::EmptyResponse => ChatProviderErrorKind::EmptyResponse,
                };
                Err(ChatProviderError::new(err_kind, "retryable failure"))
            }
            Attempt::Success(parts) => Ok(Box::new(SequenceStreamedMessage::new(parts))),
        }
    }

    fn with_thinking(&self, _effort: ThinkingEffort) -> Box<dyn ChatProvider> {
        Box::new(SequenceChatProvider {
            attempts: self.attempts.clone(),
            index: AtomicUsize::new(self.index.load(Ordering::SeqCst)),
        })
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

fn make_llm(attempts: Vec<Attempt>, capabilities: HashSet<ModelCapability>) -> LLM {
    LLM {
        chat_provider: Box::new(SequenceChatProvider::new(attempts)),
        max_context_size: 100_000,
        capabilities,
        model_config: None,
        provider_config: None,
    }
}

fn runtime_with_llm(mut runtime: Runtime, llm: LLM) -> Runtime {
    runtime.llm = Some(Arc::new(llm));
    runtime
}

fn make_soul(runtime: Runtime, llm: LLM, tmp_path: &std::path::Path) -> KimiSoul {
    let agent = Agent {
        name: "StepRetry Test Agent".to_string(),
        system_prompt: "Test system prompt.".to_string(),
        toolset: Arc::new(tokio::sync::Mutex::new(KimiToolset::new())),
        runtime: runtime_with_llm(runtime, llm),
    };

    KimiSoul::new(agent, Context::new(tmp_path.join("history.jsonl")))
}

async fn run_and_collect_events(
    soul: &KimiSoul,
    user_input: UserInput,
) -> (Result<(), String>, Vec<String>) {
    let event_types = Arc::new(Mutex::new(Vec::new()));
    let event_types_clone = Arc::clone(&event_types);

    let ui_loop = move |wire: Arc<Wire>| {
        let events = Arc::clone(&event_types_clone);
        async move {
            let ui = wire.ui_side(true);
            loop {
                let msg = match ui.receive().await {
                    Ok(msg) => msg,
                    Err(QueueShutDown) => return Ok(()),
                };
                events.lock().await.push(msg.type_name().to_string());
            }
        }
    };

    let result = run_soul(soul, user_input, ui_loop, CancellationToken::new(), None)
        .await
        .map_err(|err| err.to_string());

    (result, event_types.lock().await.clone())
}

#[tokio::test]
async fn retryable_failure_emits_step_retry_then_recovers() {
    let fixture = RuntimeFixture::new();
    let mut runtime = fixture.runtime.clone();
    runtime.config.loop_control.max_retries_per_step = 2;

    let llm = make_llm(
        vec![
            Attempt::RetryableError(RetryErrorKind::Timeout),
            Attempt::Success(vec![StreamedMessagePart::Content(ContentPart::Text(
                TextPart::new("Recovered after retry"),
            ))]),
        ],
        HashSet::new(),
    );

    let tmp = TempDir::new().expect("temp dir");
    let soul = make_soul(runtime, llm, tmp.path());

    let (result, events) = run_and_collect_events(&soul, UserInput::Text("go".to_string())).await;

    assert!(result.is_ok(), "expected successful recovery, got: {result:?}");
    assert!(events.iter().any(|t| t == "StepBegin"));
    assert!(
        events.iter().any(|t| t == "StepRetry"),
        "expected StepRetry event in wire stream, got events: {events:?}"
    );
    assert_eq!(
        events.iter().filter(|t| t.as_str() == "StepInterrupted").count(),
        0,
        "did not expect StepInterrupted on recovered retry path; events={events:?}"
    );
}

#[tokio::test]
async fn exhausted_retry_emits_step_interrupted_terminal() {
    let fixture = RuntimeFixture::new();
    let mut runtime = fixture.runtime.clone();
    runtime.config.loop_control.max_retries_per_step = 2;

    let llm = make_llm(
        vec![
            Attempt::RetryableError(RetryErrorKind::Timeout),
            Attempt::RetryableError(RetryErrorKind::Connection),
        ],
        HashSet::new(),
    );

    let tmp = TempDir::new().expect("temp dir");
    let soul = make_soul(runtime, llm, tmp.path());

    let (result, events) = run_and_collect_events(&soul, UserInput::Text("go".to_string())).await;

    assert!(result.is_err(), "expected terminal failure");
    assert_eq!(
        events.iter().filter(|t| t.as_str() == "StepInterrupted").count(),
        1,
        "expected exactly one StepInterrupted on terminal retry failure; events={events:?}"
    );
    assert!(
        events.iter().any(|t| t == "StepRetry"),
        "expected StepRetry event before terminal failure; events={events:?}"
    );
}
