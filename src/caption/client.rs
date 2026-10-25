//! The model server: one request per image, in either API.
//!
//! ```text
//! ollama   POST {endpoint}/api/chat               {model, messages: [{content, images: [b64]}]}
//!          GET  {endpoint}/api/tags               models[].name
//! openai   POST {endpoint}/v1/chat/completions    {model, messages: [{content: [text, image_url]}]}
//!          GET  {endpoint}/v1/models              data[].id
//! ```
//!
//! Requests ask for a deterministic reply (temperature 0) and no
//! streaming. An endpoint that already ends in `/v1` isn't given another.

use std::time::Duration;

use anyhow::{Context, Result, bail};
use base64::Engine;
use serde_json::{Value, json};

use crate::cache::select::usage;
use crate::settings::{Backend, Caption};

/// A connection to the model server the settings name.
pub struct Client {
    backend: Backend,
    base: String,
    model: String,
    key: Option<String>,
    agent: ureq::Agent,
}

/// Whether `endpoint`'s host is this machine, so images sent there don't
/// leave it.
pub fn is_local(endpoint: &str) -> bool {
    let rest = endpoint.split_once("://").map_or(endpoint, |(_, r)| r);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let authority = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    let host = match authority.strip_prefix('[') {
        Some(v6) => v6.split(']').next().unwrap_or_default(),
        None => authority.split(':').next().unwrap_or_default(),
    };
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback())
}

impl Client {
    /// A client for `c`. Fails if no model is set, or if the API key's
    /// variable is named but not set. It doesn't contact the server.
    pub fn new(c: &Caption) -> Result<Self> {
        if c.model.trim().is_empty() {
            return Err(usage(
                "no model set: pass --model or set one in the explorer's caption settings",
            ));
        }
        let key = match &c.api_key_env {
            Some(var) => Some(
                std::env::var(var)
                    .map_err(|_| usage(format!("API key variable {var} isn't set")))?,
            ),
            None => None,
        };
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(c.timeout_secs.max(1))))
            .http_status_as_error(false)
            .build()
            .into();
        Ok(Self {
            backend: c.backend,
            base: c.endpoint.trim_end_matches('/').to_owned(),
            model: c.model.trim().to_owned(),
            key,
            agent,
        })
    }

    /// The full URL of an API path.
    fn url(&self, path: &str) -> String {
        match self.backend {
            Backend::OpenAi if self.base.ends_with("/v1") => format!("{}{path}", self.base),
            Backend::OpenAi => format!("{}/v1{path}", self.base),
            Backend::Ollama => format!("{}/api{path}", self.base),
        }
    }

    /// The URL captions are requested from.
    pub fn chat_url(&self) -> String {
        self.url(match self.backend {
            Backend::Ollama => "/chat",
            Backend::OpenAi => "/chat/completions",
        })
    }

    /// The request body asking about `png` with `prompt`.
    pub fn body(&self, png: &[u8], prompt: &str) -> Value {
        let b64 = base64::engine::general_purpose::STANDARD.encode(png);
        match self.backend {
            Backend::Ollama => json!({
                "model": self.model,
                "messages": [{"role": "user", "content": prompt, "images": [b64]}],
                "stream": false,
                "options": {"temperature": 0},
            }),
            Backend::OpenAi => json!({
                "model": self.model,
                "messages": [{"role": "user", "content": [
                    {"type": "text", "text": prompt},
                    {"type": "image_url", "image_url": {"url": format!("data:image/png;base64,{b64}")}},
                ]}],
                "stream": false,
                "temperature": 0,
            }),
        }
    }

    /// Reads a reply as JSON, or fails with the status and the start of
    /// the body.
    fn read(
        &self,
        resp: Result<ureq::http::Response<ureq::Body>, ureq::Error>,
        url: &str,
    ) -> Result<Value> {
        let mut resp = resp.with_context(|| format!("couldn't reach {url}"))?;
        let status = resp.status();
        let text = resp
            .body_mut()
            .read_to_string()
            .with_context(|| format!("reading the reply from {url}"))?;
        if !status.is_success() {
            let start: String = text.chars().take(300).collect();
            bail!("{url} answered {status}: {start}");
        }
        serde_json::from_str(&text).with_context(|| format!("{url} didn't answer with JSON"))
    }

    fn authorised<B>(&self, req: ureq::RequestBuilder<B>) -> ureq::RequestBuilder<B> {
        match &self.key {
            Some(k) => req.header("Authorization", format!("Bearer {k}")),
            None => req,
        }
    }

    /// Asks the model about one PNG image; returns its reply text.
    pub fn caption(&self, png: &[u8], prompt: &str) -> Result<String> {
        let url = self.chat_url();
        let body = self.body(png, prompt);
        let reply = self.read(
            self.authorised(self.agent.post(&url)).send_json(&body),
            &url,
        )?;
        let text = match self.backend {
            Backend::Ollama => &reply["message"]["content"],
            Backend::OpenAi => &reply["choices"][0]["message"]["content"],
        };
        text.as_str()
            .map(str::to_owned)
            .with_context(|| format!("{url}'s reply has no message text"))
    }

    /// The models the server offers: a connection test.
    pub fn models(&self) -> Result<Vec<String>> {
        let (url, list, key) = match self.backend {
            Backend::Ollama => (self.url("/tags"), "models", "name"),
            Backend::OpenAi => (self.url("/models"), "data", "id"),
        };
        let reply = self.read(self.authorised(self.agent.get(&url)).call(), &url)?;
        let names = reply[list]
            .as_array()
            .with_context(|| format!("{url}'s reply has no model list"))?
            .iter()
            .filter_map(|m| m[key].as_str().map(str::to_owned))
            .collect();
        Ok(names)
    }

    /// The model asked.
    pub fn model(&self) -> &str {
        &self.model
    }
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc;

    use super::*;

    /// A one-request HTTP server on a free local port: answers `reply` and
    /// sends back the request line and body it got.
    fn serve(reply: &'static str) -> (String, mpsc::Receiver<(String, String)>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let mut len = 0;
            loop {
                let mut h = String::new();
                reader.read_line(&mut h).unwrap();
                if h.trim().is_empty() {
                    break;
                }
                if let Some((k, v)) = h.split_once(':')
                    && k.eq_ignore_ascii_case("content-length")
                {
                    len = v.trim().parse().unwrap();
                }
            }
            let mut body = vec![0; len];
            reader.read_exact(&mut body).unwrap();
            let mut stream = stream;
            write!(
                stream,
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{reply}",
                reply.len()
            )
            .unwrap();
            tx.send((line.trim().to_owned(), String::from_utf8(body).unwrap()))
                .unwrap();
        });
        (format!("http://{addr}"), rx)
    }

    fn settings(backend: Backend, endpoint: String) -> Caption {
        Caption {
            enabled: true,
            backend,
            endpoint,
            model: "vision".into(),
            timeout_secs: 5,
            ..Caption::default()
        }
    }

    #[test]
    fn ollama_chat() {
        let (url, got) =
            serve(r#"{"message": {"role": "assistant", "content": "falls over\ndeath"}}"#);
        let c = Client::new(&settings(Backend::Ollama, url)).unwrap();
        assert_eq!(c.caption(b"png", "what?").unwrap(), "falls over\ndeath");
        let (line, body) = got.recv().unwrap();
        assert_eq!(line, "POST /api/chat HTTP/1.1");
        let body: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(body["model"], "vision");
        assert_eq!(body["messages"][0]["content"], "what?");
        assert_eq!(body["messages"][0]["images"][0], "cG5n");
        assert_eq!(body["stream"], false);
    }

    #[test]
    fn openai_chat() {
        let (url, got) = serve(r#"{"choices": [{"message": {"content": "jumps\njump"}}]}"#);
        let c = Client::new(&settings(Backend::OpenAi, format!("{url}/v1/"))).unwrap();
        assert_eq!(c.caption(b"png", "what?").unwrap(), "jumps\njump");
        let (line, body) = got.recv().unwrap();
        assert_eq!(line, "POST /v1/chat/completions HTTP/1.1");
        let body: Value = serde_json::from_str(&body).unwrap();
        let content = &body["messages"][0]["content"];
        assert_eq!(content[0]["text"], "what?");
        assert_eq!(content[1]["image_url"]["url"], "data:image/png;base64,cG5n");
    }

    #[test]
    fn model_lists() {
        let (url, got) = serve(r#"{"models": [{"name": "a"}, {"name": "b"}]}"#);
        let c = Client::new(&settings(Backend::Ollama, url)).unwrap();
        assert_eq!(c.models().unwrap(), ["a", "b"]);
        assert_eq!(got.recv().unwrap().0, "GET /api/tags HTTP/1.1");
        let (url, got) = serve(r#"{"data": [{"id": "c"}]}"#);
        let c = Client::new(&settings(Backend::OpenAi, url)).unwrap();
        assert_eq!(c.models().unwrap(), ["c"]);
        assert_eq!(got.recv().unwrap().0, "GET /v1/models HTTP/1.1");
    }

    #[test]
    fn needs_a_model() {
        let mut s = settings(Backend::Ollama, "http://127.0.0.1:1".into());
        s.model = " ".into();
        assert!(Client::new(&s).is_err());
        s.model = "m".into();
        s.api_key_env = Some("RS2OOL_TEST_NO_SUCH_VARIABLE".into());
        assert!(Client::new(&s).is_err());
    }

    #[test]
    fn local_hosts() {
        for e in [
            "http://127.0.0.1:11434",
            "http://localhost:8080/v1",
            "http://[::1]:1234",
            "127.0.0.2",
            "https://user@LOCALHOST/",
        ] {
            assert!(is_local(e), "{e}");
        }
        for e in [
            "http://192.168.1.5:11434",
            "https://api.example.com/v1",
            "http://localhost.example.com",
            "http://127.0.0.1.example.com",
        ] {
            assert!(!is_local(e), "{e}");
        }
    }
}
