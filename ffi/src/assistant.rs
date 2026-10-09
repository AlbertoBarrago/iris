//! The assistant for Iris Next: the same tool loop the GTK app runs
//! (`mailrs_appcore::assistant`), with this module as the window behind
//! its two ports.
//!
//! The tools hold `Rc`s and run on one thread, so each assistant gets a
//! thread of its own with a single-threaded runtime. The model's requests
//! go out on the store's runtime; their tool calls come back here. What
//! the tools read from the window ([`Desk`]) comes from the store, the
//! settings and what Swift last said is on screen. What they ask of the
//! window ([`Effects`]) goes to Swift through [`AssistantWindow`], and a
//! question waits for Swift's answer by id.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use futures::channel::oneshot;
use futures::future::{Either, select};
use mailrs_ai::{AgentEvent, Conversation, ProviderConfig};
use mailrs_appcore::assistant::run::{
    Answer, Background, Desk, Effects, Modules, OnScreen, OpenConversation, Permission, Tools,
};
use mailrs_appcore::assistant::sources::{self, ApprovalRequest, Source, Toolbox, Verdict};
use mailrs_appcore::assistant::turn::{
    self, Phase, Step, ToolState, Turn, input_summary, thinking_title, tool_label,
};
use mailrs_appcore::assistant::{self, Host, ToolRequest};
use mailrs_appcore::compose::{self, Draft};
use mailrs_appcore::protection::{self, Held, Standard};
use mailrs_appcore::settings::{AiProvider, Change, Feature, Settings, WebSearch};
use mailrs_appcore::unsubscribe::RequestSent;
use mailrs_appcore::unsubscribe_lines::{ListLine, Way, body, heading, line_text};
use mailrs_appcore::unsubscribe_page::{Adviser, Browser, PageError, PageForm, Plan};
use mailrs_domain::translate::{fill, gettext};
use mailrs_domain::{Account, AccountId, EpochMillis, Label, ThreadSummary};
use mailrs_store::{accounts, labels, threads};
use mailrs_sync::calendar_copy::CalendarCopy;
use mailrs_sync::{
    AccountSettings, Accounts, Calendar, ContactBook, Invitations, MailAction, Outbox, Outcome,
    Posted, View,
};

use crate::compose::{ComposeDraft, queued};
use crate::mail::{Mail, Running, ThreadRef};

/// One step of a turn as the pane draws it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct TurnStep {
    /// `thinking`, `tool` or `reply`.
    pub kind: String,
    /// What the row says: "Thought for 3 seconds", "Searching mail".
    pub title: String,
    /// The tool's input in a few words, beside its title.
    pub summary: String,
    /// The tool's whole input, readable, for the open row.
    pub input: String,
    /// What the model thought, what a tool answered, or the reply in
    /// Markdown.
    pub text: String,
    /// `working`, `done` or `failed`.
    pub state: String,
}

/// A turn as it stands, with the status line under it while it runs.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct TurnView {
    pub steps: Vec<TurnStep>,
    pub phase: Option<String>,
}

/// What the window shows, which the tools read as "this conversation"
/// and "the selected mail".
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct AssistantScreen {
    pub mailbox: String,
    pub open: Option<ThreadRef>,
    pub open_subject: String,
    pub selected: Vec<ThreadRef>,
}

/// The person's answer to a question the assistant asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ApprovalAnswer {
    Once,
    Always,
    Deny,
}

/// One list on the Unsubscribe question, and what leaving it will do.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct LeaveLine {
    pub name: String,
    pub text: String,
    /// The page is still being read; the question waits on it.
    pub reading: bool,
}

/// The window, as the assistant reaches it. Swift implements it; every
/// call comes from the assistant's thread, so Swift hops to the main
/// actor.
#[uniffi::export(with_foreign)]
pub trait AssistantWindow: Send + Sync {
    /// The running turn changed.
    fn turn(&self, view: TurnView);
    /// The turn ended, with what went wrong if something did.
    fn finished(&self, problem: Option<String>);
    /// Asks the person `question`; the answer comes back through
    /// `Assistant::answer` with `id`. `always` offers Always Allow.
    fn ask(&self, id: u64, question: String, always: bool);
    /// Asks which lists to leave, under `heading` and the line `body`
    /// says when a page will be loaded; the answer comes back through
    /// `Assistant::answer_leave` with `id`.
    fn leave_lists(&self, id: u64, heading: String, body: Option<String>, lines: Vec<LeaveLine>);
    /// One list on that question, once its page has been read.
    fn leave_line(&self, id: u64, index: u32, line: LeaveLine);
    /// Opens a composer on `draft`.
    fn compose(&self, draft: ComposeDraft);
    /// Sends `draft` after the Undo Send delay, as the composer does.
    fn send(&self, draft: ComposeDraft);
    fn show_thread(&self, account_id: i64, thread_id: String);
    fn copy(&self, text: String);
    fn open_url(&self, url: String);
    /// Mail, counts or the outbox changed: read the sidebar and list again.
    fn changed(&self);
    /// A tool changed Preferences.
    fn settings_changed(&self);
    /// A line for the person, such as a toast.
    fn say(&self, text: String);
}

/// What the assistant's thread is asked to do.
enum Command {
    Ask(String),
    Stop,
    Reset,
}

/// A question waiting for Swift.
enum Pending {
    Verdict(oneshot::Sender<ApprovalAnswer>),
    Leave(oneshot::Sender<Option<Vec<u32>>>),
}

/// What both threads share: the questions waiting and the screen.
#[derive(Default)]
struct Shared {
    pending: Mutex<HashMap<u64, Pending>>,
    screen: Mutex<Option<AssistantScreen>>,
    ids: AtomicU64,
}

impl Shared {
    fn next_id(&self) -> u64 {
        self.ids.fetch_add(1, Ordering::Relaxed) + 1
    }

    fn take(&self, id: u64) -> Option<Pending> {
        self.pending.lock().unwrap_or_else(|p| p.into_inner()).remove(&id)
    }

    fn wait(&self, id: u64, pending: Pending) {
        self.pending.lock().unwrap_or_else(|p| p.into_inner()).insert(id, pending);
    }
}

/// The assistant of one window.
#[derive(uniffi::Object)]
pub struct Assistant {
    commands: async_channel::Sender<Command>,
    shared: Arc<Shared>,
}

#[uniffi::export]
impl Mail {
    /// Starts the assistant for `window`, on a thread of its own.
    pub fn assistant(self: Arc<Self>, window: Arc<dyn AssistantWindow>) -> Arc<Assistant> {
        assistant::preload_keys();
        let (commands, received) = async_channel::unbounded();
        let shared = Arc::new(Shared::default());
        let (mail, held) = (Arc::clone(&self), Arc::clone(&shared));
        let started = std::thread::Builder::new()
            .name("iris-assistant".into())
            .stack_size(mailrs_sync::WORKER_STACK)
            .spawn(move || {
                let runtime = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
                    Ok(runtime) => runtime,
                    Err(err) => {
                        tracing::error!(error = %err, "the assistant could not start its runtime");
                        return;
                    }
                };
                let local = tokio::task::LocalSet::new();
                local.block_on(&runtime, serve(mail, window, held, received));
            });
        if let Err(err) = started {
            tracing::error!(error = %err, "the assistant could not start its thread");
        }
        Arc::new(Assistant { commands, shared })
    }
}

#[uniffi::export]
impl Assistant {
    /// Asks the assistant something, as if typed.
    pub fn ask(&self, text: String) {
        let _ = self.commands.try_send(Command::Ask(text));
    }

    /// Stops the turn that runs.
    pub fn stop(&self) {
        let _ = self.commands.try_send(Command::Stop);
    }

    /// Starts over with an empty chat.
    pub fn reset(&self) {
        let _ = self.commands.try_send(Command::Reset);
    }

    /// What the window shows now.
    pub fn set_screen(&self, screen: AssistantScreen) {
        *self.shared.screen.lock().unwrap_or_else(|p| p.into_inner()) = Some(screen);
    }

    /// The answer to the question `id`.
    pub fn answer(&self, id: u64, answer: ApprovalAnswer) {
        if let Some(Pending::Verdict(reply)) = self.shared.take(id) {
            let _ = reply.send(answer);
        }
    }

    /// The lists ticked on the Unsubscribe question `id`, or `None` when
    /// the person said no.
    pub fn answer_leave(&self, id: u64, ticked: Option<Vec<u32>>) {
        if let Some(Pending::Leave(reply)) = self.shared.take(id) {
            let _ = reply.send(ticked);
        }
    }
}

/// Runs this process as the assistant's MCP server for Claude Code, which
/// starts the app's own binary with `--mcp-bridge <socket>`: it relays
/// tool calls to the running window and opens no window of its own.
/// Returns the process's exit status.
#[uniffi::export]
pub fn run_mcp_bridge(socket: String) -> i32 {
    let runtime = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
        Ok(runtime) => runtime,
        Err(err) => {
            eprintln!("mcp bridge: {err}");
            return 1;
        }
    };
    match runtime.block_on(mailrs_ai::bridge::run_mcp_stdio(std::path::Path::new(&socket))) {
        Ok(()) => 0,
        Err(err) => {
            eprintln!("mcp bridge: {err}");
            1
        }
    }
}

/// The model the assistant runs on, as its pane's subtitle names it, or
/// `None` while it is off.
#[uniffi::export]
pub fn assistant_model() -> Option<String> {
    let ai = Settings::load(&Settings::default_path()).ai;
    let model = ai.model_on(ai.provider);
    match ai.provider {
        AiProvider::Off => None,
        AiProvider::ClaudeCode if model.is_empty() => Some("Claude".into()),
        AiProvider::ClaudeCode => Some(format!("Claude {model}")),
        _ => Some(model.to_string()),
    }
}

/// One chat: the model it runs on, what was said, and the tool sources it
/// started with, kept for the whole chat as the GTK pane keeps them.
struct Chat {
    config: ProviderConfig,
    conversation: Arc<tokio::sync::Mutex<Conversation>>,
    sources: Vec<Arc<dyn Source>>,
}

/// The assistant's side of the thread: the ports, the chat and the turn.
struct Session {
    port: Rc<Port>,
    requests: async_channel::Sender<ToolRequest>,
    approvals: async_channel::Sender<ApprovalRequest>,
    chat: RefCell<Option<Chat>>,
    running: Cell<bool>,
    stop: RefCell<Option<async_channel::Sender<()>>>,
}

async fn serve(
    mail: Arc<Mail>,
    window: Arc<dyn AssistantWindow>,
    shared: Arc<Shared>,
    commands: async_channel::Receiver<Command>,
) {
    let handle = mail.runtime.handle().clone();
    let modules = modules(&mail);
    let port = Rc::new(Port {
        mail,
        window,
        shared,
        handle: handle.clone(),
        cache: RefCell::default(),
        turn: RefCell::default(),
    });
    let tools = Rc::new(Tools::new(
        modules,
        Rc::new(OnRuntime(handle)) as Rc<dyn Background>,
        Rc::clone(&port) as Rc<dyn Desk>,
        Rc::clone(&port) as Rc<dyn Effects>,
    ));
    let (requests, asked) = async_channel::unbounded::<ToolRequest>();
    let (approvals, approving) = async_channel::unbounded::<ApprovalRequest>();
    let session = Rc::new(Session {
        port: Rc::clone(&port),
        requests,
        approvals,
        chat: RefCell::default(),
        running: Cell::new(false),
        stop: RefCell::default(),
    });
    // Tool calls from the model run here, where the tools live.
    tokio::task::spawn_local(async move {
        while let Ok(request) = asked.recv().await {
            let tools = Rc::clone(&tools);
            tokio::task::spawn_local(async move {
                tracing::info!(tool = %request.name, "the assistant runs a tool");
                let outcome = tools.run(&request.name, request.input).await;
                let _ = request.reply.send(outcome).await;
            });
        }
    });
    // Outside sources, such as MCP servers, ask before they act.
    let asking = Rc::clone(&port);
    tokio::task::spawn_local(async move {
        while let Ok(request) = approving.recv().await {
            let verdict = asking.decide(request.question, true).await;
            if verdict == Verdict::Always {
                asking.change(Change::AllowTool(request.key.clone()));
            }
            let _ = request.reply.send(verdict).await;
        }
    });
    while let Ok(command) = commands.recv().await {
        match command {
            Command::Ask(text) => {
                if !session.running.get() {
                    tokio::task::spawn_local(Rc::clone(&session).ask(text));
                }
            }
            Command::Stop => {
                if let Some(stop) = session.stop.borrow_mut().take() {
                    let _ = stop.try_send(());
                }
            }
            Command::Reset => {
                if !session.running.get() {
                    *session.chat.borrow_mut() = None;
                }
            }
        }
    }
}

/// The mail modules the tools work through, over the store's engine.
fn modules(mail: &Mail) -> Modules<Running> {
    let (engine, db) = (Arc::clone(&mail.running), mail.db.clone());
    let copy = Arc::new(CalendarCopy::new(Arc::clone(&engine), db.clone()));
    let photos = mailrs_appcore::dirs::user_cache_dir()
        .join(mailrs_sync::config::DIR_NAME)
        .join("contact-photos");
    Modules {
        mail: Arc::clone(&mail.actions),
        lists: Arc::clone(&mail.mailboxes),
        gmail: Arc::new(AccountSettings::new(Arc::clone(&engine), db.clone())),
        calendar: Arc::new(Calendar::new(Arc::clone(&engine), db.clone(), Arc::clone(&copy))),
        invitations: Arc::new(Invitations::new(Arc::clone(&engine), db.clone(), copy)),
        contacts: Arc::new(ContactBook::new(Arc::clone(&engine), db.clone(), photos)),
        accounts: engine,
        db,
    }
}

impl Session {
    async fn ask(self: Rc<Self>, text: String) {
        let port = &self.port;
        port.refresh().await;
        let settings = port.settings();
        let config = match assistant::model_for(&settings.ai, Feature::Assistant) {
            Ok(config) => config,
            Err(problem) => {
                tracing::warn!(problem = %problem, "the assistant has no model");
                return port.window.finished(Some(problem));
            }
        };
        tracing::info!(provider = ?settings.ai.provider, "the assistant asks the model");
        let (conversation, chat_sources) = {
            let mut chat = self.chat.borrow_mut();
            match chat.as_ref() {
                Some(current) if current.config == config => {
                    (Arc::clone(&current.conversation), current.sources.clone())
                }
                _ => {
                    let chat_sources = sources::for_settings(&settings);
                    let prompt = sources::system_prompt(assistant::SYSTEM_PROMPT, &chat_sources);
                    let conversation = Arc::new(tokio::sync::Mutex::new(
                        Conversation::new(config.clone(), prompt).with_thinking(),
                    ));
                    *chat = Some(Chat {
                        config,
                        conversation: Arc::clone(&conversation),
                        sources: chat_sources.clone(),
                    });
                    (conversation, chat_sources)
                }
            }
        };
        self.running.set(true);
        *port.turn.borrow_mut() = Some(Turn::new(Instant::now()));
        port.show_turn();
        let (events, received) = async_channel::unbounded::<AgentEvent>();
        let (stop, stopped) = async_channel::bounded::<()>(1);
        *self.stop.borrow_mut() = Some(stop);
        let web = settings.ai.web_search != WebSearch::Off;
        let host = Arc::new(Toolbox::new(
            Host::new(assistant::run::specs(), self.requests.clone()),
            chat_sources,
            self.approvals.clone(),
            settings.assistant_allowed_tools,
        ));
        let shown = Rc::clone(port);
        let late = received.clone();
        tokio::task::spawn_local(async move {
            while let Ok(event) = received.recv().await {
                shown.apply(event);
            }
        });
        // The model's request runs on the store's runtime, as every
        // network call does; its tool calls come back to this thread.
        let result = port
            .handle
            .spawn(async move {
                let mut conversation = conversation.lock().await;
                conversation.set_web(web);
                tokio::select! {
                    reply = conversation.send(text, host, events) => reply.map_err(|err| err.to_string()),
                    _ = stopped.recv() => Err(gettext("Stopped.")),
                }
            })
            .await
            .unwrap_or_else(|err| Err(err.to_string()));
        // The reply can land before every event was drawn, and those
        // belong above the end of the turn.
        while let Ok(event) = late.try_recv() {
            port.apply(event);
        }
        if let Ok(reply) = &result {
            let wrote = port
                .turn
                .borrow()
                .as_ref()
                .is_some_and(|turn| turn.steps().iter().any(|step| matches!(step, Step::Reply(_))));
            if !wrote && !reply.trim().is_empty() {
                port.apply(AgentEvent::Text(reply.clone()));
            }
        }
        if let Some(turn) = port.turn.borrow_mut().as_mut() {
            turn.finish(Instant::now());
        }
        port.show_turn();
        *port.turn.borrow_mut() = None;
        *self.stop.borrow_mut() = None;
        self.running.set(false);
        match &result {
            Ok(_) => tracing::info!("the assistant answered"),
            Err(err) => tracing::warn!(error = %err, "the assistant's turn failed"),
        }
        port.window.finished(result.err());
    }
}

/// What the tools read before a turn, since the ports cannot wait: the
/// accounts, their labels, and the conversations picked.
#[derive(Default)]
struct Cache {
    accounts: Vec<Account>,
    labels: HashMap<AccountId, Vec<Label>>,
    selected: Vec<ThreadSummary>,
}

/// The window behind both ports.
struct Port {
    mail: Arc<Mail>,
    window: Arc<dyn AssistantWindow>,
    shared: Arc<Shared>,
    handle: tokio::runtime::Handle,
    cache: RefCell<Cache>,
    turn: RefCell<Option<Turn>>,
}

impl Port {
    /// Reads what the ports hand out during the turn.
    async fn refresh(&self) {
        let db = self.mail.db.clone();
        let picked: Vec<ThreadRef> = self.screen().map(|s| s.selected).unwrap_or_default();
        let read = self
            .handle
            .spawn(async move {
                db.read(move |c| {
                    let accounts = accounts::list_accounts(c)?;
                    let mut labels = HashMap::new();
                    for account in &accounts {
                        labels.insert(account.id, labels::list_labels(c, account.id)?);
                    }
                    let mut selected = Vec::new();
                    for thread in &picked {
                        selected.extend(threads::get_thread(c, thread.account_id, &thread.thread_id)?);
                    }
                    Ok((accounts, labels, selected))
                })
                .await
            })
            .await;
        match read {
            Ok(Ok((accounts, labels, selected))) => {
                *self.cache.borrow_mut() = Cache {
                    accounts,
                    labels,
                    selected,
                };
            }
            Ok(Err(err)) => tracing::warn!(error = %err, "the assistant could not read the store"),
            Err(err) => tracing::warn!(error = %err, "the assistant could not read the store"),
        }
    }

    fn screen(&self) -> Option<AssistantScreen> {
        self.shared.screen.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }

    fn apply(&self, event: AgentEvent) {
        if let Some(turn) = self.turn.borrow_mut().as_mut() {
            turn.apply(event, Instant::now());
        }
        self.show_turn();
    }

    fn show_turn(&self) {
        let view = self.turn.borrow().as_ref().map(view_of);
        if let Some(view) = view {
            self.window.turn(view);
        }
    }

    /// Asks the person and waits. With `always`, Always Allow is offered.
    async fn decide(&self, question: String, always: bool) -> Verdict {
        let id = self.shared.next_id();
        let (reply, answer) = oneshot::channel();
        self.shared.wait(id, Pending::Verdict(reply));
        self.awaiting(true);
        self.window.ask(id, question, always);
        let answer = answer.await.unwrap_or(ApprovalAnswer::Deny);
        self.awaiting(false);
        match answer {
            ApprovalAnswer::Once => Verdict::Once,
            ApprovalAnswer::Always => Verdict::Always,
            ApprovalAnswer::Deny => Verdict::Deny,
        }
    }

    fn awaiting(&self, waiting: bool) {
        if let Some(turn) = self.turn.borrow_mut().as_mut() {
            turn.set_awaiting_approval(waiting);
        }
        self.show_turn();
    }

    /// Saves a change to Preferences and tells the window.
    fn change(&self, change: Change) {
        let path = Settings::default_path();
        let mut settings = Settings::load(&path);
        change.apply_to(&mut settings);
        if let Err(err) = settings.save(&path) {
            tracing::warn!(error = %err, "could not save the settings");
        }
        self.window.settings_changed();
    }

    fn shown(&self, draft: Draft) -> ComposeDraft {
        self.mail.shown(draft)
    }

    fn signed(&self, draft: Draft) -> ComposeDraft {
        self.mail.shown(self.mail.signed(draft))
    }

    /// Hands `message` to the outbox on the store's runtime.
    async fn post(&self, message: mailrs_store::outbox::Queued, scheduled: bool) -> Result<Posted, String> {
        let outbox = Arc::new(Outbox::new(Arc::clone(&self.mail.running), self.mail.db.clone()));
        self.handle
            .spawn(async move {
                match scheduled {
                    true => outbox.schedule(message).await,
                    false => outbox.post(message).await,
                }
            })
            .await
            .map_err(|err| err.to_string())?
            .map_err(|err| err.to_string())
    }
}

/// A turn's steps in the words the pane shows.
fn view_of(turn: &Turn) -> TurnView {
    let steps = turn
        .steps()
        .iter()
        .map(|step| match step {
            Step::Thinking { text, took } => TurnStep {
                kind: "thinking".into(),
                title: thinking_title(*took),
                summary: String::new(),
                input: String::new(),
                text: text.trim().to_string(),
                state: if took.is_some() { "done" } else { "working" }.into(),
            },
            Step::Tool {
                name,
                input,
                state,
                output,
                ..
            } => TurnStep {
                kind: "tool".into(),
                title: tool_label(name),
                summary: input_summary(input),
                input: turn::readable_input(input),
                text: turn::readable(output),
                state: match state {
                    ToolState::Running => "working",
                    ToolState::Done => "done",
                    ToolState::Failed => "failed",
                }
                .into(),
            },
            Step::Reply(text) => TurnStep {
                kind: "reply".into(),
                title: String::new(),
                summary: String::new(),
                input: String::new(),
                text: text.clone(),
                state: "done".into(),
            },
        })
        .collect();
    let phase = match turn.phase() {
        Phase::Writing if turn.steps().iter().any(|s| matches!(s, Step::Reply(_))) => None,
        phase => Some(phase.label()),
    };
    TurnView { steps, phase }
}

/// Starts the tools' tasks on the store's runtime.
struct OnRuntime(tokio::runtime::Handle);

impl Background for OnRuntime {
    fn start(&self, task: std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>) {
        self.0.spawn(task);
    }
}

/// Iris Next reads no unsubscribe page yet, so every page goes to the
/// person's browser, as one the rules cannot finish does.
struct NoBrowser;

impl Browser for NoBrowser {
    fn load(&self, _url: &str) -> Answer<'_, Result<PageForm, PageError>> {
        // Only the log reads this: a page that will not load goes to the
        // browser.
        Box::pin(async { Err(PageError::Load("Iris Next reads no pages yet".into())) })
    }

    fn submit(&self, _plan: &Plan, _address: &str) -> Answer<'_, Result<PageForm, PageError>> {
        Box::pin(async { Err(PageError::Changed) })
    }

    fn reread(&self) -> Answer<'_, Result<PageForm, PageError>> {
        Box::pin(async { Err(PageError::Changed) })
    }

    fn at(&self) -> String {
        String::new()
    }
}

impl Desk for Port {
    fn settings(&self) -> Settings {
        Settings::load(&Settings::default_path())
    }

    fn accounts(&self) -> Vec<Account> {
        self.cache.borrow().accounts.clone()
    }

    fn labels(&self) -> HashMap<AccountId, Vec<Label>> {
        self.cache.borrow().labels.clone()
    }

    fn view(&self) -> View {
        let settings = self.settings();
        View {
            threading: settings.threading,
            follow_ups: settings.suggest_follow_ups,
            now: mailrs_sync::now_millis(),
            ..View::default()
        }
    }

    fn on_screen(&self) -> OnScreen {
        let screen = self.screen();
        OnScreen {
            mailbox: screen.as_ref().map(|s| s.mailbox.clone()).unwrap_or_default(),
            open: screen.as_ref().and_then(|s| {
                s.open.as_ref().map(|open| OpenConversation {
                    account_id: open.account_id,
                    thread_id: open.thread_id.clone(),
                    message_id: None,
                    subject: s.open_subject.clone(),
                })
            }),
            selected: self.cache.borrow().selected.clone(),
        }
    }

    fn default_account(&self) -> Option<AccountId> {
        let preferred = self.settings().default_account;
        let cache = self.cache.borrow();
        preferred
            .and_then(|email| cache.accounts.iter().find(|a| a.email.eq_ignore_ascii_case(&email)))
            .or_else(|| cache.accounts.first())
            .map(|account| account.id)
    }

    fn downloads(&self) -> PathBuf {
        mailrs_appcore::dirs::home_dir().join("Downloads")
    }
}

impl Effects for Port {
    fn confirm(&self, question: String) -> Answer<'_, bool> {
        Box::pin(async move { self.decide(question, false).await != Verdict::Deny })
    }

    fn ask_permission(&self, account_id: AccountId, permission: Permission) {
        let cache = self.cache.borrow();
        let Some(account) = cache.accounts.iter().find(|a| a.id == account_id) else {
            return;
        };
        let wording = permission.wording(
            mailrs_appcore::permission::Occasion::Needed,
            &account.email,
            account.provider,
        );
        self.window.say(format!("{} {}", wording.heading, wording.body));
    }

    fn explain_api_off(&self, service: &str, enable_url: &str) {
        self.window.say(fill(
            &gettext(
                "The Google Cloud project Iris signs in with has the {service} \
                 switched off, so Google refuses before it can ask for your permission. \
                 Turn it on, wait a minute, and try again.",
            ),
            &[("service", service)],
        ));
        self.window.open_url(enable_url.to_string());
    }

    fn send_later(&self, draft: Draft, at: EpochMillis) -> Result<(), String> {
        let draft = self.mail.signed(draft);
        let raw = built(&draft)?;
        let mut message = queued(&draft, raw);
        message.send_at = at;
        let window = Arc::clone(&self.window);
        let outbox = Arc::new(Outbox::new(Arc::clone(&self.mail.running), self.mail.db.clone()));
        self.handle.spawn(async move {
            match outbox.schedule(message).await {
                Ok(Posted::Refused(problem)) => {
                    window.say(fill(&gettext("Not scheduled: {reason}"), &[("reason", &problem)]))
                }
                Ok(_) => {
                    window.changed();
                    window.say(fill(
                        &gettext("Will send {when}"),
                        &[("when", &mailrs_appcore::format::future_date(at, chrono::Local::now()))],
                    ));
                }
                Err(err) => window.say(fill(
                    &gettext("Not scheduled: {reason}"),
                    &[("reason", &err.to_string())],
                )),
            }
        });
        Ok(())
    }

    fn send_request(
        &self,
        account_id: AccountId,
        from: String,
        to: String,
        subject: String,
        body: String,
    ) -> Answer<'_, Result<RequestSent, String>> {
        Box::pin(async move {
            let mut draft = self.mail.blank(account_id).map_err(|err| err.to_string())?;
            // The list knows the person by the address it writes to.
            if let Ok(senders) = self.mail.senders()
                && let Some(alias) = senders
                    .into_iter()
                    .find(|s| s.account_id == account_id && s.email.eq_ignore_ascii_case(&from))
            {
                draft.from.email = alias.email;
                draft.from.name = alias.name;
            }
            draft.to = compose::parse_recipients(&to);
            draft.subject = subject;
            draft.markdown = body;
            let raw = built(&draft)?;
            match self.post(queued(&draft, raw), false).await? {
                Posted::Sent(_) => {
                    self.mail.check_account(account_id);
                    self.window.changed();
                    Ok(RequestSent::Sent)
                }
                Posted::Waiting(_) => {
                    self.window.changed();
                    Ok(RequestSent::Waiting)
                }
                Posted::Refused(problem) => Err(problem),
            }
        })
    }

    fn open_page(&self, url: &str) {
        self.window.open_url(url.to_string());
    }

    fn left_list(&self, _account_id: AccountId, _thread_id: &str) {
        self.window.changed();
    }

    fn page_browser(&self) -> Rc<dyn Browser> {
        Rc::new(NoBrowser)
    }

    fn page_adviser(&self) -> Option<Box<dyn Adviser>> {
        None
    }

    fn confirm_unsubscribe(
        &self,
        lines: Vec<ListLine>,
        updates: async_channel::Receiver<(usize, Way)>,
    ) -> Answer<'_, Option<Vec<(usize, Way)>>> {
        Box::pin(async move {
            let id = self.shared.next_id();
            let names: Vec<String> = lines.iter().map(|line| line.name.clone()).collect();
            let shown = lines.iter().map(|line| leave_line(&line.name, &line.way)).collect();
            let (asked, said) = (heading(&lines), body(&lines));
            let ways = RefCell::new(lines.into_iter().map(|line| line.way).collect::<Vec<_>>());
            let (reply, answer) = oneshot::channel();
            self.shared.wait(id, Pending::Leave(reply));
            self.window.leave_lists(id, asked, said, shown);
            // Pages read late fill their lines in while the question waits.
            let forward = async {
                while let Ok((at, way)) = updates.recv().await {
                    if let Some(name) = names.get(at) {
                        self.window.leave_line(id, at as u32, leave_line(name, &way));
                    }
                    if let Some(slot) = ways.borrow_mut().get_mut(at) {
                        *slot = way;
                    }
                }
            };
            let ticked = match select(answer, std::pin::pin!(forward)).await {
                Either::Left((ticked, _)) => ticked,
                Either::Right(((), answer)) => answer.await,
            }
            .ok()
            .flatten()?;
            let mut ways = ways.into_inner();
            Some(
                ticked
                    .into_iter()
                    .filter_map(|at| {
                        let at = at as usize;
                        ways.get_mut(at).map(|way| (at, std::mem::replace(way, Way::Reading)))
                    })
                    .collect(),
            )
        })
    }

    fn change_settings(&self, change: Change) -> Result<(), String> {
        self.change(change);
        Ok(())
    }

    fn new_draft(&self, account_id: AccountId) -> Result<Draft, String> {
        self.mail.blank(account_id).map_err(|err| err.to_string())
    }

    fn compose(&self, draft: Draft) -> Result<(), String> {
        self.window.compose(self.signed(draft));
        Ok(())
    }

    fn send(&self, draft: Draft) -> Result<(), String> {
        self.window.send(self.signed(draft));
        Ok(())
    }

    fn show_thread(&self, summary: ThreadSummary) {
        self.window.show_thread(summary.account_id, summary.id);
    }

    fn copy(&self, text: &str) {
        self.window.copy(text.to_string());
    }

    fn mail_changed(&self, _action: &MailAction, _outcome: &Outcome) {
        self.mail.mailboxes.forget_remote();
        self.window.changed();
    }

    fn relist(&self) {
        self.window.changed();
    }

    fn categories_moved(&self) {
        self.mail.mailboxes.forget_remote();
        self.window.changed();
    }

    fn reopen_unsent(&self, draft: Draft) -> Result<(), String> {
        self.window.compose(self.shown(draft));
        Ok(())
    }

    fn queue_changed(&self) {
        self.window.changed();
    }

    fn undone(&self, _outcome: &Outcome) {
        self.mail.mailboxes.forget_remote();
        self.window.changed();
    }

    fn image_senders_changed(&self) {
        self.window.changed();
    }

    // Iris Next does not reach gpg or gpgsm yet, so it holds no keys and
    // opens no encrypted draft.

    fn keys(&self, _addresses: Vec<String>) -> Answer<'_, Held> {
        Box::pin(async { Held::default() })
    }

    fn signing_standard(&self, _from: String) -> Answer<'_, Standard> {
        Box::pin(async { protection::signing(&Held::default()) })
    }

    fn reopen_draft(&self, raw: Vec<u8>, draft: Draft) -> Answer<'_, Result<Draft, String>> {
        Box::pin(async move {
            let mut draft = draft;
            match protection::draft::standard_of(&raw) {
                None => {
                    protection::draft::reopen_plain(&raw, &mut draft);
                    Ok(draft)
                }
                Some(_) => Err(mailrs_appcore::pgp::explain(&mailrs_pgp::PgpError::NoGpg)),
            }
        })
    }

    fn save_draft(&self, draft: Draft) -> Answer<'_, Result<(), String>> {
        Box::pin(async move {
            let sync = self
                .mail
                .running
                .account(draft.account_id)
                .ok_or_else(|| gettext("That account is not connected."))?;
            let message_id = compose::new_message_id(&draft.from.email);
            let raw = compose::build_saved_draft(&draft, mailrs_sync::now_millis() / 1000, &message_id)
                .map_err(|err| fill(&gettext("Could not save: {reason}"), &[("reason", &err)]))?;
            let (thread, old) = (draft.thread_id.clone(), draft.draft_id.clone());
            self.handle
                .spawn(async move { sync.save_draft(raw, thread, old).await })
                .await
                .map_err(|err| err.to_string())?
                .map(|_| ())
                .map_err(|err| fill(&gettext("Draft not saved: {reason}"), &[("reason", &err.to_string())]))
        })
    }
}

/// The message `draft` makes, ready for the outbox.
fn built(draft: &Draft) -> Result<Vec<u8>, String> {
    let message_id = compose::new_message_id(&draft.from.email);
    compose::build_mime(draft, mailrs_sync::now_millis() / 1000, &message_id)
        .map_err(|err| fill(&gettext("Could not build the message: {reason}"), &[("reason", &err)]))
}

fn leave_line(name: &str, way: &Way) -> LeaveLine {
    LeaveLine {
        name: name.to_string(),
        text: line_text(way),
        reading: matches!(way, Way::Reading),
    }
}
