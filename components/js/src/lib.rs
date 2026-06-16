#![allow(unsafe_code)]

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use boa_engine::error::JsError as BoaJsError;
use boa_engine::native_function::NativeFunction;
use boa_engine::object::{FunctionObjectBuilder, JsObject};
use boa_engine::property::{Attribute, PropertyDescriptor, PropertyKey};
use boa_engine::string::JsString;
use boa_engine::{Context, JsValue as BoaValue, Source};
use kore_html::{Document, Element, NodeId, NodeKind};
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
    document: Arc<Mutex<Document>>,
}

impl JsRuntime {
    pub fn new(document: Arc<Mutex<Document>>) -> Result<Self, JsError> {
        let context = Context::default();
        let rt = Self {
            context: RefCell::new(context),
            document,
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

    pub fn document(&self) -> Arc<Mutex<Document>> {
        self.document.clone()
    }

    fn init_bindings(&self) -> Result<(), JsError> {
        let mut ctx = self.context.borrow_mut();
        let doc = self.document.clone();

        let console_log = NativeFunction::from_fn_ptr(|_, args, context| {
            let parts: Vec<String> = args.iter().map(|v| boa_debug_value(v, context)).collect();
            eprintln!("[JS] {}", parts.join(" "));
            Ok(BoaValue::Undefined)
        });
        ctx.register_global_callable(JsString::from("__console_log"), 1, console_log)
            .map_err(|e| JsError::Context(e.to_string()))?;

        ctx.eval(Source::from_bytes(
            r#"
var console = {
    log: function() { __console_log(Array.prototype.slice.call(arguments)); },
    error: function() { __console_log(Array.prototype.slice.call(arguments)); },
    warn: function() { __console_log(Array.prototype.slice.call(arguments)); },
};
"#,
        ))
        .map_err(|e| JsError::Context(e.to_string()))?;

        let document_obj = build_document_object(&mut ctx, &doc)?;
        ctx.register_global_property(JsString::from("document"), document_obj.clone(), Attribute::all())
            .map_err(|e| JsError::Context(e.to_string()))?;

        let window_obj = build_window_object(&mut ctx)?;
        ctx.register_global_property(JsString::from("window"), window_obj.clone(), Attribute::all())
            .map_err(|e| JsError::Context(e.to_string()))?;

        window_obj
            .set(JsString::from("document"), document_obj.clone(), false, &mut ctx)
            .map_err(|e| JsError::Context(e.to_string()))?;
        window_obj
            .set(JsString::from("self"), window_obj.clone(), false, &mut ctx)
            .map_err(|e| JsError::Context(e.to_string()))?;

        let loc_key = JsString::from("location");
        if let Ok(loc) = window_obj.get(loc_key, &mut ctx) {
            if let Some(loc_obj) = loc.as_object() {
                let reload_fn = FunctionObjectBuilder::new(
                    ctx.realm(),
                    NativeFunction::from_fn_ptr(|_, _, _| Ok(BoaValue::Undefined)),
                )
                .name("reload")
                .build();
                loc_obj.set(JsString::from("reload"), reload_fn, false, &mut ctx).ok();
            }
        }

        Ok(())
    }
}

// ============ Document Object ============

fn build_document_object(
    ctx: &mut Context,
    doc: &Arc<Mutex<Document>>,
) -> Result<JsObject, JsError> {
    let document_obj = JsObject::with_null_proto();

    document_obj
        .set(JsString::from("__kore_node_id"), 0i32, false, ctx)
        .map_err(|e| JsError::Context(e.to_string()))?;

    // document.title
    let title_getter = {
        let doc = doc.clone();
        unsafe {
            NativeFunction::from_closure(move |_, _, _| {
                let d = doc.lock().unwrap();
                Ok(BoaValue::String(JsString::from(find_title_text(&d))))
            })
        }
    };
    let title_setter = {
        let doc = doc.clone();
        unsafe {
            NativeFunction::from_closure(move |_, args, ctx| {
                let s = args
                    .first()
                    .map(|v| v.to_string(ctx).ok())
                    .flatten()
                    .map(|s| s.to_std_string_escaped())
                    .unwrap_or_default();
                let mut d = doc.lock().unwrap();
                set_title_text(&mut d, &s);
                Ok(BoaValue::Undefined)
            })
        }
    };
    let title_getter_obj = FunctionObjectBuilder::new(ctx.realm(), title_getter)
        .name("get title").length(0).build();
    let title_setter_obj = FunctionObjectBuilder::new(ctx.realm(), title_setter)
        .name("set title").length(1).build();
    document_obj
        .define_property_or_throw(
            JsString::from("title"),
            PropertyDescriptor::builder()
                .get(title_getter_obj)
                .set(title_setter_obj)
                .enumerable(true)
                .configurable(true)
                .build(),
            ctx,
        )
        .map_err(|e| JsError::Context(e.to_string()))?;

    // document.getElementById
    let get_element_by_id = {
        let doc = doc.clone();
        unsafe {
            NativeFunction::from_closure(move |_, args, ctx| {
                let id = args.first()
                    .and_then(|v| v.as_string())
                    .map(|s| s.to_std_string_escaped())
                    .unwrap_or_default();
                let d = doc.lock().unwrap();
                match find_element_by_id(&d, &id) {
                    Some(nid) => {
                        let el = create_element_object(&doc, nid, ctx);
                        Ok(el.map(BoaValue::Object).unwrap_or(BoaValue::Null))
                    }
                    None => Ok(BoaValue::Null),
                }
            })
        }
    };
    let fn_obj = FunctionObjectBuilder::new(ctx.realm(), get_element_by_id)
        .name("getElementById").length(1).build();
    document_obj.set(JsString::from("getElementById"), fn_obj, false, ctx).ok();

    // document.querySelector
    let query_selector = {
        let doc = doc.clone();
        unsafe {
            NativeFunction::from_closure(move |_, args, ctx| {
                let selector = args.first()
                    .and_then(|v| v.as_string())
                    .map(|s| s.to_std_string_escaped())
                    .unwrap_or_default();
                let d = doc.lock().unwrap();
                match find_element_by_selector(&d, d.root(), &selector) {
                    Some(nid) => {
                        let el = create_element_object(&doc, nid, ctx);
                        Ok(el.map(BoaValue::Object).unwrap_or(BoaValue::Null))
                    }
                    None => Ok(BoaValue::Null),
                }
            })
        }
    };
    let fn_obj = FunctionObjectBuilder::new(ctx.realm(), query_selector)
        .name("querySelector").length(1).build();
    document_obj.set(JsString::from("querySelector"), fn_obj, false, ctx).ok();

    // document.querySelectorAll
    let query_selector_all = {
        let doc = doc.clone();
        unsafe {
            NativeFunction::from_closure(move |_, args, ctx| {
                let selector = args.first()
                    .and_then(|v| v.as_string())
                    .map(|s| s.to_std_string_escaped())
                    .unwrap_or_default();
                let d = doc.lock().unwrap();
                let mut results: Vec<BoaValue> = Vec::new();
                find_all_by_selector(&d, d.root(), &selector, &mut |nid| {
                    if let Some(el) = create_element_object(&doc, nid, ctx) {
                        results.push(BoaValue::Object(el));
                    }
                });
                let nodelist = JsObject::with_null_proto();
                for (i, val) in results.iter().enumerate() {
                    nodelist.set(i as u32, val.clone(), false, ctx).ok();
                }
                nodelist.set(JsString::from("length"), results.len() as i32, false, ctx).ok();
                Ok(BoaValue::Object(nodelist))
            })
        }
    };
    let fn_obj = FunctionObjectBuilder::new(ctx.realm(), query_selector_all)
        .name("querySelectorAll").length(1).build();
    document_obj.set(JsString::from("querySelectorAll"), fn_obj, false, ctx).ok();

    // document.createElement
    let create_element_fn = {
        let doc = doc.clone();
        unsafe {
            NativeFunction::from_closure(move |_, args, ctx| {
                let tag = args.first()
                    .and_then(|v| v.as_string())
                    .map(|s| s.to_std_string_escaped())
                    .unwrap_or_default();
                let mut d = doc.lock().unwrap();
                let root_id = d.root();
                let nid = d.append(root_id, NodeKind::Element(Element {
                    tag_name: tag.to_uppercase(),
                    attributes: Vec::new(),
                }));
                drop(d);
                let el = create_element_object(&doc, nid, ctx);
                Ok(el.map(BoaValue::Object).unwrap_or(BoaValue::Null))
            })
        }
    };
    let fn_obj = FunctionObjectBuilder::new(ctx.realm(), create_element_fn)
        .name("createElement").length(1).build();
    document_obj.set(JsString::from("createElement"), fn_obj, false, ctx).ok();

    // document.createTextNode
    let create_text_node_fn = {
        let doc = doc.clone();
        unsafe {
            NativeFunction::from_closure(move |_, args, ctx| {
                let data = args.first()
                    .and_then(|v| v.as_string())
                    .map(|s| s.to_std_string_escaped())
                    .unwrap_or_default();
                let mut d = doc.lock().unwrap();
                let root_id = d.root();
                let nid = d.append(root_id, NodeKind::Text(data));
                drop(d);
                let el = create_element_object(&doc, nid, ctx);
                Ok(el.map(BoaValue::Object).unwrap_or(BoaValue::Null))
            })
        }
    };
    let fn_obj = FunctionObjectBuilder::new(ctx.realm(), create_text_node_fn)
        .name("createTextNode").length(1).build();
    document_obj.set(JsString::from("createTextNode"), fn_obj, false, ctx).ok();

    // document.body
    let body_getter = {
        let doc = doc.clone();
        unsafe {
            NativeFunction::from_closure(move |_, _, ctx| {
                let d = doc.lock().unwrap();
                match find_element_by_tag(&d, d.root(), "BODY") {
                    Some(nid) => {
                        let el = create_element_object(&doc, nid, ctx);
                        Ok(el.map(BoaValue::Object).unwrap_or(BoaValue::Null))
                    }
                    None => Ok(BoaValue::Null),
                }
            })
        }
    };
    let body_getter_obj = FunctionObjectBuilder::new(ctx.realm(), body_getter)
        .name("get body").length(0).build();
    document_obj
        .define_property_or_throw(
            JsString::from("body"),
            PropertyDescriptor::builder()
                .get(body_getter_obj)
                .enumerable(true)
                .configurable(true)
                .build(),
            ctx,
        )
        .map_err(|e| JsError::Context(e.to_string()))?;

    // document.head
    let head_getter = {
        let doc = doc.clone();
        unsafe {
            NativeFunction::from_closure(move |_, _, ctx| {
                let d = doc.lock().unwrap();
                match find_element_by_tag(&d, d.root(), "HEAD") {
                    Some(nid) => {
                        let el = create_element_object(&doc, nid, ctx);
                        Ok(el.map(BoaValue::Object).unwrap_or(BoaValue::Null))
                    }
                    None => Ok(BoaValue::Null),
                }
            })
        }
    };
    let head_getter_obj = FunctionObjectBuilder::new(ctx.realm(), head_getter)
        .name("get head").length(0).build();
    document_obj
        .define_property_or_throw(
            JsString::from("head"),
            PropertyDescriptor::builder()
                .get(head_getter_obj)
                .enumerable(true)
                .configurable(true)
                .build(),
            ctx,
        )
        .map_err(|e| JsError::Context(e.to_string()))?;

    // document.documentElement
    let doc_elem_getter = {
        let doc = doc.clone();
        unsafe {
            NativeFunction::from_closure(move |_, _, ctx| {
                let d = doc.lock().unwrap();
                match find_element_by_tag(&d, d.root(), "HTML") {
                    Some(nid) => {
                        let el = create_element_object(&doc, nid, ctx);
                        Ok(el.map(BoaValue::Object).unwrap_or(BoaValue::Null))
                    }
                    None => Ok(BoaValue::Null),
                }
            })
        }
    };
    let doc_elem_getter_obj = FunctionObjectBuilder::new(ctx.realm(), doc_elem_getter)
        .name("get documentElement").length(0).build();
    document_obj
        .define_property_or_throw(
            JsString::from("documentElement"),
            PropertyDescriptor::builder()
                .get(doc_elem_getter_obj)
                .enumerable(true)
                .configurable(true)
                .build(),
            ctx,
        )
        .map_err(|e| JsError::Context(e.to_string()))?;

    document_obj.set(JsString::from("cookie"), JsString::from(""), false, ctx)
        .map_err(|e| JsError::Context(e.to_string()))?;

    let location_obj = build_location_object(ctx);
    document_obj.set(JsString::from("location"), location_obj, false, ctx)
        .map_err(|e| JsError::Context(e.to_string()))?;

    Ok(document_obj)
}

// ============ Element Object ============

fn create_element_object(
    doc: &Arc<Mutex<Document>>,
    node_id: NodeId,
    ctx: &mut Context,
) -> Option<JsObject> {
    let locked = doc.lock().unwrap();
    let node = locked.node(node_id)?;
    let obj = JsObject::with_null_proto();

    let nid_key = JsString::from("__kore_node_id");
    obj.set(nid_key, node_id.0 as i32, false, ctx).ok()?;

    let nt_key = JsString::from("nodeType");
    let nn_key = JsString::from("nodeName");

    match &node.kind {
        NodeKind::Document => {
            obj.set(nt_key.clone(), 9i32, false, ctx).ok()?;
            obj.set(nn_key, JsString::from("#document"), false, ctx).ok()?;
            return Some(obj);
        }
        NodeKind::Element(el) => {
            obj.set(nt_key.clone(), 1i32, false, ctx).ok()?;
            obj.set(nn_key, JsString::from(el.tag_name.clone()), false, ctx).ok()?;
            obj.set(JsString::from("tagName"), JsString::from(el.tag_name.clone()), false, ctx).ok()?;

            let id_val = el.attributes.iter()
                .find(|a| a.name == "id")
                .map(|a| a.value.clone())
                .unwrap_or_default();
            let class_val = el.attributes.iter()
                .find(|a| a.name == "class")
                .map(|a| a.value.clone())
                .unwrap_or_default();

            obj.define_property_or_throw(
                JsString::from("id"),
                PropertyDescriptor::builder()
                    .value(JsString::from(id_val))
                    .writable(false)
                    .enumerable(true)
                    .configurable(true)
                    .build(),
                ctx,
            ).ok()?;

            obj.define_property_or_throw(
                JsString::from("className"),
                PropertyDescriptor::builder()
                    .value(JsString::from(class_val))
                    .writable(false)
                    .enumerable(true)
                    .configurable(true)
                    .build(),
                ctx,
            ).ok()?;

            // innerHTML
            let inner_getter = {
                let doc = doc.clone();
                let nid = node_id;
                unsafe {
                    NativeFunction::from_closure(move |_, _, _| {
                        let d = doc.lock().unwrap();
                        let html = get_inner_html(&d, nid);
                        Ok(BoaValue::String(JsString::from(html)))
                    })
                }
            };
            let inner_getter_obj = FunctionObjectBuilder::new(ctx.realm(), inner_getter)
                .name("get innerHTML").length(0).build();
            obj.define_property_or_throw(
                JsString::from("innerHTML"),
                PropertyDescriptor::builder()
                    .get(inner_getter_obj)
                    .enumerable(true)
                    .configurable(true)
                    .build(),
                ctx,
            ).ok()?;

            // textContent
            let text_getter = {
                let doc = doc.clone();
                let nid = node_id;
                unsafe {
                    NativeFunction::from_closure(move |_, _, _| {
                        let d = doc.lock().unwrap();
                        let text = get_text_content(&d, nid);
                        Ok(BoaValue::String(JsString::from(text)))
                    })
                }
            };
            let text_getter_obj = FunctionObjectBuilder::new(ctx.realm(), text_getter)
                .name("get textContent").length(0).build();
            obj.define_property_or_throw(
                JsString::from("textContent"),
                PropertyDescriptor::builder()
                    .get(text_getter_obj)
                    .enumerable(true)
                    .configurable(true)
                    .build(),
                ctx,
            ).ok()?;

            // getAttribute
            let get_attr = {
                let doc = doc.clone();
                let nid = node_id;
                unsafe {
                    NativeFunction::from_closure(move |_, args, _| {
                        let name = args.first()
                            .and_then(|v| v.as_string())
                            .map(|s| s.to_std_string_escaped())
                            .unwrap_or_default();
                        let d = doc.lock().unwrap();
                        match d.node(nid) {
                            Some(n) if let NodeKind::Element(el) = &n.kind => {
                                match el.attributes.iter().find(|a| a.name == name) {
                                    Some(attr) => Ok(BoaValue::String(JsString::from(attr.value.clone()))),
                                    None => Ok(BoaValue::Null),
                                }
                            }
                            _ => Ok(BoaValue::Null),
                        }
                    })
                }
            };
            let fn_obj = FunctionObjectBuilder::new(ctx.realm(), get_attr)
                .name("getAttribute").length(1).build();
            obj.set(JsString::from("getAttribute"), fn_obj, false, ctx).ok();

            // parentNode
            let parent_getter = {
                let doc = doc.clone();
                let nid = node_id;
                unsafe {
                    NativeFunction::from_closure(move |_, _, ctx| {
                        let d = doc.lock().unwrap();
                        match d.node(nid).and_then(|n| n.parent) {
                            Some(pid) => {
                                let el = create_element_object(&doc, pid, ctx);
                                Ok(el.map(BoaValue::Object).unwrap_or(BoaValue::Null))
                            }
                            None => Ok(BoaValue::Null),
                        }
                    })
                }
            };
            let parent_getter_obj = FunctionObjectBuilder::new(ctx.realm(), parent_getter)
                .name("get parentNode").length(0).build();
            obj.define_property_or_throw(
                JsString::from("parentNode"),
                PropertyDescriptor::builder()
                    .get(parent_getter_obj)
                    .enumerable(true)
                    .configurable(true)
                    .build(),
                ctx,
            ).ok()?;

            // children
            let children_getter = {
                let doc = doc.clone();
                let nid = node_id;
                unsafe {
                    NativeFunction::from_closure(move |_, _, ctx| {
                        let d = doc.lock().unwrap();
                        let children: Vec<NodeId> = d.node(nid)
                            .map(|n| n.children.clone())
                            .unwrap_or_default();
                        let arr = JsObject::with_null_proto();
                        let mut idx = 0usize;
                        for cid in children {
                            if let Some(el) = create_element_object(&doc, cid, ctx) {
                                arr.set(idx as u32, BoaValue::Object(el), false, ctx).ok();
                                idx += 1;
                            }
                        }
                        arr.set(JsString::from("length"), idx as i32, false, ctx).ok();
                        Ok(BoaValue::Object(arr))
                    })
                }
            };
            let children_getter_obj = FunctionObjectBuilder::new(ctx.realm(), children_getter)
                .name("get children").length(0).build();
            obj.define_property_or_throw(
                JsString::from("children"),
                PropertyDescriptor::builder()
                    .get(children_getter_obj)
                    .enumerable(true)
                    .configurable(true)
                    .build(),
                ctx,
            ).ok()?;

            // firstChild
            let first_child_getter = {
                let doc = doc.clone();
                let nid = node_id;
                unsafe {
                    NativeFunction::from_closure(move |_, _, ctx| {
                        let d = doc.lock().unwrap();
                        match d.node(nid).and_then(|n| n.children.first().copied()) {
                            Some(cid) => {
                                let el = create_element_object(&doc, cid, ctx);
                                Ok(el.map(BoaValue::Object).unwrap_or(BoaValue::Null))
                            }
                            None => Ok(BoaValue::Null),
                        }
                    })
                }
            };
            let first_child_getter_obj = FunctionObjectBuilder::new(ctx.realm(), first_child_getter)
                .name("get firstChild").length(0).build();
            obj.define_property_or_throw(
                JsString::from("firstChild"),
                PropertyDescriptor::builder()
                    .get(first_child_getter_obj)
                    .enumerable(true)
                    .configurable(true)
                    .build(),
                ctx,
            ).ok()?;

            // lastChild
            let last_child_getter = {
                let doc = doc.clone();
                let nid = node_id;
                unsafe {
                    NativeFunction::from_closure(move |_, _, ctx| {
                        let d = doc.lock().unwrap();
                        match d.node(nid).and_then(|n| n.children.last().copied()) {
                            Some(cid) => {
                                let el = create_element_object(&doc, cid, ctx);
                                Ok(el.map(BoaValue::Object).unwrap_or(BoaValue::Null))
                            }
                            None => Ok(BoaValue::Null),
                        }
                    })
                }
            };
            let last_child_getter_obj = FunctionObjectBuilder::new(ctx.realm(), last_child_getter)
                .name("get lastChild").length(0).build();
            obj.define_property_or_throw(
                JsString::from("lastChild"),
                PropertyDescriptor::builder()
                    .get(last_child_getter_obj)
                    .enumerable(true)
                    .configurable(true)
                    .build(),
                ctx,
            ).ok()?;

            // nextSibling
            let next_sibling_getter = {
                let doc = doc.clone();
                let nid = node_id;
                unsafe {
                    NativeFunction::from_closure(move |_, _, ctx| {
                        let d = doc.lock().unwrap();
                        let node = match d.node(nid) {
                            Some(n) => n,
                            None => return Ok(BoaValue::Null),
                        };
                        let parent_id = match node.parent {
                            Some(p) => p,
                            None => return Ok(BoaValue::Null),
                        };
                        let parent = match d.node(parent_id) {
                            Some(p) => p,
                            None => return Ok(BoaValue::Null),
                        };
                        let pos = match parent.children.iter().position(|c| *c == nid) {
                            Some(p) => p,
                            None => return Ok(BoaValue::Null),
                        };
                        if pos + 1 < parent.children.len() {
                            let sid = parent.children[pos + 1];
                            drop(d);
                            let el = create_element_object(&doc, sid, ctx);
                            Ok(el.map(BoaValue::Object).unwrap_or(BoaValue::Null))
                        } else {
                            Ok(BoaValue::Null)
                        }
                    })
                }
            };
            let next_sibling_getter_obj = FunctionObjectBuilder::new(ctx.realm(), next_sibling_getter)
                .name("get nextSibling").length(0).build();
            obj.define_property_or_throw(
                JsString::from("nextSibling"),
                PropertyDescriptor::builder()
                    .get(next_sibling_getter_obj)
                    .enumerable(true)
                    .configurable(true)
                    .build(),
                ctx,
            ).ok()?;

            // previousSibling
            let prev_sibling_getter = {
                let doc = doc.clone();
                let nid = node_id;
                unsafe {
                    NativeFunction::from_closure(move |_, _, ctx| {
                        let d = doc.lock().unwrap();
                        let node = match d.node(nid) {
                            Some(n) => n,
                            None => return Ok(BoaValue::Null),
                        };
                        let parent_id = match node.parent {
                            Some(p) => p,
                            None => return Ok(BoaValue::Null),
                        };
                        let parent = match d.node(parent_id) {
                            Some(p) => p,
                            None => return Ok(BoaValue::Null),
                        };
                        let pos = match parent.children.iter().position(|c| *c == nid) {
                            Some(p) => p,
                            None => return Ok(BoaValue::Null),
                        };
                        if pos > 0 {
                            let sid = parent.children[pos - 1];
                            drop(d);
                            let el = create_element_object(&doc, sid, ctx);
                            Ok(el.map(BoaValue::Object).unwrap_or(BoaValue::Null))
                        } else {
                            Ok(BoaValue::Null)
                        }
                    })
                }
            };
            let prev_sibling_getter_obj = FunctionObjectBuilder::new(ctx.realm(), prev_sibling_getter)
                .name("get previousSibling").length(0).build();
            obj.define_property_or_throw(
                JsString::from("previousSibling"),
                PropertyDescriptor::builder()
                    .get(prev_sibling_getter_obj)
                    .enumerable(true)
                    .configurable(true)
                    .build(),
                ctx,
            ).ok()?;

            // childElementCount
            let child_count_getter = {
                let doc = doc.clone();
                let nid = node_id;
                unsafe {
                    NativeFunction::from_closure(move |_, _, _| {
                        let d = doc.lock().unwrap();
                        let count = d.node(nid)
                            .map(|n| n.children.iter().filter(|cid| {
                                d.node(**cid).map(|c| matches!(c.kind, NodeKind::Element(_))).unwrap_or(false)
                            }).count())
                            .unwrap_or(0);
                        Ok(BoaValue::Integer(count as i32))
                    })
                }
            };
            let child_count_getter_obj = FunctionObjectBuilder::new(ctx.realm(), child_count_getter)
                .name("get childElementCount").length(0).build();
            obj.define_property_or_throw(
                JsString::from("childElementCount"),
                PropertyDescriptor::builder()
                    .get(child_count_getter_obj)
                    .enumerable(true)
                    .configurable(true)
                    .build(),
                ctx,
            ).ok()?;
        }
        NodeKind::Text(_text) => {
            obj.set(nt_key, 3i32, false, ctx).ok()?;
            obj.set(nn_key, JsString::from("#text"), false, ctx).ok()?;
            let val_getter = {
                let doc = doc.clone();
                let nid = node_id;
                unsafe {
                    NativeFunction::from_closure(move |_, _, _ctx| {
                        let d = doc.lock().unwrap();
                        let text = d.node(nid).and_then(|n| match &n.kind {
                            NodeKind::Text(t) => Some(t.clone()),
                            _ => None,
                        }).unwrap_or_default();
                        Ok(BoaValue::String(JsString::from(text)))
                    })
                }
            };
            let val_getter_obj = FunctionObjectBuilder::new(ctx.realm(), val_getter)
                .name("get nodeValue").length(0).build();
            obj.define_property_or_throw(
                JsString::from("nodeValue"),
                PropertyDescriptor::builder()
                    .get(val_getter_obj)
                    .enumerable(true)
                    .configurable(true)
                    .build(),
                ctx,
            ).ok()?;
            return Some(obj);
        }
        NodeKind::Comment(_) => {
            obj.set(nt_key, 8i32, false, ctx).ok()?;
            obj.set(nn_key, JsString::from("#comment"), false, ctx).ok()?;
            return Some(obj);
        }
        NodeKind::Doctype(_) => {
            obj.set(nt_key, 10i32, false, ctx).ok()?;
            obj.set(nn_key, JsString::from("html"), false, ctx).ok()?;
            return Some(obj);
        }
    }

    let add_el_fn = FunctionObjectBuilder::new(ctx.realm(), NativeFunction::from_fn_ptr(|_, _, _| Ok(BoaValue::Undefined)))
        .name("addEventListener").build();
    obj.set(JsString::from("addEventListener"), add_el_fn, false, ctx).ok();

    let style_obj = JsObject::with_null_proto();
    let set_prop_fn = FunctionObjectBuilder::new(ctx.realm(), NativeFunction::from_fn_ptr(|_, _, _| Ok(BoaValue::Undefined)))
        .name("setProperty").build();
    style_obj.set(JsString::from("setProperty"), set_prop_fn, false, ctx).ok();
    obj.set(JsString::from("style"), style_obj, false, ctx).ok();

    let class_list_obj = JsObject::with_null_proto();
    let class_add = FunctionObjectBuilder::new(ctx.realm(), NativeFunction::from_fn_ptr(|_, _, _| Ok(BoaValue::Undefined)))
        .name("add").build();
    let class_remove = FunctionObjectBuilder::new(ctx.realm(), NativeFunction::from_fn_ptr(|_, _, _| Ok(BoaValue::Undefined)))
        .name("remove").build();
    let class_contains = FunctionObjectBuilder::new(ctx.realm(), NativeFunction::from_fn_ptr(|_, _, _| Ok(BoaValue::Boolean(false))))
        .name("contains").build();
    let class_toggle = FunctionObjectBuilder::new(ctx.realm(), NativeFunction::from_fn_ptr(|_, _, _| Ok(BoaValue::Boolean(true))))
        .name("toggle").build();
    class_list_obj.set(JsString::from("add"), class_add, false, ctx).ok();
    class_list_obj.set(JsString::from("remove"), class_remove, false, ctx).ok();
    class_list_obj.set(JsString::from("contains"), class_contains, false, ctx).ok();
    class_list_obj.set(JsString::from("toggle"), class_toggle, false, ctx).ok();
    obj.set(JsString::from("classList"), class_list_obj, false, ctx).ok();

    obj.set(JsString::from("dataset"), JsObject::with_null_proto(), false, ctx).ok();

    Some(obj)
}

// ============ Window Object ============

fn build_window_object(ctx: &mut Context) -> Result<JsObject, JsError> {
    let win = JsObject::with_null_proto();

    let location_obj = build_location_object(ctx);
    win.set(JsString::from("location"), location_obj, false, ctx)
        .map_err(|e| JsError::Context(e.to_string()))?;

    let set_timeout_fn = FunctionObjectBuilder::new(ctx.realm(), NativeFunction::from_fn_ptr(|_, _, _| Ok(BoaValue::Integer(0))))
        .name("setTimeout").build();
    win.set(JsString::from("setTimeout"), set_timeout_fn, false, ctx)
        .map_err(|e| JsError::Context(e.to_string()))?;

    let set_interval_fn = FunctionObjectBuilder::new(ctx.realm(), NativeFunction::from_fn_ptr(|_, _, _| Ok(BoaValue::Integer(0))))
        .name("setInterval").build();
    win.set(JsString::from("setInterval"), set_interval_fn, false, ctx)
        .map_err(|e| JsError::Context(e.to_string()))?;

    let win_add_el_fn = FunctionObjectBuilder::new(ctx.realm(), NativeFunction::from_fn_ptr(|_, _, _| Ok(BoaValue::Undefined)))
        .name("addEventListener").build();
    win.set(JsString::from("addEventListener"), win_add_el_fn, false, ctx)
        .map_err(|e| JsError::Context(e.to_string()))?;

    let alert_fn = FunctionObjectBuilder::new(ctx.realm(), NativeFunction::from_fn_ptr(|_, args, context| {
        let msg = args.first().map(|v| boa_debug_value(v, context)).unwrap_or_default();
        eprintln!("[JS alert] {msg}");
        Ok(BoaValue::Undefined)
    })).name("alert").build();
    win.set(JsString::from("alert"), alert_fn, false, ctx)
        .map_err(|e| JsError::Context(e.to_string()))?;

    win.set(JsString::from("innerWidth"), 1024i32, false, ctx)
        .map_err(|e| JsError::Context(e.to_string()))?;
    win.set(JsString::from("innerHeight"), 768i32, false, ctx)
        .map_err(|e| JsError::Context(e.to_string()))?;

    let navigator = JsObject::with_null_proto();
    navigator.set(JsString::from("userAgent"), JsString::from("Mozilla/5.0 (compatible; Kore/0.1.0)"), false, ctx)
        .map_err(|e| JsError::Context(e.to_string()))?;
    win.set(JsString::from("navigator"), navigator, false, ctx)
        .map_err(|e| JsError::Context(e.to_string()))?;

    win.set(JsString::from("top"), win.clone(), false, ctx).ok();
    win.set(JsString::from("parent"), win.clone(), false, ctx).ok();

    Ok(win)
}

// ============ Location Object ============

fn build_location_object(ctx: &mut Context) -> JsObject {
    let loc = JsObject::with_null_proto();

    let hf = FunctionObjectBuilder::new(ctx.realm(), NativeFunction::from_fn_ptr(|_, _, _| Ok(BoaValue::String(JsString::from("about:blank")))))
        .name("get href").build();
    loc.define_property_or_throw(
        JsString::from("href"),
        PropertyDescriptor::builder().get(hf).enumerable(true).configurable(true).build(),
        ctx,
    ).ok();

    let hnf = FunctionObjectBuilder::new(ctx.realm(), NativeFunction::from_fn_ptr(|_, _, _| Ok(BoaValue::String(JsString::from("")))))
        .name("get hostname").build();
    loc.define_property_or_throw(
        JsString::from("hostname"),
        PropertyDescriptor::builder().get(hnf).enumerable(true).configurable(true).build(),
        ctx,
    ).ok();

    let pf = FunctionObjectBuilder::new(ctx.realm(), NativeFunction::from_fn_ptr(|_, _, _| Ok(BoaValue::String(JsString::from("/")))))
        .name("get pathname").build();
    loc.define_property_or_throw(
        JsString::from("pathname"),
        PropertyDescriptor::builder().get(pf).enumerable(true).configurable(true).build(),
        ctx,
    ).ok();

    loc
}

// ============ DOM Helper Functions ============

fn find_title_text(doc: &Document) -> String {
    if let Some(html_id) = find_element_by_tag(doc, doc.root(), "HTML") {
        if let Some(head_id) = find_element_by_tag(doc, html_id, "HEAD") {
            if let Some(head_node) = doc.node(head_id) {
                for cid in &head_node.children {
                    if let Some(node) = doc.node(*cid) {
                        if let NodeKind::Element(el) = &node.kind {
                            if el.tag_name == "TITLE" {
                                return get_text_content(doc, *cid);
                            }
                        }
                    }
                }
            }
        }
    }
    String::new()
}

fn set_title_text(doc: &mut Document, title: &str) {
    let root = doc.root();
    let html_id = find_element_by_tag(doc, root, "HTML")
        .unwrap_or_else(|| {
            doc.append(root, NodeKind::Element(Element {
                tag_name: "HTML".to_string(),
                attributes: Vec::new(),
            }))
        });
    let head_id = find_element_by_tag(doc, html_id, "HEAD")
        .unwrap_or_else(|| {
            doc.append(html_id, NodeKind::Element(Element {
                tag_name: "HEAD".to_string(),
                attributes: Vec::new(),
            }))
        });
    if let Some(head) = doc.node(head_id) {
        for cid in head.children.clone() {
            if let Some(node) = doc.node(cid) {
                if let NodeKind::Element(el) = &node.kind {
                    if el.tag_name == "TITLE" {
                        doc.append(cid, NodeKind::Text(title.to_string()));
                        return;
                    }
                }
            }
        }
    }
    let title_id = doc.append(head_id, NodeKind::Element(Element {
        tag_name: "TITLE".to_string(),
        attributes: Vec::new(),
    }));
    doc.append(title_id, NodeKind::Text(title.to_string()));
}

fn find_element_by_id(doc: &Document, id: &str) -> Option<NodeId> {
    for node in doc.nodes() {
        if let NodeKind::Element(el) = &node.kind {
            if el.attributes.iter().any(|a| a.name == "id" && a.value == id) {
                return Some(node.id);
            }
        }
    }
    None
}

fn find_element_by_tag(doc: &Document, start: NodeId, tag: &str) -> Option<NodeId> {
    let node = doc.node(start)?;
    if let NodeKind::Element(el) = &node.kind {
        if el.tag_name == tag {
            return Some(start);
        }
    }
    for cid in &node.children {
        if let Some(found) = find_element_by_tag(doc, *cid, tag) {
            return Some(found);
        }
    }
    None
}

fn find_element_by_selector(doc: &Document, start: NodeId, selector: &str) -> Option<NodeId> {
    let selector = selector.trim();
    if selector.is_empty() {
        return None;
    }
    for cid in doc.node(start)?.children.clone() {
        if element_matches_selector(doc, cid, selector) {
            return Some(cid);
        }
        if let Some(found) = find_element_by_selector(doc, cid, selector) {
            return Some(found);
        }
    }
    None
}

fn find_all_by_selector(
    doc: &Document,
    start: NodeId,
    selector: &str,
    callback: &mut impl FnMut(NodeId),
) {
    let selector = selector.trim();
    if selector.is_empty() {
        return;
    }
    if let Some(node) = doc.node(start) {
        for cid in node.children.clone() {
            if element_matches_selector(doc, cid, selector) {
                callback(cid);
            }
            find_all_by_selector(doc, cid, selector, callback);
        }
    }
}

fn element_matches_selector(doc: &Document, node_id: NodeId, selector: &str) -> bool {
    let selector = selector.trim();
    if selector.is_empty() {
        return false;
    }
    let node = match doc.node(node_id) {
        Some(n) => n,
        None => return false,
    };
    let el = match &node.kind {
        NodeKind::Element(el) => el,
        _ => return false,
    };

    let tag = &el.tag_name;
    let id = el.attributes.iter().find(|a| a.name == "id").map(|a| a.value.as_str()).unwrap_or("");
    let class = el.attributes.iter().find(|a| a.name == "class").map(|a| a.value.as_str()).unwrap_or("");

    if selector.starts_with('#') {
        return id == &selector[1..];
    }
    if selector.starts_with('.') {
        return class.split_whitespace().any(|c| c == &selector[1..]);
    }
    if selector.starts_with('[') {
        let inner = selector.trim_start_matches('[').trim_end_matches(']');
        if let Some(eq_pos) = inner.find('=') {
            let attr_name = inner[..eq_pos].trim();
            let attr_val = inner[eq_pos + 1..].trim().trim_matches(&['"', '\''][..]);
            return el.attributes.iter().any(|a| a.name == attr_name && a.value == attr_val);
        } else {
            return el.attributes.iter().any(|a| a.name == inner);
        }
    }
    if selector == "*" {
        return true;
    }

    if selector.contains('.') {
        let parts: Vec<&str> = selector.splitn(2, '.').collect();
        let target_tag = parts[0];
        let target_class = parts[1];
        if target_tag.is_empty() || target_tag == "*" {
            return class.split_whitespace().any(|c| c == target_class);
        }
        return *tag == target_tag.to_uppercase() && class.split_whitespace().any(|c| c == target_class);
    }
    if selector.contains('#') {
        let parts: Vec<&str> = selector.splitn(2, '#').collect();
        let target_tag = parts[0];
        let target_id = parts[1];
        if target_tag.is_empty() || target_tag == "*" {
            return id == target_id;
        }
        return *tag == target_tag.to_uppercase() && id == target_id;
    }

    *tag == selector.to_uppercase()
}

fn get_inner_html(doc: &Document, node_id: NodeId) -> String {
    let node = match doc.node(node_id) {
        Some(n) => n,
        None => return String::new(),
    };
    let mut html = String::new();
    for cid in &node.children {
        html.push_str(&get_outer_html(doc, *cid));
    }
    html
}

fn get_outer_html(doc: &Document, node_id: NodeId) -> String {
    let node = match doc.node(node_id) {
        Some(n) => n,
        None => return String::new(),
    };
    match &node.kind {
        NodeKind::Element(el) => {
            let mut html = format!("<{}", el.tag_name.to_lowercase());
            for attr in &el.attributes {
                html.push_str(&format!(" {}=\"{}\"", attr.name, attr.value));
            }
            html.push('>');
            for cid in &node.children {
                html.push_str(&get_outer_html(doc, *cid));
            }
            html.push_str(&format!("</{}>", el.tag_name.to_lowercase()));
            html
        }
        NodeKind::Text(t) => t.clone(),
        _ => String::new(),
    }
}

fn get_text_content(doc: &Document, node_id: NodeId) -> String {
    let node = match doc.node(node_id) {
        Some(n) => n,
        None => return String::new(),
    };
    let mut text = String::new();
    match &node.kind {
        NodeKind::Text(t) => text.push_str(t),
        NodeKind::Element(_) => {
            for cid in &node.children {
                text.push_str(&get_text_content(doc, *cid));
            }
        }
        _ => {}
    }
    text
}

// ============ Value Conversion ============

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
    let length_val = match obj.get(PropertyKey::from(JsString::from("length")), context) {
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
                if let Ok(length) = obj.get(PropertyKey::from(JsString::from("length")), context) {
                    if let Some(len) = length.as_number() {
                        for i in 0..len as usize {
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

    fn make_doc() -> Arc<Mutex<Document>> {
        Arc::new(Mutex::new(Document::new()))
    }

    #[test]
    fn eval_returns_undefined() -> Result<(), JsError> {
        let rt = JsRuntime::new(make_doc())?;
        let result = rt.eval("undefined")?;
        assert_eq!(result, QJsValue::Undefined);
        Ok(())
    }

    #[test]
    fn eval_returns_null() -> Result<(), JsError> {
        let rt = JsRuntime::new(make_doc())?;
        let result = rt.eval("null")?;
        assert_eq!(result, QJsValue::Null);
        Ok(())
    }

    #[test]
    fn eval_returns_bool() -> Result<(), JsError> {
        let rt = JsRuntime::new(make_doc())?;
        let result = rt.eval("true")?;
        assert_eq!(result, QJsValue::Bool(true));
        let result = rt.eval("false")?;
        assert_eq!(result, QJsValue::Bool(false));
        Ok(())
    }

    #[test]
    fn eval_returns_int() -> Result<(), JsError> {
        let rt = JsRuntime::new(make_doc())?;
        let result = rt.eval("42")?;
        assert_eq!(result, QJsValue::Int(42));
        Ok(())
    }

    #[test]
    fn eval_returns_float() -> Result<(), JsError> {
        let rt = JsRuntime::new(make_doc())?;
        let result = rt.eval("3.14")?;
        assert_eq!(result, QJsValue::Float(3.14));
        Ok(())
    }

    #[test]
    fn eval_returns_string() -> Result<(), JsError> {
        let rt = JsRuntime::new(make_doc())?;
        let result = rt.eval("'hello'")?;
        assert_eq!(result, QJsValue::String("hello".to_string()));
        Ok(())
    }

    #[test]
    fn eval_arithmetic() -> Result<(), JsError> {
        let rt = JsRuntime::new(make_doc())?;
        let result = rt.eval("1 + 2")?;
        assert_eq!(result, QJsValue::Int(3));
        Ok(())
    }

    #[test]
    fn eval_array() -> Result<(), JsError> {
        let rt = JsRuntime::new(make_doc())?;
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
        let rt = JsRuntime::new(make_doc())?;
        let result = rt.eval("({a: 1, b: 'two'})")?;
        let mut expected = HashMap::new();
        expected.insert("a".to_string(), QJsValue::Int(1));
        expected.insert("b".to_string(), QJsValue::String("two".to_string()));
        assert_eq!(result, QJsValue::Object(expected));
        Ok(())
    }

    #[test]
    fn console_log_does_not_crash() -> Result<(), JsError> {
        let rt = JsRuntime::new(make_doc())?;
        let result = rt.eval("console.log('test', 42); 'ok'")?;
        assert_eq!(result, QJsValue::String("ok".to_string()));
        Ok(())
    }

    #[test]
    fn document_title_get_default() -> Result<(), JsError> {
        let rt = JsRuntime::new(make_doc())?;
        let result = rt.eval("document.title")?;
        assert_eq!(result, QJsValue::String(String::new()));
        Ok(())
    }

    #[test]
    fn document_title_set_and_get() -> Result<(), JsError> {
        let rt = JsRuntime::new(make_doc())?;
        rt.eval("document.title = 'Hello World'")?;
        let result = rt.eval("document.title")?;
        assert_eq!(result, QJsValue::String("Hello World".to_string()));
        Ok(())
    }

    #[test]
    fn document_get_element_by_id_returns_null() -> Result<(), JsError> {
        let rt = JsRuntime::new(make_doc())?;
        let result = rt.eval("document.getElementById('nonexistent')")?;
        assert_eq!(result, QJsValue::Null);
        Ok(())
    }

    #[test]
    fn eval_es2020_syntax() -> Result<(), JsError> {
        let rt = JsRuntime::new(make_doc())?;
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
