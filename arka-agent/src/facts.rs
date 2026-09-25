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
        self.facts.extend(more);
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

fn status_word(w: &str) -> Option<Status> {
    match w {
        "on" | "enabled" | "active" | "randomized" | "masked" | "enforcing" | "enforced"
        | "protected" => Some(Status::On),
        "off" | "disabled" | "inactive" | "unprotected" | "exposed" => Some(Status::Off),
        _ => None,
    }
}

/// Map a token to the fact domain it refers to.
fn domain_of(word: &str) -> Option<&'static str> {
    Some(match word {
        "mac" => "mac",
        "dns" | "dns-over-tls" | "dot" | "quad9" => "dns",
        "hostname" => "hostname",
        "ipv6" => "ipv6",
        "browser" | "sandbox" | "bubblewrap" | "firefox" => "browser",
        _ => return None,
    })
}

/// Standalone integers only — a digit run whose neighbours are not alphanumeric.
/// So "ipv6", "quad9", "sha256" contribute nothing; "51", "100", "9.9.9.9" do.
fn standalone_integers(text: &str) -> Vec<i64> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i].is_ascii_digit() {
            let start = i;
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
            let before_ok = start == 0 || !b[start - 1].is_ascii_alphanumeric();
            let after_ok = i == b.len() || !b[i].is_ascii_alphanumeric();
            if before_ok && after_ok {
                if let Ok(n) = text[start..i].parse::<i64>() {
                    out.push(n);
                }
            }
        } else {
            i += 1;
        }
    }
    out
}

/// Check a model final answer against the session facts. Fail closed.
pub fn verify_answer(text: &str, store: &FactStore) -> Verdict {
    let lower = text.to_lowercase();

    // #2: overclaims are never allowed, even if "true".
    for p in OVERCLAIMS {
        if lower.contains(p) {
            return Verdict::Replace(format!("overclaim phrase: '{p}'"));
        }
    }

    // Every standalone number must be authorised by some fact.
    let allowed = store.all_numbers();
    for n in standalone_integers(&lower) {
        if !allowed.contains(&n) {
            return Verdict::Replace(format!("unbacked number: {n}"));
        }
    }

    // Status claims must match a fact for that domain.
    let toks: Vec<&str> = lower
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '-')
        .filter(|s| !s.is_empty())
        .collect();
    for (i, t) in toks.iter().enumerate() {
        if let Some(dom) = domain_of(t) {
            let lo = i.saturating_sub(3);
            let hi = (i + 4).min(toks.len());
            for w in &toks[lo..hi] {
                if let Some(claimed) = status_word(w) {
                    match store.status_for(dom) {
                        Some(actual) if actual != claimed => {
                            return Verdict::Replace(format!(
                                "status mismatch on {dom}: said {claimed:?}, fact is {actual:?}"
                            ));
                        }
                        None => {
                            return Verdict::Replace(format!(
                                "status claim on {dom} with no supporting fact"
                            ));
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    Verdict::Ok
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
    fn backed_answer_passes() {
        assert_eq!(
            verify_answer("MAC randomization is on and the score is 100.", &store()),
            Verdict::Ok
        );
    }

    #[test]
    fn ipv6_and_quad9_are_not_treated_as_numbers() {
        // 'ipv6' and 'quad9' must not trip the number check
        assert_eq!(verify_answer("IPv6 via Quad9.", &store()), Verdict::Ok);
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
