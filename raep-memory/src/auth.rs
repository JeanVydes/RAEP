#![warn(missing_docs)]

use raep_domain::Scope;
use raep_ports::{Authorization, Authorizer};
use std::collections::HashMap;
use std::convert::Infallible;

/// In-memory allow-list authorizer.
#[derive(Clone, Debug, Default)]
pub struct MemoryAuthorizer {
    grants: HashMap<String, Vec<Scope>>,
}

impl MemoryAuthorizer {
    /// Grants one scope to a principal.
    pub fn grant(&mut self, principal: impl Into<String>, scope: Scope) {
        self.grants.entry(principal.into()).or_default().push(scope);
    }
}

impl Authorizer for MemoryAuthorizer {
    type Error = Infallible;

    fn authorize<'a>(
        &'a self,
        principal: &'a str,
        scope: &'a Scope,
    ) -> raep_ports::BoxFuture<'a, Result<Authorization, Self::Error>> {
        let allowed = self
            .grants
            .get(principal)
            .is_some_and(|scopes| scopes.iter().any(|granted| granted == scope));
        Box::pin(async move {
            Ok(if allowed {
                Authorization::Allowed
            } else {
                Authorization::Denied {
                    reason: "required scope was not granted".to_owned(),
                }
            })
        })
    }
}
