mod tool_test_utils;

use std::collections::{HashSet, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use kosong::message::{ContentPart, Message, Role, StreamedMessagePart, TextPart};
use kosong::chat_provider::{ChatProvider, ChatProviderError, StreamedMessage, ThinkingEffort, TokenUsage};
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

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

struct SequenceChatProvider {
    sequences: Vec<Vec<StreamedMessagePart>>,
    index: AtomicUsize,
}

impl SequenceChatProvider {
    fn new(sequences: Vec<Vec<StreamedMessagePart>>) -> Self {
        Self {
            sequences,
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
        _tools: &[kosong::tooling::Tool],
        _history: &[Message],
    ) -> Result<Box<dyn StreamedMessage>, ChatProviderError> {
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
        Box::new(SequenceChatProvider {
            sequences: self.sequences.clone(),
            index: AtomicUsize::new(self.index.load(Ordering::SeqCst)),
        })
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
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

async fn run_and_collect(
    soul: &KimiSoul,
    user_input: UserInput,
) -> Vec<WireMessage> {
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

fn make_llm(sequences: Vec<Vec<StreamedMessagePart>>) -> kimi_agent::llm::LLM {
    kimi_agent::llm::LLM {
        chat_provider: Box::new(SequenceChatProvider::new(sequences)),
        max_context_size: 100_000,
        capabilities: HashSet::new(),
        model_config: None,
        provider_config: None,
    }
}

#[tokio::test]
async fn test_steer_injected_as_separate_user_messages() {
    let fixture = RuntimeFixture::new();
    let runtime = fixture.runtime.clone();

    let llm = make_llm(vec![vec![
        StreamedMessagePart::Content(ContentPart::Text(TextPart::new("response"))),
    ]]);

    let toolset = KimiToolset::new();
    let tmp = TempDir::new().expect("temp dir");
    let soul = make_soul(runtime, llm, toolset, tmp.path());

    // Push steer messages before running
    soul.push_steer("Be concise.".to_string()).await;
    soul.push_steer("Use bullet points.".to_string()).await;

    let collected = run_and_collect(&soul, UserInput::Text("hello".to_string())).await;

    // Verify TurnBegin and TurnEnd were emitted
    assert!(collected.iter().any(|m| matches!(m, WireMessage::TurnBegin(_))));
    assert!(collected.iter().any(|m| matches!(m, WireMessage::TurnEnd(_))));

    // Verify history: steer messages are separate user messages before the actual prompt
    let history = soul.context().lock().await.history().to_vec();
    assert_eq!(history.len(), 4, "expected 4 messages: 2 steer + 1 user + 1 assistant");
    assert_eq!(history[0].extract_text(" "), "Be concise.");
    assert_eq!(history[1].extract_text(" "), "Use bullet points.");
    assert_eq!(history[2].extract_text(" "), "hello");
}

#[tokio::test]
async fn test_set_plan_mode_single_step_no_tools() {
    let fixture = RuntimeFixture::new();
    let runtime = fixture.runtime.clone();

    let llm = make_llm(vec![vec![
        StreamedMessagePart::Content(ContentPart::Text(TextPart::new("plan response"))),
    ]]);

    let toolset = KimiToolset::new();
    let tmp = TempDir::new().expect("temp dir");
    let soul = make_soul(runtime, llm, toolset, tmp.path());

    // Enable plan mode
    soul.set_plan_mode(true).await;
    assert!(soul.is_plan_mode().await);

    let collected = run_and_collect(&soul, UserInput::Text("plan something".to_string())).await;

    // In plan mode, should see exactly one StepBegin (single step, no agent loop)
    let step_begins: Vec<_> = collected
        .iter()
        .filter(|m| matches!(m, WireMessage::StepBegin(_)))
        .collect();
    assert_eq!(
        step_begins.len(),
        1,
        "plan_mode should produce exactly one step, got {} StepBegin events",
        step_begins.len()
    );

    // Verify history has user + assistant messages
    let history = soul.context().lock().await.history().to_vec();
    assert_eq!(history.len(), 2, "plan_mode should append user and assistant messages");
}

#[tokio::test]
async fn test_replay_reverts_context() {
    let fixture = RuntimeFixture::new();
    let runtime = fixture.runtime.clone();

    let llm = kimi_agent::llm::LLM {
        chat_provider: Box::new(kosong::chat_provider::echo::echo::EchoChatProvider),
        max_context_size: 100_000,
        capabilities: HashSet::new(),
        model_config: None,
        provider_config: None,
    };

    let toolset = KimiToolset::new();
    let tmp = TempDir::new().expect("temp dir");
    let soul = make_soul(runtime, llm, toolset, tmp.path());

    // Manually create checkpoints and messages via context
    {
        let mut ctx = soul.context().lock().await;
        ctx.checkpoint(false).await.expect("checkpoint 0");
        ctx.append_messages(Message::new(
            Role::User,
            vec![ContentPart::Text(TextPart::new("first message"))],
        ))
        .await
        .expect("append first");
        ctx.checkpoint(false).await.expect("checkpoint 1");
        ctx.append_messages(Message::new(
            Role::User,
            vec![ContentPart::Text(TextPart::new("second message"))],
        ))
        .await
        .expect("append second");
    }

    // Verify pre-replay state
    {
        let ctx = soul.context().lock().await;
        assert_eq!(ctx.n_checkpoints(), 2);
        assert_eq!(ctx.history().len(), 2);
    }

    // Replay to checkpoint 1 — should revert to state before checkpoint 1
    soul.replay(1).await.expect("replay to checkpoint 1");

    // Verify post-replay state
    {
        let ctx = soul.context().lock().await;
        assert_eq!(ctx.n_checkpoints(), 1, "should have 1 checkpoint after replay");
        assert_eq!(ctx.history().len(), 1, "should have 1 message after replay");
        let text = ctx.history()[0].extract_text(" ");
        assert_eq!(text, "first message");
    }

    // Replay to checkpoint 0 — should revert to empty state
    soul.replay(0).await.expect("replay to checkpoint 0");
    {
        let ctx = soul.context().lock().await;
        assert_eq!(ctx.n_checkpoints(), 0, "should have 0 checkpoints after replay to 0");
        assert_eq!(ctx.history().len(), 0, "should have 0 messages after replay to 0");
    }
}

#[tokio::test]
async fn test_replay_invalid_checkpoint_fails() {
    let fixture = RuntimeFixture::new();
    let runtime = fixture.runtime.clone();

    let llm = kimi_agent::llm::LLM {
        chat_provider: Box::new(kosong::chat_provider::echo::echo::EchoChatProvider),
        max_context_size: 100_000,
        capabilities: HashSet::new(),
        model_config: None,
        provider_config: None,
    };

    let toolset = KimiToolset::new();
    let tmp = TempDir::new().expect("temp dir");
    let soul = make_soul(runtime, llm, toolset, tmp.path());

    // No checkpoints exist yet
    let result = soul.replay(0).await;
    assert!(result.is_err(), "replay to non-existent checkpoint should fail");
    assert!(result.unwrap_err().to_string().contains("does not exist"));
}
