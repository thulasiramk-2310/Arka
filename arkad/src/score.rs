use arka_shell_common::{BrowserSandbox, DnsStatus, SandboxStatus};
use crate::state::ArkadState;

// Weighted factors sum to 100.
// telemetry(5) + tracking(5) default true — ArkaOS ships no collectors.
// Remaining 90 points require active enforcement.
pub fn compute(s: &ArkadState) -> u8 {
    let mut n: u8 = 0;
    if s.mac_randomization                                      { n += 20; }
    if s.dns_status == DnsStatus::Encrypted                     { n += 25; }
    if s.hostname_privacy                                       { n += 10; }
    if s.ipv6_privacy                                           { n += 10; }
    if s.sandbox_status == SandboxStatus::Active                { n += 15; }
    if matches!(s.browser_sandbox,
        BrowserSandbox::Persistent
        | BrowserSandbox::Ephemeral
        | BrowserSandbox::PrivateWorkspace)                     { n += 10; }
    if s.telemetry_blocked                                      { n +=  5; }
    if s.tracking_blocked                                       { n +=  5; }
    n
}
