/// Extract a bool value from a TOML item by key (inline or regular table).
///
/// Handles both inline table form (`dep = { workspace = true }`) and
/// regular table form (`[dep]\nworkspace = true`).
pub(crate) fn toml_bool(item: &toml_edit::Item, key: &str) -> Option<bool> {
    item.as_value()
        .and_then(|v| v.as_inline_table())
        .and_then(|t| t.get(key))
        .and_then(|v| v.as_bool())
        .or_else(|| {
            item.as_table()
                .and_then(|t| t.get(key))
                .and_then(|i| i.as_value())
                .and_then(|v| v.as_bool())
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use toml_edit::DocumentMut;

    fn parse_item(toml: &str) -> toml_edit::Item {
        let doc: DocumentMut = toml.parse().unwrap();
        doc["dep"].clone()
    }

    #[test]
    fn inline_table_true() {
        let item = parse_item("dep = { workspace = true }");
        assert_eq!(toml_bool(&item, "workspace"), Some(true));
    }

    #[test]
    fn inline_table_false() {
        let item = parse_item("dep = { workspace = false }");
        assert_eq!(toml_bool(&item, "workspace"), Some(false));
    }

    #[test]
    fn inline_table_missing_key() {
        let item = parse_item("dep = { version = \"1.0\" }");
        assert_eq!(toml_bool(&item, "workspace"), None);
    }

    #[test]
    fn regular_table_true() {
        let item = parse_item("[dep]\nworkspace = true\n");
        assert_eq!(toml_bool(&item, "workspace"), Some(true));
    }

    #[test]
    fn regular_table_false() {
        let item = parse_item("[dep]\nworkspace = false\n");
        assert_eq!(toml_bool(&item, "workspace"), Some(false));
    }
}
