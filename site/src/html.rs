use std::io;

/// Escape text before inserting it into an HTML attribute or text node.
pub fn escape(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => result.push_str("&amp;"),
            '<' => result.push_str("&lt;"),
            '>' => result.push_str("&gt;"),
            '"' => result.push_str("&quot;"),
            '\'' => result.push_str("&#39;"),
            _ => result.push(ch),
        }
    }
    result
}

/// Expand owned, trusted HTML partials. Insertions are not evaluated recursively.
/// Callers escape text values; markup values come only from checked-in templates.
pub fn expand(template: &str, values: &[(&str, String)]) -> io::Result<String> {
    let mut output = String::with_capacity(template.len());
    let mut remaining = template;
    while let Some(start) = remaining.find("{{") {
        output.push_str(&remaining[..start]);
        let after = &remaining[start + 2..];
        let end = after.find("}}").ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "unclosed template binding")
        })?;
        let name = after[..end].trim();
        let value = values.iter().find(|(key, _)| *key == name).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unknown template binding: {name}"),
            )
        })?;
        output.push_str(&value.1);
        remaining = &after[end + 2..];
    }
    output.push_str(remaining);
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_and_attributes_are_escaped() {
        assert_eq!(escape("<&\"'>é"), "&lt;&amp;&quot;&#39;&gt;é");
    }

    #[test]
    fn bindings_are_checked_and_inserted_only_once() {
        assert!(expand("{{missing}}", &[]).is_err());
        assert!(expand("{{unclosed", &[]).is_err());
        assert_eq!(
            expand("<p>{{text}}</p>", &[("text", "{{literal}}".into())]).unwrap(),
            "<p>{{literal}}</p>"
        );
    }
}
