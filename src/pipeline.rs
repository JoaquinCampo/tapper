use anyhow::{bail, Result};

#[derive(Debug, Clone)]
pub struct Stage {
    pub index: usize,
    pub command: String,
    pub argv: Vec<String>,
}

#[derive(Debug)]
pub struct Pipeline {
    pub stages: Vec<Stage>,
    pub raw: String,
}

impl Pipeline {
    /// Parse a shell pipeline string into individual stages.
    /// Splits on unquoted, unescaped `|` characters.
    pub fn parse(input: &str) -> Result<Pipeline> {
        let raw = input.trim().to_string();
        if raw.is_empty() {
            bail!("empty pipeline");
        }

        let parts = split_on_pipes(&raw)?;
        let mut stages = Vec::new();

        for (i, part) in parts.iter().enumerate() {
            let command = part.trim().to_string();
            if command.is_empty() {
                bail!("empty command at stage {}", i + 1);
            }
            let argv = shell_split(&command)?;
            if argv.is_empty() {
                bail!("empty command at stage {}", i + 1);
            }
            stages.push(Stage {
                index: i,
                command,
                argv,
            });
        }

        Ok(Pipeline { stages, raw })
    }
}

/// Split a pipeline string on `|`, respecting quotes and escapes.
fn split_on_pipes(input: &str) -> Result<Vec<String>> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut chars = input.chars().peekable();
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut escaped = false;

    while let Some(c) = chars.next() {
        if escaped {
            current.push(c);
            escaped = false;
            continue;
        }

        match c {
            '\\' if !in_single_quote => {
                escaped = true;
                current.push(c);
            }
            '\'' if !in_double_quote => {
                in_single_quote = !in_single_quote;
                current.push(c);
            }
            '"' if !in_single_quote => {
                in_double_quote = !in_double_quote;
                current.push(c);
            }
            '|' if !in_single_quote && !in_double_quote => {
                parts.push(current.clone());
                current.clear();
            }
            _ => {
                current.push(c);
            }
        }
    }

    if in_single_quote {
        bail!("unterminated single quote");
    }
    if in_double_quote {
        bail!("unterminated double quote");
    }

    parts.push(current);
    Ok(parts)
}

/// Split a command string into argv, handling quotes and escapes.
fn shell_split(input: &str) -> Result<Vec<String>> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut chars = input.chars().peekable();
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut escaped = false;
    let mut has_content = false;

    while let Some(c) = chars.next() {
        if escaped {
            // In double quotes, only certain chars are special after backslash
            if in_double_quote && !matches!(c, '"' | '\\' | '$' | '`') {
                current.push('\\');
            }
            current.push(c);
            escaped = false;
            has_content = true;
            continue;
        }

        match c {
            '\\' if !in_single_quote => {
                escaped = true;
            }
            '\'' if !in_double_quote => {
                in_single_quote = !in_single_quote;
                has_content = true;
            }
            '"' if !in_single_quote => {
                in_double_quote = !in_double_quote;
                has_content = true;
            }
            ' ' | '\t' if !in_single_quote && !in_double_quote => {
                if has_content {
                    args.push(current.clone());
                    current.clear();
                    has_content = false;
                }
            }
            _ => {
                current.push(c);
                has_content = true;
            }
        }
    }

    if has_content {
        args.push(current);
    }

    Ok(args)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_pipeline() {
        let p = Pipeline::parse("cat file | grep pattern | sort").unwrap();
        assert_eq!(p.stages.len(), 3);
        assert_eq!(p.stages[0].command, "cat file");
        assert_eq!(p.stages[1].command, "grep pattern");
        assert_eq!(p.stages[2].command, "sort");
    }

    #[test]
    fn test_quoted_pipe() {
        let p = Pipeline::parse(r#"echo "hello | world" | cat"#).unwrap();
        assert_eq!(p.stages.len(), 2);
        assert_eq!(p.stages[0].command, r#"echo "hello | world""#);
    }

    #[test]
    fn test_single_command() {
        let p = Pipeline::parse("ls -la").unwrap();
        assert_eq!(p.stages.len(), 1);
        assert_eq!(p.stages[0].argv, vec!["ls", "-la"]);
    }

    #[test]
    fn test_empty_pipeline() {
        assert!(Pipeline::parse("").is_err());
    }

    #[test]
    fn test_argv_parsing() {
        let p = Pipeline::parse(r#"grep -E "pattern with spaces" file.txt"#).unwrap();
        assert_eq!(p.stages[0].argv, vec!["grep", "-E", "pattern with spaces", "file.txt"]);
    }
}
