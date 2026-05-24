/// Indentation and colon preprocessor for Vajra
/// Translates Python-like indentation syntax into standard brace-based syntax.

pub fn preprocess_source(source: &str) -> String {
    // Determine if the file is already brace-based by looking for curly braces outside of string literals and comments.
    let mut has_braces_outside = false;
    let mut in_double_quote = false;
    let mut in_single_quote = false;
    let mut in_backtick = false;
    let mut escaped = false;
    let mut in_comment = false;

    let chars: Vec<char> = source.chars().collect();
    let mut idx = 0;
    while idx < chars.len() {
        let c = chars[idx];
        if in_comment {
            if c == '\n' || c == '\r' {
                in_comment = false;
            }
            idx += 1;
            continue;
        }
        if escaped {
            escaped = false;
            idx += 1;
            continue;
        }
        if c == '\\' {
            escaped = true;
            idx += 1;
            continue;
        }
        match c {
            '#' if !in_double_quote && !in_single_quote && !in_backtick => {
                in_comment = true;
            }
            '"' if !in_single_quote && !in_backtick => {
                in_double_quote = !in_double_quote;
            }
            '\'' if !in_double_quote && !in_backtick => {
                in_single_quote = !in_single_quote;
            }
            '`' if !in_double_quote && !in_single_quote => {
                in_backtick = !in_backtick;
            }
            '{' | '}' if !in_double_quote && !in_single_quote && !in_backtick => {
                has_braces_outside = true;
                break;
            }
            _ => {}
        }
        idx += 1;
    }

    if has_braces_outside {
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
