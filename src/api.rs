use std::{net::SocketAddr, sync::Arc, time::Instant};

use axum::middleware::from_fn_with_state;
use axum::{
    Extension, Router,
    body::Body,
    error_handling::HandleErrorLayer,
    extract::{FromRequestParts, State},
    http::{Request, Uri, request::Parts},
    middleware::Next,
    response::{Html, IntoResponse, Response},
    routing::{any, get, get_service},
};
use axum_oidc::{
    AdditionalClaims, EmptyAdditionalClaims, OidcAuthLayer, OidcClient, OidcLoginLayer,
    OidcSession,
    error::MiddlewareError,
    handle_oidc_redirect,
    openidconnect::{ClientId, ClientSecret, IssuerUrl, core::CoreGenderClaim},
};
use minijinja::{Environment, context};
use tokio::sync::Mutex;
use tower::ServiceBuilder;
use tower::{Layer, Service};
use tower_http::services::ServeDir;
use tower_sessions::{
    Expiry, MemoryStore, SessionManagerLayer,
    cookie::{SameSite, time::Duration},
};

use crate::collector::IngressCollectionWrapper;

const CALLBACK_PATH: &str = "/callback";

struct Session(tower_sessions::Session);

impl<S: Send + Sync> FromRequestParts<S> for Session {
    type Rejection = <tower_sessions::Session as FromRequestParts<S>>::Rejection;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        tower_sessions::Session::from_request_parts(parts, state)
            .await
            .map(Self)
    }
}

impl<AC: AdditionalClaims> axum_oidc::Session<AC> for Session {
    type Error = tower_sessions::session::Error;

    async fn get(&self) -> Result<OidcSession<AC, CoreGenderClaim>, Self::Error> {
        Ok(self.0.get("axum-oidc").await?.unwrap_or_default())
    }

    async fn set(&mut self, value: OidcSession<AC, CoreGenderClaim>) -> Result<(), Self::Error> {
        self.0.insert("axum-oidc", value).await
    }
}

async fn index(
    Extension(collection): Extension<IngressCollectionWrapper>,
    Extension(template): Extension<String>,
) -> Html<String> {
    let mut template_env = Environment::new();
    template_env.add_template("main", &template).unwrap();
    let template = template_env.get_template("main").unwrap();
    let collection = collection.read().await;
    Html(
        template
            .render(context! { groups => collection.clone()})
            .unwrap(),
    )
}

async fn health() -> &'static str {
    "OK"
}

struct InnerOidcState {
    issuer: String,
    base_url: String,
    client_id: String,
    client_secret: Option<String>,
    renewal_interval: Option<Duration>,
    last_update: Instant,
    layer: Option<OidcAuthLayer<EmptyAdditionalClaims, Session>>,
}

impl InnerOidcState {
    async fn renew_layer(&mut self) {
        tracing::info!("Renewing oidc config");
        let redirect_url = format!("{}{CALLBACK_PATH}", self.base_url.trim_end_matches('/'));
        let mut builder = OidcClient::<EmptyAdditionalClaims>::builder()
            .with_default_http_client()
            .with_redirect_url(
                Uri::from_maybe_shared(redirect_url).expect("OIDC_BASE_URL is not valid"),
            )
            .with_client_id(ClientId::new(self.client_id.clone()));
        if let Some(client_secret) = &self.client_secret {
            builder = builder.with_client_secret(ClientSecret::new(client_secret.clone()));
        }
        let client = builder
            .discover(IssuerUrl::new(self.issuer.clone()).expect("OIDC_ISSUER is not valid"))
            .await
            .expect("Could not initialize OIDC client")
            .build();
        self.layer = Some(OidcAuthLayer::new(client));
        self.last_update = Instant::now();
    }
}

type OidcState = Arc<Mutex<InnerOidcState>>;

async fn init_oidc_state(issuer: String) -> OidcState {
    let base_url = std::env::var("OIDC_BASE_URL").expect("OIDC_BASE_URL not set");
    let client_id = std::env::var("OIDC_CLIENT_ID").expect("OIDC_CLIENT_ID not set");
    let client_secret = std::env::var("OIDC_CLIENT_SECRET").ok();
    let renewal_interval = std::env::var("OIDC_RENEWAL_INTERVAL_SECONDS")
        .ok()
        .and_then(|s| s.parse::<i64>().ok())
        .map(Duration::seconds);
    Arc::new(Mutex::new(InnerOidcState {
        issuer,
        base_url,
        client_id,
        client_secret,
        renewal_interval,
        last_update: Instant::now(),
        layer: None,
    }))
}

async fn oidc_layer(
    State(state): State<OidcState>,
    req: Request<Body>,
    next: Next,
) -> Result<Response, MiddlewareError> {
    let mut state = state.lock().await;
    if state.layer.is_none() {
        state.renew_layer().await;
    } else if let Some(renewal_interval) = state.renewal_interval
        && state.last_update.elapsed() > renewal_interval
    {
        state.renew_layer().await;
    }
    let mut service = state
        .layer
        .as_ref()
        .expect("Layer must have been initialized")
        .layer(next);
    service.call(req).await
}

pub async fn api(collection: IngressCollectionWrapper) {
    let template = if let Ok(template_path) = std::env::var("TEMPLATE_PATH") {
        tracing::info!("Using custom template at {template_path}");
        std::fs::read_to_string(template_path).unwrap()
    } else {
        std::fs::read_to_string("template.html").unwrap()
    };

    let app = Router::new()
        .route("/", get(index))
        .layer(Extension(collection))
        .layer(Extension(template));

    let app = if let Ok(issuer) = std::env::var("OIDC_ISSUER") {
        tracing::info!("Configuring OIDC with issuer {issuer}");

        let session_store = MemoryStore::default();
        let session_layer = SessionManagerLayer::new(session_store)
            .with_secure(false)
            .with_same_site(SameSite::Lax)
            .with_expiry(Expiry::OnInactivity(Duration::hours(24)));

        let oidc_login_service = ServiceBuilder::new()
            .layer(HandleErrorLayer::new(|e: MiddlewareError| async {
                e.into_response()
            }))
            .layer(OidcLoginLayer::<EmptyAdditionalClaims, Session>::new());

        app.layer(oidc_login_service)
            .route(
                CALLBACK_PATH,
                any(handle_oidc_redirect::<EmptyAdditionalClaims, Session>),
            )
            .layer(from_fn_with_state(
                init_oidc_state(issuer).await,
                oidc_layer,
            ))
            .layer(session_layer)
    } else {
        app
    };

    let app = app.route("/health", get(health));

    let app = if let Ok(static_dir) = std::env::var("STATIC_FOLDER") {
        tracing::info!("Adding static folder at {static_dir}");
        app.nest_service("/static", get_service(ServeDir::new(static_dir)))
    } else {
        app
    };

    let addr = SocketAddr::from(([0, 0, 0, 0], 8000));
    tracing::info!("Listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
