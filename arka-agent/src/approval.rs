//! Human-in-the-loop approval for write tools (rule 3). Shows a clear preview
//! and waits for an explicit yes on the terminal. Default is NO.

use std::io::{self, Write};

pub fn confirm(preview: &str) -> io::Result<bool> {
    println!("\n\x1b[1m● Proposed change — requires your approval\x1b[0m");
    println!("{preview}");
    print!("\nApply this change? [y/N]: ");
    io::stdout().flush()?;

    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    Ok(matches!(line.trim(), "y" | "Y" | "yes" | "YES"))
}
