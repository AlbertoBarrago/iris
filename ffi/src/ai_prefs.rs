//! The AI page of Iris Next's Settings: the assistant's connection and
//! model, the keys kept in the Keychain, how careful it is and how it
//! reaches the web. Each change goes through `mailrs_appcore::settings`
//! as it does from the GTK app's AI page, so the two read the same file.

use mailrs_ai::ProviderConfig;
use mailrs_appcore::assistant::{self, ANTHROPIC_KEY, BRAVE_KEY, LOCAL_KEY};
use mailrs_appcore::settings::{AiChange, AiProvider, AiSettings, Change, Feature, Use, WebSearch};
use mailrs_domain::translate::{fill, gettext};

use crate::CoreError;
use crate::mail::Mail;
use crate::prefs::{ChoiceItem, choices, key, load, parse, save};

/// The assistant's AI settings as they stand. Keys are never handed out,
/// only whether one is saved.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct AiPreferences {
    /// The assistant's connection, by key.
    pub provider: String,
    /// The assistant's model on that connection; empty lets Claude Code
    /// pick its own.
    pub model: String,
    /// The local server's API address.
    pub base_url: String,
    pub local_key_saved: bool,
    pub anthropic_key_saved: bool,
    /// Where the `claude` command was found, if it was.
    pub claude_path: Option<String>,
    pub confirm_actions: bool,
    pub details_expanded: bool,
    pub web_search: String,
    pub searxng_url: String,
    pub brave_key_saved: bool,
    /// Outside tools answered Always Allow, as `source/tool`.
    pub allowed_tools: Vec<String>,
}

/// The options the AI page's choices offer, in the GTK app's words.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct AiChoices {
    pub providers: Vec<ChoiceItem>,
    pub web_search: Vec<ChoiceItem>,
}

/// One model a connection offers.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ModelChoice {
    pub id: String,
    /// The provider's name for it; empty when it gives only the id.
    pub name: String,
    /// The id follows the newest version rather than naming one.
    pub alias: bool,
}

/// What a connection offers, and a sentence on what the list misses.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ModelListing {
    pub models: Vec<ModelChoice>,
    pub note: Option<String>,
}

/// A server or command found on this Mac that the assistant could use.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct FoundModel {
    /// For example "LM Studio on port 1234".
    pub label: String,
    /// The connection it is, by key.
    pub provider: String,
    /// The server's address, for a local server.
    pub base_url: Option<String>,
    /// The `claude` command, for Claude Code.
    pub command: Option<String>,
    pub models: Vec<String>,
}

#[uniffi::export]
impl Mail {
    pub fn ai_preferences(&self) -> AiPreferences {
        let settings = load();
        let ai = &settings.ai;
        AiPreferences {
            provider: key(&ai.provider),
            model: ai.model_on(ai.provider).to_string(),
            base_url: ai.base_url.clone(),
            local_key_saved: assistant::load_key(LOCAL_KEY).is_some(),
            anthropic_key_saved: assistant::load_key(ANTHROPIC_KEY).is_some(),
            claude_path: assistant::find_claude().map(|path| path.display().to_string()),
            confirm_actions: ai.confirm_actions,
            details_expanded: settings.assistant_details_expanded,
            web_search: key(&ai.web_search),
            searxng_url: ai.searxng_url.clone(),
            brave_key_saved: assistant::load_key(BRAVE_KEY).is_some(),
            allowed_tools: settings.assistant_allowed_tools.clone(),
        }
    }

    pub fn ai_choices(&self) -> AiChoices {
        AiChoices {
            providers: choices::<AiProvider>(),
            web_search: choices::<WebSearch>(),
        }
    }

    /// Moves the assistant to a connection, with the model it last used
    /// there.
    pub fn set_ai_provider(&self, provider: String) -> Result<(), CoreError> {
        let connection: AiProvider = parse(&provider)?;
        let model = load().ai.model_on(connection).to_string();
        save(Change::Ai(AiChange::Use {
            feature: Feature::Assistant,
            choice: Use::Model { connection, model },
        }))
    }

    /// The assistant's model on the connection it uses now.
    pub fn set_ai_model(&self, model: String) -> Result<(), CoreError> {
        let connection = load().ai.provider;
        save(Change::Ai(AiChange::Use {
            feature: Feature::Assistant,
            choice: Use::Model {
                connection,
                model: model.trim().to_string(),
            },
        }))
    }

    /// Changes one AI setting by name: `base_url`, `confirm_actions`,
    /// `details_expanded`, `web_search`, `searxng_url`, or `forbid_tool`
    /// with the tool's `source/tool` key.
    pub fn set_ai_setting(&self, name: String, value: String) -> Result<(), CoreError> {
        let change = match name.as_str() {
            "base_url" => Change::Ai(AiChange::BaseUrl(value.trim().trim_end_matches('/').to_string())),
            "confirm_actions" => Change::Ai(AiChange::ConfirmActions(value == "true")),
            "details_expanded" => Change::AssistantDetailsExpanded(value == "true"),
            "web_search" => Change::Ai(AiChange::WebSearch(parse(&value)?)),
            "searxng_url" => Change::Ai(AiChange::SearxngUrl(value.trim().to_string())),
            "forbid_tool" => Change::ForbidTool(value),
            other => return Err(CoreError::Store(format!("{other} is not an AI setting"))),
        };
        save(change)
    }

    /// Keeps a key in the Keychain under `name` (`local`, `anthropic` or
    /// `brave-search`); an empty one removes it.
    pub fn save_ai_key(&self, name: String, value: String) -> Result<(), CoreError> {
        let name = [LOCAL_KEY, ANTHROPIC_KEY, BRAVE_KEY]
            .into_iter()
            .find(|known| *known == name)
            .ok_or_else(|| CoreError::Store(format!("{name} is not a key Iris keeps")))?;
        // The keyring answers slowly the first time macOS asks to allow
        // it, so this runs off the caller's thread like any other wait.
        let value = value.trim().to_string();
        self.run(async move { tokio::task::spawn_blocking(move || assistant::save_key(name, &value)).await })
            .map_err(|err| CoreError::Store(err.to_string()))
    }

    /// Asks a connection for one line, with the model the assistant uses
    /// there, and says what came back. Blocks: call it off the main
    /// thread.
    pub fn test_ai(&self, provider: String) -> Result<String, CoreError> {
        let config = config_on(&load().ai, parse(&provider)?, None)?;
        Ok(match self.run(async move { mailrs_ai::test(&config).await }) {
            Ok(answer) => answer,
            Err(err) => fill(&gettext("No answer: {reason}"), &[("reason", &err.to_string())]),
        })
    }

    /// The models a connection offers. Blocks: call it off the main
    /// thread.
    pub fn list_ai_models(&self, provider: String) -> Result<ModelListing, CoreError> {
        // Listing needs no model; the name only gets past the check that
        // a model is chosen.
        let config = config_on(&load().ai, parse(&provider)?, Some("list"))?;
        let list = self
            .run(async move { mailrs_ai::list_models(&config).await })
            .map_err(|err| CoreError::Store(err.to_string()))?;
        Ok(ModelListing {
            models: list
                .models
                .into_iter()
                .map(|model| ModelChoice {
                    id: model.id,
                    name: model.name,
                    alias: model.alias,
                })
                .collect(),
            note: list.note,
        })
    }

    /// Looks for LM Studio, Ollama, Unsloth and Claude Code on this Mac.
    /// Blocks: call it off the main thread.
    pub fn find_ai(&self) -> Vec<FoundModel> {
        self.run(async { mailrs_ai::detect().await })
            .into_iter()
            .map(|found| {
                let (provider, base_url, command) = match found.config {
                    ProviderConfig::OpenAiCompatible { base_url, .. } => (AiProvider::Local, Some(base_url), None),
                    ProviderConfig::Anthropic { .. } => (AiProvider::Anthropic, None, None),
                    ProviderConfig::ClaudeCode { command, .. } => {
                        (AiProvider::ClaudeCode, None, Some(command.display().to_string()))
                    }
                };
                FoundModel {
                    label: found.label,
                    provider: key(&provider),
                    base_url,
                    command,
                    models: found.models,
                }
            })
            .collect()
    }

    /// Puts the assistant on something `find_ai` found, with its first
    /// model. The connection is saved first, so the assistant moves onto
    /// the address or command just found.
    pub fn use_found_ai(&self, found: FoundModel) -> Result<(), CoreError> {
        let connection: AiProvider = parse(&found.provider)?;
        let first = found.models.first().cloned().unwrap_or_default();
        match connection {
            AiProvider::Local => save(Change::Ai(AiChange::LocalServer {
                base_url: found.base_url.unwrap_or_default(),
                model: first.clone(),
            }))?,
            AiProvider::ClaudeCode => {
                if let Some(command) = found.command {
                    save(Change::Ai(AiChange::ClaudeCommand(command)))?;
                }
            }
            _ => {}
        }
        // Claude Code's first alias is its own default; the other
        // connections start from what was found.
        let model = match connection {
            AiProvider::ClaudeCode => load().ai.claude_model,
            _ => first,
        };
        save(Change::Ai(AiChange::Use {
            feature: Feature::Assistant,
            choice: Use::Model { connection, model },
        }))
    }
}

/// The connection `connection` as the assistant would use it, with
/// `model` standing in for its own when given. It goes through
/// `assistant::model_for` on a copy of the settings, so testing builds
/// what using it would.
fn config_on(ai: &AiSettings, connection: AiProvider, model: Option<&str>) -> Result<ProviderConfig, CoreError> {
    let mut ai = ai.clone();
    let model = model.map_or_else(|| ai.model_on(connection).to_string(), str::to_string);
    ai.set_use(Feature::Assistant, Use::Model { connection, model });
    assistant::model_for(&ai, Feature::Assistant).map_err(CoreError::Store)
}
