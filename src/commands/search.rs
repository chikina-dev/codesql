use std::path::PathBuf;

use anyhow::Result;

use crate::catalog::open_catalog;
use crate::indexing::segment::prefiltered_ids;
use crate::paths::CodesqlPaths;
use crate::query::{self, MetadataView, Projection};
use crate::state::{current_generation, ensure_initialized};
use crate::verifier;

pub fn run(root: PathBuf, sql: &str) -> Result<()> {
    let paths = CodesqlPaths::new(root);
    ensure_initialized(&paths)?;

    let plan = query::parse(sql)?;
    let catalog = open_catalog(&paths)?;
    let mut parameters = Vec::new();
    let where_clause = match &plan.metadata_prefilter {
        Some(predicate) => query::to_sql(predicate, &mut parameters)?,
        None => String::new(),
    };
    let order_by_clause = if plan.order_by.is_empty() {
        "ORDER BY path".to_owned()
    } else {
        let clauses: Vec<String> = plan
            .order_by
            .iter()
            .map(|ob| {
                format!(
                    "{} {}",
                    ob.field.column_name(),
                    if ob.ascending { "ASC" } else { "DESC" }
                )
            })
            .collect();
        format!("ORDER BY {}", clauses.join(", "))
    };

    let mut candidates = catalog.query_files(&where_clause, &order_by_clause, &parameters)?;
    if !plan.content_prefilter_terms.is_empty() {
        candidates.retain(|file| file.is_text);
    }

    let generation = current_generation(&paths)?;
    let prefiltered_ids = prefiltered_ids(&catalog, generation, &plan.content_prefilter_terms)?;
    let wants_line_matches = plan.projections.contains(&Projection::LineNo)
        || plan.projections.contains(&Projection::Line);
    let content_needle = query::first_content_term(plan.filter.as_ref()).map(str::to_owned);
    
    let catalog_mutex = std::sync::Mutex::new(catalog);

    use rayon::prelude::*;

    let mut rows: Vec<String> = candidates.into_par_iter()
        .filter_map(|candidate| {
            if let Some(ids) = prefiltered_ids.as_ref() {
                if !ids.contains(&candidate.file_id) {
                    return None;
                }
            }

            let content = if plan.filter.is_some() && candidate.is_text {
                verifier::read_text(paths.root(), &candidate.path).ok().unwrap_or(None)
            } else {
                None
            };
            let metadata = MetadataView {
                file_id: candidate.file_id,
                path: &candidate.path,
                ext: &candidate.ext,
                language: &candidate.language,
                is_text: candidate.is_text,
            };

            if let Some(predicate) = &plan.filter {
                let symbol_checker = |file_id: i64, kind: &crate::query::StringMatch, name: &crate::query::StringMatch| {
                    let cat = catalog_mutex.lock().unwrap();
                    cat.check_symbol(file_id, kind, name).unwrap_or(false)
                };
                if !query::evaluate(predicate, metadata, content.as_deref(), &symbol_checker) {
                    return None;
                }
            }

            let mut matched_rows = Vec::new();
            if wants_line_matches {
                if let Some(needle) = content_needle.as_deref() {
                    if let Some(text) = content.as_deref() {
                        for found in verifier::find_matches(text, needle) {
                            matched_rows.push(render_row(
                                &plan.projections,
                                &candidate.path,
                                Some(found.line_no),
                                Some(&found.line),
                            ));
                        }
                    }
                }
            } else {
                matched_rows.push(render_row(&plan.projections, &candidate.path, None, None));
            }

            if matched_rows.is_empty() {
                None
            } else {
                Some(matched_rows)
            }
        })
        .flatten()
        .collect();

    rows.truncate(plan.limit);

    if !rows.is_empty() {
        println!("{}", rows.join("\n"));
    }
    Ok(())
}

fn render_row(
    projections: &[Projection],
    path: &str,
    line_no: Option<usize>,
    line: Option<&str>,
) -> String {
    projections
        .iter()
        .map(|projection| match projection {
            Projection::Path => path.to_owned(),
            Projection::LineNo => line_no.map(|value| value.to_string()).unwrap_or_default(),
            Projection::Line => line.unwrap_or_default().to_owned(),
        })
        .collect::<Vec<_>>()
        .join("\t")
}
