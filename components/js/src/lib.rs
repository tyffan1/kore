use quick_js::{Context, ExecutionError, JsValue as QJsValue};
use thiserror::Error;

pub use quick_js::JsValue;

#[derive(Debug, Error)]
pub enum JsError {
    #[error("JS context error: {0}")]
    Context(String),
    #[error("JS execution error: {0}")]
    Execution(String),
}

impl From<quick_js::ContextError> for JsError {
    fn from(e: quick_js::ContextError) -> Self {
        JsError::Context(e.to_string())
    }
}

impl From<ExecutionError> for JsError {
    fn from(e: ExecutionError) -> Self {
        JsError::Execution(e.to_string())
    }
}

pub struct JsRuntime {
    context: Context,
}

impl JsRuntime {
    pub fn new() -> Result<Self, JsError> {
        let context = Context::new()?;
        let rt = Self { context };
        rt.init_bindings()?;
        Ok(rt)
    }

    pub fn eval(&self, code: &str) -> Result<QJsValue, JsError> {
        Ok(self.context.eval(code)?)
    }

    fn init_bindings(&self) -> Result<(), JsError> {
        self.context.add_callback("__console_log", |args: Vec<QJsValue>| {
            let parts: Vec<String> = args.iter().map(|v| js_value_debug(v)).collect();
            eprintln!("[JS] {}", parts.join(" "));
            QJsValue::Undefined
        })?;

        self.context.add_callback(
            "__document_set_title",
            |title: String| -> Result<QJsValue, String> {
                eprintln!("[JS] document.title = '{title}'");
                Ok(QJsValue::Undefined)
            },
        )?;

        self.context.eval(
            r#"
var console = {
    log: function() {
        __console_log(Array.prototype.slice.call(arguments));
    }
};

var document = (function() {
    var _title = '';
    return {
        get title() { return _title; },
        set title(v) { _title = String(v); __document_set_title(_title); },
        getElementById: function(id) { return null; }
    };
})();
"#,
        )?;

        Ok(())
    }
}

fn js_value_debug(val: &QJsValue) -> String {
    match val {
        QJsValue::Undefined => "undefined".to_string(),
        QJsValue::Null => "null".to_string(),
        QJsValue::Bool(b) => b.to_string(),
        QJsValue::Int(n) => n.to_string(),
        QJsValue::Float(f) => format!("{f}"),
        QJsValue::String(s) => s.clone(),
        QJsValue::Array(arr) => {
            let items: Vec<String> = arr.iter().map(|v| js_value_debug(v)).collect();
            format!("[{}]", items.join(", "))
        }
        QJsValue::Object(map) => {
            let items: Vec<String> = map
                .iter()
                .map(|(k, v)| format!("\"{k}\": {}", js_value_debug(v)))
                .collect();
            format!("{{{}}}", items.join(", "))
        }
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn eval_returns_undefined() -> Result<(), JsError> {
        let rt = JsRuntime::new()?;
        let result = rt.eval("undefined")?;
        assert_eq!(result, QJsValue::Undefined);
        Ok(())
    }

    #[test]
    fn eval_returns_null() -> Result<(), JsError> {
        let rt = JsRuntime::new()?;
        let result = rt.eval("null")?;
        assert_eq!(result, QJsValue::Null);
        Ok(())
    }

    #[test]
    fn eval_returns_bool() -> Result<(), JsError> {
        let rt = JsRuntime::new()?;
        let result = rt.eval("true")?;
        assert_eq!(result, QJsValue::Bool(true));
        let result = rt.eval("false")?;
        assert_eq!(result, QJsValue::Bool(false));
        Ok(())
    }

    #[test]
    fn eval_returns_int() -> Result<(), JsError> {
        let rt = JsRuntime::new()?;
        let result = rt.eval("42")?;
        assert_eq!(result, QJsValue::Int(42));
        Ok(())
    }

    #[test]
    fn eval_returns_float() -> Result<(), JsError> {
        let rt = JsRuntime::new()?;
        let result = rt.eval("3.14")?;
        assert_eq!(result, QJsValue::Float(3.14));
        Ok(())
    }

    #[test]
    fn eval_returns_string() -> Result<(), JsError> {
        let rt = JsRuntime::new()?;
        let result = rt.eval("'hello'")?;
        assert_eq!(result, QJsValue::String("hello".to_string()));
        Ok(())
    }

    #[test]
    fn eval_arithmetic() -> Result<(), JsError> {
        let rt = JsRuntime::new()?;
        let result = rt.eval("1 + 2")?;
        assert_eq!(result, QJsValue::Int(3));
        Ok(())
    }

    #[test]
    fn eval_array() -> Result<(), JsError> {
        let rt = JsRuntime::new()?;
        let result = rt.eval("[1, 2, 3]")?;
        assert_eq!(
            result,
            QJsValue::Array(vec![
                QJsValue::Int(1),
                QJsValue::Int(2),
                QJsValue::Int(3),
            ])
        );
        Ok(())
    }

    #[test]
    fn eval_object() -> Result<(), JsError> {
        let rt = JsRuntime::new()?;
        let result = rt.eval("({a: 1, b: 'two'})")?;
        let mut expected = HashMap::new();
        expected.insert("a".to_string(), QJsValue::Int(1));
        expected.insert("b".to_string(), QJsValue::String("two".to_string()));
        assert_eq!(result, QJsValue::Object(expected));
        Ok(())
    }

    #[test]
    fn console_log_does_not_crash() -> Result<(), JsError> {
        let rt = JsRuntime::new()?;
        let result = rt.eval("console.log('test', 42); 'ok'")?;
        assert_eq!(result, QJsValue::String("ok".to_string()));
        Ok(())
    }

    #[test]
    fn document_title_get_default() -> Result<(), JsError> {
        let rt = JsRuntime::new()?;
        let result = rt.eval("document.title")?;
        assert_eq!(result, QJsValue::String(String::new()));
        Ok(())
    }

    #[test]
    fn document_title_set_and_get() -> Result<(), JsError> {
        let rt = JsRuntime::new()?;
        rt.eval("document.title = 'Hello World'")?;
        let result = rt.eval("document.title")?;
        assert_eq!(result, QJsValue::String("Hello World".to_string()));
        Ok(())
    }

    #[test]
    fn document_get_element_by_id_returns_null() -> Result<(), JsError> {
        let rt = JsRuntime::new()?;
        let result = rt.eval("document.getElementById('nonexistent')")?;
        assert_eq!(result, QJsValue::Null);
        Ok(())
    }

    #[test]
    fn eval_es2020_syntax() -> Result<(), JsError> {
        let rt = JsRuntime::new()?;
        let result = rt.eval("const a = [1, 2, 3]; a.map(x => x * 2)")?;
        assert_eq!(
            result,
            QJsValue::Array(vec![
                QJsValue::Int(2),
                QJsValue::Int(4),
                QJsValue::Int(6),
            ])
        );
        Ok(())
    }
}
