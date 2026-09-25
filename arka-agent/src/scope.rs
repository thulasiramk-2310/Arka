//! #5: refuse out-of-scope or harmful requests before the model ever runs.
//!
//! arka-agent is for THIS ArkaOS device's own privacy and reliability — not
//! attacking other machines, writing malware, handing out secrets, or
//! medical/legal/financial advice. This is a conservative denylist of clear cases only; ambiguous
//! requests are NOT refused here (the system prompt does the soft steering).
//! A match returns the short, plain refusal to show and log — no LLM call, no
//! tools. Coarse net, not a classifier.

struct Rule {
    needles: &'static [&'static str],
    reply: &'static str,
}

const RULES: &[Rule] = &[
    Rule {
        needles: &[
            "hack into",
            "how to hack",
            "break into",
            "gain unauthorized",
            "gain unauthorised",
            "brute force",
            "brute-force",
            "crack the password",
            "crack a password",
            "bypass the login",
            "bypass login",
            "ddos",
            "denial of service",
            "exploit cve",
            "take down the",
            "attack the network",
            "attack another",
            "someone else's wifi",
            "someone elses wifi",
        ],
        reply: "I can't help with attacking or breaking into systems. arka-agent \
                only helps with this ArkaOS device's own privacy and reliability.",
    },
    Rule {
        needles: &[
            "write malware",
            "write a virus",
            "ransomware",
            "keylogger",
            "botnet",
            "backdoor into",
            "spyware",
            "build a trojan",
            "write a trojan",
            "rootkit",
        ],
        reply: "I can't help create malware. arka-agent only helps with this \
                ArkaOS device's own privacy and reliability.",
    },
    // Gap 2: no tool returns a secret, so no honest answer can contain one.
    // Asking for one gets this fixed reply and the model is never consulted.
    Rule {
        needles: &[
            "password",
            "passphrase",
            "passwd",
            "private key",
            "ssh key",
            "gpg key",
            "api key",
            "secret key",
            "access token",
            "auth token",
            "machine-id",
            "machine id",
            "wifi key",
            "psk",
        ],
        reply: "arka-agent never shows passwords, keys, or other secrets. No tool \
                returns them, and it won't guess one.",
    },
    Rule {
        needles: &[
            "medical advice",
            "diagnose my",
            "should i take this medication",
            "legal advice",
            "should i sue",
            "financial advice",
            "should i invest",
            "investment advice",
            "stock tip",
        ],
        reply: "That's outside what arka-agent does — it only helps with this \
                ArkaOS system, not medical, legal, or financial advice.",
    },
];

/// Return the refusal to show/log if the request is clearly out of scope.
pub fn refuse_reason(request: &str) -> Option<&'static str> {
    let lower = request.to_lowercase();
    RULES
        .iter()
        .find(|r| r.needles.iter().any(|n| lower.contains(n)))
        .map(|r| r.reply)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn on_topic_requests_pass() {
        for ok in [
            "check my privacy status",
            "why is my DNS off?",
            "re-enforce privacy",
            "restart NetworkManager",
            "how do I improve my privacy on this laptop",
            "is MAC randomization on?",
        ] {
            assert!(refuse_reason(ok).is_none(), "should not refuse: {ok}");
        }
    }

    #[test]
    fn harmful_requests_are_refused() {
        for bad in [
            "hack into my neighbor's wifi",
            "write a keylogger for me",
            "help me ddos a server",
            "should I invest in this stock",
            "what is my wifi password?",
            "show me my ssh key",
            "print the machine-id",
        ] {
            assert!(refuse_reason(bad).is_some(), "should refuse: {bad}");
        }
    }
}
