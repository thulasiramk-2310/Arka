//! Signing in through greetd: the login manager starts the sign-in screen and listens on the
//! socket in `$GREETD_SOCK`. Each message is JSON after its length (4 bytes, this machine's byte
//! order). greetd runs PAM itself; the sign-in screen only answers its questions and names the
//! session to start. See greetd-ipc(7).

use serde_json::{json, Value};
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;

/// Why signing in did not work.
#[derive(Debug, PartialEq)]
pub enum Failure {
    /// The password was not accepted. PAM's own words, when it said something (an account
    /// locked after too many tries, say).
    Refused(Option<String>),
    /// Something else went wrong: no greetd, a session that would not start.
    Broken(String),
}

fn send(socket: &mut impl Write, message: &Value) -> std::io::Result<()> {
    let body = serde_json::to_vec(message)?;
    socket.write_all(&(body.len() as u32).to_ne_bytes())?;
    socket.write_all(&body)
}

fn receive(socket: &mut impl Read) -> std::io::Result<Value> {
    let mut length = [0u8; 4];
    socket.read_exact(&mut length)?;
    let length = u32::from_ne_bytes(length) as usize;
    if length > 1 << 20 {
        return Err(std::io::Error::other("greetd sent a message too long to be real"));
    }
    let mut body = vec![0u8; length];
    socket.read_exact(&mut body)?;
    Ok(serde_json::from_slice(&body)?)
}

fn broken(err: impl std::fmt::Display) -> Failure {
    Failure::Broken(err.to_string())
}

/// Check `password` for `user` and, when it is right, have greetd start `command` for them once
/// the sign-in screen has gone. Blocking: run it off the main thread.
pub fn sign_in(user: &str, password: &str, command: &[String], env: &[String]) -> Result<(), Failure> {
    let path = std::env::var("GREETD_SOCK").map_err(|_| broken("GREETD_SOCK is not set: not started by greetd"))?;
    let mut socket = UnixStream::connect(path).map_err(broken)?;
    talk(&mut socket, user, password, command, env)
}

fn talk<S: Read + Write>(socket: &mut S, user: &str, password: &str, command: &[String], env: &[String]) -> Result<(), Failure> {
    send(socket, &json!({ "type": "create_session", "username": user })).map_err(broken)?;
    let mut answered = false;
    let mut said: Option<String> = None;
    loop {
        let reply = receive(socket).map_err(broken)?;
        match reply["type"].as_str() {
            Some("success") => break,
            Some("auth_message") => {
                let text = reply["auth_message"].as_str().unwrap_or_default().trim().to_string();
                let answer = match reply["auth_message_type"].as_str() {
                    // Only one question is answered: a second one (a new password wanted, a code)
                    // is more than this screen asks for.
                    Some("secret") | Some("visible") if !answered => {
                        answered = true;
                        Value::String(password.to_string())
                    }
                    Some("secret") | Some("visible") => {
                        let _ = cancel(socket);
                        return Err(Failure::Broken(if text.is_empty() { "More than a password was asked for.".into() } else { text }));
                    }
                    _ => {
                        if !text.is_empty() {
                            said = Some(text);
                        }
                        Value::Null
                    }
                };
                send(socket, &json!({ "type": "post_auth_message_response", "response": answer })).map_err(broken)?;
            }
            Some("error") => {
                let _ = cancel(socket);
                return Err(if reply["error_type"].as_str() == Some("auth_error") {
                    Failure::Refused(said)
                } else {
                    Failure::Broken(reply["description"].as_str().unwrap_or("greetd refused").to_string())
                });
            }
            _ => return Err(broken("greetd answered something unexpected")),
        }
    }
    send(socket, &json!({ "type": "start_session", "cmd": command, "env": env })).map_err(broken)?;
    let reply = receive(socket).map_err(broken)?;
    match reply["type"].as_str() {
        Some("success") => Ok(()),
        _ => {
            let _ = cancel(socket);
            Err(Failure::Broken(reply["description"].as_str().unwrap_or("The session could not be started.").to_string()))
        }
    }
}

fn cancel<S: Read + Write>(socket: &mut S) -> std::io::Result<Value> {
    send(socket, &json!({ "type": "cancel_session" }))?;
    receive(socket)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    /// A greetd that answers from a script and keeps what it was sent.
    struct Fake {
        replies: VecDeque<u8>,
        sent: Vec<u8>,
    }

    impl Fake {
        fn new(replies: &[Value]) -> Fake {
            let mut bytes = Vec::new();
            for reply in replies {
                send(&mut bytes, reply).unwrap();
            }
            Fake { replies: bytes.into(), sent: Vec::new() }
        }
        fn requests(&self) -> Vec<Value> {
            let mut out = Vec::new();
            let mut rest = &self.sent[..];
            while !rest.is_empty() {
                out.push(receive(&mut rest).unwrap());
            }
            out
        }
    }

    impl Read for Fake {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            self.replies.read(buf)
        }
    }

    impl Write for Fake {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.sent.extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn a_right_password_starts_the_session() {
        let mut greetd = Fake::new(&[
            json!({"type": "auth_message", "auth_message_type": "secret", "auth_message": "Password: "}),
            json!({"type": "success"}),
            json!({"type": "success"}),
        ]);
        let cmd = vec!["/usr/libexec/arkawm/arkawm-session".to_string()];
        assert_eq!(talk(&mut greetd, "ram", "pw", &cmd, &[]), Ok(()));
        let sent = greetd.requests();
        assert_eq!(sent[0], json!({"type": "create_session", "username": "ram"}));
        assert_eq!(sent[1], json!({"type": "post_auth_message_response", "response": "pw"}));
        assert_eq!(sent[2]["type"], "start_session");
        assert_eq!(sent[2]["cmd"][0], "/usr/libexec/arkawm/arkawm-session");
    }

    #[test]
    fn a_wrong_password_is_refused_with_pams_words() {
        let mut greetd = Fake::new(&[
            json!({"type": "auth_message", "auth_message_type": "secret", "auth_message": "Password: "}),
            json!({"type": "auth_message", "auth_message_type": "error", "auth_message": "The account is locked for 10 minutes."}),
            json!({"type": "error", "error_type": "auth_error", "description": "pam_authenticate: AUTH_ERR"}),
            json!({"type": "success"}),
        ]);
        assert_eq!(talk(&mut greetd, "ram", "nope", &[], &[]), Err(Failure::Refused(Some("The account is locked for 10 minutes.".into()))));
        assert_eq!(greetd.requests().last().unwrap(), &json!({"type": "cancel_session"}));
    }
}
