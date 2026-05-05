/// The result of a policy check.
#[derive(Debug, Clone)]
pub struct Decision {
    pub allowed: bool,
    pub reason: String,
    pub matched_rule: String,
}

impl Decision {
    pub fn allow(rule: impl Into<String>) -> Self {
        Self {
            allowed: true,
            reason: "allowed by policy".into(),
            matched_rule: rule.into(),
        }
    }

    pub fn deny(reason: impl Into<String>, rule: impl Into<String>) -> Self {
        Self {
            allowed: false,
            reason: reason.into(),
            matched_rule: rule.into(),
        }
    }

    pub fn deny_default() -> Self {
        Self {
            allowed: false,
            reason: "no matching allow rule (deny-by-default)".into(),
            matched_rule: "implicit-deny".into(),
        }
    }
}
