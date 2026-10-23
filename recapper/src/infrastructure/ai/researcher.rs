use std::sync::Arc;
use std::time::Duration;

use rig::completion::{Effort, GenerationOptions};
use rig::http_client::DynHttpClient;
use rig::prelude::*;
use rig::providers::chatgpt;
use rig::providers::chatgpt::auth::{AuthError, AuthSource, Authenticator, DeviceCodeHandler, FileTokenStore};
use rig::providers::openai::OpenAIConfig;

use crate::infrastructure::ai::tools::{ResearchTools, research_http_client};
use crate::use_cases::generate_recaps::RecapResearcher;

/// The recap model, fixed for every recap.
pub const RECAP_MODEL: &str = "gpt-6-luna";
const RECAP_REASONING: Effort = Effort::Medium;
// Tavily runs several queries per call, so a recap needs only a few turns; the cap stops runaway loops.
const MAX_TURNS: usize = 40;

/// Why a recap could not be researched.
#[derive(Debug, thiserror::Error)]
pub enum ResearchError {
    #[error("ChatGPT sign-in failed: {0}")]
    Auth(String),
    #[error("the agent run failed: {0}")]
    Agent(String),
    #[error("the agent run took longer than {0:?}")]
    Timeout(Duration),
    /// The model can answer "no news" after every search failed, which must not pass for a real recap.
    #[error("no web search returned results")]
    NoSearchResults,
}

/// Researches recaps with a Rig agent signed in to ChatGPT, searching through Tavily.
pub struct ChatGptResearcher {
    authenticator: Authenticator,
    http: DynHttpClient,
    /// The search and page client, shared by every run so its connections are reused.
    tools_http: reqwest::Client,
    tavily_api_key: Arc<str>,
    timeout: Duration,
}

impl ChatGptResearcher {
    /// Create a researcher that reads and refreshes the ChatGPT sign-in kept in `auth_dir`.
    pub fn new(auth_dir: &str, tavily_api_key: &str, timeout: Duration) -> anyhow::Result<Self> {
        Ok(Self {
            // The service never starts a device flow itself; `recapper login` signs in.
            authenticator: build_authenticator(auth_dir, false),
            http: rig::rig_reqwest::shared(),
            tools_http: research_http_client()?,
            tavily_api_key: tavily_api_key.into(),
            timeout,
        })
    }
}

impl RecapResearcher for ChatGptResearcher {
    async fn research(&self, prompt: &str) -> Result<String, ResearchError> {
        let context = self
            .authenticator
            .auth_context(&self.http)
            .await
            .map_err(|error| ResearchError::Auth(error.to_string()))?;
        let mut provider = OpenAIConfig::with_key(&chatgpt::DIALECT, context.api_key);
        if let Some(account_id) = context.account_id {
            provider = provider.with_account_id(account_id);
        }
        let tools = ResearchTools::new(self.tools_http.clone(), Arc::clone(&self.tavily_api_key));
        let tool_error = |error: rig::message::EmptyToolName| ResearchError::Agent(error.to_string());
        let agent = AgentBuilder::new(provider.connect(self.http.clone()).completion(RECAP_MODEL))
            .options(GenerationOptions::new().reasoning(RECAP_REASONING))
            .dynamic_tool(tools.web_search().map_err(tool_error)?)
            .dynamic_tool(tools.fetch_content().map_err(tool_error)?)
            .default_max_turns(MAX_TURNS)
            .build();

        let response = tokio::time::timeout(self.timeout, agent.prompt(prompt))
            .await
            .map_err(|_| ResearchError::Timeout(self.timeout))?
            .map_err(|error| ResearchError::Agent(error.to_string()))?;
        if tools.successful_searches() == 0 {
            return Err(ResearchError::NoSearchResults);
        }
        Ok(response.output())
    }

    async fn is_signed_in(&self) -> bool {
        // Rig reports a missing or rejected sign-in as a message; other errors are transient.
        !matches!(self.authenticator.auth_context(&self.http).await, Err(AuthError::Message(_)))
    }
}

/// Sign in to ChatGPT with a device code shown on stdout, and keep the credential in `auth_dir`.
pub async fn sign_in(auth_dir: &str) -> anyhow::Result<()> {
    let authenticator =
        build_authenticator(auth_dir, true).with_device_code_handler(DeviceCodeHandler::new(|prompt| {
            println!("Open {} and enter the code {}", prompt.verification_uri, prompt.user_code);
        }));
    let http: DynHttpClient = rig::rig_reqwest::shared();
    authenticator.sign_in_with_device_code(&http).await?;
    println!("Signed in to ChatGPT. The credential is in {auth_dir}.");
    Ok(())
}

fn build_authenticator(auth_dir: &str, allow_device_flow: bool) -> Authenticator {
    Authenticator::new(
        AuthSource::OAuth,
        FileTokenStore::new(auth_dir),
        DeviceCodeHandler::default(),
        allow_device_flow,
    )
}
