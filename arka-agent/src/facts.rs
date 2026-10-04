//! Facts and answer verification (spec #1, plus the overclaim filter of #2).
//!
//! The model never states a number, status, or verdict on its own. Read tools
//! emit typed `Fact`s into a per-session `FactStore`; when the model drafts a
//! final answer, `verify_answer` checks every state claim against the session's
//! facts. On any mismatch, an unbacked number, an unbacked status, or an
//! overclaim phrase, the model text is dropped and the caller shows a
//! deterministic template (or "I don't know") instead. Fail closed.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Status {
    On,
    Off,
}

/// One atomic, tool-produced truth for this session.
#[derive(Debug, Clone, Serialize)]
pub struct Fact {
    /// e.g. "mac","dns","hostname","ipv6","browser","score","cpu","mem","temp","health"
    pub domain: String,
    /// canonical rendered value shown to the user
    pub value: String,
    /// numeric tokens this fact authorises in an answer
    pub numbers: Vec<i64>,
    pub status: Option<Status>,
}

impl Fact {
    pub fn new(domain: &str, value: impl Into<String>) -> Self {
        Fact {
            domain: domain.into(),
            value: value.into(),
            numbers: vec![],
            status: None,
        }
    }
    pub fn num(mut self, n: i64) -> Self {
        self.numbers.push(n);
        self
    }
    pub fn nums(mut self, ns: Vec<i64>) -> Self {
        self.numbers.extend(ns);
        self
    }
    pub fn stat(mut self, s: Status) -> Self {
        self.status = Some(s);
        self
    }
}

#[derive(Default)]
pub struct FactStore {
    pub facts: Vec<Fact>,
}

impl FactStore {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn extend(&mut self, more: Vec<Fact>) {
        for fact in more {
            self.facts.retain(|old| old.domain != fact.domain);
            self.facts.push(fact);
        }
    }
    pub fn is_empty(&self) -> bool {
        self.facts.is_empty()
    }
    pub fn all_numbers(&self) -> Vec<i64> {
        self.facts.iter().flat_map(|f| f.numbers.clone()).collect()
    }
    /// Latest known status for a domain, if any.
    pub fn status_for(&self, domain: &str) -> Option<Status> {
        self.facts
            .iter()
            .rev()
            .find(|f| f.domain == domain)
            .and_then(|f| f.status)
    }
    /// The deterministic safe answer built only from facts we actually have.
    pub fn render(&self) -> String {
        if self.facts.is_empty() {
            return "I don't know — no tool reported that in this session.".into();
        }
        self.facts
            .iter()
            .map(|f| format!("{}: {}", f.domain, f.value))
            .collect::<Vec<_>>()
            .join(" · ")
    }
}

/// Overclaim phrases banned in any answer (#2). Lowercased substring match.
pub const OVERCLAIMS: &[&str] = &[
    "you are safe",
    "you're safe",
    "completely safe",
    "fully safe",
    "totally safe",
    "untrackable",
    "cannot be tracked",
    "can't be tracked",
    "cannot be traced",
    "anonymous",
    "fully private",
    "completely private",
    "totally private",
    "100%",
    "100 percent",
    "guaranteed",
    "unhackable",
    "impenetrable",
    "totally secure",
    "completely secure",
    "perfectly secure",
    "fully secure",
    "bulletproof",
    "no one can",
    "nobody can",
];

#[derive(Debug, PartialEq)]
pub enum Verdict {
    Ok,
    /// The model answer must be discarded; the string says why (for the log).
    Replace(String),
}

/// Free-form prose cannot be proven by keyword or cross-domain number matching.
/// Only the canonical tool-fact rendering or a fixed no-evidence response is
/// accepted. Other model prose is replaced by the caller with `store.render()`.
pub fn verify_answer(text: &str, store: &FactStore) -> Verdict {
    if text == store.render() || (store.is_empty() && text == "I don't have a tool that reports that.") {
        Verdict::Ok
    } else {
        Verdict::Replace("answer is not a canonical rendering of tool facts".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> FactStore {
        let mut s = FactStore::new();
        s.extend(vec![
            Fact::new("score", "100/100").num(100),
            Fact::new("mac", "randomized (on)").stat(Status::On),
            Fact::new("dns", "DoT active (Quad9 9.9.9.9)")
                .nums(vec![9])
                .stat(Status::On),
            Fact::new("temp", "51C").num(51),
        ]);
        s
    }

    #[test]
    fn canonical_answer_passes() {
        assert_eq!(
            verify_answer(&store().render(), &store()),
            Verdict::Ok
        );
    }

    #[test]
    fn rejects_invented_prose_and_cross_domain_numbers() {
        for text in ["Your disks are encrypted and firewall is configured.", "Privacy score is 51.", "IPv6 via Quad9."] {
            assert!(matches!(verify_answer(text, &store()), Verdict::Replace(_)));
        }
    }

    #[test]
    fn unbacked_number_is_replaced() {
        // 80 is standalone (degree sign separates it from C, as temps render);
        // the store only backs 51, so this must be dropped.
        assert!(matches!(
            verify_answer("Your temperature is 80\u{00b0}C.", &store()),
            Verdict::Replace(_)
        ));
    }

    #[test]
    fn wrong_status_is_replaced() {
        assert!(matches!(
            verify_answer("MAC randomization is off.", &store()),
            Verdict::Replace(_)
        ));
    }

    #[test]
    fn status_claim_without_fact_is_replaced() {
        let empty = FactStore::new();
        assert!(matches!(
            verify_answer("DNS-over-TLS is enabled.", &empty),
            Verdict::Replace(_)
        ));
    }

    #[test]
    fn overclaim_is_replaced() {
        assert!(matches!(
            verify_answer("You are completely safe and anonymous.", &store()),
            Verdict::Replace(_)
        ));
        assert!(matches!(
            verify_answer("This is 100% private.", &store()),
            Verdict::Replace(_)
        ));
    }

    #[test]
    fn idk_style_answer_passes() {
        let empty = FactStore::new();
        assert_eq!(
            verify_answer("I don't have a tool that reports that.", &empty),
            Verdict::Ok
        );
    }
}
