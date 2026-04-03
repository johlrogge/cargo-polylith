use quote::ToTokens;
use syn::{Item, Visibility};

/// Represents a single public API item extracted from source code.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct PubApiItem {
    pub kind: &'static str, // "fn", "struct", "enum", "trait", "type", "const", "static", "use"
    pub signature: String,  // normalized signature (no body, no docs)
}

/// Result of comparing two API surfaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiDiff {
    Unchanged,
    InterfaceChanged,
    InternalsOnly,
}

/// Check whether a visibility is public (not pub(crate), not pub(super), not private).
fn is_pub(vis: &Visibility) -> bool {
    matches!(vis, Visibility::Public(_))
}

/// Format generic parameters as a string (e.g., "<T, U: Clone>").
fn format_generics(generics: &syn::Generics) -> String {
    if generics.params.is_empty() {
        return String::new();
    }
    let params: Vec<String> = generics
        .params
        .iter()
        .map(|p| p.to_token_stream().to_string())
        .collect();
    format!("<{}>", params.join(", "))
}

/// Format a type path for display.
fn format_type(ty: &syn::Type) -> String {
    ty.to_token_stream().to_string()
}

/// Format function arguments as a comma-separated list of types.
fn format_fn_args(inputs: &syn::punctuated::Punctuated<syn::FnArg, syn::token::Comma>) -> String {
    inputs
        .iter()
        .map(|arg| match arg {
            syn::FnArg::Receiver(r) => {
                let mut s = String::new();
                if r.reference.is_some() {
                    s.push('&');
                    if r.mutability.is_some() {
                        s.push_str("mut ");
                    }
                }
                s.push_str("self");
                s
            }
            syn::FnArg::Typed(pat_type) => format_type(&pat_type.ty),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Format return type.
fn format_return_type(ret: &syn::ReturnType) -> String {
    match ret {
        syn::ReturnType::Default => String::new(),
        syn::ReturnType::Type(_, ty) => format!(" -> {}", format_type(ty)),
    }
}

/// Format a single struct field signature (for pub fields).
fn format_struct_field(field: &syn::Field) -> Option<String> {
    if !is_pub(&field.vis) {
        return None;
    }
    let name = field.ident.as_ref().map(|i| i.to_string()).unwrap_or_default();
    Some(format!("{}: {}", name, format_type(&field.ty)))
}

/// Format enum variant.
fn format_variant(variant: &syn::Variant) -> String {
    let name = variant.ident.to_string();
    match &variant.fields {
        syn::Fields::Named(fields) => {
            let field_strs: Vec<String> = fields
                .named
                .iter()
                .map(|f| {
                    let fname = f.ident.as_ref().map(|i| i.to_string()).unwrap_or_default();
                    format!("{}: {}", fname, format_type(&f.ty))
                })
                .collect();
            format!("{} {{ {} }}", name, field_strs.join(", "))
        }
        syn::Fields::Unnamed(fields) => {
            let types: Vec<String> = fields.unnamed.iter().map(|f| format_type(&f.ty)).collect();
            format!("{}({})", name, types.join(", "))
        }
        syn::Fields::Unit => name,
    }
}

/// Format a trait method signature.
fn format_trait_method(method: &syn::TraitItemFn) -> String {
    let name = method.sig.ident.to_string();
    let args = format_fn_args(&method.sig.inputs);
    let ret = format_return_type(&method.sig.output);
    format!("fn {name}({args}){ret}")
}

/// Format a use tree for re-exports.
fn format_use_tree(tree: &syn::UseTree) -> String {
    match tree {
        syn::UseTree::Path(p) => format!("{}::{}", p.ident, format_use_tree(&p.tree)),
        syn::UseTree::Name(n) => n.ident.to_string(),
        syn::UseTree::Rename(r) => format!("{} as {}", r.ident, r.rename),
        syn::UseTree::Glob(_) => "*".to_string(),
        syn::UseTree::Group(g) => {
            let items: Vec<String> = g.items.iter().map(format_use_tree).collect();
            format!("{{{}}}", items.join(", "))
        }
    }
}

/// Extract all public API items from a Rust source string.
/// Returns empty vec on parse failure (graceful degradation).
pub fn extract_pub_api(source: &str) -> Vec<PubApiItem> {
    let file = match syn::parse_file(source) {
        Ok(f) => f,
        Err(_) => return vec![],
    };

    let mut items = vec![];

    for item in &file.items {
        match item {
            Item::Fn(f) if is_pub(&f.vis) => {
                let name = f.sig.ident.to_string();
                let generics = format_generics(&f.sig.generics);
                let args = format_fn_args(&f.sig.inputs);
                let ret = format_return_type(&f.sig.output);
                items.push(PubApiItem {
                    kind: "fn",
                    signature: format!("fn {name}{generics}({args}){ret}"),
                });
            }
            Item::Struct(s) if is_pub(&s.vis) => {
                let name = s.ident.to_string();
                let generics = format_generics(&s.generics);
                let pub_fields: Vec<String> = match &s.fields {
                    syn::Fields::Named(fields) => fields
                        .named
                        .iter()
                        .filter_map(format_struct_field)
                        .collect(),
                    syn::Fields::Unnamed(fields) => fields
                        .unnamed
                        .iter()
                        .enumerate()
                        .filter_map(|(i, f)| {
                            if is_pub(&f.vis) {
                                Some(format!("{}: {}", i, format_type(&f.ty)))
                            } else {
                                None
                            }
                        })
                        .collect(),
                    syn::Fields::Unit => vec![],
                };
                items.push(PubApiItem {
                    kind: "struct",
                    signature: format!("struct {name}{generics} {{ {} }}", pub_fields.join(", ")),
                });
            }
            Item::Enum(e) if is_pub(&e.vis) => {
                let name = e.ident.to_string();
                let generics = format_generics(&e.generics);
                let variants: Vec<String> =
                    e.variants.iter().map(format_variant).collect();
                items.push(PubApiItem {
                    kind: "enum",
                    signature: format!("enum {name}{generics} {{ {} }}", variants.join(", ")),
                });
            }
            Item::Trait(t) if is_pub(&t.vis) => {
                let name = t.ident.to_string();
                let generics = format_generics(&t.generics);
                let methods: Vec<String> = t
                    .items
                    .iter()
                    .filter_map(|item| {
                        if let syn::TraitItem::Fn(m) = item {
                            Some(format_trait_method(m))
                        } else {
                            None
                        }
                    })
                    .collect();
                items.push(PubApiItem {
                    kind: "trait",
                    signature: format!("trait {name}{generics} {{ {} }}", methods.join(", ")),
                });
            }
            Item::Type(t) if is_pub(&t.vis) => {
                let name = t.ident.to_string();
                let generics = format_generics(&t.generics);
                let target = format_type(&t.ty);
                items.push(PubApiItem {
                    kind: "type",
                    signature: format!("type {name}{generics} = {target}"),
                });
            }
            Item::Const(c) if is_pub(&c.vis) => {
                let name = c.ident.to_string();
                let ty = format_type(&c.ty);
                items.push(PubApiItem {
                    kind: "const",
                    signature: format!("const {name}: {ty}"),
                });
            }
            Item::Static(s) if is_pub(&s.vis) => {
                let name = s.ident.to_string();
                let ty = format_type(&s.ty);
                items.push(PubApiItem {
                    kind: "static",
                    signature: format!("static {name}: {ty}"),
                });
            }
            Item::Use(u) if is_pub(&u.vis) => {
                let tree = format_use_tree(&u.tree);
                items.push(PubApiItem {
                    kind: "use",
                    signature: format!("use {tree}"),
                });
            }
            _ => {}
        }
    }

    items.sort();
    items
}

/// Compare two Rust source files and classify the diff.
pub fn compare_pub_api(old_source: &str, new_source: &str) -> ApiDiff {
    let old_api = extract_pub_api(old_source);
    let new_api = extract_pub_api(new_source);

    if old_api != new_api {
        return ApiDiff::InterfaceChanged;
    }

    // APIs are identical — check if the source text changed at all (ignoring leading/trailing whitespace)
    if old_source.trim() == new_source.trim() {
        ApiDiff::Unchanged
    } else {
        ApiDiff::InternalsOnly
    }
}

#[cfg(test)]
mod tests {
    use super::{ApiDiff, compare_pub_api, extract_pub_api};

    // 1. Adding a pub fn → InterfaceChanged
    #[test]
    fn adding_pub_fn_is_interface_changed() {
        let old = r#"pub fn foo() -> u32 { 42 }"#;
        let new = r#"pub fn foo() -> u32 { 42 }
pub fn bar() -> u32 { 0 }"#;
        assert_eq!(compare_pub_api(old, new), ApiDiff::InterfaceChanged);
    }

    // 2. Changing fn body only → InternalsOnly
    #[test]
    fn changing_fn_body_only_is_internals_only() {
        let old = r#"pub fn foo() -> u32 { 42 }"#;
        let new = r#"pub fn foo() -> u32 { 100 }"#;
        assert_eq!(compare_pub_api(old, new), ApiDiff::InternalsOnly);
    }

    // 3. Changing pub struct field type → InterfaceChanged
    #[test]
    fn changing_pub_struct_field_type_is_interface_changed() {
        let old = r#"pub struct Foo { pub x: u32 }"#;
        let new = r#"pub struct Foo { pub x: u64 }"#;
        assert_eq!(compare_pub_api(old, new), ApiDiff::InterfaceChanged);
    }

    // 4. Adding pub enum variant → InterfaceChanged
    #[test]
    fn adding_pub_enum_variant_is_interface_changed() {
        let old = r#"pub enum Color { Red, Green }"#;
        let new = r#"pub enum Color { Red, Green, Blue }"#;
        assert_eq!(compare_pub_api(old, new), ApiDiff::InterfaceChanged);
    }

    // 5. Changing private fn → InternalsOnly
    #[test]
    fn changing_private_fn_is_internals_only() {
        let old = r#"fn hidden() -> u32 { 1 }"#;
        let new = r#"fn hidden() -> u32 { 2 }"#;
        assert_eq!(compare_pub_api(old, new), ApiDiff::InternalsOnly);
    }

    // 6. pub(crate) fn change → InternalsOnly
    #[test]
    fn pub_crate_fn_change_is_internals_only() {
        let old = r#"pub(crate) fn helper() -> bool { true }"#;
        let new = r#"pub(crate) fn helper() -> bool { false }"#;
        assert_eq!(compare_pub_api(old, new), ApiDiff::InternalsOnly);
    }

    // 7. Parse failure → treat as empty API (graceful degradation)
    #[test]
    fn parse_failure_returns_empty_api() {
        let broken = "this is not valid rust !!!";
        let items = extract_pub_api(broken);
        assert!(items.is_empty(), "expected empty on parse failure, got: {items:?}");
    }

    // Parse failure in comparison — two broken files: both empty API, source different → InternalsOnly
    #[test]
    fn parse_failure_in_comparison_graceful() {
        let old = "this is not valid rust !!!";
        let new = "also not valid rust ???";
        // Both parse to empty API (equal), sources differ → InternalsOnly
        assert_eq!(compare_pub_api(old, new), ApiDiff::InternalsOnly);
    }

    // 8. pub use re-export added → InterfaceChanged
    #[test]
    fn adding_pub_use_is_interface_changed() {
        let old = r#"pub fn foo() {}"#;
        let new = r#"pub fn foo() {}
pub use std::collections::HashMap;"#;
        assert_eq!(compare_pub_api(old, new), ApiDiff::InterfaceChanged);
    }

    // 9. Doc comment change only → InternalsOnly (or Unchanged)
    #[test]
    fn doc_comment_change_is_internals_only_or_unchanged() {
        let old = r#"/// Old doc
pub fn foo() -> u32 { 42 }"#;
        let new = r#"/// New doc with more detail
pub fn foo() -> u32 { 42 }"#;
        let result = compare_pub_api(old, new);
        assert!(
            result == ApiDiff::InternalsOnly || result == ApiDiff::Unchanged,
            "expected InternalsOnly or Unchanged for doc-only change, got {result:?}"
        );
    }

    // 10. Identical source → Unchanged
    #[test]
    fn identical_source_is_unchanged() {
        let source = r#"pub fn foo() -> u32 { 42 }
pub struct Bar { pub x: i32 }
"#;
        assert_eq!(compare_pub_api(source, source), ApiDiff::Unchanged);
    }

    // Extra: pub(super) fn change → InternalsOnly (like pub(crate))
    #[test]
    fn pub_super_fn_change_is_internals_only() {
        let old = r#"pub(super) fn helper() {}"#;
        let new = r#"pub(super) fn helper() { let _x = 1; }"#;
        assert_eq!(compare_pub_api(old, new), ApiDiff::InternalsOnly);
    }

    // Extra: empty files are Unchanged
    #[test]
    fn empty_files_are_unchanged() {
        assert_eq!(compare_pub_api("", ""), ApiDiff::Unchanged);
    }
}
