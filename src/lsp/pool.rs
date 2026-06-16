use crate::config::Language;
use crate::lsp::client::LspClient;
use std::collections::HashMap;

pub struct LspPool {
    clients: HashMap<Language, LspClient>,
    root_uri: String,
}

pub fn is_binary_available(binary: &str) -> bool {
    which::which(binary).is_ok()
}

impl LspPool {
    pub fn new(root_uri: &str) -> Self {
        Self {
            clients: HashMap::new(),
            root_uri: root_uri.to_string(),
        }
    }

    pub fn get_or_start(&mut self, lang: &Language) -> Option<&mut LspClient> {
        if !self.clients.contains_key(lang) {
            let binary = lang.lsp_binary();
            if !is_binary_available(binary) {
                return None;
            }
            match LspClient::start(binary, lang.lsp_args(), &self.root_uri) {
                Ok(client) => { self.clients.insert(lang.clone(), client); }
                Err(e) => {
                    tracing::warn!("Failed to start {binary}: {e}");
                    return None;
                }
            }
        }
        self.clients.get_mut(lang)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_is_available_when_binary_exists() {
        let avail = is_binary_available("echo");
        assert!(avail);
    }

    #[test]
    fn language_not_available_for_missing_binary() {
        let avail = is_binary_available("__binary_that_does_not_exist_lcp__");
        assert!(!avail);
    }
}
