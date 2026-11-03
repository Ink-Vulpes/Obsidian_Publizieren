use async_graphql::Schema;
use axum::{extract::Extension, response::IntoResponse, Json};
use std::sync::Arc;

pub mod mutation;
pub mod query;
pub mod types;

use mutation::MutationRoot;
use query::QueryRoot;

pub type AppSchema = Schema<QueryRoot, MutationRoot, async_graphql::EmptySubscription>;

pub fn build_schema(state: crate::AppState) -> AppSchema {
    Schema::build(QueryRoot, MutationRoot, async_graphql::EmptySubscription)
        .data(state)
        .finish()
}

pub async fn graphql_handler(
    Extension(schema): Extension<Arc<AppSchema>>,
    // Auth middleware injects Option<CurrentUser> for /graphql
    Extension(maybe_user): Extension<Option<crate::auth::CurrentUser>>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    let query = body["query"].as_str().unwrap_or("").to_string();
    let variables = async_graphql::Variables::from_json(
        body.get("variables").cloned().unwrap_or_default(),
    );
    let operation_name = body.get("operationName")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let mut req = async_graphql::Request::new(query).variables(variables);
    if let Some(op) = operation_name {
        req = req.operation_name(op);
    }
    if let Some(user) = maybe_user {
        req = req.data(user);
    }

    let resp = schema.execute(req).await;
    Json(resp)
}
