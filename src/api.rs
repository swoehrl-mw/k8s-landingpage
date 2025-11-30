use std::{net::SocketAddr};

use axum::{Extension, Router, error_handling::HandleErrorLayer, http::Uri, response::{Html, IntoResponse}, routing::{get, get_service}};
use axum_oidc::{
    error::MiddlewareError, EmptyAdditionalClaims, OidcAuthLayer, OidcLoginLayer,
};
use tower::ServiceBuilder;
use tower_http::services::ServeDir;
use minijinja::{context, Environment};
use tower_sessions::{Expiry, MemoryStore, SessionManagerLayer, cookie::{SameSite, time::Duration}};

use crate::collector::IngressCollectionWrapper;

async fn index(Extension(collection): Extension<IngressCollectionWrapper>, Extension(template): Extension<String>) -> Html<String> {
    let mut template_env = Environment::new();
    template_env.add_template("main", &template).unwrap();
    let template = template_env.get_template("main").unwrap();
    let collection = collection.read().await;
    Html(template.render(context! { clusters => collection.clone()}).unwrap())
}

async fn health() -> &'static str {
    "OK"
}

pub async fn api(
    collection: IngressCollectionWrapper,
) {

    let template = if let Ok(template_path) = std::env::var("TEMPLATE_PATH") {
        std::fs::read_to_string(template_path).unwrap()
    } else {
        std::fs::read_to_string("template.html").unwrap()
    };

    let app = Router::new()
        .route("/", get(index))
        .layer(Extension(collection))
        .layer(Extension(template))
    ;

    let app = if let Some(issuer) = std::env::var("OIDC_ISSUER").ok() {
        println!("Configuring OIDC with issuer {issuer}");
        let base_url = std::env::var("OIDC_BASE_URL").expect("OIDC_BASE_URL not set");
        let client_id = std::env::var("OIDC_CLIENT_ID").expect("OIDC_CLIENT_ID not set");
        let client_secret = std::env::var("OIDC_CLIENT_SECRET").ok();

        let session_store = MemoryStore::default();
        let session_layer = SessionManagerLayer::new(session_store)
            .with_secure(false)
            .with_same_site(SameSite::Lax)
            .with_expiry(Expiry::OnInactivity(Duration::seconds(120)));

        let oidc_login_service = ServiceBuilder::new()
            .layer(HandleErrorLayer::new(|e: MiddlewareError| async {
                e.into_response()
            }))
            .layer(OidcLoginLayer::<EmptyAdditionalClaims>::new());

        let oidc_auth_service = ServiceBuilder::new()
            .layer(HandleErrorLayer::new(|e: MiddlewareError| async {
                e.into_response()
            }))
            .layer(
                OidcAuthLayer::<EmptyAdditionalClaims>::discover_client(
                    Uri::from_maybe_shared(base_url).expect("OIDC_BASE_URL is not valid"),
                    issuer,
                    client_id,
                    client_secret,
                    vec![],
                )
                .await
                .unwrap(),
            );
        app
            .layer(oidc_login_service)
            .layer(oidc_auth_service)
            .layer(session_layer)
    } else {
        app
    };

    let app = app.route("/health", get(health));

    let app = if let Ok(static_dir) = std::env::var("STATIC_FOLDER") {
        println!("Adding static folder");
        app.nest_service(
            "/static",
            get_service(ServeDir::new(static_dir)),
        )
    } else {
        app
    };
      
    let addr = SocketAddr::from(([0, 0, 0, 0], 8000));
    println!("Listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .unwrap();
    axum::serve(listener, app).await.unwrap();
}
