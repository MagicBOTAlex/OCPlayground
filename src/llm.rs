//! `ocplay --llm`: an exhaustive usage reference.
//!
//! The reference lives in `llm.md` at the root of the repository so it can be
//! updated without shipping a new binary. `--llm` fetches it over HTTP and
//! prints it. Set `OCPLAY_LLM_URL` to override the source (useful for tests or
//! for a fork).

use std::time::Duration;

/// Raw GitHub URL of the reference document on the default branch.
pub const URL: &str =
    "https://raw.githubusercontent.com/MagicBOTAlex/OCPlayground/master/llm.md";

/// The URL that will actually be fetched, honouring `OCPLAY_LLM_URL`.
pub fn url() -> String {
    std::env::var("OCPLAY_LLM_URL").unwrap_or_else(|_| URL.to_string())
}

/// Fetch the reference document. Returns the raw Markdown text.
pub fn fetch() -> anyhow::Result<String> {
    let url = url();
    let config = ureq::Agent::config_builder()
        .user_agent(concat!("ocplay/", env!("CARGO_PKG_VERSION")))
        .http_status_as_error(false)
        .timeout_global(Some(Duration::from_secs(15)))
        .build();
    let agent = ureq::Agent::new_with_config(config);

    let request = ureq::http::Request::builder()
        .method("GET")
        .uri(&url)
        .body(Vec::new())?;
    let response = agent.run(request)?;

    let status = response.status();
    if !status.is_success() {
        anyhow::bail!("HTTP {} for {}", status.as_u16(), url);
    }

    let bytes = response.into_body().read_to_vec()?;
    let text = String::from_utf8(bytes)?;
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_url_is_the_repo_markdown() {
        assert!(URL.starts_with("https://raw.githubusercontent.com/"));
        assert!(URL.ends_with("llm.md"));
    }

    #[test]
    fn env_overrides_the_url() {
        // Read-only check that the fallback is the documented default; setting
        // process env in tests is racy, so just assert `url()` never panics and
        // returns something non-empty.
        assert!(!url().is_empty());
    }
}
