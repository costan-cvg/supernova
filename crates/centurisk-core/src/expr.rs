//! Minimal predicate evaluator for `ResolutionRule.condition` and `AccuracyRule.condition`.
//!
//! Recursive-descent parser + tree-walking evaluator over a small expression grammar.
//! Shared by the temporal resolution strategy and the data quality rule engine.
//!
//! Grammar:
//! ```text
//! expr     = or_expr
//! or_expr  = and_expr ("||" and_expr)*
//! and_expr = cmp_expr ("&&" cmp_expr)*
//! cmp_expr = unary ((">=" | "<=" | "!=" | "==" | ">" | "<") unary)?
//! unary    = "!" unary | primary
//! primary  = ident | number | string | bool | "null" | "(" expr ")"
//! ```

use std::collections::HashMap;

use rust_decimal::Decimal;
use thiserror::Error;

use crate::field_value::FieldValue;

// ── AST ──────────────────────────────────────────────────────────────

/// A parsed expression AST node.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Ident(String),
    Literal(LitValue),
    BinOp { op: Op, left: Box<Expr>, right: Box<Expr> },
    Not(Box<Expr>),
}

/// Literal constant in an expression.
#[derive(Debug, Clone, PartialEq)]
pub enum LitValue {
    Text(String),
    Number(Decimal),
    Bool(bool),
    Null,
}

/// Binary operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Eq, Ne, Lt, Le, Gt, Ge, And, Or,
}

// ── Errors ───────────────────────────────────────────────────────────

#[derive(Debug, Error, PartialEq)]
pub enum ExprError {
    #[error("parse error at position {pos}: {msg}")]
    Parse { pos: usize, msg: String },
    #[error("unknown field: {0}")]
    UnknownField(String),
    #[error("type mismatch: cannot compare {lhs} with {rhs}")]
    TypeMismatch { lhs: String, rhs: String },
    #[error("expected boolean, got {0}")]
    ExpectedBool(String),
}

// ── Parser ───────────────────────────────────────────────────────────

struct Parser<'a> {
    src: &'a str,
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(src: &'a str) -> Self { Self { src, pos: 0 } }

    fn skip_ws(&mut self) {
        while self.pos < self.src.len() && self.src.as_bytes()[self.pos].is_ascii_whitespace() {
            self.pos += 1;
        }
    }

    fn peek(&mut self) -> Option<u8> {
        self.skip_ws();
        self.src.as_bytes().get(self.pos).copied()
    }

    fn starts_with(&mut self, s: &str) -> bool {
        self.skip_ws();
        self.src[self.pos..].starts_with(s)
    }

    fn advance(&mut self, n: usize) { self.pos += n; }

    fn err(&self, msg: impl Into<String>) -> ExprError {
        ExprError::Parse { pos: self.pos, msg: msg.into() }
    }

    // expr = or_expr
    fn expr(&mut self) -> Result<Expr, ExprError> { self.or_expr() }

    // or_expr = and_expr ("||" and_expr)*
    fn or_expr(&mut self) -> Result<Expr, ExprError> {
        let mut left = self.and_expr()?;
        while self.starts_with("||") {
            self.advance(2);
            let right = self.and_expr()?;
            left = Expr::BinOp { op: Op::Or, left: Box::new(left), right: Box::new(right) };
        }
        Ok(left)
    }

    // and_expr = cmp_expr ("&&" cmp_expr)*
    fn and_expr(&mut self) -> Result<Expr, ExprError> {
        let mut left = self.cmp_expr()?;
        while self.starts_with("&&") {
            self.advance(2);
            let right = self.cmp_expr()?;
            left = Expr::BinOp { op: Op::And, left: Box::new(left), right: Box::new(right) };
        }
        Ok(left)
    }

    // cmp_expr = unary ((">=" | "<=" | "!=" | "==" | ">" | "<") unary)?
    fn cmp_expr(&mut self) -> Result<Expr, ExprError> {
        let left = self.unary()?;
        self.skip_ws();
        let op = if self.src[self.pos..].starts_with(">=") { self.advance(2); Some(Op::Ge) }
            else if self.src[self.pos..].starts_with("<=") { self.advance(2); Some(Op::Le) }
            else if self.src[self.pos..].starts_with("!=") { self.advance(2); Some(Op::Ne) }
            else if self.src[self.pos..].starts_with("==") { self.advance(2); Some(Op::Eq) }
            else if self.src[self.pos..].starts_with('>') { self.advance(1); Some(Op::Gt) }
            else if self.src[self.pos..].starts_with('<') { self.advance(1); Some(Op::Lt) }
            else { None };
        match op {
            Some(op) => {
                let right = self.unary()?;
                Ok(Expr::BinOp { op, left: Box::new(left), right: Box::new(right) })
            }
            None => Ok(left),
        }
    }

    // unary = "!" unary | primary
    fn unary(&mut self) -> Result<Expr, ExprError> {
        if self.peek() == Some(b'!') && !self.starts_with("!=") {
            self.advance(1);
            Ok(Expr::Not(Box::new(self.unary()?)))
        } else {
            self.primary()
        }
    }

    // primary = ident | number | string | bool | "null" | "(" expr ")"
    fn primary(&mut self) -> Result<Expr, ExprError> {
        match self.peek() {
            Some(b'(') => {
                self.advance(1);
                let e = self.expr()?;
                if self.peek() != Some(b')') {
                    return Err(self.err("expected ')'"));
                }
                self.advance(1);
                Ok(e)
            }
            Some(b'\'') => {
                self.advance(1); // skip opening quote
                let start = self.pos;
                while self.pos < self.src.len() && self.src.as_bytes()[self.pos] != b'\'' {
                    self.pos += 1;
                }
                if self.pos >= self.src.len() {
                    return Err(self.err("unterminated string literal"));
                }
                let s = self.src[start..self.pos].to_string();
                self.advance(1); // skip closing quote
                Ok(Expr::Literal(LitValue::Text(s)))
            }
            Some(c) if c.is_ascii_digit() => {
                let start = self.pos;
                while self.pos < self.src.len() && (self.src.as_bytes()[self.pos].is_ascii_digit() || self.src.as_bytes()[self.pos] == b'.' || self.src.as_bytes()[self.pos] == b'_') {
                    self.pos += 1;
                }
                let raw: String = self.src[start..self.pos].chars().filter(|c| *c != '_').collect();
                let n: Decimal = raw.parse().map_err(|_| self.err("invalid number"))?;
                Ok(Expr::Literal(LitValue::Number(n)))
            }
            Some(c) if c.is_ascii_alphabetic() || c == b'_' => {
                let start = self.pos;
                while self.pos < self.src.len() && {
                    let b = self.src.as_bytes()[self.pos];
                    b.is_ascii_alphanumeric() || b == b'_'
                } {
                    self.pos += 1;
                }
                let word = &self.src[start..self.pos];
                match word {
                    "true" => Ok(Expr::Literal(LitValue::Bool(true))),
                    "false" => Ok(Expr::Literal(LitValue::Bool(false))),
                    "null" => Ok(Expr::Literal(LitValue::Null)),
                    _ => Ok(Expr::Ident(word.to_string())),
                }
            }
            Some(_) => Err(self.err("unexpected character")),
            None => Err(self.err("unexpected end of input")),
        }
    }
}

/// Parse an expression string into an AST.
pub fn parse(input: &str) -> Result<Expr, ExprError> {
    let mut p = Parser::new(input);
    let expr = p.expr()?;
    p.skip_ws();
    if p.pos != p.src.len() {
        return Err(p.err("unexpected trailing input"));
    }
    Ok(expr)
}

// ── Evaluator ────────────────────────────────────────────────────────

/// Evaluate an expression against a context (field name -> FieldValue map).
///
/// Comparison operators return `FieldValue::Bool`. For `Money` compared against
/// a `Number` literal, the money `amount` is extracted automatically.
pub fn eval(expr: &Expr, ctx: &HashMap<String, FieldValue>) -> Result<FieldValue, ExprError> {
    match expr {
        Expr::Literal(lit) => Ok(lit_to_field(lit)),
        Expr::Ident(name) => ctx
            .get(name.as_str())
            .cloned()
            .ok_or_else(|| ExprError::UnknownField(name.clone())),
        Expr::Not(inner) => {
            let v = eval(inner, ctx)?;
            match v {
                FieldValue::Bool(b) => Ok(FieldValue::Bool(!b)),
                other => Err(ExprError::ExpectedBool(other.type_name().into())),
            }
        }
        Expr::BinOp { op, left, right } => eval_binop(*op, left, right, ctx),
    }
}

fn lit_to_field(lit: &LitValue) -> FieldValue {
    match lit {
        LitValue::Text(s) => FieldValue::Text(s.clone()),
        LitValue::Number(n) => FieldValue::Number(*n),
        LitValue::Bool(b) => FieldValue::Bool(*b),
        LitValue::Null => FieldValue::Null,
    }
}

fn eval_binop(
    op: Op,
    left: &Expr,
    right: &Expr,
    ctx: &HashMap<String, FieldValue>,
) -> Result<FieldValue, ExprError> {
    // Short-circuit for logical operators.
    if op == Op::And {
        let lv = eval(left, ctx)?;
        return match lv {
            FieldValue::Bool(false) => Ok(FieldValue::Bool(false)),
            FieldValue::Bool(true) => {
                let rv = eval(right, ctx)?;
                match rv {
                    FieldValue::Bool(b) => Ok(FieldValue::Bool(b)),
                    other => Err(ExprError::ExpectedBool(other.type_name().into())),
                }
            }
            other => Err(ExprError::ExpectedBool(other.type_name().into())),
        };
    }
    if op == Op::Or {
        let lv = eval(left, ctx)?;
        return match lv {
            FieldValue::Bool(true) => Ok(FieldValue::Bool(true)),
            FieldValue::Bool(false) => {
                let rv = eval(right, ctx)?;
                match rv {
                    FieldValue::Bool(b) => Ok(FieldValue::Bool(b)),
                    other => Err(ExprError::ExpectedBool(other.type_name().into())),
                }
            }
            other => Err(ExprError::ExpectedBool(other.type_name().into())),
        };
    }

    let lv = eval(left, ctx)?;
    let rv = eval(right, ctx)?;

    // Null equality/inequality — any type can be compared to null.
    if matches!(op, Op::Eq | Op::Ne) && (lv.is_null() || rv.is_null()) {
        let both_null = lv.is_null() && rv.is_null();
        return Ok(FieldValue::Bool(if op == Op::Eq { both_null } else { !both_null }));
    }

    cmp_values(op, &lv, &rv)
}

/// Compare two non-null `FieldValue`s. Money amounts are coerced to Decimal
/// when compared against a Number.
fn cmp_values(op: Op, lv: &FieldValue, rv: &FieldValue) -> Result<FieldValue, ExprError> {
    // Extract comparable decimals, coercing Money.amount when paired with Number.
    let nums = match (lv, rv) {
        (FieldValue::Number(a), FieldValue::Number(b)) => Some((*a, *b)),
        (FieldValue::Money { amount, .. }, FieldValue::Number(b)) => Some((*amount, *b)),
        (FieldValue::Number(a), FieldValue::Money { amount, .. }) => Some((*a, *amount)),
        (FieldValue::Money { amount: a, .. }, FieldValue::Money { amount: b, .. }) => Some((*a, *b)),
        _ => None,
    };
    if let Some((a, b)) = nums {
        return Ok(FieldValue::Bool(match op {
            Op::Eq => a == b,
            Op::Ne => a != b,
            Op::Lt => a < b,
            Op::Le => a <= b,
            Op::Gt => a > b,
            Op::Ge => a >= b,
            _ => unreachable!(),
        }));
    }

    // String / Enum equality.
    let strs = match (lv, rv) {
        (FieldValue::Text(a), FieldValue::Text(b)) => Some((a.as_str(), b.as_str())),
        (FieldValue::Enum(a), FieldValue::Text(b))
        | (FieldValue::Text(a), FieldValue::Enum(b))
        | (FieldValue::Enum(a), FieldValue::Enum(b)) => Some((a.as_str(), b.as_str())),
        _ => None,
    };
    if let Some((a, b)) = strs {
        return Ok(FieldValue::Bool(match op {
            Op::Eq => a == b,
            Op::Ne => a != b,
            Op::Lt => a < b,
            Op::Le => a <= b,
            Op::Gt => a > b,
            Op::Ge => a >= b,
            _ => unreachable!(),
        }));
    }

    // Bool equality.
    if let (FieldValue::Bool(a), FieldValue::Bool(b)) = (lv, rv) {
        return Ok(FieldValue::Bool(match op {
            Op::Eq => a == b,
            Op::Ne => a != b,
            _ => {
                return Err(ExprError::TypeMismatch {
                    lhs: "Bool".into(),
                    rhs: "Bool".into(),
                })
            }
        }));
    }

    Err(ExprError::TypeMismatch {
        lhs: lv.type_name().into(),
        rhs: rv.type_name().into(),
    })
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use std::str::FromStr;

    fn ctx_from(pairs: &[(&str, FieldValue)]) -> HashMap<String, FieldValue> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
    }

    fn eval_bool(input: &str, ctx: &HashMap<String, FieldValue>) -> bool {
        let ast = parse(input).expect("parse should succeed");
        match eval(&ast, ctx).expect("eval should succeed") {
            FieldValue::Bool(b) => b,
            other => panic!("expected Bool, got {:?}", other),
        }
    }

    // 1. Money comparison: replacement_cost > 1_000_000
    #[test]
    fn money_field_gt_number_literal() {
        let ctx = ctx_from(&[(
            "replacement_cost",
            FieldValue::Money {
                amount: Decimal::from_str("5000000").unwrap(),
                currency: "USD".into(),
            },
        )]);
        assert!(eval_bool("replacement_cost > 1_000_000", &ctx));
        assert!(!eval_bool("replacement_cost > 10_000_000", &ctx));
    }

    // 2. String equality with && operator
    #[test]
    fn string_equality_and() {
        let ctx = ctx_from(&[
            ("construction_class", FieldValue::Enum("frame".into())),
            ("occupancy", FieldValue::Enum("habitational".into())),
        ]);
        assert!(eval_bool(
            "construction_class == 'frame' && occupancy == 'habitational'",
            &ctx,
        ));
        assert!(!eval_bool(
            "construction_class == 'masonry' && occupancy == 'habitational'",
            &ctx,
        ));
    }

    // 3. Null check with and without field
    #[test]
    fn null_check() {
        let with = ctx_from(&[("sprinkler", FieldValue::Bool(true))]);
        assert!(eval_bool("sprinkler != null", &with));

        let without = ctx_from(&[("sprinkler", FieldValue::Null)]);
        assert!(!eval_bool("sprinkler != null", &without));
    }

    // 4. Operator precedence: && binds tighter than ||
    #[test]
    fn operator_precedence_and_or() {
        // "a > 1 && b > 2 || c > 3" should parse as "(a > 1 && b > 2) || c > 3"
        let ast = parse("a > 1 && b > 2 || c > 3").unwrap();
        // Verify structure: top-level is Or
        match &ast {
            Expr::BinOp { op: Op::Or, left, .. } => match left.as_ref() {
                Expr::BinOp { op: Op::And, .. } => {}
                other => panic!("expected And on left of Or, got {:?}", other),
            },
            other => panic!("expected Or at top level, got {:?}", other),
        }

        // Verify evaluation: a=0, b=0, c=5 => (false && false) || true = true
        let ctx = ctx_from(&[
            ("a", FieldValue::Number(Decimal::from(0))),
            ("b", FieldValue::Number(Decimal::from(0))),
            ("c", FieldValue::Number(Decimal::from(5))),
        ]);
        assert!(eval_bool("a > 1 && b > 2 || c > 3", &ctx));
    }

    // 5. Negation
    #[test]
    fn negation() {
        let ctx = ctx_from(&[("active", FieldValue::Bool(true))]);
        assert!(!eval_bool("!(active == true)", &ctx));
        assert!(eval_bool("!(active == false)", &ctx));
    }

    // 6. Parenthesised grouping
    #[test]
    fn parentheses_grouping() {
        // Without parens: a > 1 && b > 2 || c > 3 = (a>1 && b>2) || c>3
        // With parens:    (a > 1 || b > 2) && c > 3
        let ctx = ctx_from(&[
            ("a", FieldValue::Number(Decimal::from(5))),
            ("b", FieldValue::Number(Decimal::from(0))),
            ("c", FieldValue::Number(Decimal::from(0))),
        ]);
        // a>1 is true, b>2 is false, c>3 is false
        // (true || false) && false = false
        assert!(!eval_bool("(a > 1 || b > 2) && c > 3", &ctx));
        // true && false || false = false (no parens)
        assert!(!eval_bool("a > 1 && b > 2 || c > 3", &ctx));
    }

    // 7. Error cases
    #[test]
    fn parse_error_malformed() {
        assert!(parse("").is_err());
        assert!(parse("a >").is_err());
        assert!(parse("a >> b").is_err());
        assert!(parse("(a > 1").is_err());
        assert!(parse("'unterminated").is_err());
    }

    #[test]
    fn eval_error_unknown_field() {
        let ast = parse("unknown_field > 1").unwrap();
        let ctx = HashMap::new();
        assert!(matches!(eval(&ast, &ctx), Err(ExprError::UnknownField(_))));
    }

    #[test]
    fn eval_error_type_mismatch() {
        let ast = parse("name > 42").unwrap();
        let ctx = ctx_from(&[("name", FieldValue::Bool(true))]);
        assert!(matches!(eval(&ast, &ctx), Err(ExprError::TypeMismatch { .. })));
    }

    // 8. Property test: parse + eval never panics
    fn arb_ident() -> impl Strategy<Value = String> {
        prop::string::string_regex("[a-z][a-z0-9_]{0,8}").unwrap()
    }

    fn arb_expr() -> impl Strategy<Value = String> {
        let leaf = prop_oneof![
            arb_ident(),
            (0i64..1000).prop_map(|n| n.to_string()),
            Just("true".to_string()),
            Just("false".to_string()),
            Just("null".to_string()),
            prop::string::string_regex("'[a-z]{0,5}'").unwrap(),
        ];
        leaf.prop_flat_map(|l| {
            let l2 = l.clone();
            let l3 = l.clone();
            prop_oneof![
                Just(l),
                arb_ident().prop_map(move |r| format!("{} == {}", l2, r)),
                arb_ident().prop_map(move |r| format!("{} != {}", l3, r)),
            ]
        })
    }

    proptest! {
        #[test]
        fn parse_eval_no_panic(input in arb_expr()) {
            // We only care that this doesn't panic — errors are fine.
            if let Ok(ast) = parse(&input) {
                let ctx: HashMap<String, FieldValue> = (0..10)
                    .map(|i| (format!("x{}", i), FieldValue::Number(Decimal::from(i))))
                    .collect();
                let _ = eval(&ast, &ctx);
            }
        }
    }

    // Additional coverage: comparison operators
    #[test]
    fn comparison_operators() {
        let ctx = ctx_from(&[("x", FieldValue::Number(Decimal::from(5)))]);
        assert!(eval_bool("x == 5", &ctx));
        assert!(eval_bool("x != 4", &ctx));
        assert!(eval_bool("x > 4", &ctx));
        assert!(eval_bool("x >= 5", &ctx));
        assert!(eval_bool("x < 6", &ctx));
        assert!(eval_bool("x <= 5", &ctx));
        assert!(!eval_bool("x > 5", &ctx));
        assert!(!eval_bool("x < 5", &ctx));
    }

    #[test]
    fn short_circuit_and() {
        // With short-circuit, the unknown_field should never be evaluated.
        let ctx = ctx_from(&[("flag", FieldValue::Bool(false))]);
        let ast = parse("flag && unknown_field > 1").unwrap();
        assert_eq!(eval(&ast, &ctx).unwrap(), FieldValue::Bool(false));
    }

    #[test]
    fn short_circuit_or() {
        let ctx = ctx_from(&[("flag", FieldValue::Bool(true))]);
        let ast = parse("flag || unknown_field > 1").unwrap();
        assert_eq!(eval(&ast, &ctx).unwrap(), FieldValue::Bool(true));
    }

    #[test]
    fn decimal_number_with_fraction() {
        let ctx = ctx_from(&[("rate", FieldValue::Number(Decimal::from_str("3.14").unwrap()))]);
        assert!(eval_bool("rate > 3.0", &ctx));
        assert!(eval_bool("rate < 3.15", &ctx));
    }

    #[test]
    fn enum_compared_to_string_literal() {
        let ctx = ctx_from(&[("status", FieldValue::Enum("active".into()))]);
        assert!(eval_bool("status == 'active'", &ctx));
        assert!(!eval_bool("status == 'inactive'", &ctx));
    }

    #[test]
    fn nested_not() {
        let ctx = ctx_from(&[("flag", FieldValue::Bool(true))]);
        assert!(eval_bool("!!flag", &ctx));
        assert!(!eval_bool("!!!flag", &ctx));
    }

    #[test]
    fn bool_literal_as_primary() {
        let ctx = HashMap::new();
        assert!(eval_bool("true", &ctx));
        assert!(!eval_bool("false", &ctx));
        assert!(!eval_bool("!true", &ctx));
    }
}
