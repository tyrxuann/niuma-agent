# Niuma Agent - System Design Document

## What It Is

Niuma Agent is a **personal task-processing Agent** — an AI-powered assistant that combines LLM reasoning with MCP tools to help you get things done.

You tell it a task. It decides how to handle it:
- **Confident?** → Executes autonomously
- **Uncertain?** → Asks clarifying questions (Socrates-style)
- **Done?** → Offers to save as a scheduled task

---

## Core Concepts

### Two Orthogonal Dimensions

| Dimension | Meaning | Options |
|-----------|---------|---------|
| **How** — Execution Strategy | Does the agent know what to do? | Autonomous / Clarifying |
| **When** — Execution Timing | When should it run? | Immediate / Scheduled / Save-as-scheduled |

These are independent. A task can be:
- Simple + Immediate → "帮我查天气"
- Complex + Scheduled → "每天早上9点导XX网站数据"
- Complex + Immediate → "帮我从XX网站导数据"

### Confidence-Based Execution

The agent checks confidence at three stages:

| Stage | Low Confidence Trigger | Response |
|-------|------------------------|----------|
| Before | Missing info, unknown tool | Ask user |
| During | Unexpected state, failure | Ask user |
| After | Ambiguous result | Ask user to verify |

---

## Project Structure

```
niuma-agent/
├── Cargo.toml                    # Workspace root
├── crates/
│   ├── niuma-core/               # Shared types & traits
│   ├── niuma-llm/                # LLM provider adapters
│   ├── niuma-tools/              # Tool abstraction & MCP client
│   └── niuma-agent/              # Agent engine
└── apps/
    └── niuma-cli/                # CLI (clap + ratatui)
```

### Crate Responsibilities

| Crate | Purpose | Key Exports |
|-------|---------|-------------|
| `niuma-core` | Shared types, no business logic | `Error`, `Result`, config types, domain types |
| `niuma-llm` | LLM API adapters | `LLMProvider` trait, `ClaudeProvider`, `OpenAIProvider` |
| `niuma-tools` | Tool abstraction, MCP, built-ins | `Tool` trait, `ToolRegistry`, `MCPClient` |
| `niuma-agent` | Core agent logic | `Agent`, `IntentParser`, `Clarifier`, `Executor`, `TaskScheduler` |
| `niuma-cli` | TUI application | Binary `niuma` |

### Dependency Graph

```
niuma-cli
    └── niuma-agent
          ├── niuma-tools ──► niuma-core
          ├── niuma-llm   ──► niuma-core
          └── niuma-core
```

---

## System Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                      niuma-cli (TUI Layer)                          │
│                    ratatui / crossterm / clap                        │
└───────────────────────────────┬─────────────────────────────────────┘
                                │
┌───────────────────────────────▼─────────────────────────────────────┐
│                      niuma-agent (Agent Engine)                      │
│                                                                      │
│  ┌─────────────┐     ┌─────────────┐     ┌─────────────────────┐   │
│  │ IntentParser│────▶│  Executor   │     │   TaskScheduler     │   │
│  │ (classify)  │     │ (run steps) │     │   (cron trigger)    │   │
│  └──────┬──────┘     └──────┬──────┘     └─────────────────────┘   │
│         │ low conf          │                                       │
│         ▼                   │                                       │
│  ┌─────────────┐            │                                       │
│  │  Clarifier  │◀───────────┘                                       │
│  │ (Socrates)  │                                                    │
│  └─────────────┘                                                    │
└───────────────────────────────┬─────────────────────────────────────┘
                                │
        ┌───────────────────────┼───────────────────────┐
        ▼                       ▼                       ▼
┌───────────────┐      ┌────────────────┐      ┌────────────────┐
│  niuma-llm    │      │  niuma-tools   │      │  niuma-core    │
│ LLMProvider   │      │ ToolRegistry   │      │ Error, Config  │
│ Claude/OpenAI │      │ MCP + Built-in │      │ Domain Types   │
└───────────────┘      └────────────────┘      └────────────────┘
```

---

## Core Components

### 1. IntentParser (`niuma-agent`)

Classifies user input into intent + execution strategy.

```rust
pub struct IntentParser {
    llm: Arc<dyn LLMProvider>,
}

pub struct IntentClassification {
    pub intent: UserIntent,
    pub strategy: ExecutionStrategy,
    pub confidence: Confidence,
}

pub enum UserIntent {
    ExecuteNow { goal: String },
    CreateScheduledTask { goal: String, schedule: String },
    SaveAsScheduledTask { name: String, schedule: String },
    Other(String),
}

pub enum ExecutionStrategy {
    Autonomous,
    Clarifying { missing: Vec<MissingInfo> },
}
```

### 2. Clarifier (`niuma-agent`)

Socrates-style dialogue loop. Key method: `distill()`.

```rust
pub struct Clarifier {
    llm: Arc<dyn LLMProvider>,
}

impl Clarifier {
    pub async fn next_question(&self, missing: &[MissingInfo]) -> String;
    pub async fn process(&self, answer: &str, ctx: &mut ClarifyContext) -> ClarifyResult;
    pub async fn distill(&self, session: &Session) -> ExecutionPlan;
}
```

**Distillation** — extracts the correct execution path from a session:

| Kept | Filtered Out |
|------|--------------|
| Successful tool calls | Failed attempts |
| Confirmed decisions | Trial-and-error paths |
| Required parameters | Clarification dialogue |

Example: A 17-step conversation distills into 6 executable steps.

### 3. Executor (`niuma-agent`)

Runs steps with confidence checks.

```rust
pub struct Executor {
    llm: Arc<dyn LLMProvider>,
    tools: Arc<ToolRegistry>,
}

impl Executor {
    pub async fn execute(&self, task: &Task) -> Result<ExecutionResult>;
    pub async fn execute_step(&self, step: &Step) -> Result<StepResult>;
    pub async fn execute_with_check(&self, steps: &[Step], ctx: &mut ExecutionContext)
        -> Result<ExecutionResult>;
}
```

After each step, checks confidence. Low? Pauses and asks user.

### 4. ToolRegistry (`niuma-tools`)

Unified tool interface.

```rust
pub struct ToolRegistry {
    builtins: HashMap<String, Arc<dyn Tool>>,
    mcp_servers: HashMap<String, MCPServer>,
}

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn input_schema(&self) -> Value;
    async fn execute(&self, args: Value) -> Result<Value>;
}
```

**Built-in tools**: `file_read`, `file_write`, `shell`, `http`

**MCP tools**: Configured via `config.yaml`, auto-registered at startup.

### 5. TaskScheduler (`niuma-agent`)

Cron-based scheduling.

```rust
pub struct TaskScheduler {
    inner: JobScheduler,
    tasks: Arc<RwLock<HashMap<String, Task>>>,
}

pub struct Task {
    pub id: String,
    pub name: String,
    pub schedule: String,  // cron
    pub enabled: bool,
    pub steps: Vec<Step>,  // pre-confirmed, no clarification needed
}
```

### 6. Session (`niuma-core`)

Conversation state.

```rust
pub struct Session {
    pub id: Uuid,
    pub state: DialogueState,
    pub events: Vec<ExecutionEvent>,
}

pub enum ExecutionEvent {
    UserMessage(String),
    AgentMessage(String),
    ToolCall { tool: String, args: HashMap<String, Value>, result: ToolResult },
    Clarification { question: String, answer: String },
    Error { step: String, error: String },
}

pub enum DialogueState {
    Idle, Classifying, Clarifying, Executing, PromptingSave,
}
```

---

## Interaction Example

```
User: 帮我从 XX 网站导数据

Agent: Sure! What's the URL? Do you need to log in?

User: https://data.example.com, yes login needed

Agent: What are your credentials?
       Username: _______________
       Password: _______________

User: admin, password xxx

Agent: [Navigating...]
       [Screenshot]
       I see the login form. Can I proceed?

User: Yes

Agent: [Logging in...] ✓
       [Screenshot]
       Where's the data?

User: Click "Customers" in the sidebar

Agent: [Clicking...] ✓
       [Screenshot showing 1,234 customers]
       Export all or filter? Where's the export button?

User: Export all, top-right corner

Agent: [Clicking export...] Format: Excel / CSV / PDF?

User: Excel

Agent: [Downloading...] ✓
       Downloaded: customers.xlsx (2.1 MB)

       ─────────────────────────────────────
       ✓ Task complete!

       Save as scheduled task?

User: Yes, every day at 9am

Agent: ✓ Saved!
       • Name: Daily Data Export
       • Schedule: 09:00 daily
       • Steps: 6 confirmed
```

---

## TUI Layer (`niuma-cli`)

```
┌─────────────────────────────────────────────────────────────────────┐
│                         NIUMA AGENT                                 │
├──────────┬──────────────────────────────────────────────────────────┤
│          │  ┌────────────────────────────────────────────────────┐ │
│  [Chat]  │  │ Agent: Hi! What can I help you with?              │ │
│          │  │ User: Export data from XX site                     │ │
│  [Tasks] │  │ Agent: Sure! What's the URL?                       │ │
│          │  └────────────────────────────────────────────────────┘ │
│  [Logs]  │  ┌────────────────────────────────────────────────────┐ │
│          │  │ > _                                                │ │
│          │  └────────────────────────────────────────────────────┘ │
├──────────┴──────────────────────────────────────────────────────────┤
│  v0.1.0  │  Connected  │  Tasks: 3  │  ? help  │  Ctrl+L clear     │
└─────────────────────────────────────────────────────────────────────┘
```

**Shortcuts**: `c/1` Chat, `t/2` Tasks, `l/3` Logs, `Tab` cycle, `Ctrl+L` clear, `q` quit

---

## Configuration

```yaml
# config.yaml
llm:
  default: "claude"
  providers:
    claude:
      api_key: "${CLAUDE_API_KEY}"
      model: "claude-sonnet-4-6"
    openai:
      api_key: "${OPENAI_API_KEY}"
      model: "gpt-4o"

mcp_servers:
  playwright:
    command: "npx"
    args: ["-y", "@playwright/mcp"]
    env:
      HEADLESS: "true"

storage:
  schedules_dir: "./data/schedules"
  cache_dir: "./data/cache"
  logs_dir: "./data/logs"
```

---

## Data Persistence

```
data/
├── schedules/         # Task definitions (YAML)
├── cache/plans/       # Distilled ExecutionPlan cache
├── logs/              # Execution logs
└── temp/              # Temporary files
```

---

## Error Handling

```rust
pub enum FailureAction {
    Retry { max_attempts: u32, backoff: Backoff },
    Skip,
    UseCached,
    Fallback { step_id: String },
    AskUser,
}
```

Scheduled tasks → prefer auto-retry. Immediate tasks → prefer asking user.

---

## LLM Optimization

1. **Plan Caching** — Distilled plans keyed by goal hash. Cache hit = 0 LLM calls for planning.
2. **Batch Tool Calls** — LLM returns multiple tool calls at once.
3. **Context Compression** — Summarize old messages when session grows.

---

## Implementation Phases

| Phase | Scope |
|-------|-------|
| 1 | TUI foundation (terminal guard, views, input) |
| 2 | LLM Provider (unified trait, Claude, OpenAI) |
| 3 | Tool layer (MCP client, ToolRegistry, built-ins) |
| 4 | Agent Engine (IntentParser, Clarifier, Executor) |
| 5 | TaskScheduler (cron, persistence) |
| 6 | Session management (clear, compress) |
| 7 | Polish (error handling, optimization) |

---

## Design Principles

1. **Confidence-driven** — Proceed when confident, ask when not
2. **Two dimensions** — Execution strategy and timing are orthogonal
3. **MCP for browsers** — No browser code in the agent
4. **Ephemeral sessions** — Clearable anytime; data lives on disk
5. **Pre-confirmed tasks** — Scheduled tasks store steps, not prompts
6. **Distill correctly** — Save only the working path, not the exploration

---

## Tech Stack

| Category | Choice |
|----------|--------|
| Async runtime | tokio |
| Scheduling | tokio-cron-scheduler |
| Serialization | serde, serde_json, serde_yaml |
| HTTP | reqwest (rustls) |
| TUI | ratatui, crossterm |
| CLI | clap |
| Logging | tracing, tracing-subscriber |
| Errors | thiserror, anyhow |
