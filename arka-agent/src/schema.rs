//! Strict parsing of one model decision. Fail closed (rule 4): anything that is
//! not valid JSON of the exact expected shape is rejected and nothing runs.

use serde_json::Value;

/// One decision from the model: call a tool, or give the final answer.
#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    Call { tool: String, args: Value },
    Final { answer: String },
}

/// Why a model output was rejected.
#[derive(Debug, PartialEq)]
pub enum Reject {
    NotJson(String),
    BadShape(String),
}

impl std::fmt::Display for Reject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Reject::NotJson(e) => write!(f, "output was not valid JSON: {e}"),
            Reject::BadShape(e) => write!(f, "output JSON had the wrong shape: {e}"),
        }
    }
}

/// Accept EXACTLY one of:
///   {"tool":"<name>","args":{ ... }}      (args optional, but must be an object)
///   {"final":"<text>"}
/// Everything else is rejected.
pub fn parse_step(s: &str) -> Result<Step, Reject> {
    let v: Value = serde_json::from_str(s.trim()).map_err(|e| Reject::NotJson(e.to_string()))?;
    let obj = v
        .as_object()
        .ok_or_else(|| Reject::BadShape("top-level value is not an object".into()))?;

    // `final` wins if both are present, but reject the ambiguous both-keys case.
    let has_final = obj.contains_key("final");
    let has_tool = obj.contains_key("tool");
    if has_final && has_tool {
        return Err(Reject::BadShape(
            "object has both `tool` and `final`".into(),
        ));
    }

    if let Some(fin) = obj.get("final") {
        let answer = fin
            .as_str()
            .ok_or_else(|| Reject::BadShape("`final` must be a string".into()))?;
        return Ok(Step::Final {
            answer: answer.to_owned(),
        });
    }

    if let Some(tool) = obj.get("tool") {
        let tool = tool
            .as_str()
            .ok_or_else(|| Reject::BadShape("`tool` must be a string".into()))?;
        if tool.is_empty() {
            return Err(Reject::BadShape("`tool` must not be empty".into()));
        }
        let args = obj
            .get("args")
            .cloned()
            .unwrap_or_else(|| Value::Object(Default::default()));
        if !args.is_object() {
            return Err(Reject::BadShape("`args` must be an object".into()));
        }
        return Ok(Step::Call {
            tool: tool.to_owned(),
            args,
        });
    }

    Err(Reject::BadShape("expected a `tool` or `final` key".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_final() {
        assert_eq!(
            parse_step(r#"{"final":"DNS-over-TLS is on."}"#).unwrap(),
            Step::Final {
                answer: "DNS-over-TLS is on.".into()
            }
        );
    }

    #[test]
    fn accepts_tool_call() {
        let s = parse_step(r#"{"tool":"system_status","args":{}}"#).unwrap();
        assert_eq!(
            s,
            Step::Call {
                tool: "system_status".into(),
                args: serde_json::json!({})
            }
        );
    }

    #[test]
    fn tool_args_default_to_empty_object() {
        let s = parse_step(r#"{"tool":"system_status"}"#).unwrap();
        assert_eq!(
            s,
            Step::Call {
                tool: "system_status".into(),
                args: serde_json::json!({})
            }
        );
    }

    #[test]
    fn rejects_non_json() {
        assert!(matches!(
            parse_step("turn off mac randomization"),
            Err(Reject::NotJson(_))
        ));
    }

    #[test]
    fn rejects_array() {
        assert!(matches!(parse_step("[1,2,3]"), Err(Reject::BadShape(_))));
    }

    #[test]
    fn rejects_non_object_args() {
        assert!(matches!(
            parse_step(r#"{"tool":"x","args":"nope"}"#),
            Err(Reject::BadShape(_))
        ));
    }

    #[test]
    fn rejects_both_keys() {
        assert!(matches!(
            parse_step(r#"{"tool":"x","final":"y"}"#),
            Err(Reject::BadShape(_))
        ));
    }

    #[test]
    fn rejects_empty_object() {
        assert!(matches!(parse_step("{}"), Err(Reject::BadShape(_))));
    }
}
