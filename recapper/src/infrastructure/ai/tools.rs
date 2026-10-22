use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use rig::message::{EmptyToolName, ToolName};
use rig::tool::{DynamicTool, ToolExecutionError, ToolOutput};
use serde::Deserialize;
use serde_json::json;

const TAVILY_SEARCH_URL: &str = "https://api.tavily.com/search";
const SEARCH_RESULTS_PER_QUERY: u8 = 5;
// Pages beyond this size waste context; the model only needs dates and key facts.
const MAX_PAGE_CHARS: usize = 20_000;
const PAGE_TEXT_WIDTH: usize = 120;
const TOOL_HTTP_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Deserialize)]
struct SearchArgs {
    queries: Vec<String>,
    recency_filter: Option<String>,
}

#[derive(Deserialize)]
struct FetchArgs {
    url: String,
}

#[derive(Deserialize)]
struct TavilyResult {
    title: String,
    url: String,
    #[serde(default)]
    content: String,
    published_date: Option<String>,
}

#[derive(Deserialize)]
struct TavilyResponse {
    #[serde(default)]
    answer: Option<String>,
    #[serde(default)]
    results: Vec<TavilyResult>,
}

/// The research tools of one recap run, which count the searches that returned results.
#[derive(Clone)]
pub struct ResearchTools {
    http: reqwest::Client,
    tavily_api_key: Arc<str>,
    successful_searches: Arc<AtomicUsize>,
}

impl ResearchTools {
    /// Create the tools for one run, with a fresh search count.
    pub fn new(tavily_api_key: &str) -> anyhow::Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(TOOL_HTTP_TIMEOUT)
            .user_agent("Mozilla/5.0 (compatible; telegram-news-recapper/0.1)")
            .build()?;
        Ok(Self { http, tavily_api_key: tavily_api_key.into(), successful_searches: Arc::default() })
    }

    /// Return how many search queries returned results in this run.
    pub fn successful_searches(&self) -> usize {
        self.successful_searches.load(Ordering::Relaxed)
    }

    /// Build the web_search tool, which runs its queries against Tavily in parallel.
    pub fn web_search(&self) -> Result<DynamicTool, EmptyToolName> {
        let parameters = json!({
            "type": "object",
            "properties": {
                "queries": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "One or more search queries, run in parallel"
                },
                "recency_filter": {
                    "type": "string",
                    "enum": ["day", "week", "month", "year"],
                    "description": "Only return results published within this period"
                }
            },
            "required": ["queries"]
        });
        let tools = self.clone();
        Ok(DynamicTool::new(
            ToolName::new("web_search")?,
            "Search the web with one or more queries. Returns a short answer and dated results with URLs per query.",
            parameters,
            move |args| {
                let tools = tools.clone();
                Box::pin(async move {
                    let args: SearchArgs = serde_json::from_value(args)
                        .map_err(|error| ToolExecutionError::invalid_args(error.to_string()))?;
                    let searches = args.queries.iter().map(|query| tools.search(query, args.recency_filter.as_deref()));
                    let results = futures::future::join_all(searches).await;
                    let succeeded = results.iter().filter(|result| result.is_ok()).count();
                    tools.successful_searches.fetch_add(succeeded, Ordering::Relaxed);
                    let sections: Vec<String> =
                        results.into_iter().map(|result| result.unwrap_or_else(|e| e)).collect();
                    if succeeded == 0 {
                        return Err(ToolExecutionError::provider(sections.join("\n\n")));
                    }
                    Ok(ToolOutput::text(sections.join("\n\n")))
                })
            },
        ))
    }

    /// Build the fetch_content tool, which reads one article as plain text.
    pub fn fetch_content(&self) -> Result<DynamicTool, EmptyToolName> {
        let parameters = json!({
            "type": "object",
            "properties": {"url": {"type": "string", "description": "The http(s) URL to read"}},
            "required": ["url"]
        });
        let tools = self.clone();
        Ok(DynamicTool::new(
            ToolName::new("fetch_content")?,
            "Fetch a web page and return its readable text, to confirm an article's date or details.",
            parameters,
            move |args| {
                let tools = tools.clone();
                Box::pin(async move {
                    let args: FetchArgs = serde_json::from_value(args)
                        .map_err(|error| ToolExecutionError::invalid_args(error.to_string()))?;
                    Ok(ToolOutput::text(tools.fetch(&args.url).await?))
                })
            },
        ))
    }

    /// Run one Tavily search and format its answer and dated sources, or describe why it failed.
    async fn search(&self, query: &str, recency_filter: Option<&str>) -> Result<String, String> {
        let failed = |reason: String| format!("## {query}\nSearch failed: {reason}");
        let mut body = json!({
            "query": query,
            "search_depth": "basic",
            "max_results": SEARCH_RESULTS_PER_QUERY,
            "include_answer": "basic",
        });
        if let Some(range) = recency_filter {
            body["time_range"] = json!(range);
        }
        let response = self
            .http
            .post(TAVILY_SEARCH_URL)
            .bearer_auth(&*self.tavily_api_key)
            .json(&body)
            .send()
            .await
            .map_err(|error| failed(error.to_string()))?;
        if !response.status().is_success() {
            return Err(failed(format!("Tavily answered {}", response.status())));
        }
        let data: TavilyResponse = response.json().await.map_err(|error| failed(error.to_string()))?;
        let results: Vec<String> = data
            .results
            .iter()
            .map(|result| {
                let date = result.published_date.as_deref().map(|date| format!(" [{date}]")).unwrap_or_default();
                format!("{}\nSource: {}{date} ({})", result.content, result.title, result.url)
            })
            .collect();
        Ok(format!("## {query}\n{}\n\n{}", data.answer.unwrap_or_default(), results.join("\n\n")))
    }

    /// Download a page and return its readable text, truncated to the context budget.
    async fn fetch(&self, url: &str) -> Result<String, ToolExecutionError> {
        if !(url.starts_with("https://") || url.starts_with("http://")) {
            return Err(ToolExecutionError::invalid_args(format!("{url} is not an http(s) URL")));
        }
        let response =
            self.http.get(url).send().await.map_err(|error| ToolExecutionError::network(error.to_string()))?;
        let status = response.status();
        if !status.is_success() {
            return Err(ToolExecutionError::provider(format!("{url} answered {status}")));
        }
        let html = response.text().await.map_err(|error| ToolExecutionError::network(error.to_string()))?;
        let mut text = html2text::from_read(html.as_bytes(), PAGE_TEXT_WIDTH).unwrap_or(html);
        if text.len() > MAX_PAGE_CHARS {
            text.truncate(text.floor_char_boundary(MAX_PAGE_CHARS));
            text.push_str("\n[truncated]");
        }
        Ok(text)
    }
}
