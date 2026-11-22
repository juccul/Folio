//! Fast pure-Rust numeric LaTeX evaluator for instant inline Auto Math Solve.
//!
//! Evaluates expressions ending with `=` (e.g. `4+4=`, `\sqrt{3}=`, `\Phi(1.96)=`, `t_{10, 0.05}=`,
//! `P(X \le 3)=`) strictly when they yield a single finite real numeric value or simple algebraic
//! simplification. Symbolic expressions with unresolved variables or bare literals (`5=`) are rejected.
use super::stats::{self, Distribution, RelOp};
use std::collections::BTreeMap;

/// Evaluate a recognized LaTeX string that MUST end with `=`.
/// Returns `Some((lhs_latex, formatted_answer))` when the LHS evaluates to a finite real number
/// (using `variables` and `equation_vars`) or genuinely simplifies as a simple single-variable
/// algebraic expression (e.g. `2x + 3x = 5x`, `2(x+3) = 2x+6`, `x =` after `2x + 4 = 10`).
pub fn evaluate_trailing_equals(
    latex: &str,
    variables: &BTreeMap<String, String>,
) -> Option<(String, String)> {
    evaluate_trailing_equals_with_equations(latex, variables, &[])
}

/// Evaluate a trailing-equals expression with both direct variable assignments (`x = 3`),
/// distribution declarations (`X \sim B(10, 0.3)`), and context equations (`2x + 4 = 10`).
pub fn evaluate_trailing_equals_with_equations(
    latex: &str,
    variables: &BTreeMap<String, String>,
    page_equations: &[String],
) -> Option<(String, String)> {
    let trimmed = latex.trim();
    if trimmed.is_empty() {
        return None;
    }
    let lhs = strip_trailing_equals(trimmed)?;
    if lhs.is_empty() {
        return None;
    }
    let lhs_toks = tokenize(lhs)?;
    if lhs_toks.is_empty() || contains_top_level_relation(&lhs_toks) {
        return None;
    }

    // Merge direct assignments, distribution declarations, and equations from `variables` & `page_equations`.
    let mut direct_raw: BTreeMap<String, String> = BTreeMap::new();
    let mut dist_raw: Vec<String> = Vec::new();
    let mut non_assignment_eqs: Vec<(String, String)> = Vec::new();
    for (name, rhs) in variables {
        if looks_like_distribution_rhs(rhs) {
            dist_raw.push(format!("{name} \\sim {rhs}"));
        } else {
            direct_raw.insert(name.clone(), rhs.clone());
        }
    }
    for eq in page_equations {
        if eq.contains("\\sim") || eq.contains('~') {
            dist_raw.push(eq.clone());
        } else if let Some((var, rhs)) = parse_variable_assignment(eq) {
            if looks_like_distribution_rhs(&rhs) {
                dist_raw.push(format!("{var} \\sim {rhs}"));
            } else {
                direct_raw.insert(var, rhs);
            }
        } else if let Some((l, r)) = split_single_equation(eq) {
            non_assignment_eqs.push((l.to_string(), r.to_string()));
        }
    }

    // Multi-pass resolution for chained assignments (`y = 3`, `x = y^2 + 4`) and distributions (`X ~ B(n, p)`).
    let mut env: BTreeMap<String, f64> = BTreeMap::new();
    let mut dists: BTreeMap<String, Distribution> = BTreeMap::new();
    for _ in 0..4 {
        let mut progress = false;
        for (name, expr) in &direct_raw {
            let key = name.trim().to_string();
            if env.contains_key(&key) {
                continue;
            }
            if let Some((val, _)) = eval_expr_with_context(expr, &env, &dists)
                && val.is_finite()
            {
                env.insert(key, val);
                progress = true;
            }
        }
        for raw in &dist_raw {
            if let Some((var, dist)) = parse_distribution_declaration(raw, &env, &dists)
                && dists.get(&var) != Some(&dist)
            {
                dists.insert(var, dist);
                progress = true;
            }
        }
        if !progress {
            break;
        }
    }

    // Solve simple single-variable linear equations from the page (e.g. `2x + 4 = 10` -> `x = 3`).
    let mut eq_solved: BTreeMap<String, f64> = BTreeMap::new();
    for (l, r) in &non_assignment_eqs {
        if let Some((var, val)) = solve_linear_equation(l, r, &env) {
            eq_solved.insert(var.clone(), val);
            env.entry(var).or_insert(val);
        }
    }

    // If the user wrote bare `x =`, only auto-fill when `x` was solved from an equation
    // (like `2x + 4 = 10` above), NOT when `x` is undefined or a bare reassignment.
    if lhs_toks.len() == 1
        && let Tok::Ident(ref name) = lhs_toks[0]
        && name != "e"
    {
        let val = eq_solved
            .get(name)
            .copied()
            .or_else(|| lookup_env_case_fallback(&eq_solved, name))?;
        return Some((lhs.to_string(), format_numeric_answer(val)?));
    }

    if let Some((value, non_trivial)) = eval_expr_with_context(lhs, &env, &dists)
        && non_trivial
        && value.is_finite()
    {
        return Some((lhs.to_string(), format_numeric_answer(value)?));
    }

    // Fall back to simple single-variable algebraic simplification (`2x + 3x = 5x`, `2(x+3) = 2x+6`).
    let simplified = simplify_simple_algebra(lhs, &env)?;
    Some((lhs.to_string(), simplified))
}

/// Fast check for whether a recognized trailing-equals expression contains an identifier or
/// probability/moment query that could benefit from scanning page assignments/distributions.
pub fn needs_page_variables(latex: &str) -> bool {
    let Some(lhs) = strip_trailing_equals(latex.trim()) else {
        return false;
    };
    let Some(toks) = tokenize(lhs) else {
        return false;
    };
    toks.iter().any(|t| match t {
        Tok::Ident(name) => name != "e",
        Tok::Cmd(c) => matches!(c.as_str(), "var" | "sd"),
        _ => false,
    })
}

/// Pure-Rust statistics solver entry point for the Math Panel ("Solve with steps").
/// Triggers strictly when the input uses statistical table/distribution notation (so ordinary
/// algebra/calculus continues through the symbolic solver), supporting optional multi-line
/// context (`X \sim B(10, 0.3)\nP(X \le 3)`), with or without a trailing `=`.
pub fn solve_statistics_panel(
    expression: &str,
    variables: &BTreeMap<String, String>,
) -> Option<(String, String, String)> {
    let lines: Vec<&str> = expression
        .split('\n')
        .flat_map(|l| l.split("\\\\"))
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    if lines.is_empty() {
        return None;
    }
    let query_raw = *lines.last()?;
    let query_body = strip_trailing_equals(query_raw).unwrap_or(query_raw).trim();
    if query_body.is_empty() || !looks_like_statistics_query(query_body, &lines[..lines.len() - 1]) {
        return None;
    }
    let context_eqs: Vec<String> = lines[..lines.len() - 1]
        .iter()
        .map(|s| (*s).to_string())
        .collect();
    let with_eq = format!("{query_body} =");
    let (lhs, answer) = evaluate_trailing_equals_with_equations(&with_eq, variables, &context_eqs)?;
    let explanation = describe_statistics_query(&lhs, &context_eqs);
    Some((lhs, answer, explanation))
}

fn looks_like_statistics_query(query: &str, context_lines: &[&str]) -> bool {
    if context_lines
        .iter()
        .any(|l| l.contains("\\sim") || l.contains('~') || looks_like_distribution_rhs(l))
    {
        return true;
    }
    let Some(toks) = tokenize(query) else {
        return false;
    };
    let has_inner_rel = toks.iter().any(|t| {
        matches!(t, Tok::Lt | Tok::Le | Tok::Gt | Tok::Ge | Tok::Eq)
    });
    if has_inner_rel && toks.iter().any(|t| matches!(t, Tok::Ident(s) if s == "P")) {
        return true;
    }
    for (idx, tok) in toks.iter().enumerate() {
        let next = toks.get(idx + 1);
        match tok {
            Tok::Cmd(c)
                if matches!(
                    c.as_str(),
                    "Phi"
                        | "chi"
                        | "chi2"
                        | "chi2cdf"
                        | "chi2inv"
                        | "normcdf"
                        | "normpdf"
                        | "norminv"
                        | "normal"
                        | "tcdf"
                        | "tinv"
                        | "fcdf"
                        | "finv"
                        | "bin"
                        | "binom"
                        | "binomial"
                        | "binomcdf"
                        | "binompdf"
                        | "pois"
                        | "poisson"
                        | "poisscdf"
                        | "poisspdf"
                        | "sw"
                        | "swa"
                        | "var"
                        | "sd"
                ) =>
            {
                return true;
            }
            Tok::Cmd(c)
                if matches!(c.as_str(), "phi" | "varphi") && matches!(next, Some(Tok::LParen)) =>
            {
                return true;
            }
            Tok::Ident(s)
                if matches!(s.as_str(), "z" | "Z" | "t" | "T" | "F" | "W")
                    && matches!(next, Some(Tok::Underscore)) =>
            {
                return true;
            }
            Tok::Ident(s)
                if matches!(s.as_str(), "F" | "W" | "B" | "N" | "t" | "T")
                    && matches!(next, Some(Tok::LParen))
                    && toks[idx + 1..].contains(&Tok::Comma) =>
            {
                return true;
            }
            _ => {}
        }
    }
    false
}

fn describe_statistics_query(lhs: &str, context: &[String]) -> String {
    let lower = lhs.to_ascii_lowercase();
    let base = if lhs.contains("\\Phi") || lower.contains("norm") || lower.contains("z_") {
        "Evaluate the standard/normal distribution probability, density, or critical value."
    } else if lhs.contains("\\varphi") || lhs.contains("\\phi(") {
        "Evaluate the standard normal density function φ(z) = (1/√(2π)) e^(-z²/2)."
    } else if lhs.contains("\\chi") || lower.contains("chi2") {
        "Evaluate the chi-square distribution critical value or tail probability."
    } else if lower.contains("pois") {
        "Evaluate the Poisson distribution probability or moment."
    } else if lower.contains("bin") || lhs.starts_with("B(") {
        "Evaluate the binomial distribution probability or moment."
    } else if lower.contains("sw") || lhs.starts_with("W_") || lhs.starts_with("W(") {
        "Evaluate the Shapiro-Wilk normality test table value or W statistic."
    } else if lhs.starts_with("F_") || lhs.starts_with("F(") || lower.contains("fcdf") || lower.contains("finv") {
        "Evaluate the F-distribution upper-tail critical value or probability."
    } else if lhs.starts_with("t_") || lhs.starts_with("T_") || lower.contains("tcdf") || lower.contains("tinv") {
        "Evaluate the Student's t-distribution upper-tail critical value or probability."
    } else {
        "Evaluate the statistical distribution probability or table value."
    };
    if context.is_empty() {
        base.to_string()
    } else {
        format!("{base} Using context: {}.", context.join(", "))
    }
}

/// Parse a handwritten variable assignment like `x = 3`, `a = 2 + 5`, or `\theta = 30^\circ`.
pub fn parse_variable_assignment(latex: &str) -> Option<(String, String)> {
    let (lhs, rhs) = split_single_equation(latex)?;
    if lhs.is_empty() || rhs.is_empty() {
        return None;
    }
    let toks = tokenize(lhs)?;
    if toks.len() != 1 {
        return None;
    }
    let name = match &toks[0] {
        Tok::Ident(s) if s != "e" => s.clone(),
        Tok::Cmd(c)
            if matches!(
                c.as_str(),
                "alpha" | "beta" | "gamma" | "delta" | "theta" | "lambda" | "mu" | "phi" | "omega"
            ) =>
        {
            c.clone()
        }
        _ => return None,
    };
    // Avoid self-referential `x = x` loops.
    if tokenize(rhs)?.iter().any(|t| match t {
        Tok::Ident(s) => s == &name,
        Tok::Cmd(c) => c == &name,
        _ => false,
    }) {
        return None;
    }
    Some((name, rhs.to_string()))
}

fn split_single_equation(latex: &str) -> Option<(&str, &str)> {
    let s = latex.trim();
    let (lhs, rhs) = s.split_once('=')?;
    let lhs = lhs.trim();
    let rhs = rhs.trim();
    if lhs.is_empty() || rhs.is_empty() || contains_relation(lhs) || contains_relation(rhs) {
        return None;
    }
    Some((lhs, rhs))
}

fn strip_trailing_equals(s: &str) -> Option<&str> {
    let s = s.trim_end_matches(|c: char| c.is_whitespace() || c == '.' || c == ',');
    let rest = s.strip_suffix('=')?;
    Some(rest.trim())
}

fn contains_relation(s: &str) -> bool {
    s.contains('=')
        || s.contains('<')
        || s.contains('>')
        || s.contains("\\le")
        || s.contains("\\ge")
        || s.contains("\\ne")
        || s.contains("\\approx")
        || s.contains("\\equiv")
        || s.contains("\\pm")
        || s.contains("\\mp")
}

/// Reject relational operators at top level (like `2+2=4=` or `x < 5 =`), while permitting
/// relational operators inside `P(...)` probability events (like `P(Z \le 1.96) =` or `P(X = 3) =`).
fn contains_top_level_relation(toks: &[Tok]) -> bool {
    let mut depth = 0usize;
    for tok in toks {
        match tok {
            Tok::LParen | Tok::LBracket | Tok::LBrace => depth += 1,
            Tok::RParen | Tok::RBracket | Tok::RBrace => depth = depth.saturating_sub(1),
            Tok::Lt | Tok::Le | Tok::Gt | Tok::Ge | Tok::Eq | Tok::Tilde if depth == 0 => {
                return true;
            }
            Tok::Cmd(c)
                if depth == 0
                    && matches!(c.as_str(), "ne" | "approx" | "equiv" | "pm" | "mp") =>
            {
                return true;
            }
            _ => {}
        }
    }
    false
}

fn looks_like_distribution_rhs(rhs: &str) -> bool {
    let s = rhs.trim();
    let lower = s.to_ascii_lowercase();
    lower.starts_with("bin(")
        || lower.starts_with("binom(")
        || lower.starts_with("binomial(")
        || lower.starts_with("pois(")
        || lower.starts_with("poisson(")
        || lower.starts_with("po(")
        || lower.starts_with("normal(")
        || lower.starts_with("n(")
        || lower.starts_with("b(")
        || lower.starts_with("f(")
        || lower.starts_with("chi2(")
        || s.starts_with("\\chi^2")
        || s.starts_with("\\chi^{2}")
        || (lower.starts_with("t_") && !lower.contains(','))
        || (lower.starts_with("t(") && !lower.contains(','))
        || (lower.starts_with("sw(") && !lower.contains(','))
}

/// Format a finite real number cleanly like a handwritten calculator answer.
pub fn format_numeric_answer(v: f64) -> Option<String> {
    if !v.is_finite() {
        return None;
    }
    // Snap tiny trig/floating-point residues (e.g. sin(pi) ~ 1.2e-16) to exact 0.
    if v == 0.0 || v.abs() < 1e-12 {
        return Some("0".to_string());
    }
    let rounded_int = v.round();
    if v.abs() < 1e12 && (v - rounded_int).abs() <= 1e-9 * v.abs().max(1.0) {
        let i = rounded_int as i64;
        return Some(i.to_string());
    }
    let abs = v.abs();
    if abs >= 1e9 || abs < 1e-4 {
        let raw = format!("{v:.3e}");
        let (mant, exp) = raw.split_once('e')?;
        let mant = trim_decimal_zeros(mant);
        return Some(format!("{mant}e{exp}"));
    }
    let raw = format!("{v:.4}");
    let cleaned = trim_decimal_zeros(&raw);
    if cleaned == "-0" {
        Some("0".to_string())
    } else {
        Some(cleaned)
    }
}

fn trim_decimal_zeros(s: &str) -> String {
    if !s.contains('.') {
        return s.to_string();
    }
    let trimmed = s.trim_end_matches('0').trim_end_matches('.');
    trimmed.to_string()
}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Num(f64),
    Inf,
    Ident(String),
    Cmd(String),
    Plus,
    Minus,
    Mul,
    Div,
    Caret,
    Bang,
    Percent,
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Pipe,
    Underscore,
    Comma,
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Tilde,
}

fn is_known_math_word(lower: &str) -> bool {
    matches!(
        lower,
        "sin"
            | "cos"
            | "tan"
            | "csc"
            | "sec"
            | "cot"
            | "arcsin"
            | "arccos"
            | "arctan"
            | "sinh"
            | "cosh"
            | "tanh"
            | "ln"
            | "log"
            | "exp"
            | "sqrt"
            | "abs"
            | "pi"
            | "normcdf"
            | "normpdf"
            | "norminv"
            | "tcdf"
            | "tinv"
            | "chi2"
            | "chi2cdf"
            | "chi2inv"
            | "fcdf"
            | "finv"
            | "bin"
            | "binom"
            | "binomial"
            | "binomcdf"
            | "binompdf"
            | "pois"
            | "poisson"
            | "poisscdf"
            | "poisspdf"
            | "normal"
            | "sw"
            | "swa"
            | "var"
            | "sd"
    )
}

fn is_prefix_splittable_func(lower: &str) -> bool {
    matches!(
        lower,
        "sin"
            | "cos"
            | "tan"
            | "csc"
            | "sec"
            | "cot"
            | "arcsin"
            | "arccos"
            | "arctan"
            | "sinh"
            | "cosh"
            | "tanh"
            | "ln"
            | "log"
            | "exp"
            | "sqrt"
            | "abs"
    )
}

/// Case-insensitive fallback for single-letter handwritten variables (e.g. `y` vs `Y`, `x` vs `X`)
/// when the exact case is not defined. Exact matches and built-in `e` always take precedence.
fn lookup_env_case_fallback(env: &BTreeMap<String, f64>, name: &str) -> Option<f64> {
    if name.len() != 1 {
        return None;
    }
    let ch = name.as_bytes()[0];
    if !ch.is_ascii_alphabetic() {
        return None;
    }
    let flipped = if ch.is_ascii_lowercase() {
        (ch as char).to_ascii_uppercase()
    } else {
        (ch as char).to_ascii_lowercase()
    };
    if flipped == 'e' {
        return None;
    }
    let mut buf = [0u8; 4];
    let key = flipped.encode_utf8(&mut buf);
    env.get(key).copied()
}

fn tokenize(input: &str) -> Option<Vec<Tok>> {
    let bytes = input.as_bytes();
    let mut i = 0;
    let mut out = Vec::new();
    while i < bytes.len() {
        let b = bytes[i];
        if b.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if b == b'\\' {
            i += 1;
            if i >= bytes.len() {
                return None;
            }
            let c = bytes[i];
            if c.is_ascii_alphabetic() {
                let start = i;
                while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
                    i += 1;
                }
                let cmd = &input[start..i];
                match cmd {
                    "left" | "right" | "big" | "Big" | "bigg" | "Bigg" | "quad" | "qquad"
                    | "displaystyle" | "textstyle" | "limits" => {}
                    "times" | "cdot" | "ast" => out.push(Tok::Mul),
                    "div" => out.push(Tok::Div),
                    "le" | "leq" | "leqslant" => out.push(Tok::Le),
                    "ge" | "geq" | "geqslant" => out.push(Tok::Ge),
                    "sim" => out.push(Tok::Tilde),
                    "infty" => out.push(Tok::Inf),
                    "mathrm" | "operatorname" | "text" => {
                        // Unwrap simple operator text like `\mathrm{sin}` or `\mathrm{Var}` if present.
                        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                            i += 1;
                        }
                        if i < bytes.len() && bytes[i] == b'{' {
                            i += 1;
                            let s0 = i;
                            while i < bytes.len() && bytes[i] != b'}' {
                                i += 1;
                            }
                            if i >= bytes.len() {
                                return None;
                            }
                            let word = input[s0..i].trim();
                            i += 1;
                            if !word.is_empty() {
                                let lower = word.to_ascii_lowercase();
                                let normalized = if is_known_math_word(&lower) {
                                    lower
                                } else {
                                    word.to_string()
                                };
                                out.push(Tok::Cmd(normalized));
                            }
                        }
                    }
                    _ => {
                        let lower = cmd.to_ascii_lowercase();
                        let normalized = if is_known_math_word(&lower) {
                            lower
                        } else {
                            cmd.to_string()
                        };
                        out.push(Tok::Cmd(normalized));
                    }
                }
            } else {
                i += 1;
                match c {
                    b',' | b';' | b':' | b'!' | b' ' => {}
                    b'%' => out.push(Tok::Percent),
                    b'{' => out.push(Tok::LBrace),
                    b'}' => out.push(Tok::RBrace),
                    b'|' => out.push(Tok::Pipe),
                    _ => return None,
                }
            }
            continue;
        }
        if b.is_ascii_digit() || (b == b'.' && i + 1 < bytes.len() && bytes[i + 1].is_ascii_digit())
        {
            let start = i;
            let mut seen_dot = false;
            while i < bytes.len() {
                if bytes[i].is_ascii_digit() {
                    i += 1;
                } else if bytes[i] == b'.' && !seen_dot {
                    seen_dot = true;
                    i += 1;
                } else {
                    break;
                }
            }
            let val: f64 = input[start..i].parse().ok()?;
            out.push(Tok::Num(val));
            continue;
        }
        if b.is_ascii_alphabetic() {
            // Recognize plain multi-letter math/statistics words (case-insensitive: `Sin`, `Tan`,
            // `Log`, `Pois`, `Bin`, `Var`, `chi2`, `sw_a`), plus tightly joined function + variable (`tanx`, `lne`).
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
                i += 1;
            }
            let word = &input[start..i];
            let lower = word.to_ascii_lowercase();
            // Special alphanumeric names: `chi2`, `chi2cdf`, `chi2inv`, `sw_a`.
            if lower == "chi" && i < bytes.len() && bytes[i] == b'2' {
                i += 1;
                let sfx_start = i;
                while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
                    i += 1;
                }
                let full = format!("chi2{}", input[sfx_start..i].to_ascii_lowercase());
                if is_known_math_word(&full) {
                    out.push(Tok::Cmd(full));
                    continue;
                }
                i = sfx_start;
                out.push(Tok::Cmd("chi2".to_string()));
                continue;
            }
            if lower == "sw"
                && i + 1 < bytes.len()
                && bytes[i] == b'_'
                && bytes[i + 1].eq_ignore_ascii_case(&b'a')
            {
                i += 2;
                out.push(Tok::Cmd("swa".to_string()));
                continue;
            }
            if is_known_math_word(&lower) {
                out.push(Tok::Cmd(lower));
            } else if word.len() >= 3 && is_prefix_splittable_func(&lower[..lower.len() - 1]) {
                out.push(Tok::Cmd(lower[..lower.len() - 1].to_string()));
                out.push(Tok::Ident(word[word.len() - 1..].to_string()));
            } else {
                // Split adjacent single-letter variables like `xy` or `2ab` into individual identifiers.
                for ch in word.chars() {
                    out.push(Tok::Ident(ch.to_string()));
                }
            }
            continue;
        }
        i += 1;
        match b {
            b'+' => out.push(Tok::Plus),
            b'-' => out.push(Tok::Minus),
            b'*' => out.push(Tok::Mul),
            b'/' => out.push(Tok::Div),
            b'^' => out.push(Tok::Caret),
            b'!' => out.push(Tok::Bang),
            b'%' => out.push(Tok::Percent),
            b'(' => out.push(Tok::LParen),
            b')' => out.push(Tok::RParen),
            b'{' => out.push(Tok::LBrace),
            b'}' => out.push(Tok::RBrace),
            b'[' => out.push(Tok::LBracket),
            b']' => out.push(Tok::RBracket),
            b'|' => out.push(Tok::Pipe),
            b'_' => out.push(Tok::Underscore),
            b',' | b';' => out.push(Tok::Comma),
            b'~' => out.push(Tok::Tilde),
            b'=' => out.push(Tok::Eq),
            b'<' => {
                if i < bytes.len() && bytes[i] == b'=' {
                    i += 1;
                    out.push(Tok::Le);
                } else {
                    out.push(Tok::Lt);
                }
            }
            b'>' => {
                if i < bytes.len() && bytes[i] == b'=' {
                    i += 1;
                    out.push(Tok::Ge);
                } else {
                    out.push(Tok::Gt);
                }
            }
            _ => return None,
        }
    }
    Some(out)
}

fn eval_expr_with_context(
    expr: &str,
    env: &BTreeMap<String, f64>,
    dists: &BTreeMap<String, Distribution>,
) -> Option<(f64, bool)> {
    let tokens = tokenize(expr)?;
    if tokens.is_empty() {
        return None;
    }
    let mut parser = Parser {
        toks: &tokens,
        pos: 0,
        env,
        dists,
        non_trivial: false,
        depth: 0,
    };
    let val = parser.parse_add()?;
    if parser.pos != parser.toks.len() {
        return None;
    }
    Some((val, parser.non_trivial))
}

/// Parse a distribution declaration like `X \sim B(10, 0.3)`, `X \sim Pois(3.2)`,
/// `X \sim N(100, 15^2)`, `T \sim t_{10}`, `X \sim \chi^2_{10}`, `F \sim F(5, 10)`, `W \sim SW(30)`.
fn parse_distribution_declaration(
    latex: &str,
    env: &BTreeMap<String, f64>,
    dists: &BTreeMap<String, Distribution>,
) -> Option<(String, Distribution)> {
    let toks = tokenize(latex)?;
    let sep = toks.iter().position(|t| matches!(t, Tok::Tilde | Tok::Eq))?;
    let lhs = &toks[..sep];
    let rhs = &toks[sep + 1..];
    if lhs.is_empty() || rhs.is_empty() {
        return None;
    }
    let var = match lhs {
        [Tok::Ident(name)] if name != "e" => name.clone(),
        [Tok::Cmd(c)] if c == "chi" => "chi2".to_string(),
        [Tok::Cmd(c), Tok::Caret, Tok::Num(v)] if c == "chi" && (*v - 2.0).abs() < 1e-9 => {
            "chi2".to_string()
        }
        [Tok::Cmd(c), Tok::Caret, Tok::LBrace, Tok::Num(v), Tok::RBrace]
            if c == "chi" && (*v - 2.0).abs() < 1e-9 =>
        {
            "chi2".to_string()
        }
        _ => return None,
    };
    let mut p = Parser {
        toks: rhs,
        pos: 0,
        env,
        dists,
        non_trivial: false,
        depth: 0,
    };
    let dist = p.parse_distribution_spec()?;
    if p.pos != rhs.len() {
        return None;
    }
    Some((var, dist))
}

struct Parser<'a> {
    toks: &'a [Tok],
    pos: usize,
    env: &'a BTreeMap<String, f64>,
    dists: &'a BTreeMap<String, Distribution>,
    non_trivial: bool,
    depth: usize,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos)
    }
    fn next(&mut self) -> Option<Tok> {
        let t = self.toks.get(self.pos)?.clone();
        self.pos += 1;
        Some(t)
    }
    fn enter(&mut self) -> Option<()> {
        self.depth += 1;
        (self.depth <= 64).then_some(())
    }
    fn leave(&mut self) {
        self.depth = self.depth.saturating_sub(1);
    }

    fn parse_add(&mut self) -> Option<f64> {
        self.enter()?;
        let mut lhs = self.parse_mul()?;
        while let Some(tok) = self.peek() {
            match tok {
                Tok::Plus => {
                    self.next();
                    let rhs = self.parse_mul()?;
                    lhs += rhs;
                    self.non_trivial = true;
                }
                Tok::Minus => {
                    self.next();
                    let rhs = self.parse_mul()?;
                    lhs -= rhs;
                    self.non_trivial = true;
                }
                _ => break,
            }
        }
        self.leave();
        Some(lhs)
    }

    fn starts_implicit_mul(&self) -> bool {
        matches!(
            self.peek(),
            Some(Tok::LParen | Tok::LBracket | Tok::Cmd(_) | Tok::Ident(_))
        )
    }

    fn parse_mul(&mut self) -> Option<f64> {
        self.enter()?;
        let mut lhs = self.parse_unary()?;
        loop {
            match self.peek() {
                Some(Tok::Mul) => {
                    self.next();
                    let rhs = self.parse_unary()?;
                    lhs *= rhs;
                    self.non_trivial = true;
                }
                Some(Tok::Div) => {
                    self.next();
                    let rhs = self.parse_unary()?;
                    if rhs == 0.0 {
                        return None;
                    }
                    lhs /= rhs;
                    self.non_trivial = true;
                }
                _ if self.starts_implicit_mul() => {
                    let rhs = self.parse_unary()?;
                    lhs *= rhs;
                    self.non_trivial = true;
                }
                _ => break,
            }
        }
        self.leave();
        Some(lhs)
    }

    fn parse_unary(&mut self) -> Option<f64> {
        match self.peek() {
            Some(Tok::Plus) => {
                self.next();
                self.parse_unary()
            }
            Some(Tok::Minus) => {
                self.next();
                Some(-self.parse_unary()?)
            }
            _ => self.parse_power(),
        }
    }

    fn parse_power(&mut self) -> Option<f64> {
        let mut base = self.parse_postfix()?;
        if matches!(self.peek(), Some(Tok::Caret)) {
            self.next();
            // Handle degree symbol: `30^\circ` or `30^{\circ}`.
            if self.consume_degree_exponent() {
                self.non_trivial = true;
                return Some(base * std::f64::consts::PI / 180.0);
            }
            let exp = self.parse_latex_group_or_unary()?;
            base = safe_pow(base, exp)?;
            self.non_trivial = true;
        }
        Some(base)
    }

    fn consume_degree_exponent(&mut self) -> bool {
        if matches!(self.peek(), Some(Tok::Cmd(c)) if c == "circ" || c == "degree") {
            self.next();
            return true;
        }
        if matches!(self.peek(), Some(Tok::LBrace))
            && matches!(self.toks.get(self.pos + 1), Some(Tok::Cmd(c)) if c == "circ" || c == "degree")
            && matches!(self.toks.get(self.pos + 2), Some(Tok::RBrace))
        {
            self.pos += 3;
            return true;
        }
        false
    }

    fn parse_postfix(&mut self) -> Option<f64> {
        let mut val = self.parse_primary()?;
        loop {
            match self.peek() {
                Some(Tok::Bang) => {
                    self.next();
                    val = factorial(val)?;
                    self.non_trivial = true;
                }
                Some(Tok::Percent) => {
                    self.next();
                    val *= 0.01;
                    self.non_trivial = true;
                }
                _ => break,
            }
        }
        Some(val)
    }

    fn parse_latex_group_or_unary(&mut self) -> Option<f64> {
        if matches!(self.peek(), Some(Tok::LBrace)) {
            self.next();
            let v = self.parse_add()?;
            if !matches!(self.next(), Some(Tok::RBrace)) {
                return None;
            }
            Some(v)
        } else {
            self.parse_unary()
        }
    }

    fn parse_func_arg(&mut self) -> Option<f64> {
        match self.peek() {
            Some(Tok::LParen) => {
                self.next();
                let v = self.parse_add()?;
                if !matches!(self.next(), Some(Tok::RParen)) {
                    return None;
                }
                Some(v)
            }
            Some(Tok::LBrace) => {
                self.next();
                let v = self.parse_add()?;
                if !matches!(self.next(), Some(Tok::RBrace)) {
                    return None;
                }
                Some(v)
            }
            _ => self.parse_unary(),
        }
    }

    fn parse_add_or_inf(&mut self) -> Option<f64> {
        if matches!(self.peek(), Some(Tok::Inf)) {
            self.next();
            return Some(f64::INFINITY);
        }
        self.parse_add()
    }

    fn parse_arg_list(&mut self, close: Tok) -> Option<Vec<f64>> {
        let mut args = Vec::new();
        if self.peek() == Some(&close) {
            self.next();
            return Some(args);
        }
        loop {
            args.push(self.parse_add_or_inf()?);
            match self.peek() {
                Some(Tok::Comma) => {
                    self.next();
                }
                Some(t) if *t == close => {
                    self.next();
                    return Some(args);
                }
                _ => return None,
            }
        }
    }

    fn parse_subscript_args(&mut self) -> Option<Vec<f64>> {
        if !matches!(self.peek(), Some(Tok::Underscore)) {
            return None;
        }
        self.next();
        if matches!(self.peek(), Some(Tok::LBrace)) {
            self.next();
            self.parse_arg_list(Tok::RBrace)
        } else if matches!(self.peek(), Some(Tok::Inf)) {
            self.next();
            Some(vec![f64::INFINITY])
        } else {
            Some(vec![self.parse_unary()?])
        }
    }

    fn parse_paren_args(&mut self) -> Option<Vec<f64>> {
        if !matches!(self.peek(), Some(Tok::LParen)) {
            return None;
        }
        self.next();
        self.parse_arg_list(Tok::RParen)
    }

    fn parse_sub_and_paren_args(&mut self) -> Option<Vec<f64>> {
        let mut args = Vec::new();
        if matches!(self.peek(), Some(Tok::Underscore)) {
            args.extend(self.parse_subscript_args()?);
        }
        if matches!(self.peek(), Some(Tok::LParen)) {
            args.extend(self.parse_paren_args()?);
        }
        (!args.is_empty()).then_some(args)
    }

    fn consume_squared_exponent(&mut self) -> bool {
        let saved = self.pos;
        if !matches!(self.peek(), Some(Tok::Caret)) {
            return false;
        }
        self.next();
        let ok = match self.peek() {
            Some(Tok::Num(v)) if (*v - 2.0).abs() < 1e-9 => {
                self.next();
                true
            }
            Some(Tok::LBrace)
                if matches!(self.toks.get(self.pos + 1), Some(Tok::Num(v)) if (*v - 2.0).abs() < 1e-9)
                    && matches!(self.toks.get(self.pos + 2), Some(Tok::RBrace)) =>
            {
                self.pos += 3;
                true
            }
            _ => false,
        };
        if !ok {
            self.pos = saved;
        }
        ok
    }

    fn consume_inverse_exponent(&mut self) -> bool {
        let saved = self.pos;
        if !matches!(self.peek(), Some(Tok::Caret)) {
            return false;
        }
        self.next();
        let ok = match self.peek() {
            Some(Tok::Minus)
                if matches!(self.toks.get(self.pos + 1), Some(Tok::Num(v)) if (*v - 1.0).abs() < 1e-9) =>
            {
                self.pos += 2;
                true
            }
            Some(Tok::LBrace)
                if matches!(self.toks.get(self.pos + 1), Some(Tok::Minus))
                    && matches!(self.toks.get(self.pos + 2), Some(Tok::Num(v)) if (*v - 1.0).abs() < 1e-9)
                    && matches!(self.toks.get(self.pos + 3), Some(Tok::RBrace)) =>
            {
                self.pos += 4;
                true
            }
            _ => false,
        };
        if !ok {
            self.pos = saved;
        }
        ok
    }

    fn parse_distribution_spec(&mut self) -> Option<Distribution> {
        let head = match self.next()? {
            Tok::Ident(s) => s.to_ascii_lowercase(),
            Tok::Cmd(c) => c.to_ascii_lowercase(),
            _ => return None,
        };
        match head.as_str() {
            "n" | "normal" => {
                let args = self.parse_paren_args()?;
                if args.len() != 2 || !(args[1] > 0.0 && args[1].is_finite()) {
                    return None;
                }
                Some(Distribution::Normal {
                    mu: args[0],
                    sigma: args[1].sqrt(),
                })
            }
            "b" | "bin" | "binom" | "binomial" => {
                let args = self.parse_paren_args()?;
                if args.len() != 2 {
                    return None;
                }
                let n = as_pos_u64(args[0])?;
                let p = args[1];
                ((0.0..=1.0).contains(&p) && p.is_finite()).then_some(Distribution::Binomial { n, p })
            }
            "pois" | "poisson" | "p" => {
                let args = self.parse_paren_args()?;
                if args.len() != 1 || !(args[0] >= 0.0 && args[0].is_finite()) {
                    return None;
                }
                Some(Distribution::Poisson { mu: args[0] })
            }
            "t" => {
                let args = self.parse_sub_and_paren_args()?;
                if args.len() != 1 || !(args[0] > 0.0) {
                    return None;
                }
                Some(Distribution::StudentT { df: args[0] })
            }
            "chi" => {
                let had_sq = self.consume_squared_exponent();
                let args = self.parse_sub_and_paren_args()?;
                if !had_sq && !self.consume_squared_exponent() {
                    return None;
                }
                if args.len() != 1 || !(args[0] > 0.0 && args[0].is_finite()) {
                    return None;
                }
                Some(Distribution::ChiSquare { df: args[0] })
            }
            "chi2" => {
                let args = self.parse_sub_and_paren_args()?;
                if args.len() != 1 || !(args[0] > 0.0 && args[0].is_finite()) {
                    return None;
                }
                Some(Distribution::ChiSquare { df: args[0] })
            }
            "f" => {
                let args = self.parse_sub_and_paren_args()?;
                if args.len() != 2 || !(args[0] > 0.0 && args[1] > 0.0) {
                    return None;
                }
                Some(Distribution::FDist {
                    d1: args[0],
                    d2: args[1],
                })
            }
            "sw" | "w" => {
                let args = self.parse_sub_and_paren_args()?;
                if args.len() != 1 {
                    return None;
                }
                let n = as_pos_u64(args[0])? as usize;
                (n >= 3).then_some(Distribution::ShapiroWilk { n })
            }
            _ => None,
        }
    }

    fn lookup_distribution(&self, name: &str) -> Option<Distribution> {
        if let Some(d) = self.dists.get(name) {
            return Some(d.clone());
        }
        if name.len() == 1 {
            let ch = name.as_bytes()[0];
            if ch.is_ascii_alphabetic() {
                let flipped = if ch.is_ascii_lowercase() {
                    (ch as char).to_ascii_uppercase()
                } else {
                    (ch as char).to_ascii_lowercase()
                };
                let mut buf = [0u8; 4];
                if let Some(d) = self.dists.get(flipped.encode_utf8(&mut buf)) {
                    return Some(d.clone());
                }
            }
        }
        if name.eq_ignore_ascii_case("z") {
            return Some(Distribution::Normal {
                mu: 0.0,
                sigma: 1.0,
            });
        }
        if name.eq_ignore_ascii_case("w") {
            let n_val = self
                .env
                .get("n")
                .copied()
                .or_else(|| self.env.get("N").copied())?;
            let n = as_pos_u64(n_val)? as usize;
            if n >= 3 {
                return Some(Distribution::ShapiroWilk { n });
            }
        }
        None
    }

    /// Try parsing a random variable reference inside `P(...)`, `E(...)`, or `Var(...)`:
    /// either a declared RV (`X`, `Z`, `T`, `F`, `W`, `\chi^2`) or an inline subscripted RV
    /// (`T_{10}`, `t_{10}`, `\chi^2_{10}`, `F_{5, 10}`).
    fn try_parse_rv(&mut self) -> Option<Distribution> {
        let saved = self.pos;
        match self.next()? {
            Tok::Cmd(cmd) if cmd == "chi" => {
                let had_sq = self.consume_squared_exponent();
                if matches!(self.peek(), Some(Tok::Underscore)) {
                    if let Some(args) = self.parse_subscript_args() {
                        if (had_sq || self.consume_squared_exponent())
                            && args.len() == 1
                            && args[0] > 0.0
                            && args[0].is_finite()
                        {
                            return Some(Distribution::ChiSquare { df: args[0] });
                        }
                    }
                    self.pos = saved;
                    return None;
                }
                if had_sq {
                    if let Some(d) = self.lookup_distribution("chi2") {
                        return Some(d);
                    }
                }
                self.pos = saved;
                None
            }
            Tok::Ident(name) => {
                if matches!(self.peek(), Some(Tok::Underscore)) {
                    let sub_saved = self.pos;
                    if let Some(args) = self.parse_subscript_args() {
                        if name.eq_ignore_ascii_case("t") && args.len() == 1 && args[0] > 0.0 {
                            return Some(Distribution::StudentT { df: args[0] });
                        }
                        if name.eq_ignore_ascii_case("f")
                            && args.len() == 2
                            && args[0] > 0.0
                            && args[1] > 0.0
                        {
                            return Some(Distribution::FDist {
                                d1: args[0],
                                d2: args[1],
                            });
                        }
                        if name.eq_ignore_ascii_case("w") && args.len() == 1 {
                            if let Some(n) = as_pos_u64(args[0])
                                && n >= 3
                            {
                                return Some(Distribution::ShapiroWilk { n: n as usize });
                            }
                        }
                    }
                    self.pos = sub_saved;
                }
                if let Some(d) = self.lookup_distribution(&name) {
                    return Some(d);
                }
                self.pos = saved;
                None
            }
            _ => {
                self.pos = saved;
                None
            }
        }
    }

    fn take_rel_op(&mut self) -> Option<RelOp> {
        let op = match self.peek()? {
            Tok::Lt => RelOp::Lt,
            Tok::Le => RelOp::Le,
            Tok::Gt => RelOp::Gt,
            Tok::Ge => RelOp::Ge,
            Tok::Eq => RelOp::Eq,
            _ => return None,
        };
        self.next();
        Some(op)
    }

    fn skip_optional_conditioning(&mut self) {
        if matches!(self.peek(), Some(Tok::Pipe)) {
            self.next();
            let mut depth = 0usize;
            while let Some(tok) = self.peek() {
                match tok {
                    Tok::LParen | Tok::LBracket | Tok::LBrace => {
                        depth += 1;
                        self.next();
                    }
                    Tok::RParen if depth == 0 => break,
                    Tok::RParen | Tok::RBracket | Tok::RBrace => {
                        depth = depth.saturating_sub(1);
                        self.next();
                    }
                    _ => {
                        self.next();
                    }
                }
            }
        }
    }

    /// Parse probability queries `P(Z \le 1.96)`, `P(X = 3)`, `P(-1.96 \le Z \le 1.96)`,
    /// `P(|Z| \le 1.96)`, `P(T_{10} \ge 2.228)`, `P(\chi^2_{10} \ge 18.31)`, `P(W \le 0.927)`.
    fn try_parse_probability(&mut self) -> Option<f64> {
        let saved = self.pos;
        if !matches!(self.peek(), Some(Tok::LParen)) {
            return None;
        }
        self.next();

        // Case 1: `P(|RV| op val)`
        if matches!(self.peek(), Some(Tok::Pipe)) {
            let p1 = self.pos;
            self.next();
            if let Some(dist) = self.try_parse_rv()
                && matches!(self.peek(), Some(Tok::Pipe))
            {
                self.next();
                if let Some(op) = self.take_rel_op()
                    && let Some(val) = self.parse_add()
                {
                    self.skip_optional_conditioning();
                    if matches!(self.peek(), Some(Tok::RParen)) {
                        self.next();
                        self.non_trivial = true;
                        return dist.prob_abs(op, val);
                    }
                }
            }
            self.pos = p1;
        }

        // Case 2: `P(RV op val)`
        let p2 = self.pos;
        if let Some(dist) = self.try_parse_rv()
            && let Some(op) = self.take_rel_op()
            && let Some(val) = self.parse_add()
        {
            self.skip_optional_conditioning();
            if matches!(self.peek(), Some(Tok::RParen)) {
                self.next();
                self.non_trivial = true;
                return dist.prob_single(op, val);
            }
        }
        self.pos = p2;

        // Case 3 & 4: `P(lo op1 RV)` or `P(lo op1 RV op2 hi)`
        if let Some(lo) = self.parse_add()
            && let Some(op1) = self.take_rel_op()
            && let Some(dist) = self.try_parse_rv()
        {
            if let Some(op2) = self.take_rel_op()
                && let Some(hi) = self.parse_add()
            {
                self.skip_optional_conditioning();
                if matches!(self.peek(), Some(Tok::RParen)) {
                    self.next();
                    self.non_trivial = true;
                    return match (op1, op2) {
                        (RelOp::Lt, RelOp::Lt) => dist.prob_interval(lo, false, hi, false),
                        (RelOp::Lt, RelOp::Le) => dist.prob_interval(lo, false, hi, true),
                        (RelOp::Le, RelOp::Lt) => dist.prob_interval(lo, true, hi, false),
                        (RelOp::Le, RelOp::Le) => dist.prob_interval(lo, true, hi, true),
                        (RelOp::Gt, RelOp::Gt) => dist.prob_interval(hi, false, lo, false),
                        (RelOp::Gt, RelOp::Ge) => dist.prob_interval(hi, true, lo, false),
                        (RelOp::Ge, RelOp::Gt) => dist.prob_interval(hi, false, lo, true),
                        (RelOp::Ge, RelOp::Ge) => dist.prob_interval(hi, true, lo, true),
                        _ => None,
                    };
                }
            } else {
                self.skip_optional_conditioning();
                if matches!(self.peek(), Some(Tok::RParen)) {
                    self.next();
                    self.non_trivial = true;
                    let flipped = match op1 {
                        RelOp::Lt => RelOp::Gt,
                        RelOp::Le => RelOp::Ge,
                        RelOp::Gt => RelOp::Lt,
                        RelOp::Ge => RelOp::Le,
                        RelOp::Eq => RelOp::Eq,
                    };
                    return dist.prob_single(flipped, lo);
                }
            }
        }

        self.pos = saved;
        None
    }

    fn try_parse_moment_arg(&mut self) -> Option<Distribution> {
        let saved = self.pos;
        let close = match self.peek()? {
            Tok::LParen => Tok::RParen,
            Tok::LBracket => Tok::RBracket,
            _ => return None,
        };
        self.next();
        let dist = self.try_parse_rv();
        if let Some(d) = dist
            && self.peek() == Some(&close)
        {
            self.next();
            return Some(d);
        }
        self.pos = saved;
        None
    }

    fn try_parse_ident_stat(&mut self, name: &str) -> Option<f64> {
        let saved = self.pos;
        match name {
            "P" if matches!(self.peek(), Some(Tok::LParen)) => {
                if let Some(v) = self.try_parse_probability() {
                    return Some(v);
                }
            }
            "E" if matches!(self.peek(), Some(Tok::LParen | Tok::LBracket)) => {
                if let Some(d) = self.try_parse_moment_arg() {
                    self.non_trivial = true;
                    return d.mean();
                }
            }
            "z" | "Z" if matches!(self.peek(), Some(Tok::Underscore)) => {
                if let Some(args) = self.parse_subscript_args()
                    && args.len() == 1
                    && args[0] > 0.0
                    && args[0] < 1.0
                {
                    self.non_trivial = true;
                    return stats::normal_upper_crit(args[0]);
                }
            }
            "t" | "T" if matches!(self.peek(), Some(Tok::Underscore | Tok::LParen)) => {
                if let Some(args) = self.parse_sub_and_paren_args()
                    && args.len() == 2
                    && let Some((df, alpha)) = split_df_alpha(args[0], args[1])
                {
                    self.non_trivial = true;
                    return stats::t_upper_crit(df, alpha);
                }
            }
            "F" if matches!(self.peek(), Some(Tok::Underscore | Tok::LParen)) => {
                if let Some(args) = self.parse_sub_and_paren_args()
                    && args.len() == 3
                    && let Some((d1, d2, alpha)) = split_f_args(&args)
                {
                    self.non_trivial = true;
                    return stats::f_upper_crit(d1, d2, alpha);
                }
            }
            "W" if matches!(self.peek(), Some(Tok::Underscore | Tok::LParen)) => {
                if let Some(args) = self.parse_sub_and_paren_args() {
                    if args.len() == 2
                        && let Some((n_f, alpha)) = split_df_alpha(args[0], args[1])
                        && let Some(n) = as_pos_u64(n_f)
                        && n >= 3
                    {
                        self.non_trivial = true;
                        return stats::shapiro_wilk_crit(n as usize, alpha);
                    }
                    if args.len() >= 3 {
                        self.non_trivial = true;
                        return stats::shapiro_wilk_stat(&args);
                    }
                }
            }
            "B" if matches!(self.peek(), Some(Tok::LParen)) => {
                if let Some(args) = self.parse_paren_args()
                    && args.len() == 3
                    && let Some(n) = as_pos_u64(args[0])
                {
                    self.non_trivial = true;
                    return stats::binom_cdf(n, args[1], args[2].floor() as i64);
                }
            }
            "C" if matches!(self.peek(), Some(Tok::LParen)) => {
                if let Some(args) = self.parse_paren_args()
                    && args.len() == 2
                    && let Some(n) = as_nonneg_u64(args[0])
                    && let Some(k) = as_nonneg_u64(args[1])
                {
                    self.non_trivial = true;
                    return stats::binom_coeff(n, k);
                }
            }
            "N" if matches!(self.peek(), Some(Tok::LParen)) => {
                if let Some(args) = self.parse_paren_args()
                    && args.len() == 3
                    && args[1] > 0.0
                    && args[1].is_finite()
                {
                    self.non_trivial = true;
                    return Some(stats::normal_cdf((args[2] - args[0]) / args[1].sqrt()));
                }
            }
            _ => {}
        }
        self.pos = saved;
        None
    }

    fn parse_primary(&mut self) -> Option<f64> {
        match self.next()? {
            Tok::Num(v) => Some(v),
            Tok::LParen => {
                let v = self.parse_add()?;
                if !matches!(self.next(), Some(Tok::RParen)) {
                    return None;
                }
                Some(v)
            }
            Tok::LBracket => {
                let v = self.parse_add()?;
                if !matches!(self.next(), Some(Tok::RBracket)) {
                    return None;
                }
                Some(v)
            }
            Tok::LBrace => {
                let v = self.parse_add()?;
                if !matches!(self.next(), Some(Tok::RBrace)) {
                    return None;
                }
                Some(v)
            }
            Tok::Pipe => {
                let v = self.parse_add()?;
                if !matches!(self.next(), Some(Tok::Pipe)) {
                    return None;
                }
                self.non_trivial = true;
                Some(v.abs())
            }
            Tok::Ident(name) => {
                if let Some(v) = self.try_parse_ident_stat(&name) {
                    return Some(v);
                }
                if let Some(&v) = self.env.get(&name) {
                    self.non_trivial = true;
                    Some(v)
                } else if name == "e" {
                    self.non_trivial = true;
                    Some(std::f64::consts::E)
                } else if let Some(v) = lookup_env_case_fallback(self.env, &name) {
                    self.non_trivial = true;
                    Some(v)
                } else {
                    None
                }
            }
            Tok::Cmd(cmd) => self.eval_command(&cmd),
            _ => None,
        }
    }

    fn eval_command(&mut self, cmd: &str) -> Option<f64> {
        match cmd {
            "pi" => {
                self.non_trivial = true;
                Some(std::f64::consts::PI)
            }
            // A handwritten `0` outside an exponent (`^\circ`) is occasionally read as `\circ`.
            "circ" => Some(0.0),
            "frac" | "dfrac" | "tfrac" => {
                let num = self.parse_latex_group_or_unary()?;
                let den = self.parse_latex_group_or_unary()?;
                if den == 0.0 {
                    return None;
                }
                self.non_trivial = true;
                Some(num / den)
            }
            "sqrt" => {
                let root = if matches!(self.peek(), Some(Tok::LBracket)) {
                    self.next();
                    let n = self.parse_add()?;
                    if !matches!(self.next(), Some(Tok::RBracket)) {
                        return None;
                    }
                    n
                } else {
                    2.0
                };
                let arg = self.parse_latex_group_or_unary()?;
                self.non_trivial = true;
                if root == 2.0 {
                    (arg >= 0.0).then(|| arg.sqrt())
                } else if root == 0.0 {
                    None
                } else if arg < 0.0 {
                    let r_int = root.round();
                    if (root - r_int).abs() < 1e-9 && (r_int as i64).abs() % 2 == 1 {
                        Some(-(-arg).powf(1.0 / root))
                    } else {
                        None
                    }
                } else {
                    Some(arg.powf(1.0 / root))
                }
            }
            "log" => {
                let base = if matches!(self.peek(), Some(Tok::Underscore)) {
                    self.next();
                    self.parse_latex_group_or_unary()?
                } else {
                    10.0
                };
                if base <= 0.0 || (base - 1.0).abs() < 1e-12 {
                    return None;
                }
                let arg = self.parse_func_arg()?;
                if arg <= 0.0 {
                    return None;
                }
                self.non_trivial = true;
                Some(arg.ln() / base.ln())
            }
            "ln" => {
                let arg = self.parse_func_arg()?;
                if arg <= 0.0 {
                    return None;
                }
                self.non_trivial = true;
                Some(arg.ln())
            }
            "exp" => {
                let arg = self.parse_func_arg()?;
                self.non_trivial = true;
                Some(arg.exp())
            }
            "abs" => {
                let arg = self.parse_func_arg()?;
                self.non_trivial = true;
                Some(arg.abs())
            }
            "sin" | "cos" | "tan" | "csc" | "sec" | "cot" | "arcsin" | "arccos" | "arctan"
            | "sinh" | "cosh" | "tanh" => {
                // Support optional power like `\sin^2(x)`.
                let func_pow = if matches!(self.peek(), Some(Tok::Caret)) {
                    self.next();
                    Some(self.parse_latex_group_or_unary()?)
                } else {
                    None
                };
                let arg = self.parse_func_arg()?;
                let raw = match cmd {
                    "sin" => arg.sin(),
                    "cos" => arg.cos(),
                    "tan" => {
                        let c = arg.cos();
                        if c.abs() < 1e-12 {
                            return None;
                        }
                        arg.sin() / c
                    }
                    "csc" => {
                        let s = arg.sin();
                        if s.abs() < 1e-12 {
                            return None;
                        }
                        1.0 / s
                    }
                    "sec" => {
                        let c = arg.cos();
                        if c.abs() < 1e-12 {
                            return None;
                        }
                        1.0 / c
                    }
                    "cot" => {
                        let s = arg.sin();
                        if s.abs() < 1e-12 {
                            return None;
                        }
                        arg.cos() / s
                    }
                    "arcsin" => {
                        if !(-1.0..=1.0).contains(&arg) {
                            return None;
                        }
                        arg.asin()
                    }
                    "arccos" => {
                        if !(-1.0..=1.0).contains(&arg) {
                            return None;
                        }
                        arg.acos()
                    }
                    "arctan" => arg.atan(),
                    "sinh" => arg.sinh(),
                    "cosh" => arg.cosh(),
                    "tanh" => arg.tanh(),
                    _ => return None,
                };
                self.non_trivial = true;
                if let Some(p) = func_pow {
                    safe_pow(raw, p)
                } else {
                    Some(raw)
                }
            }
            "Phi" => {
                let is_inv = self.consume_inverse_exponent();
                let arg = self.parse_func_arg()?;
                self.non_trivial = true;
                if is_inv {
                    stats::normal_inv_cdf(arg)
                } else {
                    Some(stats::normal_cdf(arg))
                }
            }
            "phi" | "varphi" => {
                if !self.env.contains_key(cmd) && matches!(self.peek(), Some(Tok::LParen)) {
                    let arg = self.parse_func_arg()?;
                    self.non_trivial = true;
                    Some(stats::normal_pdf(arg))
                } else if let Some(&v) = self.env.get(cmd) {
                    self.non_trivial = true;
                    Some(v)
                } else {
                    None
                }
            }
            "chi" => {
                // Support `\chi^2_{10, 0.05}`, `\chi^2_{0.05}(10)`, `\chi_{0.05, 10}^2`, `\chi^2(10, 0.05)`.
                let had_sq = self.consume_squared_exponent();
                let args = self.parse_sub_and_paren_args()?;
                if !had_sq && !self.consume_squared_exponent() {
                    return None;
                }
                if args.len() != 2 {
                    return None;
                }
                let (df, alpha) = split_df_alpha(args[0], args[1])?;
                self.non_trivial = true;
                stats::chi2_upper_crit(df, alpha)
            }
            "chi2" => {
                let args = self.parse_sub_and_paren_args()?;
                if args.len() != 2 {
                    return None;
                }
                let (df, alpha) = split_df_alpha(args[0], args[1])?;
                self.non_trivial = true;
                stats::chi2_upper_crit(df, alpha)
            }
            "chi2cdf" => {
                let args = self.parse_paren_args()?;
                if args.len() != 2 {
                    return None;
                }
                self.non_trivial = true;
                stats::chi2_cdf(args[0], args[1])
            }
            "chi2inv" => {
                let args = self.parse_paren_args()?;
                if args.len() != 2 || !(args[0] > 0.0 && args[0] < 1.0) {
                    return None;
                }
                self.non_trivial = true;
                stats::chi2_upper_crit(args[1], 1.0 - args[0])
            }
            "normcdf" => {
                let args = self.parse_paren_args()?;
                self.non_trivial = true;
                match args.as_slice() {
                    [z] => Some(stats::normal_cdf(*z)),
                    [x, mu, sigma] if *sigma > 0.0 => Some(stats::normal_cdf((x - mu) / sigma)),
                    _ => None,
                }
            }
            "normpdf" => {
                let args = self.parse_paren_args()?;
                if args.len() != 1 {
                    return None;
                }
                self.non_trivial = true;
                Some(stats::normal_pdf(args[0]))
            }
            "norminv" => {
                let args = self.parse_paren_args()?;
                if args.len() != 1 {
                    return None;
                }
                self.non_trivial = true;
                stats::normal_inv_cdf(args[0])
            }
            "normal" => {
                let args = self.parse_paren_args()?;
                if args.len() != 3 || !(args[1] > 0.0 && args[1].is_finite()) {
                    return None;
                }
                self.non_trivial = true;
                Some(stats::normal_cdf((args[2] - args[0]) / args[1].sqrt()))
            }
            "tcdf" => {
                let args = self.parse_paren_args()?;
                if args.len() != 2 {
                    return None;
                }
                self.non_trivial = true;
                stats::t_cdf(args[0], args[1])
            }
            "tinv" => {
                let args = self.parse_paren_args()?;
                if args.len() != 2 || !(args[0] > 0.0 && args[0] < 1.0) {
                    return None;
                }
                self.non_trivial = true;
                stats::t_upper_crit(args[1], 1.0 - args[0])
            }
            "fcdf" => {
                let args = self.parse_paren_args()?;
                if args.len() != 3 {
                    return None;
                }
                self.non_trivial = true;
                stats::f_cdf(args[0], args[1], args[2])
            }
            "finv" => {
                let args = self.parse_paren_args()?;
                if args.len() != 3 || !(args[0] > 0.0 && args[0] < 1.0) {
                    return None;
                }
                self.non_trivial = true;
                stats::f_upper_crit(args[1], args[2], 1.0 - args[0])
            }
            "binom" if matches!(self.peek(), Some(Tok::LBrace)) => {
                let n = as_nonneg_u64(self.parse_latex_group_or_unary()?)?;
                let k = as_nonneg_u64(self.parse_latex_group_or_unary()?)?;
                self.non_trivial = true;
                stats::binom_coeff(n, k)
            }
            "bin" | "binom" | "binomial" | "binomcdf" => {
                let args = self.parse_paren_args()?;
                if args.len() != 3 {
                    return None;
                }
                let n = as_pos_u64(args[0])?;
                self.non_trivial = true;
                stats::binom_cdf(n, args[1], args[2].floor() as i64)
            }
            "binompdf" => {
                let args = self.parse_paren_args()?;
                if args.len() != 3 {
                    return None;
                }
                let n = as_pos_u64(args[0])?;
                let k = args[2].round();
                if (args[2] - k).abs() > 1e-9 {
                    return Some(0.0);
                }
                self.non_trivial = true;
                stats::binom_pmf(n, args[1], k as i64)
            }
            "pois" | "poisson" | "poisscdf" => {
                let args = self.parse_paren_args()?;
                if args.len() != 2 {
                    return None;
                }
                self.non_trivial = true;
                stats::poisson_cdf(args[0], args[1].floor() as i64)
            }
            "poisspdf" => {
                let args = self.parse_paren_args()?;
                if args.len() != 2 {
                    return None;
                }
                let k = args[1].round();
                if (args[1] - k).abs() > 1e-9 {
                    return Some(0.0);
                }
                self.non_trivial = true;
                stats::poisson_pmf(args[0], k as i64)
            }
            "sw" => {
                let args = self.parse_sub_and_paren_args()?;
                if args.len() == 2 {
                    let (n_f, alpha) = split_df_alpha(args[0], args[1])?;
                    let n = as_pos_u64(n_f)? as usize;
                    self.non_trivial = true;
                    stats::shapiro_wilk_crit(n, alpha)
                } else if args.len() >= 3 {
                    self.non_trivial = true;
                    stats::shapiro_wilk_stat(&args)
                } else {
                    None
                }
            }
            "swa" => {
                let args = self.parse_sub_and_paren_args()?;
                if args.len() != 2 {
                    return None;
                }
                let n = as_pos_u64(args[0])? as usize;
                let i = as_pos_u64(args[1])? as usize;
                self.non_trivial = true;
                stats::shapiro_wilk_weight(n, i)
            }
            "var" => {
                let dist = self.try_parse_moment_arg()?;
                self.non_trivial = true;
                dist.variance()
            }
            "sd" => {
                let dist = self.try_parse_moment_arg()?;
                self.non_trivial = true;
                Some(dist.variance()?.sqrt())
            }
            other => {
                if let Some(&v) = self.env.get(other) {
                    self.non_trivial = true;
                    Some(v)
                } else {
                    None
                }
            }
        }
    }
}

fn as_pos_u64(v: f64) -> Option<u64> {
    if !v.is_finite() || v < 1.0 || v > 1e9 || (v - v.round()).abs() > 1e-9 {
        return None;
    }
    Some(v.round() as u64)
}

fn as_nonneg_u64(v: f64) -> Option<u64> {
    if !v.is_finite() || v < 0.0 || v > 1e9 || (v - v.round()).abs() > 1e-9 {
        return None;
    }
    Some(v.round() as u64)
}

/// Disambiguate `(df, alpha)` vs `(alpha, df)` since `0 < alpha < 1` and `df >= 1` (or `inf`).
fn split_df_alpha(a: f64, b: f64) -> Option<(f64, f64)> {
    let a_is_alpha = a.is_finite() && a > 0.0 && a < 1.0;
    let b_is_alpha = b.is_finite() && b > 0.0 && b < 1.0;
    let a_is_df = a >= 1.0 || a.is_infinite();
    let b_is_df = b >= 1.0 || b.is_infinite();
    if a_is_df && b_is_alpha {
        Some((a, b))
    } else if a_is_alpha && b_is_df {
        Some((b, a))
    } else {
        None
    }
}

/// Disambiguate `(alpha, d1, d2)` vs `(d1, d2, alpha)` for F-distribution critical values.
fn split_f_args(args: &[f64]) -> Option<(f64, f64, f64)> {
    if args.len() != 3 {
        return None;
    }
    let is_alpha = |x: f64| x.is_finite() && x > 0.0 && x < 1.0;
    let is_df = |x: f64| x >= 1.0 || x.is_infinite();
    if is_alpha(args[0]) && is_df(args[1]) && is_df(args[2]) {
        Some((args[1], args[2], args[0]))
    } else if is_df(args[0]) && is_df(args[1]) && is_alpha(args[2]) {
        Some((args[0], args[1], args[2]))
    } else {
        None
    }
}

fn safe_pow(base: f64, exp: f64) -> Option<f64> {
    if !base.is_finite() || !exp.is_finite() {
        return None;
    }
    if base < 0.0 && (exp - exp.round()).abs() > 1e-9 {
        return None;
    }
    let res = base.powf(exp);
    res.is_finite().then_some(res)
}

fn factorial(val: f64) -> Option<f64> {
    if !val.is_finite() || val < 0.0 || val > 170.0 || (val - val.round()).abs() > 1e-9 {
        return None;
    }
    let n = val.round() as u32;
    let mut acc = 1.0_f64;
    for k in 2..=n {
        acc *= k as f64;
    }
    acc.is_finite().then_some(acc)
}

/// Univariate polynomial up to degree 3 (`c[0] + c[1]*x + c[2]*x^2 + c[3]*x^3`).
#[derive(Clone, Copy, Debug)]
struct Poly {
    c: [f64; 4],
}

impl Poly {
    fn constant(v: f64) -> Self {
        Self { c: [v, 0.0, 0.0, 0.0] }
    }
    fn var() -> Self {
        Self { c: [0.0, 1.0, 0.0, 0.0] }
    }
    fn add(self, o: Self) -> Self {
        Self {
            c: [
                self.c[0] + o.c[0],
                self.c[1] + o.c[1],
                self.c[2] + o.c[2],
                self.c[3] + o.c[3],
            ],
        }
    }
    fn sub(self, o: Self) -> Self {
        Self {
            c: [
                self.c[0] - o.c[0],
                self.c[1] - o.c[1],
                self.c[2] - o.c[2],
                self.c[3] - o.c[3],
            ],
        }
    }
    fn neg(self) -> Self {
        Self {
            c: [-self.c[0], -self.c[1], -self.c[2], -self.c[3]],
        }
    }
    fn mul(self, o: Self) -> Option<Self> {
        let mut out = [0.0_f64; 4];
        for i in 0..4 {
            if self.c[i].abs() < 1e-12 {
                continue;
            }
            for j in 0..4 {
                if o.c[j].abs() < 1e-12 {
                    continue;
                }
                if i + j >= 4 {
                    return None;
                }
                out[i + j] += self.c[i] * o.c[j];
            }
        }
        out.iter().all(|v| v.is_finite()).then_some(Self { c: out })
    }
    fn div_const(self, denom: f64) -> Option<Self> {
        if !denom.is_finite() || denom.abs() < 1e-12 {
            return None;
        }
        Some(Self {
            c: [
                self.c[0] / denom,
                self.c[1] / denom,
                self.c[2] / denom,
                self.c[3] / denom,
            ],
        })
    }
    fn is_const(&self) -> Option<f64> {
        (self.c[1].abs() < 1e-10 && self.c[2].abs() < 1e-10 && self.c[3].abs() < 1e-10)
            .then_some(self.c[0])
    }
    fn pow_u32(self, exp: u32) -> Option<Self> {
        if exp > 3 {
            return None;
        }
        let mut acc = Self::constant(1.0);
        for _ in 0..exp {
            acc = acc.mul(self)?;
        }
        Some(acc)
    }
}

struct PolyParser<'a> {
    toks: &'a [Tok],
    pos: usize,
    env: &'a BTreeMap<String, f64>,
    var_name: &'a str,
    depth: usize,
}

impl<'a> PolyParser<'a> {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos)
    }
    fn next(&mut self) -> Option<Tok> {
        let t = self.toks.get(self.pos)?.clone();
        self.pos += 1;
        Some(t)
    }
    fn enter(&mut self) -> Option<()> {
        self.depth += 1;
        (self.depth <= 32).then_some(())
    }
    fn leave(&mut self) {
        self.depth = self.depth.saturating_sub(1);
    }
    fn parse_add(&mut self) -> Option<Poly> {
        self.enter()?;
        let mut lhs = self.parse_mul()?;
        while let Some(tok) = self.peek() {
            match tok {
                Tok::Plus => {
                    self.next();
                    lhs = lhs.add(self.parse_mul()?);
                }
                Tok::Minus => {
                    self.next();
                    lhs = lhs.sub(self.parse_mul()?);
                }
                _ => break,
            }
        }
        self.leave();
        Some(lhs)
    }
    fn starts_implicit_mul(&self) -> bool {
        matches!(
            self.peek(),
            Some(Tok::LParen | Tok::LBracket | Tok::Cmd(_) | Tok::Ident(_))
        )
    }
    fn parse_mul(&mut self) -> Option<Poly> {
        self.enter()?;
        let mut lhs = self.parse_unary()?;
        loop {
            match self.peek() {
                Some(Tok::Mul) => {
                    self.next();
                    lhs = lhs.mul(self.parse_unary()?)?;
                }
                Some(Tok::Div) => {
                    self.next();
                    let rhs = self.parse_unary()?;
                    let d = rhs.is_const()?;
                    lhs = lhs.div_const(d)?;
                }
                _ if self.starts_implicit_mul() => {
                    lhs = lhs.mul(self.parse_unary()?)?;
                }
                _ => break,
            }
        }
        self.leave();
        Some(lhs)
    }
    fn parse_unary(&mut self) -> Option<Poly> {
        match self.peek() {
            Some(Tok::Plus) => {
                self.next();
                self.parse_unary()
            }
            Some(Tok::Minus) => {
                self.next();
                Some(self.parse_unary()?.neg())
            }
            _ => self.parse_power(),
        }
    }
    fn parse_power(&mut self) -> Option<Poly> {
        let base = self.parse_primary()?;
        if matches!(self.peek(), Some(Tok::Caret)) {
            self.next();
            let exp_poly = if matches!(self.peek(), Some(Tok::LBrace)) {
                self.next();
                let v = self.parse_add()?;
                if !matches!(self.next(), Some(Tok::RBrace)) {
                    return None;
                }
                v
            } else {
                self.parse_unary()?
            };
            let exp = exp_poly.is_const()?;
            if let Some(b) = base.is_const() {
                return Some(Poly::constant(safe_pow(b, exp)?));
            }
            if exp < 0.0 || (exp - exp.round()).abs() > 1e-9 || exp > 3.0 {
                return None;
            }
            return base.pow_u32(exp.round() as u32);
        }
        Some(base)
    }
    fn parse_primary(&mut self) -> Option<Poly> {
        match self.next()? {
            Tok::Num(v) => Some(Poly::constant(v)),
            Tok::LParen => {
                let v = self.parse_add()?;
                if !matches!(self.next(), Some(Tok::RParen)) {
                    return None;
                }
                Some(v)
            }
            Tok::LBracket => {
                let v = self.parse_add()?;
                if !matches!(self.next(), Some(Tok::RBracket)) {
                    return None;
                }
                Some(v)
            }
            Tok::LBrace => {
                let v = self.parse_add()?;
                if !matches!(self.next(), Some(Tok::RBrace)) {
                    return None;
                }
                Some(v)
            }
            Tok::Ident(name) => {
                if let Some(&v) = self.env.get(&name) {
                    Some(Poly::constant(v))
                } else if name == "e" {
                    Some(Poly::constant(std::f64::consts::E))
                } else if let Some(v) = lookup_env_case_fallback(self.env, &name) {
                    Some(Poly::constant(v))
                } else if name.eq_ignore_ascii_case(self.var_name) {
                    Some(Poly::var())
                } else {
                    None
                }
            }
            Tok::Cmd(cmd) => match cmd.as_str() {
                "pi" => Some(Poly::constant(std::f64::consts::PI)),
                "circ" => Some(Poly::constant(0.0)),
                "frac" | "dfrac" | "tfrac" => {
                    let num = if matches!(self.peek(), Some(Tok::LBrace)) {
                        self.next();
                        let v = self.parse_add()?;
                        if !matches!(self.next(), Some(Tok::RBrace)) {
                            return None;
                        }
                        v
                    } else {
                        self.parse_unary()?
                    };
                    let den = if matches!(self.peek(), Some(Tok::LBrace)) {
                        self.next();
                        let v = self.parse_add()?;
                        if !matches!(self.next(), Some(Tok::RBrace)) {
                            return None;
                        }
                        v
                    } else {
                        self.parse_unary()?
                    };
                    num.div_const(den.is_const()?)
                }
                _ => None,
            },
            _ => None,
        }
    }
}

fn find_single_unknown(toks: &[Tok], env: &BTreeMap<String, f64>) -> Option<String> {
    let mut unknown: Option<String> = None;
    for tok in toks {
        if let Tok::Ident(name) = tok
            && name != "e"
            && !env.contains_key(name)
            && lookup_env_case_fallback(env, name).is_none()
        {
            match &unknown {
                None => unknown = Some(name.clone()),
                Some(existing) if existing.eq_ignore_ascii_case(name) => {}
                Some(_) => return None, // more than one unknown variable
            }
        }
    }
    unknown
}

fn eval_poly(toks: &[Tok], env: &BTreeMap<String, f64>, var_name: &str) -> Option<Poly> {
    let mut parser = PolyParser {
        toks,
        pos: 0,
        env,
        var_name,
        depth: 0,
    };
    let poly = parser.parse_add()?;
    if parser.pos != toks.len() {
        return None;
    }
    Some(poly)
}

/// Solve a single-variable linear equation `lhs = rhs` (e.g. `2x + 4 = 10` -> `("x", 3.0)`).
fn solve_linear_equation(
    lhs: &str,
    rhs: &str,
    env: &BTreeMap<String, f64>,
) -> Option<(String, f64)> {
    let lt = tokenize(lhs)?;
    let rt = tokenize(rhs)?;
    let mut combined = Vec::with_capacity(lt.len() + rt.len());
    combined.extend(lt.iter().cloned());
    combined.extend(rt.iter().cloned());
    let var = find_single_unknown(&combined, env)?;
    let lp = eval_poly(&lt, env, &var)?;
    let rp = eval_poly(&rt, env, &var)?;
    let diff = lp.sub(rp);
    // Require strictly linear: c2 == 0, c3 == 0, c1 != 0.
    if diff.c[2].abs() > 1e-10 || diff.c[3].abs() > 1e-10 || diff.c[1].abs() < 1e-10 {
        return None;
    }
    let sol = -diff.c[0] / diff.c[1];
    sol.is_finite().then_some((var, sol))
}

fn format_poly(poly: Poly, var: &str) -> Option<String> {
    let mut out = String::new();
    for deg in (0..4).rev() {
        let coef = poly.c[deg];
        if coef.abs() < 1e-9 {
            continue;
        }
        let neg = coef < 0.0;
        let abs = coef.abs();
        if !out.is_empty() {
            out.push(if neg { '-' } else { '+' });
        } else if neg {
            out.push('-');
        }
        let is_one = (abs - 1.0).abs() < 1e-9;
        if deg == 0 || !is_one {
            out.push_str(&format_numeric_answer(abs)?);
        }
        match deg {
            0 => {}
            1 => out.push_str(var),
            2 => {
                out.push_str(var);
                out.push_str("^2");
            }
            3 => {
                out.push_str(var);
                out.push_str("^3");
            }
            _ => unreachable!(),
        }
    }
    if out.is_empty() {
        out.push('0');
    }
    Some(out)
}

/// Simplify a single-variable polynomial expression (e.g. `2x + 3x` -> `5x`, `2(x+3)` -> `2x+6`,
/// `x + 2 - x` -> `2`). Returns `None` if already in canonical simplified form.
fn simplify_simple_algebra(lhs: &str, env: &BTreeMap<String, f64>) -> Option<String> {
    let toks = tokenize(lhs)?;
    let var = find_single_unknown(&toks, env)?;
    // Only synthesize algebraic answers for variables whose handwritten glyphs exist.
    if !matches!(var.as_str(), "x" | "y" | "z" | "a" | "b" | "c" | "t" | "n" | "k") {
        return None;
    }
    let poly = eval_poly(&toks, env, &var)?;
    let formatted = format_poly(poly, &var)?;
    let compact_in: String = lhs
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '{' && *c != '}')
        .collect();
    if compact_in == formatted {
        return None;
    }
    Some(formatted)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solve(s: &str) -> Option<String> {
        evaluate_trailing_equals(s, &BTreeMap::new()).map(|(_, ans)| ans)
    }

    #[test]
    fn evaluates_common_handwritten_numeric_equations() {
        assert_eq!(solve("4+4=").as_deref(), Some("8"));
        assert_eq!(solve("\\sqrt{3}=").as_deref(), Some("1.7321"));
        assert_eq!(solve("\\sqrt{9}=").as_deref(), Some("3"));
        assert_eq!(solve("\\sqrt[3]{27}=").as_deref(), Some("3"));
        assert_eq!(solve("\\frac{1}{2} + \\frac{1}{4} =").as_deref(), Some("0.75"));
        assert_eq!(solve("\\frac{4}{\\frac{4}{4}}=").as_deref(), Some("4"));
        assert_eq!(solve("2^{10}=").as_deref(), Some("1024"));
        assert_eq!(solve("100^{10}=").as_deref(), Some("1e20"));
        assert_eq!(solve("3^2 + 4^2 =").as_deref(), Some("25"));
        assert_eq!(solve("5!=").as_deref(), Some("120"));
        assert_eq!(solve("\\sin(\\pi/6)=").as_deref(), Some("0.5"));
        assert_eq!(solve("\\sin(30^\\circ)=").as_deref(), Some("0.5"));
        assert_eq!(solve("\\sin(\\pi)=").as_deref(), Some("0"));
        assert_eq!(solve("Sin(0)=").as_deref(), Some("0"));
        assert_eq!(solve("Tan(0)=").as_deref(), Some("0"));
        assert_eq!(solve("Tan(45)=").as_deref(), Some("1.6198"));
        assert_eq!(solve("Tan(\\frac{\\pi}{4})=").as_deref(), Some("1"));
        assert_eq!(solve("\\mathrm{Tan}(\\frac{\\pi}{4})=").as_deref(), Some("1"));
        assert_eq!(solve("Tan(45^\\circ)=").as_deref(), Some("1"));
        assert_eq!(solve("tan -\\frac{\\pi}{4}=").as_deref(), Some("-1"));
        assert_eq!(solve("lne=").as_deref(), Some("1"));
        assert_eq!(solve("log_{2}(8)=").as_deref(), Some("3"));
        assert_eq!(solve("\\log_{2}(8)=").as_deref(), Some("3"));
        assert_eq!(solve("\\ln(e^3)=").as_deref(), Some("3"));
        assert_eq!(solve("15\\% \\times 200=").as_deref(), Some("30"));
        assert_eq!(solve("|-7| + 2=").as_deref(), Some("9"));
        assert_eq!(solve("2(3+4)=").as_deref(), Some("14"));
        assert_eq!(solve("\\pi=").as_deref(), Some("3.1416"));
        assert_eq!(solve("\\Pi=").as_deref(), Some("3.1416"));
        assert_eq!(solve("8 - 0 =").as_deref(), Some("8"));
        assert_eq!(solve("8 - \\circ =").as_deref(), Some("8"));
    }

    #[test]
    fn rejects_symbolic_bare_literals_and_invalid_equations() {
        assert_eq!(solve("x + 2 ="), None);
        assert_eq!(solve("x - 2 ="), None);
        assert_eq!(solve("x ="), None);
        assert_eq!(solve("42="), None);
        assert_eq!(solve("4+4"), None);
        assert_eq!(solve("1/0="), None);
        assert_eq!(solve("\\sqrt{-4}="), None);
        assert_eq!(solve("2+2=4="), None);
    }

    #[test]
    fn uses_defined_page_variables_and_simple_algebra() {
        let mut vars = BTreeMap::new();
        vars.insert("x".into(), "5".into());
        assert_eq!(
            evaluate_trailing_equals("2x + 3 =", &vars).map(|(_, a)| a).as_deref(),
            Some("13")
        );

        // Handwritten canvas assignments (`x = 3` -> `x - 2 = 1`, chained `y = 2x` -> `y + 1 = 7`).
        let page_eqs = vec!["x = 3".to_string(), "y = 2x".to_string()];
        assert_eq!(
            evaluate_trailing_equals_with_equations("x - 2 =", &BTreeMap::new(), &page_eqs)
                .map(|(_, a)| a)
                .as_deref(),
            Some("1")
        );
        assert_eq!(
            evaluate_trailing_equals_with_equations("y + 1 =", &BTreeMap::new(), &page_eqs)
                .map(|(_, a)| a)
                .as_deref(),
            Some("7")
        );
        // Bare `x =` after `x = 3` should NOT auto-echo `3` (allows redefining `x = 5`).
        assert_eq!(
            evaluate_trailing_equals_with_equations("x =", &BTreeMap::new(), &page_eqs),
            None
        );

        // Solving a 1-variable linear equation written above (`2x + 4 = 10` -> `x = 3`).
        let linear_eqs = vec!["2x + 4 = 10".to_string()];
        assert_eq!(
            evaluate_trailing_equals_with_equations("x =", &BTreeMap::new(), &linear_eqs)
                .map(|(_, a)| a)
                .as_deref(),
            Some("3")
        );
        assert_eq!(
            evaluate_trailing_equals_with_equations("x + 7 =", &BTreeMap::new(), &linear_eqs)
                .map(|(_, a)| a)
                .as_deref(),
            Some("10")
        );

        // Chained equations with recognizer case flip (`y = 3`, `x = Y^2 + 4` -> `x^2 + y = 172`).
        let chained_case = vec!["y = 3".to_string(), "x = Y^2 + 4".to_string()];
        assert_eq!(
            evaluate_trailing_equals_with_equations("x^2 + y =", &BTreeMap::new(), &chained_case)
                .map(|(_, a)| a)
                .as_deref(),
            Some("172")
        );
        // Exact-case definitions stay distinct when both cases are explicitly defined.
        let both_cases = vec!["x = 2".to_string(), "X = 10".to_string()];
        assert_eq!(
            evaluate_trailing_equals_with_equations("X - x =", &BTreeMap::new(), &both_cases)
                .map(|(_, a)| a)
                .as_deref(),
            Some("8")
        );

        // Simple algebraic simplification without defined variables.
        assert_eq!(solve("2x + 3x =").as_deref(), Some("5x"));
        assert_eq!(solve("2(x + 3) =").as_deref(), Some("2x+6"));
        assert_eq!(solve("x + 2 - x =").as_deref(), Some("2"));
        assert_eq!(solve("6x / 2 =").as_deref(), Some("3x"));
        assert_eq!(solve("(x + 1)^2 =").as_deref(), Some("x^2+2x+1"));
    }

    #[test]
    fn evaluates_statistics_tables_and_probability_queries() {
        let solve_ctx = |expr: &str, eqs: &[&str]| -> Option<String> {
            let owned: Vec<String> = eqs.iter().map(|s| s.to_string()).collect();
            evaluate_trailing_equals_with_equations(expr, &BTreeMap::new(), &owned)
                .map(|(_, ans)| ans)
        };

        // Tab-1: Standard normal CDF, PDF, inverse CDF, critical values, and P(Z ...)
        assert_eq!(solve("\\Phi(1.96) =").as_deref(), Some("0.975"));
        assert_eq!(solve("\\Phi(-1.96) =").as_deref(), Some("0.025"));
        assert_eq!(solve("\\Phi^{-1}(0.975) =").as_deref(), Some("1.96"));
        assert_eq!(solve("\\varphi(1.0) =").as_deref(), Some("0.242"));
        assert_eq!(solve("\\phi(0) =").as_deref(), Some("0.3989"));
        assert_eq!(solve("z_{0.025} =").as_deref(), Some("1.96"));
        assert_eq!(solve("P(Z \\le 1.96) =").as_deref(), Some("0.975"));
        assert_eq!(solve("P(Z \\ge 1.96) =").as_deref(), Some("0.025"));
        assert_eq!(solve("P(-1.96 \\le Z \\le 1.96) =").as_deref(), Some("0.95"));
        assert_eq!(solve("P(|Z| \\le 1.96) =").as_deref(), Some("0.95"));
        assert_eq!(
            solve_ctx("P(X \\le 130) =", &["X \\sim N(100, 15^2)"]).as_deref(),
            Some("0.9772")
        );

        // Tab-2: Student's t critical values (both subscript orders & inf) and probabilities
        assert_eq!(solve("t_{10, 0.05} =").as_deref(), Some("1.8125"));
        assert_eq!(solve("t_{0.05, 10} =").as_deref(), Some("1.8125"));
        assert_eq!(solve("t_{0.025}(10) =").as_deref(), Some("2.2281"));
        assert_eq!(solve("t_{\\infty, 0.025} =").as_deref(), Some("1.96"));
        assert_eq!(solve("P(T_{10} \\ge 2.2281) =").as_deref(), Some("0.025"));
        assert_eq!(
            solve_ctx("P(|T| \\ge 2.2281) =", &["T \\sim t_{10}"]).as_deref(),
            Some("0.05")
        );
        // Combined arithmetic (confidence interval bound)
        assert_eq!(
            solve("12.4 + t_{15, 0.025} \\cdot \\frac{2.1}{\\sqrt{16}} =").as_deref(),
            Some("13.519")
        );

        // Tab-3: Chi-square critical values and probabilities
        assert_eq!(solve("\\chi^2_{10, 0.05} =").as_deref(), Some("18.307"));
        assert_eq!(solve("\\chi^2_{0.05, 10} =").as_deref(), Some("18.307"));
        assert_eq!(solve("\\chi_{0.95, 10}^2 =").as_deref(), Some("3.9403"));
        assert_eq!(solve("P(\\chi^2_{10} \\ge 18.307) =").as_deref(), Some("0.05"));

        // Tab-4..7: F-distribution critical values (alpha = 0.05, 0.025, inf) and probabilities
        assert_eq!(solve("F_{0.05}(5, 10) =").as_deref(), Some("3.3258"));
        assert_eq!(solve("F_{0.025}(5, 10) =").as_deref(), Some("4.2361"));
        assert_eq!(solve("F_{5, 10, 0.05} =").as_deref(), Some("3.3258"));
        assert_eq!(solve("F_{0.05}(\\infty, 10) =").as_deref(), Some("2.5379"));
        assert_eq!(solve("F_{0.05}(5, \\infty) =").as_deref(), Some("2.2141"));
        assert_eq!(solve("P(F_{5, 10} \\ge 3.3258) =").as_deref(), Some("0.05"));

        // Tab-8..10: Shapiro-Wilk critical values, weights, and W statistic
        assert_eq!(solve("W_{30, 0.05} =").as_deref(), Some("0.927"));
        assert_eq!(solve("W_{0.05}(30) =").as_deref(), Some("0.927"));
        assert_eq!(solve("sw_a(30, 1) =").as_deref(), Some("0.4254"));
        assert_eq!(solve("sw_a(30, 15) =").as_deref(), Some("0.0076"));
        assert!(solve("W(1, 2, 3, 4, 5) =").is_some());

        // Tab-11..13: Binomial CDF, PMF, coefficients, page declaration, and moments
        assert_eq!(solve("B(10, 0.3, 3) =").as_deref(), Some("0.6496"));
        assert_eq!(solve("Bin(10, 1/6, 3) =").as_deref(), Some("0.9303"));
        assert_eq!(solve("\\binom{10}{3} =").as_deref(), Some("120"));
        assert_eq!(solve("C(10, 3) =").as_deref(), Some("120"));
        assert_eq!(
            solve_ctx("P(X \\le 3) =", &["n = 10", "p = 0.3", "X \\sim B(n, p)"]).as_deref(),
            Some("0.6496")
        );
        assert_eq!(
            solve_ctx("P(X = 3) =", &["X \\sim B(10, 0.3)"]).as_deref(),
            Some("0.2668")
        );
        assert_eq!(
            solve_ctx("P(X \\ge 3) =", &["X \\sim B(10, 0.3)"]).as_deref(),
            Some("0.6172")
        );
        assert_eq!(solve_ctx("E(X) =", &["X \\sim B(10, 0.3)"]).as_deref(), Some("3"));
        assert_eq!(solve_ctx("Var(X) =", &["X \\sim B(10, 0.3)"]).as_deref(), Some("2.1"));

        // Tab-14..15: Poisson CDF, PMF, page declaration, and moments
        assert_eq!(solve("Pois(2.0, 3) =").as_deref(), Some("0.8571"));
        assert_eq!(solve("Pois(5.0, 4) =").as_deref(), Some("0.4405"));
        assert_eq!(
            solve_ctx("P(X \\le 15) =", &["X \\sim Pois(10)"]).as_deref(),
            Some("0.9513")
        );
        assert_eq!(
            solve_ctx("P(X = 0) =", &["X \\sim Pois(2)"]).as_deref(),
            Some("0.1353")
        );
        assert_eq!(solve_ctx("E[X] =", &["X \\sim Pois(3.2)"]).as_deref(), Some("3.2"));

        // Math Panel entry point (`solve_statistics_panel`): handles with or without trailing `=`
        // and multi-line distribution context while leaving ordinary algebra untouched.
        assert_eq!(
            solve_statistics_panel("\\Phi(1.96)", &BTreeMap::new())
                .map(|(_, a, _)| a)
                .as_deref(),
            Some("0.975")
        );
        assert_eq!(
            solve_statistics_panel("X \\sim B(10, 0.3)\nP(X \\le 3)", &BTreeMap::new())
                .map(|(_, a, _)| a)
                .as_deref(),
            Some("0.6496")
        );
        assert_eq!(solve_statistics_panel("2x + 3x", &BTreeMap::new()), None);

        // Ensure ordinary algebra with `t` and `z` is completely unaffected
        assert_eq!(solve("2t + 3t =").as_deref(), Some("5t"));
        assert_eq!(solve("2z + z =").as_deref(), Some("3z"));
    }
}
