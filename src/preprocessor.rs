/// Indentation and colon preprocessor for Vajra
/// Translates Python-like indentation syntax into standard brace-based syntax.

pub fn preprocess_source(source: &str) -> String {
    // If the file contains curly braces, assume it is already brace-based and return as is.
    if source.contains('{') || source.contains('}') {
        return source.to_string();
    }

    let mut result = String::new();
    let mut indent_stack = vec![0];
    let mut lines: Vec<String> = source.lines().map(|s| s.to_string()).collect();

    // First pass: replace colons at the end of block-starting lines with open braces '{'
    for i in 0..lines.len() {
        let line = &lines[i];
        let trimmed = line.trim();

        // Skip comments and empty lines
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // Strip comments for suffix check
        let code_only = if let Some(idx) = trimmed.find('#') {
            &trimmed[..idx]
        } else {
            trimmed
        };
        let code_trimmed = code_only.trim_end();

        if code_trimmed.ends_with(':') {
            // Check if the colon is at the end. Replace it with '{'
            let raw_trimmed = line.trim_end();
            if raw_trimmed.ends_with(':') {
                let prefix = &raw_trimmed[..raw_trimmed.len() - 1];
                lines[i] = format!("{}{{", prefix);
            }
        }
    }

    // Second pass: insert closing braces '}' when indentation level decreases
    for i in 0..lines.len() {
        let line = &lines[i];
        let trimmed = line.trim();

        // Skip empty lines or lines that are comments
        if trimmed.is_empty() || trimmed.starts_with('#') {
            result.push_str(line);
            result.push('\n');
            continue;
        }

        // Count leading spaces
        let mut leading_spaces = 0;
        for c in line.chars() {
            if c == ' ' {
                leading_spaces += 1;
            } else if c == '\t' {
                leading_spaces += 4;
            } else {
                break;
            }
        }

        // If the line starts with a closing brace, don't count it for dedent insertion
        if trimmed.starts_with('}') {
            result.push_str(line);
            result.push('\n');
            continue;
        }

        let current_indent = leading_spaces;
        let mut top = *indent_stack.last().unwrap();

        if current_indent > top {
            indent_stack.push(current_indent);
        } else {
            // Dedent: close open scopes
            while current_indent < top {
                indent_stack.pop();
                top = *indent_stack.last().unwrap_or(&0);
                
                // Add a closing brace before the current line (preserving indentation)
                let indent_str = " ".repeat(top);
                result.push_str(&format!("{}}}\n", indent_str));
            }
        }

        result.push_str(line);
        result.push('\n');
    }

    // Pop any remaining scopes
    while indent_stack.len() > 1 {
        indent_stack.pop();
        let top = *indent_stack.last().unwrap_or(&0);
        let indent_str = " ".repeat(top);
        result.push_str(&format!("{}}}\n", indent_str));
    }

    result
}
