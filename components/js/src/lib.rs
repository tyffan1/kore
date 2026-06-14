use std::cell::RefCell;
use std::collections::HashMap;

use boa_engine::error::JsError as BoaJsError;
use boa_engine::native_function::NativeFunction;
use boa_engine::object::JsObject;
use boa_engine::property::PropertyKey;
use boa_engine::string::JsString;
use boa_engine::{Context, JsValue as BoaValue, Source};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq)]
pub enum JsValue {
    Undefined,
    Null,
    Bool(bool),
    Int(i32),
    Float(f64),
    String(String),
    Array(Vec<JsValue>),
    Object(HashMap<String, JsValue>),
}

#[derive(Debug, Error)]
pub enum JsError {
    #[error("JS context error: {0}")]
    Context(String),
    #[error("JS execution error: {0}")]
    Execution(String),
}

impl From<BoaJsError> for JsError {
    fn from(e: BoaJsError) -> Self {
        JsError::Execution(e.to_string())
    }
}

pub struct JsRuntime {
    context: RefCell<Context>,
}

impl JsRuntime {
    pub fn new() -> Result<Self, JsError> {
        let context = Context::default();
        let rt = Self {
            context: RefCell::new(context),
        };
        rt.init_bindings()?;
        Ok(rt)
    }

    pub fn eval(&self, code: &str) -> Result<JsValue, JsError> {
        let mut ctx = self.context.borrow_mut();
        let source = Source::from_bytes(code);
        let result = ctx.eval(source).map_err(JsError::from)?;
        Ok(boa_to_our_value(&result, &mut ctx))
    }

    fn init_bindings(&self) -> Result<(), JsError> {
        let mut ctx = self.context.borrow_mut();

        let console_log = NativeFunction::from_fn_ptr(|_, args, context| {
            let parts: Vec<String> = args.iter().map(|v| boa_debug_value(v, context)).collect();
            eprintln!("[JS] {}", parts.join(" "));
            Ok(BoaValue::Undefined)
        });
        ctx.register_global_callable(JsString::from("__console_log"), 1, console_log)
            .map_err(|e| JsError::Context(e.to_string()))?;

        let doc_set_title = NativeFunction::from_fn_ptr(|_, args, _| {
            if let Some(title) = args.first() {
                if let Some(s) = title.as_string() {
                    eprintln!("[JS] document.title = '{}'", s.to_std_string_escaped());
                }
            }
            Ok(BoaValue::Undefined)
        });
        ctx.register_global_callable(JsString::from("__document_set_title"), 1, doc_set_title)
            .map_err(|e| JsError::Context(e.to_string()))?;

        ctx.eval(Source::from_bytes(
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
        ))
        .map_err(|e| JsError::Context(e.to_string()))?;

        Ok(())
    }
}

fn boa_to_our_value(val: &BoaValue, context: &mut Context) -> JsValue {
    match val {
        BoaValue::Undefined => JsValue::Undefined,
        BoaValue::Null => JsValue::Null,
        BoaValue::Boolean(b) => JsValue::Bool(*b),
        BoaValue::Integer(i) => JsValue::Int(*i),
        BoaValue::Rational(f) => JsValue::Float(*f),
        BoaValue::String(s) => JsValue::String(s.to_std_string_escaped()),
        BoaValue::Object(obj) => convert_object(obj, context),
        _ => JsValue::Undefined,
    }
}

fn convert_object(obj: &JsObject, context: &mut Context) -> JsValue {
    if obj.is_array() {
        convert_array(obj, context)
    } else if obj.is_callable() {
        JsValue::Undefined
    } else {
        convert_plain_object(obj, context)
    }
}

fn convert_array(obj: &JsObject, context: &mut Context) -> JsValue {
    let length_val = match obj
        .get(PropertyKey::from(JsString::from("length")), context)
    {
        Ok(v) => match v.as_number() {
            Some(n) => n as usize,
            None => 0,
        },
        Err(_) => 0,
    };
    let mut items = Vec::with_capacity(length_val);
    for i in 0..length_val {
        let elem = match obj.get(i as u32, context) {
            Ok(v) => boa_to_our_value(&v, context),
            Err(_) => JsValue::Undefined,
        };
        items.push(elem);
    }
    JsValue::Array(items)
}

fn convert_plain_object(obj: &JsObject, context: &mut Context) -> JsValue {
    let mut map = HashMap::new();
    if let Ok(keys) = obj.own_property_keys(context) {
        for key in keys {
            let key_str = match &key {
                PropertyKey::String(s) => s.to_std_string_escaped(),
                PropertyKey::Index(i) => i.get().to_string(),
                _ => continue,
            };
            if let Ok(val) = obj.get(key, context) {
                map.insert(key_str, boa_to_our_value(&val, context));
            }
        }
    }
    JsValue::Object(map)
}

fn boa_debug_value(val: &BoaValue, context: &mut Context) -> String {
    match val {
        BoaValue::Undefined => "undefined".to_string(),
        BoaValue::Null => "null".to_string(),
        BoaValue::Boolean(b) => b.to_string(),
        BoaValue::Integer(i) => i.to_string(),
        BoaValue::Rational(f) => format!("{f}"),
        BoaValue::String(s) => s.to_std_string_escaped(),
        BoaValue::Object(obj) => {
            if obj.is_array() {
                let mut items = Vec::new();
                if let Ok(length) =
                    obj.get(PropertyKey::from(JsString::from("length")), context)
                {
                    if let Some(len) = length.as_number() {
                        let len = len as usize;
                        for i in 0..len {
                            if let Ok(elem) = obj.get(i as u32, context) {
                                items.push(boa_debug_value(&elem, context));
                            }
                        }
                    }
                }
                format!("[{}]", items.join(", "))
            } else if obj.is_callable() {
                "function".to_string()
            } else {
                "[object Object]".to_string()
            }
        }
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    type QJsValue = JsValue;

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
