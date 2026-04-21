use axum::response::Redirect;
use axum::routing::get;
use axum::Router;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/", get(|| async { Redirect::temporary("/skills") }))
}
