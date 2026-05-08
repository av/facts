/// Boolean tag expression evaluator.
///
/// Supports: tag names, `and`, `or`, `not`, and parentheses.
/// Example: "mvp and not blocked"
/// Validate a tag expression, returning an error if it is malformed.
/// Call this once before the main loop to fail early on bad expressions.
pub fn validate_tag_expr(expr: &str) -> Result<(), String> {
    parse_expr(expr).map(|_| ())
}

/// Check if a set of tags matches a boolean expression.
pub fn matches_tag_expr(expr: &str, tags: &[String]) -> bool {
    match parse_expr(expr) {
        Ok(ast) => eval(&ast, tags),
        Err(_) => false,
    }
}

/// Check if a haystack string matches a boolean search expression.
///
/// Each term in the expression is matched case-insensitively as a substring
/// of the haystack. Supports the same operators as tag expressions: and, or,
/// not, and parentheses.
pub fn matches_search_expr(expr: &str, haystack: &str) -> bool {
    match parse_expr(expr) {
        Ok(ast) => eval_search(&ast, &haystack.to_ascii_lowercase()),
        Err(_) => false,
    }
}

fn eval_search(expr: &Expr, haystack: &str) -> bool {
    match expr {
        Expr::Tag(term) => haystack.contains(&term.to_ascii_lowercase()),
        Expr::Not(e) => !eval_search(e, haystack),
        Expr::And(a, b) => eval_search(a, haystack) && eval_search(b, haystack),
        Expr::Or(a, b) => eval_search(a, haystack) || eval_search(b, haystack),
    }
}

#[derive(Debug)]
enum Expr {
    Tag(String),
    Not(Box<Expr>),
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
}

fn eval(expr: &Expr, tags: &[String]) -> bool {
    match expr {
        Expr::Tag(name) => tags.iter().any(|t| t == name),
        Expr::Not(e) => !eval(e, tags),
        Expr::And(a, b) => eval(a, tags) && eval(b, tags),
        Expr::Or(a, b) => eval(a, tags) || eval(b, tags),
    }
}

/// Simple recursive descent parser for tag expressions.
fn parse_expr(input: &str) -> Result<Expr, String> {
    let tokens = tokenize(input);
    let (expr, rest) = parse_or(&tokens)?;
    if !rest.is_empty() {
        return Err(format!("unexpected tokens: {:?}", rest));
    }
    Ok(expr)
}

fn tokenize(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut chars = input.chars().peekable();

    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
            continue;
        }
        if c == '(' || c == ')' {
            tokens.push(c.to_string());
            chars.next();
            continue;
        }
        // Word
        let mut word = String::new();
        while let Some(&c) = chars.peek() {
            if c.is_whitespace() || c == '(' || c == ')' {
                break;
            }
            word.push(c);
            chars.next();
        }
        tokens.push(word);
    }

    tokens
}

fn parse_or(tokens: &[String]) -> Result<(Expr, &[String]), String> {
    let (mut left, mut rest) = parse_and(tokens)?;
    while !rest.is_empty() && rest[0] == "or" {
        let (right, new_rest) = parse_and(&rest[1..])?;
        left = Expr::Or(Box::new(left), Box::new(right));
        rest = new_rest;
    }
    Ok((left, rest))
}

fn parse_and(tokens: &[String]) -> Result<(Expr, &[String]), String> {
    let (mut left, mut rest) = parse_not(tokens)?;
    while !rest.is_empty() && rest[0] == "and" {
        let (right, new_rest) = parse_not(&rest[1..])?;
        left = Expr::And(Box::new(left), Box::new(right));
        rest = new_rest;
    }
    Ok((left, rest))
}

fn parse_not(tokens: &[String]) -> Result<(Expr, &[String]), String> {
    if tokens.is_empty() {
        return Err("unexpected end of expression".to_string());
    }
    if tokens[0] == "not" {
        let (expr, rest) = parse_not(&tokens[1..])?;
        return Ok((Expr::Not(Box::new(expr)), rest));
    }
    parse_primary(tokens)
}

fn parse_primary(tokens: &[String]) -> Result<(Expr, &[String]), String> {
    if tokens.is_empty() {
        return Err("unexpected end of expression".to_string());
    }
    if tokens[0] == "(" {
        let (expr, rest) = parse_or(&tokens[1..])?;
        if rest.is_empty() || rest[0] != ")" {
            return Err("missing closing parenthesis".to_string());
        }
        return Ok((expr, &rest[1..]));
    }
    // Tag name
    let tag = tokens[0].clone();
    Ok((Expr::Tag(tag), &tokens[1..]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(names: &[&str]) -> Vec<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn test_simple_tag() {
        assert!(matches_tag_expr("mvp", &tags(&["mvp", "core"])));
        assert!(!matches_tag_expr("blocked", &tags(&["mvp", "core"])));
    }

    #[test]
    fn test_and() {
        assert!(matches_tag_expr("mvp and core", &tags(&["mvp", "core"])));
        assert!(!matches_tag_expr(
            "mvp and blocked",
            &tags(&["mvp", "core"])
        ));
    }

    #[test]
    fn test_or() {
        assert!(matches_tag_expr("mvp or blocked", &tags(&["mvp", "core"])));
        assert!(!matches_tag_expr(
            "blocked or deferred",
            &tags(&["mvp", "core"])
        ));
    }

    #[test]
    fn test_not() {
        assert!(matches_tag_expr(
            "mvp and not blocked",
            &tags(&["mvp", "core"])
        ));
        assert!(!matches_tag_expr("not mvp", &tags(&["mvp", "core"])));
    }

    #[test]
    fn test_parens() {
        assert!(matches_tag_expr(
            "(mvp or blocked) and core",
            &tags(&["mvp", "core"])
        ));
    }

    #[test]
    fn test_complex_expression() {
        // (a or b) and not c
        assert!(matches_tag_expr("(a or b) and not c", &tags(&["a", "d"])));
        assert!(!matches_tag_expr("(a or b) and not c", &tags(&["a", "c"])));
        assert!(!matches_tag_expr("(a or b) and not c", &tags(&["d"])));
    }

    #[test]
    fn test_double_not() {
        assert!(matches_tag_expr("not not mvp", &tags(&["mvp"])));
        assert!(!matches_tag_expr("not not mvp", &tags(&["core"])));
    }

    #[test]
    fn test_empty_tags() {
        assert!(!matches_tag_expr("mvp", &tags(&[])));
        assert!(matches_tag_expr("not mvp", &tags(&[])));
    }

    #[test]
    fn test_invalid_expression_detected_by_validate() {
        // Malformed expressions should be caught by validate_tag_expr
        assert!(validate_tag_expr("").is_err());
        assert!(validate_tag_expr("(").is_err());
        assert!(validate_tag_expr("mvp and").is_err());
        assert!(validate_tag_expr("((").is_err());
        assert!(validate_tag_expr("mvp or").is_err());
    }

    #[test]
    fn test_search_substring_match() {
        assert!(matches_search_expr("init", "cli init scaffolds"));
        assert!(!matches_search_expr("deploy", "cli init scaffolds"));
    }

    #[test]
    fn test_search_case_insensitive() {
        assert!(matches_search_expr("CLI", "cli init scaffolds"));
        assert!(matches_search_expr("Init", "cli init scaffolds"));
    }

    #[test]
    fn test_search_boolean() {
        assert!(matches_search_expr("cli and init", "cli init scaffolds"));
        assert!(!matches_search_expr("cli and deploy", "cli init scaffolds"));
        assert!(matches_search_expr("cli or deploy", "cli init scaffolds"));
        assert!(matches_search_expr("not deploy", "cli init scaffolds"));
        assert!(!matches_search_expr("not cli", "cli init scaffolds"));
    }

    #[test]
    fn test_search_parens() {
        assert!(matches_search_expr(
            "(init or check) and cli",
            "cli init scaffolds"
        ));
        assert!(!matches_search_expr(
            "(init or check) and api",
            "cli init scaffolds"
        ));
    }

    #[test]
    fn test_valid_expression_passes_validate() {
        assert!(validate_tag_expr("mvp").is_ok());
        assert!(validate_tag_expr("mvp and core").is_ok());
        assert!(validate_tag_expr("not blocked").is_ok());
        assert!(validate_tag_expr("(mvp or core) and not blocked").is_ok());
    }
}
