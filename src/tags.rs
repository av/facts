/// Boolean tag expression evaluator.
///
/// Supports: tag names, `and`, `or`, `not`, and parentheses.
/// Example: "mvp and not blocked"

/// Check if a set of tags matches a boolean expression.
pub fn matches_tag_expr(expr: &str, tags: &[String]) -> bool {
    match parse_expr(expr) {
        Ok(ast) => eval(&ast, tags),
        Err(_) => false,
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

fn parse_or<'a>(tokens: &'a [String]) -> Result<(Expr, &'a [String]), String> {
    let (mut left, mut rest) = parse_and(tokens)?;
    while !rest.is_empty() && rest[0] == "or" {
        let (right, new_rest) = parse_and(&rest[1..])?;
        left = Expr::Or(Box::new(left), Box::new(right));
        rest = new_rest;
    }
    Ok((left, rest))
}

fn parse_and<'a>(tokens: &'a [String]) -> Result<(Expr, &'a [String]), String> {
    let (mut left, mut rest) = parse_not(tokens)?;
    while !rest.is_empty() && rest[0] == "and" {
        let (right, new_rest) = parse_not(&rest[1..])?;
        left = Expr::And(Box::new(left), Box::new(right));
        rest = new_rest;
    }
    Ok((left, rest))
}

fn parse_not<'a>(tokens: &'a [String]) -> Result<(Expr, &'a [String]), String> {
    if tokens.is_empty() {
        return Err("unexpected end of expression".to_string());
    }
    if tokens[0] == "not" {
        let (expr, rest) = parse_not(&tokens[1..])?;
        return Ok((Expr::Not(Box::new(expr)), rest));
    }
    parse_primary(tokens)
}

fn parse_primary<'a>(tokens: &'a [String]) -> Result<(Expr, &'a [String]), String> {
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
        assert!(matches_tag_expr(
            "mvp or blocked",
            &tags(&["mvp", "core"])
        ));
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
        assert!(!matches_tag_expr(
            "not mvp",
            &tags(&["mvp", "core"])
        ));
    }

    #[test]
    fn test_parens() {
        assert!(matches_tag_expr(
            "(mvp or blocked) and core",
            &tags(&["mvp", "core"])
        ));
    }
}
