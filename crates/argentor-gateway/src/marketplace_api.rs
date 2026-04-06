//! REST API endpoints for the skill marketplace.
//!
//! Provides HTTP endpoints for searching, browsing, installing, and uninstalling
//! skills from the [`MarketplaceManager`]. All endpoints are mounted under
//! `/api/v1/marketplace/` and return JSON responses.
//!
//! # Endpoints
//!
//! | Method   | Path                                | Description              |
//! |----------|-------------------------------------|--------------------------|
//! | `GET`    | `/api/v1/marketplace/search`        | Search catalog           |
//! | `GET`    | `/api/v1/marketplace/featured`      | List featured skills     |
//! | `GET`    | `/api/v1/marketplace/categories`    | List categories          |
//! | `GET`    | `/api/v1/marketplace/skills/:name`  | Get skill details        |
//! | `GET`    | `/api/v1/marketplace/popular`       | Top by downloads         |
//! | `GET`    | `/api/v1/marketplace/recent`        | Recently updated         |
//! | `POST`   | `/api/v1/marketplace/install/:name` | Install a skill          |
//! | `DELETE` | `/api/v1/marketplace/install/:name` | Uninstall a skill        |
//! | `GET`    | `/api/v1/marketplace/installed`     | List installed skills    |
//! | `GET`    | `/api/v1/marketplace/stats`         | Catalog statistics       |

use argentor_skills::{MarketplaceEntry, MarketplaceManager, MarketplaceSearch, SortBy};
use axum::{
    extract::{Json, Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

/// Shared state for the marketplace API endpoints.
pub struct MarketplaceApiState {
    /// The marketplace manager, behind a read-write lock for concurrent access.
    pub manager: RwLock<MarketplaceManager>,
}

// ---------------------------------------------------------------------------
// Query / response types
// ---------------------------------------------------------------------------

/// Query parameters for the search endpoint.
#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    /// Free-text search query.
    pub q: Option<String>,
    /// Filter by category.
    pub category: Option<String>,
    /// Sort order: "relevance", "downloads", "rating", "recent", "name".
    pub sort: Option<String>,
    /// Maximum number of results (default: 50).
    pub limit: Option<usize>,
    /// Number of results to skip (default: 0).
    pub offset: Option<usize>,
}

/// Query parameters for the popular endpoint.
#[derive(Debug, Deserialize)]
pub struct PopularQuery {
    /// Maximum number of results (default: 10).
    pub limit: Option<usize>,
}

/// Query parameters for the recent endpoint.
#[derive(Debug, Deserialize)]
pub struct RecentQuery {
    /// Maximum number of results (default: 10).
    pub limit: Option<usize>,
}

/// JSON-serializable view of a marketplace entry (avoids borrow issues).
#[derive(Debug, Serialize, Deserialize)]
pub struct MarketplaceEntryResponse {
    /// Unique skill name.
    pub name: String,
    /// Semantic version string.
    pub version: String,
    /// Human-readable description.
    pub description: String,
    /// Author name or organization.
    pub author: String,
    /// SPDX license identifier.
    pub license: Option<String>,
    /// Total download count.
    pub downloads: u64,
    /// Average rating (0.0 -- 5.0).
    pub rating: f32,
    /// Number of individual ratings.
    pub ratings_count: u32,
    /// High-level categories.
    pub categories: Vec<String>,
    /// Whether this entry is editorially featured.
    pub featured: bool,
    /// RFC 3339 timestamp of initial publication.
    pub published_at: String,
    /// RFC 3339 timestamp of most recent update.
    pub updated_at: String,
    /// WASM binary size in bytes.
    pub size_bytes: u64,
    /// URL to download the WASM binary.
    pub download_url: Option<String>,
    /// Homepage or documentation URL.
    pub homepage: Option<String>,
    /// Free-form keywords for discoverability.
    pub keywords: Vec<String>,
    /// Tags from the skill manifest.
    pub tags: Vec<String>,
}

impl From<&MarketplaceEntry> for MarketplaceEntryResponse {
    fn from(e: &MarketplaceEntry) -> Self {
        Self {
            name: e.manifest.name.clone(),
            version: e.manifest.version.clone(),
            description: e.manifest.description.clone(),
            author: e.manifest.author.clone(),
            license: e.manifest.license.clone(),
            downloads: e.downloads,
            rating: e.rating,
            ratings_count: e.ratings_count,
            categories: e.categories.clone(),
            featured: e.featured,
            published_at: e.published_at.clone(),
            updated_at: e.updated_at.clone(),
            size_bytes: e.size_bytes,
            download_url: e.download_url.clone(),
            homepage: e.homepage.clone(),
            keywords: e.keywords.clone(),
            tags: e.manifest.tags.clone(),
        }
    }
}

/// Response for the installed skills list.
#[derive(Debug, Serialize, Deserialize)]
pub struct InstalledSkillResponse {
    /// Skill name.
    pub name: String,
    /// Installed version string.
    pub version: String,
    /// RFC 3339 timestamp when the skill was installed.
    pub installed_at: String,
    /// Whether this skill passed the vetting pipeline.
    pub vetted: bool,
}

/// Response for the install endpoint.
#[derive(Debug, Serialize, Deserialize)]
pub struct InstallResponse {
    /// Whether the installation succeeded.
    pub success: bool,
    /// Name of the skill.
    pub name: String,
    /// Human-readable status message.
    pub message: String,
}

/// Response for the uninstall endpoint.
#[derive(Debug, Serialize, Deserialize)]
pub struct UninstallResponse {
    /// Whether the uninstall succeeded.
    pub success: bool,
    /// Name of the skill.
    pub name: String,
    /// Human-readable status message.
    pub message: String,
}

/// Catalog statistics response.
#[derive(Debug, Serialize, Deserialize)]
pub struct CatalogStats {
    /// Total number of skills in the catalog.
    pub total_skills: usize,
    /// Number of unique categories.
    pub total_categories: usize,
    /// Number of locally installed skills.
    pub total_installed: usize,
    /// Number of featured skills.
    pub featured_count: usize,
    /// List of all category names.
    pub categories: Vec<String>,
}

/// Unified error type for marketplace API handlers.
#[derive(Debug)]
pub enum MarketplaceApiError {
    /// The requested resource was not found.
    NotFound(String),
    /// The request was malformed.
    BadRequest(String),
    /// An internal server error occurred.
    Internal(String),
}

impl std::fmt::Display for MarketplaceApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(msg) => write!(f, "Not found: {msg}"),
            Self::BadRequest(msg) => write!(f, "Bad request: {msg}"),
            Self::Internal(msg) => write!(f, "Internal error: {msg}"),
        }
    }
}

impl IntoResponse for MarketplaceApiError {
    fn into_response(self) -> axum::response::Response {
        let (status, message) = match &self {
            Self::NotFound(msg) => (StatusCode::NOT_FOUND, msg.clone()),
            Self::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            Self::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg.clone()),
        };
        let body = serde_json::json!({ "error": message });
        (status, Json(body)).into_response()
    }
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

/// Build the marketplace API sub-router.
///
/// All routes are nested under `/api/v1/marketplace/` and return JSON responses.
pub fn marketplace_router(state: Arc<MarketplaceApiState>) -> Router {
    Router::new()
        .route("/api/v1/marketplace/search", get(search_catalog))
        .route("/api/v1/marketplace/featured", get(list_featured))
        .route("/api/v1/marketplace/categories", get(list_categories))
        .route("/api/v1/marketplace/skills/{name}", get(get_skill_details))
        .route("/api/v1/marketplace/popular", get(list_popular))
        .route("/api/v1/marketplace/recent", get(list_recent))
        .route("/api/v1/marketplace/install/{name}", post(install_skill))
        .route(
            "/api/v1/marketplace/install/{name}",
            delete(uninstall_skill),
        )
        .route("/api/v1/marketplace/installed", get(list_installed))
        .route("/api/v1/marketplace/stats", get(get_stats))
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn parse_sort(s: &str) -> SortBy {
    match s.to_lowercase().as_str() {
        "downloads" => SortBy::Downloads,
        "rating" => SortBy::Rating,
        "recent" => SortBy::Recent,
        "name" => SortBy::Name,
        _ => SortBy::Relevance,
    }
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// `GET /api/v1/marketplace/search?q=&category=&sort=&limit=&offset=`
async fn search_catalog(
    State(state): State<Arc<MarketplaceApiState>>,
    Query(params): Query<SearchQuery>,
) -> Result<Json<Vec<MarketplaceEntryResponse>>, MarketplaceApiError> {
    let mgr = state.manager.read().await;
    let search = MarketplaceSearch {
        query: params.q,
        category: params.category,
        author: None,
        min_rating: None,
        tags: Vec::new(),
        sort_by: params
            .sort
            .as_deref()
            .map(parse_sort)
            .unwrap_or(SortBy::Relevance),
        limit: params.limit.unwrap_or(50),
        offset: params.offset.unwrap_or(0),
    };

    let results = mgr.catalog().search(&search);
    let response: Vec<MarketplaceEntryResponse> = results
        .iter()
        .map(|e| MarketplaceEntryResponse::from(*e))
        .collect();
    Ok(Json(response))
}

/// `GET /api/v1/marketplace/featured`
async fn list_featured(
    State(state): State<Arc<MarketplaceApiState>>,
) -> Result<Json<Vec<MarketplaceEntryResponse>>, MarketplaceApiError> {
    let mgr = state.manager.read().await;
    let featured = mgr.catalog().list_featured();
    let response: Vec<MarketplaceEntryResponse> = featured
        .iter()
        .map(|e| MarketplaceEntryResponse::from(*e))
        .collect();
    Ok(Json(response))
}

/// `GET /api/v1/marketplace/categories`
async fn list_categories(
    State(state): State<Arc<MarketplaceApiState>>,
) -> Result<Json<Vec<String>>, MarketplaceApiError> {
    let mgr = state.manager.read().await;
    Ok(Json(mgr.catalog().categories()))
}

/// `GET /api/v1/marketplace/skills/:name`
async fn get_skill_details(
    State(state): State<Arc<MarketplaceApiState>>,
    Path(name): Path<String>,
) -> Result<Json<MarketplaceEntryResponse>, MarketplaceApiError> {
    let mgr = state.manager.read().await;
    let entry = mgr.catalog().get(&name).ok_or_else(|| {
        MarketplaceApiError::NotFound(format!("Skill '{name}' not found in catalog"))
    })?;
    Ok(Json(MarketplaceEntryResponse::from(entry)))
}

/// `GET /api/v1/marketplace/popular?limit=`
async fn list_popular(
    State(state): State<Arc<MarketplaceApiState>>,
    Query(params): Query<PopularQuery>,
) -> Result<Json<Vec<MarketplaceEntryResponse>>, MarketplaceApiError> {
    let mgr = state.manager.read().await;
    let limit = params.limit.unwrap_or(10);
    let popular = mgr.catalog().list_popular(limit);
    let response: Vec<MarketplaceEntryResponse> = popular
        .iter()
        .map(|e| MarketplaceEntryResponse::from(*e))
        .collect();
    Ok(Json(response))
}

/// `GET /api/v1/marketplace/recent?limit=`
async fn list_recent(
    State(state): State<Arc<MarketplaceApiState>>,
    Query(params): Query<RecentQuery>,
) -> Result<Json<Vec<MarketplaceEntryResponse>>, MarketplaceApiError> {
    let mgr = state.manager.read().await;
    let limit = params.limit.unwrap_or(10);
    let recent = mgr.catalog().list_recent(limit);
    let response: Vec<MarketplaceEntryResponse> = recent
        .iter()
        .map(|e| MarketplaceEntryResponse::from(*e))
        .collect();
    Ok(Json(response))
}

/// `POST /api/v1/marketplace/install/:name`
///
/// Looks up the skill in the catalog, creates a minimal manifest from catalog data,
/// and runs the install pipeline (vetting + installation) with placeholder WASM bytes.
async fn install_skill(
    State(state): State<Arc<MarketplaceApiState>>,
    Path(name): Path<String>,
) -> Result<Json<InstallResponse>, MarketplaceApiError> {
    let mut mgr = state.manager.write().await;

    // Verify the skill exists in catalog
    let entry = mgr.catalog().get(&name).ok_or_else(|| {
        MarketplaceApiError::NotFound(format!("Skill '{name}' not found in catalog"))
    })?;

    // Check if already installed
    if mgr.is_installed(&name) {
        return Ok(Json(InstallResponse {
            success: false,
            name: name.clone(),
            message: format!("Skill '{name}' is already installed"),
        }));
    }

    let manifest = entry.manifest.clone();

    // Install with placeholder bytes (in real deployment these would be fetched)
    let placeholder_wasm = b"(module)";
    match mgr.install_from_bytes(manifest, placeholder_wasm) {
        Ok(result) => {
            info!(skill = %name, "Skill installed via marketplace API");
            Ok(Json(InstallResponse {
                success: result.passed,
                name: name.clone(),
                message: if result.passed {
                    format!("Skill '{name}' installed successfully")
                } else {
                    format!(
                        "Skill '{name}' installation failed vetting: {}",
                        result
                            .checks
                            .iter()
                            .filter(|c| !c.passed)
                            .map(|c| c.message.clone())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                },
            }))
        }
        Err(e) => {
            warn!(skill = %name, error = %e, "Skill installation failed");
            Err(MarketplaceApiError::Internal(format!(
                "Failed to install skill '{name}': {e}"
            )))
        }
    }
}

/// `DELETE /api/v1/marketplace/install/:name`
async fn uninstall_skill(
    State(state): State<Arc<MarketplaceApiState>>,
    Path(name): Path<String>,
) -> Result<Json<UninstallResponse>, MarketplaceApiError> {
    let mut mgr = state.manager.write().await;

    if !mgr.is_installed(&name) {
        return Err(MarketplaceApiError::NotFound(format!(
            "Skill '{name}' is not installed"
        )));
    }

    match mgr.uninstall(&name) {
        Ok(removed) => {
            info!(skill = %name, removed, "Skill uninstalled via marketplace API");
            Ok(Json(UninstallResponse {
                success: removed,
                name: name.clone(),
                message: if removed {
                    format!("Skill '{name}' uninstalled successfully")
                } else {
                    format!("Skill '{name}' was not found in the index")
                },
            }))
        }
        Err(e) => {
            warn!(skill = %name, error = %e, "Skill uninstall failed");
            Err(MarketplaceApiError::Internal(format!(
                "Failed to uninstall skill '{name}': {e}"
            )))
        }
    }
}

/// `GET /api/v1/marketplace/installed`
async fn list_installed(
    State(state): State<Arc<MarketplaceApiState>>,
) -> Result<Json<Vec<InstalledSkillResponse>>, MarketplaceApiError> {
    let mgr = state.manager.read().await;
    let installed: Vec<InstalledSkillResponse> = mgr
        .installed()
        .list()
        .iter()
        .map(|entry| InstalledSkillResponse {
            name: entry.manifest.name.clone(),
            version: entry.manifest.version.clone(),
            installed_at: entry.installed_at.clone(),
            vetted: entry.vetted,
        })
        .collect();
    Ok(Json(installed))
}

/// `GET /api/v1/marketplace/stats`
async fn get_stats(
    State(state): State<Arc<MarketplaceApiState>>,
) -> Result<Json<CatalogStats>, MarketplaceApiError> {
    let mgr = state.manager.read().await;
    let catalog = mgr.catalog();
    let categories = catalog.categories();
    let featured_count = catalog.list_featured().len();
    let total = catalog.count();
    let installed = mgr.installed().list().len();

    Ok(Json(CatalogStats {
        total_skills: total,
        total_categories: categories.len(),
        total_installed: installed,
        featured_count,
        categories,
    }))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use argentor_skills::{builtin_catalog_entries, MarketplaceManager};
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    /// Create a test state with the built-in catalog entries loaded.
    fn test_state() -> Arc<MarketplaceApiState> {
        let dir = std::env::temp_dir().join("argentor_marketplace_api_test");
        let _ = std::fs::create_dir_all(&dir);
        let catalog_path = dir.join("catalog.json");
        let mut mgr = MarketplaceManager::new(dir, catalog_path);
        let entries = builtin_catalog_entries();
        mgr.update_catalog(entries);
        Arc::new(MarketplaceApiState {
            manager: RwLock::new(mgr),
        })
    }

    fn app(state: Arc<MarketplaceApiState>) -> Router {
        marketplace_router(state)
    }

    // -- Test: search returns results --

    #[tokio::test]
    async fn test_search_returns_results() {
        let state = test_state();
        let router = app(state);

        let resp = router
            .oneshot(
                Request::builder()
                    .uri("/api/v1/marketplace/search?q=calculator")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let results: Vec<MarketplaceEntryResponse> = serde_json::from_slice(&body).unwrap();
        assert!(!results.is_empty());
        assert!(results.iter().any(|r| r.name.contains("calculator")));
    }

    // -- Test: search with no query returns all --

    #[tokio::test]
    async fn test_search_no_query_returns_all() {
        let state = test_state();
        let router = app(state);

        let resp = router
            .oneshot(
                Request::builder()
                    .uri("/api/v1/marketplace/search")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let results: Vec<MarketplaceEntryResponse> = serde_json::from_slice(&body).unwrap();
        assert!(results.len() > 1);
    }

    // -- Test: category listing --

    #[tokio::test]
    async fn test_category_listing() {
        let state = test_state();
        let router = app(state);

        let resp = router
            .oneshot(
                Request::builder()
                    .uri("/api/v1/marketplace/categories")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let categories: Vec<String> = serde_json::from_slice(&body).unwrap();
        assert!(!categories.is_empty());
    }

    // -- Test: featured listing --

    #[tokio::test]
    async fn test_featured_listing() {
        let state = test_state();
        let router = app(state);

        let resp = router
            .oneshot(
                Request::builder()
                    .uri("/api/v1/marketplace/featured")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let featured: Vec<MarketplaceEntryResponse> = serde_json::from_slice(&body).unwrap();
        // All returned entries should be featured
        for entry in &featured {
            assert!(entry.featured);
        }
    }

    // -- Test: skill details found --

    #[tokio::test]
    async fn test_skill_details_found() {
        let state = test_state();
        let router = app(state);

        let resp = router
            .oneshot(
                Request::builder()
                    .uri("/api/v1/marketplace/skills/calculator")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let detail: MarketplaceEntryResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(detail.name, "calculator");
    }

    // -- Test: skill details not found --

    #[tokio::test]
    async fn test_skill_details_not_found() {
        let state = test_state();
        let router = app(state);

        let resp = router
            .oneshot(
                Request::builder()
                    .uri("/api/v1/marketplace/skills/nonexistent_skill_xyz")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    // -- Test: popular listing --

    #[tokio::test]
    async fn test_popular_listing() {
        let state = test_state();
        let router = app(state);

        let resp = router
            .oneshot(
                Request::builder()
                    .uri("/api/v1/marketplace/popular?limit=5")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let popular: Vec<MarketplaceEntryResponse> = serde_json::from_slice(&body).unwrap();
        assert!(popular.len() <= 5);
        // Verify sorted by downloads descending
        for w in popular.windows(2) {
            assert!(w[0].downloads >= w[1].downloads);
        }
    }

    // -- Test: recent listing --

    #[tokio::test]
    async fn test_recent_listing() {
        let state = test_state();
        let router = app(state);

        let resp = router
            .oneshot(
                Request::builder()
                    .uri("/api/v1/marketplace/recent?limit=5")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let recent: Vec<MarketplaceEntryResponse> = serde_json::from_slice(&body).unwrap();
        assert!(recent.len() <= 5);
    }

    // -- Test: installed listing (initially empty) --

    #[tokio::test]
    async fn test_installed_listing_empty() {
        let state = test_state();
        let router = app(state);

        let resp = router
            .oneshot(
                Request::builder()
                    .uri("/api/v1/marketplace/installed")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let installed: Vec<InstalledSkillResponse> = serde_json::from_slice(&body).unwrap();
        assert!(installed.is_empty());
    }

    // -- Test: stats endpoint --

    #[tokio::test]
    async fn test_stats_endpoint() {
        let state = test_state();
        let router = app(state);

        let resp = router
            .oneshot(
                Request::builder()
                    .uri("/api/v1/marketplace/stats")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let stats: CatalogStats = serde_json::from_slice(&body).unwrap();
        assert!(stats.total_skills > 0);
        assert!(stats.total_categories > 0);
        assert_eq!(stats.total_installed, 0);
    }

    // -- Test: uninstall nonexistent returns 404 --

    #[tokio::test]
    async fn test_uninstall_nonexistent() {
        let state = test_state();
        let router = app(state);

        let resp = router
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/api/v1/marketplace/install/nonexistent_skill")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    // -- Test: install nonexistent skill returns 404 --

    #[tokio::test]
    async fn test_install_nonexistent_skill() {
        let state = test_state();
        let router = app(state);

        let resp = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/marketplace/install/nonexistent_skill_xyz")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    // -- Test: search with category filter --

    #[tokio::test]
    async fn test_search_with_category_filter() {
        let state = test_state();
        let router = app(state);

        let resp = router
            .oneshot(
                Request::builder()
                    .uri("/api/v1/marketplace/search?category=data")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let results: Vec<MarketplaceEntryResponse> = serde_json::from_slice(&body).unwrap();
        // All returned results should have the "data" category
        for r in &results {
            assert!(
                r.categories.iter().any(|c| c.to_lowercase() == "data"),
                "Entry '{}' does not have 'data' category: {:?}",
                r.name,
                r.categories
            );
        }
    }
}
