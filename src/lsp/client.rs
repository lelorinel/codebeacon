use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

pub struct LspClient {
    process: Child,
    stdin: ChildStdin,
    id: MessageId,
    rx: Receiver<Value>,
}

pub struct MessageId(u64);

impl MessageId {
    pub fn new() -> Self {
        Self(0)
    }
    pub fn next(&mut self) -> u64 {
        self.0 += 1;
        self.0
    }
}

pub fn encode_message(body: &str) -> String {
    format!("Content-Length: {}\r\n\r\n{}", body.len(), body)
}

fn read_message(reader: &mut BufReader<ChildStdout>) -> Result<Value> {
    let mut content_length: Option<usize> = None;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line)?;
        let line = line.trim();
        if line.is_empty() {
            break;
        }
        if let Some(val) = line.strip_prefix("Content-Length: ") {
            content_length = Some(val.parse()?);
        }
    }
    let len = content_length.context("missing Content-Length")?;
    let mut buf = vec![0u8; len];
    std::io::Read::read_exact(reader, &mut buf)?;
    Ok(serde_json::from_slice(&buf)?)
}

impl LspClient {
    pub fn start(binary: &str, args: &[&str], root_uri: &str) -> Result<Self> {
        let mut process = Command::new(binary)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .with_context(|| format!("Failed to start LSP binary: {binary}"))?;

        let stdin = process.stdin.take().unwrap();
        let stdout = process.stdout.take().unwrap();

        let (tx, rx) = mpsc::sync_channel::<Value>(64);
        let mut reader = BufReader::new(stdout);
        std::thread::spawn(move || {
            loop {
                match read_message(&mut reader) {
                    Ok(msg) => {
                        if tx.send(msg).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        let mut client = Self {
            process,
            stdin,
            id: MessageId::new(),
            rx,
        };

        client.initialize(root_uri)?;
        Ok(client)
    }

    fn send(&mut self, msg: &Value) -> Result<()> {
        let body = serde_json::to_string(msg)?;
        let encoded = encode_message(&body);
        self.stdin.write_all(encoded.as_bytes())?;
        self.stdin.flush()?;
        Ok(())
    }

    fn request(&mut self, method: &str, params: Value) -> Result<Value> {
        let id = self.id.next();
        self.send(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        }))?;
        loop {
            let msg = self.rx.recv_timeout(REQUEST_TIMEOUT)
                .map_err(|e| anyhow::anyhow!("LSP request timeout: {e}"))?;
            // Handle server-initiated requests (e.g. workspace/configuration from csharp-ls)
            if msg.get("method").is_some() && msg.get("id").is_some() {
                let _ = self.send(&serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": msg["id"],
                    "result": null
                }));
                continue;
            }
            if msg.get("id").and_then(|v| v.as_u64()) == Some(id) {
                return Ok(msg["result"].clone());
            }
        }
    }

    fn notify(&mut self, method: &str, params: Value) -> Result<()> {
        self.send(&json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params
        }))
    }

    fn initialize(&mut self, root_uri: &str) -> Result<()> {
        self.request(
            "initialize",
            json!({
                "processId": std::process::id(),
                "rootUri": root_uri,
                "capabilities": {
                    "textDocument": {
                        "documentSymbol": { "hierarchicalDocumentSymbolSupport": true }
                    }
                }
            }),
        )?;
        self.notify("initialized", json!({}))?;
        Ok(())
    }

    pub fn document_symbols(&mut self, file_path: &Path, language_id: &str) -> Result<Value> {
        let uri = path_to_uri(file_path);
        let text = std::fs::read_to_string(file_path)?;
        self.notify(
            "textDocument/didOpen",
            json!({
                "textDocument": {
                    "uri": uri,
                    "languageId": language_id,
                    "version": 1,
                    "text": text
                }
            }),
        )?;
        self.request(
            "textDocument/documentSymbol",
            json!({
                "textDocument": { "uri": uri }
            }),
        )
    }

    pub fn references(&mut self, file_path: &Path, line: u32, character: u32) -> Result<Value> {
        let uri = path_to_uri(file_path);
        // Open the document first so the server knows about it
        if let Ok(text) = std::fs::read_to_string(file_path) {
            let lang = crate::config::detect_language(file_path)
                .map(|l| l.language_id())
                .unwrap_or("text");
            let _ = self.notify("textDocument/didOpen", json!({
                "textDocument": { "uri": &uri, "languageId": lang, "version": 1, "text": text }
            }));
        }
        self.request(
            "textDocument/references",
            json!({
                "textDocument": { "uri": uri },
                "position": { "line": line, "character": character },
                "context": { "includeDeclaration": true }
            }),
        )
    }

    pub fn definition(&mut self, file_path: &Path, line: u32, character: u32) -> Result<Value> {
        let uri = path_to_uri(file_path);
        // Open the document first so the server knows about it
        if let Ok(text) = std::fs::read_to_string(file_path) {
            let lang = crate::config::detect_language(file_path)
                .map(|l| l.language_id())
                .unwrap_or("text");
            let _ = self.notify("textDocument/didOpen", json!({
                "textDocument": { "uri": &uri, "languageId": lang, "version": 1, "text": text }
            }));
        }
        self.request(
            "textDocument/definition",
            json!({
                "textDocument": { "uri": uri },
                "position": { "line": line, "character": character }
            }),
        )
    }
}

impl Drop for LspClient {
    fn drop(&mut self) {
        let _ = self.process.kill();
    }
}

pub fn path_to_uri(path: &Path) -> String {
    let abs = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let s = abs.to_string_lossy();
    let encoded = s.replace(' ', "%20").replace('#', "%23");
    format!("file://{}", encoded)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_message_with_content_length() {
        let body = r#"{"jsonrpc":"2.0","id":1,"method":"test"}"#;
        let encoded = encode_message(body);
        assert!(encoded.starts_with("Content-Length: "));
        assert!(encoded.contains("\r\n\r\n"));
        assert!(encoded.ends_with(body));
        let expected_len = body.len().to_string();
        assert!(encoded.contains(&expected_len));
    }

    #[test]
    fn next_id_increments() {
        let mut id = MessageId::new();
        assert_eq!(id.next(), 1);
        assert_eq!(id.next(), 2);
        assert_eq!(id.next(), 3);
    }
}
