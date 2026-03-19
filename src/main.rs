use std::net::SocketAddr;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{DefaultBodyLimit, Query, State};
use axum::http::{Response, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use tower_http::cors::CorsLayer;
use tracing::info;

use phonetik::Phonetik;

struct AppState {
    engine: Phonetik,
    health_bytes: Bytes,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "phonetik=info".parse().unwrap()),
        )
        .init();

    let base_dir = std::env::current_dir().unwrap();
    load_dotenv(&base_dir);

    let engine = Phonetik::new();

    let health_bytes = Bytes::from(serde_json::to_vec(&serde_json::json!({
        "status": "ok",
        "service": "Phonetik",
        "dictionaryEntries": engine.word_count(),
    })).unwrap());

    let state = Arc::new(AppState { engine, health_bytes });

    let app = Router::new()
        .route("/health", get(health_handler))
        .route("/compare", get(compare_handler))
        .route("/scan", get(scan_handler))
        .route("/syllables", get(syllables_handler))
        .route("/syllable-counts", post(syllables_batch_handler))
        .route("/rhymes", get(rhymes_handler))
        .route("/rhymes/perfect", get(perfect_rhymes_handler))
        .route("/rhymes/slant", get(slant_rhymes_handler))
        .route("/rhymes/near", get(near_rhymes_handler))
        .route("/rhymemap", post(rhymemap_handler))
        .layer(DefaultBodyLimit::max(64 * 1024))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(1273);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    info!("Phonetik starting on :{port} [{} words]", phonetik::Phonetik::default().word_count());

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

fn load_dotenv(base: &std::path::Path) {
    for candidate in [base.join(".env"), base.join("../.env"), base.join("../../../.env")] {
        if let Ok(contents) = std::fs::read_to_string(&candidate) {
            for line in contents.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() || trimmed.starts_with('#') { continue; }
                if let Some(eq) = trimmed.find('=') {
                    let key = trimmed[..eq].trim();
                    let val = trimmed[eq + 1..].trim();
                    if !key.is_empty() {
                        unsafe { std::env::set_var(key, val) };
                    }
                }
            }
            break;
        }
    }
}

// ── Health ──────────────────────────────────────────────────────────────

async fn health_handler(State(state): State<Arc<AppState>>) -> Response<Body> {
    Response::builder()
        .status(200)
        .header("content-type", "application/json")
        .body(Body::from(state.health_bytes.clone()))
        .unwrap()
}

// ── Compare ─────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct CompareQuery { word1: Option<String>, word2: Option<String> }

async fn compare_handler(
    State(state): State<Arc<AppState>>,
    Query(q): Query<CompareQuery>,
) -> Result<Json<phonetik::Comparison>, (StatusCode, Json<ErrorResponse>)> {
    let w1 = q.word1.unwrap_or_default();
    let w2 = q.word2.unwrap_or_default();
    if w1.is_empty() || w2.is_empty() {
        return Err(bad_request("Both 'word1' and 'word2' query parameters are required."));
    }
    state.engine.compare(&w1, &w2)
        .ok_or_else(|| not_found("One or both words not found in dictionary."))
        .map(Json)
}

// ── Scan ────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct ScanQuery { line1: Option<String>, line2: Option<String> }

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ScanResponse {
    line1: phonetik::LineScan,
    line2: Option<phonetik::LineScan>,
    comparison: Option<ScanComparison>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ScanComparison {
    same_meter: bool,
    same_length: bool,
    stress_agreement: f64,
    mismatch_positions: Vec<usize>,
}

async fn scan_handler(
    State(state): State<Arc<AppState>>,
    Query(q): Query<ScanQuery>,
) -> Result<Json<ScanResponse>, (StatusCode, Json<ErrorResponse>)> {
    let l1 = q.line1.unwrap_or_default();
    if l1.is_empty() {
        return Err(bad_request("'line1' query parameter is required."));
    }

    let scan1 = state.engine.scan(&l1);

    if let Some(l2) = q.line2.filter(|s| !s.is_empty()) {
        let scan2 = state.engine.scan(&l2);

        // Compute comparison between the two binary patterns
        let min_len = scan1.binary_pattern.len().min(scan2.binary_pattern.len());
        let max_len = scan1.binary_pattern.len().max(scan2.binary_pattern.len());
        let mut matches = 0;
        let mut mismatches = Vec::new();
        for i in 0..min_len {
            if scan1.binary_pattern[i] == scan2.binary_pattern[i] { matches += 1; }
            else { mismatches.push(i); }
        }
        for i in min_len..max_len { mismatches.push(i); }
        let agreement = if max_len > 0 { matches as f64 / max_len as f64 } else { 1.0 };

        Ok(Json(ScanResponse {
            comparison: Some(ScanComparison {
                same_meter: scan1.meter.foot_type == scan2.meter.foot_type,
                same_length: scan1.meter.foot_count == scan2.meter.foot_count,
                stress_agreement: (agreement * 10000.0).round() / 10000.0,
                mismatch_positions: mismatches,
            }),
            line1: scan1,
            line2: Some(scan2),
        }))
    } else {
        Ok(Json(ScanResponse { line1: scan1, line2: None, comparison: None }))
    }
}

// ── Syllables (single word) ─────────────────────────────────────────────

#[derive(Deserialize)]
struct SyllableQuery { word: Option<String> }

async fn syllables_handler(
    State(state): State<Arc<AppState>>,
    Query(q): Query<SyllableQuery>,
) -> Result<Json<phonetik::WordInfo>, (StatusCode, Json<ErrorResponse>)> {
    let w = q.word.unwrap_or_default();
    if w.is_empty() {
        return Err(bad_request("'word' query parameter is required."));
    }
    state.engine.lookup(&w)
        .ok_or_else(|| not_found("Word not found in dictionary."))
        .map(Json)
}

// ── Syllable counts (batch) ─────────────────────────────────────────────

#[derive(Deserialize)]
struct SyllableBatchRequest { lines: Vec<String> }

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SyllableBatchResponse { lines: Vec<phonetik::LineSyllableCount> }

async fn syllables_batch_handler(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SyllableBatchRequest>,
) -> Json<SyllableBatchResponse> {
    let str_refs: Vec<&str> = req.lines.iter().map(|s| s.as_str()).collect();
    Json(SyllableBatchResponse {
        lines: state.engine.syllable_counts(&str_refs),
    })
}

// ── Rhymes (merged) ─────────────────────────────────────────────────────

#[derive(Deserialize)]
struct RhymesQuery { word: Option<String>, limit: Option<usize> }

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RhymesResponse {
    word: String,
    phonemes: Vec<String>,
    syllables: usize,
    matches: Vec<phonetik::RhymeMatch>,
}

async fn rhymes_handler(
    State(state): State<Arc<AppState>>,
    Query(q): Query<RhymesQuery>,
) -> Result<Json<RhymesResponse>, (StatusCode, Json<ErrorResponse>)> {
    let w = q.word.unwrap_or_default();
    if w.is_empty() {
        return Err(bad_request("'word' query parameter is required."));
    }
    let limit = q.limit.unwrap_or(200).min(500);
    let info = state.engine.lookup(&w)
        .ok_or_else(|| not_found("Word not found in dictionary."))?;
    let matches = state.engine.rhymes(&w, limit);

    Ok(Json(RhymesResponse {
        word: info.word,
        phonemes: info.phonemes,
        syllables: info.syllable_count,
        matches,
    }))
}

// ── Rhymes (individual type endpoints) ──────────────────────────────────

async fn perfect_rhymes_handler(
    State(state): State<Arc<AppState>>,
    Query(q): Query<RhymesQuery>,
) -> Result<Json<Vec<phonetik::RhymeMatch>>, (StatusCode, Json<ErrorResponse>)> {
    let w = q.word.unwrap_or_default();
    if w.is_empty() { return Err(bad_request("'word' query parameter is required.")); }
    let matches = state.engine.perfect_rhymes(&w);
    if matches.is_empty() { return Err(not_found("No perfect rhymes found.")); }
    Ok(Json(matches))
}

async fn slant_rhymes_handler(
    State(state): State<Arc<AppState>>,
    Query(q): Query<RhymesQuery>,
) -> Result<Json<Vec<phonetik::RhymeMatch>>, (StatusCode, Json<ErrorResponse>)> {
    let w = q.word.unwrap_or_default();
    if w.is_empty() { return Err(bad_request("'word' query parameter is required.")); }
    let limit = q.limit.unwrap_or(200).min(500);
    let matches = state.engine.slant_rhymes(&w, limit);
    if matches.is_empty() { return Err(not_found("No slant rhymes found.")); }
    Ok(Json(matches))
}

async fn near_rhymes_handler(
    State(state): State<Arc<AppState>>,
    Query(q): Query<RhymesQuery>,
) -> Result<Json<Vec<phonetik::RhymeMatch>>, (StatusCode, Json<ErrorResponse>)> {
    let w = q.word.unwrap_or_default();
    if w.is_empty() { return Err(bad_request("'word' query parameter is required.")); }
    let limit = q.limit.unwrap_or(200).min(500);
    let matches = state.engine.near_rhymes(&w, limit);
    if matches.is_empty() { return Err(not_found("No near rhymes found.")); }
    Ok(Json(matches))
}

// ── Rhyme Map ───────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct RhymeMapRequest { lines: Vec<String> }

async fn rhymemap_handler(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RhymeMapRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    if req.lines.is_empty() {
        return Err(bad_request("'lines' array is required and must not be empty."));
    }
    if req.lines.len() > 100 {
        return Err(bad_request("Maximum 100 lines per request."));
    }
    // Cap line length to prevent amplification attacks
    let lines: Vec<&str> = req.lines.iter()
        .map(|l| if l.len() > 500 { &l[..500] } else { l.as_str() })
        .collect();
    let result = state.engine.rhyme_map(&lines);
    Ok(Json(serde_json::to_value(result).unwrap()))
}

// ── Error helpers ───────────────────────────────────────────────────────

#[derive(Serialize)]
struct ErrorResponse { error: String }

fn bad_request(msg: &str) -> (StatusCode, Json<ErrorResponse>) {
    (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: msg.into() }))
}

fn not_found(msg: &str) -> (StatusCode, Json<ErrorResponse>) {
    (StatusCode::NOT_FOUND, Json(ErrorResponse { error: msg.into() }))
}
