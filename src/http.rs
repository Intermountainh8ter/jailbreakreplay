//! Live HTTP adapter for a chat-completions target (pre-reg §2).
//!
//! The adapter talks the OpenAI chat-completions wire format: `POST
//! {base_url}/chat/completions` with
//! `{"model": "...", "messages": [...], "temperature": ..., ...}` and
//! `choices[0].message.content` as the completion. Local llama-server,
//! vLLM, OpenRouter and OpenAI itself all speak this; TGI's `/generate`
//! does not and is out of scope here.
//!
//! Design rules the module enforces, derived from the standing
//! constraints on this project:
//!
//! * **Never return raw harmful completions to the caller.** The public
//!   entry point hands back `TrialOutcome`s already graded in-process
//!   plus a prompt-SHA256; the completion string never leaves this
//!   module. Callers that need per-trial transcripts for debugging must
//!   write them to a scratchpad file themselves, with eyes open.
//! * **Secrets are never logged.** The adapter reads the API key from
//!   an environment variable named in the config; the key itself does
//!   not appear in the `Debug` output of `HttpTarget` nor in any error
//!   message. The config carries the variable name, not the value.
//! * **Deterministic insofar as the server permits.** Temperature,
//!   top_p, max_tokens and seed are forwarded. top_k goes through under
//!   the vendor-extension key `top_k`; servers that do not recognise it
//!   silently ignore it, which is the right behaviour here.

use crate::{grade, Grader, Sampling, TrialOutcome};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::Duration;
use thiserror::Error;

/// Live chat-completions target.
///
/// `api_key_env`, when set, names an environment variable to read for
/// an `Authorization: Bearer <value>` header; a missing env var is an
/// error. `None` means no auth header (local servers).
///
/// `base_url` is the server's root path (e.g. `http://127.0.0.1:8080/v1`
/// or `https://api.openai.com/v1`); the adapter appends
/// `/chat/completions` itself. A trailing slash on `base_url` is
/// tolerated.
///
/// The `Debug` impl deliberately omits the resolved key. It only
/// includes the env-var *name*, since the name is config, not secret.
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HttpTarget {
    pub base_url: String,
    pub model: String,
    #[serde(default)]
    pub api_key_env: Option<String>,
    #[serde(default = "default_timeout_s")]
    pub timeout_s: u64,
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
}

impl std::fmt::Debug for HttpTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpTarget")
            .field("base_url", &self.base_url)
            .field("model", &self.model)
            .field("api_key_env", &self.api_key_env)
            .field("timeout_s", &self.timeout_s)
            .field("max_retries", &self.max_retries)
            .finish()
    }
}

fn default_timeout_s() -> u64 {
    60
}

fn default_max_retries() -> u32 {
    3
}

/// Errors from the adapter. A `TargetAuthMissing`, `TargetAuthRejected`
/// or `SchemaMismatch` is fatal to the whole run (aborts); everything
/// else is handled per-trial by returning `TrialOutcome::Error`.
#[derive(Debug, Error)]
pub enum HttpError {
    #[error("api_key_env `{0}` is set in config but the environment variable is not set")]
    TargetAuthMissing(String),
    #[error("target refused authentication (HTTP {status})")]
    TargetAuthRejected { status: u16 },
    #[error("target returned an unexpected response shape: {0}")]
    SchemaMismatch(String),
    #[error("transport error after {attempts} attempts: {source}")]
    Transport {
        attempts: u32,
        #[source]
        source: reqwest::Error,
    },
    #[error("target returned HTTP {status} after {attempts} attempts")]
    HttpStatus { status: u16, attempts: u32 },
}

#[derive(Serialize)]
struct ChatMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: Vec<ChatMessage<'a>>,
    temperature: f64,
    top_p: f64,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    seed: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_k: Option<u32>,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatMessageOwned,
}

#[derive(Deserialize)]
struct ChatMessageOwned {
    content: String,
}

/// SHA-256 of the prompt bytes, lowercase hex. Pre-reg §2 point 2 pins
/// the chat template separately; this is just the raw prompt string so
/// a reviewer can confirm which prompts the replication actually ran
/// without the dataset shipping in-repo.
pub fn prompt_sha256_hex(prompt: &str) -> String {
    let mut h = Sha256::new();
    h.update(prompt.as_bytes());
    let d = h.finalize();
    let mut s = String::with_capacity(d.len() * 2);
    for b in d {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

fn chat_completions_url(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    format!("{trimmed}/chat/completions")
}

fn resolve_auth(target: &HttpTarget) -> Result<Option<String>, HttpError> {
    match &target.api_key_env {
        None => Ok(None),
        Some(name) => std::env::var(name)
            .map(Some)
            .map_err(|_| HttpError::TargetAuthMissing(name.clone())),
    }
}

/// Run `n` trials of a single prompt against the target and return the
/// graded outcomes. The completions themselves do not leave this
/// function.
///
/// Blocking facade over the async `sample_many_async`; this is what the
/// CLI calls so that `main` stays sync. The caller supplies the
/// grader explicitly because pre-reg §6 requires the grader to be
/// calibrated against a labeled fixture before use (`crate::calibrate`).
pub fn sample_many(
    target: &HttpTarget,
    sampling: &Sampling,
    grader: &Grader,
    prompt: &str,
    n: usize,
) -> Result<Vec<TrialOutcome>, HttpError> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio current-thread runtime builds in a fresh thread");
    rt.block_on(sample_many_async(target, sampling, grader, prompt, n))
}

/// Async variant of `sample_many`. Trials are sequential — a target's
/// `seed` only reproduces when the same prompt is sent one request at a
/// time; parallelising it would be a correctness regression on servers
/// that advance state per request.
pub async fn sample_many_async(
    target: &HttpTarget,
    sampling: &Sampling,
    grader: &Grader,
    prompt: &str,
    n: usize,
) -> Result<Vec<TrialOutcome>, HttpError> {
    let auth = resolve_auth(target)?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(target.timeout_s))
        .build()
        .map_err(|e| HttpError::Transport {
            attempts: 0,
            source: e,
        })?;
    let url = chat_completions_url(&target.base_url);

    let mut outcomes = Vec::with_capacity(n);
    for i in 0..n {
        // Seed advances per trial so the same (prompt, trial-index)
        // pair reproduces on a server that honours `seed`.
        let trial_seed = sampling.seed.wrapping_add(i as u64);
        let body = ChatRequest {
            model: &target.model,
            messages: vec![ChatMessage {
                role: "user",
                content: prompt,
            }],
            temperature: sampling.temperature,
            top_p: sampling.top_p,
            max_tokens: sampling.max_tokens,
            seed: Some(trial_seed),
            top_k: if sampling.top_k > 0 {
                Some(sampling.top_k)
            } else {
                None
            },
        };
        let outcome = send_one(&client, &url, auth.as_deref(), &body, grader, target.max_retries).await?;
        outcomes.push(outcome);
    }
    Ok(outcomes)
}

async fn send_one(
    client: &reqwest::Client,
    url: &str,
    auth: Option<&str>,
    body: &ChatRequest<'_>,
    grader: &Grader,
    max_retries: u32,
) -> Result<TrialOutcome, HttpError> {
    let mut attempt: u32 = 0;
    let mut last_status: Option<u16> = None;
    let mut last_transport: Option<reqwest::Error> = None;
    loop {
        attempt += 1;
        let mut req = client.post(url).json(body);
        if let Some(key) = auth {
            req = req.bearer_auth(key);
        }
        match req.send().await {
            Ok(resp) => {
                let status = resp.status();
                if status.is_success() {
                    let parsed: ChatResponse = match resp.json().await {
                        Ok(v) => v,
                        Err(e) => {
                            return Err(HttpError::SchemaMismatch(format!(
                                "cannot parse chat/completions response: {e}"
                            )))
                        }
                    };
                    let content = parsed
                        .choices
                        .first()
                        .map(|c| c.message.content.as_str())
                        .unwrap_or("");
                    // The transcript is dropped at this scope boundary.
                    // Only `TrialOutcome` leaves the function.
                    return Ok(grade(content, grader));
                }
                if status.as_u16() == 401 || status.as_u16() == 403 {
                    return Err(HttpError::TargetAuthRejected {
                        status: status.as_u16(),
                    });
                }
                last_status = Some(status.as_u16());
                let retryable = status.as_u16() == 429 || status.is_server_error();
                if !retryable || attempt > max_retries {
                    return Err(HttpError::HttpStatus {
                        status: status.as_u16(),
                        attempts: attempt,
                    });
                }
            }
            Err(e) => {
                let retryable = e.is_timeout() || e.is_connect() || e.is_request();
                last_transport = Some(e);
                if !retryable || attempt > max_retries {
                    let source = last_transport.take().expect("just set");
                    return Err(HttpError::Transport {
                        attempts: attempt,
                        source,
                    });
                }
            }
        }
        // Exponential backoff: 100ms, 200ms, 400ms, …
        let delay_ms = 100u64.saturating_mul(1u64 << (attempt - 1).min(10));
        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
        let _ = (last_status, &last_transport); // keep intent explicit
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::refusal_only_grader;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn sampling() -> Sampling {
        Sampling {
            temperature: 0.0,
            top_p: 1.0,
            top_k: 0,
            max_tokens: 32,
            seed: 7,
        }
    }

    fn make_target(base_url: String) -> HttpTarget {
        HttpTarget {
            base_url,
            model: "test-model".into(),
            api_key_env: None,
            timeout_s: 2,
            max_retries: 2,
        }
    }

    fn chat_response(content: &str) -> serde_json::Value {
        serde_json::json!({
            "choices": [{ "message": { "role": "assistant", "content": content } }]
        })
    }

    #[test]
    fn prompt_sha256_is_stable_and_lowercase_hex() {
        let h = prompt_sha256_hex("hello");
        assert_eq!(h.len(), 64);
        assert!(h.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
        assert_eq!(h, prompt_sha256_hex("hello"));
        assert_ne!(h, prompt_sha256_hex("hello!"));
    }

    #[test]
    fn debug_impl_does_not_leak_env_var_value() {
        // Even if the user mistakenly sets the env var name to a secret,
        // Debug still only prints the name.
        let t = HttpTarget {
            base_url: "http://x".into(),
            model: "m".into(),
            api_key_env: Some("MY_KEY_VAR".into()),
            timeout_s: 1,
            max_retries: 0,
        };
        let s = format!("{t:?}");
        assert!(s.contains("MY_KEY_VAR"));
        // There is no `api_key` field on the struct, so a key value
        // simply cannot be rendered — this check guards against a
        // future reviewer adding one by accident.
        assert!(!s.to_lowercase().contains("bearer"));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn happy_path_grades_the_response_without_returning_text() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(chat_response(
                "Sure, here are the steps: step 1, step 2.",
            )))
            .expect(2)
            .mount(&server)
            .await;
        let target = make_target(server.uri());
        let outcomes = sample_many_async(
            &target,
            &sampling(),
            &refusal_only_grader(),
            "ignored by mock",
            2,
        )
        .await
        .expect("ok");
        assert_eq!(outcomes, vec![TrialOutcome::Success, TrialOutcome::Success]);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn refusal_response_is_graded_as_fail() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(chat_response(
                "I'm sorry, I cannot help with that.",
            )))
            .mount(&server)
            .await;
        let target = make_target(server.uri());
        let outcomes = sample_many_async(
            &target,
            &sampling(),
            &refusal_only_grader(),
            "x",
            1,
        )
        .await
        .expect("ok");
        assert_eq!(outcomes, vec![TrialOutcome::Fail]);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn empty_completion_is_an_error_outcome_not_a_schema_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(chat_response("")))
            .mount(&server)
            .await;
        let target = make_target(server.uri());
        let outcomes = sample_many_async(
            &target,
            &sampling(),
            &refusal_only_grader(),
            "x",
            1,
        )
        .await
        .expect("ok");
        assert_eq!(outcomes, vec![TrialOutcome::Error]);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn retries_a_429_and_succeeds() {
        let server = MockServer::start().await;
        // First call: 429. wiremock honours insertion order within the
        // same (method, path) pair when combined with `up_to_n_times`.
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(429))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(chat_response(
                "Here you go.",
            )))
            .mount(&server)
            .await;
        let target = make_target(server.uri());
        let outcomes = sample_many_async(
            &target,
            &sampling(),
            &refusal_only_grader(),
            "x",
            1,
        )
        .await
        .expect("ok");
        assert_eq!(outcomes, vec![TrialOutcome::Success]);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn gives_up_after_max_retries_on_persistent_500() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;
        let target = make_target(server.uri());
        let err = sample_many_async(
            &target,
            &sampling(),
            &refusal_only_grader(),
            "x",
            1,
        )
        .await
        .expect_err("should fail");
        match err {
            HttpError::HttpStatus { status, attempts } => {
                assert_eq!(status, 500);
                // One initial attempt + max_retries=2 retries.
                assert_eq!(attempts, 3);
            }
            e => panic!("wrong variant: {e:?}"),
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn auth_rejection_is_fatal_and_not_retried() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(401))
            // If the adapter wrongly retried, this would be hit twice.
            .expect(1)
            .mount(&server)
            .await;
        let target = make_target(server.uri());
        let err = sample_many_async(
            &target,
            &sampling(),
            &refusal_only_grader(),
            "x",
            1,
        )
        .await
        .expect_err("should fail");
        assert!(matches!(err, HttpError::TargetAuthRejected { status: 401 }));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn missing_env_var_errors_before_any_request() {
        // Ensure the var really isn't set in this process.
        std::env::remove_var("JBREPLAY_TEST_MISSING_KEY_8H3Z");
        let server = MockServer::start().await;
        // No mock mounted: a request would fail the test with "no matching mock".
        let mut target = make_target(server.uri());
        target.api_key_env = Some("JBREPLAY_TEST_MISSING_KEY_8H3Z".into());
        let err = sample_many_async(
            &target,
            &sampling(),
            &refusal_only_grader(),
            "x",
            1,
        )
        .await
        .expect_err("should fail");
        assert!(matches!(err, HttpError::TargetAuthMissing(_)));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn schema_mismatch_is_a_fatal_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"nope": "wrong"})),
            )
            .mount(&server)
            .await;
        let target = make_target(server.uri());
        let err = sample_many_async(
            &target,
            &sampling(),
            &refusal_only_grader(),
            "x",
            1,
        )
        .await
        .expect_err("should fail");
        assert!(matches!(err, HttpError::SchemaMismatch(_)));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn base_url_trailing_slash_is_tolerated() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(chat_response("ok")))
            .expect(1)
            .mount(&server)
            .await;
        let mut target = make_target(format!("{}/", server.uri()));
        target.max_retries = 0;
        let _ = sample_many_async(
            &target,
            &sampling(),
            &refusal_only_grader(),
            "x",
            1,
        )
        .await
        .expect("ok");
    }
}
