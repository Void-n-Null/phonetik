//! HTTP API built with Axum. Used by the `phonetik-server` binary and unit-tested in-process.

use std::sync::Arc;

use axum::body::Body;
use axum::extract::{DefaultBodyLimit, Query, State};
use axum::http::{Response, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use tower_http::cors::CorsLayer;

use crate::Phonetik;

pub struct AppState {
    pub engine: Phonetik,
    health_bytes: Bytes,
}

/// Build the Axum router with the given engine (state applied; suitable for [`axum::serve`] and tests).
pub fn router(engine: Phonetik) -> Router {
    let health_bytes = Bytes::from(
        serde_json::to_vec(&serde_json::json!({
            "status": "ok",
            "service": "Phonetik",
            "dictionaryEntries": engine.word_count(),
        }))
        .expect("health JSON"),
    );
    let state = Arc::new(AppState {
        engine,
        health_bytes,
    });

    Router::new()
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
        .route("/document", post(document_handler))
        .layer(DefaultBodyLimit::max(64 * 1024))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

// ── Health ──────────────────────────────────────────────────────────────

async fn health_handler(State(state): State<Arc<AppState>>) -> Response<Body> {
    Response::builder()
        .status(200)
        .header("content-type", "application/json")
        .body(Body::from(state.health_bytes.clone()))
        .expect("response")
}

// ── Compare ─────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct CompareQuery {
    word1: Option<String>,
    word2: Option<String>,
}

async fn compare_handler(
    State(state): State<Arc<AppState>>,
    Query(q): Query<CompareQuery>,
) -> Result<Json<crate::Comparison>, (StatusCode, Json<ErrorResponse>)> {
    let w1 = q.word1.unwrap_or_default();
    let w2 = q.word2.unwrap_or_default();
    if w1.is_empty() || w2.is_empty() {
        return Err(bad_request(
            "Both 'word1' and 'word2' query parameters are required.",
        ));
    }
    state
        .engine
        .compare(&w1, &w2)
        .ok_or_else(|| not_found("One or both words not found in dictionary."))
        .map(Json)
}

// ── Scan ────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct ScanQuery {
    line1: Option<String>,
    line2: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ScanResponse {
    line1: crate::LineScan,
    line2: Option<crate::LineScan>,
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

        let min_len = scan1.binary_pattern.len().min(scan2.binary_pattern.len());
        let max_len = scan1.binary_pattern.len().max(scan2.binary_pattern.len());
        let mut matches = 0;
        let mut mismatches = Vec::new();
        for i in 0..min_len {
            if scan1.binary_pattern[i] == scan2.binary_pattern[i] {
                matches += 1;
            } else {
                mismatches.push(i);
            }
        }
        for i in min_len..max_len {
            mismatches.push(i);
        }
        let agreement = if max_len > 0 {
            matches as f64 / max_len as f64
        } else {
            1.0
        };

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
        Ok(Json(ScanResponse {
            line1: scan1,
            line2: None,
            comparison: None,
        }))
    }
}

// ── Syllables (single word) ─────────────────────────────────────────────

#[derive(Deserialize)]
struct SyllableQuery {
    word: Option<String>,
}

async fn syllables_handler(
    State(state): State<Arc<AppState>>,
    Query(q): Query<SyllableQuery>,
) -> Result<Json<crate::WordInfo>, (StatusCode, Json<ErrorResponse>)> {
    let w = q.word.unwrap_or_default();
    if w.is_empty() {
        return Err(bad_request("'word' query parameter is required."));
    }
    state
        .engine
        .lookup(&w)
        .ok_or_else(|| not_found("Word not found in dictionary."))
        .map(Json)
}

// ── Syllable counts (batch) ─────────────────────────────────────────────

#[derive(Deserialize)]
struct SyllableBatchRequest {
    lines: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SyllableBatchResponse {
    lines: Vec<crate::LineSyllableCount>,
}

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
struct RhymesQuery {
    word: Option<String>,
    limit: Option<usize>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RhymesResponse {
    word: String,
    phonemes: Vec<String>,
    syllables: usize,
    matches: Vec<crate::RhymeMatch>,
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
    let info = state
        .engine
        .lookup(&w)
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
) -> Result<Json<Vec<crate::RhymeMatch>>, (StatusCode, Json<ErrorResponse>)> {
    let w = q.word.unwrap_or_default();
    if w.is_empty() {
        return Err(bad_request("'word' query parameter is required."));
    }
    let matches = state.engine.perfect_rhymes(&w);
    if matches.is_empty() {
        return Err(not_found("No perfect rhymes found."));
    }
    Ok(Json(matches))
}

async fn slant_rhymes_handler(
    State(state): State<Arc<AppState>>,
    Query(q): Query<RhymesQuery>,
) -> Result<Json<Vec<crate::RhymeMatch>>, (StatusCode, Json<ErrorResponse>)> {
    let w = q.word.unwrap_or_default();
    if w.is_empty() {
        return Err(bad_request("'word' query parameter is required."));
    }
    let limit = q.limit.unwrap_or(200).min(500);
    let matches = state.engine.slant_rhymes(&w, limit);
    if matches.is_empty() {
        return Err(not_found("No slant rhymes found."));
    }
    Ok(Json(matches))
}

async fn near_rhymes_handler(
    State(state): State<Arc<AppState>>,
    Query(q): Query<RhymesQuery>,
) -> Result<Json<Vec<crate::RhymeMatch>>, (StatusCode, Json<ErrorResponse>)> {
    let w = q.word.unwrap_or_default();
    if w.is_empty() {
        return Err(bad_request("'word' query parameter is required."));
    }
    let limit = q.limit.unwrap_or(200).min(500);
    let matches = state.engine.near_rhymes(&w, limit);
    if matches.is_empty() {
        return Err(not_found("No near rhymes found."));
    }
    Ok(Json(matches))
}

// ── Rhyme Map ───────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct RhymeMapRequest {
    lines: Vec<String>,
}

async fn rhymemap_handler(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RhymeMapRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    if req.lines.is_empty() {
        return Err(bad_request(
            "'lines' array is required and must not be empty.",
        ));
    }
    if req.lines.len() > 100 {
        return Err(bad_request("Maximum 100 lines per request."));
    }
    let lines: Vec<&str> = req
        .lines
        .iter()
        .map(|l| if l.len() > 500 { &l[..500] } else { l.as_str() })
        .collect();
    let result = state.engine.rhyme_map(&lines);
    Ok(Json(serde_json::to_value(result).expect("rhyme map JSON")))
}

// ── Document metadata ─────────────────────────────────────────────────

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DocumentRequest {
    lines: Vec<String>,
    #[serde(default)]
    stress_mode: Option<crate::StressMode>,
    #[serde(default)]
    include_rhyme_map: bool,
}

async fn document_handler(
    State(state): State<Arc<AppState>>,
    Json(req): Json<DocumentRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    if req.lines.len() > 100 {
        return Err(bad_request("Maximum 100 lines per request."));
    }
    let opts = crate::DocumentAnalyzeOptions {
        stress_mode: req.stress_mode,
        include_rhyme_map: req.include_rhyme_map,
    };
    let lines: Vec<&str> = req
        .lines
        .iter()
        .map(|l| if l.len() > 500 { &l[..500] } else { l.as_str() })
        .collect();
    let result = state.engine.analyze_document(&lines, &opts);
    Ok(Json(serde_json::to_value(result).expect("document JSON")))
}

// ── Error helpers ───────────────────────────────────────────────────────

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

fn bad_request(msg: &str) -> (StatusCode, Json<ErrorResponse>) {
    (
        StatusCode::BAD_REQUEST,
        Json(ErrorResponse {
            error: msg.into(),
        }),
    )
}

fn not_found(msg: &str) -> (StatusCode, Json<ErrorResponse>) {
    (
        StatusCode::NOT_FOUND,
        Json(ErrorResponse {
            error: msg.into(),
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    fn app() -> Router {
        router(Phonetik::new())
    }

    async fn body_json(res: axum::response::Response) -> serde_json::Value {
        let body = res.into_body().collect().await.unwrap().to_bytes();
        serde_json::from_slice(&body).expect("JSON body")
    }

    #[tokio::test]
    async fn health_returns_ok_and_dictionary_entries() {
        let app = app();
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let v = body_json(res).await;
        assert_eq!(v["status"], "ok");
        assert_eq!(v["service"], "Phonetik");
        assert!(v["dictionaryEntries"].as_u64().unwrap_or(0) > 100_000);
    }

    #[tokio::test]
    async fn compare_happy_path() {
        let app = app();
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/compare?word1=cat&word2=bat")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let v = body_json(res).await;
        assert_eq!(v["rhymeType"], "perfect");
    }

    #[tokio::test]
    async fn compare_missing_params_400() {
        let app = app();
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/compare?word1=cat")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        let v = body_json(res).await;
        assert!(v["error"].as_str().unwrap().contains("word2"));
    }

    #[tokio::test]
    async fn compare_unknown_word_404() {
        let app = app();
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/compare?word1=cat&word2=xyzzyplugh")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn scan_requires_line1() {
        let app = app();
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/scan")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn scan_single_line() {
        let app = app();
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/scan?line1=hello%20world")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let v = body_json(res).await;
        assert!(v.get("line1").is_some());
        assert!(v.get("comparison").unwrap().is_null());
    }

    #[tokio::test]
    async fn syllables_unknown_404() {
        let app = app();
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/syllables?word=notawordzzz")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn syllable_counts_batch() {
        let app = app();
        let body = serde_json::json!({ "lines": ["hello world", "the cat"] }).to_string();
        let res = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/syllable-counts")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let v = body_json(res).await;
        assert_eq!(v["lines"].as_array().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn rhymes_respects_limit_cap() {
        let app = app();
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/rhymes?word=cat&limit=9999")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let v = body_json(res).await;
        let matches = v["matches"].as_array().unwrap();
        assert!(matches.len() <= 500);
    }

    #[tokio::test]
    async fn rhymemap_empty_lines_400() {
        let app = app();
        let body = serde_json::json!({ "lines": [] }).to_string();
        let res = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/rhymemap")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn rhymemap_too_many_lines_400() {
        let app = app();
        let lines: Vec<String> = (0..101).map(|i| format!("line {i}")).collect();
        let body = serde_json::json!({ "lines": lines }).to_string();
        let res = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/rhymemap")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn document_returns_summary_and_lines() {
        let app = app();
        let body = serde_json::json!({
            "lines": ["hello world", "the cat sat"],
            "includeRhymeMap": false,
        })
        .to_string();
        let res = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/document")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let v = body_json(res).await;
        assert_eq!(v["summary"]["lineCount"], 2);
        assert_eq!(v["lines"].as_array().unwrap().len(), 2);
        assert!(v["lines"][0]["scan"]["syllableCount"].as_i64().unwrap() >= 1);
        assert!(v["rhymeMap"].is_null());
    }

    #[tokio::test]
    async fn document_too_many_lines_400() {
        let app = app();
        let lines: Vec<String> = (0..101).map(|i| format!("line {i}")).collect();
        let body = serde_json::json!({ "lines": lines }).to_string();
        let res = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/document")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    }
}
