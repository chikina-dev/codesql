use anyhow::{Context, Result};
use sqlparser::ast::{
    BinaryOperator, Expr, FunctionArg, FunctionArgExpr, FunctionArguments, LimitClause, Query,
    Select, SelectItem, SetExpr, Statement, TableFactor, UnaryOperator, Value, ValueWithSpan,
};
use sqlparser::dialect::GenericDialect;
use sqlparser::parser::Parser;
use regex::Regex;
use globset::GlobBuilder;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryPlan {
    pub projections: Vec<Projection>,
    pub filter: Option<Predicate>,
    pub metadata_prefilter: Option<Predicate>,
    pub content_prefilter_terms: Vec<String>,
    pub limit: usize,
    pub order_by: Vec<OrderBy>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderBy {
    pub field: Field,
    pub ascending: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Projection {
    Path,
    LineNo,
    Line,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Predicate {
    And(Box<Predicate>, Box<Predicate>),
    Or(Box<Predicate>, Box<Predicate>),
    Not(Box<Predicate>),
    Eq(Field, String),
    Like(Field, String),
    Contains(String),
    NotEq(Field, String),
    Gt(Field, String),
    Gte(Field, String),
    Lt(Field, String),
    Lte(Field, String),
    InList(Field, Vec<String>),
    Regex(String),
    Glob(Field, String),
    HasSymbol(StringMatch, StringMatch), // kind, name
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StringMatch {
    Exact(String),
    Regex(String),
    Glob(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Path,
    Ext,
    Language,
}

#[derive(Debug, Clone, Copy)]
pub struct MetadataView<'a> {
    pub file_id: i64,
    pub path: &'a str,
    pub ext: &'a str,
    pub language: &'a str,
    pub is_text: bool,
}

pub fn parse(sql: &str) -> Result<QueryPlan> {
    let dialect = GenericDialect {};
    let statements = Parser::parse_sql(&dialect, sql).context("failed to parse query")?;
    if statements.len() != 1 {
        anyhow::bail!("query must contain exactly one statement");
    }

    let query = match &statements[0] {
        Statement::Query(query) => query.as_ref(),
        _ => anyhow::bail!("only SELECT queries are supported"),
    };
    let select = select(query)?;
    validate_from(select)?;
    let projections = parse_projections(select)?;
    let filter = select.selection.as_ref().map(parse_expr).transpose()?;
    let metadata_prefilter = filter.as_ref().and_then(extract_metadata_prefilter);
    let content_prefilter_terms = filter
        .as_ref()
        .map(extract_content_prefilter_terms)
        .unwrap_or_default();
    let limit = parse_limit(query)?;
    let order_by = parse_order_by(query)?;

    if (projections.contains(&Projection::LineNo) || projections.contains(&Projection::Line))
        && !filter_contains_content(filter.as_ref())
    {
        anyhow::bail!("line projections require contains(content, ...) or regex(content, ...)");
    }

    Ok(QueryPlan {
        projections,
        filter,
        metadata_prefilter,
        content_prefilter_terms,
        limit,
        order_by,
    })
}

pub fn evaluate(
    predicate: &Predicate,
    metadata: MetadataView<'_>,
    content: Option<&str>,
    symbol_checker: &impl Fn(i64, &StringMatch, &StringMatch) -> bool,
) -> bool {
    match predicate {
        Predicate::And(left, right) => {
            evaluate(left, metadata, content, symbol_checker) && evaluate(right, metadata, content, symbol_checker)
        }
        Predicate::Or(left, right) => {
            evaluate(left, metadata, content, symbol_checker) || evaluate(right, metadata, content, symbol_checker)
        }
        Predicate::Not(inner) => !evaluate(inner, metadata, content, symbol_checker),
        Predicate::Eq(field, expected) => value_for_field(metadata, *field) == expected,
        Predicate::Like(field, pattern) => like_matches(value_for_field(metadata, *field), pattern),
        Predicate::Contains(needle) => content
            .filter(|_| metadata.is_text)
            .map(|text| text.contains(needle))
            .unwrap_or(false),
        Predicate::NotEq(field, expected) => value_for_field(metadata, *field) != expected,
        Predicate::Gt(field, expected) => value_for_field(metadata, *field) > expected.as_str(),
        Predicate::Gte(field, expected) => value_for_field(metadata, *field) >= expected.as_str(),
        Predicate::Lt(field, expected) => value_for_field(metadata, *field) < expected.as_str(),
        Predicate::Lte(field, expected) => value_for_field(metadata, *field) <= expected.as_str(),
        Predicate::InList(field, values) => {
            let val = value_for_field(metadata, *field);
            values.iter().any(|v| v == val)
        }
        Predicate::Regex(pattern) => content
            .filter(|_| metadata.is_text)
            .and_then(|text| Regex::new(pattern).ok().map(|re| re.is_match(text)))
            .unwrap_or(false),
        Predicate::Glob(field, pattern) => {
            let val = value_for_field(metadata, *field);
            GlobBuilder::new(pattern)
                .literal_separator(true)
                .build()
                .ok()
                .map(|g| g.compile_matcher().is_match(val))
                .unwrap_or(false)
        }
        Predicate::HasSymbol(kind, name) => {
            symbol_checker(metadata.file_id, kind, name)
        }
    }
}

pub fn to_sql(predicate: &Predicate, parameters: &mut Vec<String>) -> Result<String> {
    match predicate {
        Predicate::And(left, right) => Ok(format!(
            " AND (({}) AND ({}))",
            to_sql_fragment(left, parameters)?,
            to_sql_fragment(right, parameters)?
        )),
        Predicate::Or(left, right) => Ok(format!(
            " AND (({}) OR ({}))",
            to_sql_fragment(left, parameters)?,
            to_sql_fragment(right, parameters)?
        )),
        Predicate::Not(inner) => Ok(format!(
            " AND (NOT ({}))",
            to_sql_fragment(inner, parameters)?
        )),
        Predicate::Eq(_, _)
        | Predicate::NotEq(_, _)
        | Predicate::Gt(_, _)
        | Predicate::Gte(_, _)
        | Predicate::Lt(_, _)
        | Predicate::Lte(_, _)
        | Predicate::InList(_, _)
        | Predicate::Like(_, _)
        | Predicate::Glob(_, _)
        | Predicate::HasSymbol(_, _) => Ok(format!(
            " AND ({})",
            to_sql_fragment(predicate, parameters)?
        )),
        Predicate::Contains(_) | Predicate::Regex(_) => {
            anyhow::bail!("content predicates cannot be translated to SQL")
        }
    }
}

pub fn first_content_term(predicate: Option<&Predicate>) -> Option<&str> {
    match predicate? {
        Predicate::Contains(needle) => Some(needle.as_str()),
        Predicate::And(left, right) | Predicate::Or(left, right) => {
            first_content_term(Some(left)).or_else(|| first_content_term(Some(right)))
        }
        Predicate::Not(inner) => first_content_term(Some(inner)),
        _ => None,
    }
}

fn select(query: &Query) -> Result<&Select> {
    match query.body.as_ref() {
        SetExpr::Select(select) => Ok(select.as_ref()),
        _ => anyhow::bail!("only SELECT queries are supported"),
    }
}

fn validate_from(select: &Select) -> Result<()> {
    if select.from.len() != 1 {
        anyhow::bail!("query must target FROM files");
    }
    let relation = &select.from[0].relation;
    match relation {
        TableFactor::Table { name, .. } if name.to_string().eq_ignore_ascii_case("files") => Ok(()),
        _ => anyhow::bail!("query must target FROM files"),
    }
}

fn parse_projections(select: &Select) -> Result<Vec<Projection>> {
    let mut projections = Vec::new();
    for item in &select.projection {
        let projection = match item {
            SelectItem::UnnamedExpr(Expr::Identifier(identifier)) => {
                parse_projection_name(&identifier.value)?
            }
            SelectItem::UnnamedExpr(Expr::CompoundIdentifier(parts)) if parts.len() == 1 => {
                parse_projection_name(&parts[0].value)?
            }
            _ => anyhow::bail!("unsupported projection"),
        };
        projections.push(projection);
    }
    if projections.is_empty() {
        anyhow::bail!("query must select at least one column");
    }
    Ok(projections)
}

fn parse_projection_name(value: &str) -> Result<Projection> {
    match value.to_ascii_lowercase().as_str() {
        "path" => Ok(Projection::Path),
        "line_no" => Ok(Projection::LineNo),
        "line" => Ok(Projection::Line),
        _ => anyhow::bail!("unsupported projection `{value}`"),
    }
}

fn parse_limit(query: &Query) -> Result<usize> {
    let Some(limit_clause) = &query.limit_clause else {
        return Ok(100);
    };

    let limit = match limit_clause {
        LimitClause::LimitOffset { limit, offset, .. } => {
            if offset.is_some() {
                anyhow::bail!("OFFSET is not supported");
            }
            limit.as_ref().context("LIMIT must include a value")?
        }
        LimitClause::OffsetCommaLimit { .. } => anyhow::bail!("OFFSET is not supported"),
    };

    match limit {
        Expr::Value(ValueWithSpan {
            value: Value::Number(number, _),
            ..
        }) => number.parse::<usize>().context("failed to parse LIMIT"),
        _ => anyhow::bail!("LIMIT must be a positive integer"),
    }
}

fn parse_order_by(query: &Query) -> Result<Vec<OrderBy>> {
    let mut order_by = Vec::new();
    let Some(order_by_clause) = &query.order_by else {
        return Ok(order_by);
    };

    let sqlparser::ast::OrderByKind::Expressions(exprs) = &order_by_clause.kind else {
        anyhow::bail!("unsupported ORDER BY kind");
    };

    for expr in exprs {
        let field = parse_field(&expr.expr)?;
        let ascending = expr.options.asc.unwrap_or(true);
        order_by.push(OrderBy { field, ascending });
    }
    Ok(order_by)
}

fn parse_expr(expr: &Expr) -> Result<Predicate> {
    match expr {
        Expr::BinaryOp { left, op, right } => match op {
            BinaryOperator::And => Ok(Predicate::And(
                Box::new(parse_expr(left)?),
                Box::new(parse_expr(right)?),
            )),
            BinaryOperator::Or => Ok(Predicate::Or(
                Box::new(parse_expr(left)?),
                Box::new(parse_expr(right)?),
            )),
            BinaryOperator::Eq => {
                let field = parse_field(left)?;
                let value = parse_string_literal(right)?;
                Ok(Predicate::Eq(field, value))
            }
            BinaryOperator::NotEq => Ok(Predicate::NotEq(
                parse_field(left)?,
                parse_string_literal(right)?,
            )),
            BinaryOperator::Gt => Ok(Predicate::Gt(
                parse_field(left)?,
                parse_string_literal(right)?,
            )),
            BinaryOperator::GtEq => Ok(Predicate::Gte(
                parse_field(left)?,
                parse_string_literal(right)?,
            )),
            BinaryOperator::Lt => Ok(Predicate::Lt(
                parse_field(left)?,
                parse_string_literal(right)?,
            )),
            BinaryOperator::LtEq => Ok(Predicate::Lte(
                parse_field(left)?,
                parse_string_literal(right)?,
            )),
            _ => anyhow::bail!("unsupported operator `{op}`"),
        },
        Expr::Like {
            negated,
            expr,
            pattern,
            ..
        } => {
            let predicate = Predicate::Like(parse_field(expr)?, parse_string_literal(pattern)?);
            if *negated {
                Ok(Predicate::Not(Box::new(predicate)))
            } else {
                Ok(predicate)
            }
        }
        Expr::InList { expr, list, negated } => {
            let field = parse_field(expr)?;
            let mut values = Vec::new();
            for item in list {
                values.push(parse_string_literal(item)?);
            }
            let predicate = Predicate::InList(field, values);
            if *negated {
                Ok(Predicate::Not(Box::new(predicate)))
            } else {
                Ok(predicate)
            }
        }
        Expr::Nested(expr) => parse_expr(expr),
        Expr::UnaryOp {
            op: UnaryOperator::Not,
            expr,
        } => Ok(Predicate::Not(Box::new(parse_expr(expr)?))),
        Expr::Function(function) => parse_function(function),
        _ => anyhow::bail!("unsupported expression"),
    }
}

fn parse_function(function: &sqlparser::ast::Function) -> Result<Predicate> {
    let name = function.name.to_string();
    if name.eq_ignore_ascii_case("contains") {
        let FunctionArguments::List(argument_list) = &function.args else {
            anyhow::bail!("contains expects positional arguments");
        };
        if argument_list.args.len() != 2 {
            anyhow::bail!("contains expects exactly two arguments");
        }

        let first = parse_function_arg(&argument_list.args[0])?;
        if !first.eq_ignore_ascii_case("content") {
            anyhow::bail!("contains only supports the content field");
        }
        let second = parse_function_string_arg(&argument_list.args[1])?;
        Ok(Predicate::Contains(second))
    } else if name.eq_ignore_ascii_case("regex") {
        let FunctionArguments::List(argument_list) = &function.args else {
            anyhow::bail!("regex expects positional arguments");
        };
        if argument_list.args.len() != 2 {
            anyhow::bail!("regex expects exactly two arguments");
        }
        let first = parse_function_arg(&argument_list.args[0])?;
        if !first.eq_ignore_ascii_case("content") {
            anyhow::bail!("regex only supports the content field");
        }
        let second = parse_function_string_arg(&argument_list.args[1])?;
        Ok(Predicate::Regex(second))
    } else if name.eq_ignore_ascii_case("glob") {
        let FunctionArguments::List(argument_list) = &function.args else {
            anyhow::bail!("glob expects positional arguments");
        };
        if argument_list.args.len() != 2 {
            anyhow::bail!("glob expects exactly two arguments");
        }
        // we can pass field to glob instead of assuming path, but usually it's path.
        let first = parse_field_from_arg(&argument_list.args[0])?;
        let second = parse_function_string_arg(&argument_list.args[1])?;
        Ok(Predicate::Glob(first, second))
    } else if name.eq_ignore_ascii_case("has_symbol") {
        let FunctionArguments::List(argument_list) = &function.args else {
            anyhow::bail!("has_symbol expects positional arguments");
        };
        if argument_list.args.len() != 2 {
            anyhow::bail!("has_symbol expects exactly two arguments: kind and name");
        }
        let kind = parse_string_match_arg(&argument_list.args[0])?;
        let name = parse_string_match_arg(&argument_list.args[1])?;
        Ok(Predicate::HasSymbol(kind, name))
    } else {
        anyhow::bail!("unsupported function `{name}`");
    }
}

fn parse_string_match_arg(arg: &FunctionArg) -> Result<StringMatch> {
    match arg {
        FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::Value(ValueWithSpan {
            value: Value::SingleQuotedString(s),
            ..
        }))) => Ok(StringMatch::Exact(s.clone())),
        FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::Function(func))) => {
            let func_name = func.name.to_string().to_ascii_lowercase();
            if func_name == "regex" || func_name == "glob" {
                let FunctionArguments::List(func_args) = &func.args else {
                    anyhow::bail!("{} wrapper expects positional arguments", func_name);
                };
                if func_args.args.len() != 1 {
                    anyhow::bail!("{} wrapper expects exactly one string argument", func_name);
                }
                let pattern = parse_function_string_arg(&func_args.args[0])?;
                if func_name == "regex" {
                    Ok(StringMatch::Regex(pattern))
                } else {
                    Ok(StringMatch::Glob(pattern))
                }
            } else {
                anyhow::bail!("unsupported wrapper function `{}`", func_name);
            }
        }
        _ => anyhow::bail!("expected string literal, regex('...'), or glob('...')"),
    }
}

fn parse_function_arg(arg: &FunctionArg) -> Result<String> {
    match arg {
        FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::Identifier(identifier))) => {
            Ok(identifier.value.clone())
        }
        _ => anyhow::bail!("unsupported function argument"),
    }
}

fn parse_field_from_arg(arg: &FunctionArg) -> Result<Field> {
    match arg {
        FunctionArg::Unnamed(FunctionArgExpr::Expr(expr)) => parse_field(expr),
        _ => anyhow::bail!("unsupported function argument for field"),
    }
}

fn parse_function_string_arg(arg: &FunctionArg) -> Result<String> {
    match arg {
        FunctionArg::Unnamed(FunctionArgExpr::Expr(expr)) => parse_string_literal(expr),
        _ => anyhow::bail!("unsupported function argument"),
    }
}

fn parse_field(expr: &Expr) -> Result<Field> {
    let value = match expr {
        Expr::Identifier(identifier) => identifier.value.as_str(),
        Expr::CompoundIdentifier(parts) if parts.len() == 1 => parts[0].value.as_str(),
        _ => anyhow::bail!("unsupported field reference"),
    };
    match value.to_ascii_lowercase().as_str() {
        "path" => Ok(Field::Path),
        "ext" => Ok(Field::Ext),
        "language" => Ok(Field::Language),
        "content" => anyhow::bail!("content can only be used with contains(content, ...)"),
        _ => anyhow::bail!("unsupported field `{value}`"),
    }
}

fn parse_string_literal(expr: &Expr) -> Result<String> {
    match expr {
        Expr::Value(ValueWithSpan {
            value: Value::SingleQuotedString(value),
            ..
        }) => Ok(value.clone()),
        _ => anyhow::bail!("expected a single-quoted string literal"),
    }
}

fn extract_metadata_prefilter(predicate: &Predicate) -> Option<Predicate> {
    let mut predicates = Vec::new();
    collect_metadata_conjuncts(predicate, &mut predicates);
    build_and_predicate(predicates)
}

fn collect_metadata_conjuncts(predicate: &Predicate, predicates: &mut Vec<Predicate>) {
    match predicate {
        Predicate::And(left, right) => {
            collect_metadata_conjuncts(left, predicates);
            collect_metadata_conjuncts(right, predicates);
        }
        _ if is_metadata_only(predicate) => predicates.push(predicate.clone()),
        _ => {}
    }
}

fn build_and_predicate(mut predicates: Vec<Predicate>) -> Option<Predicate> {
    if predicates.is_empty() {
        return None;
    }
    let first = predicates.remove(0);
    Some(predicates.into_iter().fold(first, |left, right| {
        Predicate::And(Box::new(left), Box::new(right))
    }))
}

fn extract_content_prefilter_terms(predicate: &Predicate) -> Vec<String> {
    let mut terms = Vec::new();
    collect_content_terms(predicate, &mut terms);
    terms
}

fn collect_content_terms(predicate: &Predicate, terms: &mut Vec<String>) {
    match predicate {
        Predicate::And(left, right) => {
            collect_content_terms(left, terms);
            collect_content_terms(right, terms);
        }
        Predicate::Contains(term) => terms.push(term.clone()),
        _ => {}
    }
}

fn filter_contains_content(predicate: Option<&Predicate>) -> bool {
    match predicate {
        Some(Predicate::Contains(_)) | Some(Predicate::Regex(_)) => true,
        Some(Predicate::And(left, right)) | Some(Predicate::Or(left, right)) => {
            filter_contains_content(Some(left)) || filter_contains_content(Some(right))
        }
        Some(Predicate::Not(inner)) => filter_contains_content(Some(inner)),
        _ => false,
    }
}

fn is_metadata_only(predicate: &Predicate) -> bool {
    match predicate {
        Predicate::And(left, right) | Predicate::Or(left, right) => {
            is_metadata_only(left) && is_metadata_only(right)
        }
        Predicate::Not(inner) => is_metadata_only(inner),
        Predicate::Eq(_, _)
        | Predicate::NotEq(_, _)
        | Predicate::Gt(_, _)
        | Predicate::Gte(_, _)
        | Predicate::Lt(_, _)
        | Predicate::Lte(_, _)
        | Predicate::InList(_, _)
        | Predicate::Glob(_, _)
        | Predicate::HasSymbol(_, _)
        | Predicate::Like(_, _) => true,
        Predicate::Contains(_) | Predicate::Regex(_) => false,
    }
}

fn to_sql_fragment(predicate: &Predicate, parameters: &mut Vec<String>) -> Result<String> {
    match predicate {
        Predicate::And(left, right) => Ok(format!(
            "({}) AND ({})",
            to_sql_fragment(left, parameters)?,
            to_sql_fragment(right, parameters)?
        )),
        Predicate::Or(left, right) => Ok(format!(
            "({}) OR ({})",
            to_sql_fragment(left, parameters)?,
            to_sql_fragment(right, parameters)?
        )),
        Predicate::Not(inner) => Ok(format!("NOT ({})", to_sql_fragment(inner, parameters)?)),
        Predicate::Eq(field, value) => {
            parameters.push(value.clone());
            Ok(format!("{} = ?", field.column_name()))
        }
        Predicate::NotEq(field, value) => {
            parameters.push(value.clone());
            Ok(format!("{} != ?", field.column_name()))
        }
        Predicate::Gt(field, value) => {
            parameters.push(value.clone());
            Ok(format!("{} > ?", field.column_name()))
        }
        Predicate::Gte(field, value) => {
            parameters.push(value.clone());
            Ok(format!("{} >= ?", field.column_name()))
        }
        Predicate::Lt(field, value) => {
            parameters.push(value.clone());
            Ok(format!("{} < ?", field.column_name()))
        }
        Predicate::Lte(field, value) => {
            parameters.push(value.clone());
            Ok(format!("{} <= ?", field.column_name()))
        }
        Predicate::InList(field, values) => {
            let mut placeholders = Vec::new();
            for value in values {
                parameters.push(value.clone());
                placeholders.push("?");
            }
            Ok(format!(
                "{} IN ({})",
                field.column_name(),
                placeholders.join(", ")
            ))
        }
        Predicate::Like(field, value) => {
            parameters.push(value.clone());
            Ok(format!("{} LIKE ?", field.column_name()))
        }
        Predicate::Glob(field, value) => {
            // Glob is mapped to LIKE using rudimentary replacement if we push it down,
            // but for full fidelity it's safer to partially match or keep it fully in verifier.
            // Oh wait, SQLite has GLOB operator natively!
            parameters.push(value.clone());
            Ok(format!("{} GLOB ?", field.column_name()))
        }
        Predicate::HasSymbol(kind, name) => {
            let kind_cond = match kind {
                StringMatch::Exact(s) => { parameters.push(s.clone()); "symbols.kind = ?" }
                StringMatch::Regex(s) => { parameters.push(s.clone()); "REGEXP(?, symbols.kind)" }
                StringMatch::Glob(s) => { parameters.push(s.clone()); "symbols.kind GLOB ?" }
            };
            let name_cond = match name {
                StringMatch::Exact(s) => { parameters.push(s.clone()); "symbols.name = ?" }
                StringMatch::Regex(s) => { parameters.push(s.clone()); "REGEXP(?, symbols.name)" }
                StringMatch::Glob(s) => { parameters.push(s.clone()); "symbols.name GLOB ?" }
            };
            Ok(format!("EXISTS (SELECT 1 FROM symbols WHERE symbols.file_id = files.file_id AND {} AND {})", kind_cond, name_cond))
        }
        Predicate::Contains(_) | Predicate::Regex(_) => {
            anyhow::bail!("content predicates cannot be translated to SQL")
        }
    }
}

fn value_for_field<'a>(metadata: MetadataView<'a>, field: Field) -> &'a str {
    match field {
        Field::Path => metadata.path,
        Field::Ext => metadata.ext,
        Field::Language => metadata.language,
    }
}

fn like_matches(value: &str, pattern: &str) -> bool {
    let value = value.as_bytes();
    let pattern = pattern.as_bytes();
    let mut value_index = 0usize;
    let mut pattern_index = 0usize;
    let mut star_pattern_index = None;
    let mut star_value_index = 0usize;

    while value_index < value.len() {
        if pattern_index < pattern.len()
            && (pattern[pattern_index] == b'_' || pattern[pattern_index] == value[value_index])
        {
            value_index += 1;
            pattern_index += 1;
        } else if pattern_index < pattern.len() && pattern[pattern_index] == b'%' {
            star_pattern_index = Some(pattern_index);
            pattern_index += 1;
            star_value_index = value_index;
        } else if let Some(star_index) = star_pattern_index {
            pattern_index = star_index + 1;
            star_value_index += 1;
            value_index = star_value_index;
        } else {
            return false;
        }
    }

    while pattern_index < pattern.len() && pattern[pattern_index] == b'%' {
        pattern_index += 1;
    }

    pattern_index == pattern.len()
}

impl Field {
    pub fn column_name(self) -> &'static str {
        match self {
            Field::Path => "path",
            Field::Ext => "ext",
            Field::Language => "language",
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::query::{Field, MetadataView, Predicate, Projection, evaluate, like_matches, parse};

    #[test]
    fn parse_builds_metadata_and_content_prefilters() {
        let plan = parse(
            "SELECT path FROM files WHERE ext = 'rs' AND contains(content, 'unsafe') LIMIT 20",
        )
        .expect("query should parse");

        assert_eq!(plan.projections, vec![Projection::Path]);
        assert_eq!(plan.content_prefilter_terms, vec!["unsafe"]);
        assert!(plan.metadata_prefilter.is_some());
    }

    #[test]
    fn parse_supports_all_new_syntax() {
        let plan = parse(
            "SELECT path FROM files WHERE ext != 'js' AND ext > 'a' AND ext <= 'z' AND ext IN ('rs', 'ts') AND regex(content, 'TODO.*') AND glob(path, 'src/**') ORDER BY path DESC LIMIT 10",
        ).expect("query should parse");

        assert_eq!(plan.projections, vec![Projection::Path]);
        assert_eq!(plan.limit, 10);
        assert_eq!(plan.order_by.len(), 1);
        assert_eq!(plan.order_by[0].field, Field::Path);
        assert_eq!(plan.order_by[0].ascending, false);
    }

    fn dummy_symbol_checker(_: i64, _: &crate::query::StringMatch, _: &crate::query::StringMatch) -> bool {
        false
    }

    #[test]
    fn evaluate_supports_like_and_contains() {
        let predicate = Predicate::And(
            Box::new(Predicate::Like(Field::Path, "src/%".to_owned())),
            Box::new(Predicate::Contains("TODO".to_owned())),
        );
        let metadata = MetadataView {
            file_id: 1,
            path: "src/lib.rs",
            ext: "rs",
            language: "rust",
            is_text: true,
        };

        assert!(evaluate(&predicate, metadata, Some("TODO"), &dummy_symbol_checker));
        assert!(!evaluate(&predicate, metadata, Some("done"), &dummy_symbol_checker));
    }

    #[test]
    fn like_matches_handles_prefix_patterns() {
        assert!(like_matches("src/lib.rs", "src/%"));
        assert!(!like_matches("docs/lib.rs", "src/%"));
    }
}
