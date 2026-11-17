//! Checks a password with PAM, the same way swaylock does: pam_authenticate on a lock service.

use std::ffi::{c_char, c_int, c_void, CString};
use std::path::Path;

const PAM_SUCCESS: c_int = 0;
const PAM_BUF_ERR: c_int = 5;
const PAM_CONV_ERR: c_int = 19;
const PAM_PROMPT_ECHO_OFF: c_int = 1;
const PAM_PROMPT_ECHO_ON: c_int = 2;
const PAM_ERROR_MSG: c_int = 3;
const PAM_TEXT_INFO: c_int = 4;

#[repr(C)]
struct Message {
    style: c_int,
    msg: *const c_char,
}

#[repr(C)]
struct Response {
    resp: *mut c_char,
    retcode: c_int,
}

#[repr(C)]
struct Conv {
    conv: extern "C" fn(c_int, *mut *const Message, *mut *mut Response, *mut c_void) -> c_int,
    data: *mut c_void,
}

#[link(name = "pam")]
extern "C" {
    fn pam_start(service: *const c_char, user: *const c_char, conv: *const Conv, handle: *mut *mut c_void) -> c_int;
    fn pam_authenticate(handle: *mut c_void, flags: c_int) -> c_int;
    fn pam_setcred(handle: *mut c_void, flags: c_int) -> c_int;
    fn pam_end(handle: *mut c_void, status: c_int) -> c_int;
}

extern "C" {
    fn calloc(n: usize, size: usize) -> *mut c_void;
    fn strdup(s: *const c_char) -> *mut c_char;
}

const PAM_REFRESH_CRED: c_int = 0x0010;

/// What the conversation needs: the password to give, and PAM's own messages to show (faillock
/// says here that the account is locked and for how long).
struct Talk {
    password: CString,
    said: Vec<String>,
}

/// Answers PAM's prompts: the password for hidden prompts, nothing for the others. Keeps PAM's
/// error and info messages.
extern "C" fn answer(count: c_int, msgs: *mut *const Message, out: *mut *mut Response, data: *mut c_void) -> c_int {
    if count <= 0 || msgs.is_null() || out.is_null() {
        return PAM_CONV_ERR;
    }
    // PAM frees the answers with free(), so they must come from the C allocator.
    let replies = unsafe { calloc(count as usize, std::mem::size_of::<Response>()) } as *mut Response;
    if replies.is_null() {
        return PAM_BUF_ERR;
    }
    let talk = unsafe { &mut *(data as *mut Talk) };
    for i in 0..count as usize {
        // Linux-PAM passes an array of pointers to messages.
        let msg = unsafe { *msgs.add(i) };
        let style = if msg.is_null() { 0 } else { unsafe { (*msg).style } };
        if style == PAM_PROMPT_ECHO_OFF || style == PAM_PROMPT_ECHO_ON {
            unsafe { (*replies.add(i)).resp = strdup(talk.password.as_ptr()) };
        } else if (style == PAM_ERROR_MSG || style == PAM_TEXT_INFO) && !unsafe { (*msg).msg }.is_null() {
            let text = unsafe { std::ffi::CStr::from_ptr((*msg).msg) }.to_string_lossy().trim().to_string();
            if !text.is_empty() {
                talk.said.push(text);
            }
        }
    }
    unsafe { *out = replies };
    PAM_SUCCESS
}

/// The PAM service: our own when the system has one, else swaylock's (the same rules), else login.
pub fn service() -> &'static str {
    ["arka-lock", "swaylock"].into_iter().find(|name| Path::new("/etc/pam.d").join(name).exists()).unwrap_or("login")
}

/// Ok when PAM accepts this password for this user; otherwise what PAM said, if anything.
/// Blocks for a while on a wrong one.
pub fn check(user: &str, password: &str) -> Result<(), Option<String>> {
    let (Ok(service), Ok(user), Ok(password)) = (CString::new(service()), CString::new(user), CString::new(password)) else {
        return Err(None);
    };
    let mut talk = Talk { password, said: Vec::new() };
    let conv = Conv { conv: answer, data: &mut talk as *mut Talk as *mut c_void };
    let mut handle = std::ptr::null_mut();
    if unsafe { pam_start(service.as_ptr(), user.as_ptr(), &conv, &mut handle) } != PAM_SUCCESS {
        return Err(None);
    }
    let status = unsafe { pam_authenticate(handle, 0) };
    if status == PAM_SUCCESS {
        // Renews Kerberos tickets and the like; a failure here does not keep the screen locked.
        unsafe { pam_setcred(handle, PAM_REFRESH_CRED) };
    }
    unsafe { pam_end(handle, status) };
    if status == PAM_SUCCESS {
        Ok(())
    } else {
        Err(talk.said.pop())
    }
}
