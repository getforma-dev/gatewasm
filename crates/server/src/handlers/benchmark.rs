use axum::{
    extract::{Query, State},
    response::{Html, IntoResponse, Json},
};
use serde::{Deserialize, Serialize};

use crate::frontend::FrontendSnapshot;
use crate::html_template::{render_page, PageConfig};
use crate::ir;
use crate::{AppState, RenderMode};

/// Maximum number of rows to prevent abuse.
const MAX_ROWS: usize = 100_000;
/// Default number of rows when no `rows` param is specified.
const DEFAULT_ROWS: usize = 1_000;
/// Deterministic seed for the PRNG.
const DEFAULT_SEED: u64 = 0xDEAD_BEEF_CAFE_BABE;

#[derive(Debug, Deserialize)]
pub struct BenchmarkParams {
    rows: Option<usize>,
    sort: Option<String>,
    dir: Option<String>,
    search: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BenchmarkRow {
    pub id: usize,
    pub name: String,
    pub email: String,
    pub role: String,
    pub status: String,
    pub score: f64,
}

/// Deterministic seeded PRNG for benchmark data.
/// Given the same count and seed, always produces identical output.
fn seeded_data(count: usize, seed: u64) -> Vec<BenchmarkRow> {
    let mut state = seed;
    let mut rows = Vec::with_capacity(count);
    for i in 0..count {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        rows.push(BenchmarkRow {
            id: i + 1,
            name: format!("User_{}", state % 10000),
            email: format!("user{}@example.com", state % 10000),
            role: ["admin", "editor", "viewer"][(state as usize) % 3].to_string(),
            status: ["active", "inactive", "pending"][(state as usize / 3) % 3].to_string(),
            score: ((state % 1000) as f64) / 10.0,
        });
    }
    rows
}

/// Apply search filter, sort, and return processed rows.
fn process_rows(params: &BenchmarkParams) -> Vec<BenchmarkRow> {
    let count = params.rows.unwrap_or(DEFAULT_ROWS).min(MAX_ROWS);
    let mut rows = seeded_data(count, DEFAULT_SEED);

    // Search filter
    if let Some(ref query) = params.search {
        let q = query.to_lowercase();
        if !q.is_empty() {
            rows.retain(|r| {
                r.name.to_lowercase().contains(&q)
                    || r.email.to_lowercase().contains(&q)
                    || r.role.to_lowercase().contains(&q)
                    || r.status.to_lowercase().contains(&q)
            });
        }
    }

    // Sort
    let sort_col = params.sort.as_deref().unwrap_or("id");
    let ascending = params.dir.as_deref().unwrap_or("asc") != "desc";

    rows.sort_by(|a, b| {
        let cmp = match sort_col {
            "name" => a.name.cmp(&b.name),
            "email" => a.email.cmp(&b.email),
            "role" => a.role.cmp(&b.role),
            "status" => a.status.cmp(&b.status),
            "score" => a.score.partial_cmp(&b.score).unwrap_or(std::cmp::Ordering::Equal),
            _ => a.id.cmp(&b.id), // default: sort by id
        };
        if ascending { cmp } else { cmp.reverse() }
    });

    rows
}


/// Populate benchmark slot data from IR module defaults.
///
/// Phase 6: Islands are now separate boundaries (ISLAND_START/ISLAND_END).
/// Island-internal content (signals, lists, shows) is rendered client-side
/// via activateIslands(). The IR only contains the static shell + island
/// wrapper divs. No island-internal slots exist to populate.
fn populate_benchmark_slots(
    ir_module: &ir::parser::IrModule,
) -> ir::slot::SlotData {
    ir::slot::SlotData::new_from_defaults(&ir_module.slots)
}

/// GET /platform/benchmark — Benchmark page with three islands.
/// Public page (no authentication required) for performance testing.
pub async fn benchmark_page(
    State(state): State<AppState>,
    Query(params): Query<BenchmarkParams>,
) -> impl IntoResponse {
    let frontend = FrontendSnapshot::capture(&state);
    let row_count = params.rows.unwrap_or(DEFAULT_ROWS).min(MAX_ROWS);
    let rows = process_rows(&params);

    let config_script = format!(
        "window.__FORMA_SERVER_URL__=\"{}\";window.__BENCHMARK_CONFIG__={};window.__BENCHMARK_DATA__={};",
        state.config.server_url,
        serde_json::json!({
            "rows": row_count,
            "sort": params.sort.as_deref().unwrap_or("id"),
            "dir": params.dir.as_deref().unwrap_or("asc"),
            "search": params.search.as_deref().unwrap_or(""),
        }),
        serde_json::json!(rows),
    );

    let route_pattern = "/platform/benchmark";
    let render_mode = frontend.render_mode(route_pattern);
    let ir_module = frontend.ir_module(route_pattern);

    // Populate slot data from IR module defaults.
    // Phase 6: Islands render client-side; only static shell has SSR opcodes.
    let slots = if render_mode == RenderMode::Phase2SsrReconcile {
        ir_module.map(|m| populate_benchmark_slots(m))
    } else {
        None
    };

    let page = render_page(&PageConfig {
        title: "Benchmark - Forma Auth",
        route_pattern,
        manifest: frontend.manifest(),
        config_script: Some(&config_script),
        body_class: None,
        personality_css: Some(
            "body{background:#0a0a0c;color:#e8e6e3;--grid-opacity:0.12;--glow-size:1000px;--glow-intensity:0.08;}",
        ),
        body_prefix: None,
        render_mode,
        ir_module,
        slots: slots.as_ref(),
    });

    (
        [(
            axum::http::header::HeaderName::from_static("content-security-policy"),
            page.csp,
        )],
        Html(page.html),
    )
        .into_response()
}

/// GET /platform/api/benchmark — JSON API for benchmark data.
/// Returns deterministic synthetic data. Query params: rows, sort, dir, search.
pub async fn benchmark_api(
    Query(params): Query<BenchmarkParams>,
) -> impl IntoResponse {
    let row_count = params.rows.unwrap_or(DEFAULT_ROWS).min(MAX_ROWS);
    let rows = process_rows(&params);

    Json(serde_json::json!({
        "rows": rows,
        "total": row_count,
        "sort": params.sort.as_deref().unwrap_or("id"),
        "dir": params.dir.as_deref().unwrap_or("asc"),
        "search": params.search.as_deref().unwrap_or(""),
    }))
}
