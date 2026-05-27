mod tool_test_utils;

use std::collections::{HashSet, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use kosong::chat_provider::{ChatProvider, ChatProviderError, ChatProviderErrorKind, StreamedMessage, ThinkingEffort, TokenUsage};
use kosong::message::{ContentPart, Message, Role, StreamedMessagePart, TextPart};
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

use kimi_agent::llm::is_auth_error;
use kimi_agent::soul::agent::{Agent, Runtime};
use kimi_agent::soul::context::Context;
use kimi_agent::soul::kimisoul::KimiSoul;
use kimi_agent::soul::run_soul;
use kimi_agent::soul::toolset::KimiToolset;
use kimi_agent::utils::QueueShutDown;
use kimi_agent::wire::{UserInput, Wire, WireMessage};

use tool_test_utils::RuntimeFixture;

struct SequenceStreamedMessage {
    parts: VecDeque<StreamedMessagePart>,
}

impl SequenceStreamedMessage {
    fn new(parts: Vec<StreamedMessagePart>) -> Self {
        Self {
            parts: parts.into(),
        }
    }
}

#[async_trait]
impl StreamedMessage for SequenceStreamedMessage {
    async fn next_part(&mut self) -> Result<Option<StreamedMessagePart>, ChatProviderError> {
        Ok(self.parts.pop_front())
    }

    fn id(&self) -> Option<String> {
        Some("sequence".to_string())
    }

    fn usage(&self) -> Option<TokenUsage> {
        None
    }
}

struct AuthThenSuccessChatProvider {
    sequences: Vec<Vec<StreamedMessagePart>>,
    index: AtomicUsize,
    fail_first: AtomicUsize,
}

impl AuthThenSuccessChatProvider {
    fn new(sequences: Vec<Vec<StreamedMessagePart>>, fail_first_n: usize) -> Self {
        Self {
            sequences,
            index: AtomicUsize::new(0),
            fail_first: AtomicUsize::new(fail_first_n),
        }
    }
}

#[async_trait]
impl ChatProvider for AuthThenSuccessChatProvider {
    fn name(&self) -> &str {
        "auth_then_success"
    }

    fn model_name(&self) -> &str {
        "auth_then_success"
    }

    fn thinking_effort(&self) -> Option<ThinkingEffort> {
        None
    }

    async fn generate(
        &self,
        _system_prompt: &str,
        _tools: &[kosong::tooling::Tool],
        _history: &[Message],
    ) -> Result<Box<dyn StreamedMessage>, ChatProviderError> {
        let remaining = self.fail_first.load(Ordering::SeqCst);
        if remaining > 0 {
            self.fail_first.fetch_sub(1, Ordering::SeqCst);
            return Err(ChatProviderError::new(
                ChatProviderErrorKind::Status(401),
                "Unauthorized",
            ));
        }
        let index = self.index.fetch_add(1, Ordering::SeqCst);
        let sequence = if self.sequences.is_empty() {
            Vec::new()
        } else {
            let selected = std::cmp::min(index, self.sequences.len() - 1);
            self.sequences[selected].clone()
        };
        Ok(Box::new(SequenceStreamedMessage::new(sequence)))
    }

    fn with_thinking(&self, _effort: ThinkingEffort) -> Box<dyn ChatProvider> {
        Box::new(AuthThenSuccessChatProvider {
            sequences: self.sequences.clone(),
            index: AtomicUsize::new(self.index.load(Ordering::SeqCst)),
            fail_first: AtomicUsize::new(self.fail_first.load(Ordering::SeqCst)),
        })
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

fn make_llm_auth_then_success(fail_first_n: usize) -> kimi_agent::llm::LLM {
    kimi_agent::llm::LLM {
        chat_provider: Box::new(AuthThenSuccessChatProvider::new(
            vec![vec![StreamedMessagePart::Content(ContentPart::Text(TextPart::new("success")))]],
            fail_first_n,
        )),
        max_context_size: 100_000,
        capabilities: HashSet::new(),
        model_config: None,
        provider_config: None,
    }
}

fn runtime_with_llm(mut runtime: Runtime, llm: kimi_agent::llm::LLM) -> Runtime {
    runtime.llm = Some(Arc::new(llm));
    runtime
}

fn make_soul(
    runtime: Runtime,
    llm: kimi_agent::llm::LLM,
    toolset: KimiToolset,
    tmp_path: &std::path::Path,
) -> KimiSoul {
    let agent = Agent {
        name: "Test Agent".to_string(),
        system_prompt: "Test system prompt.".to_string(),
        toolset: Arc::new(tokio::sync::Mutex::new(toolset)),
        runtime: runtime_with_llm(runtime, llm),
    };

    KimiSoul::new(agent, Context::new(tmp_path.join("history.jsonl")))
}

async fn run_and_collect(soul: &KimiSoul, user_input: UserInput) -> Vec<WireMessage> {
    let messages = Arc::new(tokio::sync::Mutex::new(Vec::new()));
    let messages_clone = Arc::clone(&messages);

    let ui_loop = move |wire: Arc<Wire>| {
        let messages = Arc::clone(&messages_clone);
        async move {
            let ui = wire.ui_side(true);
            loop {
                let msg = match ui.receive().await {
                    Ok(msg) => msg,
                    Err(QueueShutDown) => return Ok(()),
                };
                messages.lock().await.push(msg);
            }
        }
    };

    run_soul(soul, user_input, ui_loop, CancellationToken::new(), None)
        .await
        .expect("run soul");

    messages.lock().await.clone()
}

#[test]
fn test_is_auth_error_401() {
    let err = ChatProviderError::new(ChatProviderErrorKind::Status(401), "Unauthorized");
    assert!(is_auth_error(&err));
}

#[test]
fn test_is_auth_error_403() {
    let err = ChatProviderError::new(ChatProviderErrorKind::Status(403), "Forbidden");
    assert!(is_auth_error(&err));
}

#[test]
fn test_is_auth_error_message_contains_auth() {
    let err = ChatProviderError::new(ChatProviderErrorKind::Other, "Authentication failed");
    assert!(is_auth_error(&err));
}

#[test]
fn test_is_auth_error_non_auth_status() {
    let err = ChatProviderError::new(ChatProviderErrorKind::Status(500), "Internal Server Error");
    assert!(!is_auth_error(&err));
}

#[test]
fn test_is_auth_error_429_not_auth() {
    let err = ChatProviderError::new(ChatProviderErrorKind::Status(429), "Too Many Requests");
    assert!(!is_auth_error(&err));
}

#[test]
fn test_is_auth_error_504_not_auth() {
    let err = ChatProviderError::new(ChatProviderErrorKind::Status(504), "Gateway Timeout");
    assert!(!is_auth_error(&err));
}

#[tokio::test]
async fn test_step_retries_auth_error_immediately() {
    let fixture = RuntimeFixture::new();
    let runtime = fixture.runtime.clone();

    let llm = make_llm_auth_then_success(1);
    let toolset = KimiToolset::new();
    let tmp = TempDir::new().expect("temp dir");
    let soul = make_soul(runtime, llm, toolset, tmp.path());

    let collected = run_and_collect(&soul, UserInput::Text("hello".to_string())).await;

    // Should succeed after 1 auth retry (immediate, no delay)
    assert!(collected.iter().any(|m| matches!(m, WireMessage::TurnBegin(_))));
    assert!(collected.iter().any(|m| matches!(m, WireMessage::TurnEnd(_))));

    let history = soul.context().lock().await.history().to_vec();
    assert_eq!(history.len(), 2, "user + assistant after auth retry success");
}

#[tokio::test]
async fn test_step_fails_on_persistent_auth_error() {
    let fixture = RuntimeFixture::new();
    let mut runtime = fixture.runtime.clone();
    runtime.config.loop_control.max_retries_per_step = 2;

    let llm = make_llm_auth_then_success(3); // fails more than max retries
    let toolset = KimiToolset::new();
    let tmp = TempDir::new().expect("temp dir");
    let soul = make_soul(runtime, llm, toolset, tmp.path());

    let result = run_soul(
        &soul,
        UserInput::Text("hello".to_string()),
        |_wire: Arc<Wire>| async move { Ok(()) },
        CancellationToken::new(),
        None,
    )
    .await;

    assert!(result.is_err(), "persistent auth error should fail after retry exhaustion");
}
