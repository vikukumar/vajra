/// Indentation and colon preprocessor for Vajra
/// Translates Python-like indentation syntax into standard brace-based syntax.

pub fn preprocess_source(source: &str) -> String {
    // Determine if the file is already brace-based by looking for curly braces outside of string literals and comments.
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
            _ => {}
        }
        idx += 1;
    }

    let mut result = String::new();
    let mut indent_stack = vec![0];
    let mut lines: Vec<String> = source.lines().map(|s| s.to_string()).collect();

    // First pass: replace colons at the end of block-starting lines with open braces '{'
    for line in &mut lines {
        let trimmed = line.trim();

        // Skip comments and empty lines
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("//") {
            continue;
        }

        // Strip comments for suffix check
        let code_only = strip_line_comment(trimmed);
        let code_trimmed = code_only.trim_end();

        if code_trimmed.ends_with(':') {
            // Check if the colon is at the end. Replace it with '{'
            let raw_trimmed = line.trim_end();
            if let Some(prefix) = raw_trimmed.strip_suffix(':') {
                *line = format!("{}{{", prefix);
            }
        }
    }

    // Second pass: insert closing braces '}' when indentation level decreases,
    // and pop from the stack if a line explicitly starts with a closing brace.
    for line in &lines {
        let trimmed = line.trim();

        // Skip empty lines or full-line comments
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("//") {
            if trimmed.is_empty() {
                result.push_str(line);
                result.push('\n');
            }
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

        // If the line starts with a closing brace, pop it from stack and don't count for dedent insertion
        if trimmed.starts_with('}') {
            if indent_stack.len() > 1 {
                indent_stack.pop();
            }
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

fn strip_line_comment(line: &str) -> &str {
    let mut in_double_quote = false;
    let mut in_single_quote = false;
    let mut escaped = false;
    let mut prev = '\0';

    for (idx, c) in line.char_indices() {
        if escaped {
            escaped = false;
            prev = c;
            continue;
        }
        if c == '\\' {
            escaped = true;
            prev = c;
            continue;
        }
        if c == '"' && !in_single_quote {
            in_double_quote = !in_double_quote;
        } else if c == '\'' && !in_double_quote {
            in_single_quote = !in_single_quote;
        } else if !in_double_quote && !in_single_quote {
            if c == '#' {
                return &line[..idx];
            }
            if prev == '/' && c == '/' {
                return &line[..idx - 1];
            }
        }
        prev = c;
    }

    line
}
