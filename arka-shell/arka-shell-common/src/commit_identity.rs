//! The name and email recorded on commits by VARA and Git. Setup and Settings
//! check them with the same rules as /usr/libexec/arkaos-oobe-apply, so a bad
//! value is caught where it is typed.

pub fn valid_name(n: &str) -> bool {
    !n.trim().is_empty() && n.chars().count() <= 100 && !n.chars().any(char::is_control)
}

pub fn valid_email(e: &str) -> bool {
    let mut parts = e.split('@');
    let ok = |p: Option<&str>| p.is_some_and(|p| !p.is_empty() && !p.chars().any(|c| c.is_whitespace() || c.is_control()));
    e.len() <= 254 && ok(parts.next()) && ok(parts.next()) && parts.next().is_none()
}

#[cfg(test)]
mod tests {
    #[test]
    fn names_and_emails_follow_the_helper_rules() {
        assert!(super::valid_name("Alice Example"));
        for bad in ["", "   ", "Ali\nce", &"x".repeat(101)] {
            assert!(!super::valid_name(bad), "{bad:?}");
        }
        assert!(super::valid_email("alice@example.org"));
        for bad in ["", "alice", "@example.org", "alice@", "a b@c.d", "a@b@c", "a@b\n"] {
            assert!(!super::valid_email(bad), "{bad:?}");
        }
    }
}
