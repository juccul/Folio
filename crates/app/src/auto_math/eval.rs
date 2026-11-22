//! Fast pure-Rust numeric LaTeX evaluator for instant inline Auto Math Solve.
//!
//! Evaluates expressions ending with `=` (e.g. `4+4=`, `\sqrt{3}=`, `\frac{1}{2}+\frac{1}{3}=`)
//! strictly when they yield a single finite real numeric value. Symbolic expressions with
//! unresolved variables or trivial bare numbers (e.g. `5=`) are intentionally rejected.
use std::collections::BTreeMap;

/// Evaluate a recognized LaTeX string that MUST end with `=`.
/// Returns `Some((lhs_latex, formatted_answer))` only when the LHS is a non-trivial numeric
/// expression that evaluates to a finite real number.
pub fn evaluate_trailing_equals(
    latex: &str,
    variables: &BTreeMap<String, String>,
) -> Option<(String, String)> {
    let trimmed = latex.trim();
    if trimmed.is_empty() {
        return None;
    }
    let lhs = strip_trailing_equals(trimmed)?;
    if lhs.is_empty() || contains_relation(lhs) {
        return None;
    }
    let mut env = BTreeMap::new();
    for (name, expr) in variables {
        if let Some((val, _)) = eval_expr_with_env(expr, &BTreeMap::new()) {
            env.insert(name.trim().to_string(), val);
        }
    }
    let (value, non_trivial) = eval_expr_with_env(lhs, &env)?;
    if !non_trivial || !value.is_finite() {
        return None;
    }
    Some((lhs.to_string(), format_numeric_answer(value)?))
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
                    "mathrm" | "operatorname" | "text" => {
                        // Unwrap simple operator text like `\mathrm{sin}` if present.
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
                                out.push(Tok::Cmd(word.to_string()));
                            }
                        }
                    }
                    _ => out.push(Tok::Cmd(cmd.to_string())),
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
            // Recognize plain multi-letter math words like `sqrt`, `sin`, `cos`, `pi` if typed/recognized unescaped.
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
                i += 1;
            }
            let word = &input[start..i];
            if matches!(
                word,
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
            ) {
                out.push(Tok::Cmd(word.to_string()));
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
            b',' => {}
            _ => return None,
        }
    }
    Some(out)
}

fn eval_expr_with_env(expr: &str, env: &BTreeMap<String, f64>) -> Option<(f64, bool)> {
    let tokens = tokenize(expr)?;
    if tokens.is_empty() {
        return None;
    }
    let mut parser = Parser {
        toks: &tokens,
        pos: 0,
        env,
        non_trivial: false,
        depth: 0,
    };
    let val = parser.parse_add()?;
    if parser.pos != parser.toks.len() {
        return None;
    }
    Some((val, parser.non_trivial))
}

struct Parser<'a> {
    toks: &'a [Tok],
    pos: usize,
    env: &'a BTreeMap<String, f64>,
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
            _ => self.parse_power(),
        }
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
                if let Some(&v) = self.env.get(&name) {
                    self.non_trivial = true;
                    Some(v)
                } else if name == "e" {
                    self.non_trivial = true;
                    Some(std::f64::consts::E)
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
        assert_eq!(solve("2^{10}=").as_deref(), Some("1024"));
        assert_eq!(solve("3^2 + 4^2 =").as_deref(), Some("25"));
        assert_eq!(solve("5!=").as_deref(), Some("120"));
        assert_eq!(solve("\\sin(\\pi/6)=").as_deref(), Some("0.5"));
        assert_eq!(solve("\\sin(30^\\circ)=").as_deref(), Some("0.5"));
        assert_eq!(solve("\\sin(\\pi)=").as_deref(), Some("0"));
        assert_eq!(solve("\\log_{2}(8)=").as_deref(), Some("3"));
        assert_eq!(solve("\\ln(e^3)=").as_deref(), Some("3"));
        assert_eq!(solve("15\\% \\times 200=").as_deref(), Some("30"));
        assert_eq!(solve("|-7| + 2=").as_deref(), Some("9"));
        assert_eq!(solve("2(3+4)=").as_deref(), Some("14"));
        assert_eq!(solve("\\pi=").as_deref(), Some("3.1416"));
    }

    #[test]
    fn rejects_symbolic_bare_literals_and_invalid_equations() {
        assert_eq!(solve("x + 2 ="), None);
        assert_eq!(solve("42="), None);
        assert_eq!(solve("4+4"), None);
        assert_eq!(solve("1/0="), None);
        assert_eq!(solve("\\sqrt{-4}="), None);
        assert_eq!(solve("2+2=4="), None);
    }

    #[test]
    fn uses_defined_page_variables() {
        let mut vars = BTreeMap::new();
        vars.insert("x".into(), "5".into());
        assert_eq!(
            evaluate_trailing_equals("2x + 3 =", &vars).map(|(_, a)| a).as_deref(),
            Some("13")
        );
    }
}
