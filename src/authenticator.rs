use base64::prelude::*;
use hyper::{body::Incoming, Request};

#[derive(Debug, Clone)]
pub struct Authenticator {
    accepted_values: Vec<Vec<u8>>,
}

impl Authenticator {
    pub fn new(bearer_tokens: Vec<String>, basic_auth_combos: Vec<String>) -> Self {
        Self {
            accepted_values: bearer_tokens
                .into_iter()
                .map(|token| format!("Bearer {token}"))
                .chain(
                    basic_auth_combos
                        .into_iter()
                        .map(|combo| format!("Basic {}", BASE64_STANDARD.encode(combo))),
                )
                .map(|auth| auth.into_bytes())
                .collect(),
        }
    }

    pub fn authenticate_request(&self, req: &Request<Incoming>) -> bool {
        if self.accepted_values.is_empty() {
            return true;
        }

        req.headers()
            .get("Authorization")
            .map(|authorization| {
                self.accepted_values
                    .iter()
                    .any(|value| value == authorization.as_bytes())
            })
            .unwrap_or(false)
    }
}
